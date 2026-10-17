use super::*;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::FieldEntry;
use crate::sym::StringInterner;
use papyrus_core::pex::{PexFilePayload, PexFunctionPayload};

mod source_semantics;

#[test]
fn hit_registration_survives_state_handlers_and_fo4_state_entry_arguments() {
    let parsed = papyrus_core::parser::parse_script(
        "Scriptname B21_SkyrimEffectShaderOnHit extends ActiveMagicEffect\nEvent OnEffectStart(Actor target, Actor caster)\nEndEvent\nAuto State Waiting\nEvent OnHit(ObjectReference aggressor, Form source, Projectile projectile, Bool power, Bool sneak, Bool bash, Bool blocked)\nGotoState(\"Done\")\nEndEvent\nEndState\nState Done\nEvent OnBeginState()\nEndEvent\nEndState\n",
    );
    assert!(parsed.errors.is_empty());
    let mut ast = parsed.ast.unwrap();
    ports::adapt(&mut ast).unwrap();
    let hit = &ast.states[0].events[0];
    assert_eq!(hit.params.len(), 9);
    assert_eq!(hit.params[1].name, "aggressor");
    assert_eq!(hit.params[7].name, "blocked");
    assert_eq!(ast.states[1].events[0].params[0].ty, "String");
    let source = papyrus_core::emitter::emit_script(&ast);
    assert_eq!(source.matches("RegisterForHitEvent").count(), 3);
    assert!(source.contains("GotoState(\"Done\")"));
    assert!(ast.events.iter().any(|event| event.name == "OnHit"));
}

#[test]
fn script_actor_values_use_target_master_indexes_and_allocated_magicka() {
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    let interner = StringInterner::new();
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            source_plugin_name: "Skyrim.esm".into(),
            output_plugin_name: "Converted.esm".into(),
            target_master_names: vec!["DLCRobot.esm".into(), "Fallout4.esm".into()],
            ..Default::default()
        },
        &interner,
    );
    let magicka = super::super::magic_resources::magic_resource_source_key(
        &FieldValue::String(interner.intern("Magicka")),
        &interner,
    )
    .unwrap();
    mapper.add_mapping(
        magicka,
        FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 0x8000,
        },
    );
    let mut record = Record::new(
        SigCode(*b"MGEF"),
        FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 1,
        },
    );
    let script = "B21_SkyrimWispActor";
    let mut vmad = vec![6, 0, 2, 0, 1, 0];
    vmad.extend_from_slice(&(script.len() as u16).to_le_bytes());
    vmad.extend_from_slice(script.as_bytes());
    vmad.extend_from_slice(&[0, 0, 0]);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"VMAD"),
        value: FieldValue::Bytes(vmad.into()),
    });
    bind_attack_script_actor_values(&mut record, &mapper, &interner).unwrap();
    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!()
    };
    let decoded = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(bytes, &["DLCRobot.esm".into(), "Fallout4.esm".into()], "Converted.esm", Some("MGEF")).unwrap();
    let properties = decoded["Scripts"][0]["Properties"].as_array().unwrap();
    assert_eq!(properties.len(), 4);
    for (index, plugin, local) in [
        (0, "Fallout4.esm", "0002D4"),
        (1, "Converted.esm", "008000"),
        (2, "Fallout4.esm", "000306"),
        (3, "Fallout4.esm", "000307"),
    ] {
        assert_eq!(
            properties[index]["Value"]["FormID"]["reference"]["plugin"],
            plugin
        );
        assert_eq!(
            properties[index]["Value"]["FormID"]["reference"]["object_id"],
            local
        );
        assert_eq!(properties[index]["Value"]["Alias"], -1);
    }
}

