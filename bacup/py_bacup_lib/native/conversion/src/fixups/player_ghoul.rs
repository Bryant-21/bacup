use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

pub struct PlayerGhoulFixup;

const SHARED_ABILITIES: &[&str] = &[
    "GHL_ActionGhoul",
    "GHL_BattleGenes",
    "GHL_BreatheItIn",
    "GHL_GunTricks",
    "GHL_JaguarSpeed",
];

fn shared_ability_root(eid: &str) -> Option<String> {
    SHARED_ABILITIES
        .iter()
        .find(|family| {
            eid.strip_prefix(**family)
                .is_some_and(|rank| matches!(rank, "01" | "02" | "03"))
        })
        .map(|family| format!("{family}01"))
}

fn ability_entries(root: &Record) -> Vec<FieldEntry> {
    let mut ability = false;
    let mut result = Vec::new();
    for field in &root.fields {
        if field.sig.0 == *b"PRKE" {
            ability = match &field.value {
                FieldValue::Bytes(bytes) => bytes.first() == Some(&1),
                _ => false,
            };
        }
        if ability {
            result.push(field.clone());
        }
        if field.sig.0 == *b"PRKF" {
            ability = false;
        }
    }
    result
}

fn error(value: impl std::fmt::Display) -> FixupError {
    FixupError::Other(value.to_string())
}
fn raw(sig: &[u8; 4], data: impl Into<Vec<u8>>) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value: FieldValue::Bytes(data.into().into()),
    }
}

fn limb_damage_entries(
    fields: &[([u8; 4], Vec<u8>)],
    rank: usize,
) -> Result<Vec<FieldEntry>, FixupError> {
    let effects = fields
        .split(|(sig, _)| sig == b"EFID")
        .skip(1)
        .collect::<Vec<_>>();
    if effects.len() != 3 || rank >= effects.len() {
        return Err(error("Bone Shatterer effects changed"));
    }
    let effect = effects[rank];
    let magnitude = effect
        .iter()
        .find(|(sig, _)| sig == b"EFIT")
        .filter(|(_, bytes)| bytes.len() == 12)
        .map(|(_, bytes)| f32::from_le_bytes(bytes[..4].try_into().unwrap()))
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| error("Bone Shatterer magnitude missing"))?;
    // FO4 Rifleman05 uses this outgoing-limb entrypoint/function/tab layout.
    // The source STAT_DmgLimbs ability has no corresponding FO4 stat consumer.
    let mut entries = vec![
        raw(b"PRKE", [2, 0, 0]),
        raw(b"DATA", [0x73, 3, 3]),
        raw(b"PRKC", [0]),
    ];
    let mut functions = Vec::new();
    for (sig, bytes) in effect {
        if sig != b"CTDA" {
            continue;
        }
        let function = bytes
            .get(8..10)
            .map(|id| u16::from_le_bytes([id[0], id[1]]));
        if bytes.len() != 32 || !matches!(function, Some(682 | 560 | 448)) {
            return Err(error("Bone Shatterer condition changed"));
        }
        functions.extend(function);
        entries.push(raw(b"CTDA", bytes.clone()));
    }
    // Melee weapon, ghoul identity and card rank must all survive; losing any one
    // widens the damage bonus beyond the source gate.
    if ![682, 560, 448].iter().all(|id| functions.contains(id)) {
        return Err(error("Bone Shatterer conditions missing"));
    }
    entries.extend([
        raw(b"EPFT", [1]),
        raw(b"EPFD", (1.0 + magnitude / 100.0).to_le_bytes()),
        raw(b"PRKF", []),
    ]);
    Ok(entries)
}

fn replace_form(value: &mut FieldValue, old: FormKey, new: FormKey) {
    match value {
        FieldValue::FormKey(key) if *key == old => *key = new,
        FieldValue::List(values) => {
            for value in values {
                replace_form(value, old, new);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                replace_form(value, old, new);
            }
        }
        _ => {}
    }
}

