use super::*;
use papyrus_core::pex::{PexFilePayload, PexFunctionPayload};
use serde_json::json;

fn raven() -> TranslatedScript {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/BlackbirdSinging.geck"
    ));
    let record = json!({
        "eid": "BlackbirdSinging",
        "fields": [
            {"SCTX": {"raw_hex": hex::encode_upper(source)}},
            {"SCRO": "000014:FalloutNV.esm"}
        ],
        "__source_idle_symbols": {"nvravenflyaway": "1424D2:FalloutNV.esm"},
        "__mapped_form_keys": {"1424D2:FalloutNV.esm": "2424D2:Target.esp"}
    });
    translate_scpt_record_with_attachments(
        &record,
        "B21",
        "14C124:FalloutNV.esm",
        &[ScptAttachmentEvidence {
            source_target_form_key: "140DB0:FalloutNV.esm".into(),
            target_signature: "CREA".into(),
        }],
    )
    .unwrap()
}

#[test]
fn actual_raven_source_keeps_source_idle_and_scaled_clock() {
    let translated = raven();
    assert_eq!(translated.properties.len(), 1);
    assert_eq!(
        translated.properties[0].target_form_key.as_deref(),
        Some("2424D2:Target.esp")
    );
    assert!(
        translated
            .psc_text
            .contains("Self.GetDistance(Game.GetPlayer())")
    );
    assert!(
        translated
            .psc_text
            .contains("B21_ConversionClock.GetGameModeClock()")
    );
    assert_eq!(
        translated
            .psc_text
            .matches("fFlyTime = (fFlyTime - B21GameModeDelta)")
            .count(),
        2
    );
}

