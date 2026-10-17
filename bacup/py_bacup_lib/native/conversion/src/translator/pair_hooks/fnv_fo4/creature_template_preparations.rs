use super::*;
use crate::source_rig::{CreatureLeveledNpcEntry, CreatureLeveledNpcProjection};
use crate::translator::pair_hooks::fnv_fo4::creature_templates::{
    LegacyTemplatePlan, resolve_template_plan,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    input: FnvFo3LiveMvpPreparationBuildInput<'_>,
    candidate: &CreatureDependencyCandidateReceipt,
    source_recipe: &super::super::creature_recipe::CreatureRecordRecipeEntry,
    evidence: &FnvFo3LiveRecipeEvidence,
    shared_recipe_ledger: &Arc<CreatureFamilyRecipeLedger>,
    winning_creatures: &BTreeMap<StableFormKey, &Record>,
    runtime_records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    records: &BTreeMap<StableFormKey, &Record>,
    dependency_records: &BTreeMap<StableFormKey, &RuntimeDependencyRecordReceipt>,
    settings: Option<ActionPointSettings>,
    actor_action_owners: &BTreeMap<String, StableFormKey>,
    actor_action_plans: &BTreeMap<String, Vec<CreatureActorActionRecordPlan>>,
    family_cache: &mut BTreeMap<RigFamilyKey, Result<PreparedLiveFamily, String>>,
    model_cache: &mut BTreeMap<
        (LegacyCreatureGame, StableFormKey),
        Result<PreparedLegacyDebrisRecord, String>,
    >,
    allocator: &mut FormKeyMapper<'_>,
    interner: &StringInterner,
) -> Result<
    Option<Vec<super::super::creature_mvp_adapter::FnvFo3MvpCandidatePreparationInput>>,
    String,
