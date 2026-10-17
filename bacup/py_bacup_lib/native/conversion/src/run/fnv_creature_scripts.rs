use super::*;
use crate::fnv_legacy_scripting::vmad::{
    VmadTarget, build_scpt_vmad_intents_with_compiled_evidence, synthesize_vmad,
};
use crate::source_rig::{
    CreatureNpcScriptAttachment, CreatureSourceRecordReference, CreatureTargetRecordReference,
    SourceCreatureIdentity, TargetFormKey,
};

impl ConversionRun {
    pub(crate) fn compiled_fnv_creature_script_attachment(
        &self,
        source_scpt: FormKey,
        target_npc: FormKey,
    ) -> Result<CreatureNpcScriptAttachment, RunError> {
        if self.source != Game::Fnv
            || self.target != Game::Fo4
            || !self.is_admitted_creature_dependency(source_scpt, SigCode(*b"SCPT"))
        {
            return Err(RunError::InvalidConfig(
                "creature script attachment requires an admitted FNV SCPT".into(),
            ));
        }
        let source_text = form_key_to_legacy_str(source_scpt, &self.interner).ok_or_else(|| {
            RunError::InvalidConfig("creature script source plugin is unresolved".into())
        })?;
        let target_text = form_key_to_legacy_str(target_npc, &self.interner).ok_or_else(|| {
            RunError::InvalidConfig("creature script target plugin is unresolved".into())
        })?;
        let intents = build_scpt_vmad_intents_with_compiled_evidence(
            &[VmadTarget {
                target_form_key: target_text,
                source_scpt_form_key: source_text.clone(),
            }],
            &self.fnv_pending_vmad_scripts,
            &self.fnv_quest_runtime_compiler_evidence,
        )
        .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
        let [intent] = intents.as_slice() else {
            return Err(RunError::InvalidConfig(format!(
                "creature script {source_text} has no compiled binding for its generated NPC"
            )));
        };
        if !self.fnv_pending_vmad_scripts.iter().any(|script| {
            script.script_class_name.eq_ignore_ascii_case(&intent.script_class_name)
                && matches!(script.papyrus_type, crate::fnv_legacy_scripting::script_synthesizer::PapyrusType::Actor | crate::fnv_legacy_scripting::script_synthesizer::PapyrusType::ObjectReference)
        }) {
            return Err(RunError::InvalidConfig(format!("creature script {source_text} cannot attach to an NPC")));
        }
        let mut dependencies = Vec::new();
        for property in &intent.properties {
            let text = property
                .value
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "creature script property {} lacks its target FormKey",
                        property.name
                    ))
                })?;
            let key = parse_legacy_form_key_text(text, &self.interner)
                .map_err(RunError::InvalidConfig)?;
            let signature = if let Some(record) = self.read_target_record_if_available(key)? {
                record.sig.as_str().to_string()
            } else if property.prop_type.eq_ignore_ascii_case("ActorValue")
                && crate::fnv_legacy_scripting::actor_value_bindings::native_actor_value_binding(
                    &property.name,
                )
                .is_some_and(|(_, expected)| expected.eq_ignore_ascii_case(text))
            {
                "AVIF".into()
            } else {
                return Err(RunError::InvalidConfig(format!(
                    "creature script property {} target {text} is unavailable",
                    property.name
                )));
            };
            let plugin = self.interner.resolve(key.plugin).ok_or_else(|| {
                RunError::InvalidConfig("creature script property plugin is unresolved".into())
            })?;
            dependencies.push(CreatureTargetRecordReference::new(
                signature,
                TargetFormKey::new(key.local, plugin),
            ));
        }
        let (plugin_name, target_masters) = plugin_context_for_handle(self.target_handle_id)?;
        if !self
            .interner
            .resolve(target_npc.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&plugin_name))
        {
            return Err(RunError::InvalidConfig(
                "generated creature script target must belong to the output plugin".into(),
            ));
        }
        let mut payload =
            synthesize_vmad(intent).map_err(|error| RunError::InvalidConfig(error.to_string()))?;
        payload["Version"] = serde_json::json!(6);
        let target_vmad = esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload(
            &payload,
            &target_masters,
            &plugin_name,
        )
        .ok_or_else(|| {
            RunError::InvalidConfig(format!(
                "cannot encode creature script {source_text} with the actual target masters"
            ))
        })?;
        Ok(CreatureNpcScriptAttachment {
            source_script: CreatureSourceRecordReference {
                source_identity: SourceCreatureIdentity {
                    namespace: self.source.as_str().into(),
                    plugin: self.interner.resolve(source_scpt.plugin).unwrap().into(),
                    local_form_id: source_scpt.local,
                },
                signature: "SCPT".into(),
            },
            target_vmad,
            dependencies,
            target_masters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store2::source::test_fixture::{group, plugin, record, subrecord};
    use esp_authoring_core::plugin_runtime::{
        parse_plugin_file, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_new_native, plugin_handle_store_ref,
    };

    fn load_source_handle(path: &std::path::Path, game: &str) -> u64 {
        let parsed = parse_plugin_file(&path.to_string_lossy(), Some(game.to_string()), true).unwrap();
        let handle = plugin_handle_new_native(&parsed.plugin_name, Some(game)).unwrap();
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle).unwrap();
        slot.parsed = parsed;
        slot.invalidate_sections();
        handle
    }
    fn fresh_target_handle() -> u64 {
        plugin_handle_new_native("Out.esm", Some("fo4")).unwrap()
    }
    #[test]
    #[ignore = "requires generated Fallout 4 Papyrus headers"]
    fn generated_creature_script_vmad_requires_compilation_and_keeps_exact_master_identities() {
        use crate::fnv_legacy_scripting::script_synthesizer::{
            PapyrusType, ScriptCompileStatus, ScriptTerminalStatus, TranslatedProperty,
            TranslatedScript,
        };
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, plugin(&[], &[])).unwrap();
        let source = load_source_handle(&source_path, "fnv");
        let mut masters = Vec::new();
        for name in ["First.esm", "Second.esm"] {
            let path = temp.path().join(name);
            std::fs::write(
                &path,
                plugin(
                    &["Fallout4.esm"],
                    &[group(
                        b"FLST",
                        0,
                        &[record(
                            b"FLST",
                            0x01001000,
                            0,
                            &subrecord(b"EDID", b"B21_ExactList\0"),
                        )],
                    )],
                ),
            )
            .unwrap();
            masters.push(load_source_handle(&path, "fo4"));
        }
        let target = fresh_target_handle();
        for name in ["Fallout4.esm", "Second.esm", "First.esm"] {
            plugin_handle_add_master_native(target, name, None).unwrap();
        }
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: masters.clone(),
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                target_master_names: vec![
                    "Fallout4.esm".into(),
                    "Second.esm".into(),
                    "First.esm".into(),
                ],
                ..Default::default()
            },
        })
        .unwrap();
        with_run(id, |run| {
        let source_key = FormKey { local: 0x1001, plugin: run.interner.intern("FalloutNV.esm") };
        let target_key = FormKey { local: 0x2000, plugin: run.interner.intern("Out.esm") };
        run.install_creature_dependency_admissions(vec![CreatureDependencyAdmission {
            source_form_key: source_key, source_signature: SigCode(*b"SCPT"), owner_family_ids: vec!["probe".into()], legacy_idle: None,
        }])?;
        let psc = "ScriptName B21_S_001001 extends Actor\nFormList Property FirstList Auto\nFormList Property SecondList Auto\nEvent OnLoad()\nEndEvent\n";
        run.fnv_pending_vmad_scripts.push(TranslatedScript {
            source_editor_id: "B21_ScriptProbe".into(), source_form_key: "001001:FalloutNV.esm".into(), script_class_name: "B21_S_001001".into(), papyrus_type: PapyrusType::Actor, psc_text: psc.into(),
            properties: [("FirstList", "First.esm"), ("SecondList", "Second.esm")].into_iter().map(|(name, plugin)| TranslatedProperty {
                source_name: name.into(), papyrus_name: name.into(), papyrus_type: "FormList".into(), source_form_key: format!("001000:{plugin}"), target_form_key: Some(format!("001000:{plugin}")),
            }).collect(),
            package_data_aliases: vec![], dedicated_topics: vec![], deferred_creature_quest: None,
            compile_status: ScriptCompileStatus::SourceGeneratedPendingCompile, terminal_status: ScriptTerminalStatus::SourceGeneratedPendingCompile,
        });
        assert!(run.compiled_fnv_creature_script_attachment(source_key, target_key).is_err());
        let compiled = papyrus_core::compiler::compile_source(psc, &[std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()], papyrus_core::profile::Game::Fo4, None);
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        run.fnv_quest_runtime_compiler_evidence.push(crate::fnv_legacy_scripting::vmad::CompiledScriptEvidence { class_name: "B21_S_001001".into(), compiled_success: true, relative_pex_path: "Scripts/B21_S_001001.pex".into() });
        let attachment = run.compiled_fnv_creature_script_attachment(source_key, target_key)?;
        assert_eq!(attachment.target_masters, ["Fallout4.esm", "Second.esm", "First.esm"]);
        assert_eq!(attachment.dependencies.len(), 2);
        let decoded = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(&attachment.target_vmad, &attachment.target_masters, "Out.esm", Some("NPC_")).unwrap();
        for (index, plugin) in [(0, "First.esm"), (1, "Second.esm")] {
            let reference = &decoded["Scripts"][0]["Properties"][index]["Value"]["FormID"]["reference"];
            assert_eq!(reference["plugin"], plugin, "{decoded}");
            assert_eq!(reference["object_id"], "001000", "{decoded}");
        }
        assert_eq!(u16::from_le_bytes(attachment.target_vmad[..2].try_into().unwrap()), 6);
        run.fnv_pending_vmad_scripts[0].properties[0].target_form_key = Some("001001:First.esm".into());
        assert!(run.compiled_fnv_creature_script_attachment(source_key, target_key).is_err());
        Ok::<_, RunError>(())
    }).unwrap();
        drop_run(id).unwrap();
        for handle in masters.into_iter().chain([source, target]) {
            plugin_handle_close_native(handle);
        }
    }
}
