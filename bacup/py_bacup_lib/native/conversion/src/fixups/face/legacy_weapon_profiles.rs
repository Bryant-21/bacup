use std::path::{Component, Path, PathBuf};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::face::build_additive_race_record::{
    SubgraphBlock, build_additive_race_record, parse_canonical_subgraphs,
};
use crate::fixups::face::generate_additive_races::load_target_race_template_from_masters;
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

#[derive(Debug, Clone)]
pub struct LegacyWeaponProfile {
    pub donor: SubgraphBlock,
    pub custom_graph_path: String,
    pub custom_animation_folder: String,
    pub emitted_clip_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LegacyWeaponProfileRegistration {
    pub source_weapon: FormKey,
    pub target_weapon: FormKey,
    pub editor_id: String,
    pub profiles: Vec<LegacyWeaponProfile>,
}

pub fn legacy_weapon_donor(
    animation_type: u32,
    attack_animation: u8,
    per_round_reload: bool,
    interner: &StringInterner,
) -> Option<FormKey> {
    let local = match animation_type {
        3 | 4 if attack_animation == 50 || per_round_reload => 0x0CE97D,
        3 | 4 => 0x004822,
        5 | 7 if matches!(attack_animation, 38 | 144) => 0x04F46A,
        5..=7 => 0x0DF42E,
        8 => 0x01F669,
        9 => 0x03F6F8,
        _ => return None,
    };
    Some(FormKey {
        local,
        plugin: interner.intern("Fallout4.esm"),
    })
}

pub fn discover_legacy_weapon_donor_profiles(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    donor_weapon: FormKey,
) -> Result<Vec<SubgraphBlock>, FixupError> {
    let schema = session.schema().map_err(handle_error)?;
    let weapon =
        load_target_race_template_from_masters(session, &schema, mapper, config, donor_weapon)
            .ok_or_else(|| FixupError::Other("legacy weapon donor WEAP is not loaded".into()))?;
    if weapon.sig.as_str() != "WEAP" {
        return Err(FixupError::Other(
            "legacy weapon donor is not a WEAP".into(),
        ));
    }
    let parent = human_subgraph_parent(mapper.interner);
    let template = load_target_race_template_from_masters(session, &schema, mapper, config, parent)
        .ok_or_else(|| FixupError::Other("FO4 human subgraph template is not loaded".into()))?;
    let keywords: FxHashSet<_> = weapon_keywords(&weapon)?.into_iter().collect();
    Ok(select_donor_profiles(
        parse_canonical_subgraphs(&template),
        &keywords,
    ))
}

fn select_donor_profiles(
    blocks: Vec<SubgraphBlock>,
    keywords: &FxHashSet<FormKey>,
) -> Vec<SubgraphBlock> {
    let mut selected: Vec<SubgraphBlock> = Vec::new();
    let mut contexts = FxHashMap::default();
    for block in blocks {
        let Some(perspective) = weapon_perspective(&block) else {
            continue;
        };
        if block.target_keywords.is_empty()
            || !block
                .target_keywords
                .iter()
                .all(|keyword| keywords.contains(keyword))
        {
            continue;
        }
        let key = (perspective, block.subgraph_keywords.clone());
        if let Some(&index) = contexts.get(&key) {
            let existing: &mut SubgraphBlock = &mut selected[index];
            if block.target_keywords.len() > existing.target_keywords.len() {
                *existing = block;
            }
        } else {
            contexts.insert(key, selected.len());
            selected.push(block);
        }
    }
    selected
}

pub fn register_legacy_weapon_profiles(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    meshes_root: &Path,
    registrations: &[LegacyWeaponProfileRegistration],
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    if registrations.is_empty() {
        return Ok(report);
    }
    let schema = session.schema().map_err(handle_error)?;
    let parent = human_subgraph_parent(mapper.interner);
    let template = load_target_race_template_from_masters(session, &schema, mapper, config, parent)
        .ok_or_else(|| FixupError::Other("FO4 human subgraph template is not loaded".into()))?;

    for registration in registrations {
        let validation = validate_staged_profiles(meshes_root, &registration.profiles);
        if let Err(reason) = validation {
            report.warnings.push(mapper.interner.intern(&format!(
                "legacy_weapon_profiles:{}:{reason}",
                registration.editor_id,
            )));
            continue;
        }
        if registration.target_weapon.plugin != mapper.output_plugin_sym() {
            return Err(FixupError::Other(
                "legacy weapon profile must own its WEAP".into(),
            ));
        }
        let mut weapon = session
            .record_decoded(&registration.target_weapon, &schema, mapper.interner)
            .map_err(handle_error)?;
        if weapon.sig.as_str() != "WEAP" {
            return Err(FixupError::Other(
                "legacy weapon profile target is not a WEAP".into(),
            ));
        }
        let mut keywords = weapon_keywords(&weapon)?;
        let keyword_eid = mapper
            .interner
            .intern(&format!("{}WeaponAnimationKeyword", registration.editor_id,));
        let race_eid = mapper
            .interner
            .intern(&format!("{}HumanRaceAdditive", registration.editor_id,));
        let synthetic_plugin = mapper.interner.intern(&format!(
            "__legacy_weapon_profiles__{}_{:06X}",
            mapper
                .interner
                .resolve(registration.source_weapon.plugin)
                .unwrap_or(""),
            registration.source_weapon.local,
        ));
        let keyword_fk = mapper.allocate_or_resolve(
            FormKey {
                local: 0,
                plugin: synthetic_plugin,
            },
            Some(keyword_eid),
            SigCode(*b"KYWD"),
        );
        let race_fk = mapper.allocate_or_resolve(
            FormKey {
                local: 1,
                plugin: synthetic_plugin,
            },
            Some(race_eid),
            SigCode(*b"RACE"),
        );
        let blocks = registration
            .profiles
            .iter()
            .map(|profile| {
                let mut block = profile.donor.clone();
                block.behaviour_graph = mapper.interner.intern(&profile.custom_graph_path);
                let folder = mapper.interner.intern(&profile.custom_animation_folder);
                block.paths.retain(|path| *path != folder);
                block.paths.insert(0, folder);
                if !block.target_keywords.contains(&keyword_fk) {
                    block.target_keywords.push(keyword_fk);
                }
                block
            })
            .collect::<Vec<_>>();
        for keyword in blocks.iter().flat_map(|block| &block.target_keywords) {
            if !keywords.contains(keyword) {
                keywords.push(*keyword);
            }
        }
        set_weapon_keywords(&mut weapon, keywords);
        let mut keyword = Record::new(SigCode(*b"KYWD"), keyword_fk);
        set_editor_id(&mut keyword, keyword_eid);
        let mut race = build_additive_race_record(template.clone(), parent, &blocks);
        race.form_key = race_fk;
        set_editor_id(&mut race, race_eid);
        for record in [keyword, race] {
            if session
                .record_decoded(&record.form_key, &schema, mapper.interner)
                .is_ok()
            {
                session
                    .replace_record_contents(record, &schema, mapper.interner)
                    .map_err(handle_error)?;
                report.records_changed += 1;
            } else {
                session
                    .add_record(record, &schema, mapper.interner)
                    .map_err(handle_error)?;
                report.records_added += 1;
            }
        }
        session
            .replace_record_contents(weapon, &schema, mapper.interner)
            .map_err(handle_error)?;
        report.records_changed += 1;
    }
    Ok(report)
}

pub fn weapon_perspective(block: &SubgraphBlock) -> Option<u16> {
    let flags = block.flags_bytes.as_deref()?;
    let role = u16::from_le_bytes(flags.get(..2)?.try_into().ok()?);
    let perspective = u16::from_le_bytes(flags.get(2..4)?.try_into().ok()?);
    (role == 1 && perspective <= 1).then_some(perspective)
}

fn human_subgraph_parent(interner: &StringInterner) -> FormKey {
    FormKey {
        local: 0x166729,
        plugin: interner.intern("Fallout4.esm"),
    }
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn relative_mesh_path(value: &str) -> Result<PathBuf, String> {
    let normalized = value.replace('\\', "/");
    let path = PathBuf::from(&normalized);
    if normalized.is_empty()
        || normalized.contains(':')
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("invalid relative mesh path: {value}"));
    }
    Ok(path)
}