fn route_glow(record: &mut Record, rads: FormKey, glow: FormKey, rads_raw: u32, glow_raw: u32) {
    for field in &mut record.fields {
        // Only condition reads are redirected. The controller owns Glow writes;
        // source spells that spend rads are not silently made FO4 radiation spells.
        if field.sig.0 != *b"CTDA" {
            continue;
        }
        replace_form(&mut field.value, rads, glow);
        if let FieldValue::Bytes(bytes) = &mut field.value {
            if bytes.len() == 32
                && u16::from_le_bytes([bytes[8], bytes[9]]) == 14
                && u32::from_le_bytes(bytes[12..16].try_into().unwrap()) == rads_raw
            {
                bytes[12..16].copy_from_slice(&glow_raw.to_le_bytes());
            }
        }
    }
}

fn normalize_rank(record: &mut Record, interner: &crate::sym::StringInterner) {
    for field in &mut record.fields {
        if field.sig.0 != *b"PRKE" {
            continue;
        }
        match &mut field.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 2 => bytes[1] = 0,
            FieldValue::Struct(fields) => {
                for (name, value) in fields {
                    if interner
                        .resolve(*name)
                        .is_some_and(|name| name.eq_ignore_ascii_case("rank"))
                    {
                        *value = FieldValue::Uint(0);
                    }
                }
            }
            _ => {}
        }
    }
}

