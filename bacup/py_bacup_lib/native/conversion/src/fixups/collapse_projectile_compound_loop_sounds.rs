//! Fixup: collapse Compound projectile sounds to a single looping descriptor.
//!
//! FO76 `PROJ.DNAM.sound` points at a Compound `SNDR` whose `DNAM` children each
//! loop independently (`FXProjectileMissileByLP` stacks FO4's
//! `FXProjectileMissileBy` with three FO76-only `LAYERA/B/C` loops). All 139 FO4
//! `PROJ` sounds are Standard descriptors and FO4 compounds are one-shots. In FO4
//! the extra loops outlive the projectile and stack with every shot, worst on
//! auto-firing turrets.
//!
//! A compound with a looping layer is repointed at the layer that lives in
//! `Fallout4.esm`, else at the first converted plain looping layer. One-shot
//! compounds, and compounds with neither candidate, are left alone.
//!
//! `source_read::decode_subrecord` emits `struct:` subrecords as raw bytes, so
//! `PROJ.DNAM.sound` and `SNDR.LNAM.looping` are accessed at byte offsets from
//! the target schema at `FO4_TARGET_FORM_VERSION` (the layouts are
//! version-gated). The named-field arms are for a future struct decode.

use crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use rustc_hash::FxHashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Base-game master every FO4 plugin loads at master index 0.
const FO4_BASE_MASTER: &str = "Fallout4.esm";

/// `SNDR.LNAM.looping` value meaning "loop until explicitly stopped".
const LOOPING_LOOP: u64 = 8;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct CollapseProjectileCompoundLoopSoundsFixup;

