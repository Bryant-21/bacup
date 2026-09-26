//! Carry FO76 keyword-filtered object templates onto the Fallout 4 weapons they map to.
//!
//! FO76 builds its named weapons (Medical Malpractice, Somerset Special, the Survival
//! rewards) from a leveled list holding the *base* weapon plus an instantiation-filter
//! keyword (`if_tmp_<Name>`). The base weapon carries an object-template combination
//! keyed on that keyword which adds `mod_Custom_<Name>`. When the base weapon's EditorID
//! also exists in Fallout4.esm or a DLC, the mapper points it at the vanilla record and
//! the FO76 record, templates included, is never written, so those leveled lists spawn
//! an ordinary weapon. This writes a master override of the vanilla weapon with the
//! FO76 combinations that vanilla has no equivalent for, plus the FO76 attach points
//! their mods hang from.
//!
//! Runs after `rewrite_raw_object_template_formids`: the appended bytes are remapped
//! here, and the vanilla combinations of the override are left untouched.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use esp_authoring_core::plugin_runtime::ParsedSubrecord;

use crate::fixups::quest_script_binding::SourcePlugin;
use crate::fixups::rewrite_raw_object_template_formids::{
    add_output_record_targets, encoded_targets_by_source_object_id, rewrite_obts_bytes,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, write_u32_field};
use crate::session::PluginSession;
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fo76_fo4::strip_raw_weapon_object_template_property_rows;

const OBTS_FIXED_HEADER_LEN: usize = 16;
const OBTS_KEYWORD_COUNT_OFFSET: usize = 15;
const OBTS_INCLUDE_PADDING_LEN: usize = 2;
const OBTS_INCLUDE_ROW_LEN: usize = 7;

pub struct BridgeFo76WeaponObjectTemplatesFixup;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SourceWeaponTemplates {
    attach_parent_slots: Vec<u32>,
    combinations: Vec<SourceCombination>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceCombination {
    editor_only: bool,
    obts: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BridgeOutcome {
    combinations_added: u32,
    unresolved_keywords: u32,
    dropped_includes: u32,
}

impl Fixup for BridgeFo76WeaponObjectTemplatesFixup {
    fn name(&self) -> &'static str {
        "bridge_fo76_weapon_object_templates"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let Some(source) = SourcePlugin::of(session, mapper.interner) else {
            return Ok(report);
        };
        let output_plugin = mapper.output_plugin_sym();
        let target_masters = session.target_masters().to_vec();
        let master_handles: FxHashMap<String, u64> = target_masters
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .zip(config.target_master_handle_ids.iter().copied())
            .collect();

        let weapons = {
            let scan = session.handle_raw_scan(source.id).map_err(handle_error)?;
            source
                .own_raw_form_ids(&scan, SigCode(*b"WEAP"))
                .into_iter()
                .filter_map(|raw_form_id| {
                    let source_weapon = source.form_key(raw_form_id, mapper.interner)?;
                    let target = mapper
                        .lookup(source_weapon)
                        .filter(|target| target.plugin != output_plugin)?;
                    let templates = scan
                        .with_record_subrecords(raw_form_id, |subrecords| {
                            parse_source_weapon_templates(subrecords)
                        })
                        .flatten()?;
                    Some((target, templates))
                })
                .collect::<Vec<_>>()
        };
        if weapons.is_empty() {
            return Ok(report);
        }

        let mut encoded_targets = encoded_targets_by_source_object_id(mapper, &target_masters);
        add_output_record_targets(
            session,
            mapper.interner,
            &target_masters,
            &mut encoded_targets,
        )?;
        let source_targets: FxHashMap<u32, FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (source.local, target))
            .collect();
        let first_master_ids = match config.target_master_handle_ids.first() {
            Some(handle) => session
                .local_object_ids_in_handle(*handle)
                .map_err(handle_error)?,
            None => FxHashSet::default(),
        };
        let mut is_valid_first_master_form_id = |raw: u32| {
            raw == 0
                || raw >> 24 != 0
                || first_master_ids.is_empty()
                || first_master_ids.contains(&raw)
        };

        let schema = session.schema().map_err(handle_error)?;
        let existing_overrides: FxHashSet<FormKey> = session
            .form_keys_of_sig(SigCode(*b"WEAP"), mapper.interner)
            .map_err(handle_error)?
            .into_iter()
            .collect();

        let mut added = Vec::new();
        let mut replaced = Vec::new();
        let mut total = BridgeOutcome::default();
        let mut weapons_bridged = 0u32;
        for (target, templates) in weapons {
            let exists = existing_overrides.contains(&target);
            let vanilla = if exists {
                session.record_decoded(&target, schema.as_ref(), mapper.interner)
            } else {
                let Some(handle) = mapper
                    .interner
                    .resolve(target.plugin)
                    .and_then(|plugin| master_handles.get(&plugin.to_ascii_lowercase()))
                else {
                    continue;
                };
                session.record_decoded_in_handle(*handle, &target, schema.as_ref(), mapper.interner)
            };
            let Ok(mut vanilla) = vanilla else {
                warn(
                    &mut report,
                    mapper.interner,
                    target,
                    "vanilla_weapon_unreadable",
                );
                continue;
            };
            let outcome = bridge_weapon_templates(
                &mut vanilla,
                &templates,
                &encoded_targets,
                &source_targets,
                &mut is_valid_first_master_form_id,
            );
            total.unresolved_keywords += outcome.unresolved_keywords;
            total.dropped_includes += outcome.dropped_includes;
            if outcome.combinations_added == 0 {
                continue;
            }
            total.combinations_added += outcome.combinations_added;
            weapons_bridged += 1;
            if exists {
                replaced.push(vanilla);
            } else {
                added.push(vanilla);
            }
        }

        for record in added {
            session
                .add_record(record, schema.as_ref(), mapper.interner)
                .map_err(handle_error)?;
            report.records_added += 1;
        }
        if !replaced.is_empty() {
            report.records_changed = session
                .replace_records_contents(replaced, schema.as_ref(), mapper.interner)
                .map_err(handle_error)?
                .try_into()
                .unwrap_or(u32::MAX);
        }
        report.message = Some(mapper.interner.intern(&format!(
            "fo76_weapon_object_templates:weapons={weapons_bridged};combinations={};unresolved_keywords={};dropped_includes={}",
            total.combinations_added, total.unresolved_keywords, total.dropped_includes
        )));
        Ok(report)
    }
}

