use super::*;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

#[test]
fn converted_trap_bindings_require_only_the_bounded_source_family() {
    let interner = StringInterner::new();
    let mut record = Record::new(
        SigCode(*b"ACTI"),
        FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 1,
        },
    );
    let mut bytes = vec![6, 0, 2, 0, 2, 0];
    for script in ["TrapBear", "TrapHitBase"] {
        bytes.extend_from_slice(&(script.len() as u16).to_le_bytes());
        bytes.extend_from_slice(script.as_bytes());
        bytes.extend_from_slice(&[0, 0, 0]);
    }
    crate::fixups::quest_script_vmad::add_top_level_object_properties(&mut bytes, &mut |script| {
        Some(if script == "TrapBear" {
            vec![
                ("LightFoot".into(), 0x0105820C),
                ("LightFootTriggerPercent".into(), 0x01067194),
            ]
        } else {
            vec![("TrapDiseaseAtaxia".into(), 0x0110A24A)]
        })
    })
    .unwrap();
    let decode = |bytes: &[u8]| {
        esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(bytes, &["Fallout4.esm".into()], "Converted.esm", Some("ACTI")).unwrap()
    };
    let original = decode(&bytes);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"VMAD"),
        value: FieldValue::Bytes(bytes.into()),
    });
    assert!(required_target_scripts(&record).is_err());
    super::super::rewrite_attack_script_bindings(&mut record).unwrap();
    assert_eq!(
        required_target_scripts(&record).unwrap(),
        BTreeSet::from(["trapbear".into(), "traphitbase".into()])
    );
    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!()
    };
    let converted = decode(bytes);
    for index in 0..2 {
        assert_eq!(
            converted["Scripts"][index]["Properties"],
            original["Scripts"][index]["Properties"]
        );
    }
    validate_properties(&original, &converted, &mut |key| Ok(key.to_string())).unwrap();
    let mut broken = converted.clone();
    broken["Scripts"][0]["Properties"][0]["Value"] = serde_json::Value::Null;
    assert!(validate_properties(&original, &broken, &mut |key| Ok(key.to_string())).is_err());
    assert!(validate_properties(&original, &converted, &mut |_| Err(
        "dependency not emitted".into()
    ))
    .is_err());
    assert!(source_parent("dunKarthspirePlatePuzzleSCRIPT").is_none());
}

#[test]
fn magic_callbacks_rearm_in_active_and_inactive_states() {
    let mut ast = papyrus_core::parser::parse_script("Scriptname B21_SkyrimTrapTriggerBase extends ObjectReference\nEvent OnCellAttach()\nBlockActivation(True)\nEndEvent\nState Active\nEvent OnMagicEffectApply(ObjectReference caster, MagicEffect effect)\nActivate(Self)\nGotoState(\"Inactive\")\nEndEvent\nEvent OnBeginState()\nEndEvent\nEndState\nAuto State Inactive\nEndState\n").ast.unwrap();
    adapt(&mut ast).unwrap();
    let event = &ast.states[0].events[0];
    assert_eq!(event.params.len(), 3);
    assert_eq!(event.params[1].name, "caster");
    assert_eq!(event.params[2].name, "effect");
    assert_eq!(ast.states[0].events[1].params[0].ty, "String");
    let output = papyrus_core::emitter::emit_script(&ast);
    assert_eq!(
        output.matches("RegisterForMagicEffectApplyEvent").count(),
        4
    );
    assert!(output.contains("GotoState(\"Inactive\")"));
    assert!(output.contains("BlockActivation(True)"));
}

