//! Creature-specific fixups for the FO76→FO4 conversion pipeline.
pub mod augment_creature_factions;
pub mod clean_creature_esp_check_fields;
pub mod cleanup_bodypart_data;
pub mod creature_predicate;
pub mod fix_creature_npc_records;
pub mod fix_creature_race_records;
pub mod fix_creature_weapon_fire_seconds;
pub mod fix_creature_weapons_and_records;
pub mod normalize_creature_lvln_template_chains;
pub mod nullify_creature_death_items;
pub mod strip_creature_subgraph_additive_race;
pub mod synthesize_weapon_innr;

use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::ReadView;
use crate::sym::StringInterner;
use creature_predicate::{
    CreatureVerdict, npc_is_creature_following_template, record_has_actor_type_npc,
};

/// Byte offset of the reference FormID inside a raw FO4 `LVLO` payload.
const LVLO_REFERENCE_OFFSET: usize = 4;

/// Whole-plugin per-record gate for the NPC-internal creature fixups
/// (`fix_creature_npc_records`, `augment_creature_factions`,
/// `nullify_creature_death_items`).
///
/// These run unconditionally per-NPC on a creature-rooted bounded graph
/// (where every record is a creature by construction). On the whole-plugin path
/// they must touch only actual creatures, or they would stamp creature
/// keywords / perks / factions / death-item strips onto every HUMAN NPC. This
/// resolves the NPC's race through the template chain (UseTraits) so a
/// Traits-template creature whose literal `RNAM` is irrelevant still classifies
/// correctly.
///
/// Resolution is conservative: an NPC we cannot confidently classify as a
/// creature returns `false` (Unknown / NotCreature both skip). Resolution reads
/// only the TARGET plugin via `view.record_decoded`; a creature whose race was
/// dropped (no FO4 equivalent) therefore reads `Unknown` and is skipped — that
/// is SAFE (never mis-flags a human). Once its race resolves, its RNAM/TPLT
/// classify it as a creature here.
pub fn npc_internal_fixup_applies_to_record(
    npc: &Record,
    view: &ReadView,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    config: &crate::fixups::FixupConfig,
) -> bool {
    // On a creature-rooted graph walk every NPC in scope is a creature by
    // construction — applying the predicate there would REGRESS bounded graph
    // runs (a dropped-race creature reads Unknown and would be wrongly skipped).
    if !config.is_whole_plugin {
        return true;
    }
    npc_is_creature_in_view(npc, view, schema, interner)
}

/// Confident creature classification for an NPC, resolving its race through the
/// target plugin (own keyword → template chain → RNAM race). Conservative:
/// Unknown / NotCreature → `false`.
pub fn npc_is_creature_in_view(
    npc: &Record,
    view: &ReadView,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> bool {
    let resolve =
        |fk: FormKey| -> Option<Record> { view.record_decoded(&fk, schema, interner).ok() };
    let lvln_entry_npcs = |fk: FormKey| -> Vec<FormKey> {
        let Some(lvln) = view.record_decoded(&fk, schema, interner).ok() else {
            return Vec::new();
        };
        lvln_entry_form_keys(&lvln, interner)
    };
    matches!(
        npc_is_creature_following_template(npc, &resolve, &lvln_entry_npcs),
        CreatureVerdict::Creature
    )
}

/// Collect the NPC_ FormKeys a LVLN references via its `LVLO` entries. The
/// session can expose LVLO as either its raw FO4 payload or a decoded struct.
fn lvln_entry_form_keys(lvln: &Record, interner: &StringInterner) -> Vec<FormKey> {
    let Ok(lvlo_sig) = SubrecordSig::from_str("LVLO") else {
        return Vec::new();
    };
    let plugin = lvln.form_key.plugin;
    let mut out = Vec::new();
    for entry in &lvln.fields {
        if entry.sig != lvlo_sig {
            continue;
        }
        if let Some(fk) = lvln_entry_form_key(&entry.value, plugin, interner) {
            out.push(fk);
        }
    }
    out
}

fn lvln_entry_form_key(
    value: &FieldValue,
    plugin: crate::sym::Sym,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(data) if data.len() >= LVLO_REFERENCE_OFFSET + 4 => {
            let raw = u32::from_le_bytes(
                data[LVLO_REFERENCE_OFFSET..LVLO_REFERENCE_OFFSET + 4]
                    .try_into()
                    .ok()?,
            );
            (raw != 0).then_some(FormKey {
                local: raw & 0x00FF_FFFF,
                plugin,
            })
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            let is_entry = interner.resolve(*name).is_some_and(|name| {
                name.eq_ignore_ascii_case("npc") || name.eq_ignore_ascii_case("reference")
            });
            is_entry.then(|| scalar_form_key(value, plugin)).flatten()
        }),
        FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
        _ => None,
    }
}

