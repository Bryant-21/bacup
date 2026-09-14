use serde_json::{Map, Value};

use crate::errors::RecordReadError;
use crate::fnv_legacy_scripting::component::{
    FnvQuestTopologyEvidence, REQUIRED_FNV_QUEST_SLICE, REQUIRED_FNV_SCPT_BINDINGS,
    admit_exact_fnv_quest_slice, apply_fnv_start_routes, apply_runtime_origin_provenance,
};
use crate::fnv_legacy_scripting::record_identity::{LegacyRecordIdentity, LegacyRecordTopology};
use crate::fnv_legacy_scripting::script_synthesizer::{PackageDataAliasContract, TranslatedScript};
use crate::fnv_legacy_scripting::{FnvLegacyScriptingResult, FnvScriptingError};
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::run::{ConversionRun, form_key_to_legacy_str};
use crate::source_read::{form_key_to_read_str, read_record};
use crate::sym::StringInterner;
use crate::translator::DeferredKind;
use crate::translator::pair_hooks::fnv_fo4::{PlacedActorAliasTarget, placed_actor_base_formkey};
use crate::translator::pair_hooks::fnv_pack::{
    LegacyPackLowerer, LegacyPackSourceFamily, VerifiedFo4LegacyPackLowerer, classify_legacy_pack,
};

#[derive(Default)]
struct DeferredFnvInputs {
    script_records: Vec<(Value, String)>,
    quest_records: Vec<(Value, String)>,
    scene_records: Vec<(Value, String)>,
    info_records: Vec<(Value, String)>,
    dial_records: Vec<(Value, String)>,
    identities: Vec<LegacyRecordIdentity>,
    typed_quest_records: Vec<(Record, String)>,
    typed_scene_records: Vec<(Record, String)>,
    typed_info_records: Vec<(Record, String)>,
    typed_dial_records: Vec<(Record, String)>,
}

#[derive(Default)]
struct FnvQuestSlicePreparation {
    records_written: u32,
    accounting: Vec<(String, String, String)>,
    component_plans: Vec<crate::quest_runtime::QuestRuntimeComponentPlan>,
    expected_receipts: Vec<crate::quest_runtime::QuestRuntimeExpectedReceipt>,
}

impl FnvQuestSlicePreparation {
    fn from_admitted(
        component_plans: Vec<crate::quest_runtime::QuestRuntimeComponentPlan>,
    ) -> Self {
        let expected_receipts = component_plans
            .iter()
            .map(|plan| plan.expected_receipt.clone())
            .collect();
        Self {
            component_plans,
            expected_receipts,
            ..Self::default()
        }
    }

    fn apply_to(self, result: &mut FnvLegacyScriptingResult) {
        result.records_written += self.records_written;
        result.skipped_records.extend(self.accounting);
        result.quest_runtime_component_plans = self.component_plans;
        result.quest_runtime_expected_receipts = self.expected_receipts;
    }
}

pub fn run_from_deferred(
    run: &mut ConversionRun,
    mod_prefix: &str,
    source_plugin: &str,
    mod_path: &str,
) -> Result<FnvLegacyScriptingResult, FnvScriptingError> {
    let preparation = prepare_fnv_quest_slice(run, source_plugin)?;
    let inputs = collect_deferred_inputs(run, source_plugin)?;
    let scri_links = run
        .fnv_scri_links
        .iter()
        .map(|link| {
            (
                link.target_form_key.clone(),
                link.source_scpt_form_key.clone(),
            )
        })
        .collect::<Vec<_>>();
    let result = run.run_fnv_legacy_scripting(
        mod_prefix,
        source_plugin,
        mod_path,
        &inputs.script_records,
        &inputs.quest_records,
        &inputs.scene_records,
        &inputs.info_records,
        &inputs.dial_records,
        &inputs.identities,
        &scri_links,
        &inputs.typed_quest_records,
        &inputs.typed_scene_records,
        &inputs.typed_info_records,
        &inputs.typed_dial_records,
    )?;
    let mut result = result;
    preparation.apply_to(&mut result);
    if run.config.fnv_quest_slice
        && (result.records_failed != 0 || !result.skipped_records.is_empty())
    {
        return Err(FnvScriptingError::Setup(format!(
            "strict FNV quest slice is incomplete: records_failed={} skipped_records={}",
            result.records_failed,
            result.skipped_records.len()
        )));
    }
    run.deferred
        .retain(|(_, kind)| !matches!(kind, DeferredKind::FnvLegacyScripting));
    Ok(result)
}

fn prepare_fnv_quest_slice(
    run: &mut ConversionRun,
    source_plugin: &str,
) -> Result<FnvQuestSlicePreparation, FnvScriptingError> {
    if !run.config.fnv_quest_slice {
        return Ok(FnvQuestSlicePreparation::default());
    }
    let source_plugin_sym = run.interner.intern(source_plugin);
    let selected = run.config.fnv_quest_slice_records.clone();
    validate_required_quest_slice(&selected)?;
    validate_required_source_records(run, source_plugin_sym, &selected)?;
    let component_mappings = exact_component_mappings(run, source_plugin_sym, &selected);
    let component_topology =
        exact_component_topology(run, source_plugin_sym, &selected, &component_mappings)?;
    let mut component_plans = admit_exact_fnv_quest_slice(
        &selected,
        source_plugin,
        &component_mappings,
        &component_topology,
    )
    .map_err(FnvScriptingError::Setup)?;
    apply_runtime_origin_provenance(&mut component_plans, &run.config.legacy_runtime_origins)
        .map_err(FnvScriptingError::Setup)?;
    apply_fnv_start_routes(&mut component_plans).map_err(FnvScriptingError::Setup)?;
    let mut preparation = FnvQuestSlicePreparation::from_admitted(component_plans);
    ensure_exact_acti_vmad_target(run, source_plugin_sym, &selected)?;
    validate_existing_world_path_mappings(run, source_plugin_sym, &selected)?;
    register_existing_placed_actor_aliases(run, source_plugin_sym, &selected)?;
    let mut translate = Vec::new();
    let bypass_signatures = ["DIAL", "INFO", "PERK", "QUST", "SPEL"];
    for signature in bypass_signatures {
        for local in selected.get(signature).into_iter().flatten() {
            translate.push(FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_plugin_sym,
            });
        }
    }
    let removed = bypass_signatures
        .into_iter()
        .filter(|signature| run.translator.maps.skip_records.remove(*signature))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let translated = run.translate_records_preserving_mapper(&translate);
    for signature in removed {
        run.translator.maps.skip_records.insert(signature);
    }
    let translated = translated.map_err(|error| {
        FnvScriptingError::Setup(format!("prepare exact FNV quest records: {error}"))
    })?;
    if translated.records_failed != 0 || translated.records_dropped != 0 {
        return Err(FnvScriptingError::Setup(format!(
            "prepare exact FNV quest records was incomplete: failed={} dropped={}",
            translated.records_failed, translated.records_dropped
        )));
    }
    if run.fnv_dedicated_topic_targets.is_some() {
        return Err(FnvScriptingError::Setup(
            "strict FNV dedicated topic identities were already allocated".to_string(),
        ));
    }
    let dedicated_topic_targets = {
        let state = run.mapper_state.as_mut().ok_or_else(|| {
            FnvScriptingError::Setup(
                "strict FNV dedicated topic allocation requires mapper state".to_string(),
            )
        })?;
        let mut mapper = crate::formkey_mapper::FormKeyMapper::from_state(state, &run.interner);
        crate::run::FnvDedicatedTopicTargets {
            keyword: mapper.allocate_generated(),
            topic: mapper.allocate_generated(),
        }
    };
    run.fnv_dedicated_topic_targets = Some(dedicated_topic_targets);

    let mut records_written = 0u32;
    let accounting = Vec::new();
    let force_greet_donor = if selected
        .get("PACK")
        .is_some_and(|locals| locals.contains(&0x13289E))
    {
        let donor_form_key = require_force_greet_donor_form_key(
            run.config.fnv_force_greet_donor_form_key.as_deref(),
        )?;
        Some(run.read_explicit_target_master_record(donor_form_key)?)
    } else {
        None
    };
    for local in selected.get("PACK").into_iter().flatten().copied() {
        let source_fk = FormKey {
            local: local & 0x00FF_FFFF,
            plugin: source_plugin_sym,
        };
        let source_key = form_key_to_read_str(&source_fk, &run.interner);
        let record = read_record(
            run.source_handle_id,
            &source_key,
            &run.schema_source,
            &mut run.interner,
        )
        .map_err(fnv_read_error)?;
        let target_sig = SigCode::from_str("PACK").map_err(|error| {
            FnvScriptingError::Setup(format!("invalid PACK allocation signature: {error}"))
        })?;
        let coverage =
            run.preallocate_legacy_form_key_intents([crate::run::LegacyFormKeyAllocationIntent {
                source_fk,
                editor_id: record.eid,
                target_sig,
            }]);
        if coverage.mapped != 1 || coverage.missing != 0 {
            return Err(FnvScriptingError::Setup(format!(
                "selected PACK {local:06X} target identity preallocation failed"
            )));
        }
        let report = classify_legacy_pack(&record, LegacyPackSourceFamily::Fnv, &run.interner);
        let inventory = report.inventory.ok_or_else(|| {
            FnvScriptingError::Setup(format!("selected PACK {local:06X} failed classification"))
        })?;
        let (target_fk, mappings) = {
            let state = run.mapper_state.as_mut().ok_or_else(|| {
                FnvScriptingError::Setup("selected PACK lowering requires mapper state".to_string())
            })?;
            let mut mapper = crate::formkey_mapper::FormKeyMapper::from_state(state, &run.interner);
            let target = mapper.allocate_or_resolve(source_fk, record.eid, record.sig);
            (target, state.source_to_target.clone())
        };
        let reference_mapper = |text: &str| {
            FormKey::parse(text, &run.interner)
                .ok()
                .and_then(|source| mappings.get(&source).copied())
        };
        let lowerer = VerifiedFo4LegacyPackLowerer::new(&run.interner, reference_mapper);
        let lowered = if local == 0x13289E {
            let donor = force_greet_donor.as_ref().ok_or_else(|| {
                FnvScriptingError::Setup("selected PACK 13289E donor disappeared".to_string())
            })?;
            let state = run.mapper_state.as_mut().ok_or_else(|| {
                FnvScriptingError::Setup(
                    "selected PACK donor lowering requires mapper state".to_string(),
                )
            })?;
            let mut mapper = crate::formkey_mapper::FormKeyMapper::from_state(state, &run.interner);
            lowerer.lower_audited_renolds_dialogue_with_donor(&inventory, &mut mapper, donor)
        } else {
            lowerer.lower_supported_legacy_pack(&inventory)
        };
        match lowered {
            Ok(mut lowered) => {
                lowered.form_key = target_fk;
                crate::target_write::add_record_native(
                    run.target_handle_id,
                    lowered,
                    &run.schema_target,
                    &run.interner,
                )
                .map_err(|error| {
                    FnvScriptingError::Setup(format!("write selected PACK {local:06X}: {error}"))
                })?;
                records_written += 1;
            }
            Err(error) => {
                return Err(FnvScriptingError::Setup(format!(
                    "selected PACK {local:06X} failed verified lowering: {error}"
                )));
            }
        }
    }
    preparation.records_written = records_written;
    preparation.accounting = accounting;
    Ok(preparation)
}

