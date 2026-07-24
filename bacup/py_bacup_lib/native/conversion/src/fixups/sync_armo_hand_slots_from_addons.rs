//! Fixup: sync ARMO hand slots from referenced hand ARMA add-ons.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

mod keys {
    pub const FIRST_PERSON_FLAGS: &str = "FirstPersonFlags";
    pub const RACE: &str = "Race";
}

const SLOT_34_LEFT_HAND: u8 = 34;
const SLOT_35_RIGHT_HAND: u8 = 35;
const BIPED_SLOT_34_LEFT_HAND: u64 = 1 << (SLOT_34_LEFT_HAND - 30);
const BIPED_SLOT_35_RIGHT_HAND: u64 = 1 << (SLOT_35_RIGHT_HAND - 30);
const HAND_SLOT_MASK: u64 = BIPED_SLOT_34_LEFT_HAND | BIPED_SLOT_35_RIGHT_HAND;
const BARE_HUMAN_HAND_ADDONS: &[u32] = &[0x000D6C, 0x01D980, 0x0316C7];
const BARE_GHOUL_HAND_ADDON: u32 = 0x0EAFBA;
const GHOUL_RACE: u32 = 0x0EAFB6;
const LEGACY_ARMOR_BMDT_CODEC: &str = "struct:I,B,B,B,B";
const LEGACY_ARMOR_DATA_CODEC: &str = "struct:i,i,f";
const SYNTHETIC_ARMOR_ADDON_PLUGIN: &str = "__legacy_armor_addon__";
const FO4_HUMAN_RACE_LOCAL: u32 = 0x0001_3746;

#[derive(Default)]
struct HandAddonIndex {
    hand_addons: FxHashSet<FormKey>,
    hand_only_addons: FxHashSet<FormKey>,
    mixed_hand_addons: FxHashSet<FormKey>,
    ghoul_body_addons: FxHashSet<FormKey>,
    ghoul_hand_addon: Option<FormKey>,
}

pub struct SyncArmoHandSlotsFromAddonsFixup;

impl Fixup for SyncArmoHandSlotsFromAddonsFixup {
    fn name(&self) -> &'static str {
        "sync_armo_hand_slots_from_addons"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let arma_sig =
            SigCode::from_str("ARMA").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let armo_sig =
            SigCode::from_str("ARMO").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        resolve_legacy_armor_addon_lists(
            session,
            mapper,
            target_schema,
            &config.target_master_handle_ids,
            &mut report,
            arma_sig,
            armo_sig,
        )?;
        synthesize_legacy_armor_addons(
            session,
            mapper,
            config,
            target_schema,
            &mut report,
            arma_sig,
            armo_sig,
        )?;
        let hand_addons = collect_hand_addons(
            session,
            mapper,
            target_schema,
            &config.target_master_handle_ids,
            &mut report,
            arma_sig,
        )?;
        if hand_addons.hand_addons.is_empty() {
            return Ok(report);
        }

        let armo_fks = session
            .form_keys_of_sig(armo_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut changed_records = Vec::new();
        let mut strip_mixed_hand_slots_from_addons = FxHashSet::default();
        let mut protected_mixed_hand_addons = FxHashSet::default();

        for fk in armo_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(e) => {
                    let warning = mapper.interner.intern(&format!(
                        "sync_armo_hand_slots_from_addons:armo_read_err:{e}"
                    ));
                    report.warnings.push(warning);
                    continue;
                }
            };

            collect_mixed_hand_addon_strip_candidates(
                &record,
                &hand_addons.hand_only_addons,
                &hand_addons.mixed_hand_addons,
                &mut strip_mixed_hand_slots_from_addons,
                &mut protected_mixed_hand_addons,
                mapper.interner,
            );
            let synced = sync_armo_hand_slots_from_addons(
                &mut record,
                &hand_addons.hand_addons,
                mapper.interner,
            );
            let added_ghoul_hands = ensure_ghoul_hand_addon_for_ghoul_capable_armo(
                &mut record,
                &hand_addons.ghoul_body_addons,
                hand_addons.ghoul_hand_addon,
                &hand_addons.hand_addons,
                mapper.interner,
            );
            let pruned = prune_redundant_human_hand_addons(
                &mut record,
                &hand_addons.hand_addons,
                mapper.interner,
            );
            if synced || added_ghoul_hands || pruned {
                changed_records.push(record);
                report.records_changed += 1;
            }
        }

        let stripped_addon_records = strip_redundant_mixed_hand_slots_from_addons(
            session,
            mapper,
            target_schema,
            &strip_mixed_hand_slots_from_addons,
            &protected_mixed_hand_addons,
            &mut report,
        )?;
        let expected = changed_records.len() + stripped_addon_records.len();
        changed_records.extend(stripped_addon_records);
        if expected > 0 {
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "sync_armo_hand_slots_from_addons replaced {replaced} of {expected} expected records"
                )));
            }
        }

        Ok(report)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ArmorActorModels {
    male: Option<String>,
    female: Option<String>,
}