#[test]
#[ignore = "requires generated Fallout 4 Papyrus headers"]
fn creature_quest_partition_compiles_and_retains_koch_load_behavior() {
    for (key, name, source) in [
        (
            "0FE6CF:FalloutNV.esm",
            "V22KochScript",
            include_str!("fixtures/V22KochScript.geck"),
        ),
        (
            "0E9483:FalloutNV.esm",
            "BMRhondaScript",
            include_str!("fixtures/BMRhondaScript.geck"),
        ),
    ] {
        let record =
            json!({"eid": name, "fields": [{"SCTX": source}], "__source_form_symbols": {}});
        let script = translate_scpt_record_with_attachments(
            &record,
            "B21",
            key,
            &[ScptAttachmentEvidence {
                source_target_form_key: "001000:FalloutNV.esm".into(),
                target_signature: "CREA".into(),
            }],
        )
        .unwrap();
        assert!(script.properties.is_empty());
        assert_eq!(
            script.deferred_creature_quest.as_ref().unwrap().source_text,
            source
        );
        let compiled = papyrus_core::compiler::compile_source(
            &script.psc_text,
            &[std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()],
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(
            compiled.ok,
            "{:?}\n{}",
            compiled.diagnostics, script.psc_text
        );
        let pex = papyrus_core::pex::parse_pex_bytes(&compiled.pex_bytes.unwrap()).unwrap();
        if name == "V22KochScript" {
            let mut values = HashMap::from([("self".into(), 1.0), ("ghosted".into(), 0.0)]);
            assert_eq!(
                execute(event(&pex, "OnLoad"), &mut values, 0.0),
                ["setghost"]
            );
            assert_eq!(values["ghosted"], 1.0);
            assert!(execute(event(&pex, "OnLoad"), &mut values, 0.0).is_empty());
        }
        let output = tempfile::tempdir().unwrap();
        let emitted = crate::fnv_legacy_scripting::psc_emission::emit_psc_files(
            output.path(),
            &[script],
            &[],
            &[],
            &[],
        );
        assert!(emitted.errors.is_empty());
        let audit: Value = serde_json::from_slice(
            &std::fs::read(
                output
                    .path()
                    .join("Scripts/Source/User/fnv_deferred_creature_quests.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(audit[0]["source_text"], source);
        assert_eq!(audit[0]["events"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn creature_actor_value_operations_use_verified_typed_native_properties() {
    let record = json!({"eid": "CreatureAVProbe", "fields": [{"SCTX":
        "scn CreatureAVProbe\nBegin OnLoad\nSetAV Aggression 0\nSetActorValue Variable01 1\nDamageAV BrainCondition 100\nRestoreAV LeftAttackCondition 100\nModActorValue Sneak 25\nEnd\n"}], "__source_form_symbols": {}});
    let script = translate_scpt_record_with_attachments(
        &record,
        "B21",
        "001001:FalloutNV.esm",
        &[ScptAttachmentEvidence {
            source_target_form_key: "001000:FalloutNV.esm".into(),
            target_signature: "CREA".into(),
        }],
    )
    .unwrap();
    for (name, key) in [
        ("Aggression", "0002BC:Fallout4.esm"),
        ("Variable01", "000301:Fallout4.esm"),
        ("BrainCondition", "000372:Fallout4.esm"),
        ("LeftAttackCondition", "00036E:Fallout4.esm"),
        ("Sneak", "00037F:Fallout4.esm"),
    ] {
        assert!(
            script
                .properties
                .iter()
                .any(|property| property.papyrus_name == name
                    && property.target_form_key.as_deref() == Some(key))
        );
    }
    for method in [
        "SetValue(Aggression",
        "SetValue(Variable01",
        "DamageValue(BrainCondition",
        "RestoreValue(LeftAttackCondition",
        "ModValue(Sneak",
    ] {
        assert!(script.psc_text.contains(method), "{}", script.psc_text);
    }
}

fn effect_script(kind: &str) -> TranslatedScript {
    let (eid, script_key, owner, source, dependency, dependency_key, signature) = match kind {
        "nightstalker" => (
            "JacobstownNightstalkerScript",
            "144F2A",
            "144BC4",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/JacobstownNightstalkerScript.geck"
            )),
            "StealthBoyInvisibilitySpell",
            "07E474",
            "SPEL",
        ),
        "glowingone" => (
            "vGlowingOneSCRIPT",
            "13303B",
            "01CF7B",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/vGlowingOneSCRIPT.geck"
            )),
            "vGlowingOneRadiationAuraExplosion",
            "133032",
            "EXPL",
        ),
        "radroach" => (
            "NVCCBunkerRadroachSCRIPT",
            "13303C",
            "13215C",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/NVCCBunkerRadroachSCRIPT.geck"
            )),
            "vGlowingOneRadiationAuraExplosion",
            "133032",
            "EXPL",
        ),
        _ => unreachable!(),
    };
    let source_key = format!("{dependency_key}:FalloutNV.esm");
    let mut record = json!({
        "eid": eid,
        "fields": [{"SCTX": {"raw_hex": hex::encode_upper(source)}}, {"SCRO": source_key}],
        "__source_form_symbols": {}, "__mapped_form_keys": {}
    });
    record["__source_form_symbols"][dependency.to_ascii_lowercase()] =
        json!({"form_key": source_key, "signature": signature});
    record["__mapped_form_keys"][&source_key] = json!("008000:Converted.esm");
    translate_scpt_record_with_attachments(
        &record,
        "B21",
        &format!("{script_key}:FalloutNV.esm"),
        &[ScptAttachmentEvidence {
            source_target_form_key: format!("{owner}:FalloutNV.esm"),
            target_signature: "CREA".into(),
        }],
    )
    .unwrap()
}

#[test]
fn actual_creature_effect_scripts_bind_source_spell_explosion_and_target_health() {
    for kind in ["nightstalker", "glowingone", "radroach"] {
        let translated = effect_script(kind);
        assert_eq!(translated.papyrus_type, PapyrusType::Actor);
        assert!(translated
            .properties
            .iter()
            .any(|property| property.target_form_key.as_deref() == Some("008000:Converted.esm")));
        if kind == "glowingone" {
            let health = translated
                .properties
                .iter()
                .find(|property| property.papyrus_name == "Health")
                .unwrap();
            assert_eq!(health.papyrus_type, "ActorValue");
            assert_eq!(
                health.target_form_key.as_deref(),
                Some("0002D4:Fallout4.esm")
            );
        }
    }
}

#[test]
fn typed_source_symbols_cannot_bypass_scro_identity_or_target_publication() {
    let source = "scn TypedEffect\nBegin OnLoad\nAddSpell Cloak\nEnd";
    let mut record = json!({
        "eid": "TypedEffect",
        "fields": [{"SCTX": source}, {"SCRO": "001234:Skyrim.esm"}],
        "__source_form_symbols": {"cloak": {"form_key": "001234:Dawnguard.esm", "signature": "SPEL"}},
        "__mapped_form_keys": {"001234:Dawnguard.esm": "008000:Converted.esm"}
    });
    assert!(
        translate_scpt_record(&record, "B21", "008001:Source.esm")
            .unwrap_err()
            .to_string()
            .contains("not in the source SCRO")
    );
    record["__source_form_symbols"]["cloak"]["form_key"] = json!("001234:Skyrim.esm");
    assert!(
        translate_scpt_record(&record, "B21", "008001:Source.esm")
            .unwrap_err()
            .to_string()
            .contains("no emitted target record")
    );
}

fn key(name: &str) -> String {
    name.strip_prefix("::")
        .unwrap_or(name)
        .trim_end_matches("_var")
        .to_ascii_lowercase()
}

fn limb_script(kind: &str) -> TranslatedScript {
    let (eid, local, owner, source, dependencies): (_, _, _, _, &[(&str, &str, &str)]) = match kind
    {
        "mantis" => (
            "NVCrGiantMantisScript",
            "136DDC",
            "0EF412",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/NVCrGiantMantisScript.geck"
            )),
            &[("MantisLeg", "0EE6CC", "MISC")],
        ),
        "nymph" => (
            "CrGiantMantisNymphScript",
            "0EF407",
            "0EF411",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/CrGiantMantisNymphScript.geck"
            )),
            &[("Player", "000014", "REFR")],
        ),
        "tumbleweed" => (
            "NVCRTumbleweedScript",
            "1732D3",
            "16938D",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/NVCRTumbleweedScript.geck"
            )),
            &[],
        ),
        "radiotumbleweed" => (
            "NVCRTumbleweedRadSCRIPT",
            "1732D4",
            "163CF4",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/NVCRTumbleweedRadSCRIPT.geck"
            )),
            &[("vGlowingOneRadiationAuraExplosion", "133032", "EXPL")],
        ),
        _ => unreachable!(),
    };
    let mut record = json!({"eid": eid, "fields": [{"SCTX": source}], "__source_form_symbols": {}, "__mapped_form_keys": {}});
    for (name, local, signature) in dependencies {
        let key = format!("{local}:FalloutNV.esm");
        record["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"SCRO": key}));
        record["__source_form_symbols"][name.to_ascii_lowercase()] =
            json!({"form_key": key, "signature": signature});
        record["__mapped_form_keys"][key] = json!("008000:Converted.esm");
    }
    translate_scpt_record_with_attachments(
        &record,
        "B21",
        &format!("{local}:FalloutNV.esm"),
        &[ScptAttachmentEvidence {
            source_target_form_key: format!("{owner}:FalloutNV.esm"),
            target_signature: "CREA".into(),
        }],
    )
    .unwrap()
}

