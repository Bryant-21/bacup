//! Fixup: repoint NPC weapon entries at the playable twin of a `cr` weapon,
//! and unseal the corpses that then have something worth looting.
//!
//! FO76 arms humanoid creatures (mole miners, Scorched, Fanatics) from `cr`-marked
//! duplicates of ordinary weapons (`crPumpActionShotgun`, `HTO_crPowerFist`). Those
//! carry the `Non-Playable` header flag (`0x04`), which FO4 honours by keeping the
//! item out of the player inventory, so a converted mole miner never drops its
//! shotgun. `LVLI` and `NPC_` inventory entries are repointed at the standard twin
//! (same EditorID minus the `cr`), looked up in the output first, then in the
//! base-game masters (`DEL_crMinigun`, `DEL_crPipeGun` shadow stock FO4 weapons).
//! Duplicates are found by the `cr` marker, not the flag (see `build_twin_remap`).
//! `cr` weapons without a twin are natural attacks (`crUnarmedBigfoot`,
//! `crE01B_AssaultronRightClaw`) and keep their entries.
//!
//! FO76 hands out mob loot server-side and seals corpses with the `ACBS` `No Loot`
//! flag (`0x1000`), set on about a fifth of converted actors and on no vanilla FO4
//! actor; in FO4 the body cannot be opened. The flag is cleared only on actors whose
//! own `CNTO` reaches a repointed weapon through any depth of leveled lists (a mole
//! miner points at `HTO_crLLI_MoleMiner_Mob_Sniper`, then a per-weapon list). Actors
//! that inherit inventory from a template are left alone. Placed `cr` weapons
//! (`REFR`) are not touched.
//!
//! `records_changed` = `LVLI` records with a repointed entry plus `NPC_` records with
//! a repointed entry or a cleared `No Loot` flag.

use crate::fixups::remap_nonplayable_armor_to_playable_twin::repoint_record;
use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use rustc_hash::{FxHashMap, FxHashSet};

/// `ACBS` flag bit FO4 reads as "this corpse cannot be looted".
const NO_LOOT_BIT: u32 = 0x0000_1000;

pub struct RemapCreatureWeaponToPlayableTwinFixup;

impl Fixup for RemapCreatureWeaponToPlayableTwinFixup {
    fn name(&self) -> &'static str {
        "remap_creature_weapon_to_playable_twin"
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