impl Fixup for CollapseProjectileCompoundLoopSoundsFixup {
    fn name(&self) -> &'static str {
        "collapse_projectile_compound_loop_sounds"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let proj_sig =
            SigCode::from_str("PROJ").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let sndr_sig =
            SigCode::from_str("SNDR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let dnam_sig =
            SubrecordSig::from_str("DNAM").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let lnam_sig =
            SubrecordSig::from_str("LNAM").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let mut report = FixupReport::empty();

        let Some(offsets) = StructOffsets::resolve(target_schema) else {
            let w = mapper
                .interner
                .intern("proj_sound_schema_offsets_unresolved");
            report.warnings.push(w);
            return Ok(report);
        };

        // ── 1. No projectiles → nothing to repoint; skip the SNDR scan ────
        let proj_fks = session
            .form_keys_of_sig(proj_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if proj_fks.is_empty() {
            return Ok(report);
        }

        // The master indices packed into `PROJ.DNAM` are resolved against the
        // output plugin's own load order.
        let load_order = {
            let target_id = session.target_id();
            let (masters, plugin_name) = session
                .handle_load_order(target_id)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            LoadOrder {
                masters: masters.to_vec(),
                plugin_name: plugin_name.to_string(),
            }
        };

        // ── 2. Summarize every converted SNDR (layers + looping) ──────────
        let sndr_fks = session
            .form_keys_of_sig(sndr_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut sndrs: FxHashMap<(u32, Sym), SndrLayers> = FxHashMap::default();
        for fk in sndr_fks {
            let Ok(record) = session.record_decoded(&fk, target_schema, mapper.interner) else {
                continue;
            };
            sndrs.insert(
                (fk.local, fk.plugin),
                summarize_sndr(&record, dnam_sig, lnam_sig, &offsets, mapper.interner),
            );
        }

        if sndrs.is_empty() {
            return Ok(report);
        }

        // ── 3. Repoint PROJ sounds that resolve to a looping compound ─────
        // Collected and written in one batch: `replace_record` invalidates the
        // target indexes the `record_decoded` below still needs.
        let mut replacements = Vec::new();
        for fk in proj_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("proj_sound_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let Some(replacement) = plan_replacement(
                &record,
                dnam_sig,
                &sndrs,
                &load_order,
                &offsets,
                mapper.interner,
            ) else {
                continue;
            };

            if set_projectile_sound(
                &mut record,
                dnam_sig,
                &load_order,
                &offsets,
                mapper.interner,
                replacement,
            ) {
                replacements.push(record);
            }
        }

        report.records_changed = session
            .replace_records_contents(replacements, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// SNDR shape
// ---------------------------------------------------------------------------

/// What a fixup needs to know about one converted `SNDR`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SndrLayers {
    /// `DNAM` children. Non-empty only for Compound descriptors.
    pub children: Vec<FormKey>,
    /// `LNAM.looping` is `Loop`.
    pub looping: bool,
}

/// Collect a SNDR's compound layers and whether it loops.
pub fn summarize_sndr(
    record: &Record,
    dnam_sig: SubrecordSig,
    lnam_sig: SubrecordSig,
    offsets: &StructOffsets,
    interner: &StringInterner,
) -> SndrLayers {
    let mut layers = SndrLayers::default();

    for entry in record.fields.iter() {
        if entry.sig == dnam_sig {
            if let Some(fk) = first_formkey(&entry.value) {
                if fk.local != 0 {
                    layers.children.push(fk);
                }
            }
        } else if entry.sig == lnam_sig {
            layers.looping |= lnam_is_looping(&entry.value, offsets, interner);
        }
    }

    layers
}

/// True when a `SNDR.LNAM` says `Loop`.
///
/// `LNAM` is a `struct:` codec, so in production it arrives as raw `Bytes` and
/// `looping` is the second one. The named-field arm covers a decoded struct.
pub fn lnam_is_looping(
    value: &FieldValue,
    offsets: &StructOffsets,
    interner: &StringInterner,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => bytes
            .get(offsets.sndr_looping)
            .is_some_and(|b| u64::from(*b) == LOOPING_LOOP),
        FieldValue::Struct(fields) => fields.iter().any(|(name, value)| {
            field_name_is(*name, "looping", interner) && looping_value_is_loop(value, interner)
        }),
        _ => false,
    }
}

/// True when a decoded `looping` field says `Loop`.
///
/// `looping` is `enum_ref`'d, so the decoder hands it back as a token string —
/// or as a token list / raw integer depending on which codec path ran. Accept
/// every shape rather than assume one.
fn looping_value_is_loop(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::String(sym) => interner
            .resolve(*sym)
            .is_some_and(|token| token.eq_ignore_ascii_case("loop")),
        FieldValue::List(tokens) => tokens
            .iter()
            .any(|token| looping_value_is_loop(token, interner)),
        FieldValue::Uint(bits) => *bits == LOOPING_LOOP,
        FieldValue::Int(bits) => *bits >= 0 && (*bits as u64) == LOOPING_LOOP,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// PROJ planning + mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Decide which single layer a projectile's compound sound should collapse to,
/// or `None` when this projectile should be left alone.
pub fn plan_replacement(
    record: &Record,
    dnam_sig: SubrecordSig,
    sndrs: &FxHashMap<(u32, Sym), SndrLayers>,
    load_order: &LoadOrder,
    offsets: &StructOffsets,
    interner: &StringInterner,
) -> Option<FormKey> {
    let sound = projectile_sound(record, dnam_sig, load_order, offsets, interner)?;
    let compound = sndrs.get(&(sound.local, sound.plugin))?;

    // Standard descriptor (no layers) — already the shape FO4 authors.
    if compound.children.is_empty() {
        return None;
    }

    // A compound of one-shots cannot leak a voice; leave its layering intact.
    let has_looping_layer = compound.children.iter().any(|fk| {
        sndrs
            .get(&(fk.local, fk.plugin))
            .is_some_and(|layer| layer.looping)
    });
    if !has_looping_layer {
        return None;
    }

    // Prefer FO4's own descriptor — vanilla projectiles point straight at it.
    if let Some(fk) = compound
        .children
        .iter()
        .find(|fk| plugin_is(fk.plugin, FO4_BASE_MASTER, interner))
    {
        return Some(*fk);
    }

    // Otherwise the first converted layer that is a plain looping descriptor,
    // never another compound.
    compound.children.iter().copied().find(|fk| {
        sndrs
            .get(&(fk.local, fk.plugin))
            .is_some_and(|layer| layer.looping && layer.children.is_empty())
    })
}

/// Read a projectile's `DNAM` `sound` FormKey, if it has a non-null one.
fn projectile_sound(
    record: &Record,
    dnam_sig: SubrecordSig,
    load_order: &LoadOrder,
    offsets: &StructOffsets,
    interner: &StringInterner,
) -> Option<FormKey> {
    for entry in record.fields.iter() {
        if entry.sig != dnam_sig {
            continue;
        }
        match &entry.value {
            FieldValue::Bytes(bytes) => {
                let raw = read_u32(bytes, offsets.proj_sound)?;
                return load_order.form_key(raw, interner);
            }
            FieldValue::Struct(fields) => {
                for (name, value) in fields {
                    if !field_name_is(*name, "sound", interner) {
                        continue;
                    }
                    let FieldValue::FormKey(fk) = value else {
                        return None;
                    };
                    return (fk.local != 0).then_some(*fk);
                }
            }
            _ => continue,
        }
    }
    None
}

/// Point a projectile's `DNAM` `sound` at `replacement`.
///
/// Returns `true` when the record was mutated. Only the `sound` field is
/// touched; every other DNAM field keeps its converted value.
pub fn set_projectile_sound(
    record: &mut Record,
    dnam_sig: SubrecordSig,
    load_order: &LoadOrder,
    offsets: &StructOffsets,
    interner: &StringInterner,
    replacement: FormKey,
) -> bool {
    for entry in record.fields.iter_mut() {
        if entry.sig != dnam_sig {
            continue;
        }
        match entry.value {
            FieldValue::Bytes(ref mut bytes) => {
                let Some(raw) = load_order.raw_form_id(replacement, interner) else {
                    return false;
                };
                let Some(slot) = bytes.get_mut(offsets.proj_sound..offsets.proj_sound + 4) else {
                    return false;
                };
                if slot == raw.to_le_bytes() {
                    return false;
                }
                slot.copy_from_slice(&raw.to_le_bytes());
                return true;
            }
            FieldValue::Struct(ref mut fields) => {
                for (name, value) in fields.iter_mut() {
                    if !field_name_is(*name, "sound", interner) {
                        continue;
                    }
                    if *value == FieldValue::FormKey(replacement) {
                        return false;
                    }
                    *value = FieldValue::FormKey(replacement);
                    return true;
                }
            }
            _ => continue,
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Schema-resolved struct offsets
// ---------------------------------------------------------------------------

/// Where the two struct fields this fixup touches sit in their payloads.
///
/// Resolved from the target schema at [`FO4_TARGET_FORM_VERSION`] rather than
/// hardcoded: `struct:` layouts are form-version gated, so a version-less
/// lookup skews every field past the first gate.
pub struct StructOffsets {
    /// `PROJ.DNAM.sound`
    pub proj_sound: usize,
    /// `SNDR.LNAM.looping`
    pub sndr_looping: usize,
}

impl StructOffsets {
    pub fn resolve(schema: &AuthoringSchema) -> Option<Self> {
        Some(Self {
            proj_sound: struct_field_offset(schema, "PROJ", "DNAM", "sound", 4)?,
            sndr_looping: struct_field_offset(schema, "SNDR", "LNAM", "looping", 1)?,
        })
    }
}

fn struct_field_offset(
    schema: &AuthoringSchema,
    record_sig: &str,
    subrecord_sig: &str,
    field_id: &str,
    width: usize,
) -> Option<usize> {
    schema
        .struct_field_layout_versioned(record_sig, subrecord_sig, Some(FO4_TARGET_FORM_VERSION))
        .into_iter()
        .find(|field| field.field_id == field_id && field.width == width)
        .map(|field| field.offset)
}

// ---------------------------------------------------------------------------
// Raw FormID ↔ FormKey against the output plugin's load order
// ---------------------------------------------------------------------------

/// The output plugin's masters plus its own name — what the master index
/// packed into a byte-level FormID is resolved against.
pub struct LoadOrder {
    pub masters: Vec<String>,
    pub plugin_name: String,
}

impl LoadOrder {
    /// Resolve a raw FormID read out of struct bytes. An index past the master
    /// list is the plugin's own space, matching `source_read::resolve_form_id`.
    pub fn form_key(&self, raw: u32, interner: &StringInterner) -> Option<FormKey> {
        if raw == 0 {
            return None;
        }
        let index = ((raw >> 24) & 0xFF) as usize;
        let plugin = self
            .masters
            .get(index)
            .map_or(self.plugin_name.as_str(), String::as_str);
        Some(FormKey {
            local: raw & 0x00FF_FFFF,
            plugin: interner.intern(plugin),
        })
    }

    /// Pack a FormKey back into a raw FormID. `None` when the key's plugin is
    /// neither the output plugin nor one of its masters — writing it would
    /// point the projectile at whatever else sits at that master index.
    pub fn raw_form_id(&self, fk: FormKey, interner: &StringInterner) -> Option<u32> {
        let plugin = interner.resolve(fk.plugin)?;
        let index = if plugin.eq_ignore_ascii_case(&self.plugin_name) {
            self.masters.len()
        } else {
            self.masters
                .iter()
                .position(|master| master.eq_ignore_ascii_case(plugin))?
        };
        let index = u32::try_from(index).ok().filter(|index| *index <= 0xFF)?;
        Some((index << 24) | (fk.local & 0x00FF_FFFF))
    }
}

/// Little-endian `u32` at `offset`, or `None` when the payload is too short.
fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Struct field names arrive as the schema field id (`sound`) or its display
/// label (`Sound`) depending on the decode path; both compare equal here.
fn field_name_is(name: Sym, expected: &str, interner: &StringInterner) -> bool {
    interner
        .resolve(name)
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn plugin_is(plugin: Sym, expected: &str, interner: &StringInterner) -> bool {
    interner
        .resolve(plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(expected))
}

/// First `FormKey` leaf in a field value (depth-first), if any.
fn first_formkey(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::List(items) => items.iter().find_map(first_formkey),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, v)| first_formkey(v)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{FieldEntry, RecordFlags};

    /// `PROJ.DNAM` payload length for
    /// `struct:H,H,f,f,f,I,I,f,f,I,I,f,f,f,I,I,I,f,f,f,f,I,I,B,I`.
    const PROJ_DNAM_LEN: usize = 93;
    const DNAM_SPEED_OFFSET: usize = 8;
    const DNAM_RANGE_OFFSET: usize = 12;

    fn schema() -> std::sync::Arc<AuthoringSchema> {
        AuthoringSchema::for_game("fo4").expect("fo4 schema")
    }

    fn offsets() -> StructOffsets {
        StructOffsets::resolve(&schema()).expect("PROJ.DNAM.sound + SNDR.LNAM.looping")
    }

    fn dnam() -> SubrecordSig {
        SubrecordSig::from_str("DNAM").unwrap()
    }

    fn lnam() -> SubrecordSig {
        SubrecordSig::from_str("LNAM").unwrap()
    }

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    /// An output plugin named `Out.esp` mastered on `Fallout4.esm`, so
    /// `Fallout4.esm` refs pack at index 0 and own refs at index 1.
    fn load_order() -> LoadOrder {
        LoadOrder {
            masters: vec![FO4_BASE_MASTER.to_string()],
            plugin_name: "Out.esp".to_string(),
        }
    }

    /// A PROJ carrying the production `DNAM`: raw struct bytes with `sound`
    /// packed at its schema offset.
    fn make_proj(sound: Option<FormKey>, interner: &StringInterner) -> Record {
        let mut data = vec![0u8; PROJ_DNAM_LEN];
        data[DNAM_SPEED_OFFSET..DNAM_SPEED_OFFSET + 4].copy_from_slice(&2500.0f32.to_le_bytes());
        data[DNAM_RANGE_OFFSET..DNAM_RANGE_OFFSET + 4].copy_from_slice(&15000.0f32.to_le_bytes());
        if let Some(sound) = sound {
            let raw = load_order()
                .raw_form_id(sound, interner)
                .expect("test sound must live in the test load order");
            let at = offsets().proj_sound;
            data[at..at + 4].copy_from_slice(&raw.to_le_bytes());
        }
        make_proj_record(FieldValue::Bytes(data.into_iter().collect()), interner)
    }

    /// The same PROJ as a decoded struct, for the named-field arm.
    fn make_proj_struct(sound: Option<FormKey>, interner: &StringInterner) -> Record {
        let mut fields = vec![(interner.intern("speed"), FieldValue::Float(2500.0))];
        if let Some(sound) = sound {
            fields.push((interner.intern("sound"), FieldValue::FormKey(sound)));
        }
        fields.push((interner.intern("range"), FieldValue::Float(15000.0)));
        make_proj_record(FieldValue::Struct(fields), interner)
    }

    fn make_proj_record(dnam_value: FieldValue, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("PROJ").unwrap(),
            form_key: fk(0x8A_A1DC, "Out.esp", interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: dnam(),
                value: dnam_value,
            }],
            warnings: smallvec::SmallVec::new(),
        }
    }

    /// A SNDR: `children` DNAM layers plus the production `LNAM` byte row.
    fn make_sndr(children: &[FormKey], looping: bool, interner: &StringInterner) -> Record {
        let looping_byte = if looping { LOOPING_LOOP as u8 } else { 0 };
        make_sndr_record(
            children,
            FieldValue::Bytes(smallvec::smallvec![1, looping_byte, 0, 0]),
            interner,
        )
    }

    /// The same SNDR with a decoded `LNAM`, for the named-field arm.
    fn make_sndr_struct(children: &[FormKey], looping: bool, interner: &StringInterner) -> Record {
        make_sndr_record(
            children,
            FieldValue::Struct(vec![
                (interner.intern("unknown_u8_0"), FieldValue::Uint(1)),
                (
                    interner.intern("looping"),
                    FieldValue::String(interner.intern(if looping { "Loop" } else { "None" })),
                ),
            ]),
            interner,
        )
    }

    fn make_sndr_record(
        children: &[FormKey],
        lnam_value: FieldValue,
        interner: &StringInterner,
    ) -> Record {
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        fields.push(FieldEntry {
            sig: lnam(),
            value: lnam_value,
        });
        for child in children {
            fields.push(FieldEntry {
                sig: dnam(),
                value: FieldValue::FormKey(*child),
            });
        }
        Record {
            sig: SigCode::from_str("SNDR").unwrap(),
            form_key: fk(0x04_FEB1, "Out.esp", interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn summarize(record: &Record, interner: &StringInterner) -> SndrLayers {
        summarize_sndr(record, dnam(), lnam(), &offsets(), interner)
    }

    fn plan(
        proj: &Record,
        sndrs: &FxHashMap<(u32, Sym), SndrLayers>,
        interner: &StringInterner,
    ) -> Option<FormKey> {
        plan_replacement(proj, dnam(), sndrs, &load_order(), &offsets(), interner)
    }

    // ---- schema offsets -------------------------------------------------

    /// Pins the resolved offsets to values ground-truthed elsewhere: `36` is
    /// in the `PROJ.DNAM` FormID table `rewrite_raw_object_template_formids`
    /// remaps, and `1` is where the shipped `FXProjectileMissileByLAYERBLP`
    /// carries its `Loop` byte behind a `1`-valued `unknown_u8_0`.
    #[test]
    fn schema_offsets_match_shipped_records() {
        let offsets = offsets();
        assert_eq!(offsets.proj_sound, 36, "PROJ.DNAM.sound");
        assert_eq!(offsets.sndr_looping, 1, "SNDR.LNAM.looping");
    }

    // ---- summarize_sndr -------------------------------------------------

    #[test]
    fn standard_looping_descriptor_has_no_layers() {
        let interner = StringInterner::new();
        let record = make_sndr(&[], true, &interner);
        let layers = summarize(&record, &interner);
        assert!(
            layers.children.is_empty(),
            "Standard SNDR has no DNAM layers"
        );
        assert!(layers.looping, "LNAM byte row must read as looping");
    }

    #[test]
    fn compound_collects_every_layer() {
        let interner = StringInterner::new();
        let a = fk(0x0A_6BDC, "Fallout4.esm", &interner);
        let b = fk(0x04_FEB3, "Out.esp", &interner);
        let record = make_sndr(&[a, b], false, &interner);
        let layers = summarize(&record, &interner);
        assert_eq!(layers.children, vec![a, b]);
        assert!(!layers.looping);
    }

    /// `looping` is the *second* byte of `LNAM`, and the first byte is
    /// routinely 1 on real records.
    #[test]
    fn looping_reads_the_second_lnam_byte_not_the_first() {
        let interner = StringInterner::new();
        assert!(lnam_is_looping(
            &FieldValue::Bytes(smallvec::smallvec![1, 8, 0, 0]),
            &offsets(),
            &interner
        ));
        assert!(
            !lnam_is_looping(
                &FieldValue::Bytes(smallvec::smallvec![8, 0, 0, 0]),
                &offsets(),
                &interner
            ),
            "a Loop-valued first byte is unknown_u8_0, not looping"
        );
        assert!(
            !lnam_is_looping(
                &FieldValue::Bytes(smallvec::smallvec![1, 16, 0, 0]),
                &offsets(),
                &interner
            ),
            "EnvelopeFast is not a loop"
        );
        assert!(!lnam_is_looping(
            &FieldValue::Bytes(smallvec::smallvec![1]),
            &offsets(),
            &interner
        ));
    }

    #[test]
    fn looping_reads_from_decoded_struct_shapes() {
        let interner = StringInterner::new();
        assert!(summarize(&make_sndr_struct(&[], true, &interner), &interner).looping);
        assert!(!summarize(&make_sndr_struct(&[], false, &interner), &interner).looping);
        assert!(looping_value_is_loop(&FieldValue::Uint(8), &interner));
        assert!(looping_value_is_loop(&FieldValue::Int(8), &interner));
        assert!(looping_value_is_loop(
            &FieldValue::List(vec![FieldValue::String(interner.intern("Loop"))]),
            &interner
        ));
        assert!(!looping_value_is_loop(&FieldValue::Uint(0), &interner));
        assert!(!looping_value_is_loop(
            &FieldValue::String(interner.intern("EnvelopeFast")),
            &interner
        ));
    }

    // ---- LoadOrder ------------------------------------------------------

    #[test]
    fn load_order_round_trips_master_and_own_refs() {
        let interner = StringInterner::new();
        let order = load_order();
        let base = fk(0x0A_6BDC, "Fallout4.esm", &interner);
        let own = fk(0x04_FEB3, "Out.esp", &interner);

        assert_eq!(order.raw_form_id(base, &interner), Some(0x000A_6BDC));
        assert_eq!(order.raw_form_id(own, &interner), Some(0x0104_FEB3));
        assert_eq!(order.form_key(0x000A_6BDC, &interner), Some(base));
        assert_eq!(order.form_key(0x0104_FEB3, &interner), Some(own));
        assert_eq!(order.form_key(0, &interner), None, "NULL is not a ref");
        assert_eq!(
            order.raw_form_id(fk(0x01_0000, "Absent.esm", &interner), &interner),
            None,
            "a plugin outside the load order has no index to pack"
        );
    }

    // ---- plan_replacement -----------------------------------------------

    /// The production shape: `FXProjectileMissileByLP` wraps FO4's own
    /// `FXProjectileMissileBy` plus FO76 loop layers.
    fn missile_world(interner: &StringInterner) -> (FxHashMap<(u32, Sym), SndrLayers>, FormKey) {
        let base = fk(0x0A_6BDC, "Fallout4.esm", interner);
        let layer_b = fk(0x04_FEB3, "Out.esp", interner);
        let layer_c = fk(0x04_FEB4, "Out.esp", interner);
        let compound = fk(0x04_FEB1, "Out.esp", interner);

        let mut sndrs = FxHashMap::default();
        sndrs.insert(
            (compound.local, compound.plugin),
            summarize(
                &make_sndr(&[base, layer_b, layer_c], false, interner),
                interner,
            ),
        );
        for layer in [layer_b, layer_c] {
            sndrs.insert(
                (layer.local, layer.plugin),
                summarize(&make_sndr(&[], true, interner), interner),
            );
        }
        (sndrs, compound)
    }

    #[test]
    fn looping_compound_collapses_to_the_fo4_base_layer() {
        let interner = StringInterner::new();
        let (sndrs, compound) = missile_world(&interner);
        let proj = make_proj(Some(compound), &interner);

        assert_eq!(
            plan(&proj, &sndrs, &interner),
            Some(fk(0x0A_6BDC, "Fallout4.esm", &interner)),
            "must collapse to FO4's own FXProjectileMissileBy"
        );
    }

    #[test]
    fn decoded_struct_projectiles_plan_the_same_way() {
        let interner = StringInterner::new();
        let (sndrs, compound) = missile_world(&interner);
        let proj = make_proj_struct(Some(compound), &interner);

        assert_eq!(
            plan(&proj, &sndrs, &interner),
            Some(fk(0x0A_6BDC, "Fallout4.esm", &interner))
        );
    }

    #[test]
    fn falls_back_to_first_converted_looping_layer() {
        let interner = StringInterner::new();
        let layer_b = fk(0x04_FEB3, "Out.esp", &interner);
        let layer_c = fk(0x04_FEB4, "Out.esp", &interner);
        let compound = fk(0x04_FEB1, "Out.esp", &interner);

        let mut sndrs = FxHashMap::default();
        sndrs.insert(
            (compound.local, compound.plugin),
            summarize(&make_sndr(&[layer_b, layer_c], false, &interner), &interner),
        );
        // layer_b is a one-shot, layer_c loops.
        sndrs.insert(
            (layer_b.local, layer_b.plugin),
            summarize(&make_sndr(&[], false, &interner), &interner),
        );
        sndrs.insert(
            (layer_c.local, layer_c.plugin),
            summarize(&make_sndr(&[], true, &interner), &interner),
        );

        let proj = make_proj(Some(compound), &interner);
        assert_eq!(
            plan(&proj, &sndrs, &interner),
            Some(layer_c),
            "no FO4 layer present -> keep the looping converted layer"
        );
    }

    #[test]
    fn standard_sound_is_left_alone() {
        let interner = StringInterner::new();
        let standard = fk(0x04_FEB3, "Out.esp", &interner);
        let mut sndrs = FxHashMap::default();
        sndrs.insert(
            (standard.local, standard.plugin),
            summarize(&make_sndr(&[], true, &interner), &interner),
        );

        let proj = make_proj(Some(standard), &interner);
        assert_eq!(
            plan(&proj, &sndrs, &interner),
            None,
            "a Standard descriptor is already FO4's shape"
        );
    }

    #[test]
    fn one_shot_compound_keeps_its_layers() {
        let interner = StringInterner::new();
        let layer_a = fk(0x01_82A7, "Out.esp", &interner);
        let compound = fk(0x01_81D9, "Out.esp", &interner);

        let mut sndrs = FxHashMap::default();
        sndrs.insert(
            (compound.local, compound.plugin),
            summarize(&make_sndr(&[layer_a], false, &interner), &interner),
        );
        sndrs.insert(
            (layer_a.local, layer_a.plugin),
            summarize(&make_sndr(&[], false, &interner), &interner),
        );

        let proj = make_proj(Some(compound), &interner);
        assert_eq!(
            plan(&proj, &sndrs, &interner),
            None,
            "a compound of one-shots cannot leak a looping voice"
        );
    }

    #[test]
    fn unresolvable_and_missing_sounds_are_skipped() {
        let interner = StringInterner::new();
        let (sndrs, _) = missile_world(&interner);

        // Sound already points into a master we did not summarize.
        let external = make_proj(Some(fk(0x0A_6BDC, "Fallout4.esm", &interner)), &interner);
        assert_eq!(plan(&external, &sndrs, &interner), None);

        // A NULL sound slot is four zero bytes.
        let silent = make_proj(None, &interner);
        assert_eq!(plan(&silent, &sndrs, &interner), None);

        // A truncated DNAM has no sound slot to read.
        let truncated = make_proj_record(FieldValue::Bytes(smallvec::smallvec![0; 20]), &interner);
        assert_eq!(plan(&truncated, &sndrs, &interner), None);
    }

    // ---- set_projectile_sound -------------------------------------------

    #[test]
    fn set_sound_touches_only_the_sound_slot() {
        let interner = StringInterner::new();
        let compound = fk(0x04_FEB1, "Out.esp", &interner);
        let base = fk(0x0A_6BDC, "Fallout4.esm", &interner);
        let mut proj = make_proj(Some(compound), &interner);
        let before = proj.fields[0].value.clone();

        assert!(set_projectile_sound(
            &mut proj,
            dnam(),
            &load_order(),
            &offsets(),
            &interner,
            base
        ));

        let (FieldValue::Bytes(after), FieldValue::Bytes(before)) =
            (&proj.fields[0].value, &before)
        else {
            panic!("expected DNAM bytes");
        };
        let sound_slot = offsets().proj_sound..offsets().proj_sound + 4;
        assert_eq!(after.len(), before.len(), "payload length must not change");
        for (offset, (a, b)) in after.iter().zip(before.iter()).enumerate() {
            if sound_slot.contains(&offset) {
                continue;
            }
            assert_eq!(a, b, "byte {offset} outside the sound slot changed");
        }
        assert_eq!(
            read_u32(after, sound_slot.start),
            Some(0x000A_6BDC),
            "sound repointed at Fallout4.esm:0A6BDC"
        );
    }

    #[test]
    fn set_sound_is_idempotent() {
        let interner = StringInterner::new();
        let base = fk(0x0A_6BDC, "Fallout4.esm", &interner);
        let mut proj = make_proj(Some(base), &interner);
        assert!(
            !set_projectile_sound(
                &mut proj,
                dnam(),
                &load_order(),
                &offsets(),
                &interner,
                base
            ),
            "rewriting the same FormKey must not report a change"
        );
    }

    #[test]
    fn set_sound_refuses_a_plugin_outside_the_load_order() {
        let interner = StringInterner::new();
        let compound = fk(0x04_FEB1, "Out.esp", &interner);
        let stranger = fk(0x01_0000, "Absent.esm", &interner);
        let mut proj = make_proj(Some(compound), &interner);

        assert!(
            !set_projectile_sound(
                &mut proj,
                dnam(),
                &load_order(),
                &offsets(),
                &interner,
                stranger
            ),
            "an unpackable FormKey must leave the payload alone"
        );
    }

    #[test]
    fn set_sound_still_writes_a_decoded_struct() {
        let interner = StringInterner::new();
        let compound = fk(0x04_FEB1, "Out.esp", &interner);
        let base = fk(0x0A_6BDC, "Fallout4.esm", &interner);
        let mut proj = make_proj_struct(Some(compound), &interner);

        assert!(set_projectile_sound(
            &mut proj,
            dnam(),
            &load_order(),
            &offsets(),
            &interner,
            base
        ));

        let FieldValue::Struct(ref fields) = proj.fields[0].value else {
            panic!("expected DNAM struct");
        };
        assert_eq!(fields.len(), 3, "no field may be added or dropped");
        assert_eq!(fields[0].1, FieldValue::Float(2500.0), "speed preserved");
        assert_eq!(fields[1].1, FieldValue::FormKey(base), "sound repointed");
        assert_eq!(fields[2].1, FieldValue::Float(15000.0), "range preserved");
    }
}