fn synthesize_legacy_armor_addons(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    target_schema: &AuthoringSchema,
    report: &mut FixupReport,
    arma_sig: SigCode,
    armo_sig: SigCode,
) -> Result<(), FixupError> {
    let Some(source_schema) = config.source_schema.as_deref() else {
        return Ok(());
    };
    if !is_legacy_armor_schema(source_schema) || session.source_slot_opt().is_none() {
        return Ok(());
    }

    let mut armor_fks = session
        .form_keys_of_sig(armo_sig, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    armor_fks.sort_by_key(|form_key| form_key.local);
    let armor_fk_set = armor_fks.iter().copied().collect::<FxHashSet<_>>();

    let mut mappings = mapper
        .source_to_target_iter()
        .filter(|(_, target)| armor_fk_set.contains(target))
        .collect::<Vec<_>>();
    mappings.sort_by(|(source_a, target_a), (source_b, target_b)| {
        target_a
            .local
            .cmp(&target_b.local)
            .then_with(|| source_a.local.cmp(&source_b.local))
            .then_with(|| {
                mapper
                    .interner
                    .resolve(source_a.plugin)
                    .unwrap_or("")
                    .cmp(mapper.interner.resolve(source_b.plugin).unwrap_or(""))
            })
    });
    let mut source_by_target = FxHashMap::default();
    for (source, target) in mappings {
        source_by_target.entry(target).or_insert(source);
    }

    let mut target_addon_models = collect_target_armor_addon_models(
        session,
        mapper.interner,
        target_schema,
        &config.target_master_handle_ids,
        arma_sig,
        report,
    )?;
    let mut changed_armors = Vec::new();
    let mut added_addons = Vec::new();

    for armor_fk in armor_fks {
        let Some(&source_fk) = source_by_target.get(&armor_fk) else {
            continue;
        };
        let source_record =
            match session.source_record_decoded(&source_fk, source_schema, mapper.interner) {
                Ok(record) if record.sig == armo_sig => record,
                Ok(_) => continue,
                Err(error) => {
                    report.warnings.push(mapper.interner.intern(&format!(
                        "sync_armo_hand_slots_from_addons:legacy_source_armo_read_err:{error}"
                    )));
                    continue;
                }
            };
        let actor_models = legacy_armo_actor_models(&source_record, mapper.interner);
        if actor_models == ArmorActorModels::default() {
            continue;
        }

        let mut armor = match session.record_decoded(&armor_fk, target_schema, mapper.interner) {
            Ok(record) => record,
            Err(error) => {
                report.warnings.push(mapper.interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:legacy_target_armo_read_err:{error}"
                )));
                continue;
            }
        };
        let uncovered_models = uncovered_actor_models(&armor, &target_addon_models, &actor_models);
        if uncovered_models == ArmorActorModels::default() {
            continue;
        }

        let synthetic_source = synthetic_armor_addon_source_key(source_fk, mapper.interner);
        let addon_fk = mapper.allocate_or_resolve(synthetic_source, None, arma_sig);
        if !target_addon_models.contains_key(&addon_fk) {
            let addon = build_synthetic_armor_addon(
                addon_fk,
                source_fk,
                &armor,
                &actor_models,
                mapper.interner,
            );
            added_addons.push(addon);
            target_addon_models.insert(addon_fk, actor_models.clone());
        }
        if insert_addon_entry(&mut armor, addon_fk) {
            changed_armors.push(armor);
        }
    }

    let expected_added = added_addons.len();
    let added = session
        .add_records(added_addons, target_schema, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    if added != expected_added {
        return Err(FixupError::HandleError(format!(
            "sync_armo_hand_slots_from_addons added {added} of {expected_added} synthesized armor add-ons"
        )));
    }
    let expected_changed = changed_armors.len();
    let changed = session
        .replace_records_contents(changed_armors, target_schema, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    if changed != expected_changed {
        return Err(FixupError::HandleError(format!(
            "sync_armo_hand_slots_from_addons attached {changed} of {expected_changed} synthesized armor add-ons"
        )));
    }
    report.records_added += added as u32;
    report.records_changed += changed as u32;
    Ok(())
}

fn is_legacy_armor_schema(schema: &AuthoringSchema) -> bool {
    let Some(armor) = schema.record_def("ARMO") else {
        return false;
    };
    armor
        .subrecord_def("BMDT")
        .and_then(|field| field.codec.as_deref())
        == Some(LEGACY_ARMOR_BMDT_CODEC)
        && armor
            .subrecord_def("DATA")
            .and_then(|field| field.codec.as_deref())
            == Some(LEGACY_ARMOR_DATA_CODEC)
}

fn collect_target_armor_addon_models(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_schema: &AuthoringSchema,
    target_master_handle_ids: &[u64],
    arma_sig: SigCode,
    report: &mut FixupReport,
) -> Result<FxHashMap<FormKey, ArmorActorModels>, FixupError> {
    let mut models = FxHashMap::default();
    for form_key in session
        .form_keys_of_sig(arma_sig, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?
    {
        match session.record_decoded(&form_key, target_schema, interner) {
            Ok(record) => {
                models.insert(form_key, target_arma_actor_models(&record, interner));
            }
            Err(error) => report.warnings.push(interner.intern(&format!(
                "sync_armo_hand_slots_from_addons:synthesis_arma_read_err:{error}"
            ))),
        }
    }
    for handle_id in target_master_handle_ids {
        for form_key in session
            .form_keys_of_sig_in_handle(*handle_id, arma_sig, interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
        {
            match session.record_decoded_in_handle(*handle_id, &form_key, target_schema, interner) {
                Ok(record) => {
                    models.insert(form_key, target_arma_actor_models(&record, interner));
                }
                Err(error) => report.warnings.push(interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:synthesis_master_arma_read_err:{error}"
                ))),
            }
        }
    }
    Ok(models)
}

fn legacy_armo_actor_models(record: &Record, interner: &StringInterner) -> ArmorActorModels {
    actor_models_from_record(record, interner, "MODL", "MOD3")
}

fn target_arma_actor_models(record: &Record, interner: &StringInterner) -> ArmorActorModels {
    actor_models_from_record(record, interner, "MOD2", "MOD3")
}

fn actor_models_from_record(
    record: &Record,
    interner: &StringInterner,
    male_sig: &str,
    female_sig: &str,
) -> ArmorActorModels {
    let mut male = Vec::new();
    let mut female = Vec::new();
    for entry in &record.fields {
        if entry.sig.as_str() == male_sig {
            match &entry.value {
                FieldValue::Struct(_) | FieldValue::List(_) => {
                    collect_named_model_paths(&entry.value, male_sig, interner, &mut male);
                    collect_named_model_paths(&entry.value, female_sig, interner, &mut female);
                }
                value => {
                    if let Some(path) = model_path_from_value(value, interner) {
                        male.push(path);
                    }
                }
            }
        } else if entry.sig.as_str() == female_sig {
            match &entry.value {
                FieldValue::Struct(_) | FieldValue::List(_) => {
                    collect_named_model_paths(&entry.value, female_sig, interner, &mut female);
                }
                value => {
                    if let Some(path) = model_path_from_value(value, interner) {
                        female.push(path);
                    }
                }
            }
        }
    }
    ArmorActorModels {
        male: male.into_iter().next(),
        female: female.into_iter().next(),
    }
}

fn collect_named_model_paths(
    value: &FieldValue,
    name: &str,
    interner: &StringInterner,
    output: &mut Vec<String>,
) {
    match value {
        FieldValue::Struct(fields) => {
            for (key, nested) in fields {
                if interner.resolve(*key) == Some(name) {
                    if let Some(path) = model_path_from_value(nested, interner) {
                        output.push(path);
                    }
                } else {
                    collect_named_model_paths(nested, name, interner, output);
                }
            }
        }
        FieldValue::List(items) => {
            for item in items {
                collect_named_model_paths(item, name, interner, output);
            }
        }
        _ => {}
    }
}

fn model_path_from_value(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(value) => interner
            .resolve(*value)
            .and_then(normalize_legacy_armor_model_path),
        FieldValue::Bytes(value) => String::from_utf8(value.to_vec())
            .ok()
            .and_then(|value| normalize_legacy_armor_model_path(&value)),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| model_path_from_value(value, interner)),
        FieldValue::List(items) => items
            .iter()
            .find_map(|value| model_path_from_value(value, interner)),
        _ => None,
    }
}

fn normalize_legacy_armor_model_path(path: &str) -> Option<String> {
    let mut path = path.trim().trim_matches('\0').replace('/', "\\");
    path = path.trim_start_matches('\\').to_string();
    for prefix in ["data\\", "meshes\\"] {
        if path
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            path = path[prefix.len()..].to_string();
        }
    }
    if !path.to_ascii_lowercase().ends_with(".nif") || path.contains(':') {
        return None;
    }
    Some(path)
}

fn attached_addons_cover_actor_models(
    armor: &Record,
    addon_models: &FxHashMap<FormKey, ArmorActorModels>,
    source_models: &ArmorActorModels,
) -> bool {
    uncovered_actor_models(armor, addon_models, source_models) == ArmorActorModels::default()
}

fn uncovered_actor_models(
    armor: &Record,
    addon_models: &FxHashMap<FormKey, ArmorActorModels>,
    source_models: &ArmorActorModels,
) -> ArmorActorModels {
    let mut uncovered = source_models.clone();
    for addon in record_addon_formkeys(armor) {
        let Some(models) = addon_models.get(&addon) else {
            continue;
        };
        if uncovered
            .male
            .as_ref()
            .zip(models.male.as_ref())
            .is_some_and(|(source, target)| source.eq_ignore_ascii_case(target))
        {
            uncovered.male = None;
        }
        if uncovered
            .female
            .as_ref()
            .zip(models.female.as_ref())
            .is_some_and(|(source, target)| source.eq_ignore_ascii_case(target))
        {
            uncovered.female = None;
        }
    }
    uncovered
}

fn synthetic_armor_addon_source_key(source_armor: FormKey, interner: &StringInterner) -> FormKey {
    let source_plugin = interner.resolve(source_armor.plugin).unwrap_or("unknown");
    FormKey {
        local: 1,
        plugin: interner.intern(&format!(
            "{SYNTHETIC_ARMOR_ADDON_PLUGIN}:{source_plugin}:{:06X}",
            source_armor.local
        )),
    }
}