        let weap_sig =
            SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let weap_form_keys = session
            .form_keys_of_sig(weap_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if weap_form_keys.is_empty() {
            return Ok(report);
        }

        // Indexed by decoding each weapon rather than by zipping the signature
        // list against `raw_form_ids_of_sig`: that ordering is documented against
        // `form_keys_of_sig_in_handle`, not the list used here, so pairing the two
        // risks attaching an EditorID to the wrong FormKey. Decoding by FormKey is
        // the same route the leveled-list and actor passes below already take.
        let mut weapons_by_editor_id: FxHashMap<String, FormKey> = FxHashMap::default();
        let mut marked: Vec<(FormKey, String)> = Vec::new();
        for form_key in &weap_form_keys {
            let Some(record) = decode(session, form_key, target_schema, mapper, &mut report) else {
                continue;
            };
            let Some(editor_id) = record
                .eid
                .and_then(|eid| mapper.interner.resolve(eid))
                .map(|eid| eid.to_ascii_lowercase())
            else {
                continue;
            };
            if !twin_candidates(&editor_id).is_empty() {
                marked.push((*form_key, editor_id.clone()));
            }
            weapons_by_editor_id.insert(editor_id, *form_key);
        }

        // An empty index against a non-empty signature list means the EditorIDs
        // did not come back, not that the plugin has no weapons — the state this
        // pass silently sat in while `own_editor_ids` returned nothing for the
        // target handle. Say so rather than reporting a clean no-op.
        if weapons_by_editor_id.is_empty() {
            let warning = mapper
                .interner
                .intern("creature_weapon_twin_no_editor_ids_for_target_handle");
            report.warnings.push(warning);
            return Ok(report);
        }

        let weapon_count = weapons_by_editor_id.len();
        let marked_count = marked.len();
        let sample = marked
            .first()
            .map(|(_, editor_id)| editor_id.clone())
            .unwrap_or_default();

        let remap = build_twin_remap(&weapons_by_editor_id, &marked, |stripped| {
            mapper.find_vanilla_fk(stripped, weap_sig)
        });
        if remap.is_empty() {
            report.message = Some(mapper.interner.intern(&format!(
                "weapons={weapon_count} marked={marked_count} remap=0 first_marked={sample}"
            )));
            return Ok(report);
        }

        let lvli_sig =
            SigCode::from_str("LVLI").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let lvli_form_keys = session
            .form_keys_of_sig(lvli_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        // Everything below works in target-encoded FormIDs, because that is how
        // the ids inside `LVLO`/`CNTO` struct blobs are stored by the time this
        // pass runs.
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
        let encoded_lists: FxHashSet<u32> = lvli_form_keys
            .iter()
            .filter_map(|key| encode_target_form_id(*key, mapper.interner, &masters))
            .collect();

        let mut changed_records = Vec::new();
        let mut lists_repointed = 0usize;
        let mut actors_unsealed = 0usize;

        // Pass 1: repoint the leveled lists, and note which of them come to hold
        // a twin - directly here, or through a nested list resolved below.
        let mut carries_twin: FxHashSet<u32> = FxHashSet::default();
        let mut nested_lists: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
        for form_key in &lvli_form_keys {
            let Some(mut record) = decode(session, form_key, target_schema, mapper, &mut report)
            else {
                continue;
            };
            let plugin = record.form_key.plugin;

            if let Some(encoded_self) = encode_target_form_id(*form_key, mapper.interner, &masters)
            {
                let entries = read_struct_form_ids(&record, "LVLO", target_schema);
                if entries
                    .iter()
                    .any(|entry| encoded_remap.contains_key(entry))
                {
                    carries_twin.insert(encoded_self);
                }
                let children: Vec<u32> = entries
                    .into_iter()
                    .filter(|entry| encoded_lists.contains(entry))
                    .collect();
                if !children.is_empty() {
                    nested_lists.insert(encoded_self, children);
                }
            }

            if repoint_record(&mut record, plugin, &remap, &encoded_remap, target_schema) {
                lists_repointed += 1;
                changed_records.push(record);
            }
        }
        propagate_nested_lists(&nested_lists, &mut carries_twin);

        // Pass 2: repoint actor inventories, and unseal the ones this pass armed.
        let npc_form_keys = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for form_key in npc_form_keys {
            let Some(mut record) = decode(session, &form_key, target_schema, mapper, &mut report)
            else {
                continue;
            };
            let plugin = record.form_key.plugin;

            let inventory = read_struct_form_ids(&record, "CNTO", target_schema);
            let armed = inventory
                .iter()
                .any(|item| encoded_remap.contains_key(item) || carries_twin.contains(item));

            let mut touched =
                repoint_record(&mut record, plugin, &remap, &encoded_remap, target_schema);
            if armed && clear_no_loot(&mut record, target_schema) {
                touched = true;
                actors_unsealed += 1;
            }
            if touched {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        let carrying = carries_twin.len();
        session
            .replace_records(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = expected.try_into().unwrap_or(u32::MAX);
        report.message = Some(mapper.interner.intern(&format!(
            "weapons={weapon_count} marked={marked_count} remap={} encoded={} lists_carrying={carrying} lists_repointed={lists_repointed} actors_unsealed={actors_unsealed}",
            remap.len(),
            encoded_remap.len()
        )));

        Ok(report)
    }
}

fn decode(
    session: &mut PluginSession,
    form_key: &FormKey,
    target_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
    report: &mut FixupReport,
) -> Option<Record> {
    match session.record_decoded(form_key, target_schema, mapper.interner) {
        Ok(record) => Some(record),
        Err(error) => {
            let warning = mapper
                .interner
                .intern(&format!("creature_weapon_twin_read_err:{error}"));
            report.warnings.push(warning);
            None
        }
    }
}

/// A leveled list that draws from another inherits whatever that one holds, so
/// carrying a twin propagates up the nesting until nothing new is reached.
fn propagate_nested_lists(
    nested_lists: &FxHashMap<u32, Vec<u32>>,
    carries_twin: &mut FxHashSet<u32>,
) {
    loop {
        let additions: Vec<u32> = nested_lists
            .iter()
            .filter(|(list, _)| !carries_twin.contains(list))
            .filter(|(_, children)| children.iter().any(|child| carries_twin.contains(child)))
            .map(|(list, _)| *list)
            .collect();
        if additions.is_empty() {
            return;
        }
        carries_twin.extend(additions);
    }
}

/// Target-encoded FormIDs held in every `sub_sig` struct blob of `record`.
///
/// The read counterpart of `rewrite_struct_form_ids`: `LVLO` and `CNTO` are
/// struct codecs, so what a list holds or an actor carries cannot be seen by
/// walking typed FormKeys.
fn read_struct_form_ids(
    record: &Record,
    sub_sig: &str,
    schema: &crate::schema::AuthoringSchema,
) -> Vec<u32> {
    let layout = schema.struct_field_layout_versioned(
        record.sig.as_str(),
        sub_sig,
        Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
    );
    let row_size = layout
        .iter()
        .map(|field| field.offset + field.width)
        .max()
        .unwrap_or(0);
    let form_id_fields: Vec<_> = layout
        .iter()
        .filter(|field| field.width == 4 && !field.formlink_targets.is_empty())
        .collect();
    if row_size == 0 || form_id_fields.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for entry in record.fields.iter().filter(|e| e.sig.as_str() == sub_sig) {
        let FieldValue::Bytes(bytes) = &entry.value else {
            continue;
        };
        if bytes.len() % row_size != 0 {
            continue;
        }
        for row in 0..bytes.len() / row_size {
            for field in &form_id_fields {
                let offset = row * row_size + field.offset;
                if let Some(slot) = bytes.get(offset..offset + 4) {
                    out.push(u32::from_le_bytes(slot.try_into().unwrap()));
                }
            }
        }
    }
    out
}

/// Drop `No Loot` from the actor `ACBS` flags, so the corpse can be opened.
///
/// `ACBS` is `struct:I,h,H,H,H,h,H,H,B,B`, and every `struct:` codec decodes to
/// one opaque `FieldValue::Bytes` blob (`source_read::decode_subrecord`), so the
/// flags are cleared in place in those bytes. The `flags` offset comes from the
/// schema rather than being hardcoded at 0.
fn clear_no_loot(record: &mut Record, schema: &crate::schema::AuthoringSchema) -> bool {
    let Some(field) = schema
        .struct_field_layout_versioned(
            record.sig.as_str(),
            "ACBS",
            Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
        )
        .into_iter()
        .find(|field| field.field_id == "flags" && field.width == 4)
    else {
        return false;
    };

    let mut cleared = false;
    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "ACBS" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let Some(slot) = bytes.get_mut(field.offset..field.offset + 4) else {
            continue;
        };
        let bits = u32::from_le_bytes(slot.try_into().unwrap());
        if bits & NO_LOOT_BIT == 0 {
            continue;
        }
        slot.copy_from_slice(&(bits & !NO_LOOT_BIT).to_le_bytes());
        cleared = true;
    }
    cleared
}

/// EditorIDs the standard twin of `editor_id_lower` could go by, best first.
///
/// FO76 writes the marker either as a leading `cr` (`crPumpActionShotgun`) or on
/// the segment after a campaign prefix (`HTO_crPowerFist`, `DEL_crMinigun`). The
/// prefixed form has two plausible twins, because FO76 is inconsistent about
/// whether the prefix belongs to the duplicate or to the weapon: `HTO_crBearArm`
/// sits beside plain `BearArm`, while other pairs keep the prefix. Both are
/// offered rather than guessed at.
fn twin_candidates(editor_id_lower: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(stripped) = editor_id_lower.strip_prefix("cr") {
        candidates.push(stripped.to_string());
    }
    let mut search = 0;
    while let Some(offset) = editor_id_lower[search..].find("_cr") {
        let at = search + offset;
        candidates.push(format!(
            "{}{}",
            &editor_id_lower[..=at],
            &editor_id_lower[at + 3..]
        ));
        candidates.push(editor_id_lower[at + 3..].to_string());
        search = at + 1;
    }
    candidates.retain(|candidate| candidate.len() > 2);
    candidates
}

/// `FormKey of the creature duplicate` → `FormKey of its standard twin`, for every
/// `cr` weapon whose stripped EditorID names another weapon. Converted weapons win
/// over the base game, since the rest of the conversion is wired to them.
///
/// Selection ignores the `Non-Playable` header flag: on the target raw scan at
/// fixup time it reads as clear for every record. The marker alone is exact: all 89
/// weapons with both a `cr` marker and a twin are non-playable.
fn build_twin_remap(
    weapons_by_editor_id: &FxHashMap<String, FormKey>,
    marked: &[(FormKey, String)],
    mut find_vanilla: impl FnMut(&str) -> Option<FormKey>,
) -> FxHashMap<FormKey, FormKey> {
    let mut remap = FxHashMap::default();
    for (form_key, editor_id) in marked {
        for candidate in twin_candidates(editor_id) {
            let twin = weapons_by_editor_id
                .get(candidate.as_str())
                .copied()
                .or_else(|| find_vanilla(&candidate));
            if let Some(twin) = twin {
                if twin != *form_key {
                    remap.insert(*form_key, twin);
                }
                break;
            }
        }
    }
    remap
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::remap_nonplayable_armor_to_playable_twin::rewrite_struct_form_ids;
    use crate::ids::SubrecordSig;
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;
    use smallvec::smallvec;

    fn own(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    fn no_vanilla(_: &str) -> Option<FormKey> {
        None
    }

    fn index(interner: &StringInterner, entries: &[(&str, u32)]) -> FxHashMap<String, FormKey> {
        entries
            .iter()
            .map(|(editor_id, local)| (editor_id.to_ascii_lowercase(), own(interner, *local)))
            .collect()
    }

    fn fo4_schema() -> std::sync::Arc<crate::schema::AuthoringSchema> {
        crate::schema::AuthoringSchema::for_game("fo4").expect("fo4 schema")
    }

    /// `ACBS` as the decoder actually hands it over: `struct:I,h,H,H,H,h,H,H,B,B`
    /// is a `struct:` codec, so it arrives as one opaque byte blob.
    fn npc_with_acbs_flags(interner: &StringInterner, flags: u32) -> Record {
        let mut record = Record::new(SigCode::from_str("NPC_").unwrap(), own(interner, 0x10));
        let mut bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        record.fields = smallvec![FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Bytes(bytes),
        }];
        record
    }

    /// One `LVLO` row: `struct:H,B,B,I,H,B,B`, FormID at byte 4.
    fn lvlo_row(item: u32) -> [u8; 12] {
        let mut row = [0u8; 12];
        row[0..2].copy_from_slice(&1u16.to_le_bytes());
        row[4..8].copy_from_slice(&item.to_le_bytes());
        row[8..10].copy_from_slice(&1u16.to_le_bytes());
        row
    }

    fn lvli_with_entries(interner: &StringInterner, items: &[u32]) -> Record {
        let mut record = Record::new(SigCode::from_str("LVLI").unwrap(), own(interner, 0x20));
        let mut bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        for item in items {
            bytes.extend_from_slice(&lvlo_row(*item));
        }
        record.fields = smallvec![FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Bytes(bytes),
        }];
        record
    }

    #[test]
    fn offers_prefixed_twin_before_bare_twin() {
        assert_eq!(
            twin_candidates("hto_crbeararm"),
            vec!["hto_beararm".to_string(), "beararm".to_string()]
        );
    }

    #[test]
    fn strips_a_leading_marker() {
        assert_eq!(
            twin_candidates("crpumpactionshotgun"),
            vec!["pumpactionshotgun"]
        );
    }

    #[test]
    fn maps_creature_weapon_onto_its_playable_twin() {
        let interner = StringInterner::new();
        let weapons = index(
            &interner,
            &[("crPumpActionShotgun", 0x01), ("PumpActionShotgun", 0x02)],
        );
        let marked = vec![(own(&interner, 0x01), "crpumpactionshotgun".to_string())];

        let remap = build_twin_remap(&weapons, &marked, no_vanilla);

        assert_eq!(
            remap.get(&own(&interner, 0x01)),
            Some(&own(&interner, 0x02))
        );
    }

    #[test]
    fn leaves_a_natural_attack_with_no_twin_alone() {
        let interner = StringInterner::new();
        let weapons = index(&interner, &[("crUnarmedBigfoot", 0x01)]);
        let marked = vec![(own(&interner, 0x01), "crunarmedbigfoot".to_string())];

        let remap = build_twin_remap(&weapons, &marked, no_vanilla);

        assert!(remap.is_empty());
    }

    #[test]
    fn leaves_a_weapon_with_no_marker_alone() {
        let interner = StringInterner::new();
        let weapons = index(&interner, &[("Crossbow", 0x01), ("ossbow", 0x02)]);
        let marked: Vec<(FormKey, String)> = Vec::new();

        let remap = build_twin_remap(&weapons, &marked, no_vanilla);

        assert!(remap.is_empty());
    }

    #[test]
    fn falls_back_to_the_base_game_twin() {
        let interner = StringInterner::new();
        let vanilla = FormKey {
            local: 0x0F_FFFF,
            plugin: interner.intern("Fallout4.esm"),
        };
        let weapons = index(&interner, &[("DEL_crMinigun", 0x01)]);
        let marked = vec![(own(&interner, 0x01), "del_crminigun".to_string())];

        let remap = build_twin_remap(&weapons, &marked, |stripped| {
            (stripped == "minigun").then_some(vanilla)
        });

        assert_eq!(remap.get(&own(&interner, 0x01)), Some(&vanilla));
    }

    #[test]
    fn carrying_a_twin_reaches_up_through_nested_lists() {
        let (mob, per_weapon, unrelated) = (0x0800_0020, 0x0800_0021, 0x0800_0022);
        let nested_lists = [(mob, vec![per_weapon]), (unrelated, vec![0x0800_0099])]
            .into_iter()
            .collect();
        let mut carries_twin: FxHashSet<u32> = [per_weapon].into_iter().collect();

        propagate_nested_lists(&nested_lists, &mut carries_twin);

        assert!(carries_twin.contains(&mob));
        assert!(!carries_twin.contains(&unrelated));
    }

    #[test]
    fn clears_the_no_loot_bit_in_the_acbs_blob() {
        let interner = StringInterner::new();
        let mut record = npc_with_acbs_flags(&interner, 0x1018);

        assert!(clear_no_loot(&mut record, &fo4_schema()));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw ACBS bytes");
        };
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0x0018);
    }