#[test]
fn actual_mantis_nymph_and_tumbleweeds_preserve_source_limb_and_death_operations() {
    let mantis = limb_script("mantis");
    assert!(
        mantis
            .psc_text
            .contains("B21_ConversionCreature.IsLimbGone(ThisCreature as Actor, 3)")
    );
    assert!(
        mantis
            .psc_text
            .contains("B21_ConversionCreature.IsLimbGone(ThisCreature as Actor, 5)")
    );
    assert!(
        mantis
            .psc_text
            .contains("ThisCreature.RemoveItem(MantisLeg, 1)")
    );
    assert!(mantis.psc_text.contains("Self.AddItem(MantisLeg, 1)"));
    assert!(
        limb_script("nymph")
            .psc_text
            .contains(".Kill(Game.GetPlayer() as Actor)")
    );
    for kind in ["tumbleweed", "radiotumbleweed"] {
        let script = limb_script(kind);
        assert_eq!(
            script
                .psc_text
                .matches(
                    "B21_ConversionCreature.KillActorWithLimb(Self as Actor, rWeed as Actor, 0)"
                )
                .count(),
            2
        );
    }
}

fn nellis_scripts() -> (TranslatedScript, TranslatedScript) {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/NellisExplodingAntScript.geck"
    ));
    let mut record = json!({"eid": "NellisExplodingAntScript", "fields": [{"SCTX": source}], "__source_form_symbols": {}, "__mapped_form_keys": {}});
    for (index, name, local, signature) in [
        (0, "ExplodesGasTrapsOnHit", "07627F", "FLST"),
        (1, "NellisAntExplosion", "10414A", "EXPL"),
        (2, "VFreeformNellis", "0FED44", "QUST"),
        (3, "VNellisInfestation", "1083DE", "QUST"),
    ] {
        let source_key = format!("{local}:FalloutNV.esm");
        record["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"SCRO": source_key}));
        record["__source_form_symbols"][name.to_ascii_lowercase()] =
            json!({"form_key": source_key, "signature": signature});
        record["__mapped_form_keys"][source_key] =
            json!(format!("{:06X}:Converted.esm", 0x8000 + index));
    }
    let ant = translate_scpt_record_with_attachments(
        &record,
        "B21",
        "102AB3:FalloutNV.esm",
        &[ScptAttachmentEvidence {
            source_target_form_key: "102AAD:FalloutNV.esm".into(),
            target_signature: "CREA".into(),
        }],
    )
    .unwrap();
    (ant, nellis_storage_script())
}

