use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use conversion_native::skyrimse_fo4_runtime::papyrus::{
    SkyrimPscKind, SkyrimPscSourceArtifact, emit_script_sources,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct FixtureManifest {
    version: u32,
    classes: Vec<FixtureClass>,
}

#[derive(Deserialize)]
struct FixtureClass {
    class_name: String,
    fixture_source_path: String,
    kind: String,
    required: bool,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("skyrim_jzargo")
}

fn load_manifest() -> FixtureManifest {
    serde_json::from_str(&fs::read_to_string(fixture_root().join("manifest.json")).unwrap())
        .unwrap()
}

fn load_sources(manifest: &FixtureManifest) -> BTreeMap<String, String> {
    manifest
        .classes
        .iter()
        .map(|class| {
            (
                class.class_name.clone(),
                fs::read_to_string(fixture_root().join(&class.fixture_source_path)).unwrap(),
            )
        })
        .collect()
}

fn function_body<'a>(source: &'a str, function_name: &str) -> &'a str {
    let declaration = format!("Function {function_name}(");
    let declaration_start = source.find(&declaration).unwrap();
    let body_start = source[declaration_start..].find('\n').unwrap() + declaration_start + 1;
    let body_end = source[body_start..].find("EndFunction").unwrap() + body_start;
    source[body_start..body_end].trim_end()
}

fn artifact_kind(kind: &str) -> SkyrimPscKind {
    match kind {
        "quest-fragment" => SkyrimPscKind::QuestFragment,
        "info-fragment" => SkyrimPscKind::InfoFragment,
        "helper" => SkyrimPscKind::Helper,
        "alias-script" => SkyrimPscKind::AliasScript,
        _ => panic!("unexpected fixture PSC kind {kind}"),
    }
}

#[test]
fn exact_psc_fixture_is_emitted_by_the_generic_manifest_api() {
    let manifest = load_manifest();
    let sources = load_sources(&manifest);
    let expected_names = [
        "QF_MGRJzargoSpell01_000958B3",
        "MGRJzargoSpell01QuestScript",
        "MGRJzargoFlameEffectScript",
        "TIF__000958B2",
        "TIF__000967E2",
        "TIF__000C041C",
        "TIF__000C04FD",
        "TIF__000C0502",
        "TIF__000C0506",
        "TIF__000F1B29",
        "MGRApprenticeQGScript",
    ];

    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.classes.len(), 11);
    assert_eq!(
        manifest
            .classes
            .iter()
            .map(|class| class.class_name.as_str())
            .collect::<Vec<_>>(),
        expected_names
    );
    assert!(!sources.contains_key("TIF__000C04F0"));

    let mut unique = BTreeSet::new();
    let artifacts = manifest
        .classes
        .iter()
        .map(|class| {
            assert!(!class.class_name.is_empty());
            assert!(class.class_name.len() <= 38);
            assert!(unique.insert(class.class_name.to_ascii_lowercase()));
            let source = sources.get(&class.class_name).unwrap();
            assert_eq!(
                source.split_whitespace().nth(1),
                Some(class.class_name.as_str())
            );
            assert!(!source.contains("GotoState"));
            assert!(!source.contains("GetState"));
            SkyrimPscSourceArtifact {
                class_name: class.class_name.clone(),
                source: source.clone(),
                kind: artifact_kind(&class.kind),
                required: class.required,
            }
        })
        .collect::<Vec<_>>();

    let output = tempfile::tempdir().unwrap();
    let generated = emit_script_sources(output.path(), &artifacts).unwrap();
    assert_eq!(generated.len(), 11);
    for (row, class) in generated.iter().zip(&manifest.classes) {
        assert_eq!(row.class_name, class.class_name);
        assert_eq!(row.kind, class.kind);
        assert_eq!(row.required, class.required);
        assert_eq!(
            fs::read_to_string(output.path().join(&row.relative_source_path)).unwrap(),
            sources[&class.class_name]
        );
    }

    let qf = &sources["QF_MGRJzargoSpell01_000958B3"];
    assert!(qf.contains("ReferenceAlias Property Alias_Jzargo Auto Const"));
    assert!(qf.contains("Form Property Scroll01 Auto Const"));
    assert!(qf.contains("MusicType Property CompletionMusic Auto Const"));
    assert_eq!(
        function_body(qf, "Fragment_0"),
        "    SetObjectiveCompleted(10)\n    SetObjectiveDisplayed(20)\n    Game.GetPlayer().AddItem(Scroll01, 10)"
    );
    assert_eq!(
        function_body(qf, "Fragment_2"),
        "    SetObjectiveDisplayed(10)"
    );
    assert_eq!(
        function_body(qf, "Fragment_4"),
        "    SetObjectiveCompleted(20)\n    SetObjectiveDisplayed(30)"
    );
    assert_eq!(
        function_body(qf, "Fragment_6"),
        "    CompleteAllObjectives()\n    Alias_Jzargo.GetActorReference().SetRelationshipRank(Game.GetPlayer(), 1)\n    CompletionMusic.Add()\n    Stop()"
    );
    assert_eq!(
        function_body(qf, "Fragment_8"),
        "    FailAllObjectives()\n    Stop()"
    );

    let helper = &sources["MGRJzargoSpell01QuestScript"];
    assert!(helper.starts_with("ScriptName MGRJzargoSpell01QuestScript Extends Quest Conditional"));
    assert!(helper.contains("Int Property HelpAsked Auto"));
    assert_eq!(
        function_body(helper, "VCount"),
        "    ModObjectiveGlobal(1.0, MGRJ1Test, 20, -1.0, True, True, True)\n    If MGRJ1Test.Value == MGRJ1Total.Value\n        SetStage(30)\n    EndIf"
    );

    let effect = &sources["MGRJzargoFlameEffectScript"];
    assert!(effect.contains("GlobalVariable Property TestCast Auto Const"));
    assert!(effect.contains("If akTarget.HasKeyword(UndeadKeyword)"));
    assert!(effect.contains("akTarget.PlaceAtMe(FireballExplosion, 1, False, False)"));
    assert!(effect.contains("If Jzargo01.GetStage() == 20"));
    assert!(effect.contains("questScript.VCount()"));

    assert_eq!(
        sources["MGRApprenticeQGScript"],
        "ScriptName MGRApprenticeQGScript Extends ReferenceAlias\n\nQuest Property MGRQuest Auto Const\n\nEvent OnDeath(Actor akKiller)\n    MGRQuest.SetStage(255)\nEndEvent\n"
    );
    assert_eq!(
        function_body(&sources["TIF__000958B2"], "Fragment_0"),
        "    MGRJzargoSpell01QuestScript questScript = GetOwningQuest() as MGRJzargoSpell01QuestScript\n    questScript.HelpAsked = 2"
    );
    assert_eq!(
        function_body(&sources["TIF__000967E2"], "Fragment_0"),
        "    GetOwningQuest().SetStage(20)"
    );
    assert_eq!(
        function_body(&sources["TIF__000C041C"], "Fragment_0"),
        "    MGRJzargoSpell01QuestScript questScript = GetOwningQuest() as MGRJzargoSpell01QuestScript\n    questScript.HelpAsked = 1"
    );
    for class_name in ["TIF__000C04FD", "TIF__000C0502", "TIF__000C0506"] {
        assert_eq!(
            function_body(&sources[class_name], "Fragment_0"),
            "    GetOwningQuest().SetStage(200)"
        );
    }
    assert_eq!(
        function_body(&sources["TIF__000F1B29"], "Fragment_0"),
        "    GetOwningQuest().SetStage(255)"
    );
}

