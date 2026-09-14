//! Fixup: repoint NPC outfit entries at the playable twin of a `_NONPLAYABLE` armor.
//!
//! FO76 dresses NPCs in `*_NONPLAYABLE` duplicates of ordinary armor and clothing.
//! Those carry the `Non-Playable` header flag (`0x04`), which FO4 honours by keeping
//! the item out of the player inventory, so a converted raider never drops his
//! jacket. `LVLI` and `OTFT` entries are repointed at the standard twin (same
//! EditorID minus the suffix, otherwise identical, e.g.
//! `Clothes_GreaserJacketLeather_BloodEagle`), looked up in the output first, then in
//! the base-game masters (most pieces duplicate stock FO4 armor such as
//! `Armor_Combat_ArmLeft`). Duplicates with no twin keep pointing at the
//! non-playable record; making them lootable would mean clearing the flag instead.
//!
//! `records_changed` = `LVLI`/`OTFT` records with at least one entry repointed.

use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::Sym;
use rustc_hash::FxHashMap;

/// EditorID suffixes FO76 uses for the NPC-only duplicate. `_nonplayalbe` is a
/// real typo in the source data (`LL_Armor_Leather_ArmLeft_Heavy_NONPLAYALBE`),
/// not a mistake here.
const NONPLAYABLE_SUFFIXES: [&str; 3] = ["_nonplayable", "_notplayable", "_nonplayalbe"];

/// Signatures whose entries point at the armor an NPC ends up wearing.
const REFERRING_SIGS: [&str; 2] = ["LVLI", "OTFT"];

pub struct RemapNonplayableArmorToPlayableTwinFixup;