/// The source weapon's attach parent slots and every keyword-filtered combination.
/// Combinations with no keyword are the weapon's defaults, which vanilla has its own of.
fn parse_source_weapon_templates(subrecords: &[ParsedSubrecord]) -> Option<SourceWeaponTemplates> {
    let mut templates = SourceWeaponTemplates::default();
    let mut in_block = false;
    let mut editor_only = false;
    for subrecord in subrecords {
        let data = subrecord.data.as_ref();
        match subrecord.signature.as_str() {
            "APPR" if !in_block => templates.attach_parent_slots.extend(
                data.chunks_exact(4)
                    .map(|raw| u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
            ),
            "OBTE" => in_block = true,
            "STOP" => in_block = false,
            "OBTF" if in_block => editor_only = true,
            "OBTS" if in_block => {
                if obts_keywords(data).is_some_and(|keywords| !keywords.is_empty()) {
                    templates.combinations.push(SourceCombination {
                        editor_only,
                        obts: data.to_vec(),
                    });
                }
                editor_only = false;
            }
            _ => {}
        }
    }
    (!templates.combinations.is_empty()).then_some(templates)
}

fn bridge_weapon_templates(
    vanilla: &mut Record,
    source: &SourceWeaponTemplates,
    encoded_targets: &FxHashMap<u32, u32>,
    source_targets: &FxHashMap<u32, FormKey>,
    is_valid_first_master_form_id: &mut dyn FnMut(u32) -> bool,
) -> BridgeOutcome {
    let mut outcome = BridgeOutcome::default();
    let (Some(count_index), Some(stop_index)) = (
        vanilla
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"OBTE"),
        vanilla
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"STOP"),
    ) else {
        return outcome;
    };
    let mut known_keyword_sets: Vec<Vec<u32>> = vanilla.fields[count_index..stop_index]
        .iter()
        .filter(|field| field.sig.0 == *b"OBTS")
        .filter_map(|field| match &field.value {
            FieldValue::Bytes(bytes) => obts_keywords(bytes).map(sorted),
            _ => None,
        })
        .collect();

    let mut appended = Vec::new();
    for combination in &source.combinations {
        let Some(source_keywords) = obts_keywords(&combination.obts) else {
            continue;
        };
        let Some(target_keywords) = source_keywords
            .iter()
            .map(|raw| encoded_targets.get(&(raw & 0x00FF_FFFF)).copied())
            .collect::<Option<Vec<_>>>()
            .map(sorted)
        else {
            outcome.unresolved_keywords += 1;
            continue;
        };
        if known_keyword_sets.contains(&target_keywords) {
            continue;
        }
        let mut obts: SmallVec<[u8; 32]> = SmallVec::from_slice(&combination.obts);
        let Some(dropped) = retain_includes(&mut obts, |raw| {
            encoded_targets.contains_key(&(raw & 0x00FF_FFFF))
        }) else {
            continue;
        };
        outcome.dropped_includes += dropped;
        rewrite_obts_bytes(&mut obts, encoded_targets, is_valid_first_master_form_id);
        strip_raw_weapon_object_template_property_rows(&mut obts);
        if combination.editor_only {
            appended.push(field(b"OBTF", FieldValue::Bytes(SmallVec::new())));
        }
        appended.push(field(b"OBTS", FieldValue::Bytes(obts)));
        known_keyword_sets.push(target_keywords);
        outcome.combinations_added += 1;
    }
    if outcome.combinations_added == 0 {
        return outcome;
    }

    let vanilla_count = field_value_u32(&vanilla.fields[count_index].value).unwrap_or(0);
    write_u32_field(
        &mut vanilla.fields[count_index].value,
        vanilla_count + outcome.combinations_added,
    );
    vanilla.fields.insert_many(stop_index, appended);
    add_attach_parent_slots(
        vanilla,
        &source.attach_parent_slots,
        encoded_targets,
        source_targets,
    );
    outcome
}

