use std::fs;
use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;

use esp_authoring_core::plugin_runtime::compiled_schema_for_game;

use crate::fixups::clean_leveled_item_entries::extract_entry_reference;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

const WORKSHOP_CATALOG_PATH: &str = "misc/workshop/workshop.json";
const RECIPE_FILTER_KEYWORD_TYPE: u32 = 9;
const SYNTHETIC_RECIPE_PLUGIN: &str = "B21_FO76WorkshopSynthetic";
const RECIPE_PRIORITY_STRIDE: u32 = 192;

const FO4_WORKSHOP_WORKBENCH_EXTERIOR: u32 = 0x05A0C8;
const FO4_WORKSHOP_WORKBENCH_FURNITURE: u32 = 0x05B5E3;
const FO4_WORKSHOP_WORKBENCH_DECORATIONS: u32 = 0x08280B;
const FO4_WORKSHOP_WORKBENCH_POWER: u32 = 0x05A0CA;
const FO4_WORKSHOP_WORKBENCH_CRAFTING: u32 = 0x12E2C8;
const FO4_WORKSHOP_WORKBENCH_SETTLEMENT: u32 = 0x246F85;

pub struct ApplyFo76WorkshopCatalogFixup;

#[derive(Deserialize)]
struct CatalogCategory {
    #[serde(rename = "CategoryKeyword")]
    category_keyword: CatalogForm,
    #[serde(rename = "SubCategories")]
    subcategories: Vec<CatalogSubcategory>,
}

#[derive(Deserialize)]
struct CatalogSubcategory {
    #[serde(rename = "CategoryKeyword")]
    category_keyword: CatalogForm,
    #[serde(rename = "Recipes")]
    recipes: Vec<CatalogForm>,
}

#[derive(Deserialize)]
struct CatalogForm {
    #[serde(rename = "FormEditorID")]
    editor_id: String,
    #[serde(rename = "FormID")]
    form_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecipePlacement {
    category_ids: Vec<u32>,
    workbench_id: u32,
    priority: u16,
}

#[derive(Default)]
struct WorkshopCatalog {
    category_ids: FxHashSet<u32>,
    recipes: FxHashMap<u32, RecipePlacement>,
}

impl Fixup for ApplyFo76WorkshopCatalogFixup {
    fn name(&self) -> &'static str {
        "apply_fo76_workshop_catalog"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        config.is_whole_plugin && source_game == Some("fo76") && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let Some(source_extracted_dir) = config.source_extracted_dir.as_deref() else {
            return missing_catalog_report(
                mapper,
                config,
                "source_extracted_dir is not configured",
            );
        };
        let catalog_path = source_extracted_dir.join(WORKSHOP_CATALOG_PATH);
        if !catalog_path.is_file() {
            return missing_catalog_report(
                mapper,
                config,
                &format!("{} does not exist", catalog_path.display()),
            );
        }