fn exact_component_mappings(
    run: &ConversionRun,
    source_plugin: crate::sym::Sym,
    selected: &std::collections::HashMap<String, Vec<u32>>,
) -> std::collections::BTreeMap<
    crate::quest_runtime::QuestRecordKey,
    crate::quest_runtime::QuestRecordKey,
> {
    let Some(state) = run.mapper_state.as_ref() else {
        return std::collections::BTreeMap::new();
    };
    let mut source_records = selected
        .iter()
        .flat_map(|(signature, locals)| {
            locals.iter().map(move |local| (signature.as_str(), *local))
        })
        .collect::<std::collections::BTreeSet<_>>();
    source_records.extend(
        REQUIRED_FNV_SCPT_BINDINGS
            .iter()
            .map(|(_, signature, local)| (*signature, *local)),
    );
    source_records
        .into_iter()
        .filter_map(|(signature, local)| {
            let source = FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_plugin,
            };
            let target = state.source_to_target.get(&source).copied()?;
            let source_text = form_key_to_legacy_str(source, &run.interner)?;
            let target_text = form_key_to_legacy_str(target, &run.interner)?;
            Some((
                crate::quest_runtime::QuestRecordKey::new(signature, source_text),
                crate::quest_runtime::QuestRecordKey::new(signature, target_text),
            ))
        })
        .collect()
}

fn exact_component_topology(
    run: &ConversionRun,
    source_plugin: crate::sym::Sym,
    selected: &std::collections::HashMap<String, Vec<u32>>,
    mappings: &std::collections::BTreeMap<
        crate::quest_runtime::QuestRecordKey,
        crate::quest_runtime::QuestRecordKey,
    >,
) -> Result<FnvQuestTopologyEvidence, FnvScriptingError> {
    let mut evidence = FnvQuestTopologyEvidence::default();
    for signature in ["ACHR", "REFR"] {
        for local in selected.get(signature).into_iter().flatten().copied() {
            let source_form_key = FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_plugin,
            };
            let source_text =
                form_key_to_legacy_str(source_form_key, &run.interner).ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "render source {signature} {local:06X} topology identity"
                    ))
                })?;
            let source = crate::quest_runtime::QuestRecordKey::new(signature, source_text);
            let target = mappings.get(&source).cloned().ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "mapped {signature} {local:06X} topology identity is absent"
                ))
            })?;
            let target_local = target
                .form_key
                .split_once(':')
                .and_then(|(local, _)| u32::from_str_radix(local, 16).ok())
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "mapped {signature} {local:06X} topology identity is invalid"
                    ))
                })?;
            evidence.source.insert(
                source,
                exact_cell_child_group_path(run.source_handle_id, signature, local)?,
            );
            evidence.target.insert(
                target,
                exact_cell_child_group_path(run.target_handle_id, signature, target_local)?,
            );
        }
    }
    Ok(evidence)
}

fn exact_cell_child_group_path(
    handle_id: u64,
    signature: &str,
    local: u32,
) -> Result<Vec<String>, FnvScriptingError> {
    fn collect_paths(
        items: &[esp_authoring_core::plugin_runtime::ParsedItem],
        signature: &str,
        local: u32,
        path: &mut Vec<String>,
        matches: &mut Vec<Vec<String>>,
    ) {
        for item in items {
            match item {
                esp_authoring_core::plugin_runtime::ParsedItem::Record(record)
                    if record.signature.eq_ignore_ascii_case(signature)
                        && record.form_id & 0x00FF_FFFF == local & 0x00FF_FFFF =>
                {
                    matches.push(path.clone());
                }
                esp_authoring_core::plugin_runtime::ParsedItem::Group(group) => {
                    let segment = if group.group_type == 0 {
                        format!("GRUP:{}", String::from_utf8_lossy(&group.label))
                    } else {
                        format!(
                            "GRUP:type={}:label={:08X}",
                            group.group_type,
                            u32::from_le_bytes(group.label)
                        )
                    };
                    path.push(segment);
                    collect_paths(&group.children, signature, local, path, matches);
                    path.pop();
                }
                _ => {}
            }
        }
    }

    let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
        .lock()
        .unwrap();
    let slot = store.get(&handle_id).ok_or_else(|| {
        FnvScriptingError::Setup(format!("topology handle {handle_id} is unavailable"))
    })?;
    let mut matches = Vec::new();
    collect_paths(
        &slot.parsed.root_items,
        signature,
        local,
        &mut Vec::new(),
        &mut matches,
    );
    let [path] = matches.as_slice() else {
        return Err(FnvScriptingError::Setup(format!(
            "{signature} {local:06X} must have one exact topology path, observed {}",
            matches.len()
        )));
    };
    let is_cell_child = path.first().is_some_and(|segment| segment == "GRUP:CELL")
        && path
            .iter()
            .any(|segment| segment.starts_with("GRUP:type=6:"))
        && path.iter().any(|segment| {
            segment.starts_with("GRUP:type=8:") || segment.starts_with("GRUP:type=9:")
        });
    if !is_cell_child {
        return Err(FnvScriptingError::Setup(format!(
            "{signature} {local:06X} is not under exact CELL child topology: {path:?}"
        )));
    }
    Ok(path.clone())
}

pub(crate) fn ensure_exact_acti_vmad_target(
    run: &mut ConversionRun,
    source_plugin: crate::sym::Sym,
    selected: &std::collections::HashMap<String, Vec<u32>>,
) -> Result<(), FnvScriptingError> {
    let locals = selected.get("ACTI").ok_or_else(|| {
        FnvScriptingError::Setup("strict FNV quest slice is missing ACTI dependencies".to_string())
    })?;
    if locals.as_slice() != [0x133F41] {
        return Err(FnvScriptingError::Setup(format!(
            "strict FNV quest slice ACTI dependencies are not exact: {locals:?}"
        )));
    }
    let source_acti = FormKey {
        local: 0x133F41,
        plugin: source_plugin,
    };
    let source_record = crate::source_read::read_record_relayout_by_form_key(
        run.source_handle_id,
        &source_acti,
        &run.schema_source,
        &run.interner,
        None,
    )
    .map_err(fnv_read_error)?;
    let expected_script = FormKey {
        local: 0x134491,
        plugin: source_plugin,
    };
    if source_record.sig.as_str() != "ACTI"
        || source_record
            .eid
            .and_then(|editor_id| run.interner.resolve(editor_id))
            != Some("TecRenoldsDialogueActivator")
        || direct_form_key_fields(&source_record, "SCRI")? != [expected_script]
    {
        return Err(FnvScriptingError::Setup(
            "required ACTI 133F41 is not exact TecRenoldsDialogueActivator SCRI 134491".to_string(),
        ));
    }
    let target_acti = required_existing_mapping(run, source_acti, "ACTI")?;
    let before = crate::target_write::existing_record_topology_native(
        run.target_handle_id,
        target_acti,
        "ACTI",
        &run.interner,
    )
    .map_err(|error| FnvScriptingError::Setup(format!("inspect mapped ACTI 133F41: {error}")))?;
    if before.occurrences == 0 {
        let translated = run
            .translate_records_preserving_mapper(&[source_acti])
            .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "recover exact ACTI 133F41 through normal translation: {error}"
                ))
            })?;
        if translated.records_translated != 1
            || translated.records_vanilla_remapped != 0
            || translated.records_dropped != 0
            || translated.records_deferred != 0
            || translated.records_failed != 0
        {
            return Err(FnvScriptingError::Setup(format!(
                "recover exact ACTI 133F41 was incomplete: translated={} remapped={} dropped={} deferred={} failed={}",
                translated.records_translated,
                translated.records_vanilla_remapped,
                translated.records_dropped,
                translated.records_deferred,
                translated.records_failed
            )));
        }
    }
    validate_exact_acti_target(run, source_plugin, source_acti, target_acti)
}

fn required_existing_mapping(
    run: &ConversionRun,
    source: FormKey,
    signature: &str,
) -> Result<FormKey, FnvScriptingError> {
    run.mapper_state
        .as_ref()
        .and_then(|state| state.source_to_target.get(&source))
        .copied()
        .ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "required {signature} {:06X} has no existing translate_v2 mapping",
                source.local
            ))
        })
}

