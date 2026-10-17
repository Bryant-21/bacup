use super::*;
use std::collections::HashMap;

fn key(value: &str) -> String {
    value
        .strip_prefix("::")
        .unwrap_or(value)
        .trim_end_matches("_var")
        .to_ascii_lowercase()
}

fn execute(function: &PexFunctionPayload, variables: &[(&str, i64)]) -> HashMap<String, i64> {
    execute_trace(function, variables).0
}

fn execute_trace(
    function: &PexFunctionPayload,
    variables: &[(&str, i64)],
) -> (HashMap<String, i64>, Vec<(String, Vec<i64>)>) {
    let mut values: HashMap<_, _> = variables
        .iter()
        .map(|(name, value)| (key(name), *value))
        .collect();
    let mut pc = 0i64;
    let mut calls = Vec::new();
    for _ in 0..2000 {
        if pc as usize == function.instructions.len() {
            return (values, calls);
        }
        let instruction = &function.instructions[pc as usize];
        let raw_name = |index: usize| instruction.args[index].data.as_str().unwrap();
        let value = |index: usize| {
            let arg = &instruction.args[index];
            if arg.value_type == 1 {
                *values.get(&key(raw_name(index))).unwrap_or(&0)
            } else if let Some(value) = arg.data.as_bool() {
                i64::from(value)
            } else {
                arg.data.as_f64().unwrap_or_default() as i64
            }
        };
        let mut destination = 0;
        let output = match instruction.opcode {
            0 => None,
            1 | 2 => Some(value(1) + value(2)),
            3 | 4 => Some(value(1) - value(2)),
            5 | 6 => Some(value(1) * value(2)),
            7 | 8 => Some(value(1) / value(2)),
            0x0b | 0x0c => Some(-value(1)),
            0x0d => Some(value(1)),
            0x0e => {
                let boolean = function.locals.iter().any(|local| {
                    local.name == raw_name(0) && local.ty.eq_ignore_ascii_case("bool")
                });
                Some(if boolean {
                    i64::from(value(1) != 0)
                } else {
                    value(1)
                })
            }
            0x0a => Some(i64::from(value(1) == 0)),
            0x0f => Some(i64::from(value(1) == value(2))),
            0x10 => Some(i64::from(value(1) < value(2))),
            0x11 => Some(i64::from(value(1) <= value(2))),
            0x12 => Some(i64::from(value(1) > value(2))),
            0x13 => Some(i64::from(value(1) >= value(2))),
            0x14 => {
                pc += value(0);
                continue;
            }
            0x15 if value(0) != 0 => {
                pc += value(1);
                continue;
            }
            0x16 if value(0) == 0 => {
                pc += value(1);
                continue;
            }
            0x15 | 0x16 => None,
            0x17 => {
                let method = raw_name(0).to_ascii_lowercase();
                destination = 2;
                match method.as_str() {
                    "getvalue" => Some(*values.get("enabled").unwrap()),
                    "getparentcell" => Some(if value(1) == 10 {
                        *values.get("targetcell").unwrap()
                    } else {
                        1
                    }),
                    "getpositionx" => Some(7),
                    "getpositiony" => Some(8),
                    "getpositionz" => Some(9),
                    "getdistance" => Some(200),
                    "setposition" | "remotecast" => {
                        calls.push((method, vec![value(1), value(4), value(5), value(6)]));
                        None
                    }
                    "registerforsingleupdate" | "starttimer" => {
                        calls.push(("schedule".into(), vec![value(4)]));
                        None
                    }
                    "disable" | "delete" => {
                        calls.push((method, vec![value(1)]));
                        None
                    }
                    "canceltimer" => None,
                    _ => panic!("unexpected native call: {instruction:?}"),
                }
            }
            0x19 => {
                assert!(
                    raw_name(0).eq_ignore_ascii_case("Utility")
                        && matches!(
                            raw_name(1).to_ascii_lowercase().as_str(),
                            "wait" | "randomfloat"
                        ),
                    "unexpected native call: {instruction:?}"
                );
                destination = 2;
                raw_name(1).eq_ignore_ascii_case("RandomFloat").then_some(0)
            }
            0x1a => return (values, calls),
            0x1c => {
                destination = 2;
                Some(*values.get(&key(raw_name(0))).unwrap_or(&0))
            }
            0x1d => Some(value(2)),
            opcode => panic!("unsupported source-semantic opcode {opcode:#x}: {instruction:?}"),
        };
        if let Some(value) = output {
            values.insert(key(raw_name(destination)), value);
        }
        pc += 1;
        if pc as usize == function.instructions.len() {
            return (values, calls);
        }
    }
    panic!("{} did not terminate", function.name);
}