fn nellis_storage_script() -> TranslatedScript {
    translate_scpt_record_with_attachments(&json!({
        "eid": "VFreeformNellisScript", "fields": [
            {"SCHR": {"Type": 1}},
            {"SCTX": include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/VFreeformNellisScript.geck"))}
        ], "__source_form_symbols": {}
    }), "B21", "0FED40:FalloutNV.esm", &[ScptAttachmentEvidence {source_target_form_key: "0FED44:FalloutNV.esm".into(), target_signature: "QUST".into()}]).unwrap()
}

#[test]
fn actual_nellis_ant_keeps_hit_list_thresholds_and_original_quest_storage() {
    let (ant, quest) = nellis_scripts();
    assert!(
        ant.psc_text
            .contains("ExplodesGasTrapsOnHit.HasForm(akSource)")
    );
    assert!(ant.psc_text.contains("Self.GetValuePercentage(Health)"));
    assert!(
        ant.psc_text
            .contains("B21_S_0FED40 Property vfreeformnellis Auto Const")
    );
    assert!(
        ant.psc_text
            .to_ascii_lowercase()
            .contains("vfreeformnellis.iantcount = (vfreeformnellis.iantcount + 1)")
    );
    assert_eq!(
        ant.psc_text
            .matches("PlaceAtMe(NellisAntExplosion)")
            .count(),
        2
    );
    assert_eq!(quest.papyrus_type, PapyrusType::Quest);
    assert_eq!(quest.script_class_name, "B21_S_0FED40");
    assert!(quest.psc_text.contains("Int Property iAntCount Auto"));
    assert!(quest.psc_text.contains("Int Property iInfestation Auto"));
    assert!(!quest.psc_text.contains("Event "));
}

fn execute(
    function: &PexFunctionPayload,
    values: &mut HashMap<String, f64>,
    clock: f64,
) -> Vec<String> {
    let mut calls = Vec::new();
    let mut pc = 0i64;
    for _ in 0..1000 {
        if pc as usize == function.instructions.len() {
            return calls;
        }
        let instruction = &function.instructions[pc as usize];
        let raw = |index: usize| instruction.args[index].data.as_str().unwrap();
        let value = |index: usize| {
            let arg = &instruction.args[index];
            if arg.value_type == 1 {
                *values.get(&key(raw(index))).unwrap_or(&0.0)
            } else if let Some(value) = arg.data.as_bool() {
                f64::from(u8::from(value))
            } else {
                arg.data.as_f64().unwrap_or_default()
            }
        };
        let mut destination = 0;
        let result = match instruction.opcode {
            0 => None,
            1 | 2 => Some(value(1) + value(2)),
            3 | 4 => Some(value(1) - value(2)),
            0x0a => Some(f64::from(u8::from(value(1) == 0.0))),
            0x0d => Some(value(1)),
            0x0e => Some(
                if function
                    .locals
                    .iter()
                    .any(|local| local.name == raw(0) && local.ty.eq_ignore_ascii_case("bool"))
                {
                    f64::from(u8::from(value(1) != 0.0))
                } else {
                    value(1)
                },
            ),
            0x0f => Some(f64::from(u8::from(value(1) == value(2)))),
            0x10 => Some(f64::from(u8::from(value(1) < value(2)))),
            0x11 => Some(f64::from(u8::from(value(1) <= value(2)))),
            0x12 => Some(f64::from(u8::from(value(1) > value(2)))),
            0x13 => Some(f64::from(u8::from(value(1) >= value(2)))),
            0x14 => {
                pc += value(0) as i64;
                continue;
            }
            0x15 if value(0) != 0.0 => {
                pc += value(1) as i64;
                continue;
            }
            0x16 if value(0) == 0.0 => {
                pc += value(1) as i64;
                continue;
            }
            0x15 | 0x16 => None,
            0x17 => {
                let method = raw(0).to_ascii_lowercase();
                calls.push(method.clone());
                destination = 2;
                match method.as_str() {
                    "getdistance" => Some(*values.get("distance_result").unwrap_or(&256.0)),
                    "playidle" | "addspell" | "removespell" | "placeatme" => Some(1.0),
                    "isdead" => Some(0.0),
                    "restorevalue" => None,
                    "getvalue" => Some(*values.get("health").unwrap()),
                    "getitemcount" => Some(*values.get("itemcount").unwrap()),
                    "isininterior" => Some(*values.get("interior").unwrap()),
                    "additem" | "removeitem" | "kill" | "setghost" => None,
                    "getvaluepercentage" => Some(*values.get("healthpercentage").unwrap()),
                    "hasform" => Some(*values.get("weaponmatches").unwrap()),
                    "isobjectivecompleted" => Some(f64::from(u8::from(value(4) == 15.0))),
                    "setobjectivecompleted" | "setobjectivedisplayed" => None,
                    "registerforhitevent" | "starttimer" | "disable" | "delete" => None,
                    _ => panic!("unverified Raven call {instruction:?}"),
                }
            }
            0x19 => {
                destination = 2;
                match (
                    raw(0).to_ascii_lowercase().as_str(),
                    raw(1).to_ascii_lowercase().as_str(),
                ) {
                    ("b21_conversionclock", "canrungamemode") => {
                        Some(if values.get("paused").copied().unwrap_or(0.0) == 0.0 {
                            1.0
                        } else {
                            0.0
                        })
                    }
                    ("b21_conversionclock", "getgamemodeclock") => {
                        calls.push("clock".into());
                        Some(clock)
                    }
                    ("b21_conversionclock", "requestnextgamemode") => {
                        calls.push("nextframe".into());
                        None
                    }
                    ("b21_conversionclock", "cancelgamemode") => {
                        calls.push("cancelframes".into());
                        None
                    }
                    ("game", "getplayer") => Some(1.0),
                    ("b21_conversioncreature", "islimbgone") => {
                        Some(*values.get(&format!("limb{}", value(5) as i32)).unwrap())
                    }
                    ("b21_conversioncreature", "killactorwithlimb") => {
                        assert_eq!(value(6), 0.0);
                        calls.push("killwithtorso".into());
                        None
                    }
                    _ => panic!("unverified Raven static call {instruction:?}"),
                }
            }
            0x1a => return calls,
            0x1c => {
                destination = 2;
                Some(*values.get(&key(raw(0))).unwrap_or(&0.0))
            }
            0x1d => Some(value(2)),
            _ => panic!("unverified Raven opcode {instruction:?}"),
        };
        if let Some(result) = result {
            values.insert(key(raw(destination)), result);
        }
        pc += 1;
        if pc as usize == function.instructions.len() {
            return calls;
        }
    }
    panic!("Raven event {} did not terminate", function.name);
}

fn event<'a>(pex: &'a PexFilePayload, name: &str) -> &'a PexFunctionPayload {
    pex.objects[0]
        .states
        .iter()
        .flat_map(|state| &state.functions)
        .find(|function| function.name.eq_ignore_ascii_case(name))
        .unwrap()
}