fn validate_exact_acti_target(
    run: &mut ConversionRun,
    source_plugin: crate::sym::Sym,
    source_acti: FormKey,
    target_acti: FormKey,
) -> Result<(), FnvScriptingError> {
    let target_record = crate::source_read::read_record_relayout_by_form_key(
        run.target_handle_id,
        &target_acti,
        &run.schema_target,
        &run.interner,
        None,
    )
    .map_err(|error| {
        FnvScriptingError::Setup(format!(
            "required ACTI 133F41 did not flow through normal translation: {error}"
        ))
    })?;
    if target_record.sig.as_str() != "ACTI" {
        return Err(FnvScriptingError::Setup(format!(
            "required ACTI 133F41 mapped to {}",
            target_record.sig.as_str()
        )));
    }
    let topology = crate::target_write::existing_record_topology_native(
        run.target_handle_id,
        target_acti,
        "ACTI",
        &run.interner,
    )
    .map_err(|error| FnvScriptingError::Setup(format!("inspect mapped ACTI 133F41: {error}")))?;
    if topology.occurrences != 1 {
        return Err(FnvScriptingError::Setup(format!(
            "mapped ACTI 133F41 must occur exactly once (occurrences={})",
            topology.occurrences
        )));
    }

    let expected_target = render_legacy_form_key(target_acti, &run.interner);
    let expected_script = render_legacy_form_key(
        FormKey {
            local: 0x134491,
            plugin: source_plugin,
        },
        &run.interner,
    );
    let mut seen = false;
    let mut wrong = Vec::new();
    run.fnv_scri_links.retain(|link| {
        if link.target_form_key != expected_target {
            return true;
        }
        if link.source_scpt_form_key != expected_script {
            wrong.push(link.source_scpt_form_key.clone());
            return true;
        }
        if seen {
            false
        } else {
            seen = true;
            true
        }
    });
    if !wrong.is_empty() {
        return Err(FnvScriptingError::Setup(format!(
            "mapped ACTI 133F41 has unexpected SCRI attachment(s): {wrong:?}"
        )));
    }
    if !seen {
        return Err(FnvScriptingError::Setup(format!(
            "mapped ACTI 133F41 has no exact SCPT 134491 attachment receipt (source {:06X})",
            source_acti.local
        )));
    }
    Ok(())
}

fn validate_required_quest_slice(
    selected: &std::collections::HashMap<String, Vec<u32>>,
) -> Result<(), FnvScriptingError> {
    let normalized = selected
        .iter()
        .map(|(signature, locals)| {
            let mut locals = locals
                .iter()
                .map(|local| local & 0x00FF_FFFF)
                .collect::<Vec<_>>();
            locals.sort_unstable();
            locals.dedup();
            (signature.trim().to_ascii_uppercase(), locals)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let expected = REQUIRED_FNV_QUEST_SLICE
        .iter()
        .map(|(signature, locals)| {
            let mut locals = locals.to_vec();
            locals.sort_unstable();
            ((*signature).to_string(), locals)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    if normalized != expected {
        return Err(FnvScriptingError::Setup(format!(
            "strict FNV quest slice closure mismatch: expected {expected:?}, got {normalized:?}"
        )));
    }
    Ok(())
}

fn validate_required_source_records(
    run: &ConversionRun,
    source_plugin: crate::sym::Sym,
    selected: &std::collections::HashMap<String, Vec<u32>>,
) -> Result<(), FnvScriptingError> {
    for (signature, locals) in selected {
        for local in locals {
            let source_fk = FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_plugin,
            };
            let record = crate::source_read::read_record_relayout_by_form_key(
                run.source_handle_id,
                &source_fk,
                &run.schema_source,
                &run.interner,
                None,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "required {signature} {local:06X} is absent from the selected source plugin: {error}"
                ))
            })?;
            if !record.sig.as_str().eq_ignore_ascii_case(signature) {
                return Err(FnvScriptingError::Setup(format!(
                    "required {signature} {local:06X} resolved as {}",
                    record.sig.as_str()
                )));
            }
        }
    }
    Ok(())
}

fn register_existing_placed_actor_aliases(
    run: &mut ConversionRun,
    source_plugin: crate::sym::Sym,
    selected: &std::collections::HashMap<String, Vec<u32>>,
) -> Result<(), FnvScriptingError> {
    for local in selected.get("ACHR").into_iter().flatten() {
        let source_placed = FormKey {
            local: local & 0x00FF_FFFF,
            plugin: source_plugin,
        };
        let target_placed = run
            .mapper_state
            .as_ref()
            .and_then(|state| state.source_to_target.get(&source_placed))
            .copied()
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "required nested ACHR {local:06X} has no existing world-path mapping"
                ))
            })?;
        let source_record = crate::source_read::read_record_relayout_by_form_key(
            run.source_handle_id,
            &source_placed,
            &run.schema_source,
            &run.interner,
            None,
        )
        .map_err(fnv_read_error)?;
        let source_base =
            placed_actor_base_formkey(&source_record, &run.interner).ok_or_else(|| {
                FnvScriptingError::Setup(format!("required ACHR {local:06X} has no NAME base"))
            })?;
        let target_base = run
            .mapper_state
            .as_ref()
            .and_then(|state| state.source_to_target.get(&source_base))
            .copied()
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "required ACHR {local:06X} base {:06X} has no target mapping",
                    source_base.local
                ))
            })?;
        let target_record = crate::source_read::read_record_relayout_by_form_key(
            run.target_handle_id,
            &target_placed,
            &run.schema_target,
            &run.interner,
            None,
        )
        .map_err(|error| {
            FnvScriptingError::Setup(format!(
                "mapped ACHR {local:06X} is absent from recursive target topology: {error}"
            ))
        })?;
        if target_record.sig.as_str() != "ACHR"
            || placed_actor_base_formkey(&target_record, &run.interner) != Some(target_base)
        {
            return Err(FnvScriptingError::Setup(format!(
                "mapped ACHR {local:06X} target record/base mismatch"
            )));
        }
        let topology = crate::target_write::existing_record_topology_native(
            run.target_handle_id,
            target_placed,
            "ACHR",
            &run.interner,
        )
        .map_err(|error| FnvScriptingError::Setup(format!("inspect ACHR topology: {error}")))?;
        if topology.occurrences != 1 || topology.top_level {
            return Err(FnvScriptingError::Setup(format!(
                "mapped ACHR {local:06X} must occur exactly once under world topology (occurrences={}, top_level={})",
                topology.occurrences, topology.top_level
            )));
        }
        run.legacy_placed_actor_aliases
            .register(PlacedActorAliasTarget {
                source_placed,
                target_placed,
                source_base,
                target_base,
            })
            .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
    }
    Ok(())
}

fn validate_existing_world_path_mappings(
    run: &ConversionRun,
    source_plugin: crate::sym::Sym,
    selected: &std::collections::HashMap<String, Vec<u32>>,
) -> Result<(), FnvScriptingError> {
    for signature in ["ACHR", "ACTI", "MGEF", "REFR", "SCPT"] {
        for local in selected.get(signature).into_iter().flatten() {
            let source = FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_plugin,
            };
            let target = run
                .mapper_state
                .as_ref()
                .and_then(|state| state.source_to_target.get(&source))
                .copied()
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "required {signature} {local:06X} has no existing translate_v2 mapping"
                    ))
                })?;
            if matches!(signature, "ACTI" | "MGEF") {
                let target_record = crate::source_read::read_record_relayout_by_form_key(
                    run.target_handle_id,
                    &target,
                    &run.schema_target,
                    &run.interner,
                    None,
                )
                .map_err(|error| {
                    FnvScriptingError::Setup(format!(
                        "required {signature} {local:06X} did not flow through normal translation: {error}"
                    ))
                })?;
                if target_record.sig.as_str() != signature {
                    return Err(FnvScriptingError::Setup(format!(
                        "required {signature} {local:06X} mapped to {}",
                        target_record.sig.as_str()
                    )));
                }
            }
        }
    }
    validate_exact_persistent_refr(run, source_plugin)
}

pub(crate) fn validate_exact_persistent_refr(
    run: &ConversionRun,
    source_plugin: crate::sym::Sym,
) -> Result<(), FnvScriptingError> {
    let source_refr = FormKey {
        local: 0x133F42,
        plugin: source_plugin,
    };
    let source_acti = FormKey {
        local: 0x133F41,
        plugin: source_plugin,
    };
    let target_refr = required_existing_mapping(run, source_refr, "REFR")?;
    let target_acti = required_existing_mapping(run, source_acti, "ACTI")?;
    let source_record = crate::source_read::read_record_relayout_by_form_key(
        run.source_handle_id,
        &source_refr,
        &run.schema_source,
        &run.interner,
        None,
    )
    .map_err(fnv_read_error)?;
    if source_record.sig.as_str() != "REFR"
        || record_form_key_field(&source_record, "NAME") != Some(source_acti)
        || !source_record
            .flags
            .contains(crate::record::RecordFlags::PERSISTENT)
    {
        return Err(FnvScriptingError::Setup(
            "required REFR 133F42 is not a persistent reference to ACTI 133F41".to_string(),
        ));
    }
    validate_exact_renolds_trigger_geometry(&source_record, &run.interner, "source")?;
    let source_topology = crate::target_write::existing_record_topology_native(
        run.source_handle_id,
        source_refr,
        "REFR",
        &run.interner,
    )
    .map_err(|error| FnvScriptingError::Setup(format!("inspect source REFR topology: {error}")))?;
    let source_base_references = crate::target_write::count_references_to_base_native(
        run.source_handle_id,
        source_acti,
        &run.interner,
    )
    .map_err(|error| {
        FnvScriptingError::Setup(format!(
            "inspect source ACTI reference cardinality: {error}"
        ))
    })?;
    if source_topology.occurrences != 1
        || source_topology.top_level
        || !source_topology.persistent
        || source_base_references != 1
    {
        return Err(FnvScriptingError::Setup(format!(
            "source REFR 133F42 must be the sole persistent ACTI 133F41 instance (occurrences={}, top_level={}, persistent={}, base_references={source_base_references})",
            source_topology.occurrences, source_topology.top_level, source_topology.persistent
        )));
    }
    let target_record = crate::source_read::read_record_relayout_by_form_key(
        run.target_handle_id,
        &target_refr,
        &run.schema_target,
        &run.interner,
        None,
    )
    .map_err(|error| {
        FnvScriptingError::Setup(format!(
            "mapped REFR 133F42 is absent from recursive target topology: {error}"
        ))
    })?;
    if target_record.sig.as_str() != "REFR"
        || record_form_key_field(&target_record, "NAME") != Some(target_acti)
    {
        return Err(FnvScriptingError::Setup(
            "mapped REFR 133F42 target record/base mismatch".to_string(),
        ));
    }
    validate_exact_renolds_trigger_geometry(&target_record, &run.interner, "target")?;
    let topology = crate::target_write::existing_record_topology_native(
        run.target_handle_id,
        target_refr,
        "REFR",
        &run.interner,
    )
    .map_err(|error| FnvScriptingError::Setup(format!("inspect REFR topology: {error}")))?;
    let target_base_references = crate::target_write::count_references_to_base_native(
        run.target_handle_id,
        target_acti,
        &run.interner,
    )
    .map_err(|error| {
        FnvScriptingError::Setup(format!(
            "inspect mapped ACTI reference cardinality: {error}"
        ))
    })?;
    if topology.occurrences != 1
        || topology.top_level
        || !topology.persistent
        || target_base_references != 1
    {
        return Err(FnvScriptingError::Setup(format!(
            "mapped REFR 133F42 must be the sole persistent target ACTI instance (occurrences={}, top_level={}, persistent={}, base_references={target_base_references})",
            topology.occurrences, topology.top_level, topology.persistent
        )));
    }
    Ok(())
}