fn headers(root: &Path) -> Vec<String> {
    for (name, source) in [
        ("ScriptObject", "Scriptname ScriptObject Native Hidden"),
        ("Form", "Scriptname Form extends ScriptObject Native Hidden"),
        (
            "ObjectReference",
            "Scriptname ObjectReference extends Form Native Hidden\nFunction PushActorAway(Actor target, Float force) Native\nFloat Function GetDistance(ObjectReference target) Native",
        ),
        (
            "Actor",
            "Scriptname Actor extends ObjectReference Native Hidden",
        ),
        (
            "ActiveMagicEffect",
            "Scriptname ActiveMagicEffect extends ScriptObject Native Hidden\nEvent OnEffectStart(Actor target, Actor caster)\nEndEvent\nEvent OnEffectFinish(Actor target, Actor caster)\nEndEvent",
        ),
        (
            "Game",
            "Scriptname Game Native Hidden\nActor Function GetPlayer() Global Native",
        ),
        (
            "VisualEffect",
            "Scriptname VisualEffect extends Form Native Hidden\nFunction Play(ObjectReference target, Float time = -1.0, ObjectReference facing = None) Native\nFunction Stop(ObjectReference target) Native",
        ),
        (
            "ImageSpaceModifier",
            "Scriptname ImageSpaceModifier extends Form Native Hidden\nFunction Apply(Float strength = 1.0) Native",
        ),
    ] {
        std::fs::write(root.join(format!("{name}.psc")), source).unwrap();
    }
    vec![root.to_string_lossy().into_owned()]
}

fn stage_fixture(source_name: &str, source: &str) -> (tempfile::TempDir, PexFilePayload) {
    let root = tempfile::tempdir().unwrap();
    let imports = headers(root.path());
    let compiled = papyrus_core::compiler::compile_source(
        source,
        &imports,
        papyrus_core::profile::Game::SkyrimSe,
        None,
    );
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let source_dir = root.path().join("source/Scripts");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join(format!("{source_name}.pex")),
        compiled.pex_bytes.unwrap(),
    )
    .unwrap();
    let artifacts = stage_attack_scripts(
        &BTreeSet::from([source_name.to_string()]),
        &root.path().join("source"),
        &root.path().join("staged"),
        &imports,
        None,
    )
    .unwrap();
    let artifact = &artifacts[source_name];
    assert_eq!(
        artifact.runtime_path,
        format!("Scripts/{}.pex", target_script_name(source_name).unwrap())
    );
    let bytes = std::fs::read(&artifact.source_path).unwrap();
    let pex = papyrus_core::pex::parse_pex_bytes(&bytes).unwrap();
    assert_eq!(pex.game_id, 2);
    assert_eq!(
        pex.objects[0].parent.to_ascii_lowercase(),
        "activemagiceffect"
    );
    let receipt: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join(format!(
            "staged/{}.json",
            target_script_name(source_name).unwrap()
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        receipt["pex_blake3"],
        blake3::hash(&bytes).to_hex().to_string()
    );
    (root, pex)
}

fn event<'a>(pex: &'a PexFilePayload, name: &str) -> &'a PexFunctionPayload {
    pex.objects[0]
        .states
        .iter()
        .flat_map(|state| &state.functions)
        .find(|function| function.name.eq_ignore_ascii_case(name))
        .unwrap()
}

fn calls(function: &PexFunctionPayload, method: &str) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.opcode, 23 | 24 | 25)
                && instruction.args.iter().any(|argument| {
                    argument
                        .data
                        .as_str()
                        .is_some_and(|value| value.eq_ignore_ascii_case(method))
                })
        })
        .count()
}

#[test]
fn voice_push_preserves_force_property_and_caster_push_call() {
    let (_root, pex) = stage_fixture(
        "voicepusheffectscript",
        "Scriptname VoicePushEffectScript extends ActiveMagicEffect\nInt Property PushForce Auto\nEvent OnEffectStart(Actor Target, Actor Caster)\nCaster.PushActorAway(Target, PushForce as Float)\nEndEvent\n",
    );
    assert!(
        pex.objects[0]
            .properties
            .iter()
            .any(|property| property.name == "PushForce" && property.ty.eq_ignore_ascii_case("int"))
    );
    let start = event(&pex, "OnEffectStart");
    assert_eq!(calls(start, "PushActorAway"), 1);
    let instruction = start
        .instructions
        .iter()
        .find(|instruction| {
            instruction
                .args
                .first()
                .is_some_and(|argument| argument.data.as_str() == Some("PushActorAway"))
        })
        .unwrap();
    assert!(instruction.args.iter().any(|argument| {
        argument
            .data
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case("Caster"))
    }));
    assert!(instruction.args.iter().any(|argument| {
        argument
            .data
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case("Target"))
    }));
}

