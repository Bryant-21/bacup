//! Fixup: normalize copied FO76 package templates that shadow FO4 templates.
//!
//! PACK records are deliberately not vanilla-remapped by EditorID because many
//! package instances need stable output FormIDs. When a FO76 vanilla template
//! collides with a FO4 template, the translator keeps the copied record and
//! renames it with a `fo76` suffix. Some of those copied templates expose
//! FO76-only public inputs, which the FO4 Creation Kit reports as obsolete
//! procedure parameters. This pass keeps the copied FormID/EditorID but gives
//! the template and its instances the matching FO4 template ABI.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

pub struct NormalizeFo76PackTemplatesFixup;

impl Fixup for NormalizeFo76PackTemplatesFixup {
    fn name(&self) -> &'static str {
        "normalize_fo76_pack_templates"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        let target_is_fo4 = session
            .target_slot()
            .parsed
            .game
            .as_deref()
            .is_some_and(|game| game.eq_ignore_ascii_case("fo4"));
        let source_is_fo76 = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            .is_some_and(|game| game.eq_ignore_ascii_case("fo76"));
        target_is_fo4
            && source_is_fo76
            && config.target_schema.is_some()
            && !config.target_master_handle_ids.is_empty()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let pack_sig =
            SigCode::from_str("PACK").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_masters = session.target_masters().to_vec();

        let mut report = FixupReport::empty();
        let canonical_templates = collect_canonical_pack_templates(
            session,
            mapper.interner,
            target_schema,
            &config.target_master_handle_ids,
            pack_sig,
            &mut report,
        )?;
        if canonical_templates.is_empty() {
            return Ok(report);
        }

