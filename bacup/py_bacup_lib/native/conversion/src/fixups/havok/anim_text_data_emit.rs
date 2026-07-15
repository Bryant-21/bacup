//! Conversion-side RACE/IDLE decode driver for `ck_native` AnimTextData generation.

use std::path::Path;

use crate::fixups::face::build_additive_race_record::parse_canonical_subgraphs;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use ck_native::anim_text_data::emit::{
    AnimTextDataInputs, SubgraphInput, WeaponProfileInput, generate_anim_text_data_with_progress,
};
use ck_native::anim_text_data::graph::StancePerspective;
use ck_native::anim_text_data::stance::{
    StanceFormKey, WeaponRaceFamily, WeaponSraf, WeaponSubgraphMetadata,
};

fn string_value(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_string()),
        _ => None,
    }
}

/// Read every subgraph from a decoded RACE record. Each `SGNM` (Behaviour Graph)
/// begins a subgraph; the consecutive `SAPT` (Path) entries that follow are its
/// SAPT chain, self-first — exactly as the engine stores it. Other subgraph
/// subrecords (`SRAF`, `SAKD`, ...) are ignored.
pub fn subgraphs_from_race_record(
    record: &Record,
    interner: &StringInterner,
) -> Vec<SubgraphInput> {
    let (Ok(sgnm), Ok(sapt)) = (
        SubrecordSig::from_str("SGNM"),
        SubrecordSig::from_str("SAPT"),
    ) else {
        return Vec::new();
    };
    let mut out: Vec<SubgraphInput> = Vec::new();
    for entry in &record.fields {
        if entry.sig == sgnm {
            out.push(SubgraphInput {
                core_behavior: string_value(&entry.value, interner).unwrap_or_default(),
                sapt_chain: Vec::new(),
            });
        } else if entry.sig == sapt {
            if let (Some(sg), Some(p)) = (out.last_mut(), string_value(&entry.value, interner)) {
                sg.sapt_chain.push(p);
            }
        }
    }
    out
}

fn stance_form_key(form_key: FormKey, interner: &StringInterner) -> Option<StanceFormKey> {
    Some(StanceFormKey {
        plugin: interner.resolve(form_key.plugin)?.to_string(),
        local: form_key.local,
    })
}

/// Preserve the full weapon profile carried by native RACE fields. The canonical
/// additive-race parser owns block boundaries and keyword ordering; this adapter
/// only resolves interned values and decodes the raw `SRAF` H,H payload.
fn weapon_subgraph_profiles_from_race_record(
    record: &Record,
    interner: &StringInterner,
) -> Vec<WeaponProfileInput> {
    let Ok(sadd_sig) = SubrecordSig::from_str("SADD") else {
        return Vec::new();
    };
    let owner_race = match stance_form_key(record.form_key, interner) {
        Some(form_key) => form_key,
        None => return Vec::new(),
    };
    let sadd = match record.fields.iter().find(|entry| entry.sig == sadd_sig) {
        Some(entry) => match &entry.value {
            FieldValue::FormKey(form_key) => match stance_form_key(*form_key, interner) {
                Some(form_key) => Some(form_key),
                None => return Vec::new(),
            },
            _ => return Vec::new(),
        },
        None => None,
    };

    parse_canonical_subgraphs(record)
        .into_iter()
        .filter_map(|block| {
            let core_behavior = interner.resolve(block.behaviour_graph)?.to_string();
            let sapt: Vec<String> = block
                .paths
                .iter()
                .map(|path| interner.resolve(*path).map(str::to_string))
                .collect::<Option<_>>()?;
            let sakd: Vec<StanceFormKey> = block
                .subgraph_keywords
                .iter()
                .map(|form_key| stance_form_key(*form_key, interner))
                .collect::<Option<_>>()?;
            let stkd: Vec<StanceFormKey> = block
                .target_keywords
                .iter()
                .map(|form_key| stance_form_key(*form_key, interner))
                .collect::<Option<_>>()?;
            let flags = block.flags_bytes.as_deref()?;
            let sraf = WeaponSraf {
                role: u16::from_le_bytes(flags.get(0..2)?.try_into().ok()?),
                perspective: u16::from_le_bytes(flags.get(2..4)?.try_into().ok()?),
            };
            let perspective = if sraf.perspective != 0 || sakd.is_empty() {
                StancePerspective::FirstPerson
            } else {
                StancePerspective::ThirdPerson
            };
            let subgraph = SubgraphInput {
                core_behavior: core_behavior.clone(),
                sapt_chain: sapt.clone(),
            };
            let id = subgraph.id();
            Some(WeaponProfileInput {
                subgraph,
                stance: WeaponSubgraphMetadata {
                    race_family: WeaponRaceFamily {
                        owner_race: owner_race.clone(),
                        sadd: sadd.clone(),
                    },
                    perspective,
                    sakd,
                    stkd,
                    core_behavior,
                    sapt,
                    sraf,
                    id,
                },
            })
        })
        .collect()
}