#[test]
fn vmad_fixture_preserves_discovered_attachment_and_property_intents() {
    let document: Value = serde_json::from_str(
        &fs::read_to_string(fixture_root().join("vmad_intents.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(document["version"], 1);
    let bindings = document["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 11);

    let by_class = bindings
        .iter()
        .map(|binding| (binding["class_name"].as_str().unwrap(), binding))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(by_class.len(), 11);
    assert!(!by_class.contains_key("TIF__000C04F0"));

    let qf = by_class["QF_MGRJzargoSpell01_000958B3"];
    assert_eq!(qf["carrier"]["signature"], "QUST");
    assert_eq!(qf["carrier"]["source_form_key"], "0958B3:Skyrim.esm");
    assert_eq!(qf["attachment"]["kind"], "quest-stage-fragment");
    assert_eq!(
        qf["attachment"]["stages"],
        serde_json::json!([10, 20, 30, 200, 255])
    );
    assert_eq!(qf["properties"][0]["target"]["kind"], "quest-alias");
    assert_eq!(qf["properties"][0]["target"]["alias_id"], 0);
    assert_eq!(
        qf["properties"][1]["target"]["source_form_key"],
        "0967E3:Skyrim.esm"
    );
    assert_eq!(
        qf["properties"][2]["target"]["source_form_key"],
        "07F810:Skyrim.esm"
    );

    let helper = by_class["MGRJzargoSpell01QuestScript"];
    assert_eq!(helper["properties"][2]["name"], "HelpAsked");
    assert_eq!(helper["properties"][2]["target"]["kind"], "literal");
    assert_eq!(helper["properties"][2]["target"]["value"], 0);
    assert_eq!(helper["properties"][3]["name"], "EffectTriggered");
    assert_eq!(helper["properties"][3]["target"]["value"], 0);

    let effect = by_class["MGRJzargoFlameEffectScript"];
    assert_eq!(effect["carrier"]["signature"], "MGEF");
    assert_eq!(effect["carrier"]["source_form_key"], "097EE1:Skyrim.esm");
    assert_eq!(
        effect["properties"][0]["target"]["source_form_key"],
        "097EE4:Skyrim.esm"
    );
    assert_eq!(
        effect["properties"][1]["target"]["source_form_key"],
        "0958B3:Skyrim.esm"
    );
    assert_eq!(
        effect["properties"][2]["target"]["source_form_key"],
        "097EE5:Skyrim.esm"
    );
    assert_eq!(
        effect["properties"][3]["target"]["source_form_key"],
        "013796:Skyrim.esm"
    );

    let alias = by_class["MGRApprenticeQGScript"];
    assert_eq!(alias["attachment"]["kind"], "quest-alias-script");
    assert_eq!(alias["attachment"]["alias_id"], 0);
    assert_eq!(alias["properties"][0]["name"], "MGRQuest");
    assert_eq!(alias["properties"][0]["target"]["kind"], "owning-quest");

    for class_name in [
        "TIF__000958B2",
        "TIF__000967E2",
        "TIF__000C041C",
        "TIF__000C04FD",
        "TIF__000C0502",
        "TIF__000C0506",
        "TIF__000F1B29",
    ] {
        assert_eq!(by_class[class_name]["carrier"]["signature"], "INFO");
        assert_eq!(by_class[class_name]["attachment"]["kind"], "info-fragment");
        assert_eq!(by_class[class_name]["properties"], serde_json::json!([]));
    }
}