        let catalog = load_catalog(&catalog_path)?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let keyword_sig =
            SigCode::from_str("KYWD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let cobj_sig =
            SigCode::from_str("COBJ").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let compiled =
            compiled_schema_for_game("fo4").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let created_object_targets = compiled.allowed_targets("COBJ", "CNAM").ok_or_else(|| {
            FixupError::SchemaError("FO4 COBJ.CNAM target metadata missing".into())
        })?;

        let keyword_fks = session
            .form_keys_of_sig(keyword_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let category_fks: FxHashMap<u32, FormKey> = keyword_fks
            .iter()
            .copied()
            .filter(|fk| catalog.category_ids.contains(&fk.local))
            .map(|fk| (fk.local, fk))
            .collect();

        let mut changed_records = Vec::new();
        let mut decode_errors = 0u32;
        for fk in category_fks.values() {
            match session.record_decoded(fk, target_schema, mapper.interner) {
                Ok(mut record) => {
                    if normalize_category_keyword(&mut record) {
                        changed_records.push(record);
                    }
                }
                Err(_) => decode_errors += 1,
            }
        }

        let cobj_fks = session
            .form_keys_of_sig(cobj_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let cobj_fks: FxHashMap<u32, FormKey> = cobj_fks
            .into_iter()
            .filter(|fk| catalog.recipes.contains_key(&fk.local))
            .map(|fk| (fk.local, fk))
            .collect();
        let fallout4_plugin = mapper.interner.intern("Fallout4.esm");
        let output_plugin = mapper.output_plugin_sym();
        let target_master_names = session.target_masters().to_vec();
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let mut added_records = Vec::new();
        let mut expansion_cache = FxHashMap::default();
        let mut unsupported_created_types: FxHashMap<SigCode, u32> = FxHashMap::default();
        let mut expanded_recipes = 0u32;
        let mut hidden_recipes = 0u32;
        let mut recipe_ids: Vec<u32> = catalog.recipes.keys().copied().collect();
        recipe_ids.sort_unstable();

        for recipe_id in recipe_ids {
            let placement = &catalog.recipes[&recipe_id];
            let Some(fk) = cobj_fks.get(&recipe_id) else {
                continue;
            };
            let category_fks: Vec<FormKey> = placement
                .category_ids
                .iter()
                .filter_map(|category_id| category_fks.get(category_id).copied())
                .collect();
            if category_fks.is_empty() {
                continue;
            }
            match session.record_decoded(fk, target_schema, mapper.interner) {
                Ok(mut record) => {
                    let workbench = FormKey {
                        local: placement.workbench_id,
                        plugin: fallout4_plugin,
                    };
                    let Some(created_object) = record
                        .fields
                        .iter()
                        .find(|field| field.sig.0 == *b"CNAM")
                        .and_then(|field| first_form_key(&field.value))
                        .filter(|fk| fk.local != 0)
                    else {
                        continue;
                    };
                    let mut visiting = FxHashSet::default();
                    let resolution = resolve_created_object(
                        session,
                        created_object,
                        output_plugin,
                        &target_master_names,
                        &target_plugin_name,
                        target_schema,
                        mapper.interner,
                        &mut expansion_cache,
                        &mut visiting,
                        &|sig| created_object_targets.allows_target(sig),
                        &mut unsupported_created_types,
                        &mut decode_errors,
                    );
                    let (variants, expanded) = match resolution {
                        CreatedObjectResolution::Direct(fk) => (vec![fk], false),
                        CreatedObjectResolution::Variants(variants) => (variants, true),
                        CreatedObjectResolution::Unsupported => {
                            if disable_workshop_recipe(&mut record) {
                                changed_records.push(record);
                            }
                            hidden_recipes = hidden_recipes.saturating_add(1);
                            continue;
                        }
                    };
                    if variants.is_empty() {
                        if disable_workshop_recipe(&mut record) {
                            changed_records.push(record);
                        }
                        hidden_recipes = hidden_recipes.saturating_add(1);
                        continue;
                    }
                    if expanded {
                        expanded_recipes = expanded_recipes.saturating_add(1);
                    }

                    let base_record = record.clone();
                    for (variant_index, variant) in variants.into_iter().enumerate() {
                        let priority = expanded_recipe_priority(placement.priority, variant_index);
                        if variant_index == 0 {
                            let mut changed =
                                set_form_key_field(&mut record, SubrecordSig(*b"CNAM"), variant);
                            changed |= normalize_recipe(
                                &mut record,
                                workbench,
                                category_fks.clone(),
                                priority,
                            );
                            if changed {
                                changed_records.push(record.clone());
                            }
                            continue;
                        }

                        let mut child = base_record.clone();
                        let synthetic_source = synthetic_recipe_source_key(
                            recipe_id,
                            variant,
                            variant_index,
                            mapper.interner,
                        );
                        child.form_key =
                            mapper.allocate_or_resolve(synthetic_source, None, cobj_sig);
                        set_editor_id(
                            &mut child,
                            &format!(
                                "B21_FO76Workshop_{recipe_id:06X}_{:06X}_{variant_index:03}",
                                variant.local
                            ),
                            mapper.interner,
                        );
                        set_form_key_field(&mut child, SubrecordSig(*b"CNAM"), variant);
                        normalize_recipe(&mut child, workbench, category_fks.clone(), priority);
                        added_records.push(child);
                    }
                }
                Err(_) => decode_errors += 1,
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "apply_fo76_workshop_catalog replaced {replaced} of {expected} expected records"
            )));
        }
        let added = session
            .add_records(added_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut report = FixupReport::empty();
        report.records_changed = replaced as u32;
        report.records_added = added as u32;
        let missing_categories = catalog
            .category_ids
            .len()
            .saturating_sub(category_fks.len());
        let missing_recipes = catalog.recipes.len().saturating_sub(cobj_fks.len());
        if missing_categories > 0 || missing_recipes > 0 || decode_errors > 0 {
            report.warnings.push(mapper.interner.intern(&format!(
                "apply_fo76_workshop_catalog:missing_categories={missing_categories}:missing_recipes={missing_recipes}:decode_errors={decode_errors}"
            )));
        }
        if hidden_recipes > 0 || !unsupported_created_types.is_empty() {
            let mut unsupported: Vec<String> = unsupported_created_types
                .iter()
                .map(|(sig, count)| format!("{}={count}", sig.as_str()))
                .collect();
            unsupported.sort();
            report.warnings.push(mapper.interner.intern(&format!(
                "apply_fo76_workshop_catalog:expanded_recipes={expanded_recipes}:synthetic_recipes={added}:hidden_recipes={hidden_recipes}:unsupported_created_types={}",
                unsupported.join(",")
            )));
        }
        Ok(report)
    }
}

fn missing_catalog_report(
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    reason: &str,
) -> Result<FixupReport, FixupError> {
    let message = format!("apply_fo76_workshop_catalog:catalog_unavailable:{reason}");
    if config.strict {
        return Err(FixupError::Other(message));
    }
    let mut report = FixupReport::empty();
    report.warnings.push(mapper.interner.intern(&message));
    Ok(report)
}

fn load_catalog(path: &Path) -> Result<WorkshopCatalog, FixupError> {
    let bytes = fs::read(path).map_err(|e| {
        FixupError::Other(format!(
            "failed to read workshop catalog {}: {e}",
            path.display()
        ))
    })?;
    parse_catalog(&bytes)
}

fn parse_catalog(bytes: &[u8]) -> Result<WorkshopCatalog, FixupError> {
    let categories: Vec<CatalogCategory> = serde_json::from_slice(bytes)
        .map_err(|e| FixupError::Other(format!("invalid workshop catalog: {e}")))?;
    let mut catalog = WorkshopCatalog::default();

    for main in categories {
        let Some(workbench_id) = workbench_for_main_category(&main.category_keyword.editor_id)
        else {
            continue;
        };
        catalog.category_ids.insert(main.category_keyword.form_id);

        for subcategory in main.subcategories {
            let category_id = subcategory.category_keyword.form_id;
            catalog.category_ids.insert(category_id);
            for (index, recipe) in subcategory.recipes.into_iter().enumerate() {
                let priority = u16::try_from(index + 1).unwrap_or(u16::MAX);
                let placement =
                    catalog
                        .recipes
                        .entry(recipe.form_id)
                        .or_insert_with(|| RecipePlacement {
                            category_ids: Vec::new(),
                            workbench_id,
                            priority,
                        });
                if !placement.category_ids.contains(&category_id) {
                    placement.category_ids.push(category_id);
                }
                placement.priority = placement.priority.min(priority);
            }
        }
    }

    Ok(catalog)
}

fn workbench_for_main_category(editor_id: &str) -> Option<u32> {
    match editor_id {
        "Workshop2_MainCategory_Quest"
        | "Workshop2_MainCategory_CAMP"
        | "Workshop2_MainCategory_Defense"
        | "Workshop2_MainCategory_Resources"
        | "Workshop2_MainCategory_Structure"
        | "Workshop2_MainCategory_Wallpapers" => Some(FO4_WORKSHOP_WORKBENCH_EXTERIOR),
        "Workshop2_MainCategory_Power" | "Workshop2_MainCategory_Lights" => {
            Some(FO4_WORKSHOP_WORKBENCH_POWER)
        }
        "Workshop2_MainCategory_Utility" => Some(FO4_WORKSHOP_WORKBENCH_CRAFTING),
        "Workshop2_MainCategory_Furniture" | "Workshop2_MainCategory_Storage" => {
            Some(FO4_WORKSHOP_WORKBENCH_FURNITURE)
        }
        "Workshop2_MainCategory_Decorations" | "Workshop2_MainCategory_WallDecor" => {
            Some(FO4_WORKSHOP_WORKBENCH_DECORATIONS)
        }
        "Workshop2_MainCategory_Dwellers" => Some(FO4_WORKSHOP_WORKBENCH_SETTLEMENT),
        _ => None,
    }
}

fn normalize_category_keyword(record: &mut Record) -> bool {
    set_scalar_field(record, SubrecordSig(*b"TNAM"), RECIPE_FILTER_KEYWORD_TYPE)
}

enum CreatedObjectResolution {
    Direct(FormKey),
    Variants(Vec<FormKey>),
    Unsupported,
}

#[allow(clippy::too_many_arguments)]
fn resolve_created_object(
    session: &mut PluginSession<'_>,
    created_object: FormKey,
    output_plugin: crate::sym::Sym,
    target_master_names: &[String],
    target_plugin_name: &str,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &crate::sym::StringInterner,
    expansion_cache: &mut FxHashMap<FormKey, Vec<FormKey>>,
    visiting: &mut FxHashSet<FormKey>,
    allows_target: &dyn Fn(&str) -> bool,
    unsupported_created_types: &mut FxHashMap<SigCode, u32>,
    decode_errors: &mut u32,
) -> CreatedObjectResolution {
    if created_object.plugin != output_plugin {
        return CreatedObjectResolution::Direct(created_object);
    }
    if let Some(variants) = expansion_cache.get(&created_object) {
        return CreatedObjectResolution::Variants(variants.clone());
    }
    if !visiting.insert(created_object) {
        *decode_errors = decode_errors.saturating_add(1);
        return CreatedObjectResolution::Unsupported;
    }

    let record = match session.record_decoded(&created_object, target_schema, interner) {
        Ok(record) => record,
        Err(_) => {
            visiting.remove(&created_object);
            *decode_errors = decode_errors.saturating_add(1);
            return CreatedObjectResolution::Unsupported;
        }
    };
    if !is_workshop_group_signature(record.sig) {
        visiting.remove(&created_object);
        if allows_target(record.sig.as_str()) {
            return CreatedObjectResolution::Direct(created_object);
        }
        *unsupported_created_types.entry(record.sig).or_default() += 1;
        return CreatedObjectResolution::Unsupported;
    }

    let mut variants = Vec::new();
    for member in group_member_form_keys(&record, target_master_names, target_plugin_name, interner)
    {
        match resolve_created_object(
            session,
            member,
            output_plugin,
            target_master_names,
            target_plugin_name,
            target_schema,
            interner,
            expansion_cache,
            visiting,
            allows_target,
            unsupported_created_types,
            decode_errors,
        ) {
            CreatedObjectResolution::Direct(member) => variants.push(member),
            CreatedObjectResolution::Variants(members) => variants.extend(members),
            CreatedObjectResolution::Unsupported => {}
        }
    }
    visiting.remove(&created_object);
    let mut seen = FxHashSet::default();
    variants.retain(|member| seen.insert(*member));
    expansion_cache.insert(created_object, variants.clone());
    CreatedObjectResolution::Variants(variants)
}

fn is_workshop_group_signature(sig: SigCode) -> bool {
    matches!(
        sig.0,
        [b'L', b'V', b'L', b'I'] | [b'L', b'V', b'L', b'N'] | [b'F', b'L', b'S', b'T']
    )
}

fn group_member_form_keys(
    record: &Record,
    target_master_names: &[String],
    target_plugin_name: &str,
    interner: &crate::sym::StringInterner,
) -> Vec<FormKey> {
    let member_sig = if record.sig.0 == *b"FLST" {
        *b"LNAM"
    } else {
        *b"LVLO"
    };
    let mut members = Vec::new();
    for field in &record.fields {
        if field.sig.0 == member_sig {
            if record.sig.0 == *b"FLST" {
                collect_form_keys(&field.value, &mut members);
            } else if let Some(member) = extract_entry_reference(
                &field.value,
                interner.intern("Reference"),
                target_master_names,
                target_plugin_name,
                interner,
            )
            .or_else(|| first_form_key(&field.value))
            {
                members.push(member);
            }
        }
    }
    members.retain(|member| member.local != 0);
    members
}

fn collect_form_keys(value: &FieldValue, output: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(fk) => output.push(*fk),
        FieldValue::List(values) => {
            for value in values {
                collect_form_keys(value, output);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_form_keys(value, output);
            }
        }
        _ => {}
    }
}

fn disable_workshop_recipe(record: &mut Record) -> bool {
    let previous_len = record.fields.len();
    record.fields.retain(|field| {
        !matches!(
            field.sig.0,
            [b'B', b'N', b'A', b'M'] | [b'F', b'N', b'A', b'M']
        )
    });
    record.fields.len() != previous_len
}

fn expanded_recipe_priority(priority: u16, variant_index: usize) -> u16 {
    let parent_offset = u32::from(priority.saturating_sub(1)) * RECIPE_PRIORITY_STRIDE;
    parent_offset
        .saturating_add(u32::try_from(variant_index).unwrap_or(u32::MAX))
        .saturating_add(1)
        .min(u32::from(u16::MAX)) as u16
}

fn synthetic_recipe_source_key(
    recipe_id: u32,
    variant: FormKey,
    variant_index: usize,
    interner: &crate::sym::StringInterner,
) -> FormKey {
    FormKey {
        local: 1,
        plugin: interner.intern(&format!(
            "{SYNTHETIC_RECIPE_PLUGIN}_{recipe_id:06X}_{:06X}_{variant_index:03}",
            variant.local
        )),
    }
}

fn set_editor_id(record: &mut Record, editor_id: &str, interner: &crate::sym::StringInterner) {
    let editor_id = interner.intern(editor_id);
    record.eid = Some(editor_id);
    if let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"EDID")
    {
        field.value = FieldValue::String(editor_id);
        return;
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(editor_id),
        },
    );
}