fn scalar_form_key(value: &FieldValue, plugin: crate::sym::Sym) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
        FieldValue::Uint(raw) if *raw > 0 && *raw <= u32::MAX as u64 => Some(FormKey {
            local: *raw as u32 & 0x00FF_FFFF,
            plugin,
        }),
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => Some(FormKey {
            local: *raw as u32 & 0x00FF_FFFF,
            plugin,
        }),
        FieldValue::Bytes(data) if data.len() >= 4 => {
            let local = u32::from_le_bytes(data[0..4].try_into().ok()?) & 0x00FF_FFFF;
            (local != 0).then_some(FormKey { local, plugin })
        }
        _ => None,
    }
}

/// Per-RACE gate for the RACE-internal creature fixups
/// (`fix_creature_race_records`, `strip_creature_subgraph_additive_race`).
///
/// On a creature-rooted graph walk (`is_whole_plugin == false`) every RACE in
/// scope is creature-relevant by construction, so the fixup runs unconditionally
/// — gating there would REGRESS the bounded graph path (which fixed every race in
/// the graph). On whole-plugin we must avoid touching humanoid races
/// (HumanRace-family, ghoul/fisherman/armor-rack player-races, …).
///
/// The gate is "run unless the race is a confirmed HUMANOID" (carries
/// `ActorTypeNPC`), rather than "run only if it carries `ActorTypeCreature`":
/// ground-truth on the output ESM shows several creature-class races — robots
/// (Protectron/Turret/Liberator/Vertibot…) and segmented creatures
/// (Scorchtongue body/head/tail) — that legitimately lack `ActorTypeCreature`.
/// Those still need the FO76 ATKD/skeletal/behavior/subgraph fixes; only the
/// `ActorTypeNPC` humanoids must be protected.
pub fn race_internal_fixup_applies_to_record(
    record: &Record,
    config: &crate::fixups::FixupConfig,
    interner: &StringInterner,
) -> bool {
    if !config.is_whole_plugin {
        return true;
    }
    if race_is_generated_additive(record, interner) {
        return false;
    }
    !record_has_actor_type_npc(record)
}

fn race_is_generated_additive(record: &Record, interner: &StringInterner) -> bool {
    record_editor_id(record, interner)
        .map(|eid| eid.contains("RaceAdditive"))
        .unwrap_or(false)
}

fn record_editor_id<'a>(record: &'a Record, interner: &'a StringInterner) -> Option<&'a str> {
    if let Some(eid) = record.eid.and_then(|sym| interner.resolve(sym)) {
        return Some(eid);
    }

    let edid_sig = SubrecordSig::from_str("EDID").ok()?;
    record
        .fields
        .iter()
        .find(|entry| entry.sig == edid_sig)
        .and_then(|entry| match entry.value {
            FieldValue::String(sym) => interner.resolve(sym),
            _ => None,
        })
}

/// Whether a fixup that self-gates on the creature predicate should run for the
/// given config: always on a creature-rooted graph walk (every record is a
/// creature), and on whole-plugin (where it self-gates per record).
pub fn creature_internal_fixup_applies(config: &crate::fixups::FixupConfig) -> bool {
    config.is_whole_plugin
        || config
            .root_sig
            .map(crate::fixups::prune_orphaned_records::is_creature_root_sig)
            .unwrap_or(false)
}

pub fn likely_creature_weapon_editor_id(eid_lower: &str) -> bool {
    eid_lower.starts_with("cr")
        || eid_lower.contains("crrobot")
        || eid_lower.contains("creature")
        || eid_lower.contains("unarmed")
        || likely_ranged_creature_weapon_editor_id(eid_lower)
        || eid_lower.contains("bite")
        || eid_lower.contains("claw")
}