        let pack_fks = session
            .form_keys_of_sig(pack_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if pack_fks.is_empty() {
            return Ok(report);
        }

        let mut changed_records = Vec::new();
        let mut abi_by_encoded_template: FxHashMap<u32, PackTemplateAbi> = FxHashMap::default();
        // Renamed FO76-copy template (07xxxxxx) -> FO4 NATIVE template (Fallout4.esm).
        // Package templates are critical engine systems: FO4's procedure logic is
        // keyed to the native template form id, so instances must reference it, not
        // the EID-avoidance copy (whose tree null-derefs at runtime).
        let mut native_template_redirect: FxHashMap<u32, u32> = FxHashMap::default();

        for fk in &pack_fks {
            let mut record = match session.record_decoded(fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(e) => {
                    let warning = mapper
                        .interner
                        .intern(&format!("normalize_fo76_pack_templates_read_err:{e}"));
                    report.warnings.push(warning);
                    continue;
                }
            };
            let Some(base_key) = fo76_collision_base_key(&record, mapper.interner) else {
                continue;
            };
            let Some(canonical) = canonical_templates.get(&base_key) else {
                continue;
            };
            if !is_standalone_pack_template(&record) {
                continue;
            }
            let Some(encoded_template) =
                encode_target_form_id(record.form_key, mapper.interner, &target_masters)
            else {
                continue;
            };

            abi_by_encoded_template.insert(encoded_template, canonical.abi.clone());
            if let Some(native_template) =
                encode_target_form_id(canonical.record.form_key, mapper.interner, &target_masters)
            {
                native_template_redirect.insert(encoded_template, native_template);
            }
            if replace_pack_template_contents_preserving_editor_id(&mut record, &canonical.record) {
                changed_records.push(record);
            }
        }

        if abi_by_encoded_template.is_empty() {
            return Ok(report);
        }

        for fk in &pack_fks {
            let mut record = match session.record_decoded(fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(_) => continue,
            };
            let Some((_, pkcu)) = read_pack_pkcu(&record) else {
                continue;
            };
            if pkcu.package_template == 0 {
                continue;
            }
            let Some(abi) = abi_by_encoded_template.get(&pkcu.package_template) else {
                continue;
            };
            let mut changed = trim_pack_instance_to_template_abi(&mut record, abi);
            // Repoint the instance at FO4's native template so the engine runs its
            // hardcoded template procedure logic instead of the copy's inert tree.
            if let Some(&native_template) = native_template_redirect.get(&pkcu.package_template) {
                changed |= rewrite_pack_instance_template(&mut record, native_template);
            }
            if changed {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "normalize_fo76_pack_templates replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

#[derive(Clone)]
pub(crate) struct CanonicalPackTemplate {
    pub(crate) record: Record,
    pub(crate) abi: PackTemplateAbi,
}

#[derive(Clone)]
pub(crate) struct PackTemplateAbi {
    data_input_count: u32,
    version: u32,
    public_unams: Vec<u32>,
    xnam: Option<FieldEntry>,
}

#[derive(Clone, Copy)]
pub(crate) struct PackPkcu {
    data_input_count: u32,
    pub(crate) package_template: u32,
    version: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PackInputLayout {
    InlineUnams,
    TrailingUnams,
}

#[derive(Clone)]
struct PackInputChunk {
    unam: Option<u32>,
    data_fields: Vec<FieldEntry>,
    unam_field: Option<FieldEntry>,
}

pub(crate) fn collect_canonical_pack_templates(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_schema: &crate::schema::AuthoringSchema,
    master_handles: &[u64],
    pack_sig: SigCode,
    report: &mut FixupReport,
) -> Result<FxHashMap<String, CanonicalPackTemplate>, FixupError> {
    let mut out = FxHashMap::default();
    for handle_id in master_handles {
        let fks = session
            .form_keys_of_sig_in_handle(*handle_id, pack_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in fks {
            let record =
                match session.record_decoded_in_handle(*handle_id, &fk, target_schema, interner) {
                    Ok(record) => record,
                    Err(e) => {
                        let warning = interner.intern(&format!(
                            "normalize_fo76_pack_templates_master_read_err:{e}"
                        ));
                        report.warnings.push(warning);
                        continue;
                    }
                };
            if !is_standalone_pack_template(&record) {
                continue;
            }
            let Some(eid) = record.eid.and_then(|sym| interner.resolve(sym)) else {
                continue;
            };
            let Some(abi) = pack_template_abi(&record) else {
                continue;
            };
            out.entry(normalized_editor_id_key(eid))
                .or_insert(CanonicalPackTemplate { record, abi });
        }
    }
    Ok(out)
}

pub(crate) fn fo76_collision_base_key(
    record: &Record,
    interner: &StringInterner,
) -> Option<String> {
    let eid = record.eid.and_then(|sym| interner.resolve(sym))?;
    base_editor_id_from_fo76_suffix(eid).map(normalized_editor_id_key)
}

fn normalized_editor_id_key(editor_id: &str) -> String {
    editor_id.to_ascii_lowercase()
}

fn base_editor_id_from_fo76_suffix(editor_id: &str) -> Option<&str> {
    let lower = editor_id.to_ascii_lowercase();
    let suffix_start = lower.rfind("fo76")?;
    if suffix_start == 0 {
        return None;
    }
    lower[suffix_start + 4..]
        .chars()
        .all(|ch| ch.is_ascii_digit())
        .then_some(&editor_id[..suffix_start])
}

pub(crate) fn is_standalone_pack_template(record: &Record) -> bool {
    if record.sig.0 != *b"PACK" {
        return false;
    }
    read_pack_pkcu(record).is_some_and(|(_, pkcu)| pkcu.package_template == 0)
}

fn pack_template_abi(record: &Record) -> Option<PackTemplateAbi> {
    let (pkcu_pos, pkcu) = read_pack_pkcu(record)?;
    if pkcu.package_template != 0 {
        return None;
    }
    Some(PackTemplateAbi {
        data_input_count: pkcu.data_input_count,
        version: pkcu.version,
        public_unams: pack_unam_values(record, pkcu_pos),
        xnam: record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "XNAM")
            .cloned(),
    })
}

pub(crate) fn replace_pack_template_contents_preserving_editor_id(
    record: &mut Record,
    canonical: &Record,
) -> bool {
    let original_form_key = record.form_key;
    let original_eid = record.eid;
    let original_flags = record.flags;
    let original_warnings = record.warnings.clone();

    let mut replacement = canonical.clone();
    replacement.form_key = original_form_key;
    replacement.eid = original_eid;
    replacement.flags = original_flags;
    replacement.warnings = original_warnings;
    if let Some(eid) = original_eid {
        set_edid_field(&mut replacement, eid);
    }

    let changed = record.eid != replacement.eid
        || record.flags != replacement.flags
        || record.fields != replacement.fields
        || record.form_key != replacement.form_key;
    if changed {
        *record = replacement;
    }
    changed
}

fn set_edid_field(record: &mut Record, eid: Sym) {
    let edid_sig = SubrecordSig(*b"EDID");
    if let Some(field) = record.fields.iter_mut().find(|field| field.sig == edid_sig) {
        field.value = FieldValue::String(eid);
        return;
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(eid),
        },
    );
}

pub(crate) fn trim_pack_instance_to_template_abi(
    record: &mut Record,
    abi: &PackTemplateAbi,
) -> bool {
    let Some((pkcu_pos, pkcu)) = read_pack_pkcu(record) else {
        return false;
    };
    if pkcu.package_template == 0 {
        return false;
    }

    let mut changed = retain_template_public_inputs(record, pkcu_pos, abi);
    let Some((pkcu_pos, _)) = read_pack_pkcu(record) else {
        return changed;
    };
    let actual_count = pack_data_input_count(record, pkcu_pos);
    changed |= write_pack_pkcu_count_and_version(record, pkcu_pos, actual_count, abi.version);
    changed |= copy_pack_xnam_from_abi(record, abi);
    changed
}

fn retain_template_public_inputs(
    record: &mut Record,
    pkcu_pos: usize,
    abi: &PackTemplateAbi,
) -> bool {
    let Some((layout, chunks)) = pack_input_chunks(record, pkcu_pos) else {
        return false;
    };
    if chunks.is_empty() {
        return false;
    }

    let allowed_unams: FxHashSet<u32> = abi.public_unams.iter().copied().collect();
    let mut kept = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.into_iter().enumerate() {
        let keep = if !allowed_unams.is_empty() {
            chunk.unam.is_some_and(|unam| allowed_unams.contains(&unam))
        } else {
            index < abi.data_input_count as usize
        };
        if keep {
            kept.push(chunk);
        }
    }
    if kept.is_empty() || kept.len() == pack_data_input_count(record, pkcu_pos) as usize {
        return false;
    }

    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    let mut fields = SmallVec::<[FieldEntry; 8]>::with_capacity(record.fields.len());
    fields.extend(record.fields[..=pkcu_pos].iter().cloned());
    match layout {
        PackInputLayout::TrailingUnams => {
            for chunk in &kept {
                fields.extend(chunk.data_fields.iter().cloned());
            }
            for chunk in kept {
                if let Some(unam_field) = chunk.unam_field {
                    fields.push(unam_field);
                }
            }
        }
        PackInputLayout::InlineUnams => {
            for chunk in kept {
                fields.extend(chunk.data_fields.into_iter());
                if let Some(unam_field) = chunk.unam_field {
                    fields.push(unam_field);
                }
            }
        }
    }
    fields.extend(record.fields[data_end_pos..].iter().cloned());
    record.fields = fields;
    true
}

pub(crate) fn read_pack_pkcu(record: &Record) -> Option<(usize, PackPkcu)> {
    if record.sig.0 != *b"PACK" {
        return None;
    }
    let pkcu_sig = SubrecordSig(*b"PKCU");
    let (index, entry) = record
        .fields
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.sig == pkcu_sig)?;
    let FieldValue::Bytes(bytes) = &entry.value else {
        return None;
    };
    if bytes.len() < 12 {
        return None;
    }
    Some((
        index,
        PackPkcu {
            data_input_count: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            package_template: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
            version: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
        },
    ))
}

fn write_pack_pkcu_count_and_version(
    record: &mut Record,
    pkcu_pos: usize,
    data_input_count: u32,
    version: u32,
) -> bool {
    let Some(entry) = record.fields.get_mut(pkcu_pos) else {
        return false;
    };
    let FieldValue::Bytes(bytes) = &mut entry.value else {
        return false;
    };
    if bytes.len() < 12 {
        return false;
    }
    let mut changed = false;
    if bytes[0..4] != data_input_count.to_le_bytes() {
        bytes[0..4].copy_from_slice(&data_input_count.to_le_bytes());
        changed = true;
    }
    if bytes[8..12] != version.to_le_bytes() {
        bytes[8..12].copy_from_slice(&version.to_le_bytes());
        changed = true;
    }
    changed
}

/// Overwrite a package instance's PKCU `package_template` slot (offset 4) with the
/// FO4 native template's encoded form id. Returns true if it changed.
fn rewrite_pack_instance_template(record: &mut Record, native_template: u32) -> bool {
    let Some((pkcu_pos, _)) = read_pack_pkcu(record) else {
        return false;
    };
    let Some(entry) = record.fields.get_mut(pkcu_pos) else {
        return false;
    };
    let FieldValue::Bytes(bytes) = &mut entry.value else {
        return false;
    };
    if bytes.len() < 12 || bytes[4..8] == native_template.to_le_bytes() {
        return false;
    }
    bytes[4..8].copy_from_slice(&native_template.to_le_bytes());
    true
}

fn copy_pack_xnam_from_abi(record: &mut Record, abi: &PackTemplateAbi) -> bool {
    let Some(template_xnam) = abi.xnam.as_ref() else {
        return false;
    };
    let Some(record_xnam) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "XNAM")
    else {
        return false;
    };
    if record_xnam.value == template_xnam.value {
        return false;
    }
    record_xnam.value = template_xnam.value.clone();
    true
}

fn pack_input_chunks(
    record: &Record,
    pkcu_pos: usize,
) -> Option<(PackInputLayout, Vec<PackInputChunk>)> {
    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    if let Some(layout) = pack_trailing_unam_layout(record, pkcu_pos, data_end_pos) {
        let mut chunks = Vec::with_capacity(layout.anam_indices.len());
        for (input_index, data_start) in layout.anam_indices.iter().copied().enumerate() {
            let data_end = layout
                .anam_indices
                .get(input_index + 1)
                .copied()
                .unwrap_or(layout.first_unam_pos);
            let unam_index = layout.unam_indices[input_index];
            let unam_field = record.fields[unam_index].clone();
            chunks.push(PackInputChunk {
                unam: pack_unam_value(&unam_field),
                data_fields: record.fields[data_start..data_end].to_vec(),
                unam_field: Some(unam_field),
            });
        }
        return Some((PackInputLayout::TrailingUnams, chunks));
    }

    let mut chunks = Vec::new();
    let mut chunk_start = None;
    for index in pkcu_pos + 1..data_end_pos {
        let entry = &record.fields[index];
        if chunk_start.is_none() && entry.sig.as_str() == "ANAM" {
            chunk_start = Some(index);
        }
        if entry.sig.as_str() != "UNAM" {
            continue;
        }
        let Some(start) = chunk_start.take() else {
            continue;
        };
        let unam_field = entry.clone();
        chunks.push(PackInputChunk {
            unam: pack_unam_value(&unam_field),
            data_fields: record.fields[start..index].to_vec(),
            unam_field: Some(unam_field),
        });
    }
    (!chunks.is_empty()).then_some((PackInputLayout::InlineUnams, chunks))
}

struct PackTrailingUnamLayout {
    anam_indices: Vec<usize>,
    unam_indices: Vec<usize>,
    first_unam_pos: usize,
}

fn pack_trailing_unam_layout(
    record: &Record,
    pkcu_pos: usize,
    data_end_pos: usize,
) -> Option<PackTrailingUnamLayout> {
    let mut anam_indices = Vec::new();
    let mut unam_indices = Vec::new();
    for index in pkcu_pos + 1..data_end_pos {
        match record.fields[index].sig.as_str() {
            "ANAM" => anam_indices.push(index),
            "UNAM" => unam_indices.push(index),
            _ => {}
        }
    }
    if anam_indices.is_empty() || anam_indices.len() != unam_indices.len() {
        return None;
    }
    let first_unam_pos = *unam_indices.first()?;
    if first_unam_pos <= *anam_indices.last()? {
        return None;
    }
    if !record.fields[first_unam_pos..data_end_pos]
        .iter()
        .all(|entry| entry.sig.as_str() == "UNAM")
    {
        return None;
    }
    Some(PackTrailingUnamLayout {
        anam_indices,
        unam_indices,
        first_unam_pos,
    })
}

fn pack_unam_values(record: &Record, pkcu_pos: usize) -> Vec<u32> {
    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    record.fields[pkcu_pos + 1..data_end_pos]
        .iter()
        .filter(|entry| entry.sig.as_str() == "UNAM")
        .filter_map(pack_unam_value)
        .collect()
}

fn pack_unam_value(entry: &FieldEntry) -> Option<u32> {
    if entry.sig.as_str() != "UNAM" {
        return None;
    }
    match &entry.value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().ok()?))
        }
        FieldValue::Bytes(bytes) => bytes.first().copied().map(u32::from),
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn pack_data_input_count(record: &Record, pkcu_pos: usize) -> u32 {
    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    record.fields[pkcu_pos + 1..data_end_pos]
        .iter()
        .filter(|entry| entry.sig.as_str() == "ANAM")
        .count() as u32
}