#[test]
#[ignore = "requires generated Fallout 4 Papyrus headers"]
fn actual_raven_compiled_bytecode_flies_then_deletes_on_source_elapsed_time() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("B21_ConversionClock.psc"),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../python/bacup_lib/script_additions/fnv_fo4/B21_ConversionClock.psc"
        )),
    )
    .unwrap();
    let imports = vec![
        root.path().to_string_lossy().into_owned(),
        std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap(),
    ];
    let translated = raven();
    let compiled = papyrus_core::compiler::compile_source(
        &translated.psc_text,
        &imports,
        papyrus_core::profile::Game::Fo4,
        None,
    );
    assert!(
        compiled.ok,
        "{:?}\n{}",
        compiled.diagnostics, translated.psc_text
    );
    let pex = papyrus_core::pex::parse_pex_bytes(&compiled.pex_bytes.unwrap()).unwrap();
    assert_eq!(pex.game_id, 2);
    let mut values = HashMap::from([
        ("aitimerid".into(), 0.0),
        ("self".into(), 1.0),
        ("nvravenflyaway".into(), 1.0),
    ]);
    execute(event(&pex, "OnLoad"), &mut values, 100.0);
    let hit = execute(event(&pex, "OnHit"), &mut values, 100.0);
    assert_eq!(hit, vec!["registerforhitevent", "playidle"]);
    assert_eq!(values["fflytime"], 9.0);
    values.insert("paused".into(), 1.0);
    assert_eq!(
        execute(event(&pex, "OnTimer"), &mut values, 100.0),
        ["nextframe"]
    );
    assert_eq!(values["fflytime"], 9.0);
    values.insert("paused".into(), 0.0);
    let calls = execute(event(&pex, "OnTimer"), &mut values, 101.25);
    assert_eq!(calls.iter().filter(|call| *call == "clock").count(), 1);
    assert_eq!(values["fflytime"], 6.5);
    execute(event(&pex, "OnTimer"), &mut values, 105.0);
    assert_eq!(values["fflytime"], -1.0);
    let calls = execute(event(&pex, "OnTimer"), &mut values, 105.5);
    assert!(calls.windows(2).any(|pair| pair == ["disable", "delete"]));
    assert_eq!(values["bflyready"], 0.0);
    execute(event(&pex, "OnLoad"), &mut values, 0.0);
    execute(event(&pex, "OnTimer"), &mut values, 0.25);
    assert_eq!(values["b21gamemodedelta"], 0.25);
    assert_eq!(
        execute(event(&pex, "OnUnload"), &mut values, 0.25),
        ["cancelframes"]
    );
    assert!(execute(event(&pex, "OnTimer"), &mut values, 1.0).is_empty());
}