fn normalize_recipe(
    record: &mut Record,
    workbench: FormKey,
    categories: Vec<FormKey>,
    priority: u16,
) -> bool {
    if !record.fields.iter().any(|entry| {
        entry.sig.0 == *b"CNAM" && first_form_key(&entry.value).is_some_and(|fk| fk.local != 0)
    }) {
        return false;
    }

    let mut changed = set_form_key_field(record, SubrecordSig(*b"BNAM"), workbench);
    changed |= set_form_key_list_field(record, SubrecordSig(*b"FNAM"), categories);
    changed |= set_recipe_priority(record, priority);
    changed
}

fn set_scalar_field(record: &mut Record, sig: SubrecordSig, value: u32) -> bool {
    if let Some(field) = record.fields.iter_mut().find(|field| field.sig == sig) {
        return set_unsigned_value(&mut field.value, value);
    }
    record.fields.push(FieldEntry {
        sig,
        value: FieldValue::Uint(u64::from(value)),
    });
    true
}

fn set_form_key_field(record: &mut Record, sig: SubrecordSig, value: FormKey) -> bool {
    if let Some(field) = record.fields.iter_mut().find(|field| field.sig == sig) {
        if field.value == FieldValue::FormKey(value) {
            return false;
        }
        field.value = FieldValue::FormKey(value);
        return true;
    }
    let insert_at = record
        .fields
        .iter()
        .rposition(|field| field.sig.0 == *b"CNAM")
        .map_or(record.fields.len(), |index| index + 1);
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig,
            value: FieldValue::FormKey(value),
        },
    );
    true
}