fn build_synthetic_armor_addon(
    addon_fk: FormKey,
    source_armor: FormKey,
    target_armor: &Record,
    models: &ArmorActorModels,
    interner: &StringInterner,
) -> Record {
    let editor_suffix = target_armor
        .eid
        .and_then(|editor_id| interner.resolve(editor_id))
        .map(sanitize_editor_id_suffix)
        .filter(|suffix| !suffix.is_empty());
    let editor_id = match editor_suffix {
        Some(suffix) => format!("B21_Legacy_ARMA_{:06X}_{suffix}", source_armor.local),
        None => format!("B21_Legacy_ARMA_{:06X}", source_armor.local),
    };
    let editor_id = interner.intern(&editor_id);
    let mut addon = Record::new(SigCode::from_str("ARMA").expect("ARMA signature"), addon_fk);
    addon.eid = Some(editor_id);
    addon.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("EDID").expect("EDID signature"),
        value: FieldValue::String(editor_id),
    });
    addon.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("BOD2").expect("BOD2 signature"),
        value: target_armor
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "BOD2")
            .map(|entry| entry.value.clone())
            .unwrap_or(FieldValue::Uint(0)),
    });
    addon.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("RNAM").expect("RNAM signature"),
        value: FieldValue::FormKey(FormKey {
            local: FO4_HUMAN_RACE_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        }),
    });
    addon.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("DNAM").expect("DNAM signature"),
        value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0; 12])),
    });
    if let Some(model) = &models.male {
        addon.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MOD2").expect("MOD2 signature"),
            value: FieldValue::String(interner.intern(model)),
        });
    }
    if let Some(model) = &models.female {
        addon.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MOD3").expect("MOD3 signature"),
            value: FieldValue::String(interner.intern(model)),
        });
    }
    addon
}

fn sanitize_editor_id_suffix(editor_id: &str) -> String {
    editor_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

fn resolve_legacy_armor_addon_lists(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    target_master_handle_ids: &[u64],
    report: &mut FixupReport,
    arma_sig: SigCode,
    armo_sig: SigCode,
) -> Result<(), FixupError> {
    let flst_sig = SigCode::from_str("FLST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut valid_addons = session
        .form_keys_of_sig(arma_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
        .into_iter()
        .collect::<FxHashSet<_>>();
    for handle_id in target_master_handle_ids {
        valid_addons.extend(
            session
                .form_keys_of_sig_in_handle(*handle_id, arma_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?,
        );
    }

    let mut addon_lists = FxHashMap::default();
    for fk in session
        .form_keys_of_sig(flst_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        match session.record_decoded(&fk, target_schema, mapper.interner) {
            Ok(record) => {
                addon_lists.insert(fk, valid_armor_addons_from_list(&record, &valid_addons));
            }
            Err(e) => report.warnings.push(mapper.interner.intern(&format!(
                "sync_armo_hand_slots_from_addons:flst_read_err:{e}"
            ))),
        }
    }
    for handle_id in target_master_handle_ids {
        for fk in session
            .form_keys_of_sig_in_handle(*handle_id, flst_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            match session.record_decoded_in_handle(*handle_id, &fk, target_schema, mapper.interner)
            {
                Ok(record) => {
                    addon_lists.insert(fk, valid_armor_addons_from_list(&record, &valid_addons));
                }
                Err(e) => report.warnings.push(mapper.interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:master_flst_read_err:{e}"
                ))),
            }
        }
    }
    if addon_lists.is_empty() {
        return Ok(());
    }

    let mut changed_records = Vec::new();
    for fk in session
        .form_keys_of_sig(armo_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
            Ok(record) => record,
            Err(e) => {
                report.warnings.push(mapper.interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:legacy_armo_read_err:{e}"
                )));
                continue;
            }
        };
        let expansion = expand_legacy_armor_addon_lists(&mut record, &addon_lists);
        if expansion.lists_expanded == 0 {
            continue;
        }
        if expansion.addons_added == 0 {
            report.warnings.push(
                mapper.interner.intern(
                    "sync_armo_hand_slots_from_addons:legacy_addon_list_had_no_arma_members",
                ),
            );
        }
        report.records_changed += 1;
        changed_records.push(record);
    }

    let expected = changed_records.len();
    if expected > 0 {
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "sync_armo_hand_slots_from_addons resolved {replaced} of {expected} legacy armor addon lists"
            )));
        }
    }
    Ok(())
}

#[derive(Default)]
struct AddonListExpansion {
    lists_expanded: u32,
    addons_added: u32,
}

fn expand_legacy_armor_addon_lists(
    record: &mut Record,
    addon_lists: &FxHashMap<FormKey, Vec<FormKey>>,
) -> AddonListExpansion {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return AddonListExpansion::default(),
    };
    let indx_sig = match SubrecordSig::from_str("INDX") {
        Ok(sig) => sig,
        Err(_) => return AddonListExpansion::default(),
    };
    let mut seen_addons = record_addon_formkeys(record);
    for list in addon_lists.keys() {
        seen_addons.remove(list);
    }

    let mut expansion = AddonListExpansion::default();
    let mut output: smallvec::SmallVec<[FieldEntry; 8]> =
        smallvec::SmallVec::with_capacity(record.fields.len());
    for entry in record.fields.drain(..) {
        let list = (entry.sig == modl_sig)
            .then(|| first_formkey_from_value(&entry.value))
            .flatten()
            .and_then(|form_key| addon_lists.get(&form_key));
        let Some(list) = list else {
            output.push(entry);
            continue;
        };

        expansion.lists_expanded += 1;
        if output.last().is_some_and(|entry| entry.sig == indx_sig) {
            output.pop();
        }
        for addon in list {
            if !seen_addons.insert(*addon) {
                continue;
            }
            output.push(FieldEntry {
                sig: indx_sig,
                value: FieldValue::Uint(0),
            });
            output.push(FieldEntry {
                sig: modl_sig,
                value: FieldValue::FormKey(*addon),
            });
            expansion.addons_added += 1;
        }
    }
    record.fields = output;
    expansion
}

fn valid_armor_addons_from_list(
    record: &Record,
    valid_addons: &FxHashSet<FormKey>,
) -> Vec<FormKey> {
    let mut members = Vec::new();
    for entry in record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "LNAM")
    {
        collect_formkeys_in_order(&entry.value, &mut members);
    }
    let mut seen = FxHashSet::default();
    members.retain(|member| valid_addons.contains(member) && seen.insert(*member));
    members
}

fn first_formkey_from_value(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::List(items) => items.iter().find_map(first_formkey_from_value),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| first_formkey_from_value(value)),
        _ => None,
    }
}

fn collect_formkeys_in_order(value: &FieldValue, form_keys: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(form_key) => form_keys.push(*form_key),
        FieldValue::List(items) => {
            for item in items {
                collect_formkeys_in_order(item, form_keys);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_formkeys_in_order(value, form_keys);
            }
        }
        _ => {}
    }
}

fn collect_hand_addons(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    target_master_handle_ids: &[u64],
    report: &mut FixupReport,
    arma_sig: SigCode,
) -> Result<HandAddonIndex, FixupError> {
    let interner = mapper.interner;
    let arma_fks = session
        .form_keys_of_sig(arma_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut hand_addons = HandAddonIndex::default();

    for fk in arma_fks {
        let record = match session.record_decoded(&fk, target_schema, interner) {
            Ok(record) => record,
            Err(e) => {
                let warning = interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:arma_read_err:{e}"
                ));
                report.warnings.push(warning);
                continue;
            }
        };

        index_arma_record(fk, &record, interner, &mut hand_addons, true);
    }

    for handle_id in target_master_handle_ids {
        let arma_fks = session
            .form_keys_of_sig_in_handle(*handle_id, arma_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in arma_fks {
            let record =
                match session.record_decoded_in_handle(*handle_id, &fk, target_schema, interner) {
                    Ok(record) => record,
                    Err(e) => {
                        let warning = interner.intern(&format!(
                            "sync_armo_hand_slots_from_addons:master_arma_read_err:{e}"
                        ));
                        report.warnings.push(warning);
                        continue;
                    }
                };
            index_arma_record(fk, &record, interner, &mut hand_addons, false);
        }
    }

    Ok(hand_addons)
}

fn index_arma_record(
    fk: FormKey,
    record: &Record,
    interner: &StringInterner,
    hand_addons: &mut HandAddonIndex,
    can_mutate: bool,
) {
    let owns_hand_slots = record_has_hand_bod2(record, interner);
    if owns_hand_slots {
        hand_addons.hand_addons.insert(fk);
        if record_has_non_hand_bod2(record, interner) {
            if can_mutate {
                hand_addons.mixed_hand_addons.insert(fk);
            }
        } else {
            hand_addons.hand_only_addons.insert(fk);
        }
        if is_bare_ghoul_hand_addon_formkey(&fk, interner) {
            hand_addons.ghoul_hand_addon = Some(fk);
        }
        return;
    }

    if record_supports_ghoul_race(record, interner) {
        hand_addons.ghoul_body_addons.insert(fk);
    }
}