#[test]
fn absorb_fx_preserves_distance_fade_facing_effects_and_finish_cleanup() {
    let (_root, pex) = stage_fixture(
        "magicabsorbfxscript",
        include_str!("magicAbsorbFXScript.psc"),
    );
    let start = event(&pex, "OnEffectStart");
    let finish = event(&pex, "OnEffectFinish");
    assert_eq!(calls(start, "Apply"), 3);
    assert_eq!(calls(start, "Play"), 4);
    assert_eq!(calls(start, "GetDistance"), 1);
    assert_eq!(calls(finish, "Stop"), 2);
    for (name, expected) in [("fEffectDurationMax", 40.0), ("fImodFadeDistance", 2048.0)] {
        let property = pex.objects[0]
            .properties
            .iter()
            .find(|property| property.name.eq_ignore_ascii_case(name))
            .unwrap();
        let variable = pex.objects[0]
            .variables
            .iter()
            .find(|variable| variable.name.eq_ignore_ascii_case(&property.auto_var))
            .unwrap();
        assert_eq!(variable.data.data.as_f64(), Some(expected));
    }
    assert!(
        start
            .instructions
            .iter()
            .flat_map(|instruction| &instruction.args)
            .any(|argument| argument.data.as_f64() == Some(0.25))
    );
    assert!(
        start
            .instructions
            .iter()
            .flat_map(|instruction| &instruction.args)
            .any(|argument| argument.data.as_f64() == Some(2.0))
    );
}

#[test]
fn vmad_renames_only_class_names_preserving_properties_and_tail() {
    let interner = StringInterner::new();
    let mut record = Record::new(
        SigCode(*b"MGEF"),
        FormKey {
            plugin: interner.intern("Skyrim.esm"),
            local: 1,
        },
    );
    let mut payload = vec![5, 0, 2, 0, 1, 0];
    for text in ["voicepusheffectscript"] {
        payload.extend_from_slice(&(text.len() as u16).to_le_bytes());
        payload.extend_from_slice(text.as_bytes());
    }
    let body = [
        vec![0, 1, 0, 9, 0],
        b"PushForce".to_vec(),
        vec![3, 1, 10, 0, 0, 0],
    ]
    .concat();
    payload.extend_from_slice(&body);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"VMAD"),
        value: FieldValue::Bytes(payload.into()),
    });
    assert_eq!(
        required_attack_scripts(&record).unwrap(),
        BTreeSet::from(["voicepusheffectscript".into()])
    );
    rewrite_attack_script_bindings(&mut record).unwrap();
    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!()
    };
    assert_eq!(
        &bytes[8 + "B21_SkyrimVoicePushEffect".len()..],
        body.as_slice()
    );
    assert_eq!(&bytes[..6], &[5, 0, 2, 0, 1, 0]);
}

#[test]
fn publication_requires_matching_source_and_pex_artifacts() {
    let (root, _) = stage_fixture(
        "voicepusheffectscript",
        "Scriptname VoicePushEffectScript extends ActiveMagicEffect\nInt Property PushForce Auto\nEvent OnEffectStart(Actor Target, Actor Caster)\nCaster.PushActorAway(Target, PushForce as Float)\nEndEvent\n",
    );
    let staging = root.path().join("staged");
    let destination = root.path().join("mod");
    publish_attack_scripts(&staging, &destination).unwrap();
    validate_attack_script_publication(&staging, &destination).unwrap();
    let pex = destination.join("data/Scripts/B21_SkyrimVoicePushEffect.pex");
    std::fs::remove_file(&pex).unwrap();
    assert!(
        validate_attack_script_publication(&staging, &destination)
            .unwrap_err()
            .contains("required creature attack script")
    );
    std::fs::write(&pex, "stale").unwrap();
    assert!(
        validate_attack_script_publication(&staging, &destination)
            .unwrap_err()
            .contains("changed")
    );
}