> {
    let game = source_recipe.provenance.game;
    let source = stable_map_entry(records, &candidate.source)
        .map(|(_, record)| *record)
        .ok_or("template source CREA is unavailable")?;
    if legacy_template_target(source, interner)?.is_none() {
        return Ok(None);
    }
    let plan = resolve_template_plan(&candidate.source, game, records, interner)?;
    if matches!(plan, LegacyTemplatePlan::Creature(_)) {
        return Ok(None);
    }
    let settings = settings.ok_or("template source action point GMST winners are missing")?;
    let wrapper = primary_reservation(&candidate.source, input.mapper_state, interner)
        .map_err(|error| format!("template wrapper reservation: {error:?}"))?;
    let plugin = &input.mapper_state.options.output_plugin_name;
    let paths = plan.branch_paths();
    let leaf_keys = paths
        .iter()
        .map(|path| {
            let key = allocator.allocate_generated();
            (path.clone(), TargetFormKey::new(key.local, plugin))
        })
        .collect::<BTreeMap<_, _>>();
    let mut lists = Vec::new();
    let target = clone_lists(
        &plan,
        &mut Vec::new(),
        &leaf_keys,
        records,
        dependency_records,
        input.mapper_state,
        allocator,
        &mut lists,
        input.editor_id_prefix,
        interner,
    )?;
    let templates = CreatureTemplateProjection {
        wrapper: TargetFormKey::new(wrapper.local, plugin),
        template: target,
        template_flags: 0x1fbf,
        leveled_lists: lists,
    };
    let mut results = Vec::new();
    let mut emitted_action_families = BTreeSet::new();
    for (ordinal, path) in paths.into_iter().enumerate() {
        let profile = plan.profile_at(&path)?;
        let visual_key = canonical_recipe_source_key(&profile.owners[6]);
        let visual = evidence
            .recipe_ledger
            .records
            .iter()
            .find(|record| canonical_recipe_source_key(&record.source) == visual_key)
            .ok_or_else(|| format!("template visual owner {} has no recipe", profile.owners[6]))?;
        let rig = match &visual.disposition {
            CreatureRecordRecipeDisposition::VisualOwner { rig, .. } => rig,
            _ => {
                return Err(format!(
                    "template visual owner {} has no physical body recipe",
                    profile.owners[6]
                ));
            }
        };
        let family = evidence
            .recipe_ledger
            .families
            .iter()
            .find(|family| &family.rig == rig)
            .filter(|family| matches!(family.disposition, FamilyRecipeDisposition::Ready))
            .ok_or_else(|| format!("template visual family {rig:?} is not ready"))?;
        let family_assets = family_cache
            .entry(rig.clone())
            .or_insert_with(|| {
                prepare_live_family(
                    family,
                    evidence,
                    input.source_data_root,
                    input.private_staging_root,
                    runtime_records,
                    interner,
                )
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let (_, stats_record) = stable_map_entry(winning_creatures, &profile.owners[1])
            .ok_or("template Stats owner is missing")?;
        let stats = legacy_creature_stats(stats_record, settings, interner)?;
        let is_wrapper = ordinal == 0;
        let leaf = leaf_keys
            .get(&path)
            .expect("every source branch was reserved")
            .clone();
        let reservation = if is_wrapper {
            wrapper
        } else {
            FormKey {
                local: leaf.local,
                plugin: interner.intern(plugin),
            }
        };
        let receipt = LegacyTemplateBranchReceipt::new(&plan, path, game, is_wrapper)?;
        let family_id = canonical_fnv_fo3_family_id(rig);
        let actor_actions = if actor_action_owners.get(&family_id) == Some(&candidate.source)
            && emitted_action_families.insert(family_id.clone())
        {
            actor_action_plans
                .get(&family_id)
                .cloned()
                .ok_or("template family actor actions are missing")?
        } else {
            Vec::new()
        };
        results.push(prepare_live_candidate(
            candidate,
            visual,
            family,
            source,
            stats,
            reservation,
            family_assets,
            evidence,
            shared_recipe_ledger,
            winning_creatures,
            dependency_records,
            input.dependency_ledger,
            runtime_records,
            model_cache,
            input.source_data_root,
            input.private_staging_root,
            input.mapper_state,
            allocator,
            plugin,
            input.editor_id_prefix,
            actor_actions,
            input.compiled_script,
            Some(receipt),
            is_wrapper.then(|| templates.clone()),
            is_wrapper.then_some(leaf),
            interner,
        )?);
    }
    Ok(Some(results))
}

#[allow(clippy::too_many_arguments)]
fn clone_lists(
    plan: &LegacyTemplatePlan,
    path: &mut Vec<usize>,
    leaves: &BTreeMap<Vec<usize>, TargetFormKey>,
    records: &BTreeMap<StableFormKey, &Record>,
    dependency_records: &BTreeMap<StableFormKey, &RuntimeDependencyRecordReceipt>,
    mapper: &MapperState,
    allocator: &mut FormKeyMapper<'_>,
    lists: &mut Vec<CreatureLeveledNpcProjection>,
    prefix: &str,
    interner: &StringInterner,
) -> Result<CreatureTargetRecordReference, String> {
    let LegacyTemplatePlan::Leveled { source, entries } = plan else {
        return Ok(CreatureTargetRecordReference::new(
            "NPC_",
            leaves
                .get(path)
                .ok_or("template leaf reservation is missing")?
                .clone(),
        ));
    };
    let (_, record) = stable_map_entry(records, source).ok_or("template LVLC source is missing")?;
    let chance_none = list_byte(record, "LVLD")?;
    let flags = list_byte(record, "LVLF")?;
    let global = exact_optional_record_form_key(record, "LVLG", interner)?;
    if chance_none != 0 || global.is_some() {
        return Err(format!(
            "template {source} requires the source default-CREA no-selection overlay"
        ));
    }
    if flags & !1 != 0 {
        return Err(format!(
            "template {source} has unproved list selection flags {flags:#x}"
        ));
    }
    let source_entries = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.as_str() == "LVLO")
        .collect::<Vec<_>>();
    if source_entries.len() != entries.len() {
        return Err("template plan changed source LVLO cardinality".to_string());
    }
    let target = allocator.allocate_generated();
    let target = TargetFormKey::new(target.local, &mapper.options.output_plugin_name);
    let mut target_entries = Vec::new();
    for (index, ((field_index, field), child)) in
        source_entries.into_iter().zip(entries).enumerate()
    {
        let FieldValue::Struct(fields) = &field.value else {
            return Err("template LVLO has not been source-master decoded".to_string());
        };
        let scalar = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| interner.resolve(*key) == Some(name))
                .and_then(|(_, value)| match value {
                    FieldValue::Bytes(bytes) if bytes.len() == 2 => {
                        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
                    }
                    value => integer_value(value).and_then(|value| u16::try_from(value).ok()),
                })
                .ok_or_else(|| format!("template {source} LVLO[{index}] lacks {name}"))
        };
        path.push(index);
        let reference = clone_lists(
            child,
            path,
            leaves,
            records,
            dependency_records,
            mapper,
            allocator,
            lists,
            prefix,
            interner,
        )?;
        path.pop();
        let ownership = record
            .fields
            .get(field_index + 1)
            .filter(|field| field.sig.as_str() == "COED")
            .map(|field| {
                let coed_index = record.fields[..field_index + 1]
                    .iter()
                    .filter(|field| field.sig.as_str() == "COED")
                    .count();
                let dependency = stable_map_entry(dependency_records, source)
                    .map(|(_, record)| *record)
                    .ok_or("template COED dependency receipt missing")?;
                exact_inventory_ownership(&field.value, coed_index, dependency, mapper, interner)
            })
            .transpose()?;
        target_entries.push(CreatureLeveledNpcEntry {
            level: scalar("level")?,
            count: scalar("count")?,
            reference,
            ownership,
        });
    }
    lists.push(CreatureLeveledNpcProjection {
        form_key: target.clone(),
        editor_id: format!("{prefix}_CreatureTemplateList_{:06X}", target.local),
        chance_none: 0,
        flags,
        global: None,
        entries: target_entries,
    });
    Ok(CreatureTargetRecordReference::new("LVLN", target))
}