#[test]
#[ignore = "requires generated Fallout 4 Papyrus headers"]
fn actual_creature_effect_bytecode_preserves_cloak_and_periodic_effect_callbacks() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("B21_ConversionClock.psc"),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../python/bacup_lib/script_additions/fnv_fo4/B21_ConversionClock.psc"
        )),
    )
    .unwrap();
    let imports = vec![
        root.path().to_string_lossy().into_owned(),
        std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap(),
    ];
    for kind in ["nightstalker", "glowingone", "radroach"] {
        let translated = effect_script(kind);
        let compiled = papyrus_core::compiler::compile_source(
            &translated.psc_text,
            &imports,
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(
            compiled.ok,
            "{kind}: {:?}\n{}",
            compiled.diagnostics, translated.psc_text
        );
        let pex = papyrus_core::pex::parse_pex_bytes(&compiled.pex_bytes.unwrap()).unwrap();
        let mut values = HashMap::from([
            ("self".into(), 1.0),
            ("aitimerid".into(), 0.0),
            ("aecombatstate".into(), 2.0),
        ]);
        let calls = execute(event(&pex, "OnLoad"), &mut values, 50.0);
        if kind == "nightstalker" {
            assert!(calls.contains(&"addspell".into()));
            assert!(
                execute(event(&pex, "OnHit"), &mut values, 50.0).contains(&"removespell".into())
            );
            assert!(
                !execute(event(&pex, "OnHit"), &mut values, 50.0).contains(&"removespell".into())
            );
            assert!(execute(event(&pex, "OnCombatStateChanged"), &mut values, 50.0).is_empty());
            values.insert("aecombatstate".into(), 0.0);
            assert!(
                execute(event(&pex, "OnCombatStateChanged"), &mut values, 50.0)
                    .contains(&"addspell".into())
            );
            assert_eq!(values["decloak"], 0.0);
        } else {
            execute(event(&pex, "OnTimer"), &mut values, 54.0);
            let calls = execute(event(&pex, "OnTimer"), &mut values, 54.25);
            assert!(calls.contains(&"placeatme".into()));
            assert_eq!(calls.contains(&"restorevalue".into()), kind == "glowingone");
            assert_eq!(values["itimer"], 0.0);
        }
    }
}

#[test]
#[ignore = "requires generated Fallout 4 Papyrus headers"]
fn actual_nellis_compiled_bytecode_retains_hit_and_death_explosion_conditions() {
    let (ant, quest) = nellis_scripts();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join(format!("{}.psc", quest.script_class_name)),
        &quest.psc_text,
    )
    .unwrap();
    let imports = vec![
        root.path().to_string_lossy().into_owned(),
        std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap(),
    ];
    for source in [&quest.psc_text, &ant.psc_text] {
        let compiled = papyrus_core::compiler::compile_source(
            source,
            &imports,
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(compiled.ok, "{:?}\n{source}", compiled.diagnostics);
        if source == &quest.psc_text {
            continue;
        }
        let pex = papyrus_core::pex::parse_pex_bytes(&compiled.pex_bytes.unwrap()).unwrap();
        for (health, hit_explodes) in [(0.08, true), (0.4, false)] {
            let mut values = HashMap::from([
                ("self".into(), 1.0),
                ("healthpercentage".into(), health),
                ("weaponmatches".into(), 1.0),
                ("iantcount".into(), 14.0),
            ]);
            let calls = execute(event(&pex, "OnHit"), &mut values, 0.0);
            assert_eq!(calls.contains(&"placeatme".into()), hit_explodes);
            assert_eq!(values["ilightmyfire"], 1.0);
            let calls = execute(event(&pex, "OnDeath"), &mut values, 0.0);
            assert_eq!(calls.contains(&"placeatme".into()), !hit_explodes);
            assert!(calls.contains(&"setobjectivecompleted".into()));
            assert_eq!(values["iantcount"], 15.0);
            assert_eq!(values["iinfestation"], 1.0);
            assert_eq!(values["doonce"], 1.0);
        }
        let mut values = HashMap::from([
            ("healthpercentage".into(), 0.01),
            ("weaponmatches".into(), 0.0),
        ]);
        assert!(!execute(event(&pex, "OnHit"), &mut values, 0.0).contains(&"placeatme".into()));
    }
}