fn pack_data_end_pos(record: &Record, pkcu_pos: usize) -> usize {
    record
        .fields
        .iter()
        .enumerate()
        .skip(pkcu_pos + 1)
        .find_map(|(index, entry)| (entry.sig.as_str() == "XNAM").then_some(index))
        .unwrap_or(record.fields.len())
}

pub(crate) fn encode_target_form_id(
    target: FormKey,
    interner: &StringInterner,
    target_masters: &[String],
) -> Option<u32> {
    if target.local == 0 {
        return Some(0);
    }
    let plugin_name = interner.resolve(target.plugin)?;
    let load_index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin_name))
        .unwrap_or(target_masters.len());
    if load_index > u8::MAX as usize || target.local > 0x00FF_FFFF {
        return None;
    }
    Some(((load_index as u32) << 24) | target.local)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interner() -> StringInterner {
        StringInterner::new()
    }

    fn pack_fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey::parse(&format!("{local:06X}@SeventySix.esm"), interner).unwrap()
    }

    fn pack_record(interner: &StringInterner, local: u32, eid: &str) -> Record {
        let mut record = Record::new(SigCode::from_str("PACK").unwrap(), pack_fk(interner, local));
        let eid_sym = interner.intern(eid);
        record.eid = Some(eid_sym);
        record
            .fields
            .push(field("EDID", FieldValue::String(eid_sym)));
        record
    }

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn bytes_field(sig: &str, bytes: Vec<u8>) -> FieldEntry {
        field(sig, FieldValue::Bytes(SmallVec::from_vec(bytes)))
    }

    fn pkcu(data_input_count: u32, package_template: u32, version: u32) -> FieldEntry {
        let mut raw = Vec::new();
        raw.extend_from_slice(&data_input_count.to_le_bytes());
        raw.extend_from_slice(&package_template.to_le_bytes());
        raw.extend_from_slice(&version.to_le_bytes());
        bytes_field("PKCU", raw)
    }

    fn anam(name: &str, interner: &StringInterner) -> FieldEntry {
        field("ANAM", FieldValue::String(interner.intern(name)))
    }

    fn unam(value: u32) -> FieldEntry {
        field("UNAM", FieldValue::Uint(value as u64))
    }

    #[test]
    fn strips_fo76_collision_suffix() {
        assert_eq!(
            base_editor_id_from_fo76_suffix("Travelfo76"),
            Some("Travel")
        );
        assert_eq!(
            base_editor_id_from_fo76_suffix("TravelFO761"),
            Some("Travel")
        );
        assert_eq!(base_editor_id_from_fo76_suffix("fo76"), None);
        assert_eq!(base_editor_id_from_fo76_suffix("Travel"), None);
    }

    #[test]
    fn canonical_template_replacement_preserves_identity() {
        let interner = interner();
        let mut copied = pack_record(&interner, 0x002CB0, "Travelfo76");
        copied.fields.push(pkcu(5, 0, 2));
        copied.fields.push(anam("Location", &interner));
        copied.fields.push(anam("Bool", &interner));
        copied.fields.push(anam("Bool", &interner));
        copied.fields.push(anam("Bool", &interner));
        copied.fields.push(anam("Bool", &interner));
        copied.fields.push(unam(1));
        copied.fields.push(unam(3));
        copied.fields.push(unam(5));
        copied.fields.push(unam(7));
        copied.fields.push(unam(9));
        copied.fields.push(bytes_field("XNAM", vec![10]));

        let mut canonical = pack_record(&interner, 0x002CB0, "Travel");
        canonical.fields.push(pkcu(4, 0, 2));
        canonical.fields.push(anam("Location", &interner));
        canonical.fields.push(anam("Bool", &interner));
        canonical.fields.push(anam("Bool", &interner));
        canonical.fields.push(anam("Bool", &interner));
        canonical.fields.push(unam(1));
        canonical.fields.push(unam(3));
        canonical.fields.push(unam(5));
        canonical.fields.push(unam(7));
        canonical.fields.push(bytes_field("XNAM", vec![8]));

        assert!(replace_pack_template_contents_preserving_editor_id(
            &mut copied,
            &canonical
        ));

        assert_eq!(copied.form_key.local, 0x002CB0);
        assert_eq!(
            copied.eid.and_then(|sym| interner.resolve(sym)),
            Some("Travelfo76")
        );
        assert_eq!(
            copied
                .fields
                .iter()
                .filter(|entry| entry.sig.as_str() == "ANAM")
                .count(),
            4
        );
        assert_eq!(
            pack_unam_values(&copied, read_pack_pkcu(&copied).unwrap().0),
            vec![1, 3, 5, 7]
        );
    }

    #[test]
    fn repoints_instance_template_to_fo4_native() {
        let interner = interner();
        let mut instance = pack_record(&interner, 0x8A4CF5, "W05_Raider_SleepInstance");
        // PKCU.package_template currently points at the renamed FO76 copy (07002F75).
        instance.fields.push(pkcu(17, 0x07002F75, 2));
        instance.fields.push(anam("Location", &interner));
        instance.fields.push(bytes_field("PLDT", vec![0; 16]));

        // FO4 native Sleep template is Fallout4.esm:002F75 -> encoded 0x00002F75.
        let changed = rewrite_pack_instance_template(&mut instance, 0x0000_2F75);

        assert!(changed);
        let (_, pkcu) = read_pack_pkcu(&instance).unwrap();
        assert_eq!(pkcu.package_template, 0x0000_2F75);
        // Idempotent: a second pass makes no change.
        assert!(!rewrite_pack_instance_template(&mut instance, 0x0000_2F75));
    }

    #[test]
    fn trims_instance_extra_trailing_template_input() {
        let interner = interner();
        let mut instance = pack_record(&interner, 0x8A4CF5, "TravelInstance");
        instance.fields.push(pkcu(5, 0x07002CB0, 2));
        instance.fields.push(anam("Location", &interner));
        instance.fields.push(bytes_field("PLDT", vec![0; 16]));
        for _ in 0..4 {
            instance.fields.push(anam("Bool", &interner));
            instance.fields.push(bytes_field("CNAM", vec![0]));
        }
        for value in [1, 3, 5, 7, 9] {
            instance.fields.push(unam(value));
        }
        instance.fields.push(bytes_field("XNAM", vec![10]));
        instance.fields.push(bytes_field("POBA", Vec::new()));

        let abi = PackTemplateAbi {
            data_input_count: 4,
            version: 2,
            public_unams: vec![1, 3, 5, 7],
            xnam: Some(bytes_field("XNAM", vec![8])),
        };

        assert!(trim_pack_instance_to_template_abi(&mut instance, &abi));

        let (pkcu_pos, pkcu) = read_pack_pkcu(&instance).unwrap();
        assert_eq!(pkcu.data_input_count, 4);
        assert_eq!(pkcu.package_template, 0x07002CB0);
        assert_eq!(pack_unam_values(&instance, pkcu_pos), vec![1, 3, 5, 7]);
        assert_eq!(pack_data_input_count(&instance, pkcu_pos), 4);
        let xnam = instance
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "XNAM")
            .expect("XNAM");
        assert_eq!(xnam.value, FieldValue::Bytes(SmallVec::from_vec(vec![8])));
    }
}