impl Fixup for RemapNonplayableArmorToPlayableTwinFixup {
    fn name(&self) -> &'static str {
        "remap_nonplayable_armor_to_playable_twin"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.target_schema.is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let armo_sig =
            SigCode::from_str("ARMO").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let armo_form_keys = session
            .form_keys_of_sig(armo_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if armo_form_keys.is_empty() {
            return Ok(report);
        }

        // Index by decoding each armor, not by zipping the signature list with
        // `raw_form_ids_of_sig`: that list skips empty-plugin keys, so the two agree
        // only up to the first skip and later pairings attach EditorIDs to the
        // wrong armor.
        let mut armor_by_editor_id: FxHashMap<String, FormKey> = FxHashMap::default();
        for form_key in &armo_form_keys {
            let record = match session.record_decoded(form_key, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(error) => {
                    let warning = mapper
                        .interner
                        .intern(&format!("nonplayable_twin_read_err:{error}"));
                    report.warnings.push(warning);
                    continue;
                }
            };
            if let Some(editor_id) = record
                .eid
                .and_then(|eid| mapper.interner.resolve(eid))
                .map(|eid| eid.to_ascii_lowercase())
            {
                armor_by_editor_id.insert(editor_id, *form_key);
            }
        }

        // An empty index against a non-empty signature list means the EditorIDs
        // did not come back, not that the plugin has no armor.
        if armor_by_editor_id.is_empty() {
            let warning = mapper
                .interner
                .intern("nonplayable_twin_no_editor_ids_for_target_handle");
            report.warnings.push(warning);
            return Ok(report);
        }

        let remap = build_twin_remap(&armor_by_editor_id, |stripped| {
            mapper.find_vanilla_fk(stripped, armo_sig)
        });
        if remap.is_empty() {
            return Ok(report);
        }

        // `LVLI.LVLO` is `struct:H,B,B,I,H,B,B`, which decodes to one opaque
        // `FieldValue::Bytes` blob, so its item FormID is invisible to a FormKey
        // walk. Those bytes are already target-encoded here, hence the second map.
        let masters = session.target_masters().to_vec();
        let encoded_remap: FxHashMap<u32, u32> = remap
            .iter()
            .filter_map(|(source, twin)| {
                Some((
                    encode_target_form_id(*source, mapper.interner, &masters)?,
                    encode_target_form_id(*twin, mapper.interner, &masters)?,
                ))
            })
            .collect();

        let mut changed_records = Vec::new();
        for sig in REFERRING_SIGS {
            let sig = SigCode::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let form_keys = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for form_key in form_keys {
                let mut record =
                    match session.record_decoded(&form_key, target_schema, mapper.interner) {
                        Ok(record) => record,
                        Err(error) => {
                            let warning = mapper
                                .interner
                                .intern(&format!("nonplayable_twin_read_err:{error}"));
                            report.warnings.push(warning);
                            continue;
                        }
                    };
                let plugin = record.form_key.plugin;
                if repoint_record(&mut record, plugin, &remap, &encoded_remap, target_schema) {
                    changed_records.push(record);
                }
            }
        }

        let expected = changed_records.len();
        let remapped = remap.len();
        session
            .replace_records(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = expected.try_into().unwrap_or(u32::MAX);
        report.message = Some(mapper.interner.intern(&format!(
            "armor={} remap={remapped} encoded={} records_repointed={expected}",
            armor_by_editor_id.len(),
            encoded_remap.len()
        )));

        Ok(report)
    }
}

fn strip_nonplayable_suffix(editor_id_lower: &str) -> Option<&str> {
    NONPLAYABLE_SUFFIXES
        .iter()
        .find_map(|suffix| editor_id_lower.strip_suffix(suffix))
}

/// `FormKey of the NPC-only duplicate` → `FormKey of its playable twin`, for every
/// `*_NONPLAYABLE` armor whose stripped EditorID names another armor. Converted
/// armor wins over the base game, since the rest of the conversion is wired to it;
/// `find_vanilla` covers twins that exist only in `Fallout4.esm`
/// (`Armor_Combat_ArmLeft`, `Armor_RaiderMod_Torso`).
fn build_twin_remap(
    armor_by_editor_id: &FxHashMap<String, FormKey>,
    mut find_vanilla: impl FnMut(&str) -> Option<FormKey>,
) -> FxHashMap<FormKey, FormKey> {
    let mut remap = FxHashMap::default();
    for (editor_id, form_key) in armor_by_editor_id {
        let Some(stripped) = strip_nonplayable_suffix(editor_id) else {
            continue;
        };
        let twin = armor_by_editor_id
            .get(stripped)
            .copied()
            .or_else(|| find_vanilla(stripped));
        if let Some(twin) = twin {
            if twin != *form_key {
                remap.insert(*form_key, twin);
            }
        }
    }
    remap
}

/// Rewrite every own-plugin FormKey in `value` that names a remapped record.
/// Recurses so `LVLI` entry structs and `OTFT` item lists are both covered.
/// Shared with the creature-weapon twin remap, which needs the same walk over
/// `LVLI` and `NPC_` inventory entries.
pub(crate) fn repoint_form_keys(
    value: &mut FieldValue,
    plugin: Sym,
    remap: &FxHashMap<u32, FormKey>,
) -> bool {
    match value {
        FieldValue::FormKey(form_key) => {
            if form_key.plugin != plugin {
                return false;
            }
            match remap.get(&form_key.local) {
                Some(&twin) => {
                    *form_key = twin;
                    true
                }
                None => false,
            }
        }
        FieldValue::List(items) => {
            let mut touched = false;
            for item in items.iter_mut() {
                touched |= repoint_form_keys(item, plugin, remap);
            }
            touched
        }
        FieldValue::Struct(fields) => {
            let mut touched = false;
            for (_, field) in fields.iter_mut() {
                touched |= repoint_form_keys(field, plugin, remap);
            }
            touched
        }
        _ => false,
    }
}

/// Repoint every reference to a remapped record inside `record`. Shared with the
/// creature-weapon twin remap, which covers the same two shapes.
///
/// Typed `FormKey` leaves (`OTFT.INAM`, codec `formid_array`) are walked directly.
/// `LVLI.LVLO` (`struct:H,B,B,I,H,B,B`) and `NPC_.CNTO` (`struct:I,i`) decode to one
/// opaque `FieldValue::Bytes` blob (`source_read::decode_subrecord`), so their
/// FormIDs are matched in the bytes. `remap_struct_internal_formids` runs earlier in
/// `fixups_v2`, so those ids are already target-encoded (`0x08_7AC763`, not
/// `0x00_7AC763`), hence the encoded lookup.
pub(crate) fn repoint_record(
    record: &mut Record,
    plugin: Sym,
    remap: &FxHashMap<FormKey, FormKey>,
    encoded_remap: &FxHashMap<u32, u32>,
    schema: &crate::schema::AuthoringSchema,
) -> bool {
    let local_remap: FxHashMap<u32, FormKey> = remap
        .iter()
        .filter(|(source, _)| source.plugin == plugin)
        .map(|(source, twin)| (source.local, *twin))
        .collect();
    let record_sig = record.sig.as_str().to_string();
    let mut touched = false;
    for entry in record.fields.iter_mut() {
        match &mut entry.value {
            FieldValue::Bytes(bytes) => {
                let sub_sig = entry.sig.as_str().to_string();
                if rewrite_struct_form_ids(bytes, &record_sig, &sub_sig, encoded_remap, schema) {
                    touched = true;
                }
            }
            value => {
                if repoint_form_keys(value, plugin, &local_remap) {
                    touched = true;
                }
            }
        }
    }
    touched
}

/// Rewrite target-encoded FormIDs inside one struct-codec subrecord blob.
///
/// Field offsets come from the schema rather than being hardcoded, and a row
/// that does not divide evenly into the layout stride is left alone rather than
/// smeared — a wrong offset here corrupts neighbouring fields silently.
pub(crate) fn rewrite_struct_form_ids(
    bytes: &mut [u8],
    record_sig: &str,
    sub_sig: &str,
    encoded_remap: &FxHashMap<u32, u32>,
    schema: &crate::schema::AuthoringSchema,
) -> bool {
    if encoded_remap.is_empty() {
        return false;
    }
    let layout = schema.struct_field_layout_versioned(
        record_sig,
        sub_sig,
        Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
    );
    if layout.is_empty() {
        return false;
    }
    let row_size = layout
        .iter()
        .map(|field| field.offset + field.width)
        .max()
        .unwrap_or(0);
    if row_size == 0 || bytes.len() % row_size != 0 {
        return false;
    }
    let form_id_fields: Vec<_> = layout
        .iter()
        .filter(|field| field.width == 4 && !field.formlink_targets.is_empty())
        .collect();
    if form_id_fields.is_empty() {
        return false;
    }

    let mut touched = false;
    for row in 0..bytes.len() / row_size {
        for field in &form_id_fields {
            let offset = row * row_size + field.offset;
            let Some(slot) = bytes.get_mut(offset..offset + 4) else {
                continue;
            };
            let raw = u32::from_le_bytes(slot.try_into().unwrap());
            if let Some(encoded) = encoded_remap.get(&raw) {
                slot.copy_from_slice(&encoded.to_le_bytes());
                touched = true;
            }
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    use crate::sym::StringInterner;

    fn own(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    fn armor_index(
        interner: &StringInterner,
        entries: &[(&str, u32)],
    ) -> FxHashMap<String, FormKey> {
        entries
            .iter()
            .map(|(eid, local)| (eid.to_ascii_lowercase(), own(interner, *local)))
            .collect()
    }

    fn no_vanilla(_: &str) -> Option<FormKey> {
        None
    }

    #[test]
    fn maps_nonplayable_armor_onto_its_standard_twin() {
        let interner = StringInterner::new();
        let index = armor_index(
            &interner,
            &[
                ("Clothes_GreaserJacketLeather_BloodEagle", 0x59A80D),
                (
                    "Clothes_GreaserJacketLeather_BloodEagle_NONPLAYABLE",
                    0x59AA5E,
                ),
            ],
        );
        let remap = build_twin_remap(&index, no_vanilla);
        assert_eq!(
            remap.get(&own(&interner, 0x59AA5E)),
            Some(&own(&interner, 0x59A80D))
        );
        assert_eq!(remap.len(), 1, "the twin itself must not be remapped");
    }

    #[test]
    fn leaves_nonplayable_armor_with_no_twin_alone() {
        // Robot/shadowed variants are genuinely NPC-only — making them lootable
        // would hand the player equipment that has no playable form.
        let interner = StringInterner::new();
        let index = armor_index(
            &interner,
            &[("Armor_DLC01_Robot_ArmRight_Shadowed_NONPLAYABLE", 0x644CF2)],
        );
        assert!(build_twin_remap(&index, no_vanilla).is_empty());
    }

    #[test]
    fn falls_back_to_the_base_game_twin() {
        // Most _NONPLAYABLE pieces duplicate stock FO4 armor, whose playable
        // twin only ever existed in Fallout4.esm.
        let interner = StringInterner::new();
        let vanilla = FormKey {
            local: 0x11D3C7,
            plugin: interner.intern("Fallout4.esm"),
        };
        let index = armor_index(&interner, &[("Armor_Combat_ArmLeft_NONPLAYABLE", 0x59878C)]);
        let remap = build_twin_remap(&index, |stripped| {
            (stripped == "armor_combat_armleft").then_some(vanilla)
        });
        assert_eq!(remap.get(&own(&interner, 0x59878C)), Some(&vanilla));
    }

    #[test]
    fn prefers_our_own_converted_twin_over_the_base_game() {
        let interner = StringInterner::new();
        let vanilla = FormKey {
            local: 0x0536C1,
            plugin: interner.intern("Fallout4.esm"),
        };
        let index = armor_index(
            &interner,
            &[
                ("Clothes_GreaserJacket", 1),
                ("Clothes_GreaserJacket_NONPLAYABLE", 2),
            ],
        );
        let remap = build_twin_remap(&index, |_| Some(vanilla));
        assert_eq!(remap.get(&own(&interner, 2)), Some(&own(&interner, 1)));
    }

    #[test]
    fn matches_suffix_spelling_variants_case_insensitively() {
        let interner = StringInterner::new();
        let index = armor_index(
            &interner,
            &[
                ("Clothes_GreaserJacket", 1),
                ("Clothes_GreaserJacket_NotPlayable", 2),
                ("Armor_Leather_ArmLeft_Heavy", 3),
                // Real typo in the FO76 source data.
                ("Armor_Leather_ArmLeft_Heavy_NONPLAYALBE", 4),
            ],
        );
        let remap = build_twin_remap(&index, no_vanilla);
        assert_eq!(remap.get(&own(&interner, 2)), Some(&own(&interner, 1)));
        assert_eq!(remap.get(&own(&interner, 4)), Some(&own(&interner, 3)));
    }

    #[test]
    fn repoints_nested_entries_but_not_other_plugins() {
        let interner = StringInterner::new();
        let own = interner.intern("SeventySix.esm");
        let base = interner.intern("Fallout4.esm");
        let twin = FormKey {
            local: 0x59A80D,
            plugin: own,
        };
        let remap: FxHashMap<u32, FormKey> = [(0x59AA5E, twin)].into_iter().collect();

        // An LVLI entry struct wrapping the item reference.
        let mut entry = FieldValue::Struct(vec![(
            interner.intern("Item"),
            FieldValue::FormKey(FormKey {
                local: 0x59AA5E,
                plugin: own,
            }),
        )]);
        assert!(repoint_form_keys(&mut entry, own, &remap));
        let FieldValue::Struct(fields) = &entry else {
            panic!("struct");
        };
        assert_eq!(
            fields[0].1,
            FieldValue::FormKey(FormKey {
                local: 0x59A80D,
                plugin: own,
            })
        );

        // A same-numbered object-id in a master must not be touched.
        let mut foreign = FieldValue::List(vec![FieldValue::FormKey(FormKey {
            local: 0x59AA5E,
            plugin: base,
        })]);
        assert!(!repoint_form_keys(&mut foreign, own, &remap));
    }
}