fn validate_exact_renolds_trigger_geometry(
    record: &Record,
    interner: &StringInterner,
    side: &str,
) -> Result<(), FnvScriptingError> {
    const EXPECTED_NUMBERS: &[(&str, &str, f64)] = &[
        ("XPRM", "BoundsX", 624.0),
        ("XPRM", "BoundsY", 1032.0),
        ("XPRM", "BoundsZ", 176.0),
        ("XMBO", "BoundHalfExtentsX", 624.0),
        ("XMBO", "BoundHalfExtentsY", 1032.0),
        ("XMBO", "BoundHalfExtentsZ", 176.0),
        ("DATA", "PositionRotationPositionX", 54928.0),
        ("DATA", "PositionRotationPositionY", -47392.0),
        ("DATA", "PositionRotationPositionZ", 4760.0),
        ("DATA", "PositionRotationRotationZ", 2.3831870555877686),
    ];
    for signature in ["XPRM", "XMBO", "DATA"] {
        if record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == signature)
            .count()
            != 1
        {
            return Err(FnvScriptingError::Setup(format!(
                "{side} REFR 133F42 must contain exactly one {signature} field"
            )));
        }
    }
    for (signature, member, expected) in EXPECTED_NUMBERS {
        let actual = named_record_number(record, signature, member, interner).ok_or_else(|| {
            let available = record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == *signature)
                .map(|field| match &field.value {
                    FieldValue::Struct(fields) => fields
                        .iter()
                        .filter_map(|(name, _)| interner.resolve(*name))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                })
                .collect::<Vec<_>>();
            FnvScriptingError::Setup(format!(
                "{side} REFR 133F42 is missing {signature}.{member} (available={available:?})"
            ))
        })?;
        if (actual - expected).abs() > 0.0001 {
            return Err(FnvScriptingError::Setup(format!(
                "{side} REFR 133F42 {signature}.{member} expected {expected}, got {actual}"
            )));
        }
    }
    let primitive_field_value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "XPRM")
        .map(|field| &field.value);
    let primitive_type = named_record_value(record, "XPRM", "Type", interner);
    let raw_primitive = raw_record_field_bytes(record, "XPRM");
    let raw_primitive_type = raw_primitive
        .filter(|bytes| bytes.len() >= 32)
        .map(|bytes| u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]));
    let is_box = matches!(primitive_type, Some(FieldValue::String(value)) if interner.resolve(*value) == Some("Box"))
        || matches!(
            primitive_type,
            Some(FieldValue::Uint(1) | FieldValue::Int(1))
        )
        || raw_primitive_type == Some(1);
    if !is_box {
        let observed = primitive_field_value
            .map(|value| describe_xprm_field_value(value, interner))
            .unwrap_or_else(|| "missing".to_string());
        let raw_len = raw_primitive.map_or(0, <[u8]>::len);
        let raw_hex = raw_primitive
            .map(|bytes| {
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02X}"))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_else(|| "<none>".to_string());
        return Err(FnvScriptingError::Setup(format!(
            "{side} REFR 133F42 XPRM.Type is not Box: observed={observed}, raw_len={raw_len}, raw_hex={raw_hex}, raw_type_u32={raw_primitive_type:?}, expected FNV XPRM enum Box=1"
        )));
    }
    Ok(())
}

fn describe_xprm_field_value(value: &FieldValue, interner: &StringInterner) -> String {
    match value {
        FieldValue::None => "None".to_string(),
        FieldValue::Bool(value) => format!("Bool({value})"),
        FieldValue::Int(value) => format!("Int({value})"),
        FieldValue::Uint(value) => format!("Uint({value})"),
        FieldValue::Float(value) => format!("Float({value})"),
        FieldValue::String(value) => format!(
            "String({})",
            interner.resolve(*value).unwrap_or("<unresolved>")
        ),
        FieldValue::Bytes(bytes) => format!("Bytes(len={})", bytes.len()),
        FieldValue::FormKey(value) => format!("FormKey({value:?})"),
        FieldValue::List(values) => format!("List(len={})", values.len()),
        FieldValue::Struct(fields) => format!(
            "Struct({})",
            fields
                .iter()
                .map(|(name, value)| format!(
                    "{}={}",
                    interner.resolve(*name).unwrap_or("<unresolved>"),
                    describe_xprm_field_value(value, interner)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn named_record_number(
    record: &Record,
    signature: &str,
    member: &str,
    interner: &StringInterner,
) -> Option<f64> {
    if let Some(value) = named_record_value(record, signature, member, interner) {
        return match value {
            FieldValue::Float(value) => Some(f64::from(*value)),
            FieldValue::Int(value) => Some(*value as f64),
            FieldValue::Uint(value) => Some(*value as f64),
            _ => None,
        };
    }
    let offset = match (signature, member) {
        ("XPRM", "BoundsX") | ("XMBO", "BoundHalfExtentsX") => 0,
        ("XPRM", "BoundsY") | ("XMBO", "BoundHalfExtentsY") => 4,
        ("XPRM", "BoundsZ") | ("XMBO", "BoundHalfExtentsZ") => 8,
        ("DATA", "PositionRotationPositionX") => 0,
        ("DATA", "PositionRotationPositionY") => 4,
        ("DATA", "PositionRotationPositionZ") => 8,
        ("DATA", "PositionRotationRotationZ") => 20,
        _ => return None,
    };
    let bytes = raw_record_field_bytes(record, signature)?;
    let value = bytes.get(offset..offset + 4)?;
    Some(f64::from(f32::from_le_bytes([
        value[0], value[1], value[2], value[3],
    ])))
}

fn raw_record_field_bytes<'a>(record: &'a Record, signature: &str) -> Option<&'a [u8]> {
    record.fields.iter().find_map(|field| {
        (field.sig.as_str() == signature)
            .then_some(&field.value)
            .and_then(|value| match value {
                FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
    })
}

fn named_record_value<'a>(
    record: &'a Record,
    signature: &str,
    member: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    record.fields.iter().find_map(|field| {
        if field.sig.as_str() != signature {
            return None;
        }
        let FieldValue::Struct(fields) = &field.value else {
            return None;
        };
        fields
            .iter()
            .find_map(|(name, value)| (interner.resolve(*name) == Some(member)).then_some(value))
    })
}

fn validate_dynamic_package_absent_from_npc_base(
    run: &ConversionRun,
    source_plugin: crate::sym::Sym,
    source_npc_local: u32,
    source_package_local: u32,
) -> Result<(), FnvScriptingError> {
    let state = run.mapper_state.as_ref().ok_or_else(|| {
        FnvScriptingError::Setup("dynamic package validation requires mapper state".to_string())
    })?;
    let target_npc = state
        .source_to_target
        .get(&FormKey {
            local: source_npc_local,
            plugin: source_plugin,
        })
        .copied()
        .ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "NPC {source_npc_local:06X} has no mapped target identity"
            ))
        })?;
    let target_package = state
        .source_to_target
        .get(&FormKey {
            local: source_package_local,
            plugin: source_plugin,
        })
        .copied()
        .ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "PACK {source_package_local:06X} has no mapped target identity"
            ))
        })?;
    let npc = crate::source_read::read_record_relayout_by_form_key(
        run.target_handle_id,
        &target_npc,
        &run.schema_target,
        &run.interner,
        None,
    )
    .map_err(|error| {
        FnvScriptingError::Setup(format!(
            "read mapped NPC {source_npc_local:06X} for dynamic package validation: {error}"
        ))
    })?;
    if npc.fields.iter().any(|field| {
        field.sig.as_str() == "PKID" && field_value_contains_form_key(&field.value, target_package)
    }) {
        return Err(FnvScriptingError::Setup(format!(
            "mapped NPC {source_npc_local:06X} contains dynamic PACK {source_package_local:06X} unconditionally"
        )));
    }
    Ok(())
}