pub fn sync_armo_hand_slots_from_addons(
    record: &mut Record,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    if !record_allows_human_hand_slots(record, interner) {
        return false;
    }
    if !record_references_any_addon(record, hand_addons) {
        return false;
    }

    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    for entry in &mut record.fields {
        if entry.sig == bod2_sig {
            return ensure_hand_slots(&mut entry.value, interner);
        }
    }
    false
}

fn collect_mixed_hand_addon_strip_candidates(
    record: &Record,
    hand_only_addons: &FxHashSet<FormKey>,
    mixed_hand_addons: &FxHashSet<FormKey>,
    strip_candidates: &mut FxHashSet<FormKey>,
    protected_candidates: &mut FxHashSet<FormKey>,
    interner: &StringInterner,
) {
    if !record_allows_human_hand_slots(record, interner) {
        return;
    }

    let addon_refs = record_addon_formkeys(record);
    if addon_refs.is_empty() {
        return;
    }

    let has_dedicated_hand_addon = addon_refs.iter().any(|fk| hand_only_addons.contains(fk));
    for fk in addon_refs {
        if !mixed_hand_addons.contains(&fk) {
            continue;
        }
        if has_dedicated_hand_addon {
            strip_candidates.insert(fk);
        } else {
            protected_candidates.insert(fk);
        }
    }
}

fn strip_redundant_mixed_hand_slots_from_addons(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    strip_candidates: &FxHashSet<FormKey>,
    protected_candidates: &FxHashSet<FormKey>,
    report: &mut FixupReport,
) -> Result<Vec<Record>, FixupError> {
    let mut changed_records = Vec::new();
    for fk in strip_candidates {
        if protected_candidates.contains(fk) {
            continue;
        }

        let mut record = match session.record_decoded(fk, target_schema, mapper.interner) {
            Ok(record) => record,
            Err(e) => {
                let warning = mapper.interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:strip_arma_read_err:{e}"
                ));
                report.warnings.push(warning);
                continue;
            }
        };
        if strip_hand_slots_from_record(&mut record, mapper.interner) {
            changed_records.push(record);
            report.records_changed += 1;
        }
    }
    Ok(changed_records)
}

pub fn prune_redundant_human_hand_addons(
    record: &mut Record,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    if !record_allows_human_hand_slots(record, interner) {
        return false;
    }
    if !record_references_bare_human_hand_addon(record, interner) {
        return false;
    }
    if !record_references_non_bare_hand_addon(record, hand_addons, interner) {
        return false;
    }

    remove_bare_human_hand_addon_entries(record, interner)
}

pub fn ensure_ghoul_hand_addon_for_ghoul_capable_armo(
    record: &mut Record,
    ghoul_body_addons: &FxHashSet<FormKey>,
    ghoul_hand_addon: Option<FormKey>,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    let Some(ghoul_hand_addon) = ghoul_hand_addon else {
        return false;
    };
    if record_references_bare_ghoul_hand_addon(record, interner) {
        return false;
    }
    if !record_references_bare_human_hand_addon(record, interner) {
        return false;
    }
    if record_references_non_bare_hand_addon(record, hand_addons, interner) {
        return false;
    }
    if !record_references_any_addon(record, ghoul_body_addons) {
        return false;
    }

    insert_addon_entry(record, ghoul_hand_addon)
}

fn record_has_hand_bod2(record: &Record, interner: &StringInterner) -> bool {
    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == bod2_sig)
        .any(|entry| {
            value_has_slot(&entry.value, SLOT_34_LEFT_HAND, interner)
                && value_has_slot(&entry.value, SLOT_35_RIGHT_HAND, interner)
        })
}

fn record_has_non_hand_bod2(record: &Record, interner: &StringInterner) -> bool {
    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == bod2_sig)
        .any(|entry| value_has_non_hand_slot(&entry.value, interner))
}

fn record_supports_ghoul_race(record: &Record, interner: &StringInterner) -> bool {
    race_formkey_from_record(record, interner).is_some_and(|fk| is_ghoul_race_formkey(fk, interner))
        || record_has_modl_ghoul_race(record, interner)
}

fn record_has_modl_ghoul_race(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| modl_value_has_ghoul_race(&entry.value, interner))
}

fn modl_value_has_ghoul_race(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::FormKey(fk) => is_ghoul_race_formkey(*fk, interner),
        FieldValue::List(items) => items.iter().any(|item| match item {
            FieldValue::FormKey(fk) => is_ghoul_race_formkey(*fk, interner),
            _ => false,
        }),
        _ => false,
    }
}

fn record_allows_human_hand_slots(record: &Record, interner: &StringInterner) -> bool {
    match race_formkey_from_record(record, interner) {
        Some(fk) => is_human_race_formkey(fk, interner),
        None => true,
    }
}

fn race_formkey_from_record(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    let rnam_sig = SubrecordSig::from_str("RNAM").ok()?;
    let race_sym = interner.intern(keys::RACE);

    record
        .fields
        .iter()
        .find(|entry| entry.sig == rnam_sig)
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(fk) => Some(*fk),
            FieldValue::Struct(fields) => struct_find(fields, race_sym).and_then(|value| {
                if let FieldValue::FormKey(fk) = value {
                    Some(*fk)
                } else {
                    None
                }
            }),
            _ => None,
        })
}

fn is_human_race_formkey(fk: FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    fk.local == 0x013746
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn is_ghoul_race_formkey(fk: FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    fk.local == GHOUL_RACE
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn struct_find<'a>(fields: &'a [(Sym, FieldValue)], key: Sym) -> Option<&'a FieldValue> {
    fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn struct_find_mut<'a>(
    fields: &'a mut Vec<(Sym, FieldValue)>,
    key: Sym,
) -> Option<&'a mut FieldValue> {
    fields.iter_mut().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn record_references_any_addon(record: &Record, hand_addons: &FxHashSet<FormKey>) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_any_addon(&entry.value, hand_addons))
}

fn value_references_any_addon(value: &FieldValue, hand_addons: &FxHashSet<FormKey>) -> bool {
    match value {
        FieldValue::FormKey(fk) => hand_addons.contains(fk),
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_any_addon(item, hand_addons)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_any_addon(value, hand_addons)),
        _ => false,
    }
}

fn record_addon_formkeys(record: &Record) -> FxHashSet<FormKey> {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return FxHashSet::default(),
    };
    let mut refs = FxHashSet::default();
    for entry in record.fields.iter().filter(|entry| entry.sig == modl_sig) {
        collect_formkeys_from_value(&entry.value, &mut refs);
    }
    refs
}

fn collect_formkeys_from_value(value: &FieldValue, refs: &mut FxHashSet<FormKey>) {
    match value {
        FieldValue::FormKey(fk) => {
            refs.insert(*fk);
        }
        FieldValue::List(items) => {
            for item in items {
                collect_formkeys_from_value(item, refs);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_formkeys_from_value(value, refs);
            }
        }
        _ => {}
    }
}

fn record_references_bare_human_hand_addon(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_bare_human_hand_addon(&entry.value, interner))
}

fn record_references_bare_ghoul_hand_addon(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_bare_ghoul_hand_addon(&entry.value, interner))
}

fn record_references_non_bare_hand_addon(
    record: &Record,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_non_bare_hand_addon(&entry.value, hand_addons, interner))
}

fn value_references_bare_human_hand_addon(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::FormKey(fk) => is_bare_human_hand_addon_formkey(fk, interner),
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_bare_human_hand_addon(item, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_bare_human_hand_addon(value, interner)),
        _ => false,
    }
}

fn value_references_bare_ghoul_hand_addon(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::FormKey(fk) => is_bare_ghoul_hand_addon_formkey(fk, interner),
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_bare_ghoul_hand_addon(item, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_bare_ghoul_hand_addon(value, interner)),
        _ => false,
    }
}