#[test]
#[ignore = "requires generated Fallout 4 Papyrus headers"]
fn actual_limb_script_bytecode_preserves_loot_squish_and_dismemberment_conditions() {
    let root = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "B21_ConversionCreature",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../python/bacup_lib/script_additions/fnv_fo4/B21_ConversionCreature.psc"
            )),
        ),
        (
            "B21_ConversionClock",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../python/bacup_lib/script_additions/fnv_fo4/B21_ConversionClock.psc"
            )),
        ),
    ] {
        std::fs::write(root.path().join(format!("{name}.psc")), source).unwrap();
    }
    let imports = vec![
        root.path().to_string_lossy().into_owned(),
        std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap(),
    ];
    for kind in ["mantis", "nymph", "tumbleweed", "radiotumbleweed"] {
        let source = limb_script(kind).psc_text;
        let compiled = papyrus_core::compiler::compile_source(
            &source,
            &imports,
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(compiled.ok, "{kind}: {:?}\n{source}", compiled.diagnostics);
        let pex = papyrus_core::pex::parse_pex_bytes(&compiled.pex_bytes.unwrap()).unwrap();
        let mut values = HashMap::from([("self".into(), 1.0), ("aitimerid".into(), 0.0)]);
        if kind == "mantis" {
            values.extend([
                ("health".into(), 0.0),
                ("limb3".into(), 1.0),
                ("limb5".into(), 0.0),
                ("itemcount".into(), 2.0),
            ]);
            let calls = execute(event(&pex, "OnHit"), &mut values, 0.0);
            assert_eq!(calls.iter().filter(|call| *call == "removeitem").count(), 1);
            assert_eq!(calls.iter().filter(|call| *call == "additem").count(), 1);
            assert_eq!(values["doonce"], 1.0);
            let calls = execute(event(&pex, "OnHit"), &mut values, 0.0);
            assert!(
                !calls
                    .iter()
                    .any(|call| matches!(call.as_str(), "removeitem" | "additem"))
            );
        } else if kind == "nymph" {
            execute(event(&pex, "OnLoad"), &mut values, 0.0);
            values.extend([("interior".into(), 0.0), ("distance_result".into(), 20.0)]);
            assert!(!execute(event(&pex, "OnTimer"), &mut values, 0.0).contains(&"kill".into()));
            values.insert("interior".into(), 1.0);
            assert!(execute(event(&pex, "OnTimer"), &mut values, 0.0).contains(&"kill".into()));
            assert_eq!(values["alive"], 0.0);
        } else {
            assert!(
                execute(event(&pex, "OnHit"), &mut values, 0.0).contains(&"killwithtorso".into())
            );
            values.insert("aecombatstate".into(), 1.0);
            assert!(
                execute(event(&pex, "OnCombatStateChanged"), &mut values, 0.0)
                    .contains(&"killwithtorso".into())
            );
            assert!(execute(event(&pex, "OnDeath"), &mut values, 0.0).contains(&"disable".into()));
        }
    }
}