pub(crate) fn reconcile_fnv_quest_slice_dynamic_package_bases(
    run: &mut ConversionRun,
    translated_scripts: &[TranslatedScript],
) -> Result<String, FnvScriptingError> {
    if !run.config.fnv_quest_slice {
        return Ok(String::new());
    }
    let state = run.mapper_state.as_ref().ok_or_else(|| {
        FnvScriptingError::Setup(
            "dynamic package base reconciliation requires mapper state".to_string(),
        )
    })?;
    if !state
        .options
        .source_plugin_name
        .eq_ignore_ascii_case("FalloutNV.esm")
    {
        return Err(FnvScriptingError::Setup(format!(
            "strict FNV dynamic package reconciliation requires FalloutNV.esm, got {}",
            state.options.source_plugin_name
        )));
    }
    let source_plugin = run.interner.intern(&state.options.source_plugin_name);
    validate_dynamic_package_absent_from_npc_base(run, source_plugin, 0x123193, 0x1231B6)?;

    let source_npc = mapped_source_form_key(source_plugin, 0x1300F0);
    let source_script = mapped_source_form_key(source_plugin, 0x166305);
    let source_patrol = mapped_source_form_key(source_plugin, 0x133F3E);
    let source_dialogue = mapped_source_form_key(source_plugin, 0x13289E);
    let source_quest = mapped_source_form_key(source_plugin, 0x11F935);
    let target_npc = required_target_mapping(run, source_npc, "NPC 1300F0")?;
    let target_patrol = required_target_mapping(run, source_patrol, "PACK 133F3E")?;
    let target_dialogue = required_target_mapping(run, source_dialogue, "PACK 13289E")?;
    let target_quest = required_target_mapping(run, source_quest, "QUST 11F935")?;

    let source_record = crate::source_read::read_record_relayout_by_form_key(
        run.source_handle_id,
        &source_npc,
        &run.schema_source,
        &run.interner,
        None,
    )
    .map_err(fnv_read_error)?;
    validate_renolds_source_npc(
        &source_record,
        source_npc,
        source_script,
        source_patrol,
        source_dialogue,
        &run.interner,
    )?;

    let contracts = translated_scripts
        .iter()
        .flat_map(|script| script.package_data_aliases.iter())
        .filter(|contract| {
            contract
                .source_script_form_key
                .eq_ignore_ascii_case("134491:FalloutNV.esm")
        })
        .collect::<Vec<_>>();
    let [contract] = contracts.as_slice() else {
        return Err(FnvScriptingError::Setup(format!(
            "strict FNV Renolds base-package adaptation expected one SCPT 134491 dynamic contract, observed {}",
            contracts.len()
        )));
    };
    validate_renolds_dynamic_contract(contract, target_quest, target_dialogue, &run.interner)?;

    let mut target_record = crate::source_read::read_record_relayout_by_form_key(
        run.target_handle_id,
        &target_npc,
        &run.schema_target,
        &run.interner,
        None,
    )
    .map_err(|error| {
        FnvScriptingError::Setup(format!(
            "read mapped NPC 1300F0 for dynamic package reconciliation: {error}"
        ))
    })?;
    validate_renolds_dialogue_pack(run, target_dialogue, target_quest)?;
    prune_shadowed_renolds_dialogue_package(
        &mut target_record,
        target_patrol,
        target_dialogue,
        &run.interner,
    )?;
    let replaced = crate::target_write::replace_record_contents_native(
        run.target_handle_id,
        target_record,
        &run.schema_target,
        &run.interner,
    )
    .map_err(|error| {
        FnvScriptingError::Setup(format!(
            "replace mapped NPC 1300F0 after shadowed package removal: {error}"
        ))
    })?;
    if !replaced {
        return Err(FnvScriptingError::Setup(
            "mapped NPC 1300F0 disappeared during shadowed package removal".to_string(),
        ));
    }
    validate_dynamic_package_absent_from_npc_base(run, source_plugin, 0x1300F0, 0x13289E)?;
    let reconciled = crate::source_read::read_record_relayout_by_form_key(
        run.target_handle_id,
        &target_npc,
        &run.schema_target,
        &run.interner,
        None,
    )
    .map_err(fnv_read_error)?;
    if direct_form_key_fields(&reconciled, "PKID")? != [target_patrol] {
        return Err(FnvScriptingError::Setup(
            "mapped NPC 1300F0 patrol package changed during shadowed fallback removal".to_string(),
        ));
    }

    Ok(
        "target_native_adaptation:NPC_1300F0:removed_shadowed_PACK_13289E:dynamic_SCPT_134491_ALPC"
            .to_string(),
    )
}

fn mapped_source_form_key(plugin: crate::sym::Sym, local: u32) -> FormKey {
    FormKey { plugin, local }
}

fn required_target_mapping(
    run: &ConversionRun,
    source: FormKey,
    label: &str,
) -> Result<FormKey, FnvScriptingError> {
    run.mapper_state
        .as_ref()
        .and_then(|state| state.source_to_target.get(&source))
        .copied()
        .ok_or_else(|| FnvScriptingError::Setup(format!("{label} has no mapped target identity")))
}

fn validate_renolds_source_npc(
    record: &Record,
    expected_npc: FormKey,
    expected_script: FormKey,
    expected_patrol: FormKey,
    expected_dialogue: FormKey,
    interner: &StringInterner,
) -> Result<(), FnvScriptingError> {
    let editor_id = record.eid.and_then(|editor_id| interner.resolve(editor_id));
    if record.form_key != expected_npc
        || record.sig.as_str() != "NPC_"
        || !editor_id
            .is_some_and(|editor_id| editor_id.eq_ignore_ascii_case("NVTechatticupNCRRenolds"))
    {
        return Err(FnvScriptingError::Setup(
            "strict FNV Renolds package adaptation source NPC identity mismatch".to_string(),
        ));
    }
    if direct_form_key_fields(record, "SCRI")? != [expected_script] {
        return Err(FnvScriptingError::Setup(
            "strict FNV Renolds package adaptation requires exact SCRI 166305".to_string(),
        ));
    }
    if direct_form_key_fields(record, "PKID")? != [expected_patrol, expected_dialogue] {
        return Err(FnvScriptingError::Setup(
            "strict FNV Renolds package adaptation requires ordered source stack [133F3E, 13289E]"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_renolds_dynamic_contract(
    contract: &PackageDataAliasContract,
    target_quest: FormKey,
    target_dialogue: FormKey,
    interner: &StringInterner,
) -> Result<(), FnvScriptingError> {
    let expected_target_quest = render_legacy_form_key(target_quest, interner);
    let expected_target_dialogue = render_legacy_form_key(target_dialogue, interner);
    let valid = contract
        .source_quest_form_key
        .eq_ignore_ascii_case("11F935:FalloutNV.esm")
        && contract
            .source_package_form_key
            .eq_ignore_ascii_case("13289E:FalloutNV.esm")
        && contract
            .target_quest_form_key
            .eq_ignore_ascii_case(&expected_target_quest)
        && contract
            .target_package_form_key
            .eq_ignore_ascii_case(&expected_target_dialogue)
        && contract.owner_quest_property.eq_ignore_ascii_case("VTechatticup")
        && contract
            .actor_expression
            .eq_ignore_ascii_case("NVTecNCRRenoldsREF")
        && contract.property_name.eq_ignore_ascii_case(
            "TechaticupNCRRenoldsDialoguePackageData",
        )
        && contract.requested_package_property.eq_ignore_ascii_case(
            "TechaticupNCRRenoldsDialoguePackage",
        )
        && contract.alias_package_subrecord == "ALPC"
        && contract.operation.eq_ignore_ascii_case(
            "TechaticupNCRRenoldsDialoguePackageData.ApplyToRef(NVTecNCRRenoldsREF); NVTecNCRRenoldsREF.EvaluatePackage(true)",
        );
    if !valid {
        return Err(FnvScriptingError::Setup(
            "strict FNV Renolds package adaptation dynamic ALPC contract mismatch".to_string(),
        ));
    }
    Ok(())
}

fn prune_shadowed_renolds_dialogue_package(
    record: &mut Record,
    expected_patrol: FormKey,
    expected_dialogue: FormKey,
    interner: &StringInterner,
) -> Result<(), FnvScriptingError> {
    let stack = direct_form_key_fields(record, "PKID")?;
    if stack != [expected_patrol, expected_dialogue] {
        return Err(FnvScriptingError::Setup(format!(
            "mapped NPC 1300F0 package stack mismatch: expected [{}, {}], got {:?}",
            render_legacy_form_key(expected_patrol, interner),
            render_legacy_form_key(expected_dialogue, interner),
            stack
                .iter()
                .map(|form_key| render_legacy_form_key(*form_key, interner))
                .collect::<Vec<_>>()
        )));
    }
    let dialogue_index = record
        .fields
        .iter()
        .position(|field| {
            field.sig.as_str() == "PKID"
                && matches!(field.value, FieldValue::FormKey(form_key) if form_key == expected_dialogue)
        })
        .ok_or_else(|| {
            FnvScriptingError::Setup(
                "mapped NPC 1300F0 lost exact PACK 13289E field before reconciliation".to_string(),
            )
        })?;
    record.fields.remove(dialogue_index);
    if direct_form_key_fields(record, "PKID")? != [expected_patrol] {
        return Err(FnvScriptingError::Setup(
            "mapped NPC 1300F0 patrol package changed during shadowed fallback removal".to_string(),
        ));
    }
    Ok(())
}

fn validate_renolds_dialogue_pack(
    run: &ConversionRun,
    target_dialogue: FormKey,
    target_quest: FormKey,
) -> Result<(), FnvScriptingError> {
    let record = crate::source_read::read_record_relayout_by_form_key(
        run.target_handle_id,
        &target_dialogue,
        &run.schema_target,
        &run.interner,
        None,
    )
    .map_err(fnv_read_error)?;
    let conditions = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "CTDA")
        .collect::<Vec<_>>();
    let [condition] = conditions.as_slice() else {
        return Err(FnvScriptingError::Setup(format!(
            "mapped PACK 13289E expected one GetStage condition, observed {}",
            conditions.len()
        )));
    };
    let FieldValue::Bytes(bytes) = &condition.value else {
        return Err(FnvScriptingError::Setup(
            "mapped PACK 13289E GetStage condition is not raw FO4 CTDA".to_string(),
        ));
    };
    let expected_quest_raw = target_raw_form_id(run, target_quest)?;
    let valid_condition = bytes.len()
        == crate::translator::pair_hooks::fnv_conditions::FO4_CTDA_LEN
        && bytes[0] == 0x80
        && read_u32_at(bytes, 4) == 10.0_f32.to_bits()
        && read_u16_at(bytes, 8) == 58
        && read_u32_at(bytes, 12) == expected_quest_raw
        && read_u32_at(bytes, 20) == 0;
    let owners = direct_form_key_fields(&record, "QNAM")?;
    if !valid_condition || owners != [target_quest] {
        return Err(FnvScriptingError::Setup(
            "mapped PACK 13289E did not preserve GetStage(mapped QUST 11F935) < 10".to_string(),
        ));
    }
    Ok(())
}

fn target_raw_form_id(run: &ConversionRun, form_key: FormKey) -> Result<u32, FnvScriptingError> {
    let state = run.mapper_state.as_ref().ok_or_else(|| {
        FnvScriptingError::Setup("target raw FormID requires mapper state".to_string())
    })?;
    let plugin = run.interner.resolve(form_key.plugin).ok_or_else(|| {
        FnvScriptingError::Setup("target raw FormID plugin is unresolved".to_string())
    })?;
    let index = if plugin.eq_ignore_ascii_case(&state.options.output_plugin_name) {
        state.options.target_master_names.len()
    } else {
        state
            .options
            .target_master_names
            .iter()
            .position(|master| master.eq_ignore_ascii_case(plugin))
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "target raw FormID plugin {plugin} is neither output nor target master"
                ))
            })?
    };
    if index > u8::MAX as usize {
        return Err(FnvScriptingError::Setup(
            "target raw FormID master index exceeds 255".to_string(),
        ));
    }
    Ok(((index as u32) << 24) | form_key.local)
}