pub fn likely_ranged_creature_weapon_editor_id(eid_lower: &str) -> bool {
    eid_lower.contains("spit")
        || eid_lower.contains("barf")
        || eid_lower.contains("breath")
        || eid_lower.contains("fireball")
        || eid_lower.contains("stare")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::FixupConfig;
    use crate::ids::SigCode;
    use crate::record::{FieldEntry, RecordFlags};

    #[test]
    fn recognizes_prefixed_robot_creature_weapons() {
        assert!(likely_creature_weapon_editor_id(
            "hto_crrobot_liberator_lasergun"
        ));
    }

    fn lvln(local: u32, plugin: &str, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("LVLN").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn npc(local: u32, plugin: &str, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push_bytes(record: &mut Record, sig: &str, data: &[u8]) {
        let mut buf: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        buf.extend_from_slice(data);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(buf),
        });
    }

    fn race_with_editor_id(
        local: u32,
        plugin: &str,
        editor_id: &str,
        interner: &StringInterner,
    ) -> Record {
        let eid = interner.intern(editor_id);
        let mut record = Record {
            sig: SigCode::from_str("RACE").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: Some(eid),
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        };
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid),
        });
        record
    }

    #[test]
    fn lvln_entry_form_keys_reads_reference_at_offset_4_with_lvln_plugin() {
        let interner = StringInterner::new();
        let mut rec = lvln(0x000900, "Output.esm", &interner);
        // FO4 LVLO raw payload: level(u16) + 2 pad + reference(u32) + count(u16)...
        let mut e1 = vec![0u8; 4];
        e1.extend_from_slice(&0x07_200001u32.to_le_bytes());
        e1.extend_from_slice(&1u16.to_le_bytes());
        push_bytes(&mut rec, "LVLO", &e1);
        let mut e2 = vec![0u8; 4];
        e2.extend_from_slice(&0x07_200002u32.to_le_bytes());
        e2.extend_from_slice(&1u16.to_le_bytes());
        push_bytes(&mut rec, "LVLO", &e2);

        let fks = lvln_entry_form_keys(&rec, &interner);
        assert_eq!(fks.len(), 2);
        // Reference object-id is low-24; plugin is the LVLN's own plugin.
        assert_eq!(fks[0].local, 0x200001);
        assert_eq!(fks[1].local, 0x200002);
        assert_eq!(fks[0].plugin, rec.form_key.plugin);
    }

    #[test]
    fn lvln_entry_form_keys_reads_structured_npc_field() {
        let interner = StringInterner::new();
        let mut rec = lvln(0x000900, "Output.esm", &interner);
        let entry_fk = FormKey {
            local: 0x200001,
            plugin: rec.form_key.plugin,
        };
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("level"), FieldValue::Uint(1)),
                (interner.intern("npc"), FieldValue::FormKey(entry_fk)),
                (interner.intern("count"), FieldValue::Uint(1)),
            ]),
        });

        assert_eq!(lvln_entry_form_keys(&rec, &interner), vec![entry_fk]);
    }

    #[test]
    fn structured_lvln_entry_keeps_template_creature_gate_active() {
        let interner = StringInterner::new();
        let plugin = "Output.esm";
        let lvln_fk = FormKey {
            local: 0x000900,
            plugin: interner.intern(plugin),
        };
        let entry_fk = FormKey {
            local: 0x200001,
            plugin: lvln_fk.plugin,
        };
        let race_fk = FormKey {
            local: 0x10CA5F,
            plugin: lvln_fk.plugin,
        };

        let mut root = npc(0x31B1FF, plugin, &interner);
        let mut acbs = [0u8; 20];
        acbs[14..16].copy_from_slice(&1u16.to_le_bytes());
        push_bytes(&mut root, "ACBS", &acbs);
        root.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPLT").unwrap(),
            value: FieldValue::FormKey(lvln_fk),
        });

        let mut list = lvln(lvln_fk.local, plugin, &interner);
        list.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("level"), FieldValue::Uint(1)),
                (interner.intern("npc"), FieldValue::FormKey(entry_fk)),
                (interner.intern("count"), FieldValue::Uint(1)),
            ]),
        });

        let mut entry = npc(entry_fk.local, plugin, &interner);
        entry.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RNAM").unwrap(),
            value: FieldValue::FormKey(race_fk),
        });
        let mut race = race_with_editor_id(race_fk.local, plugin, "ScorchedRace", &interner);
        push_bytes(&mut race, "KWDA", &0x0001_3795u32.to_le_bytes());

        let resolve = |fk: FormKey| {
            if fk == lvln_fk {
                Some(list.clone())
            } else if fk == entry_fk {
                Some(entry.clone())
            } else if fk == race_fk {
                Some(race.clone())
            } else {
                None
            }
        };
        let entries = |fk: FormKey| {
            (fk == lvln_fk)
                .then(|| lvln_entry_form_keys(&list, &interner))
                .unwrap_or_default()
        };

        assert_eq!(
            npc_is_creature_following_template(&root, &resolve, &entries),
            CreatureVerdict::Creature
        );
    }

    #[test]
    fn creature_internal_applies_on_whole_plugin_and_creature_root_only() {
        let whole = FixupConfig {
            is_whole_plugin: true,
            ..Default::default()
        };
        assert!(creature_internal_fixup_applies(&whole));

        let npc_root = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            ..Default::default()
        };
        assert!(creature_internal_fixup_applies(&npc_root));

        let lvln_root = FixupConfig {
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
            ..Default::default()
        };
        assert!(creature_internal_fixup_applies(&lvln_root));

        // Non-creature graph root, not whole-plugin → does not apply.
        let weap_root = FixupConfig {
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        assert!(!creature_internal_fixup_applies(&weap_root));

        let no_root = FixupConfig::default();
        assert!(!creature_internal_fixup_applies(&no_root));
    }

    #[test]
    fn race_internal_skips_generated_additive_races_in_whole_plugin() {
        let interner = StringInterner::new();
        let config = FixupConfig {
            is_whole_plugin: true,
            ..Default::default()
        };
        let record = race_with_editor_id(
            0x000100,
            "Output.esp",
            "SuperMutantRaceAdditiveMinigun",
            &interner,
        );

        assert!(!race_internal_fixup_applies_to_record(
            &record, &config, &interner
        ));
    }

    #[test]
    fn race_internal_still_runs_for_non_additive_creature_races_in_whole_plugin() {
        let interner = StringInterner::new();
        let config = FixupConfig {
            is_whole_plugin: true,
            ..Default::default()
        };
        let record = race_with_editor_id(0x000100, "Output.esp", "SuperMutantRace", &interner);

        assert!(race_internal_fixup_applies_to_record(
            &record, &config, &interner
        ));
    }
}