fn check_perks(
    function: &papyrus_core::pex::PexFunctionPayload,
    enabled: bool,
    has_perk: bool,
    roll: f64,
    threshold: f64,
) -> bool {
    let key = |name: &str| {
        name.trim_start_matches("::")
            .trim_end_matches("_var")
            .to_ascii_lowercase()
    };
    let mut values = HashMap::from([
        ("checkforlightfootperk".into(), f64::from(u8::from(enabled))),
        ("checkforlighthandsperk".into(), 0.0),
        ("triggerref".into(), 1.0),
    ]);
    let mut pc = 0i64;
    for _ in 0..1000 {
        let instruction = &function.instructions[pc as usize];
        let name = |index: usize| instruction.args[index].data.as_str().unwrap();
        let value = |index: usize| {
            let arg = &instruction.args[index];
            if arg.value_type == 1 {
                *values.get(&key(name(index))).unwrap_or(&0.0)
            } else if let Some(value) = arg.data.as_bool() {
                f64::from(u8::from(value))
            } else {
                arg.data.as_f64().unwrap_or_default()
            }
        };
        let mut destination = 0;
        let result = match instruction.opcode {
            0 => None,
            0x0a => Some(f64::from(u8::from(value(1) == 0.0))),
            0x0d | 0x0e => Some(value(1)),
            0x0f => Some(f64::from(u8::from(value(1) == value(2)))),
            0x11 => Some(f64::from(u8::from(value(1) <= value(2)))),
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
                destination = 2;
                Some(match name(0).to_ascii_lowercase().as_str() {
                    "hasperk" => f64::from(u8::from(has_perk)),
                    "getvalue" => threshold,
                    method => panic!("unexpected checkPerks native {method}"),
                })
            }
            0x18 => {
                assert!(
                    name(0).eq_ignore_ascii_case("Utility")
                        && name(1).eq_ignore_ascii_case("RandomFloat")
                );
                destination = 2;
                Some(roll)
            }
            0x1a => return value(0) != 0.0,
            0x1c => {
                destination = 2;
                Some(*values.get(&key(name(0))).unwrap_or(&0.0))
            }
            opcode => panic!("unsupported checkPerks opcode {opcode:#x}"),
        };
        if let Some(result) = result {
            values.insert(key(name(destination)), result);
        }
        pc += 1;
    }
    panic!("checkPerks did not return");
}

#[test]
#[ignore = "requires extracted Skyrim PEX and generated Fallout 4 headers"]
fn actual_lightfoot_trap_corpus_preserves_predicates_and_publishes_fo4_pex() {
    let source_root = std::env::var("B21_SKYRIM_ATTACK_SCRIPTS_SOURCE").unwrap();
    let imports = vec![std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()];
    let root = tempfile::tempdir().unwrap();
    let names = SCRIPT_PORTS
        .iter()
        .map(|(source, _)| source.to_ascii_lowercase())
        .collect();
    let staged = super::super::stage_attack_scripts(
        &names,
        Path::new(&source_root),
        &root.path().join("stage"),
        &imports,
        None,
    )
    .unwrap();
    assert_eq!(staged.len(), 6);
    super::super::publish_attack_scripts(&root.path().join("stage"), &root.path().join("mod"))
        .unwrap();
    for class in ["trapbear", "traptriggerbase"] {
        let original = papyrus_core::pex::parse_pex_file(
            &Path::new(&source_root)
                .join(format!("Scripts/{class}.pex"))
                .to_string_lossy(),
        )
        .unwrap();
        let converted =
            papyrus_core::pex::parse_pex_file(&staged[class].source_path.to_string_lossy())
                .unwrap();
        assert_eq!(converted.game_id, 2);
        for pex in [&original, &converted] {
            let method = pex.objects[0]
                .states
                .iter()
                .flat_map(|state| &state.functions)
                .find(|function| function.name.eq_ignore_ascii_case("checkPerks"))
                .unwrap();
            for (enabled, perk, roll, threshold, expected) in [
                (false, true, 0.0, 100.0, true),
                (true, false, 0.0, 100.0, true),
                (true, true, 0.0, 100.0, false),
                (true, true, 100.0, 100.0, false),
                (true, true, 49.0, 50.0, false),
                (true, true, 50.0, 50.0, false),
                (true, true, 51.0, 50.0, true),
            ] {
                assert_eq!(
                    check_perks(method, enabled, perk, roll, threshold),
                    expected,
                    "{class}: {enabled}/{perk}/{roll}/{threshold}"
                );
            }
        }
    }
}