fn direct_form_key_fields(
    record: &Record,
    signature: &str,
) -> Result<Vec<FormKey>, FnvScriptingError> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .map(|field| match field.value {
            FieldValue::FormKey(form_key) => Ok(form_key),
            _ => Err(FnvScriptingError::Setup(format!(
                "{} {:06X} {signature} is not a typed FormKey",
                record.sig.as_str(),
                record.form_key.local
            ))),
        })
        .collect()
}

fn render_legacy_form_key(form_key: FormKey, interner: &StringInterner) -> String {
    form_key_to_legacy_str(form_key, interner).unwrap_or_default()
}

fn read_u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn field_value_contains_form_key(value: &FieldValue, expected: FormKey) -> bool {
    match value {
        FieldValue::FormKey(form_key) => *form_key == expected,
        FieldValue::List(values) => values
            .iter()
            .any(|value| field_value_contains_form_key(value, expected)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| field_value_contains_form_key(value, expected)),
        _ => false,
    }
}

fn require_force_greet_donor_form_key(configured: Option<&str>) -> Result<&str, FnvScriptingError> {
    configured
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            FnvScriptingError::Setup(
                "selected PACK 13289E requires explicit FO4 ForceGreet donor".to_string(),
            )
        })
}

fn collect_deferred_inputs(
    run: &mut ConversionRun,
    source_plugin_name: &str,
) -> Result<DeferredFnvInputs, FnvScriptingError> {
    let mut inputs = DeferredFnvInputs::default();
    let info_parents =
        crate::target_write::build_source_info_to_dialogue_index(run.source_handle_id)
            .map_err(|error| FnvScriptingError::Setup(format!("INFO parent index: {error}")))?;
    let deferred = run
        .deferred
        .iter()
        .filter(|(_, kind)| matches!(kind, DeferredKind::FnvLegacyScripting))
        .map(|(fk, _)| *fk)
        .collect::<Vec<_>>();
    let selected = run
        .config
        .fnv_quest_slice_records
        .iter()
        .flat_map(|(signature, locals)| {
            locals
                .iter()
                .map(move |local| (signature.to_ascii_uppercase(), local & 0x00FF_FFFF))
        })
        .collect::<std::collections::HashSet<_>>();
    let source_plugin = run.interner.intern(source_plugin_name);

    for fk in deferred {
        if run.config.fnv_quest_slice && fk.plugin != source_plugin {
            continue;
        }
        let source_key = form_key_to_read_str(&fk, &run.interner);
        if source_key.is_empty() {
            continue;
        }
        let record = read_record(
            run.source_handle_id,
            &source_key,
            &run.schema_source,
            &mut run.interner,
        )
        .map_err(fnv_read_error)?;
        if run.config.fnv_quest_slice
            && !is_selected_deferred_record(&selected, source_plugin, fk, record.sig.as_str())
        {
            continue;
        }
        let legacy_key = form_key_to_legacy_key(&source_key);
        let mut payload = record_to_legacy_payload(&record, &legacy_key, &run.interner);
        expose_selected_sctx_source(&record, &mut payload);
        if record.sig.as_str() == "SCPT" {
            attach_scpt_mapped_form_keys(run, &record, &mut payload)?;
        }
        let target_fk = run
            .mapper_state
            .as_ref()
            .and_then(|state| state.source_to_target.get(&fk))
            .copied()
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "deferred {} {} has no target identity",
                    record.sig.as_str(),
                    legacy_key
                ))
            })?;
        let target_key = form_key_to_legacy_str(target_fk, &run.interner).ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "deferred {} {} target identity cannot be rendered",
                record.sig.as_str(),
                legacy_key
            ))
        })?;
        let topology = record_topology(run, &record, fk, &info_parents)?;
        inputs.identities.push(LegacyRecordIdentity {
            signature: record.sig.as_str().to_string(),
            source_form_key: fk,
            target_form_key: target_fk,
            source_form_key_text: legacy_key.clone(),
            target_form_key_text: target_key,
            topology,
        });
        match record.sig.as_str() {
            "SCPT" => inputs.script_records.push((payload, legacy_key)),
            "QUST" => {
                inputs
                    .typed_quest_records
                    .push((record, legacy_key.clone()));
                inputs.quest_records.push((payload, legacy_key));
            }
            "SCEN" => {
                inputs
                    .typed_scene_records
                    .push((record, legacy_key.clone()));
                inputs.scene_records.push((payload, legacy_key));
            }
            "INFO" => {
                inputs.typed_info_records.push((record, legacy_key.clone()));
                inputs.info_records.push((payload, legacy_key));
            }
            "DIAL" => {
                inputs.typed_dial_records.push((record, legacy_key.clone()));
                inputs.dial_records.push((payload, legacy_key));
            }
            _ => {}
        }
    }

    Ok(inputs)
}

fn is_selected_deferred_record(
    selected: &std::collections::HashSet<(String, u32)>,
    source_plugin: crate::sym::Sym,
    form_key: FormKey,
    signature: &str,
) -> bool {
    form_key.plugin == source_plugin
        && selected.contains(&(
            signature.trim().to_ascii_uppercase(),
            form_key.local & 0x00FF_FFFF,
        ))
}

fn expose_selected_sctx_source(record: &Record, payload: &mut Value) {
    let Some(fields) = payload.get_mut("fields").and_then(Value::as_array_mut) else {
        return;
    };
    for (source, field) in record.fields.iter().zip(fields.iter_mut()) {
        if source.sig.as_str() != "SCTX" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &source.value else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        if let Some(object) = field.as_object_mut() {
            object.insert(
                "SCTX".to_string(),
                Value::String(text.trim_end_matches('\0').to_string()),
            );
        }
    }
}

fn attach_scpt_mapped_form_keys(
    run: &ConversionRun,
    record: &Record,
    payload: &mut Value,
) -> Result<(), FnvScriptingError> {
    let state = run.mapper_state.as_ref().ok_or_else(|| {
        FnvScriptingError::Setup("SCPT dependency mapping requires mapper state".to_string())
    })?;
    attach_scpt_mapped_form_keys_with_state(record, payload, state, &run.interner)
}

fn attach_scpt_mapped_form_keys_with_state(
    record: &Record,
    payload: &mut Value,
    state: &crate::formkey_mapper::MapperState,
    interner: &StringInterner,
) -> Result<(), FnvScriptingError> {
    let source_form_key = form_key_to_legacy_str(record.form_key, interner).ok_or_else(|| {
        FnvScriptingError::Setup(format!(
            "SCPT {:06X} source identity is unresolved",
            record.form_key.local
        ))
    })?;
    let editor_id = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .unwrap_or("");
    let mut dependencies = Vec::new();
    for field in &record.fields {
        if field.sig.as_str() != "SCRO" {
            continue;
        }
        let before = dependencies.len();
        collect_field_value_form_keys(&field.value, &mut dependencies);
        if dependencies.len() == before {
            return Err(FnvScriptingError::Setup(format!(
                "SCPT {:06X} has an undecodable SCRO dependency",
                record.form_key.local
            )));
        }
    }
    let mut mapped = serde_json::Map::new();
    for source in dependencies {
        let source_text = form_key_to_legacy_str(source, interner).ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "SCPT {:06X} dependency source identity is unresolved",
                record.form_key.local
            ))
        })?;
        if crate::fnv_legacy_scripting::script_synthesizer::is_exact_compat_consumed_scro(
            editor_id,
            &source_form_key,
            &source_text,
        ) {
            continue;
        }
        let target = state
            .source_to_target
            .get(&source)
            .copied()
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "SCPT {:06X} dependency {:06X} has no target mapping",
                    record.form_key.local, source.local
                ))
            })?;
        let target_text = form_key_to_legacy_str(target, interner).ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "SCPT {:06X} dependency target identity is unresolved",
                record.form_key.local
            ))
        })?;
        mapped.insert(source_text, Value::String(target_text));
    }
    payload
        .as_object_mut()
        .ok_or_else(|| FnvScriptingError::Setup("SCPT payload is not an object".to_string()))?
        .insert("__mapped_form_keys".to_string(), Value::Object(mapped));
    Ok(())
}

fn collect_field_value_form_keys(value: &FieldValue, out: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(form_key) => out.push(*form_key),
        FieldValue::List(values) => {
            for value in values {
                collect_field_value_form_keys(value, out);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_field_value_form_keys(value, out);
            }
        }
        _ => {}
    }
}