fn add_attach_parent_slots(
    record: &mut Record,
    source_slots: &[u32],
    encoded_targets: &FxHashMap<u32, u32>,
    source_targets: &FxHashMap<u32, FormKey>,
) {
    let Some(slots) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"APPR")
    else {
        return;
    };
    match &mut slots.value {
        FieldValue::List(values) => {
            for target in source_slots
                .iter()
                .filter_map(|raw| source_targets.get(&(raw & 0x00FF_FFFF)))
            {
                let value = FieldValue::FormKey(*target);
                if !values.contains(&value) {
                    values.push(value);
                }
            }
        }
        FieldValue::Bytes(bytes) => {
            let mut present: FxHashSet<u32> = bytes
                .chunks_exact(4)
                .map(|raw| u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
                .collect();
            for encoded in source_slots
                .iter()
                .filter_map(|raw| encoded_targets.get(&(raw & 0x00FF_FFFF)))
            {
                if present.insert(*encoded) {
                    bytes.extend_from_slice(&encoded.to_le_bytes());
                }
            }
        }
        _ => {}
    }
}

fn obts_keywords(bytes: &[u8]) -> Option<Vec<u32>> {
    let keyword_count = usize::from(*bytes.get(OBTS_KEYWORD_COUNT_OFFSET)?);
    let keywords = bytes.get(OBTS_FIXED_HEADER_LEN..OBTS_FIXED_HEADER_LEN + keyword_count * 4)?;
    Some(
        keywords
            .chunks_exact(4)
            .map(|raw| u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
            .collect(),
    )
}

/// Drops include rows whose mod did not survive conversion; a raw source id left in
/// place would name an unrelated Fallout4.esm form. Returns the number dropped.
fn retain_includes(bytes: &mut SmallVec<[u8; 32]>, keep: impl Fn(u32) -> bool) -> Option<u32> {
    let include_count =
        usize::try_from(u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?)).ok()?;
    let keyword_count = usize::from(*bytes.get(OBTS_KEYWORD_COUNT_OFFSET)?);
    let includes_start = OBTS_FIXED_HEADER_LEN + keyword_count * 4 + OBTS_INCLUDE_PADDING_LEN;
    let includes_end = includes_start + include_count * OBTS_INCLUDE_ROW_LEN;
    let rows = bytes.get(includes_start..includes_end)?;
    let kept: Vec<u8> = rows
        .chunks_exact(OBTS_INCLUDE_ROW_LEN)
        .filter(|row| keep(u32::from_le_bytes([row[0], row[1], row[2], row[3]])))
        .flatten()
        .copied()
        .collect();
    let kept_count = kept.len() / OBTS_INCLUDE_ROW_LEN;
    let dropped = u32::try_from(include_count - kept_count).ok()?;
    if dropped > 0 {
        let mut rebuilt: SmallVec<[u8; 32]> = SmallVec::from_slice(&bytes[..includes_start]);
        rebuilt[0..4].copy_from_slice(&u32::try_from(kept_count).ok()?.to_le_bytes());
        rebuilt.extend_from_slice(&kept);
        rebuilt.extend_from_slice(&bytes[includes_end..]);
        *bytes = rebuilt;
    }
    Some(dropped)
}

fn field_value_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(n) => u32::try_from(*n).ok(),
        FieldValue::Int(n) => u32::try_from(*n).ok(),
        FieldValue::Bytes(bytes) => Some(u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?)),
        _ => None,
    }
}

fn sorted(mut values: Vec<u32>) -> Vec<u32> {
    values.sort_unstable();
    values
}

fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value,
    }
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn warn(report: &mut FixupReport, interner: &StringInterner, weapon: FormKey, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "bridge_fo76_weapon_object_templates:{:06X}:{reason}",
        weapon.local
    )));
}

#[cfg(test)]
mod tests;