#[test]
#[ignore = "requires extracted Skyrim PEX and generated Fallout 4 headers"]
fn actual_storm_bytecode_keeps_cast_target_gates_and_cleanup() {
    let source_root = std::env::var("B21_SKYRIM_ATTACK_SCRIPTS_SOURCE").unwrap();
    let imports = vec![std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()];
    let root = tempfile::tempdir().unwrap();
    let name = "magicprojectilestormscript";
    let original = papyrus_core::pex::parse_pex_file(
        &Path::new(&source_root)
            .join(format!("Scripts/{name}.pex"))
            .to_string_lossy(),
    )
    .unwrap();
    let staged = stage_attack_scripts(
        &BTreeSet::from([name.into()]),
        Path::new(&source_root),
        root.path(),
        &imports,
        None,
    )
    .unwrap();
    let ported =
        papyrus_core::pex::parse_pex_file(&staged[name].source_path.to_string_lossy()).unwrap();
    for (enabled, miss, cell) in [(1, 0, 1), (1, 1, 1), (0, 0, 1), (1, 0, 0)] {
        let inputs = [
            ("enabled", enabled),
            ("bMissOnPurpose", miss),
            ("targetcell", cell),
            ("TargetActor", 10),
            ("ActivatorRef", 20),
            ("CasterActor", 30),
            ("ActivatorTargetRef", 40),
            ("player", 50),
            ("SpellRef", 60),
            ("KeepUpdating", 1),
            ("DeletAtEnd", 1),
            ("bSuperMiss", 1),
            ("PosX", 100),
            ("PosY", 200),
            ("PosZ", 4096),
            ("fRecast", 1),
        ];
        let source = execute_trace(function(&original, "OnUpdate"), &inputs);
        let target = execute_trace(function(&ported, "OnUpdate"), &inputs);
        assert_eq!(
            target.1, source.1,
            "enabled={enabled}, miss={miss}, cell={cell}"
        );
        if enabled == 0 {
            assert!(target.1.is_empty());
        } else if cell == 0 {
            assert_eq!(target.0["keepupdating"], 0);
            assert_eq!(
                target.1,
                [
                    ("disable".into(), vec![20]),
                    ("delete".into(), vec![20]),
                    ("disable".into(), vec![40]),
                    ("delete".into(), vec![40])
                ]
            );
        } else {
            assert!(target.1.contains(&(
                "remotecast".into(),
                vec![60, 20, 30, if miss == 1 { 40 } else { 10 }]
            )));
            assert!(target.1.iter().any(|(method, _)| method == "schedule"));
            if miss == 1 {
                assert!(
                    target
                        .1
                        .contains(&("setposition".into(), vec![40, 607, 608, 9]))
                );
            }
        }
    }
    for pex in [&original, &ported] {
        let (state, calls) = execute_trace(
            function(pex, "OnEffectFinish"),
            &[
                ("DeletAtEnd", 1),
                ("ActivatorRef", 20),
                ("ActivatorTargetRef", 40),
                ("KeepUpdating", 1),
            ],
        );
        assert_eq!(state["keepupdating"], 0);
        assert_eq!(
            calls,
            [
                ("disable".into(), vec![20]),
                ("delete".into(), vec![20]),
                ("disable".into(), vec![40]),
                ("delete".into(), vec![40])
            ]
        );
    }
}

fn function<'a>(pex: &'a PexFilePayload, name: &str) -> &'a PexFunctionPayload {
    pex.objects[0]
        .states
        .iter()
        .flat_map(|state| &state.functions)
        .find(|function| function.name.eq_ignore_ascii_case(name))
        .unwrap()
}

#[test]
#[ignore = "requires extracted Skyrim PEX and generated Fallout 4 headers"]
fn actual_deer_wisp_and_mg01_control_flow_preserves_source_results() {
    let source_root = std::env::var("B21_SKYRIM_ATTACK_SCRIPTS_SOURCE").unwrap();
    let imports = vec![std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()];
    let root = tempfile::tempdir().unwrap();
    for (class, method, cases, result_name, expected) in [
        (
            "DLC1DeerFXGlowScript",
            "OnUpdate",
            vec![vec![("bHit", 1), ("iHitCounter", 6)]],
            "iHitCounter",
            vec![0],
        ),
        (
            "MG01QuestScript",
            "StartPlayerHitTimer",
            vec![vec![
                ("TimerInit", 1),
                ("TimerCounter", 0),
                ("PlayerHit", 0),
            ]],
            "TimerCounter",
            vec![10],
        ),
        (
            "wispActorScript",
            "OnActivate",
            vec![
                vec![
                    ("actronaut", 1),
                    ("orb01", 1),
                    ("orb02", 0),
                    ("orb03", 0),
                    ("liveLights", 3),
                ],
                vec![
                    ("actronaut", 2),
                    ("orb01", 1),
                    ("orb02", 0),
                    ("orb03", 0),
                    ("liveLights", 3),
                ],
                vec![
                    ("actronaut", 2),
                    ("orb01", 1),
                    ("orb02", 1),
                    ("orb03", 0),
                    ("liveLights", 3),
                ],
            ],
            "liveLights",
            vec![2, 3, 2],
        ),
    ] {
        let original = papyrus_core::pex::parse_pex_file(
            &Path::new(&source_root)
                .join(format!("Scripts/{class}.pex"))
                .to_string_lossy(),
        )
        .unwrap();
        let staged = stage_attack_scripts(
            &BTreeSet::from([class.to_ascii_lowercase()]),
            Path::new(&source_root),
            &root.path().join(class),
            &imports,
            None,
        )
        .unwrap();
        let artifact = staged.get(&class.to_ascii_lowercase()).unwrap();
        let ported =
            papyrus_core::pex::parse_pex_file(&artifact.source_path.to_string_lossy()).unwrap();
        for (inputs, expected) in cases.iter().zip(expected) {
            for pex in [&original, &ported] {
                let result = execute(function(pex, method), inputs);
                assert_eq!(
                    result[&key(result_name)],
                    expected,
                    "{class}.{method}: {inputs:?}"
                );
            }
        }
    }
}