/// Collect every `ATKE` (attack event) string from a decoded RACE record. `ATKE` is the
/// flat, repeatable `zstring` paired with each `ATKD` attack-data struct (FO4 schema:
/// `scope_id="attacks"`). Per the AnimEventInfo recipe (`bucket2_animevent_findings.md`)
/// **all** ATKE events are AnimEventInfo candidates — the always-included combat
/// attack→clip events, classified without any AACT walk.
pub fn atke_events_from_race_record(record: &Record, interner: &StringInterner) -> Vec<String> {
    let Ok(atke) = SubrecordSig::from_str("ATKE") else {
        return Vec::new();
    };
    record
        .fields
        .iter()
        .filter(|e| e.sig == atke)
        .filter_map(|e| string_value(&e.value, interner))
        .collect()
}

/// Whether an IDLE `ENAM` event is an AI-initiated combat event by NAME prefix — a
/// proxy for the recipe's root-AACT whitelist (`ActionEvade`/`ActionDodge`/`ActionFire*`/
/// dynamic-anim), which classifies by walking `ANAM` up to a vanilla `AACT`.
///
/// The dispatcher cannot perform that walk: AACT records live in the master
/// (`Fallout4.esm`) and the session decodes only the active plugin. This name proxy
/// admits exactly the AI-combat event families (their event names carry the action
/// semantics: `evadeLeft`/`dodgeBack`/`FireSingle`/`DynamicAnim`) and excludes
/// hit-react/death/locomotion/weapon-handling idles — matching the converted-creature
/// oracles (Snallygaster `DynamicAnim`/`FireSingle`/`evade*`). KNOWN-GAP: the precise
/// SET membership is in-game-gated; the byte-exact resolver maps whatever survives.
fn is_ai_combat_idle_event(name: &str) -> bool {
    let lc = name.to_ascii_lowercase();
    ["evade", "dodge", "fire", "dynamic"]
        .iter()
        .any(|p| lc.starts_with(p))
}

/// Generate AnimTextData for every RACE subgraph in an already-loaded plugin
/// handle. Reads the RACE + IDLE records via a session, extracts subgraphs, and
/// writes the **derivable** bucket files under `out_meshes_root`.
/// `src_meshes_root` is the converted mod's `Meshes` (behavior/animation `.hkx` +
/// SAPT-chain override resolution). The caller owns the handle lifecycle (load
/// before, close after). Returns the total number of bucket files written.
///
/// Emitted (CK-free, byte-exact format — RE specs in `scratchpad/atd_re/`):
/// * `AnimationFileData/<id>.txt` — per subgraph (clip→anim file list).
/// * `AnimationFileData/<projectname>.txt` — main flag-0 project manifest.
/// * `AnimationFileData/<name>fx.txt` — FX project manifest (from
///   `Meshes\UniqueBehaviors\<name>fx`); byte-exact bar the project-name *case*
///   (BA2-lowercased sources; runtime-irrelevant, lookup is case-insensitive).
/// * `ClipGeneratorData/<name_id>.txt` — per core behavior.
/// * `DynamicIdleData/<id>.txt` — per subgraph, from IDLE `GNAM` wildcard idles.
/// * `SyncAnimData/ResolvedSyncAnimData<Race>.txt` — empty creature paired-anim form.
/// * `AnimationOffsets/<id>.txt` — populated per-subgraph root motion.
/// * `AnimationOffsets/<name_id(project)>.txt` — project-level empty creature entry.
/// * `AnimationOffsets/PersistantSubgraphInfoAndOffsetData.txt` — complete aggregate,
///   emitted only when the trusted base aggregate is available.
/// * `AnimationSpeedInfo/<id>.txt`, `AnimationStanceData/<id>.txt`, and
///   `AnimEventInfo/<name_id>.txt` — native generated forms for applicable subgraphs.
///
/// Non-authoritative buckets remain independently best-effort. The aggregate and
/// weapon files are a transaction: any authoritative failure is returned and no
/// stale authoritative output is retained. Weapon `AnimationStanceData` is emitted
/// per subgraph combo (base reuse or generated). Generated weapon SyncAnimData
/// remains absent unless its exact CK representation is known.
pub fn generate_anim_text_data_for_handle(
    handle_id: u64,
    src_meshes_root: &Path,
    out_meshes_root: &Path,
    base_meshes_root: Option<&Path>,
    mod_prefix: Option<&str>,
) -> Result<u32, String> {
    generate_anim_text_data_for_handle_with_base_race_handles(
        handle_id,
        &[],
        src_meshes_root,
        out_meshes_root,
        base_meshes_root,
        mod_prefix,
    )
}