    #[test]
    fn leaves_an_actor_without_the_flag_untouched() {
        let interner = StringInterner::new();
        let mut record = npc_with_acbs_flags(&interner, 0x0018);

        assert!(!clear_no_loot(&mut record, &fo4_schema()));
    }

    /// The defect this pass sat on for three regens: `LVLO` is a `struct:` codec,
    /// so its FormIDs are only reachable through the blob.
    #[test]
    fn repoints_a_leveled_list_entry_inside_the_struct_blob() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let (creature, twin, unrelated) = (0x087A_C763, 0x0812_DBB3, 0x0801_0000);
        let mut record = lvli_with_entries(&interner, &[creature, unrelated]);
        let encoded_remap: FxHashMap<u32, u32> = [(creature, twin)].into_iter().collect();

        assert_eq!(
            read_struct_form_ids(&record, "LVLO", &schema),
            vec![creature, unrelated]
        );

        let FieldValue::Bytes(bytes) = &mut record.fields[0].value else {
            panic!("expected raw LVLO bytes");
        };
        assert!(rewrite_struct_form_ids(
            bytes,
            "LVLI",
            "LVLO",
            &encoded_remap,
            &schema
        ));

        assert_eq!(
            read_struct_form_ids(&record, "LVLO", &schema),
            vec![twin, unrelated]
        );
    }

    #[test]
    fn leaves_a_leveled_list_holding_nothing_remappable_alone() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let mut record = lvli_with_entries(&interner, &[0x0801_0000]);
        let encoded_remap: FxHashMap<u32, u32> = [(0x087A_C763, 0x0812_DBB3)].into_iter().collect();

        let FieldValue::Bytes(bytes) = &mut record.fields[0].value else {
            panic!("expected raw LVLO bytes");
        };
        assert!(!rewrite_struct_form_ids(
            bytes,
            "LVLI",
            "LVLO",
            &encoded_remap,
            &schema
        ));
    }

    /// A blob that does not divide into the schema stride is left alone rather
    /// than smeared: a wrong offset here silently corrupts neighbouring fields.
    #[test]
    fn refuses_a_blob_that_does_not_match_the_layout_stride() {
        let schema = fo4_schema();
        let mut bytes = [0u8; 7];
        let encoded_remap: FxHashMap<u32, u32> = [(0x087A_C763, 0x0812_DBB3)].into_iter().collect();

        assert!(!rewrite_struct_form_ids(
            &mut bytes,
            "LVLI",
            "LVLO",
            &encoded_remap,
            &schema
        ));
    }
}