fn record_topology(
    run: &ConversionRun,
    record: &Record,
    source_fk: FormKey,
    info_parents: &std::collections::HashMap<u32, u32>,
) -> Result<LegacyRecordTopology, FnvScriptingError> {
    if record.sig.as_str() == "INFO" && matches!(source_fk.local, 0x15734B | 0x15734C | 0x15734D) {
        let source_parent = info_parents
            .get(&source_fk.local)
            .copied()
            .map(|local| FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_fk.plugin,
            })
            .filter(|parent| parent.local == 0x0000C8)
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "dedicated INFO {:06X} is not parented to source GREETING 0000C8",
                    source_fk.local
                ))
            })?;
        let target_parent = run
            .fnv_dedicated_topic_targets
            .ok_or_else(|| {
                FnvScriptingError::Setup(
                    "dedicated greeting target identity was not allocated".to_string(),
                )
            })?
            .topic;
        return Ok(LegacyRecordTopology::TopicChild {
            source_parent,
            target_parent,
        });
    }
    let source_parent = match record.sig.as_str() {
        "DIAL" => record_form_key_field(record, "QSTI"),
        "SCEN" => record_form_key_field(record, "PNAM"),
        "INFO" => info_parents
            .get(&source_fk.local)
            .or_else(|| {
                info_parents.iter().find_map(|(info, parent)| {
                    ((*info & 0x00FF_FFFF) == source_fk.local).then_some(parent)
                })
            })
            .copied()
            .map(|local| FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_fk.plugin,
            }),
        _ => return Ok(LegacyRecordTopology::TopLevel),
    }
    .ok_or_else(|| {
        FnvScriptingError::Setup(format!(
            "deferred {} {:06X} has no source parent topology",
            record.sig.as_str(),
            source_fk.local
        ))
    })?;
    let target_parent = run
        .mapper_state
        .as_ref()
        .and_then(|state| state.source_to_target.get(&source_parent))
        .copied()
        .ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "deferred {} {:06X} parent {:06X} has no target identity",
                record.sig.as_str(),
                source_fk.local,
                source_parent.local
            ))
        })?;
    if record.sig.as_str() == "INFO" {
        Ok(LegacyRecordTopology::TopicChild {
            source_parent,
            target_parent,
        })
    } else {
        Ok(LegacyRecordTopology::QuestChild {
            source_parent,
            target_parent,
        })
    }
}

fn record_form_key_field(record: &Record, signature: &str) -> Option<FormKey> {
    record.fields.iter().rev().find_map(|field| {
        (field.sig.as_str() == signature)
            .then_some(&field.value)
            .and_then(field_value_form_key)
    })
}

fn field_value_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::List(values) => values.iter().find_map(field_value_form_key),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| field_value_form_key(value)),
        _ => None,
    }
}

fn fnv_read_error(err: RecordReadError) -> FnvScriptingError {
    FnvScriptingError::Setup(format!("native deferred FNV read: {err}"))
}

fn form_key_to_legacy_key(source_key: &str) -> String {
    let Some((plugin, local)) = source_key.rsplit_once(':') else {
        return source_key.to_string();
    };
    format!("{}:{}", local.trim(), plugin.trim())
}

fn record_to_legacy_payload(
    record: &Record,
    source_form_key: &str,
    interner: &StringInterner,
) -> Value {
    let mut map = Map::new();
    if let Some(eid) = record.eid.and_then(|sym| interner.resolve(sym)) {
        map.insert("eid".to_string(), Value::String(eid.to_string()));
    }
    map.insert(
        "signature".to_string(),
        Value::String(record.sig.as_str().to_string()),
    );
    map.insert(
        "form_id".to_string(),
        Value::String(source_form_key.to_string()),
    );
    if record.sig.as_str() == "DIAL" {
        map.insert(
            "__source_form_key".to_string(),
            Value::String(source_form_key.to_string()),
        );
    }
    let fields = record
        .fields
        .iter()
        .map(|entry| {
            let mut field = Map::new();
            field.insert(
                entry.sig.as_str().to_string(),
                field_value_to_json(&entry.value, interner),
            );
            Value::Object(field)
        })
        .collect::<Vec<_>>();
    map.insert("fields".to_string(), Value::Array(fields));
    Value::Object(map)
}