fn set_form_key_list_field(record: &mut Record, sig: SubrecordSig, values: Vec<FormKey>) -> bool {
    let value = FieldValue::List(values.into_iter().map(FieldValue::FormKey).collect());
    if let Some(field) = record.fields.iter_mut().find(|field| field.sig == sig) {
        if field.value == value {
            return false;
        }
        field.value = value;
        return true;
    }
    let insert_at = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"BNAM")
        .map_or(record.fields.len(), |index| index + 1);
    record.fields.insert(insert_at, FieldEntry { sig, value });
    true
}

fn set_recipe_priority(record: &mut Record, priority: u16) -> bool {
    let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"INTV")
    else {
        return false;
    };
    match &mut field.value {
        FieldValue::Struct(fields) if fields.len() >= 2 => {
            set_unsigned_value(&mut fields[1].1, u32::from(priority))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let current = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
            if current == priority {
                false
            } else {
                bytes[2..4].copy_from_slice(&priority.to_le_bytes());
                true
            }
        }
        _ => false,
    }
}

fn set_unsigned_value(value: &mut FieldValue, replacement: u32) -> bool {
    match value {
        FieldValue::Uint(current) => {
            let replacement = u64::from(replacement);
            let changed = *current != replacement;
            *current = replacement;
            changed
        }
        FieldValue::Int(current) => {
            let replacement = i64::from(replacement);
            let changed = *current != replacement;
            *current = replacement;
            changed
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let current = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            bytes[..4].copy_from_slice(&replacement.to_le_bytes());
            current != replacement
        }
        other => {
            *other = FieldValue::Uint(u64::from(replacement));
            true
        }
    }
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::List(values) => values.iter().find_map(first_form_key),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FIRST_ALLOCATION_ID, MapperOptions};
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;
    use smallvec::{SmallVec, smallvec};

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    #[test]
    fn parses_exact_membership_order_and_omits_non_build_categories() {
        let json = br#"[
          {"CategoryKeyword":{"FormEditorID":"Workshop2_MainCategory_CAMP","FormID":8530398},"SubCategories":[
            {"CategoryKeyword":{"FormEditorID":"Workshop2_SubCategory_Building_Fences","FormID":8530421},"Recipes":[
              {"FormEditorID":"workshop_co_A","FormID":100},{"FormEditorID":"workshop_co_B","FormID":101}
            ]},
            {"CategoryKeyword":{"FormEditorID":"Workshop2_SubCategory_Building_Walls","FormID":8530413},"Recipes":[
              {"FormEditorID":"workshop_co_A","FormID":100}
            ]}
          ]},
          {"CategoryKeyword":{"FormEditorID":"Workshop2_MainCategory_Testing","FormID":8647521},"SubCategories":[
            {"CategoryKeyword":{"FormEditorID":"Workshop2_SubCategory_Testing_Nothing","FormID":8647522},"Recipes":[
              {"FormEditorID":"workshop_co_Test","FormID":102}
            ]}
          ]},
          {"CategoryKeyword":{"FormEditorID":"Workshop2_MainCategory_Wallpapers","FormID":8653989},"SubCategories":[
            {"CategoryKeyword":{"FormEditorID":"Workshop2_SubCategory_Wallpapers","FormID":8676101},"Recipes":[
              {"FormEditorID":"ATX_workshop_co_Wallpapers","FormID":103}
            ]}
          ]}
        ]"#;

        let catalog = parse_catalog(json).unwrap();
        assert_eq!(catalog.recipes.len(), 3);
        assert_eq!(
            catalog.recipes[&100],
            RecipePlacement {
                category_ids: vec![8_530_421, 8_530_413],
                workbench_id: FO4_WORKSHOP_WORKBENCH_EXTERIOR,
                priority: 1,
            }
        );
        assert_eq!(catalog.recipes[&101].priority, 2);
        assert!(!catalog.recipes.contains_key(&102));
        assert!(!catalog.category_ids.contains(&8_647_521));
        assert_eq!(
            catalog.recipes[&103].workbench_id,
            FO4_WORKSHOP_WORKBENCH_EXTERIOR
        );
    }

    #[test]
    fn replaces_heuristic_recipe_fields_with_catalog_values() {
        let interner = StringInterner::new();
        let count = interner.intern("created_object_count");
        let priority = interner.intern("priority");
        let mut record = Record {
            sig: SigCode::from_str("COBJ").unwrap(),
            form_key: fk(100, "SeventySix.esm", &interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                field(
                    "CNAM",
                    FieldValue::FormKey(fk(200, "SeventySix.esm", &interner))
                ),
                field(
                    "BNAM",
                    FieldValue::FormKey(fk(0x05A0C8, "Fallout4.esm", &interner))
                ),
                field(
                    "FNAM",
                    FieldValue::List(vec![FieldValue::FormKey(fk(
                        0x1573F0,
                        "SeventySix.esm",
                        &interner
                    ))])
                ),
                field(
                    "INTV",
                    FieldValue::Struct(vec![
                        (count, FieldValue::Uint(1)),
                        (priority, FieldValue::Uint(99))
                    ])
                ),
            ],
            warnings: SmallVec::new(),
        };
        let exact_categories = vec![
            fk(0x8229F5, "SeventySix.esm", &interner),
            fk(0x8229ED, "SeventySix.esm", &interner),
        ];

        assert!(normalize_recipe(
            &mut record,
            fk(FO4_WORKSHOP_WORKBENCH_POWER, "Fallout4.esm", &interner),
            exact_categories.clone(),
            7,
        ));
        assert_eq!(
            record
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"FNAM")
                .unwrap()
                .value,
            FieldValue::List(
                exact_categories
                    .into_iter()
                    .map(FieldValue::FormKey)
                    .collect()
            )
        );
        let FieldValue::Struct(data) = &record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"INTV")
            .unwrap()
            .value
        else {
            panic!("INTV was not a struct");
        };
        assert_eq!(data[1].1, FieldValue::Uint(7));
    }

    #[test]
    fn converts_modern_workshop_keyword_to_recipe_filter() {
        let interner = StringInterner::new();
        let mut record = Record {
            sig: SigCode::from_str("KYWD").unwrap(),
            form_key: fk(0x8229F5, "SeventySix.esm", &interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![field("TNAM", FieldValue::Uint(0))],
            warnings: SmallVec::new(),
        };
        assert!(normalize_category_keyword(&mut record));
        assert_eq!(record.fields[0].value, FieldValue::Uint(9));
        assert!(!normalize_category_keyword(&mut record));
    }

    #[test]
    fn reads_every_variant_from_leveled_and_form_lists() {
        let interner = StringInterner::new();
        let item = interner.intern("item");
        let mut leveled = Record::new(
            SigCode::from_str("LVLI").unwrap(),
            fk(0x100, "SeventySix.esm", &interner),
        );
        leveled.fields = smallvec![
            field(
                "LVLO",
                FieldValue::Struct(vec![(
                    item,
                    FieldValue::FormKey(fk(0x200, "SeventySix.esm", &interner)),
                )]),
            ),
            field(
                "LVLO",
                FieldValue::Struct(vec![(
                    item,
                    FieldValue::FormKey(fk(0x201, "SeventySix.esm", &interner)),
                )]),
            ),
        ];
        assert_eq!(
            group_member_form_keys(&leveled, &[], "SeventySix.esm", &interner),
            vec![
                fk(0x200, "SeventySix.esm", &interner),
                fk(0x201, "SeventySix.esm", &interner),
            ]
        );

        let mut form_list = Record::new(
            SigCode::from_str("FLST").unwrap(),
            fk(0x300, "SeventySix.esm", &interner),
        );
        form_list.fields = smallvec![
            field(
                "LNAM",
                FieldValue::FormKey(fk(0x400, "SeventySix.esm", &interner)),
            ),
            field(
                "LNAM",
                FieldValue::FormKey(fk(0x401, "SeventySix.esm", &interner)),
            ),
        ];
        assert_eq!(
            group_member_form_keys(&form_list, &[], "SeventySix.esm", &interner),
            vec![
                fk(0x400, "SeventySix.esm", &interner),
                fk(0x401, "SeventySix.esm", &interner),
            ]
        );
    }

    #[test]
    fn reads_raw_lvlo_references_before_leveled_list_cleanup() {
        let interner = StringInterner::new();
        let masters = vec!["Fallout4.esm".to_string()];
        let mut leveled = Record::new(
            SigCode::from_str("LVLI").unwrap(),
            fk(0x100, "SeventySix.esm", &interner),
        );
        let raw_entry = |raw: u32| {
            let mut bytes = vec![0u8; 12];
            bytes[4..8].copy_from_slice(&raw.to_le_bytes());
            field("LVLO", FieldValue::Bytes(bytes.into()))
        };
        leveled.fields = smallvec![raw_entry(0x00_001234), raw_entry(0x01_7B2872), raw_entry(0),];

        assert_eq!(
            group_member_form_keys(&leveled, &masters, "SeventySix.esm", &interner),
            vec![
                fk(0x001234, "Fallout4.esm", &interner),
                fk(0x7B2872, "SeventySix.esm", &interner),
            ]
        );
    }

    #[test]
    fn hides_unsupported_recipe_without_removing_its_created_object() {
        let interner = StringInterner::new();
        let created = fk(0x200, "SeventySix.esm", &interner);
        let mut recipe = Record::new(
            SigCode::from_str("COBJ").unwrap(),
            fk(0x100, "SeventySix.esm", &interner),
        );
        recipe.fields = smallvec![
            field("CNAM", FieldValue::FormKey(created)),
            field(
                "BNAM",
                FieldValue::FormKey(fk(FO4_WORKSHOP_WORKBENCH_POWER, "Fallout4.esm", &interner)),
            ),
            field(
                "FNAM",
                FieldValue::List(vec![FieldValue::FormKey(fk(
                    0x8229FA,
                    "SeventySix.esm",
                    &interner,
                ))]),
            ),
        ];

        assert!(disable_workshop_recipe(&mut recipe));
        assert_eq!(first_form_key(&recipe.fields[0].value), Some(created),);
        assert!(!recipe.fields.iter().any(|field| matches!(
            field.sig.0,
            [b'B', b'N', b'A', b'M'] | [b'F', b'N', b'A', b'M']
        )));
    }

    #[test]
    fn variant_priorities_stay_grouped_without_overlapping_next_parent() {
        assert_eq!(expanded_recipe_priority(1, 0), 1);
        assert_eq!(expanded_recipe_priority(1, 15), 16);
        assert_eq!(expanded_recipe_priority(2, 0), 193);
    }

    #[test]
    fn synthetic_recipe_sources_use_the_shared_fresh_allocator() {
        let mut interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let variant = fk(0x111E01, "SeventySix.esm", &interner);
        let first = synthetic_recipe_source_key(0x4F6D69, variant, 1, &interner);
        let second = synthetic_recipe_source_key(0x4F6D69, variant, 2, &interner);

        assert_ne!(first, second);
        assert!(first.local < FIRST_ALLOCATION_ID);
        assert_eq!(
            first,
            synthetic_recipe_source_key(0x4F6D69, variant, 1, &interner),
        );

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                preserve_source_ids: true,
                generated_object_id_floor: 0x00A0_0000,
                ..MapperOptions::default()
            },
            &mut interner,
        );
        let cobj_sig = SigCode::from_str("COBJ").unwrap();
        assert_eq!(
            mapper.allocate_or_resolve(first, None, cobj_sig),
            FormKey {
                local: 0x00A0_0000,
                plugin: output,
            },
        );
        assert_eq!(
            mapper.allocate_or_resolve(second, None, cobj_sig),
            FormKey {
                local: 0x00A0_0001,
                plugin: output,
            },
        );
    }
}