#[test]
#[ignore = "requires the CLI source creature audit and generated Fallout 4 Papyrus headers"]
fn actual_108_attached_fnv_creature_scripts_lower_and_compile() {
    let audit = std::path::PathBuf::from(std::env::var("B21_FNV_CREATURE_SCRIPT_AUDIT").unwrap());
    let read = |name: &str| -> Value {
        let text = std::fs::read_to_string(audit.join(name)).unwrap();
        serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()
    };
    let scripts = read("source_creature_scripts.json");
    let owners = read("source_creature_script_refs.json");
    let symbols = read("source_creature_script_symbols.json");
    let idles = read("source_creature_idle_symbols.json");
    assert_eq!(scripts["data"].as_array().unwrap().len(), 108);
    let symbols = symbols["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| (record["form_id"].as_str().unwrap(), record))
        .collect::<HashMap<_, _>>();
    let root = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "B21_ConversionCreature",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../python/bacup_lib/script_additions/fnv_fo4/B21_ConversionCreature.psc"
            )),
        ),
        (
            "B21_ConversionClock",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../python/bacup_lib/script_additions/fnv_fo4/B21_ConversionClock.psc"
            )),
        ),
    ] {
        std::fs::write(root.path().join(format!("{name}.psc")), source).unwrap();
    }
    let nellis = nellis_storage_script();
    std::fs::write(
        root.path()
            .join(format!("{}.psc", nellis.script_class_name)),
        nellis.psc_text,
    )
    .unwrap();
    let mut translated = Vec::new();
    let mut report = Vec::new();
    for script in scripts["data"].as_array().unwrap() {
        let local = script["form_id"].as_str().unwrap();
        let source_key = format!("{local}:FalloutNV.esm");
        let mut record = json!({"eid": script["eid"], "fields": [
            {"SCTX": script["fields"]["ScriptSource"]}, {"SCHR": script["fields"]["BasicScriptData"]}
        ], "__source_form_symbols": {}, "__source_idle_symbols": {}, "__mapped_form_keys": {}});
        let raw_refs = &script["fields"]["GlobalReference"];
        let refs = if let Some(values) = raw_refs.as_array() {
            values.iter().collect::<Vec<_>>()
        } else if raw_refs.is_object() {
            vec![raw_refs]
        } else {
            Vec::new()
        };
        for reference in refs {
            let reference = &reference["reference"];
            let ref_local = reference["object_id"].as_str().unwrap();
            assert_eq!(reference["plugin"], "FalloutNV.esm");
            let ref_key = format!("{ref_local}:FalloutNV.esm");
            record["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"SCRO": ref_key}));
            if ref_local == "000014" {
                continue;
            }
            let metadata = symbols[ref_local];
            if let Some(name) = metadata["eid"].as_str() {
                record["__source_form_symbols"][name.to_ascii_lowercase()] =
                    json!({"form_key": ref_key, "signature": metadata["signature"]});
            }
            record["__mapped_form_keys"][ref_key] = json!(format!("{ref_local}:CorpusTest.esp"));
        }
        let bytes = hex::decode(
            script["fields"]["ScriptSource"]["raw_hex"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let (source, _, _) = WINDOWS_1252.decode(&bytes);
        if let Ok(ast) = parse_script(&source) {
            for name in source_idle_argument_symbols(&ast) {
                let matches = idles["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|record| {
                        record["eid"]
                            .as_str()
                            .is_some_and(|eid| eid.eq_ignore_ascii_case(&name))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(matches.len(), 1, "actual source IDLE {name}");
                let local = matches[0]["form_id"].as_str().unwrap();
                let key = format!("{local}:FalloutNV.esm");
                record["__source_idle_symbols"][name] = json!(key);
                record["__mapped_form_keys"][key] = json!(format!("{local}:CorpusTest.esp"));
            }
        }
        let attachments = owners
            .as_array()
            .unwrap()
            .iter()
            .filter(|owner| owner["script"]["object_id"] == local)
            .map(|owner| ScptAttachmentEvidence {
                source_target_form_key: format!(
                    "{}:FalloutNV.esm",
                    owner["creature"].as_str().unwrap()
                ),
                target_signature: "CREA".into(),
            })
            .collect::<Vec<_>>();
        assert!(!attachments.is_empty());
        match translate_scpt_record_with_attachments(&record, "B21", &source_key, &attachments) {
            Ok(script) => {
                std::fs::write(root.path().join(format!("{}.psc", script.script_class_name)), &script.psc_text).unwrap();
                translated.push(script);
            }
            Err(error) => report.push(json!({"source": source_key, "eid": script["eid"], "stage": "source_lowering", "error": error.to_string()})),
        }
    }
    let imports = vec![
        root.path().to_string_lossy().into_owned(),
        std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap(),
    ];
    for script in &translated {
        let compiled = papyrus_core::compiler::compile_source(
            &script.psc_text,
            &imports,
            papyrus_core::profile::Game::Fo4,
            None,
        );
        if !compiled.ok {
            report.push(json!({"source": script.source_form_key, "eid": script.source_editor_id, "stage": "fo4_compiler", "error": format!("{:?}", compiled.diagnostics), "source_text": script.psc_text}));
        } else {
            let pex =
                papyrus_core::pex::parse_pex_bytes(compiled.pex_bytes.as_ref().unwrap()).unwrap();
            assert_eq!(pex.game_id, 2);
        }
    }
    std::fs::write(
        audit.join("actual_108_script_compile_report.json"),
        serde_json::to_vec_pretty(
            &json!({"source_count": 108, "lowered_count": translated.len(), "deferred_quest_capabilities": translated.iter().filter_map(|script| script.deferred_creature_quest.as_ref()).collect::<Vec<_>>(), "failures": report}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        report.is_empty(),
        "actual creature script failures: {}",
        serde_json::to_string_pretty(&report).unwrap()
    );
}