fn field_value_to_json(value: &FieldValue, interner: &StringInterner) -> Value {
    match value {
        FieldValue::None => Value::Null,
        FieldValue::Bool(v) => Value::Bool(*v),
        FieldValue::Int(v) => Value::Number((*v).into()),
        FieldValue::Uint(v) => serde_json::Number::from(*v).into(),
        FieldValue::Float(v) => serde_json::Number::from_f64(*v as f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        FieldValue::String(sym) => interner
            .resolve(*sym)
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
        FieldValue::Bytes(bytes) => {
            let mut raw = Map::new();
            raw.insert(
                "encoding".to_string(),
                Value::String("raw-bytes-hex".to_string()),
            );
            raw.insert("hex".to_string(), Value::String(hex::encode_upper(bytes)));
            Value::Object(raw)
        }
        FieldValue::FormKey(fk) => {
            let plugin = interner.resolve(fk.plugin).unwrap_or("");
            Value::String(format!("{:06X}:{}", fk.local, plugin))
        }
        FieldValue::List(items) => Value::Array(
            items
                .iter()
                .map(|item| field_value_to_json(item, interner))
                .collect(),
        ),
        FieldValue::Struct(fields) => {
            let mut map = Map::new();
            for (key, value) in fields {
                if let Some(name) = interner.resolve(*key) {
                    map.insert(name.to_string(), field_value_to_json(value, interner));
                }
            }
            Value::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};

    #[test]
    fn form_key_to_legacy_key_flips_plugin_first_shape() {
        assert_eq!(
            form_key_to_legacy_key("FalloutNV.esm:001234"),
            "001234:FalloutNV.esm"
        );
    }

    #[test]
    fn record_to_legacy_payload_adds_source_key_for_dial() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let eid = interner.intern("TopicOne");
        let qnam = interner.intern("speaker");
        let mut record = Record::new(
            SigCode::from_str("DIAL").unwrap(),
            FormKey {
                local: 0x1234,
                plugin,
            },
        );
        record.eid = Some(eid);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("QNAM").unwrap(),
            value: FieldValue::String(qnam),
        });

        let payload = record_to_legacy_payload(&record, "001234:FalloutNV.esm", &interner);
        assert_eq!(payload["eid"], "TopicOne");
        assert_eq!(payload["signature"], "DIAL");
        assert_eq!(payload["form_id"], "001234:FalloutNV.esm");
        assert_eq!(payload["__source_form_key"], "001234:FalloutNV.esm");
        assert_eq!(payload["fields"][0]["QNAM"], "speaker");
    }

    #[test]
    fn production_sctx_and_scda_bytes_are_not_presented_as_source_text() {
        let interner = StringInterner::new();
        for bytes in [b"set x to 1".as_slice(), &[0x01, 0xFF, 0x00][..]] {
            let value = field_value_to_json(
                &FieldValue::Bytes(smallvec::SmallVec::from_slice(bytes)),
                &interner,
            );
            assert!(!value.is_string());
            assert_eq!(value["encoding"], "raw-bytes-hex");
            assert!(value["hex"].as_str().is_some_and(|hex| !hex.is_empty()));
        }
    }

    #[test]
    fn field_value_to_json_renders_form_key_in_legacy_shape() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let value = FieldValue::FormKey(FormKey {
            local: 0x5678,
            plugin,
        });
        assert_eq!(
            field_value_to_json(&value, &interner),
            Value::String("005678:FalloutNV.esm".to_string())
        );
    }

    #[test]
    fn scpt_payload_carries_central_mapper_targets_for_scro_dependencies() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let target_plugin = interner.intern("Converted.esp");
        let script_form_key = FormKey {
            local: 0x13015B,
            plugin: source_plugin,
        };
        let source_dependency = FormKey {
            local: 0x130161,
            plugin: source_plugin,
        };
        let target_dependency = FormKey {
            local: 0x800123,
            plugin: target_plugin,
        };
        let mut record = Record::new(SigCode::from_str("SCPT").unwrap(), script_form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SCRO").unwrap(),
            value: FieldValue::FormKey(source_dependency),
        });
        let mut state = MapperState::new([], MapperOptions::default());
        state
            .source_to_target
            .insert(source_dependency, target_dependency);
        let mut payload = serde_json::json!({});

        attach_scpt_mapped_form_keys_with_state(&record, &mut payload, &state, &interner).unwrap();

        assert_eq!(
            payload["__mapped_form_keys"]["130161:FalloutNV.esm"],
            "800123:Converted.esp"
        );
    }

    #[test]
    fn scpt_payload_fails_closed_when_scro_dependency_is_unmapped() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let mut record = Record::new(
            SigCode::from_str("SCPT").unwrap(),
            FormKey {
                local: 0x13015B,
                plugin: source_plugin,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SCRO").unwrap(),
            value: FieldValue::FormKey(FormKey {
                local: 0x130161,
                plugin: source_plugin,
            }),
        });
        let state = MapperState::new([], MapperOptions::default());
        let mut payload = serde_json::json!({});

        let error =
            attach_scpt_mapped_form_keys_with_state(&record, &mut payload, &state, &interner)
                .unwrap_err();

        assert!(error.to_string().contains("has no target mapping"));
    }

    #[test]
    fn exact_reputation_scro_is_consumed_without_a_faction_mapping() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let mut record = Record::new(
            SigCode::from_str("SCPT").unwrap(),
            FormKey {
                local: 0x123191,
                plugin: source_plugin,
            },
        );
        record.eid = Some(interner.intern("TecMineHostage"));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SCRO").unwrap(),
            value: FieldValue::FormKey(FormKey {
                local: 0x0F43DE,
                plugin: source_plugin,
            }),
        });
        let state = MapperState::new([], MapperOptions::default());
        let mut payload = serde_json::json!({});

        attach_scpt_mapped_form_keys_with_state(&record, &mut payload, &state, &interner).unwrap();

        assert_eq!(payload["__mapped_form_keys"], serde_json::json!({}));
    }

    #[test]
    fn exact_player_scro_is_consumed_only_for_audited_script_identities() {
        let interner = StringInterner::new();
        let merged_plugin = interner.intern("FalloutNV.esm");
        let wrong_plugin = interner.intern("Fallout3.esm");
        let cases = [
            (
                0x123191,
                "TecMineHostage",
                merged_plugin,
                0x000014,
                merged_plugin,
                true,
            ),
            (
                0x134491,
                "NVTechatticupRenoldsDialogueScript",
                merged_plugin,
                0x000014,
                merged_plugin,
                true,
            ),
            (
                0x123192,
                "TecMineHostage",
                merged_plugin,
                0x000014,
                merged_plugin,
                false,
            ),
            (
                0x123191,
                "WrongHostageScript",
                merged_plugin,
                0x000014,
                merged_plugin,
                false,
            ),
            (
                0x123191,
                "TecMineHostage",
                merged_plugin,
                0x000014,
                wrong_plugin,
                false,
            ),
            (
                0x123191,
                "TecMineHostage",
                merged_plugin,
                0x000015,
                merged_plugin,
                false,
            ),
        ];

        for (
            script_local,
            editor_id,
            script_plugin,
            dependency_local,
            dependency_plugin,
            consumed,
        ) in cases
        {
            let mut record = Record::new(
                SigCode::from_str("SCPT").unwrap(),
                FormKey {
                    local: script_local,
                    plugin: script_plugin,
                },
            );
            record.eid = Some(interner.intern(editor_id));
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("SCRO").unwrap(),
                value: FieldValue::FormKey(FormKey {
                    local: dependency_local,
                    plugin: dependency_plugin,
                }),
            });
            let state = MapperState::new([], MapperOptions::default());
            let mut payload = serde_json::json!({});

            let result =
                attach_scpt_mapped_form_keys_with_state(&record, &mut payload, &state, &interner);
            if consumed {
                result.unwrap();
                assert_eq!(payload["__mapped_form_keys"], serde_json::json!({}));
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("has no target mapping")
                );
            }
        }
    }

    #[test]
    fn renolds_force_greet_donor_is_terminal_when_absent() {
        assert!(require_force_greet_donor_form_key(None).is_err());
        assert!(require_force_greet_donor_form_key(Some("  ")).is_err());
        assert_eq!(
            require_force_greet_donor_form_key(Some("05B307:Fallout4.esm")).unwrap(),
            "05B307:Fallout4.esm"
        );
    }

    #[test]
    fn dynamic_script_packages_are_detected_in_npc_base_stacks() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Converted.esm");
        let package = FormKey {
            local: 0x2231B6,
            plugin,
        };
        let other = FormKey {
            local: 0x2231B7,
            plugin,
        };
        assert!(field_value_contains_form_key(
            &FieldValue::List(vec![FieldValue::FormKey(package)]),
            package,
        ));
        assert!(!field_value_contains_form_key(
            &FieldValue::FormKey(other),
            package,
        ));
    }

    #[test]
    fn exact_renolds_base_stack_is_pruned_only_with_dynamic_alpc_evidence() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let target_plugin = interner.intern("Converted.esm");
        let source_npc = FormKey {
            local: 0x1300F0,
            plugin: source_plugin,
        };
        let source_script = FormKey {
            local: 0x166305,
            plugin: source_plugin,
        };
        let source_patrol = FormKey {
            local: 0x133F3E,
            plugin: source_plugin,
        };
        let source_dialogue = FormKey {
            local: 0x13289E,
            plugin: source_plugin,
        };
        let target_quest = FormKey {
            local: 0x211935,
            plugin: target_plugin,
        };
        let target_patrol = FormKey {
            local: 0x233F3E,
            plugin: target_plugin,
        };
        let target_dialogue = FormKey {
            local: 0x23289E,
            plugin: target_plugin,
        };
        let field = |signature: &str, form_key: FormKey| FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value: FieldValue::FormKey(form_key),
        };

        let mut source = Record::new(SigCode::from_str("NPC_").unwrap(), source_npc);
        source.eid = Some(interner.intern("NVTechatticupNCRRenolds"));
        source.fields.extend([
            field("SCRI", source_script),
            field("PKID", source_patrol),
            field("PKID", source_dialogue),
        ]);
        validate_renolds_source_npc(
            &source,
            source_npc,
            source_script,
            source_patrol,
            source_dialogue,
            &interner,
        )
        .unwrap();

        let contract = PackageDataAliasContract {
            source_script_form_key: "134491:FalloutNV.esm".into(),
            owner_quest_property: "VTechatticup".into(),
            source_quest_form_key: "11F935:FalloutNV.esm".into(),
            target_quest_form_key: render_legacy_form_key(target_quest, &interner),
            actor_expression: "NVTecNCRRenoldsREF".into(),
            property_name: "TechaticupNCRRenoldsDialoguePackageData".into(),
            requested_package_property: "TechaticupNCRRenoldsDialoguePackage".into(),
            source_package_form_key: "13289E:FalloutNV.esm".into(),
            target_package_form_key: render_legacy_form_key(target_dialogue, &interner),
            alias_package_subrecord: "ALPC".into(),
            operation: "TechaticupNCRRenoldsDialoguePackageData.ApplyToRef(NVTecNCRRenoldsREF); NVTecNCRRenoldsREF.EvaluatePackage(true)".into(),
        };
        validate_renolds_dynamic_contract(&contract, target_quest, target_dialogue, &interner)
            .unwrap();

        let mut target = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey {
                local: 0x2300F0,
                plugin: target_plugin,
            },
        );
        target
            .fields
            .extend([field("PKID", target_patrol), field("PKID", target_dialogue)]);
        prune_shadowed_renolds_dialogue_package(
            &mut target,
            target_patrol,
            target_dialogue,
            &interner,
        )
        .unwrap();
        assert_eq!(
            direct_form_key_fields(&target, "PKID").unwrap(),
            [target_patrol]
        );

        let mut wrong_order = source.clone();
        wrong_order.fields.swap(1, 2);
        assert!(
            validate_renolds_source_npc(
                &wrong_order,
                source_npc,
                source_script,
                source_patrol,
                source_dialogue,
                &interner,
            )
            .is_err()
        );
        let mut wrong_contract = contract;
        wrong_contract.alias_package_subrecord = "ALPS".into();
        assert!(
            validate_renolds_dynamic_contract(
                &wrong_contract,
                target_quest,
                target_dialogue,
                &interner,
            )
            .is_err()
        );
    }

    #[test]
    fn strict_slice_closure_is_immutable() {
        let selected = REQUIRED_FNV_QUEST_SLICE
            .iter()
            .map(|(signature, locals)| ((*signature).to_string(), locals.to_vec()))
            .collect();
        validate_required_quest_slice(&selected).unwrap();
        assert_eq!(
            selected.get("DIAL"),
            Some(&vec![0x13015B, 0x134B9A, 0x138A74])
        );
        assert!(
            !selected
                .get("DIAL")
                .is_some_and(|locals| locals.contains(&0x0000C8))
        );
        assert_eq!(
            selected.get("INFO"),
            Some(&vec![0x130161, 0x134B9B, 0x15734B, 0x15734C, 0x15734D])
        );
        assert_eq!(selected.get("ACTI"), Some(&vec![0x133F41]));
        assert_eq!(selected.get("REFR"), Some(&vec![0x133F42]));

        for missing in ["QUST", "INFO"] {
            let mut incomplete = selected.clone();
            incomplete.remove(missing);
            assert!(validate_required_quest_slice(&incomplete).is_err());
        }
        let mut phantom_scene = selected;
        phantom_scene.insert("SCEN".to_string(), vec![0x123456]);
        assert!(validate_required_quest_slice(&phantom_scene).is_err());
    }

    #[test]
    fn admitted_plans_and_frozen_receipts_survive_slice_preparation() {
        let selected = REQUIRED_FNV_QUEST_SLICE
            .iter()
            .map(|(signature, locals)| ((*signature).to_string(), locals.to_vec()))
            .collect::<std::collections::HashMap<_, _>>();
        let mut source_records = REQUIRED_FNV_QUEST_SLICE
            .iter()
            .flat_map(|(signature, locals)| {
                locals
                    .iter()
                    .map(move |local| ((*signature).to_owned(), *local))
            })
            .collect::<std::collections::BTreeSet<_>>();
        source_records.extend(
            REQUIRED_FNV_SCPT_BINDINGS
                .iter()
                .map(|(_, signature, local)| ((*signature).to_owned(), *local)),
        );
        let mappings = source_records
            .into_iter()
            .map(|(signature, local)| {
                (
                    crate::quest_runtime::QuestRecordKey::new(
                        &signature,
                        format!("{local:06X}:FalloutNV.esm"),
                    ),
                    crate::quest_runtime::QuestRecordKey::new(
                        &signature,
                        format!("{:06X}:Converted.esm", local + 0x200000),
                    ),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut topology = FnvQuestTopologyEvidence::default();
        for (signature, local, child_type) in [
            ("ACHR", 0x12319B, 9),
            ("ACHR", 0x12319C, 9),
            ("ACHR", 0x134B9C, 9),
            ("REFR", 0x133F42, 8),
        ] {
            let source = crate::quest_runtime::QuestRecordKey::new(
                signature,
                format!("{local:06X}:FalloutNV.esm"),
            );
            topology.source.insert(
                source.clone(),
                vec![
                    "GRUP:CELL".to_owned(),
                    "GRUP:type=6:label=00000100".to_owned(),
                    format!("GRUP:type={child_type}:label=00000100"),
                ],
            );
            topology.target.insert(
                mappings[&source].clone(),
                vec![
                    "GRUP:CELL".to_owned(),
                    "GRUP:type=6:label=00000200".to_owned(),
                    format!("GRUP:type={child_type}:label=00000200"),
                ],
            );
        }
        let plans =
            admit_exact_fnv_quest_slice(&selected, "FalloutNV.esm", &mappings, &topology).unwrap();
        let frozen = plans
            .iter()
            .map(|plan| plan.expected_receipt.clone())
            .collect::<Vec<_>>();
        let preparation = FnvQuestSlicePreparation::from_admitted(plans.clone());
        let mut result = FnvLegacyScriptingResult::default();

        preparation.apply_to(&mut result);

        assert_eq!(result.quest_runtime_component_plans, plans);
        assert_eq!(result.quest_runtime_expected_receipts, frozen);
    }

    #[test]
    fn strict_deferred_allowlist_keys_signature_and_source_plugin() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let other_plugin = interner.intern("Collision.esm");
        let selected = std::collections::HashSet::from([("QUST".to_string(), 0x11F935)]);
        assert!(is_selected_deferred_record(
            &selected,
            source_plugin,
            FormKey {
                local: 0x11F935,
                plugin: source_plugin,
            },
            "QUST",
        ));
        assert!(!is_selected_deferred_record(
            &selected,
            source_plugin,
            FormKey {
                local: 0x11F935,
                plugin: other_plugin,
            },
            "QUST",
        ));
        assert!(!is_selected_deferred_record(
            &selected,
            source_plugin,
            FormKey {
                local: 0x11F935,
                plugin: source_plugin,
            },
            "INFO",
        ));
    }
}