impl Fixup for PlayerGhoulFixup {
    fn name(&self) -> &'static str {
        "player_ghoul"
    }
    fn uses_session(&self) -> bool {
        true
    }
    fn applies_to_session(&self, session: &PluginSession, _: &FixupConfig) -> bool {
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
        _: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let schema = session.schema().map_err(error)?;
        let source_plugin = session
            .source_slot_opt()
            .ok_or_else(|| error("ghoul source missing"))?
            .parsed
            .plugin_name
            .clone();
        let source_id = session.source_id().unwrap();
        let candidates = {
            let scan = session.handle_raw_scan(source_id).map_err(error)?;
            scan.own_editor_ids()
                .iter()
                .filter(|(_, eid)| eid.starts_with("GHL_"))
                .filter_map(|(id, eid)| {
                    mapper
                        .lookup(FormKey {
                            plugin: mapper.interner.intern(&source_plugin),
                            local: *id,
                        })
                        .map(|key| (eid.clone(), key))
                })
                .collect::<Vec<_>>()
        };
        let Some((_, feral)) = candidates.iter().find(|(eid, _)| eid == "GHL_SURV_Feral") else {
            return Ok(report);
        };
        let mut glow = session
            .record_decoded(feral, &schema, mapper.interner)
            .map_err(error)?;
        let eid = "B21_TFA_GhoulGlow";
        let glow_key = mapper.allocate_or_resolve(
            FormKey {
                plugin: mapper.interner.intern(eid),
                local: 1,
            },
            None,
            SigCode(*b"AVIF"),
        );
        glow.form_key = glow_key;
        glow.eid = Some(mapper.interner.intern(eid));
        glow.fields
            .retain(|field| !matches!(&field.sig.0, b"EDID" | b"FULL" | b"DESC" | b"ANAM"));
        glow.fields.push(FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(glow.eid.unwrap()),
        });
        glow.fields.push(FieldEntry {
            sig: SubrecordSig(*b"FULL"),
            value: FieldValue::String(mapper.interner.intern("Glow")),
        });
        let races_eid = "B21_TFA_GhoulAppearanceRaces";
        let races_key = mapper.allocate_or_resolve(
            FormKey {
                plugin: mapper.interner.intern(races_eid),
                local: 1,
            },
            None,
            SigCode(*b"FLST"),
        );
        let human = FormKey {
            plugin: mapper.interner.intern("Fallout4.esm"),
            local: 0x013746,
        };
        let mut races = Record::new(SigCode(*b"FLST"), races_key);
        races.eid = Some(mapper.interner.intern(races_eid));
        races.fields.push(FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(races.eid.unwrap()),
        });
        for local in [0x013746, 0x0EAFB6] {
            races.fields.push(FieldEntry {
                sig: SubrecordSig(*b"LNAM"),
                value: FieldValue::FormKey(FormKey {
                    plugin: human.plugin,
                    local,
                }),
            });
        }
        for record in [glow, races] {
            if session
                .record_decoded(&record.form_key, &schema, mapper.interner)
                .is_ok()
            {
                session
                    .replace_records_contents(vec![record], &schema, mapper.interner)
                    .map_err(error)?;
                report.records_changed += 1;
            } else {
                session
                    .add_record(record, &schema, mapper.interner)
                    .map_err(error)?;
                report.records_added += 1;
            }
        }
        let rads = FormKey {
            plugin: human.plugin,
            local: 0x0002E1,
        };
        let masters = session.target_masters().to_vec();
        let rads_raw = encode_target_form_id(rads, mapper.interner, &masters)
            .ok_or_else(|| error("ghoul rads binding"))?;
        let glow_raw = encode_target_form_id(glow_key, mapper.interner, &masters)
            .ok_or_else(|| error("ghoul Glow binding"))?;
        let mut changed = Vec::new();
        for (eid, key) in &candidates {
            let Ok(mut record) = session.record_decoded(&key, &schema, mapper.interner) else {
                continue;
            };
            if !matches!(
                &record.sig.0,
                b"PERK" | b"SPEL" | b"MGEF" | b"ARMA" | b"HDPT"
            ) {
                continue;
            }
            route_glow(&mut record, rads, glow_key, rads_raw, glow_raw);
            if record.sig.0 == *b"PERK" {
                if let Some(rank) = eid
                    .strip_prefix("GHL_BoneShatterer")
                    .and_then(|rank| rank.parse::<usize>().ok())
                    .filter(|rank| (1..=3).contains(rank))
                {
                    let spell_source = session
                        .handle_raw_scan(source_id)
                        .map_err(error)?
                        .own_editor_ids()
                        .iter()
                        .find(|(_, eid)| eid.as_str() == "Perk_BoneShatterer_LimbDamageSpell")
                        .map(|(id, _)| *id)
                        .ok_or_else(|| error("Bone Shatterer source ability missing"))?;
                    let spell = mapper
                        .lookup(FormKey {
                            plugin: mapper.interner.intern(&source_plugin),
                            local: spell_source,
                        })
                        .and_then(|key| encode_target_form_id(key, mapper.interner, &masters))
                        .ok_or_else(|| error("Bone Shatterer converted ability missing"))?;
                    let target_id = session.target_id();
                    let fields = session
                        .handle_raw_scan(target_id)
                        .map_err(error)?
                        .with_record_subrecords(spell, |fields| {
                            fields
                                .iter()
                                .map(|field| {
                                    (
                                        field.signature.as_bytes().try_into().unwrap(),
                                        field.data.to_vec(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .ok_or_else(|| error("Bone Shatterer ability fields missing"))?;
                    let entries = limb_damage_entries(&fields, rank - 1)?;
                    record.fields = record
                        .fields
                        .into_iter()
                        .take_while(|field| field.sig.0 != *b"PRKE")
                        .collect();
                    record.fields.extend(entries);
                }
                if let Some(root_eid) = shared_ability_root(eid) {
                    let root_key = candidates
                        .iter()
                        .find(|(eid, _)| *eid == root_eid)
                        .ok_or_else(|| error(format!("Missing ghoul ability root {root_eid}")))?
                        .1;
                    // FO76 keeps the shared ability only on rank 1. FO4 card ranks
                    // are independent perks, so each must own that same ability.
                    let root_raw = encode_target_form_id(root_key, mapper.interner, &masters)
                        .ok_or_else(|| error("ghoul ability root binding"))?;
                    let target_id = session.target_id();
                    let entries = session
                        .handle_raw_scan(target_id)
                        .map_err(error)?
                        .with_record_subrecords(root_raw, |fields| {
                            fields
                                .iter()
                                .map(|field| {
                                    (
                                        field.signature.as_bytes().try_into().unwrap(),
                                        field.data.to_vec(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .ok_or_else(|| error("ghoul ability root missing"))?;
                    let mut root = Record::new(SigCode(*b"PERK"), root_key);
                    root.fields = entries
                        .iter()
                        .map(|(sig, bytes)| raw(sig, bytes.clone()))
                        .collect();
                    let entries = ability_entries(&root);
                    if entries.is_empty() {
                        return Err(error(format!("Missing ability entry on {root_eid}")));
                    }
                    record.fields = record
                        .fields
                        .into_iter()
                        .take_while(|field| field.sig.0 != *b"PRKE")
                        .collect();
                    record.fields.extend(entries);
                }
                normalize_rank(&mut record, mapper.interner);
            }
            if record.sig.0 == *b"ARMA"
                && matches!(eid.as_str(), "GHL_NakedGhoulHands" | "GHL_NakedGhoulTorso")
                && !record
                    .fields
                    .iter()
                    .any(|f| f.sig.0 == *b"MODL" && f.value == FieldValue::FormKey(human))
            {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"MODL"),
                    value: FieldValue::FormKey(human),
                });
            }
            if record.sig.0 == *b"HDPT" {
                record.fields.retain(|f| f.sig.0 != *b"RNAM");
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"RNAM"),
                    value: FieldValue::FormKey(races_key),
                });
            }
            changed.push(record);
        }
        report.records_changed += changed.len() as u32;
        session
            .replace_records_contents(changed, &schema, mapper.interner)
            .map_err(error)?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bone_shatterer_uses_source_magnitudes_and_preserves_rank_weapon_conditions() {
        let mut fields = Vec::new();
        for magnitude in [25.0f32, 50.0, 75.0] {
            let mut data = magnitude.to_le_bytes().to_vec();
            data.extend([0; 8]);
            fields.extend([
                (*b"EFID", 123u32.to_le_bytes().to_vec()),
                (*b"EFIT", data),
                (*b"CTDA", fo76_condition(682, 1.0, 0x01000444)),
                (*b"CTDA", fo76_condition(560, 1.0, 0x01000445)),
                (*b"CTDA", fo76_condition(448, 0.0, 0x01000446)),
            ]);
        }
        for rank in 0..3 {
            let entries = limb_damage_entries(&fields, rank).unwrap();
            assert_eq!(entries[1].value, raw(b"DATA", [0x73, 3, 3]).value);
            assert_eq!(entries[3].value, raw(b"CTDA", fields[2].1.clone()).value);
            assert_eq!(
                entries[7].value,
                raw(b"EPFD", (1.25 + 0.25 * rank as f32).to_le_bytes()).value
            );
        }
        let mut without_identity = fields.clone();
        without_identity.remove(3);
        assert!(matches!(limb_damage_entries(&without_identity, 0),
            Err(FixupError::Other(message)) if message == "Bone Shatterer conditions missing"));
        fields[3].1[8..10].copy_from_slice(&10017u16.to_le_bytes());
        assert!(limb_damage_entries(&fields, 0).is_err());
        assert!(limb_damage_entries(&fields, 3).is_err());
    }

    fn fo76_condition(function: u16, comparison: f32, parameter_1: u32) -> Vec<u8> {
        let mut bytes = vec![0; 32];
        bytes[4..8].copy_from_slice(&comparison.to_le_bytes());
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[28..32].copy_from_slice(&(-1i32).to_le_bytes());
        bytes
    }

    // Regen abort 2026-09-23: the SPEL translation map dropped every CTDA, so the
    // fixup saw effects without conditions. Drive the installed record's shape
    // through the real translators instead of hand-built target bytes.
    #[test]
    fn translated_bone_shatterer_spell_keeps_source_conditions_for_every_rank() {
        use crate::run::{RunConfig, RunParams, create_run, drop_run, with_run};
        use crate::store2::source::test_fixture::{group, plugin, record, subrecord};
        use crate::translator::Game;
        use esp_authoring_core::plugin_runtime::{
            effective_subrecords_for_record, parse_plugin_file, plugin_handle_close_native,
            plugin_handle_new_native, plugin_handle_store_ref,
        };

        const MELEE: u32 = 0x33AB12;
        const GHOUL: u32 = 0x79CCE5;
        const RANK2: u32 = 0x797E32;
        const RANK3: u32 = 0x797E3E;
        let conditions = |fields: &[([u8; 4], Vec<u8>)]| {
            fields
                .iter()
                .filter(|(sig, _)| sig == b"CTDA")
                .map(|(_, bytes)| {
                    (
                        u16::from_le_bytes([bytes[8], bytes[9]]),
                        u32::from_le_bytes(bytes[12..16].try_into().unwrap()) & 0x00FF_FFFF,
                    )
                })
                .collect::<Vec<_>>()
        };
        let edid = |eid: &str| subrecord(b"EDID", format!("{eid}\0").as_bytes());
        let keywords = [
            (MELEE, "WeaponTypeMeleeGeneral"),
            (GHOUL, "GHL_ActorTypePlayerGhoul"),
            (0x868BD1, "GHL_SpellKeyword"),
        ]
        .map(|(id, eid)| record(b"KYWD", id, 0, &edid(eid)));
        let perks = [
            (0x797E47, "GHL_BoneShatterer01"),
            (RANK2, "GHL_BoneShatterer02"),
            (RANK3, "GHL_BoneShatterer03"),
        ]
        .map(|(id, eid)| record(b"PERK", id, 0, &edid(eid)));
        let feral = record(b"AVIF", 0x7A0000, 0, &edid("GHL_SURV_Feral"));
        let effect = record(
            b"MGEF",
            0x820D75,
            0,
            &edid("Perk_BoneShatterer_LimbDamageEffect"),
        );
        let mut payload = edid("Perk_BoneShatterer_LimbDamageSpell");
        let mut spit = vec![0; 36];
        spit[12..16].copy_from_slice(&4u32.to_le_bytes());
        payload.extend(subrecord(b"SPIT", &spit));
        let ranks: [(f32, &[(u16, f32, u32)]); 3] = [
            (
                25.0,
                &[(682, 1.0, MELEE), (10017, 1.0, 0), (448, 0.0, RANK2)],
            ),
            (
                50.0,
                &[
                    (682, 1.0, MELEE),
                    (10017, 1.0, 0),
                    (448, 1.0, RANK2),
                    (448, 0.0, RANK3),
                ],
            ),
            (
                75.0,
                &[(682, 1.0, MELEE), (10017, 1.0, 0), (448, 1.0, RANK3)],
            ),
        ];
        for (index, (magnitude, conditions)) in ranks.iter().enumerate() {
            let mut efit = (3 + index as u32).to_le_bytes().to_vec();
            efit.extend(magnitude.to_le_bytes());
            efit.extend([0; 8]);
            payload.extend(subrecord(b"EFID", &0x820D75u32.to_le_bytes()));
            payload.extend(subrecord(b"EFIT", &efit));
            payload.extend(subrecord(b"MAGF", &[0; 4]));
            for (function, comparison, parameter) in *conditions {
                payload.extend(subrecord(
                    b"CTDA",
                    &fo76_condition(*function, *comparison, *parameter),
                ));
            }
            payload.extend(subrecord(b"CODV", &[0; 4]));
        }
        payload.extend(subrecord(b"MIID", &5u32.to_le_bytes()));
        let mut spell = record(b"SPEL", 0x820D76, 0, &payload);
        spell[20..22].copy_from_slice(&209u16.to_le_bytes());
        let bytes = plugin(
            &[],
            &[
                group(b"KYWD", 0, &keywords),
                group(b"AVIF", 0, &[feral]),
                group(b"MGEF", 0, &[effect]),
                group(b"PERK", 0, &perks),
                group(b"SPEL", 0, &[spell]),
            ],
        );
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("SeventySix.esm");
        std::fs::write(&source_path, bytes).unwrap();

        for use_v2 in [false, true] {
            let parsed =
                parse_plugin_file(&source_path.to_string_lossy(), Some("fo76".into()), true)
                    .unwrap();
            let source = plugin_handle_new_native(&parsed.plugin_name, Some("fo76")).unwrap();
            {
                let mut store = plugin_handle_store_ref().lock().unwrap();
                let slot = store.get_mut(&source).unwrap();
                slot.parsed = parsed;
                slot.invalidate_sections();
            }
            let target = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
            let id = create_run(RunParams {
                source: Game::Fo76,
                target: Game::Fo4,
                source_handle_id: source,
                target_handle_id: target,
                master_handle_ids: vec![],
                config: RunConfig {
                    output_plugin_name: "Out.esm".into(),
                    ..Default::default()
                },
            })
            .unwrap();
            with_run(id, |run| {
                if use_v2 {
                    run.translate_all_v2(&source_path)
                } else {
                    run.translate_all()
                }
            })
            .unwrap();
            let spell = crate::store2::test_util::handle_records(target)
                .into_iter()
                .find(|record| record.signature == "SPEL")
                .expect("translated spell");
            let translated = effective_subrecords_for_record(&spell)
                .iter()
                .map(|field| {
                    (
                        field.signature.as_bytes().try_into().unwrap(),
                        field.data.to_vec(),
                    )
                })
                .collect::<Vec<([u8; 4], Vec<u8>)>>();
            // Translation keeps source-local raw parameters; the raw FormID fixup
            // remaps them before PlayerGhoulFixup reads the spell.
            assert_eq!(
                conditions(&translated),
                [
                    (682, MELEE),
                    (560, GHOUL),
                    (448, RANK2),
                    (682, MELEE),
                    (560, GHOUL),
                    (448, RANK2),
                    (448, RANK3),
                    (682, MELEE),
                    (560, GHOUL),
                    (448, RANK3),
                ],
                "v2={use_v2}"
            );
            with_run(id, |run| {
                run.apply_fixups_v2().map_err(crate::run::RunError::from)
            })
            .unwrap();
            let mapped = |local: u32| {
                with_run(id, |run| {
                    Ok::<_, crate::run::RunError>(
                        run.mapper_state
                            .as_ref()
                            .unwrap()
                            .source_to_target
                            .iter()
                            .find(|(source, _)| source.local == local)
                            .map(|(_, target)| target.local),
                    )
                })
                .unwrap()
                .unwrap_or_else(|| panic!("{local:06X} unmapped, v2={use_v2}"))
            };
            let [melee, ghoul, rank2, rank3] = [MELEE, GHOUL, RANK2, RANK3].map(mapped);
            let expected: [Vec<(u16, u32)>; 3] = [
                vec![(682, melee), (560, ghoul), (448, rank2)],
                vec![(682, melee), (560, ghoul), (448, rank2), (448, rank3)],
                vec![(682, melee), (560, ghoul), (448, rank3)],
            ];
            let records = crate::store2::test_util::handle_records(target);
            for (rank, conditions_for_rank) in expected.iter().enumerate() {
                let eid = format!("GHL_BoneShatterer0{}\0", rank + 1);
                let perk = records
                    .iter()
                    .find(|record| {
                        record.signature == "PERK"
                            && effective_subrecords_for_record(record).iter().any(|field| {
                                field.signature == "EDID" && field.data == eid.as_bytes()
                            })
                    })
                    .unwrap_or_else(|| panic!("{eid} missing, v2={use_v2}"));
                let fields = effective_subrecords_for_record(perk)
                    .iter()
                    .map(|field| {
                        (
                            field.signature.as_bytes().try_into().unwrap(),
                            field.data.to_vec(),
                        )
                    })
                    .collect::<Vec<([u8; 4], Vec<u8>)>>();
                let entry = &fields[fields
                    .iter()
                    .position(|(sig, _)| sig == b"PRKE")
                    .expect("perk entry")..];
                let factor = (1.25f32 + 0.25 * rank as f32).to_le_bytes().to_vec();
                let mut layout = vec![
                    (*b"PRKE", vec![2, 0, 0]),
                    (*b"DATA", vec![0x73, 3, 3]),
                    (*b"PRKC", vec![0]),
                ];
                layout.extend(entry.iter().filter(|(sig, _)| sig == b"CTDA").cloned());
                layout.extend([(*b"EPFT", vec![1]), (*b"EPFD", factor), (*b"PRKF", vec![])]);
                assert_eq!(entry, layout.as_slice(), "rank {rank}, v2={use_v2}");
                assert_eq!(
                    &conditions(entry),
                    conditions_for_rank,
                    "rank {rank}, v2={use_v2}"
                );
            }
            drop_run(id).unwrap();
            assert!(plugin_handle_close_native(source));
            assert!(plugin_handle_close_native(target));
        }
    }
    #[test]
    fn session_generates_bindings_routes_glow_and_preserves_human_appearance_idempotently() {
        use crate::formkey_mapper::MapperOptions;
        use crate::session::open_session;
        use bytes::Bytes;
        use esp_authoring_core::plugin_runtime::{
            ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_close_native,
            plugin_handle_new_native, plugin_handle_store_ref,
        };
        let interner = crate::sym::StringInterner::new();
        let source_id = plugin_handle_new_native("Source.esm", Some("fo76")).unwrap();
        let target_id = plugin_handle_new_native("Output.esm", Some("fo4")).unwrap();
        let definitions = [
            ("AVIF", "GHL_SURV_Feral"),
            ("SPEL", "GHL_TestGlow"),
            ("ARMA", "GHL_NakedGhoulHands"),
            ("HDPT", "GHL_MaleHeadGhoul"),
            ("PERK", "GHL_ActionGhoul01"),
            ("PERK", "GHL_ActionGhoul02"),
            ("PERK", "GHL_ActionGhoul03"),
            ("PERK", "GHL_BoneShatterer01"),
            ("PERK", "GHL_BoneShatterer02"),
            ("PERK", "GHL_BoneShatterer03"),
            ("SPEL", "Perk_BoneShatterer_LimbDamageSpell"),
        ];
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source = store.get_mut(&source_id).unwrap();
            for (index, (sig, eid)) in definitions.iter().enumerate() {
                source
                    .parsed
                    .root_items
                    .push(ParsedItem::Record(ParsedRecord {
                        signature: (*sig).into(),
                        form_id: 0x800 + index as u32,
                        flags: 0,
                        version_control: 0,
                        form_version: Some(209),
                        version2: None,
                        raw_payload: None,
                        parse_error: None,
                        subrecords: vec![ParsedSubrecord {
                            signature: "EDID".into(),
                            data: Bytes::from(format!("{eid}\0")),
                            semantic_type: None,
                        }],
                    }));
            }
            source.invalidate_sections();
        }
        let mut session = open_session(target_id, Some(source_id)).unwrap();
        session
            .target_slot_mut()
            .parsed
            .header
            .masters
            .push("Fallout4.esm".into());
        let schema = session.schema().unwrap();
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                source_plugin_name: "Source.esm".into(),
                output_plugin_name: "Output.esm".into(),
                ..MapperOptions::default()
            },
            &interner,
        );
        let target = |index| FormKey {
            plugin: interner.intern("Output.esm"),
            local: 0x1800 + index,
        };
        for (index, (sig, _)) in definitions.iter().enumerate() {
            mapper.add_mapping(
                FormKey {
                    plugin: interner.intern("Source.esm"),
                    local: 0x800 + index as u32,
                },
                target(index as u32),
            );
            let mut record = Record::new(
                SigCode(sig.as_bytes().try_into().unwrap()),
                target(index as u32),
            );
            if *sig == "AVIF" {
                record.fields.push(raw(b"NAM0", 0.0f32.to_le_bytes()));
            }
            if *sig == "PERK" {
                record.fields.push(raw(b"DATA", [0, 0, 1, 0, 1]));
                if index == 4 {
                    record.fields.extend([
                        raw(b"PRKE", [1, 1, 0]),
                        raw(b"DATA", 0x01001801u32.to_le_bytes()),
                        raw(b"PRKF", []),
                    ]);
                }
            }
            if *sig == "SPEL" && index == 10 {
                for magnitude in [25.0f32, 50.0, 75.0] {
                    let mut efit = magnitude.to_le_bytes().to_vec();
                    efit.extend([0; 8]);
                    record.fields.extend([
                        raw(b"EFID", 0x01000445u32.to_le_bytes()),
                        raw(b"EFIT", efit),
                        raw(b"CTDA", fo76_condition(682, 1.0, 0x01000444)),
                        raw(b"CTDA", fo76_condition(560, 1.0, 0x01000446)),
                        raw(b"CTDA", fo76_condition(448, 0.0, 0x01000447)),
                    ]);
                }
            } else if *sig == "SPEL" {
                record.fields.push(raw(b"EFID", 0x1234u32.to_le_bytes()));
                record.fields.push(raw(b"EFIT", [0; 12]));
                let mut condition = vec![0; 32];
                condition[8..10].copy_from_slice(&14u16.to_le_bytes());
                condition[12..16].copy_from_slice(&0x2E1u32.to_le_bytes());
                record.fields.push(raw(b"CTDA", condition));
            }
            session.add_record(record, &schema, &interner).unwrap();
        }
        let config = FixupConfig::default();
        assert!(PlayerGhoulFixup.applies_to_session(&session, &config));
        let first = PlayerGhoulFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(first.records_added, 2);
        let glow = mapper
            .lookup(FormKey {
                plugin: interner.intern("B21_TFA_GhoulGlow"),
                local: 1,
            })
            .unwrap();
        let condition = session
            .first_subrecord_bytes(&target(1), "CTDA")
            .unwrap()
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(condition[12..16].try_into().unwrap()),
            0x01000000 | glow.local
        );
        let human = session
            .first_subrecord_bytes(&target(2), "MODL")
            .unwrap()
            .unwrap();
        assert_eq!(u32::from_le_bytes(human[..].try_into().unwrap()), 0x013746);
        assert!(
            session
                .first_subrecord_bytes(&target(3), "RNAM")
                .unwrap()
                .is_some()
        );
        for index in 4..=6 {
            assert_eq!(
                session
                    .first_subrecord_bytes(&target(index), "PRKE")
                    .unwrap()
                    .unwrap(),
                vec![1, 0, 0]
            );
        }
        for index in 7..=9 {
            assert_eq!(
                session
                    .first_subrecord_bytes(&target(index), "EPFD")
                    .unwrap()
                    .unwrap(),
                (1.25 + 0.25 * (index - 7) as f32).to_le_bytes()
            );
        }
        assert_eq!(
            PlayerGhoulFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap()
                .records_added,
            0
        );
        let hands = session
            .record_decoded(&target(2), &schema, &interner)
            .unwrap();
        assert_eq!(
            hands
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"MODL")
                .count(),
            1
        );
        drop(session);
        assert!(plugin_handle_close_native(source_id));
        assert!(plugin_handle_close_native(target_id));
    }
    #[test]
    fn glow_condition_keeps_comparison_target_and_other_actor_values() {
        let interner = crate::sym::StringInterner::new();
        let rads = FormKey {
            plugin: interner.intern("Fallout4.esm"),
            local: 0x2E1,
        };
        let glow = FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 0x800,
        };
        let mut record = Record::new(SigCode(*b"SPEL"), glow);
        let mut condition = vec![0; 32];
        condition[0] = 0x60;
        condition[4..8].copy_from_slice(&180.0f32.to_le_bytes());
        condition[8..10].copy_from_slice(&14u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x2E1u32.to_le_bytes());
        condition[20] = 1;
        record.fields.push(raw(b"CTDA", condition.clone()));
        record.fields.push(raw(b"DATA", condition.clone()));
        route_glow(&mut record, rads, glow, 0x2E1, 0x01000800);
        condition[12..16].copy_from_slice(&0x01000800u32.to_le_bytes());
        assert_eq!(record.fields[0].value, raw(b"CTDA", condition).value);
        let FieldValue::Bytes(bytes) = &record.fields[1].value else {
            panic!();
        };
        assert_eq!(&bytes[12..16], &0x2E1u32.to_le_bytes());
    }
    #[test]
    fn independent_card_ranks_are_zero_based_in_fo4() {
        let interner = crate::sym::StringInterner::new();
        let key = FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 0x800,
        };
        let mut record = Record::new(SigCode(*b"PERK"), key);
        record
            .fields
            .extend([raw(b"PRKE", [2, 1, 0]), raw(b"EPFD", 0.85f32.to_le_bytes())]);
        normalize_rank(&mut record, &interner);
        assert_eq!(record.fields[0].value, raw(b"PRKE", [2, 0, 0]).value);
    }

    #[test]
    fn shared_abilities_are_granted_at_every_rank_without_extra_entrypoints() {
        let interner = crate::sym::StringInterner::new();
        let key = FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 0x800,
        };
        let mut root = Record::new(SigCode(*b"PERK"), key);
        root.fields.extend([
            raw(b"DATA", [0, 0, 1, 0, 1]),
            raw(b"PRKE", [1, 1, 0]),
            raw(b"DATA", 0x01001234u32.to_le_bytes()),
            raw(b"PRKF", []),
            raw(b"NNAM", [0; 4]),
            raw(b"PRKE", [2, 1, 0]),
            raw(b"DATA", [134, 3, 2]),
            raw(b"PRKF", []),
        ]);
        let mut rank = Record::new(SigCode(*b"PERK"), key);
        rank.fields = ability_entries(&root).into();
        normalize_rank(&mut rank, &interner);
        assert_eq!(rank.fields.len(), 3);
        assert_eq!(rank.fields[0].value, raw(b"PRKE", [1, 0, 0]).value);
        assert_eq!(
            rank.fields[1].value,
            raw(b"DATA", 0x01001234u32.to_le_bytes()).value
        );
        for family in SHARED_ABILITIES {
            for number in 1..=3 {
                assert_eq!(
                    shared_ability_root(&format!("{family}{number:02}")),
                    Some(format!("{family}01"))
                );
            }
        }
        assert_eq!(shared_ability_root("GHL_RadiationPower01"), None);
    }
}