fn list_byte(record: &Record, signature: &str) -> Result<u8, String> {
    let Some(field) = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
    else {
        return Ok(0);
    };
    match &field.value {
        FieldValue::Bytes(bytes) if bytes.len() == 1 => Ok(bytes[0]),
        value => integer_value(value)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| format!("template {signature} is not a source byte")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    fn source_key(local: u32) -> StableFormKey {
        StableFormKey {
            local,
            plugin: "FalloutNV.esm".to_string(),
        }
    }

    fn list(local: u32, rows: &[(u16, u16)], interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("LVLC").unwrap(),
            FormKey {
                local,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"LVLD"),
                value: FieldValue::Bytes([0].as_slice().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"LVLF"),
                value: FieldValue::Bytes([1].as_slice().into()),
            },
        ]);
        for (level, count) in rows {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"LVLO"),
                value: FieldValue::Struct(vec![
                    (
                        interner.intern("level"),
                        FieldValue::Bytes(level.to_le_bytes().as_slice().into()),
                    ),
                    (
                        interner.intern("count"),
                        FieldValue::Bytes(count.to_le_bytes().as_slice().into()),
                    ),
                ]),
            });
        }
        record
    }

    #[test]
    fn template_list_projection_retains_nested_levels_counts_and_duplicate_entries() {
        let interner = StringInterner::new();
        let root = list(1, &[(1, 1), (12, 2), (1, 1)], &interner);
        let nested = list(2, &[(1, 3)], &interner);
        let profile = LegacyTemplateProfile {
            root: source_key(3),
            owners: std::array::from_fn(|_| source_key(3)),
            actor_flags: 0,
        };
        let plan = LegacyTemplatePlan::Leveled {
            source: source_key(1),
            entries: vec![
                LegacyTemplatePlan::Creature(profile.clone()),
                LegacyTemplatePlan::Leveled {
                    source: source_key(2),
                    entries: vec![LegacyTemplatePlan::Creature(profile.clone())],
                },
                LegacyTemplatePlan::Creature(profile),
            ],
        };
        let records = BTreeMap::from([(source_key(1), &root), (source_key(2), &nested)]);
        let leaves = BTreeMap::from([
            (vec![0], TargetFormKey::new(0x4000, "Converted.esp")),
            (vec![1, 0], TargetFormKey::new(0x4001, "Converted.esp")),
            (vec![2], TargetFormKey::new(0x4002, "Converted.esp")),
        ]);
        let state = MapperState::new(
            std::iter::empty(),
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: "Converted.esp".into(),
                ..Default::default()
            },
        );
        let mut allocated = state.clone();
        let mut allocator = FormKeyMapper::from_state(&mut allocated, &interner);
        let mut lists = Vec::new();
        let reference = clone_lists(
            &plan,
            &mut Vec::new(),
            &leaves,
            &records,
            &BTreeMap::new(),
            &state,
            &mut allocator,
            &mut lists,
            "B21",
            &interner,
        )
        .unwrap();
        assert_eq!(lists.len(), 2);
        let target_root = lists
            .iter()
            .find(|list| list.form_key == reference.form_key)
            .unwrap();
        assert_eq!(target_root.flags, 1);
        assert_eq!(target_root.chance_none, 0);
        assert_eq!(
            target_root
                .entries
                .iter()
                .map(|entry| (entry.level, entry.count))
                .collect::<Vec<_>>(),
            vec![(1, 1), (12, 2), (1, 1)]
        );
        assert_eq!(target_root.entries[0].reference.form_key, leaves[&vec![0]]);
        assert_eq!(target_root.entries[2].reference.form_key, leaves[&vec![2]]);
        let target_nested = lists
            .iter()
            .find(|list| list.form_key == target_root.entries[1].reference.form_key)
            .unwrap();
        assert_eq!(target_nested.entries[0].count, 3);
        assert_eq!(
            target_nested.entries[0].reference.form_key,
            leaves[&vec![1, 0]]
        );
    }
}