#[test]
fn vmad_source_masters_and_output_mapper_preserve_same_local_identity() {
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use serde_json::json;
    let interner = StringInterner::new();
    let masters = vec!["Skyrim.esm".to_string()];
    let bytes = esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload(&json!({
        "Version": 5, "Object Format": 2,
        "Scripts": [{"ScriptName": "magicAbsorbFXScript", "Flags": 0, "Properties": [
            {"propertyName": "TargetVFX", "Type": "Object", "Flags": 1, "Value": {"Alias": -1, "FormID": {"reference": {"plugin": "Skyrim.esm", "object_id": "001234"}}}},
            {"propertyName": "CasterVFX", "Type": "Object", "Flags": 1, "Value": {"Alias": -1, "FormID": {"reference": {"plugin": "Dawnguard.esm", "object_id": "001234"}}}}
        ]}]
    }), &masters, "Dawnguard.esm").unwrap();
    let mut record = Record::new(
        SigCode(*b"MGEF"),
        FormKey {
            plugin: interner.intern("Dawnguard.esm"),
            local: 0x2000,
        },
    );
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"VMAD"),
        value: FieldValue::Bytes(bytes.into()),
    });
    let refs =
        attack_script_form_references(&record, "Dawnguard.esm", &masters, &interner).unwrap();
    let skyrim = FormKey {
        plugin: interner.intern("Skyrim.esm"),
        local: 0x1234,
    };
    let dawnguard = FormKey {
        plugin: interner.intern("Dawnguard.esm"),
        local: 0x1234,
    };
    assert_eq!(refs.len(), 2);
    assert!(refs.iter().any(|(_, target)| *target == skyrim));
    assert!(refs.iter().any(|(_, target)| *target == dawnguard));
    let mut mapper = FormKeyMapper::new(
        [],
        MapperOptions {
            source_plugin_name: "Dawnguard.esm".to_string(),
            source_master_names: masters,
            output_plugin_name: "Converted.esm".to_string(),
            target_master_names: vec!["Fallout4.esm".to_string()],
            ..Default::default()
        },
        &interner,
    );
    mapper.add_mapping(
        skyrim,
        FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 0x5000,
        },
    );
    mapper.add_mapping(
        dawnguard,
        FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 0x6000,
        },
    );
    rewrite_attack_script_bindings(&mut record).unwrap();
    mapper.rewrite_record(&mut record).unwrap();
    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!()
    };
    let decoded = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(bytes, &["Fallout4.esm".to_string()], "Converted.esm", Some("MGEF")).unwrap();
    let script = &decoded["Scripts"][0];
    assert_eq!(script["ScriptName"], "B21_SkyrimAbsorbFXEffect");
    for (index, expected) in [(0, "005000"), (1, "006000")] {
        let value = &script["Properties"][index]["Value"];
        assert_eq!(value["Alias"], -1);
        assert_eq!(value["FormID"]["reference"]["plugin"], "Converted.esm");
        assert_eq!(value["FormID"]["reference"]["object_id"], expected);
    }
}

#[test]
#[ignore = "requires extracted Skyrim PEX and generated Fallout 4 headers"]
fn actual_skyrim_attack_scripts_compile_and_publish_as_fo4() {
    let source = std::env::var("B21_SKYRIM_ATTACK_SCRIPTS_SOURCE").unwrap();
    let imports = vec![std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()];
    let root = tempfile::tempdir().unwrap();
    let names = BTreeSet::from(["voicepusheffectscript".into(), "magicabsorbfxscript".into()]);
    let staged = stage_attack_scripts(
        &names,
        Path::new(&source),
        &root.path().join("staged"),
        &imports,
        None,
    )
    .unwrap();
    assert_eq!(staged.len(), 2);
    publish_attack_scripts(&root.path().join("staged"), &root.path().join("mod")).unwrap();
    for artifact in staged.values() {
        let pex =
            papyrus_core::pex::parse_pex_bytes(&std::fs::read(&artifact.source_path).unwrap())
                .unwrap();
        let start = event(&pex, "OnEffectStart");
        if artifact.runtime_path.contains("VoicePush") {
            assert_eq!(calls(start, "PushActorAway"), 1);
        } else {
            assert_eq!(calls(start, "Play"), 4);
            assert_eq!(calls(event(&pex, "OnEffectFinish"), "Stop"), 2);
        }
    }
}

#[test]
#[ignore = "requires extracted Skyrim PEX and generated Fallout 4 headers"]
fn actual_skyrim_creature_ability_scripts_compile_as_fo4() {
    let source = std::env::var("B21_SKYRIM_ATTACK_SCRIPTS_SOURCE").unwrap();
    let imports = vec![std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()];
    let root = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for (name, _) in ports::SCRIPT_PORTS {
        if let Err(error) = stage_attack_scripts(
            &BTreeSet::from([name.to_ascii_lowercase()]),
            Path::new(&source),
            &root.path().join(name),
            &imports,
            None,
        ) {
            failures.push(error);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
