use super::*;

#[test]
#[ignore = "requires generated Fallout 4 Papyrus headers"]
fn admitted_creature_quest_script_requires_compiled_vmad_in_both_translation_paths() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("FalloutNV.esm");
    let mut quest = subrecord(b"EDID", b"VFreeformNellis\0");
    quest.extend(subrecord(b"SCRI", &0x0FED40u32.to_le_bytes()));
    quest.extend(subrecord(b"FULL", b"Nellis AFB: General Quests\0"));
    quest.extend(subrecord(b"DATA", &[17, 55, 0, 0, 0, 0, 0, 0]));
    let mut script = subrecord(b"EDID", b"VFreeformNellisScript\0");
    let mut header = [0u8; 20];
    header[16..18].copy_from_slice(&1u16.to_le_bytes());
    script.extend(subrecord(b"SCHR", &header));
    script.extend(subrecord(b"SCTX", include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../py_creation_lib/native/fnv_script/tests/fixtures/VFreeformNellisScript.geck"))));
    std::fs::write(
        &path,
        plugin(
            &[],
            &[
                group(b"QUST", 0, &[record(b"QUST", 0x0FED44, 0, &quest)]),
                group(b"SCPT", 0, &[record(b"SCPT", 0x0FED40, 0, &script)]),
            ],
        ),
    )
    .unwrap();
    for v2 in [false, true] {
        let source = load_source_handle(&path, "fnv");
        let target = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                preserve_source_ids: true,
                skip_record_signatures: vec!["QUST".into(), "SCPT".into(), "PACK".into()],
                legacy_pack_expected_counts: Some(LegacyPackExpectedCounts { fnv: 0, fo3: 0 }),
                ..Default::default()
            },
        })
        .unwrap();
        with_run(id, |run| {
            let plugin = run.interner.intern("FalloutNV.esm");
            run.install_creature_dependency_admissions(
                [("QUST", 0x0FED44), ("SCPT", 0x0FED40)]
                    .into_iter()
                    .map(|(signature, local)| CreatureDependencyAdmission {
                        source_form_key: FormKey { local, plugin },
                        source_signature: SigCode::from_str(signature).unwrap(),
                        owner_family_ids: vec!["nellis-ant".into()],
                        legacy_idle: None,
                    })
                    .collect(),
            )?;
            if v2 {
                run.translate_all_v2(&path)?;
            } else {
                run.translate_all()?;
            }
            run.validate_creature_dependency_accounting()?;
            assert!(run.validate_creature_dependency_publication().is_err());
            assert_eq!(run.fnv_scri_links.len(), 1, "v2={v2}");
            let output = temp.path().join(if v2 { "v2" } else { "v1" });
            let result = run
                .run_fnv_legacy_scripting_from_deferred(
                    "B21",
                    "FalloutNV.esm",
                    output.to_str().unwrap(),
                )
                .map_err(RunError::from)?;
            assert_eq!(result.translated_scripts.len(), 1);
            assert!(result.skipped_records.is_empty());
            assert!(
                run.reconcile_fnv_compiled_scripts(&[])
                    .unwrap_err()
                    .to_string()
                    .contains("compiled PEX/VMAD")
            );
            let script = &result.translated_scripts[0];
            let compiled = papyrus_core::compiler::compile_source(
                &script.psc_text,
                &[std::env::var("B21_FO4_PAPYRUS_HEADERS").unwrap()],
                papyrus_core::profile::Game::Fo4,
                None,
            );
            assert!(compiled.ok, "{:?}", compiled.diagnostics);
            std::fs::create_dir_all(output.join("Scripts")).unwrap();
            std::fs::write(
                output
                    .join("Scripts")
                    .join(format!("{}.pex", script.script_class_name)),
                compiled.pex_bytes.unwrap(),
            )
            .unwrap();
            let evidence = crate::fnv_legacy_scripting::vmad::CompiledScriptEvidence {
                class_name: script.script_class_name.clone(),
                compiled_success: true,
                relative_pex_path: format!("Scripts/{}.pex", script.script_class_name),
            };
            let (intents, attached, _, _) = run
                .reconcile_fnv_compiled_scripts(&[evidence])
                .map_err(RunError::from)?;
            assert!(attached);
            assert_eq!(intents.len(), 1);
            run.validate_creature_dependency_publication()?;
            let receipt = output.join("fnv_scripts.json");
            run.write_fnv_creature_script_publication(&output, &receipt)?;
            crate::fnv_legacy_scripting::creature_publication::validate(&output, &receipt).unwrap();
            std::fs::write(
                output
                    .join("Scripts")
                    .join(format!("{}.pex", script.script_class_name)),
                b"changed",
            )
            .unwrap();
            assert!(
                crate::fnv_legacy_scripting::creature_publication::validate(&output, &receipt)
                    .is_err()
            );
            Ok::<_, RunError>(())
        })
        .unwrap();
        let quests = handle_records(target)
            .into_iter()
            .filter(|record| record.signature == "QUST")
            .collect::<Vec<_>>();
        assert_eq!(quests.len(), 1);
        assert!(
            quests[0]
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == "VMAD")
        );
        assert!(
            !handle_records(target)
                .iter()
                .any(|record| record.signature == "SCPT")
        );
        drop_run(id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
    }
}