fn validate_staged_profiles(root: &Path, profiles: &[LegacyWeaponProfile]) -> Result<(), String> {
    if profiles.is_empty() {
        return Err("no profiles".into());
    }
    for profile in profiles {
        if weapon_perspective(&profile.donor).is_none() || profile.donor.target_keywords.is_empty()
        {
            return Err("donor is not a gated human weapon profile".into());
        }
        let graph = relative_mesh_path(&profile.custom_graph_path)?;
        let folder = relative_mesh_path(&profile.custom_animation_folder)?;
        if !root.join(graph).is_file() {
            return Err(format!(
                "graph is not staged: {}",
                profile.custom_graph_path
            ));
        }
        if profile.emitted_clip_paths.is_empty() {
            return Err("no staged source clips".into());
        }
        for clip in &profile.emitted_clip_paths {
            let clip_path = relative_mesh_path(clip)?;
            if !clip_path.starts_with(&folder) || !root.join(&clip_path).is_file() {
                return Err(format!(
                    "source clip is not staged in its profile folder: {clip}"
                ));
            }
        }
    }
    Ok(())
}

fn weapon_keywords(record: &Record) -> Result<Vec<FormKey>, FixupError> {
    let mut keywords = Vec::new();
    for entry in record.fields.iter().filter(|entry| entry.sig.0 == *b"KWDA") {
        match &entry.value {
            FieldValue::List(values) => {
                for value in values {
                    let FieldValue::FormKey(keyword) = value else {
                        return Err(FixupError::Other(
                            "WEAP KWDA has an undecoded keyword".into(),
                        ));
                    };
                    keywords.push(*keyword);
                }
            }
            FieldValue::FormKey(keyword) => keywords.push(*keyword),
            _ => return Err(FixupError::Other("WEAP KWDA is not decoded".into())),
        }
    }
    Ok(keywords)
}