/// Generate AnimTextData with optional loaded base/master handles supplying the
/// decoded RACE metadata used for base StanceData reuse and as the donor catalog
/// that seeds generated weapon StanceData grids.
///
/// A target-only handle has no master records, so no base donors are supplied.
/// That is safe: generated weapon StanceData is withheld and the engine rebuilds it.
pub fn generate_anim_text_data_for_handle_with_base_race_handles(
    handle_id: u64,
    base_race_handle_ids: &[u64],
    src_meshes_root: &Path,
    out_meshes_root: &Path,
    base_meshes_root: Option<&Path>,
    mod_prefix: Option<&str>,
) -> Result<u32, String> {
    generate_anim_text_data_for_handle_with_progress(
        handle_id,
        base_race_handle_ids,
        src_meshes_root,
        out_meshes_root,
        base_meshes_root,
        mod_prefix,
        &mut |_| {},
    )
}

pub fn generate_anim_text_data_for_handle_with_progress(
    handle_id: u64,
    base_race_handle_ids: &[u64],
    src_meshes_root: &Path,
    out_meshes_root: &Path,
    base_meshes_root: Option<&Path>,
    mod_prefix: Option<&str>,
    progress: &mut dyn FnMut(&str),
) -> Result<u32, String> {
    let inputs = decode_anim_text_data_inputs_for_handle(handle_id, base_race_handle_ids)?;
    generate_anim_text_data_with_progress(
        &inputs,
        src_meshes_root,
        out_meshes_root,
        base_meshes_root,
        mod_prefix,
        progress,
    )
    .map(|report| report.written)
}