fn value_references_non_bare_hand_addon(
    value: &FieldValue,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => {
            hand_addons.contains(fk)
                && !is_bare_human_hand_addon_formkey(fk, interner)
                && !is_bare_ghoul_hand_addon_formkey(fk, interner)
        }
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_non_bare_hand_addon(item, hand_addons, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_non_bare_hand_addon(value, hand_addons, interner)),
        _ => false,
    }
}

fn insert_addon_entry(record: &mut Record, addon_fk: FormKey) -> bool {
    let indx_sig = match SubrecordSig::from_str("INDX") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    let insert_at = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.sig == modl_sig)
        .map(|(index, _)| index + 1)
        .last()
        .unwrap_or(record.fields.len());

    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: indx_sig,
            value: FieldValue::Uint(0),
        },
    );
    record.fields.insert(
        insert_at + 1,
        FieldEntry {
            sig: modl_sig,
            value: FieldValue::FormKey(addon_fk),
        },
    );
    true
}

fn is_bare_human_hand_addon_formkey(fk: &FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    BARE_HUMAN_HAND_ADDONS.contains(&fk.local)
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn is_bare_ghoul_hand_addon_formkey(fk: &FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    fk.local == BARE_GHOUL_HAND_ADDON
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn remove_bare_human_hand_addon_entries(record: &mut Record, interner: &StringInterner) -> bool {
    let indx_sig = match SubrecordSig::from_str("INDX") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    let mut changed = false;
    let mut index = 0;
    while index < record.fields.len() {
        let is_indexed_bare_hand_addon = index + 1 < record.fields.len()
            && record.fields[index].sig == indx_sig
            && record.fields[index + 1].sig == modl_sig
            && value_references_bare_human_hand_addon(&record.fields[index + 1].value, interner);
        if is_indexed_bare_hand_addon {
            record.fields.remove(index + 1);
            record.fields.remove(index);
            changed = true;
            continue;
        }

        let is_bare_hand_addon = record.fields[index].sig == modl_sig
            && value_references_bare_human_hand_addon(&record.fields[index].value, interner);
        if is_bare_hand_addon {
            record.fields.remove(index);
            changed = true;
            continue;
        }

        index += 1;
    }
    changed
}

fn strip_hand_slots_from_record(record: &mut Record, interner: &StringInterner) -> bool {
    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    for entry in &mut record.fields {
        if entry.sig == bod2_sig {
            return remove_hand_slots(&mut entry.value, interner);
        }
    }
    false
}

fn remove_hand_slots(value: &mut FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => {
            let before = *mask;
            *mask &= !HAND_SLOT_MASK;
            *mask != before
        }
        FieldValue::Int(mask) if *mask >= 0 => {
            let before = *mask;
            *mask &= !(HAND_SLOT_MASK as i64);
            *mask != before
        }
        FieldValue::List(items) => {
            let before = items.len();
            items.retain(|item| {
                let FieldValue::String(sym) = item else {
                    return true;
                };
                !interner
                    .resolve(*sym)
                    .and_then(biped_slot_from_token)
                    .is_some_and(|slot| slot == SLOT_34_LEFT_HAND || slot == SLOT_35_RIGHT_HAND)
            });
            items.len() != before
        }
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            if let Some(flags_value) = struct_find_mut(fields, first_person_flags_sym) {
                remove_hand_slots(flags_value, interner)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn ensure_hand_slots(value: &mut FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => {
            let before = *mask;
            *mask |= HAND_SLOT_MASK;
            *mask != before
        }
        FieldValue::Int(mask) if *mask >= 0 => {
            let before = *mask;
            *mask |= HAND_SLOT_MASK as i64;
            *mask != before
        }
        FieldValue::List(items) => ensure_hand_slots_in_list(items, interner),
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            if let Some(flags_value) = struct_find_mut(fields, first_person_flags_sym) {
                ensure_hand_slots(flags_value, interner)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn ensure_hand_slots_in_list(items: &mut Vec<FieldValue>, interner: &StringInterner) -> bool {
    let mut changed = false;
    if !list_has_slot(items, SLOT_34_LEFT_HAND, interner) {
        items.push(FieldValue::String(interner.intern("34LHand")));
        changed = true;
    }
    if !list_has_slot(items, SLOT_35_RIGHT_HAND, interner) {
        items.push(FieldValue::String(interner.intern("35RHand")));
        changed = true;
    }
    changed
}

fn value_has_slot(value: &FieldValue, slot: u8, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => *mask & mask_for_slot(slot) != 0,
        FieldValue::Int(mask) if *mask >= 0 => (*mask as u64) & mask_for_slot(slot) != 0,
        FieldValue::List(items) => list_has_slot(items, slot, interner),
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            struct_find(fields, first_person_flags_sym)
                .map(|value| value_has_slot(value, slot, interner))
                .unwrap_or(false)
        }
        _ => false,
    }
}

fn value_has_non_hand_slot(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => *mask & !HAND_SLOT_MASK != 0,
        FieldValue::Int(mask) if *mask >= 0 => (*mask as u64) & !HAND_SLOT_MASK != 0,
        FieldValue::List(items) => items.iter().any(|item| {
            let FieldValue::String(sym) = item else {
                return false;
            };
            interner
                .resolve(*sym)
                .and_then(biped_slot_from_token)
                .is_some_and(|slot| slot != SLOT_34_LEFT_HAND && slot != SLOT_35_RIGHT_HAND)
        }),
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            struct_find(fields, first_person_flags_sym)
                .map(|value| value_has_non_hand_slot(value, interner))
                .unwrap_or(false)
        }
        _ => false,
    }
}

fn list_has_slot(items: &[FieldValue], slot: u8, interner: &StringInterner) -> bool {
    items.iter().any(|item| {
        let FieldValue::String(sym) = item else {
            return false;
        };
        interner
            .resolve(*sym)
            .and_then(biped_slot_from_token)
            .is_some_and(|item_slot| item_slot == slot)
    })
}

fn biped_slot_from_token(token: &str) -> Option<u8> {
    let digits: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u8>().ok()
}

fn mask_for_slot(slot: u8) -> u64 {
    1 << (slot - 30)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_new_native,
    };

    fn make_fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    fn make_record(sig: &str, fk: FormKey) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn push_bod2_tokens(record: &mut Record, tokens: &[&str], interner: &StringInterner) {
        push_field(
            record,
            "BOD2",
            FieldValue::List(
                tokens
                    .iter()
                    .map(|token| FieldValue::String(interner.intern(token)))
                    .collect(),
            ),
        );
    }

    fn push_rnam(record: &mut Record, local: u32, plugin: &str, interner: &StringInterner) {
        push_field(
            record,
            "RNAM",
            FieldValue::FormKey(FormKey {
                local,
                plugin: interner.intern(plugin),
            }),
        );
    }

    fn push_addon(record: &mut Record, addon_fk: FormKey) {
        push_field(record, "INDX", FieldValue::Uint(0));
        push_field(record, "MODL", FieldValue::FormKey(addon_fk));
    }

    fn set_editor_id(record: &mut Record, editor_id: &str, interner: &StringInterner) {
        let editor_id = interner.intern(editor_id);
        record.eid = Some(editor_id);
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::String(editor_id),
            },
        );
    }

    fn seed_records(handle: u64, records: Vec<Record>, interner: &StringInterner) {
        let mut session = open_session(handle, None).unwrap();
        let schema = session.schema().unwrap();
        session
            .add_records(records, schema.as_ref(), interner)
            .unwrap();
    }

    fn bod2_tokens(record: &Record, interner: &StringInterner) -> Vec<String> {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "BOD2")
            .and_then(|entry| {
                let FieldValue::List(items) = &entry.value else {
                    return None;
                };
                Some(
                    items
                        .iter()
                        .filter_map(|item| {
                            let FieldValue::String(sym) = item else {
                                return None;
                            };
                            interner.resolve(*sym).map(str::to_string)
                        })
                        .collect(),
                )
            })
            .unwrap_or_default()
    }

    fn addon_formkeys(record: &Record) -> Vec<FormKey> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "MODL")
            .filter_map(|entry| match entry.value {
                FieldValue::FormKey(fk) => Some(fk),
                _ => None,
            })
            .collect()
    }

    fn addon_index_count(record: &Record) -> usize {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "INDX")
            .count()
    }

    fn model_path(record: &Record, sig: &str, interner: &StringInterner) -> Option<String> {
        record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == sig)
            .and_then(|field| model_path_from_value(&field.value, interner))
    }

    #[test]
    fn builds_synthetic_legacy_arma_with_proto_model_routing() {
        let interner = StringInterner::new();
        let source_fk = make_fk("104C23", "FalloutNV.esm", &interner);
        let addon_fk = make_fk("A00001", "FNV_FO3_Merged.esm", &interner);
        let mut armor = make_record("ARMO", make_fk("104C23", "FNV_FO3_Merged.esm", &interner));
        armor.eid = Some(interner.intern("OutfitBennySuit"));
        push_field(&mut armor, "BOD2", FieldValue::Uint(1 << 6));
        let models = ArmorActorModels {
            male: Some("armor\\1950stylesuit\\m\\NV_Outfit_Benny.NIF".into()),
            female: Some("armor\\1950stylesuit\\f\\bennysuit_f.NIF".into()),
        };

        let addon = build_synthetic_armor_addon(addon_fk, source_fk, &armor, &models, &interner);

        assert_eq!(addon.form_key, addon_fk);
        assert_eq!(
            addon.eid.and_then(|editor_id| interner.resolve(editor_id)),
            Some("B21_Legacy_ARMA_104C23_OutfitBennySuit")
        );
        assert_eq!(
            model_path(&addon, "MOD2", &interner).as_deref(),
            models.male.as_deref()
        );
        assert_eq!(
            model_path(&addon, "MOD3", &interner).as_deref(),
            models.female.as_deref()
        );
        assert!(
            !addon
                .fields
                .iter()
                .any(|field| matches!(field.sig.as_str(), "MOD4" | "MOD5"))
        );
        assert_eq!(
            addon
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "DNAM")
                .and_then(|field| match &field.value {
                    FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
                    _ => None,
                }),
            Some([0; 12].as_slice())
        );
        assert_eq!(
            addon
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "RNAM")
                .and_then(|field| first_formkey_from_value(&field.value)),
            Some(make_fk("013746", "Fallout4.esm", &interner))
        );
    }

    #[test]
    fn extracts_mixed_legacy_actor_models_and_ignores_missing_paths() {
        let interner = StringInterner::new();
        let mut armor = make_record("ARMO", make_fk("000800", "FalloutNV.esm", &interner));
        push_field(
            &mut armor,
            "MODL",
            FieldValue::List(vec![FieldValue::Struct(vec![
                (
                    interner.intern("MODL"),
                    FieldValue::String(interner.intern("Meshes/Armor/Male.NIF")),
                ),
                (
                    interner.intern("MOD3"),
                    FieldValue::String(interner.intern("Armor/Female.NIF")),
                ),
            ])]),
        );
        assert_eq!(
            legacy_armo_actor_models(&armor, &interner),
            ArmorActorModels {
                male: Some("Armor\\Male.NIF".into()),
                female: Some("Armor\\Female.NIF".into()),
            }
        );

        armor.fields.clear();
        push_field(
            &mut armor,
            "MODL",
            FieldValue::List(vec![FieldValue::Struct(vec![(
                interner.intern("MOD3"),
                FieldValue::String(interner.intern("Armor/FemaleOnly.NIF")),
            )])]),
        );
        assert_eq!(
            legacy_armo_actor_models(&armor, &interner),
            ArmorActorModels {
                male: None,
                female: Some("Armor\\FemaleOnly.NIF".into()),
            }
        );

        armor.fields.clear();
        push_field(&mut armor, "MODL", FieldValue::String(interner.intern("")));
        push_field(
            &mut armor,
            "MOD3",
            FieldValue::String(interner.intern("textures\\not_a_mesh.dds")),
        );
        assert_eq!(
            legacy_armo_actor_models(&armor, &interner),
            ArmorActorModels::default()
        );
    }

    #[test]
    fn attached_addon_must_cover_actor_models_not_merely_exist() {
        let interner = StringInterner::new();
        let body = make_fk("000900", "Output.esp", &interner);
        let hands = make_fk("000901", "Output.esp", &interner);
        let mut armor = make_record("ARMO", make_fk("000800", "Output.esp", &interner));
        push_addon(&mut armor, hands);
        let source_models = ArmorActorModels {
            male: Some("armor\\body_m.nif".into()),
            female: Some("armor\\body_f.nif".into()),
        };
        let mut addon_models = FxHashMap::default();
        addon_models.insert(
            hands,
            ArmorActorModels {
                male: Some("armor\\hands.nif".into()),
                female: Some("armor\\hands.nif".into()),
            },
        );
        assert!(!attached_addons_cover_actor_models(
            &armor,
            &addon_models,
            &source_models
        ));

        push_addon(&mut armor, body);
        addon_models.insert(
            body,
            ArmorActorModels {
                male: source_models.male.clone(),
                female: Some("armor\\other_f.nif".into()),
            },
        );
        assert_eq!(
            uncovered_actor_models(&armor, &addon_models, &source_models),
            ArmorActorModels {
                male: None,
                female: source_models.female.clone(),
            }
        );
        assert!(!attached_addons_cover_actor_models(
            &armor,
            &addon_models,
            &source_models
        ));

        addon_models.insert(body, source_models.clone());
        assert!(attached_addons_cover_actor_models(
            &armor,
            &addon_models,
            &source_models
        ));
    }

    #[test]
    fn synthetic_armor_addon_keys_are_deterministic_and_collision_safe() {
        let interner = StringInterner::new();
        let source = make_fk("104C23", "FalloutNV.esm", &interner);
        let same_local_other_plugin = make_fk("104C23", "Fallout3.esm", &interner);
        let source_key = synthetic_armor_addon_source_key(source, &interner);
        assert_eq!(
            source_key,
            synthetic_armor_addon_source_key(source, &interner)
        );
        assert_ne!(
            source_key,
            synthetic_armor_addon_source_key(same_local_other_plugin, &interner)
        );

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Output.esp".into(),
                preserve_source_ids: true,
                generated_object_id_floor: 0x00A0_0000,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.reserve_object_ids([0x00A0_0000, source.local]);
        assert_eq!(
            mapper
                .allocate_or_resolve(source_key, None, SigCode::from_str("ARMA").unwrap())
                .local,
            0x00A0_0001
        );
    }

    #[test]
    fn legacy_armor_schema_gate_excludes_fo76_and_target_fo4() {
        assert!(is_legacy_armor_schema(
            &AuthoringSchema::for_game("fnv").unwrap()
        ));
        assert!(is_legacy_armor_schema(
            &AuthoringSchema::for_game("fo3").unwrap()
        ));
        assert!(!is_legacy_armor_schema(
            &AuthoringSchema::for_game("fo76").unwrap()
        ));
        assert!(!is_legacy_armor_schema(
            &AuthoringSchema::for_game("fo4").unwrap()
        ));
    }

    #[test]
    fn session_synthesis_preserves_bipl_addons_and_covers_actor_models() {
        let interner = StringInterner::new();
        let source_name = "FalloutNV.esm";
        let target_name = "FNV_FO3_Merged.esm";
        let source_handle = plugin_handle_new_native(source_name, Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();

        let source_armor_fk = make_fk("1649DD", source_name, &interner);
        let target_armor_fk = make_fk("1649DD", target_name, &interner);
        let hand_addon_fk = make_fk("1649DF", target_name, &interner);
        let mut source_armor = make_record("ARMO", source_armor_fk);
        set_editor_id(&mut source_armor, "ArmorLegate", &interner);
        push_field(
            &mut source_armor,
            "BMDT",
            FieldValue::Bytes(smallvec::smallvec![4, 0, 0, 0, 0, 0, 0, 0]),
        );
        push_field(
            &mut source_armor,
            "MODL",
            FieldValue::String(interner.intern("armor/LegateArmor/LegateArmor.NIF")),
        );
        push_field(
            &mut source_armor,
            "MOD3",
            FieldValue::String(interner.intern("armor\\LegateArmor\\LegateArmor.NIF")),
        );
        push_field(
            &mut source_armor,
            "DATA",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
                250, 0, 0, 0, 100, 0, 0, 0, 0, 0, 52, 66,
            ])),
        );
        seed_records(source_handle, vec![source_armor], &interner);

        let mut target_armor = make_record("ARMO", target_armor_fk);
        set_editor_id(&mut target_armor, "ArmorLegate", &interner);
        push_field(&mut target_armor, "BOD2", FieldValue::Uint(1 << 6));
        push_addon(&mut target_armor, hand_addon_fk);
        let mut hand_addon = make_record("ARMA", hand_addon_fk);
        set_editor_id(&mut hand_addon, "LegateArmorRightH", &interner);
        push_field(&mut hand_addon, "BOD2", FieldValue::Uint(1 << 5));
        push_rnam(&mut hand_addon, 0x013746, "Fallout4.esm", &interner);
        push_field(
            &mut hand_addon,
            "DNAM",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0; 12])),
        );
        push_field(
            &mut hand_addon,
            "MOD2",
            FieldValue::String(interner.intern("armor\\LegateArmor\\LegateArmor.NIF")),
        );
        seed_records(target_handle, vec![target_armor, hand_addon], &interner);

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_name.into(),
                source_plugin_name: source_name.into(),
                generated_object_id_floor: 0x00A0_0000,
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(source_armor_fk, target_armor_fk);
        mapper.reserve_object_ids([target_armor_fk.local, hand_addon_fk.local]);
        let config = FixupConfig {
            target_schema: Some(AuthoringSchema::for_game("fo4").unwrap()),
            source_schema: Some(AuthoringSchema::for_game("fnv").unwrap()),
            ..Default::default()
        };
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let first = SyncArmoHandSlotsFromAddonsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(first.records_added, 1);

        let arma_sig = SigCode::from_str("ARMA").unwrap();
        let arma_fks = session.form_keys_of_sig(arma_sig, &interner).unwrap();
        assert_eq!(arma_fks.len(), 2);
        let synthesized_fk = *arma_fks
            .iter()
            .find(|form_key| **form_key != hand_addon_fk)
            .unwrap();
        assert_eq!(synthesized_fk.local, 0x00A0_0000);
        let synthesized = session
            .record_decoded(
                &synthesized_fk,
                config.target_schema.as_deref().unwrap(),
                &interner,
            )
            .unwrap();
        assert_eq!(
            model_path(&synthesized, "MOD2", &interner).as_deref(),
            Some("armor\\LegateArmor\\LegateArmor.NIF")
        );
        assert_eq!(
            model_path(&synthesized, "MOD3", &interner).as_deref(),
            Some("armor\\LegateArmor\\LegateArmor.NIF")
        );
        assert!(
            !synthesized
                .fields
                .iter()
                .any(|field| matches!(field.sig.as_str(), "MOD4" | "MOD5"))
        );

        let armor = session
            .record_decoded(
                &target_armor_fk,
                config.target_schema.as_deref().unwrap(),
                &interner,
            )
            .unwrap();
        let attached = addon_formkeys(&armor);
        assert_eq!(attached, vec![hand_addon_fk, synthesized_fk]);
        let second = SyncArmoHandSlotsFromAddonsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(second.records_added, 0);
        assert_eq!(
            addon_formkeys(
                &session
                    .record_decoded(
                        &target_armor_fk,
                        config.target_schema.as_deref().unwrap(),
                        &interner,
                    )
                    .unwrap()
            ),
            attached
        );
    }

    #[test]
    fn session_synthesis_covers_fo3_male_only_actor_model() {
        let interner = StringInterner::new();
        let source_name = "Fallout3.esm";
        let target_name = "FNV_FO3_Merged.esm";
        let source_handle = plugin_handle_new_native(source_name, Some("fo3")).unwrap();
        let target_handle = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();

        let source_armor_fk = make_fk("00431E", source_name, &interner);
        let target_armor_fk = make_fk("00431E", target_name, &interner);
        let mut source_armor = make_record("ARMO", source_armor_fk);
        set_editor_id(&mut source_armor, "VaultSuit101", &interner);
        push_field(
            &mut source_armor,
            "BMDT",
            FieldValue::Bytes(smallvec::smallvec![4, 0, 0, 0, 0, 0, 0, 0]),
        );
        push_field(
            &mut source_armor,
            "MODL",
            FieldValue::String(interner.intern("armor\\vaultsuit\\VaultSuit101M.NIF")),
        );
        push_field(
            &mut source_armor,
            "DATA",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
                50, 0, 0, 0, 100, 0, 0, 0, 0, 0, 32, 65,
            ])),
        );
        seed_records(source_handle, vec![source_armor], &interner);

        let mut target_armor = make_record("ARMO", target_armor_fk);
        set_editor_id(&mut target_armor, "VaultSuit101", &interner);
        push_field(&mut target_armor, "BOD2", FieldValue::Uint(1 << 6));
        seed_records(target_handle, vec![target_armor], &interner);

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_name.into(),
                source_plugin_name: source_name.into(),
                generated_object_id_floor: 0x00A0_0000,
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(source_armor_fk, target_armor_fk);
        mapper.reserve_object_ids([target_armor_fk.local]);
        let config = FixupConfig {
            target_schema: Some(AuthoringSchema::for_game("fo4").unwrap()),
            source_schema: Some(AuthoringSchema::for_game("fo3").unwrap()),
            ..Default::default()
        };
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();

        let report = SyncArmoHandSlotsFromAddonsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();

        assert_eq!(report.records_added, 1);
        let armor = session
            .record_decoded(
                &target_armor_fk,
                config.target_schema.as_deref().unwrap(),
                &interner,
            )
            .unwrap();
        let synthesized_fk = addon_formkeys(&armor).into_iter().next().unwrap();
        let synthesized = session
            .record_decoded(
                &synthesized_fk,
                config.target_schema.as_deref().unwrap(),
                &interner,
            )
            .unwrap();
        assert_eq!(
            model_path(&synthesized, "MOD2", &interner).as_deref(),
            Some("armor\\vaultsuit\\VaultSuit101M.NIF")
        );
        assert_eq!(model_path(&synthesized, "MOD3", &interner), None);
    }

    #[test]
    fn expands_legacy_bipl_list_into_direct_arma_rows() {
        let interner = StringInterner::new();
        let list = make_fk("1649E0", "FNV_FO3_Merged.esm", &interner);
        let male = make_fk("1649DF", "FNV_FO3_Merged.esm", &interner);
        let female = make_fk("1649DE", "FNV_FO3_Merged.esm", &interner);
        let mut lists = FxHashMap::default();
        lists.insert(list, vec![male, female]);
        let mut armo = make_record("ARMO", make_fk("1649DD", "FNV_FO3_Merged.esm", &interner));
        push_field(&mut armo, "INDX", FieldValue::Uint(0));
        push_field(&mut armo, "MODL", FieldValue::FormKey(list));

        let expansion = expand_legacy_armor_addon_lists(&mut armo, &lists);

        assert_eq!(expansion.lists_expanded, 1);
        assert_eq!(expansion.addons_added, 2);
        assert_eq!(addon_formkeys(&armo), vec![male, female]);
        assert_eq!(addon_index_count(&armo), 2);
        assert_eq!(
            armo.fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["INDX", "MODL", "INDX", "MODL"]
        );
    }

    #[test]
    fn omits_non_arma_members_from_legacy_addon_list() {
        let interner = StringInterner::new();
        let arma = make_fk("01D980", "FNV_FO3_Merged.esm", &interner);
        let not_arma = make_fk("000007", "Fallout4.esm", &interner);
        let mut flst = make_record("FLST", make_fk("01D981", "FNV_FO3_Merged.esm", &interner));
        push_field(&mut flst, "LNAM", FieldValue::FormKey(arma));
        push_field(&mut flst, "LNAM", FieldValue::FormKey(not_arma));
        let valid = FxHashSet::from_iter([arma]);

        assert_eq!(valid_armor_addons_from_list(&flst, &valid), vec![arma]);
    }

    #[test]
    fn detects_converted_hand_addon_from_bod2_slots() {
        let interner = StringInterner::new();
        let fk = make_fk("58B272", "SeventySix.esm", &interner);
        let mut addon = make_record("ARMA", fk);
        push_bod2_tokens(&mut addon, &["34LHand", "35RHand"], &interner);

        assert!(record_has_hand_bod2(&addon, &interner));
    }

    #[test]
    fn adds_hand_slots_when_armo_references_hand_addon() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("58B272", "SeventySix.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(addon_fk);

        let mut armo = make_record("ARMO", make_fk("58B273", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(
            bod2_tokens(&armo, &interner),
            vec!["33BODY", "34LHand", "35RHand"]
        );
    }

    #[test]
    fn adds_hand_slots_when_armo_references_master_hand_addon() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("07239F", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(addon_fk);

        let mut armo = make_record("ARMO", make_fk("3B7D7F", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(
            bod2_tokens(&armo, &interner),
            vec!["33BODY", "34LHand", "35RHand"]
        );
    }

    #[test]
    fn skips_armo_without_matching_hand_addon() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("58B271", "SeventySix.esm", &interner);
        let hand_addons = FxHashSet::default();

        let mut armo = make_record("ARMO", make_fk("58B273", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(!sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(bod2_tokens(&armo, &interner), vec!["33BODY"]);
    }

    #[test]
    fn skips_explicit_nonhuman_armo() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("58B272", "SeventySix.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(addon_fk);

        let mut armo = make_record("ARMO", make_fk("58B273", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x6356DD, "SeventySix.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(!sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(bod2_tokens(&armo, &interner), vec!["33BODY"]);
    }

    #[test]
    fn strips_hand_slots_from_mixed_body_addon_when_dedicated_gloves_exist() {
        let interner = StringInterner::new();
        let body_fk = make_fk("787E52", "SeventySix.esm", &interner);
        let gloves_fk = make_fk("7AC69F", "SeventySix.esm", &interner);

        let mut body_addon = make_record("ARMA", body_fk);
        push_bod2_tokens(
            &mut body_addon,
            &["33BODY", "34LHand", "35RHand"],
            &interner,
        );
        let mut gloves_addon = make_record("ARMA", gloves_fk);
        push_bod2_tokens(&mut gloves_addon, &["34LHand", "35RHand"], &interner);

        let mut index = HandAddonIndex::default();
        index_arma_record(body_fk, &body_addon, &interner, &mut index, true);
        index_arma_record(gloves_fk, &gloves_addon, &interner, &mut index, true);

        let mut armo = make_record("ARMO", make_fk("787E5A", "SeventySix.esm", &interner));
        push_bod2_tokens(
            &mut armo,
            &[
                "33BODY", "34LHand", "35RHand", "36UTorso", "37ULArm", "38URArm", "39ULLeg",
                "40URLeg",
            ],
            &interner,
        );
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, body_fk);
        push_addon(&mut armo, gloves_fk);

        let mut strip_candidates = FxHashSet::default();
        let mut protected_candidates = FxHashSet::default();
        collect_mixed_hand_addon_strip_candidates(
            &armo,
            &index.hand_only_addons,
            &index.mixed_hand_addons,
            &mut strip_candidates,
            &mut protected_candidates,
            &interner,
        );

        assert!(strip_candidates.contains(&body_fk));
        assert!(!protected_candidates.contains(&body_fk));
        assert!(strip_hand_slots_from_record(&mut body_addon, &interner));
        assert_eq!(bod2_tokens(&body_addon, &interner), vec!["33BODY"]);
        assert_eq!(
            bod2_tokens(&gloves_addon, &interner),
            vec!["34LHand", "35RHand"]
        );
    }

    #[test]
    fn protects_mixed_body_addon_when_used_without_dedicated_gloves() {
        let interner = StringInterner::new();
        let body_fk = make_fk("787E52", "SeventySix.esm", &interner);
        let mut body_addon = make_record("ARMA", body_fk);
        push_bod2_tokens(
            &mut body_addon,
            &["33BODY", "34LHand", "35RHand"],
            &interner,
        );

        let mut index = HandAddonIndex::default();
        index_arma_record(body_fk, &body_addon, &interner, &mut index, true);

        let mut armo = make_record("ARMO", make_fk("787E5A", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, body_fk);

        let mut strip_candidates = FxHashSet::default();
        let mut protected_candidates = FxHashSet::default();
        collect_mixed_hand_addon_strip_candidates(
            &armo,
            &index.hand_only_addons,
            &index.mixed_hand_addons,
            &mut strip_candidates,
            &mut protected_candidates,
            &interner,
        );

        assert!(!strip_candidates.contains(&body_fk));
        assert!(protected_candidates.contains(&body_fk));
    }

    #[test]
    fn prunes_naked_hands_when_another_addon_owns_hand_slots() {
        let interner = StringInterner::new();
        let resident_addon = make_fk("0E5083", "Fallout4.esm", &interner);
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(resident_addon);
        hand_addons.insert(naked_hands);

        let mut armo = make_record("ARMO", make_fk("3B7D81", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, resident_addon);
        push_addon(&mut armo, naked_hands);

        assert!(prune_redundant_human_hand_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(addon_formkeys(&armo), vec![resident_addon]);
        assert_eq!(addon_index_count(&armo), 1);
    }

    #[test]
    fn adds_ghoul_hands_when_bare_human_hands_back_a_ghoul_body_addon() {
        let interner = StringInterner::new();
        let responder_jumpsuit = make_fk("3E5769", "SeventySix.esm", &interner);
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let naked_ghoul_hands = make_fk("0EAFBA", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(naked_hands);
        hand_addons.insert(naked_ghoul_hands);
        let mut ghoul_body_addons = FxHashSet::default();
        ghoul_body_addons.insert(responder_jumpsuit);

        let mut armo = make_record("ARMO", make_fk("3B7D64", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, responder_jumpsuit);
        push_addon(&mut armo, naked_hands);

        assert!(ensure_ghoul_hand_addon_for_ghoul_capable_armo(
            &mut armo,
            &ghoul_body_addons,
            Some(naked_ghoul_hands),
            &hand_addons,
            &interner
        ));
        assert_eq!(
            addon_formkeys(&armo),
            vec![responder_jumpsuit, naked_hands, naked_ghoul_hands]
        );
        assert_eq!(addon_index_count(&armo), 3);
        assert!(!prune_redundant_human_hand_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
    }

    #[test]
    fn skips_ghoul_hands_when_a_non_bare_hand_addon_is_present() {
        let interner = StringInterner::new();
        let responder_jumpsuit = make_fk("3E5769", "SeventySix.esm", &interner);
        let glove_addon = make_fk("58B272", "SeventySix.esm", &interner);
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let naked_ghoul_hands = make_fk("0EAFBA", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(glove_addon);
        hand_addons.insert(naked_hands);
        hand_addons.insert(naked_ghoul_hands);
        let mut ghoul_body_addons = FxHashSet::default();
        ghoul_body_addons.insert(responder_jumpsuit);

        let mut armo = make_record("ARMO", make_fk("3B7D64", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, responder_jumpsuit);
        push_addon(&mut armo, glove_addon);
        push_addon(&mut armo, naked_hands);

        assert!(!ensure_ghoul_hand_addon_for_ghoul_capable_armo(
            &mut armo,
            &ghoul_body_addons,
            Some(naked_ghoul_hands),
            &hand_addons,
            &interner
        ));
        assert_eq!(
            addon_formkeys(&armo),
            vec![responder_jumpsuit, glove_addon, naked_hands]
        );
    }

    #[test]
    fn keeps_naked_hands_when_it_is_the_only_hand_addon() {
        let interner = StringInterner::new();
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(naked_hands);

        let mut armo = make_record("ARMO", make_fk("000D64", "Fallout4.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, naked_hands);

        assert!(!prune_redundant_human_hand_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(addon_formkeys(&armo), vec![naked_hands]);
        assert_eq!(addon_index_count(&armo), 1);
    }
}