fn set_weapon_keywords(record: &mut Record, keywords: Vec<FormKey>) {
    let insertion = record
        .fields
        .iter()
        .position(|field| matches!(field.sig.as_str(), "KSIZ" | "KWDA"))
        .unwrap_or(record.fields.len());
    record
        .fields
        .retain(|field| !matches!(field.sig.as_str(), "KSIZ" | "KWDA"));
    record.fields.insert(
        insertion,
        FieldEntry {
            sig: SubrecordSig(*b"KSIZ"),
            value: FieldValue::Uint(keywords.len() as u64),
        },
    );
    record.fields.insert(
        insertion + 1,
        FieldEntry {
            sig: SubrecordSig(*b"KWDA"),
            value: FieldValue::List(keywords.into_iter().map(FieldValue::FormKey).collect()),
        },
    );
}

fn set_editor_id(record: &mut Record, eid: Sym) {
    record.eid = Some(eid);
    if let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"EDID")
    {
        field.value = FieldValue::String(eid);
    } else {
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(eid),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };
    use smallvec::smallvec;

    fn donor(interner: &StringInterner, perspective: u16) -> SubgraphBlock {
        SubgraphBlock {
            behaviour_graph: interner.intern(r"Actors\Character\Behaviors\Weapon.hkx"),
            paths: vec![interner.intern(r"Actors\Character\Animations\Weapon\Pistol")],
            subgraph_keywords: vec![],
            target_keywords: vec![FormKey {
                local: 0x1f948,
                plugin: interner.intern("Fallout4.esm"),
            }],
            flags_bytes: Some(smallvec![1, 0, perspective as u8, 0]),
        }
    }

    fn profile(interner: &StringInterner) -> LegacyWeaponProfile {
        LegacyWeaponProfile {
            donor: donor(interner, 0),
            custom_graph_path: "Actors/B21_FNV/Weapon/Weapon.hkx".into(),
            custom_animation_folder: "Actors/B21_FNV/Weapon/Animations".into(),
            emitted_clip_paths: vec!["Actors/B21_FNV/Weapon/Animations/Fire.hkx".into()],
        }
    }

    fn stage(root: &Path, profile: &LegacyWeaponProfile) {
        for file in std::iter::once(&profile.custom_graph_path).chain(&profile.emitted_clip_paths) {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"staged").unwrap();
        }
    }

    #[test]
    fn source_grip_family_selects_donor_topology() {
        let interner = StringInterner::new();
        for (family, attack, per_round, expected) in [
            (3, 3, false, 0x004822),
            (4, 3, false, 0x004822),
            (3, 50, true, 0x0CE97D),
            (5, 38, false, 0x04F46A),
            (5, 144, false, 0x04F46A),
            (7, 144, false, 0x04F46A),
            (5, 3, false, 0x0DF42E),
            (6, 38, false, 0x0DF42E),
            (6, 144, false, 0x0DF42E),
            (7, 3, false, 0x0DF42E),
            (8, 80, false, 0x01F669),
            (9, 3, false, 0x03F6F8),
        ] {
            assert_eq!(
                legacy_weapon_donor(family, attack, per_round, &interner)
                    .unwrap()
                    .local,
                expected
            );
        }
        assert!(legacy_weapon_donor(2, 3, false, &interner).is_none());
        assert!(legacy_weapon_donor(10, 3, false, &interner).is_none());
    }

    #[test]
    fn missing_clip_rejects_registration_before_records_are_added() {
        let interner = StringInterner::new();
        let temp = tempfile::tempdir().unwrap();
        let profile = profile(&interner);
        stage(temp.path(), &profile);
        std::fs::remove_file(temp.path().join(&profile.emitted_clip_paths[0])).unwrap();
        assert!(
            validate_staged_profiles(temp.path(), &[profile])
                .unwrap_err()
                .contains("source clip is not staged")
        );
    }

    #[test]
    fn staged_clip_must_belong_to_its_profile_folder() {
        let interner = StringInterner::new();
        let temp = tempfile::tempdir().unwrap();
        let mut profile = profile(&interner);
        profile.emitted_clip_paths[0] = "Actors/Other/Animations/Fire.hkx".into();
        stage(temp.path(), &profile);
        assert!(validate_staged_profiles(temp.path(), &[profile]).is_err());
        assert!(relative_mesh_path("../Fallout4.esm").is_err());
    }

    #[test]
    fn more_specific_donor_wins_without_losing_stance_variants() {
        let interner = StringInterner::new();
        let generic = donor(&interner, 0);
        let extra = FormKey {
            local: 0x23456,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut specific = generic.clone();
        specific.target_keywords.push(extra);
        specific.paths.insert(
            0,
            interner.intern(r"Actors\Character\Animations\Weapon\Specific"),
        );
        let mut injured = generic.clone();
        injured.subgraph_keywords.push(extra);
        let keywords = [generic.target_keywords[0], extra].into_iter().collect();
        let blocks =
            select_donor_profiles(vec![generic, specific.clone(), injured.clone()], &keywords);
        assert_eq!(blocks, vec![specific, injured]);
    }

    #[test]
    fn registration_keeps_perspective_variants_and_sapt_fallbacks() {
        let mut interner = StringInterner::new();
        let temp = tempfile::tempdir().unwrap();
        let output = interner.intern("B21_FNV.esp");
        let weapon_fk = FormKey {
            local: 0x800,
            plugin: output,
        };
        let target = plugin_handle_new_native("B21_FNV.esp", Some("fo4")).unwrap();
        let master = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        {
            let mut session = open_session(master, None).unwrap();
            let schema = session.schema().unwrap();
            let mut race = Record::new(SigCode(*b"RACE"), human_subgraph_parent(&interner));
            race.fields.extend(
                crate::fixups::face::build_additive_race_record::block_to_entries(&donor(
                    &interner, 0,
                )),
            );
            session.add_record(race, &schema, &interner).unwrap();
        }
        let mut first = profile(&interner);
        first.donor = donor(&interner, 1);
        let first_gate = FormKey {
            local: 0x4c7ea,
            plugin: interner.intern("Fallout4.esm"),
        };
        first.donor.target_keywords = vec![first_gate];
        first.custom_graph_path = "Actors/B21_FNV/Weapon/FirstPerson.hkx".into();
        let mut third = profile(&interner);
        third.donor.subgraph_keywords.push(FormKey {
            local: 0x12345,
            plugin: interner.intern("Fallout4.esm"),
        });
        stage(temp.path(), &first);
        stage(temp.path(), &third);
        let registration = LegacyWeaponProfileRegistration {
            source_weapon: FormKey {
                local: 0x1234,
                plugin: interner.intern("FalloutNV.esm"),
            },
            target_weapon: weapon_fk,
            editor_id: "B21_9mm".into(),
            profiles: vec![first, third],
        };
        {
            let mut mapper = FormKeyMapper::new(
                [],
                MapperOptions {
                    output_plugin_name: "B21_FNV.esp".into(),
                    target_master_names: vec!["Fallout4.esm".into()],
                    ..MapperOptions::default()
                },
                &mut interner,
            );
            mapper.add_mapping(weapon_fk, weapon_fk);
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            let unrelated = FormKey {
                local: 0x45678,
                plugin: mapper.interner.intern("Fallout4.esm"),
            };
            let mut weapon = Record::new(SigCode(*b"WEAP"), weapon_fk);
            set_weapon_keywords(
                &mut weapon,
                vec![registration.profiles[0].donor.target_keywords[0], unrelated],
            );
            session
                .add_record(weapon, &schema, mapper.interner)
                .unwrap();
            let config = FixupConfig {
                target_master_handle_ids: vec![master],
                ..FixupConfig::default()
            };
            let missing_clip = temp
                .path()
                .join(&registration.profiles[0].emitted_clip_paths[0]);
            std::fs::remove_file(&missing_clip).unwrap();
            let rejected = register_legacy_weapon_profiles(
                &mut session,
                &mut mapper,
                &config,
                temp.path(),
                std::slice::from_ref(&registration),
            )
            .unwrap();
            assert_eq!(rejected.records_added, 0);
            assert_eq!(rejected.records_changed, 0);
            assert_eq!(rejected.warnings.len(), 1);
            assert!(
                session
                    .form_keys_of_sig(SigCode(*b"RACE"), mapper.interner)
                    .unwrap()
                    .is_empty()
            );
            assert!(
                session
                    .form_keys_of_sig(SigCode(*b"KYWD"), mapper.interner)
                    .unwrap()
                    .is_empty()
            );
            std::fs::write(missing_clip, b"staged").unwrap();
            let report = register_legacy_weapon_profiles(
                &mut session,
                &mut mapper,
                &config,
                temp.path(),
                std::slice::from_ref(&registration),
            )
            .unwrap();
            assert_eq!(report.records_added, 2);
            assert_eq!(report.records_changed, 1);
            let race_fk = session
                .form_keys_of_sig(SigCode(*b"RACE"), mapper.interner)
                .unwrap()[0];
            assert_eq!(race_fk.plugin, output);
            let race = session
                .record_decoded(&race_fk, &schema, mapper.interner)
                .unwrap();
            let blocks = parse_canonical_subgraphs(&race);
            assert_eq!(blocks.len(), 2);
            assert_eq!(weapon_perspective(&blocks[0]), Some(1));
            assert_eq!(weapon_perspective(&blocks[1]), Some(0));
            assert_eq!(blocks[1].subgraph_keywords.len(), 1);
            assert_eq!(
                mapper.interner.resolve(blocks[0].paths[0]),
                Some("Actors/B21_FNV/Weapon/Animations")
            );
            assert_eq!(
                mapper.interner.resolve(blocks[0].paths[1]),
                Some(r"Actors\Character\Animations\Weapon\Pistol")
            );
            let unique_keyword = blocks[0].target_keywords[1];
            assert_eq!(blocks[0].target_keywords, vec![first_gate, unique_keyword]);
            assert_eq!(
                blocks[1].target_keywords,
                vec![
                    registration.profiles[1].donor.target_keywords[0],
                    unique_keyword
                ]
            );
            assert!(race.fields.iter().any(|field| field.sig.0 == *b"SADD"
                && field.value == FieldValue::FormKey(human_subgraph_parent(mapper.interner))));
            let weapon = session
                .record_decoded(&weapon_fk, &schema, mapper.interner)
                .unwrap();
            assert_eq!(
                weapon_keywords(&weapon).unwrap(),
                vec![
                    first_gate,
                    unrelated,
                    unique_keyword,
                    registration.profiles[1].donor.target_keywords[0]
                ]
            );
        }
        plugin_handle_close_native(target);
        plugin_handle_close_native(master);
    }
}