fn decode_anim_text_data_inputs_for_handle(
    handle_id: u64,
    base_race_handle_ids: &[u64],
) -> Result<AnimTextDataInputs, String> {
    use crate::ids::SigCode;
    use crate::sym::StringInterner;

    let mut session = crate::session::open_session(handle_id, None).map_err(|e| e.to_string())?;
    let target_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let schema = session.schema().map_err(|e| e.to_string())?;
    let interner = StringInterner::new();
    let race_sig = SigCode::from_str("RACE").map_err(|e| e.to_string())?;
    let fks = session
        .form_keys_of_sig(race_sig, &interner)
        .map_err(|e| e.to_string())?;

    let mut subgraphs: Vec<SubgraphInput> = Vec::new();
    let mut weapon_profiles: Vec<WeaponProfileInput> = Vec::new();
    // AnimEventInfo candidate events: RACE `ATKE` (always included) + whitelisted IDLE
    // `ENAM` (collected below). The byte-exact resolver maps each survivor to its clip.
    let mut event_candidates: Vec<String> = Vec::new();
    for fk in &fks {
        let record = session
            .record_decoded(fk, schema.as_ref(), &interner)
            .map_err(|error| format!("failed to decode target RACE {fk:?}: {error}"))?;
        subgraphs.extend(subgraphs_from_race_record(&record, &interner));
        weapon_profiles.extend(weapon_subgraph_profiles_from_race_record(
            &record, &interner,
        ));
        event_candidates.extend(atke_events_from_race_record(&record, &interner));
    }

    let mut base_stance_profiles = Vec::new();
    for &base_handle_id in base_race_handle_ids {
        if base_handle_id == handle_id {
            return Err("target handle cannot be used as the base RACE donor source".to_string());
        }
        let base_fks = session
            .form_keys_of_sig_in_handle(base_handle_id, race_sig, &interner)
            .map_err(|error| {
                format!("failed to enumerate RACE records in base handle {base_handle_id}: {error}")
            })?;
        for fk in &base_fks {
            let record = session
                .record_decoded_in_handle(base_handle_id, fk, schema.as_ref(), &interner)
                .map_err(|error| {
                    format!(
                        "failed to decode base RACE {fk:?} from handle {base_handle_id}: {error}"
                    )
                })?;
            base_stance_profiles.extend(
                weapon_subgraph_profiles_from_race_record(&record, &interner)
                    .into_iter()
                    .map(|profile| profile.stance),
            );
        }
    }

    // IDLE `GNAM` wildcard animation paths feed DynamicIdleData; IDLE `ENAM` (the idle
    // event name) feeds the AnimEventInfo candidate set when AI-combat-classified.
    let mut idle_globs: Vec<String> = Vec::new();
    if let (Ok(idle_sig), Ok(gnam_sig), Ok(enam_sig)) = (
        SigCode::from_str("IDLE"),
        SubrecordSig::from_str("GNAM"),
        SubrecordSig::from_str("ENAM"),
    ) {
        if let Ok(idle_fks) = session.form_keys_of_sig(idle_sig, &interner) {
            for fk in &idle_fks {
                if let Ok(rec) = session.record_decoded(fk, schema.as_ref(), &interner) {
                    for f in &rec.fields {
                        if f.sig == gnam_sig {
                            if let Some(s) = string_value(&f.value, &interner) {
                                if s.contains('*') {
                                    idle_globs.push(s);
                                }
                            }
                        } else if f.sig == enam_sig {
                            if let Some(s) = string_value(&f.value, &interner) {
                                if is_ai_combat_idle_event(&s) {
                                    event_candidates.push(s);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    drop(session);
    Ok(AnimTextDataInputs {
        race_record_count: fks.len(),
        subgraphs,
        weapon_profiles,
        base_stance_profiles,
        target_plugin_name,
        idle_globs,
        event_candidates,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use ck_native::anim_text_data::race_decode::subgraph_inputs_from_plugin;
    use esp_authoring_core::plugin_runtime::{
        ParsedRecord as RawRecord, ParsedSubrecord, insert_parsed_record_in_slot,
        plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_native,
        plugin_handle_save_no_py, plugin_handle_store_ref,
    };

    use super::*;
    use crate::ids::SigCode;
    use crate::record::{FieldEntry, RecordFlags};

    fn raw_zstring(signature: &str, value: &str) -> ParsedSubrecord {
        let mut data = value.as_bytes().to_vec();
        data.push(0);
        ParsedSubrecord {
            signature: signature.into(),
            data: data.into(),
            semantic_type: None,
        }
    }

    fn raw_form_id(signature: &str, value: u32) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: value.to_le_bytes().to_vec().into(),
            semantic_type: Some("formid".to_string()),
        }
    }

    fn raw_bytes(signature: &str, value: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: value.to_vec().into(),
            semantic_type: None,
        }
    }

    fn raw_record(signature: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> RawRecord {
        RawRecord {
            signature: signature.into(),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn write_race_idle_fixture(dir: &Path) -> PathBuf {
        let name = "AnimFixture.esp";
        let path = dir.join(name);
        let handle = plugin_handle_new_native(name, Some("fo4")).expect("create fixture plugin");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.header.masters = vec!["Fallout4.esm".to_string()];
            insert_parsed_record_in_slot(
                slot,
                raw_record(
                    "RACE",
                    0x0100_0800,
                    vec![
                        raw_zstring("EDID", "AnimFixtureRace"),
                        raw_form_id("SAKD", 0x0001_2345),
                        raw_form_id("STKD", 0x0002_3456),
                        raw_zstring("SGNM", r"Actors\Fixture\Behaviors\Fixture.hkx"),
                        raw_zstring("SAPT", r"Actors\Fixture\Animations"),
                        raw_bytes("SRAF", &[7, 0, 0, 0]),
                        raw_form_id("SADD", 0x0003_4567),
                        raw_zstring("ATKE", "AttackPrimary"),
                    ],
                ),
            );
            insert_parsed_record_in_slot(
                slot,
                raw_record(
                    "IDLE",
                    0x0100_0801,
                    vec![
                        raw_zstring("EDID", "AnimFixtureIdle"),
                        raw_zstring("ENAM", "evadeLeft"),
                        raw_zstring("GNAM", r"Actors\Fixture\Animations\*.hkx"),
                    ],
                ),
            );
        }
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).expect("save fixture plugin");
        assert!(plugin_handle_close_native(handle));
        path
    }

    struct PluginHandle(u64);

    impl Drop for PluginHandle {
        fn drop(&mut self) {
            plugin_handle_close_native(self.0);
        }
    }

    #[test]
    fn ck_race_decode_matches_session_decode_for_race_idle_and_master_form_ids() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = write_race_idle_fixture(temp.path());
        let via_ck = AnimTextDataInputs::from(
            subgraph_inputs_from_plugin(&plugin, "fo4", &[]).expect("CK path decode"),
        );
        let handle = PluginHandle(
            plugin_handle_load_no_py(plugin.to_str().unwrap(), Some("fo4"), None, None, true)
                .expect("load fixture for session decode"),
        );
        let via_session =
            decode_anim_text_data_inputs_for_handle(handle.0, &[]).expect("session decode");

        assert_eq!(via_ck, via_session);
        assert_eq!(via_ck.race_record_count, 1);
        assert_eq!(via_ck.subgraphs.len(), 1);
        assert_eq!(via_ck.weapon_profiles.len(), 1);
        assert_eq!(via_ck.idle_globs.len(), 1);
        assert_eq!(via_ck.event_candidates, vec!["AttackPrimary", "evadeLeft"]);
        let stance = &via_ck.weapon_profiles[0].stance;
        assert_eq!(stance.race_family.owner_race.plugin, "AnimFixture.esp");
        assert_eq!(stance.race_family.owner_race.local, 0x800);
        assert_eq!(
            stance.race_family.sadd.as_ref().unwrap().plugin,
            "Fallout4.esm"
        );
        assert_eq!(stance.sakd[0].plugin, "Fallout4.esm");
        assert_eq!(stance.stkd[0].plugin, "Fallout4.esm");
    }

    /// Build a RACE record whose subgraph section mirrors the real Snallygaster
    /// ESP (4 subgraphs sharing one core behavior; injured variants carry a 2-entry
    /// SAPT chain). Interleaves SRAF to prove non-SGNM/SAPT subrecords are ignored.
    fn make_snally_race(interner: &StringInterner) -> Record {
        let sgnm = SubrecordSig::from_str("SGNM").unwrap();
        let sapt = SubrecordSig::from_str("SAPT").unwrap();
        let sraf = SubrecordSig::from_str("SRAF").unwrap();
        let core = r"Actors\Snallygaster\Behaviors\SnallygasterCoreBehavior.hkx";
        let base = r"Actors\Snallygaster\Animations";
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        let mut push = |sig, s: Option<&str>| {
            fields.push(FieldEntry {
                sig,
                value: s
                    .map(|v| FieldValue::String(interner.intern(v)))
                    .unwrap_or(FieldValue::None),
            });
        };
        for chain in [
            vec![base],
            vec![r"Actors\Snallygaster\Animations\Injured\RightLeg", base],
            vec![r"Actors\Snallygaster\Animations\Injured\LeftLeg", base],
            vec![r"Actors\Snallygaster\Animations\injured\boothlegs", base],
        ] {
            push(sgnm, Some(core));
            for p in chain {
                push(sapt, Some(p));
            }
            push(sraf, None); // ignored marker subrecord between subgraphs
        }
        Record {
            sig: SigCode::from_str("RACE").unwrap(),
            form_key: FormKey {
                local: 0x000800,
                plugin: interner.intern("Snallygaster.esp"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    /// The RACE record stores each subgraph's SAPT chain explicitly as the
    /// consecutive Path entries after its Behaviour Graph — reading them back
    /// reproduces all four validated Snallygaster subgraph ids.
    #[test]
    fn reads_subgraphs_and_ids_from_race_record() {
        let interner = StringInterner::new();
        let subs = subgraphs_from_race_record(&make_snally_race(&interner), &interner);
        assert_eq!(subs.len(), 4);
        assert_eq!(
            subs[0].sapt_chain,
            vec![r"Actors\Snallygaster\Animations".to_string()]
        );
        assert_eq!(subs[0].id(), 16837539554263781675);
        assert_eq!(
            subs[1].sapt_chain,
            vec![
                r"Actors\Snallygaster\Animations\Injured\RightLeg".to_string(),
                r"Actors\Snallygaster\Animations".to_string(),
            ]
        );
        assert_eq!(subs[1].id(), 3419542945344999723);
        assert_eq!(subs[2].id(), 9947826212300345643);
        assert_eq!(subs[3].id(), 3632382008203366699);
    }

}
