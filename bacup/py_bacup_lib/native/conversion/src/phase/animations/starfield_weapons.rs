use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use rustc_hash::FxHashMap;
use serde::Serialize;

use super::{starfield_afx, starfield_ammo, starfield_subgraphs};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::HandleRawScan;
use crate::sym::StringInterner;
use crate::translator::Game;
use crate::translator::pair_hooks::starfield_fo4::weapon::{self, omod};

pub struct PlanStarfieldWeaponsPhase;

#[derive(Debug, Clone)]
pub(crate) struct SourceWeaponRecordContract {
    pub source_raw: u32,
    pub stats: Option<weapon::StarfieldWeaponComponents>,
    pub templates: Vec<omod::SourceObjectTemplate>,
    pub source_errors: Vec<String>,
}

#[derive(Debug, Default)]
pub(crate) struct SourceWeaponContracts {
    weapons: FxHashMap<FormKey, SourceWeaponRecordContract>,
    omods: FxHashMap<u32, omod::SourceWeaponOmod>,
    omod_raw_by_key: FxHashMap<FormKey, u32>,
    plans: Vec<WeaponPlan>,
    ballistics: FxHashMap<FormKey, SourceBallisticRecordContract>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SourceBallisticRecordContract {
    pub source_raw: u32,
    pub signature: String,
    pub fields: Vec<starfield_ammo::SourceBallisticField>,
    pub ammo: Option<starfield_ammo::SourceAmmoContract>,
    pub projectile: Option<starfield_ammo::SourceProjectileContract>,
    pub source_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SourceWeaponInstanceMetadata {
    source_key: FormKey,
    combinations: Vec<Vec<FieldEntry>>,
}

impl SourceWeaponContracts {
    pub(crate) fn ballistic(&self, source_key: FormKey) -> Option<&SourceBallisticRecordContract> {
        self.ballistics.get(&source_key)
    }

    pub(crate) fn owns_ballistic(&self, source_key: FormKey) -> bool {
        self.ballistics.contains_key(&source_key)
    }

    fn required_ammo(
        &self,
        source_key: FormKey,
    ) -> Result<&starfield_ammo::SourceAmmoContract, String> {
        let source = self
            .ballistic(source_key)
            .ok_or("Starfield AMMO source contract was not retained")?;
        source.ammo.as_ref().ok_or_else(|| {
            format!(
                "Starfield AMMO source contract is incomplete: {}",
                source.source_errors.join("; ")
            )
        })
    }
    pub(crate) fn weapon(&self, source_key: FormKey) -> Option<&SourceWeaponRecordContract> {
        self.weapons.get(&source_key)
    }

    pub(crate) fn omod(&self, source_key: FormKey) -> Option<&omod::SourceWeaponOmod> {
        self.omod_raw_by_key
            .get(&source_key)
            .and_then(|raw| self.omods.get(raw))
    }

    pub(crate) fn omods_by_raw(&self) -> &FxHashMap<u32, omod::SourceWeaponOmod> {
        &self.omods
    }

    pub(crate) fn owns_omod(&self, source_key: FormKey) -> bool {
        self.omod_raw_by_key.contains_key(&source_key)
    }

    fn required_weapon(&self, source_key: FormKey) -> Result<&SourceWeaponRecordContract, String> {
        let source = self
            .weapon(source_key)
            .ok_or("Starfield weapon source contract was not retained")?;
        if source.stats.is_none() || !source.source_errors.is_empty() {
            return Err(format!(
                "Starfield weapon source contract is incomplete: {}",
                source.source_errors.join("; ")
            ));
        }
        Ok(source)
    }

    pub(crate) fn capture_instance_metadata(
        &self,
        source_key: FormKey,
        record: &Record,
    ) -> Result<Option<SourceWeaponInstanceMetadata>, String> {
        if record.sig.0 != *b"WEAP" {
            return Ok(None);
        }
        let source = self.required_weapon(source_key)?;
        let Some(range) = template_group_range(record, source.templates.len())? else {
            return Ok(None);
        };
        let mut combinations = Vec::new();
        let mut metadata = Vec::new();
        for field in &record.fields[range.start + 1..range.end] {
            match &field.sig.0 {
                b"OBTF" | b"FULL" => {
                    if metadata.iter().any(|old: &FieldEntry| old.sig == field.sig) {
                        return Err("Starfield template has duplicate metadata".into());
                    }
                    if field.sig.0 == *b"FULL"
                        && !matches!(field.value, FieldValue::String(_) | FieldValue::None)
                    {
                        return Err(
                            "Starfield template FULL must be a resolved source string".into()
                        );
                    }
                    metadata.push(field.clone());
                }
                b"OBTS" => combinations.push(std::mem::take(&mut metadata)),
                b"STOP" => {}
                _ => unreachable!(),
            }
        }
        Ok(Some(SourceWeaponInstanceMetadata {
            source_key,
            combinations,
        }))
    }

    pub(crate) fn strip_instance_fields(
        &self,
        source_key: FormKey,
        record: &mut Record,
    ) -> Result<(), String> {
        match &record.sig.0 {
            b"WEAP" => {
                let source = self.required_weapon(source_key)?;
                if let Some(range) = template_group_range(record, source.templates.len())? {
                    record.fields.drain(range);
                }
            }
            b"OMOD" if self.owns_omod(source_key) => {
                record.fields.retain(|field| field.sig.0 != *b"DATA")
            }
            b"AMMO" => {
                self.required_ammo(source_key)?;
                let mut component = false;
                record.fields.retain(|field| {
                    match &field.sig.0 {
                        b"BFCB" => component = true,
                        b"BFCE" => component = false,
                        _ => {}
                    }
                    component || !matches!(&field.sig.0, b"DATA" | b"DNAM" | b"NAM1")
                });
            }
            b"PROJ" => return Err(
                "Starfield projectile particle/audio and physical-unit lowering is not implemented"
                    .into(),
            ),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn restore_instance_fields(
        &self,
        source_key: FormKey,
        record: &mut Record,
        mapper: &mut FormKeyMapper<'_>,
        metadata: Option<&SourceWeaponInstanceMetadata>,
    ) -> Result<(), String> {
        match &record.sig.0 {
            b"WEAP" => {
                let source = self.required_weapon(source_key)?;
                if record
                    .fields
                    .iter()
                    .any(|field| matches!(&field.sig.0, b"OBTE" | b"OBTS"))
                {
                    return Err("Starfield source instance fields were not stripped before target restoration".into());
                }
                let Some(metadata) = metadata else {
                    return if source.templates.is_empty() {
                        Ok(())
                    } else {
                        Err(
                            "Starfield template metadata was not captured before translation"
                                .into(),
                        )
                    };
                };
                if metadata.source_key != source_key
                    || metadata.combinations.len() != source.templates.len()
                {
                    return Err(
                        "Starfield template metadata belongs to a different source contract".into(),
                    );
                }
                let mut replacement = vec![raw_field(
                    *b"OBTE",
                    &(source.templates.len() as u32).to_le_bytes(),
                )];
                for (template, names) in source.templates.iter().zip(&metadata.combinations) {
                    replacement.extend(names.iter().cloned());
                    replacement.push(raw_field(
                        *b"OBTS",
                        &omod::lower_source_object_template(template, mapper)?,
                    ));
                }
                // SF uses OBTS as the row terminator; FO4 additionally requires the group STOP.
                replacement.push(raw_field(*b"STOP", &[]));
                record.fields.extend(replacement);
            }
            b"OMOD" => {
                if let Some(source) = self.omod(source_key) {
                    let bytes = omod::lower_source_omod(source, &self.omods, mapper)?;
                    record.fields.retain(|field| field.sig.0 != *b"DATA");
                    record.fields.push(raw_field(*b"DATA", &bytes));
                }
            }
            b"AMMO" => {
                let source = self.required_ammo(source_key)?;
                let (data, dnam) = starfield_ammo::lower_ammo(source, |raw| {
                    let source_key = self.ballistics.iter()
                        .find(|(_, contract)| contract.signature == "PROJ" && contract.source_raw == raw)
                        .map(|(key, _)| *key)
                        .ok_or_else(|| format!("Starfield AMMO projectile {raw:08X} has no retained source PROJ identity"))?;
                    let target = mapper.lookup(source_key).ok_or_else(|| {
                        format!(
                            "Starfield AMMO projectile {raw:08X} has no explicit target mapping"
                        )
                    })?;
                    mapper.raw_formid_for_target(target).ok_or_else(|| {
                        format!("Starfield AMMO projectile {raw:08X} target has no master index")
                    })
                })?;
                if record
                    .fields
                    .iter()
                    .any(|field| matches!(&field.sig.0, b"DATA" | b"DNAM" | b"NAM1"))
                {
                    return Err(
                        "Starfield AMMO source payload was not stripped before target restoration"
                            .into(),
                    );
                }
                let mut replacement = vec![raw_field(*b"DATA", &data), raw_field(*b"DNAM", &dnam)];
                if let Some(path) = &source.casing_model {
                    replacement.push(FieldEntry {
                        sig: SubrecordSig(*b"NAM1"),
                        value: FieldValue::String(mapper.interner.intern(path)),
                    });
                }
                record.fields.extend(replacement);
            }
            b"PROJ" => return Err(
                "Starfield projectile particle/audio and physical-unit lowering is not implemented"
                    .into(),
            ),
            _ => {}
        }
        Ok(())
    }
}

fn raw_field(sig: [u8; 4], bytes: &[u8]) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value: FieldValue::Bytes(bytes.into()),
    }
}

fn template_group_range(
    record: &Record,
    count: usize,
) -> Result<Option<std::ops::Range<usize>>, String> {
    let starts: Vec<_> = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.0 == *b"OBTE")
        .map(|(index, _)| index)
        .collect();
    let Some(&start) = starts.first() else {
        return if count == 0 && record.fields.iter().all(|field| field.sig.0 != *b"OBTS") {
            Ok(None)
        } else {
            Err("Starfield source template group is missing".into())
        };
    };
    let declared = match &record.fields[start].value {
        FieldValue::Uint(value) => usize::try_from(*value).ok(),
        FieldValue::Int(value) => usize::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(u32::from_le_bytes(bytes[..].try_into().unwrap()) as usize)
        }
        _ => None,
    };
    if starts.len() != 1 || declared != Some(count) {
        return Err(
            "Starfield source template group count disagrees with retained contract".into(),
        );
    }
    let mut end = start + 1;
    for _ in 0..count {
        while let Some(field) = record.fields.get(end) {
            end += 1;
            match &field.sig.0 {
                b"OBTS" => break,
                b"OBTF" | b"FULL" => continue,
                _ => return Err("Starfield source template combination is unterminated".into()),
            }
        }
        if record
            .fields
            .get(end - 1)
            .is_none_or(|field| field.sig.0 != *b"OBTS")
        {
            return Err("Starfield source template combination is truncated".into());
        }
    }
    if record
        .fields
        .get(end)
        .is_some_and(|field| field.sig.0 == *b"STOP")
    {
        end += 1;
    }
    if record.fields[end..]
        .iter()
        .any(|field| field.sig.0 == *b"OBTS")
        || record.fields[..start]
            .iter()
            .any(|field| field.sig.0 == *b"OBTS")
    {
        return Err("Starfield source template has an OBTS outside its group".into());
    }
    Ok(Some(start..end))
}

pub(crate) fn collect_source_weapon_contracts(
    scan: &HandleRawScan<'_>,
    interner: &StringInterner,
    source_root: &Path,
) -> SourceWeaponContracts {
    let mut result = SourceWeaponContracts::default();
    for signature in [*b"AMMO", *b"PROJ"] {
        for (key, raw) in scan.record_identities_of_sig(SigCode(signature)) {
            let key = FormKey {
                local: key.object_id,
                plugin: interner.intern(&key.plugin),
            };
            let mut contract = SourceBallisticRecordContract {
                source_raw: raw,
                signature: String::from_utf8_lossy(&signature).into(),
                fields: vec![],
                ammo: None,
                projectile: None,
                source_errors: vec![],
            };
            if let Some(fields) = scan.with_record_subrecords(raw, |fields| {
                fields
                    .iter()
                    .map(|field| starfield_ammo::SourceBallisticField {
                        signature: field.signature.to_string(),
                        bytes: field.data.to_vec(),
                    })
                    .collect::<Vec<_>>()
            }) {
                contract.fields = fields;
                let decoded = if signature == *b"AMMO" {
                    starfield_ammo::decode_ammo(&contract.fields)
                        .map(|value| contract.ammo = Some(value))
                } else {
                    starfield_ammo::decode_projectile(&contract.fields)
                        .map(|value| contract.projectile = Some(value))
                };
                if let Err(error) = decoded {
                    contract.source_errors.push(error);
                }
            } else {
                contract
                    .source_errors
                    .push("source ballistic record is absent".into());
            }
            result.ballistics.insert(key, contract);
        }
    }
    for (key, raw) in scan.record_identities_of_sig(SigCode(*b"WEAP")) {
        let key = FormKey {
            local: key.object_id,
            plugin: interner.intern(&key.plugin),
        };
        let plan = plan_weapon(scan, raw, key, interner, source_root);
        result.weapons.insert(
            key,
            SourceWeaponRecordContract {
                source_raw: raw,
                stats: plan.stats.clone(),
                templates: plan.templates.clone(),
                source_errors: plan.source_errors.clone(),
            },
        );
        for item in &plan.omods {
            result
                .omods
                .entry(item.source_raw)
                .or_insert_with(|| item.source.clone());
        }
        result.plans.push(plan);
    }
    for (key, raw) in scan.record_identities_of_sig(SigCode(*b"OMOD")) {
        if result.omods.contains_key(&raw) {
            result.omod_raw_by_key.insert(
                FormKey {
                    local: key.object_id,
                    plugin: interner.intern(&key.plugin),
                },
                raw,
            );
        }
    }
    let subgraphs = mesh_path(
        source_root,
        "AnimTextData/GeneratedSubgraphData/HumanRace.txt",
    )
    .ok_or_else(|| "source HumanRace generated subgraph table is absent".to_owned())
    .and_then(|path| std::fs::read_to_string(path).map_err(|error| error.to_string()))
    .and_then(|text| starfield_subgraphs::parse_generated_subgraphs(&text));
    let editor_ids = scan.own_editor_ids();
    for plan in &mut result.plans {
        let selection = subgraphs.as_ref().map_err(Clone::clone).and_then(|rows| {
            plan_actor_profiles(scan, plan, &result.omods, &editor_ids, rows, source_root)
        });
        match selection {
            Ok(profiles) => plan.source_actor_profiles = profiles,
            Err(error) => plan.actor_selection_errors.push(error),
        }
    }
    result
}

impl Phase for PlanStarfieldWeaponsPhase {
    fn name(&self) -> &'static str {
        "plan_starfield_weapons"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Starfield || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        let mut session =
            crate::session::open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
                .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let scan = session
            .handle_raw_scan(ctx.run.source_handle_id)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        ctx.check_cancel()?;
        let contracts =
            collect_source_weapon_contracts(&scan, &ctx.run.interner, ctx.source_extracted_dir);
        ctx.check_cancel()?;
        let failed = contracts
            .plans
            .iter()
            .filter(|plan| plan.source_errors.len() > 0)
            .count();
        let path = ctx.mod_path.join(".starfield_weapon_contracts.json");
        std::fs::create_dir_all(ctx.mod_path)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let manifest = serde_json::json!({
            "schema_version": 1, "phase": self.name(), "source_weapon_count": contracts.plans.len(),
            "source_contract_failures": failed, "ready_weapon_count": 0,
            "functionality_certified": false, "weapons": &contracts.plans,
            "ballistics": contracts.ballistics.iter().map(|(key, source)| serde_json::json!({
                "source_form_key": key.format(&ctx.run.interner), "source_raw": source.source_raw,
                "signature": source.signature, "ammo": source.ammo, "projectile": source.projectile,
                "source_errors": source.source_errors, "raw_payloads_retained": true,
                "functionality_certified": false,
                "target_projectile_lowering_implemented": false,
            })).collect::<Vec<_>>(),
        });
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&manifest)
                .map_err(|error| PhaseError::Internal(error.to_string()))?,
        )
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
        ctx.run.starfield_weapon_source_contracts = Some(contracts);
        Ok(PhaseReport {
            records_deferred: manifest["source_weapon_count"].as_u64().unwrap() as u32,
            warnings: manifest["source_weapon_count"].as_u64().unwrap() as u32,
            ..PhaseReport::default()
        })
    }
}

#[derive(Debug, Serialize)]
struct ModelClaim {
    source_model: String,
    source_record: String,
    perspective: &'static str,
    weapon_role: &'static str,
    source_exists: bool,
    provisional: bool,
}

#[derive(Debug, Serialize)]
struct PlannedOmod {
    source_raw: u32,
    editor_id: String,
    source: omod::SourceWeaponOmod,
    property_plan: Vec<omod::OmodPropertyPlan>,
}

#[derive(Debug, Serialize)]
struct SourceActorClip {
    source: starfield_subgraphs::ResolvedSourceClip,
    authored_last_frame: u16,
    animated_bone_count: u16,
    contract: starfield_afx::AfxContract,
}

#[derive(Debug, Serialize)]
struct SourceActorProfile {
    instance_selection: &'static str,
    actor_keywords: Vec<String>,
    target_keywords: Vec<String>,
    subgraph: starfield_subgraphs::SourceSubgraph,
    inspected_clips: Vec<SourceActorClip>,
    clip_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct WeaponPlan {
    source_form_key: String,
    source_raw: u32,
    editor_id: String,
    stats: Option<weapon::StarfieldWeaponComponents>,
    templates: Vec<omod::SourceObjectTemplate>,
    default_template_indices: Vec<usize>,
    omods: Vec<PlannedOmod>,
    models: Vec<ModelClaim>,
    source_fx_graphs: Vec<String>,
    source_fx_rigs: Vec<String>,
    source_actor_profiles: Vec<SourceActorProfile>,
    actor_selection_errors: Vec<String>,
    blockers: Vec<String>,
    source_errors: Vec<String>,
    functionality_certified: bool,
}

fn text(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes.strip_suffix(&[0]).unwrap_or(bytes))
        .ok()
        .map(str::to_owned)
}

fn mesh_path(root: &Path, path: &str) -> Option<std::path::PathBuf> {
    let relative = path.replace('\\', "/");
    if relative.starts_with('/')
        || relative.split('/').any(|part| part == "..")
        || relative.contains(':')
    {
        return None;
    }
    [
        root.join("Meshes").join(&relative),
        root.join("meshes").join(&relative),
        root.join(&relative),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn plan_weapon(
    scan: &HandleRawScan<'_>,
    raw: u32,
    key: FormKey,
    interner: &StringInterner,
    source_root: &Path,
) -> WeaponPlan {
    let mut plan = WeaponPlan {
        source_form_key: key.format(interner), source_raw: raw, editor_id: String::new(),
        stats: None, templates: vec![], default_template_indices: vec![], omods: vec![], models: vec![],
        source_fx_graphs: vec![], source_fx_rigs: vec![], source_actor_profiles: vec![], actor_selection_errors: vec![], source_errors: vec![],
        blockers: vec![
            "source ObjectTemplates and recursive OMOD properties require FO4 payload/reference/unit lowering".into(),
            "source actor RACE/AGX tag selection, additive pose composition and FO4 behavior registration are not integrated".into(),
            "source AF/AFX weapon-part motion, graph variables and bone visibility require target event/graph lowering".into(),
            "AMMO/PROJ, WWise audio, materials and optional script dependencies require target closure validation".into(),
        ], functionality_certified: false,
    };
    let Some(result) = scan.with_record_subrecords(raw, |fields| {
        let mut record = Record::new(SigCode(*b"WEAP"), key);
        let mut component = false;
        let mut template_count = None;
        for field in fields {
            let sig = field.signature.as_str();
            if sig == "BFCB" {
                component = true;
            }
            if sig == "BFCE" {
                component = false;
            }
            if let Some(value) = text(&field.data) {
                if sig == "EDID" {
                    plan.editor_id = value.clone();
                }
                if component && value.to_ascii_lowercase().ends_with(".agx") {
                    plan.source_fx_graphs.push(value.clone());
                }
                if component && value.to_ascii_lowercase().ends_with(".rig") {
                    plan.source_fx_rigs.push(value.clone());
                }
                if !component
                    && matches!(sig, "MODL" | "MOD4")
                    && value.to_ascii_lowercase().ends_with(".nif")
                {
                    plan.models.push(ModelClaim {
                        source_exists: mesh_path(source_root, &value).is_some(),
                        source_model: value,
                        source_record: plan.source_form_key.clone(),
                        perspective: if sig == "MOD4" { "First" } else { "Third" },
                        weapon_role: "gun",
                        provisional: true,
                    });
                }
            }
            if !component && sig == "OBTE" {
                if field.data.len() != 4 || template_count.is_some() {
                    return Err("invalid or duplicate source OBTE".to_owned());
                }
                template_count =
                    Some(u32::from_le_bytes(field.data[..].try_into().unwrap()) as usize);
            }
            if !component && sig == "OBTS" {
                plan.templates
                    .push(omod::decode_source_object_template(&field.data)?);
            }
            if sig.len() == 4 {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig::from_str(sig)?,
                    value: FieldValue::Bytes(field.data.to_vec().into()),
                });
            }
        }
        if component {
            return Err("unterminated Starfield component".into());
        }
        if template_count.unwrap_or(0) != plan.templates.len() {
            return Err("source OBTE count disagrees with OBTS templates".into());
        }
        plan.stats = Some(weapon::decode_components(&record)?);
        validate_template_parents(&plan.templates)?;
        Ok::<_, String>(())
    }) else {
        plan.source_errors
            .push("source WEAP record is absent".into());
        return plan;
    };
    if let Err(error) = result {
        plan.source_errors.push(error);
        return plan;
    }
    plan.default_template_indices = plan
        .templates
        .iter()
        .enumerate()
        .filter(|(_, template)| template.default && template.parent_combination_index == -1)
        .map(|(index, _)| index)
        .collect();
    if !plan.templates.is_empty() && plan.default_template_indices.len() != 1 {
        plan.blockers.push(format!("source has {} root default templates; conditional instance selection remains unresolved", plan.default_template_indices.len()));
    }
    let mut pending: BTreeSet<_> = plan
        .templates
        .iter()
        .flat_map(|template| template.includes.iter().map(|include| include.raw_mod))
        .collect();
    let mut seen = HashSet::new();
    while let Some(raw_mod) = pending.pop_first() {
        if !seen.insert(raw_mod) {
            continue;
        }
        let mut eid = String::new();
        let mut data = None;
        let mut models = Vec::new();
        let found = scan
            .with_record(raw_mod, |record| record.signature == "OMOD")
            .unwrap_or(false);
        if !found {
            plan.source_errors.push(format!(
                "required OMOD {raw_mod:08X} is absent or has the wrong record type"
            ));
            continue;
        }
        scan.with_record_subrecords(raw_mod, |fields| {
            for field in fields {
                match field.signature.as_str() {
                    "EDID" => eid = text(&field.data).unwrap_or_default(),
                    "DATA" => {
                        if data.is_some() {
                            plan.source_errors
                                .push(format!("OMOD {raw_mod:08X} has duplicate DATA"));
                        }
                        data = Some(field.data.clone());
                    }
                    "MODL" => {
                        if let Some(path) = text(&field.data)
                            .filter(|value| value.to_ascii_lowercase().ends_with(".nif"))
                        {
                            models.push(path);
                        }
                    }
                    _ => {}
                }
            }
        });
        let source = match data
            .as_deref()
            .ok_or_else(|| "missing DATA".to_owned())
            .and_then(omod::decode_source_omod)
        {
            Ok(source) => source,
            Err(error) => {
                plan.source_errors
                    .push(format!("OMOD {raw_mod:08X} {eid}: {error}"));
                continue;
            }
        };
        pending.extend(
            source
                .includes
                .iter()
                .map(|include| include.raw_mod)
                .filter(|id| !seen.contains(id)),
        );
        for model in models {
            plan.models.push(ModelClaim {
                source_exists: mesh_path(source_root, &model).is_some(),
                source_model: model,
                source_record: format!("OMOD:{raw_mod:08X}:{eid}"),
                perspective: "SharedPart",
                weapon_role: "gun",
                provisional: true,
            });
        }
        plan.omods.push(PlannedOmod {
            source_raw: raw_mod,
            editor_id: eid,
            property_plan: omod::property_plan(&source),
            source,
        });
    }
    for model in &plan.models {
        if !model.source_exists {
            plan.source_errors
                .push(format!("source model absent: {}", model.source_model));
        }
    }
    plan.source_fx_graphs.sort();
    plan.source_fx_graphs.dedup();
    plan.source_fx_rigs.sort();
    plan.source_fx_rigs.dedup();
    for path in plan.source_fx_graphs.iter().chain(&plan.source_fx_rigs) {
        if mesh_path(source_root, path).is_none() {
            plan.source_errors
                .push(format!("source graph/rig absent: {path}"));
        }
    }
    plan
}

fn validate_template_parents(templates: &[omod::SourceObjectTemplate]) -> Result<(), String> {
    for index in 0..templates.len() {
        let mut visited = HashSet::new();
        let mut current = index;
        loop {
            if !visited.insert(current) {
                return Err(format!("source ObjectTemplate parent cycle at {index}"));
            }
            let parent = templates
                .get(current)
                .ok_or_else(|| {
                    format!("source ObjectTemplate parent index {current} is out of range")
                })?
                .parent_combination_index;
            if parent == -1 {
                break;
            }
            if parent < -1 {
                return Err(format!("invalid source ObjectTemplate parent {parent}"));
            }
            current = parent as usize;
        }
    }
    Ok(())
}

fn default_keywords(
    base: BTreeSet<u32>,
    template: &omod::SourceObjectTemplate,
    mods: &FxHashMap<u32, omod::SourceWeaponOmod>,
) -> Result<BTreeSet<u32>, String> {
    fn apply_properties(
        properties: &[omod::OmodProperty],
        keywords: &mut BTreeSet<u32>,
    ) -> Result<(), String> {
        for property in properties {
            if property.property != *b"WKEY" {
                continue;
            }
            if property.value_type != 4 || property.value1 == 0 || property.step != 0. {
                return Err(
                    "source default actor selection has an unproven keyword property".into(),
                );
            }
            match property.function {
                1 => {
                    keywords.remove(&property.value1);
                }
                2 => {
                    keywords.insert(property.value1);
                }
                _ => {
                    return Err(
                        "source default actor selection has an unproven keyword SET operation"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }
    fn apply(
        raw: u32,
        mods: &FxHashMap<u32, omod::SourceWeaponOmod>,
        keywords: &mut BTreeSet<u32>,
        path: &mut HashSet<u32>,
    ) -> Result<(), String> {
        if !path.insert(raw) {
            return Err("source default keyword OMOD closure has a cycle".into());
        }
        let item = mods
            .get(&raw)
            .ok_or_else(|| format!("source default keyword OMOD {raw:08X} is absent"))?;
        for include in &item.includes {
            if include.optional
                || include.minimum_level != 0
                || (include.dont_use_all && item.includes.len() > 1)
            {
                return Err(
                    "source default actor selection depends on a conditional nested OMOD".into(),
                );
            }
            apply(include.raw_mod, mods, keywords, path)?;
        }
        apply_properties(&item.properties, keywords)?;
        path.remove(&raw);
        Ok(())
    }
    let mut keywords = base;
    keywords.extend(
        template
            .keywords_raw
            .iter()
            .copied()
            .filter(|raw| *raw != 0),
    );
    for include in &template.includes {
        if include.optional || (include.dont_use_all && template.includes.len() > 1) {
            return Err(
                "source default actor selection depends on an optional template mod".into(),
            );
        }
        apply(include.raw_mod, mods, &mut keywords, &mut HashSet::new())?;
    }
    apply_properties(&template.properties, &mut keywords)?;
    Ok(keywords)
}

fn plan_actor_profiles(
    scan: &HandleRawScan<'_>,
    plan: &WeaponPlan,
    mods: &FxHashMap<u32, omod::SourceWeaponOmod>,
    editor_ids: &FxHashMap<u32, String>,
    rows: &[starfield_subgraphs::SourceSubgraph],
    root: &Path,
) -> Result<Vec<SourceActorProfile>, String> {
    let mut base = BTreeSet::new();
    let valid = scan
        .with_record_subrecords(plan.source_raw, |fields| {
            let mut component = false;
            for field in fields {
                match field.signature.as_str() {
                    "BFCB" => component = true,
                    "BFCE" => component = false,
                    "KWDA" if !component => {
                        if field.data.len() % 4 != 0 {
                            return false;
                        }
                        base.extend(
                            field
                                .data
                                .chunks_exact(4)
                                .map(|raw| u32::from_le_bytes(raw.try_into().unwrap()))
                                .filter(|raw| *raw != 0),
                        );
                    }
                    _ => {}
                }
            }
            true
        })
        .unwrap_or(false);
    if !valid {
        return Err("source base weapon keyword list is unavailable or malformed".into());
    }
    let keywords = match plan.default_template_indices.as_slice() {
        [index] => default_keywords(base, &plan.templates[*index], mods)?,
        [] if plan.templates.is_empty() => base,
        _ => {
            return Err(
                "source actor selection requires exactly one root default ObjectTemplate".into(),
            );
        }
    };
    let target_keywords: Vec<_> = keywords
        .into_iter()
        .map(|raw| {
            editor_ids
                .get(&raw)
                .cloned()
                .ok_or_else(|| format!("source keyword {raw:08X} has no retained EditorID"))
        })
        .collect::<Result<_, _>>()?;
    let meshes = mesh_path(root, "AnimTextData/GeneratedSubgraphData/HumanRace.txt")
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let mut profiles = Vec::new();
    for (perspective, actor_keywords) in [
        ("1st", vec![]),
        ("3rd", vec!["AnimArchetypePlayer".into()]),
        ("3rd", vec![]),
    ] {
        let row = starfield_subgraphs::select_weapon_subgraph(
            rows,
            perspective,
            &actor_keywords,
            &target_keywords,
        )?;
        let mut profile = SourceActorProfile {
            instance_selection: "root_default_template_only",
            actor_keywords,
            target_keywords: target_keywords.clone(),
            subgraph: row.clone(),
            inspected_clips: Vec::new(),
            clip_errors: Vec::new(),
        };
        for tag in ["FireSingle", "Reload"] {
            let clip = (|| {
                let source = starfield_subgraphs::resolve_source_clip(row, tag, &meshes)?;
                let bytes = std::fs::read(meshes.join(&source.source_af))
                    .map_err(|error| error.to_string())?;
                if bytes.len() < 64 || i16::from_le_bytes(bytes[40..42].try_into().unwrap()) != 5 {
                    return Err("source actor clip has an unsupported AF header".to_owned());
                }
                let animated_bone_count = u16::from_le_bytes(bytes[42..44].try_into().unwrap());
                let authored_last_frame = u16::from_le_bytes(bytes[44..46].try_into().unwrap());
                let afx = std::fs::read_to_string(meshes.join(&source.source_afx))
                    .map_err(|error| error.to_string())?;
                let contract = starfield_afx::parse_afx(&afx, f64::from(authored_last_frame))?;
                Ok(SourceActorClip {
                    source,
                    animated_bone_count,
                    authored_last_frame,
                    contract,
                })
            })();
            match clip {
                Ok(clip) => profile.inspected_clips.push(clip),
                Err(error) => profile.clip_errors.push(format!("{tag}: {error}")),
            }
        }
        profiles.push(profile);
    }
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached_weapon(
        interner: &StringInterner,
        template: omod::SourceObjectTemplate,
    ) -> (SourceWeaponContracts, Record) {
        let key = FormKey {
            local: 0x800,
            plugin: interner.intern("Starfield.esm"),
        };
        let mut source = Record::new(SigCode(*b"WEAP"), key);
        for (sig, size) in [
            (*b"WGEN", 24),
            (*b"WAM2", 23),
            (*b"WDMG", 62),
            (*b"WFIR", 45),
            (*b"WRLO", 10),
            (*b"WFLG", 12),
        ] {
            source.fields.push(raw_field(sig, &vec![0; size]));
        }
        let mut cache = SourceWeaponContracts::default();
        cache.weapons.insert(
            key,
            SourceWeaponRecordContract {
                source_raw: 0x800,
                stats: Some(weapon::decode_components(&source).unwrap()),
                templates: vec![template],
                source_errors: vec![],
            },
        );
        source.fields.clear();
        source.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"FULL"),
                value: FieldValue::String(interner.intern("Weapon name")),
            },
            raw_field(*b"OBTE", &1_u32.to_le_bytes()),
            raw_field(*b"OBTF", &[]),
            FieldEntry {
                sig: SubrecordSig(*b"FULL"),
                value: FieldValue::String(interner.intern("Template name")),
            },
            raw_field(*b"OBTS", &[0xCC; 21]),
            raw_field(*b"DNAM", &[0; 132]),
        ]);
        (cache, source)
    }

    fn empty_template() -> omod::SourceObjectTemplate {
        let mut bytes = vec![0; 21];
        bytes[12..14].copy_from_slice(&(-1_i16).to_le_bytes());
        bytes[14] = 1;
        omod::decode_source_object_template(&bytes).unwrap()
    }

    fn mapper(interner: &StringInterner) -> FormKeyMapper<'_> {
        FormKeyMapper::new(
            std::iter::empty(),
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..Default::default()
            },
            interner,
        )
    }

    #[test]
    fn ammo_restoration_uses_original_binary_contract_after_component_shadow_decode() {
        let interner = StringInterner::new();
        let key = FormKey {
            local: 0x4AD3E,
            plugin: interner.intern("Starfield.esm"),
        };
        let source = starfield_ammo::SourceAmmoContract {
            value: 9,
            weight: 0.003,
            projectile_raw: 0x481F,
            flags: 1,
            damage: 7.,
            health: 123456789,
            casing_model: Some("Ammo/Casing.nif".into()),
        };
        let mut cache = SourceWeaponContracts::default();
        cache.ballistics.insert(
            key,
            SourceBallisticRecordContract {
                source_raw: key.local,
                signature: "AMMO".into(),
                fields: vec![],
                ammo: Some(source),
                projectile: None,
                source_errors: vec![],
            },
        );
        cache.ballistics.insert(
            FormKey {
                local: 0x481F,
                plugin: key.plugin,
            },
            SourceBallisticRecordContract {
                source_raw: 0x481F,
                signature: "PROJ".into(),
                fields: vec![],
                ammo: None,
                projectile: None,
                source_errors: vec![],
            },
        );
        let mut record = Record::new(SigCode(*b"AMMO"), key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::String(interner.intern("wrong-schema-response-file")),
        });
        record.fields.push(raw_field(*b"DATA", &[0xCC; 8]));
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"NAM1"),
            value: FieldValue::Float(1.),
        });
        cache.strip_instance_fields(key, &mut record).unwrap();
        assert!(record.fields.is_empty());
        let mut mapper = mapper(&interner);
        mapper.add_mapping(
            FormKey {
                local: 0x481F,
                plugin: key.plugin,
            },
            FormKey {
                local: 0x800,
                plugin: interner.intern("Converted.esm"),
            },
        );
        cache
            .restore_instance_fields(key, &mut record, &mut mapper, None)
            .unwrap();
        let FieldValue::Bytes(dnam) = &record.fields[1].value else {
            panic!("raw target DNAM")
        };
        assert_eq!(
            u32::from_le_bytes(dnam[..4].try_into().unwrap()),
            0x01000800
        );
        assert_eq!(
            u32::from_le_bytes(dnam[12..].try_into().unwrap()),
            123456789
        );
        assert_eq!(
            record.fields[2].value,
            FieldValue::String(interner.intern("Ammo/Casing.nif"))
        );
        assert!(cache.owns_ballistic(key));
    }

    #[test]
    fn ammo_unmapped_projectile_and_projectile_lowering_fail_without_record_mutation() {
        let interner = StringInterner::new();
        let key = FormKey {
            local: 0x800,
            plugin: interner.intern("Starfield.esm"),
        };
        let mut cache = SourceWeaponContracts::default();
        cache.ballistics.insert(
            key,
            SourceBallisticRecordContract {
                source_raw: key.local,
                signature: "AMMO".into(),
                fields: vec![],
                ammo: Some(starfield_ammo::SourceAmmoContract {
                    value: 9,
                    weight: 0.003,
                    projectile_raw: 0x481F,
                    flags: 0,
                    damage: 0.,
                    health: 0,
                    casing_model: None,
                }),
                projectile: None,
                source_errors: vec![],
            },
        );
        let mut record = Record::new(SigCode(*b"AMMO"), key);
        record.fields.push(raw_field(*b"EDID", b"Ammo\0"));
        let before = record.fields.clone();
        assert!(
            cache
                .restore_instance_fields(key, &mut record, &mut mapper(&interner), None)
                .is_err()
        );
        assert_eq!(record.fields, before);
        record.sig = SigCode(*b"PROJ");
        assert!(
            cache
                .strip_instance_fields(key, &mut record)
                .unwrap_err()
                .contains("not implemented")
        );
        assert_eq!(record.fields, before);
    }

    #[test]
    fn ammo_same_name_preserved_ids_still_require_explicit_projectile_mapping() {
        let interner = StringInterner::new();
        let key = FormKey {
            local: 0x800,
            plugin: interner.intern("Starfield.esm"),
        };
        let mut cache = SourceWeaponContracts::default();
        cache.ballistics.insert(
            key,
            SourceBallisticRecordContract {
                source_raw: key.local,
                signature: "AMMO".into(),
                fields: vec![],
                ammo: Some(starfield_ammo::SourceAmmoContract {
                    value: 9,
                    weight: 0.003,
                    projectile_raw: 0x481F,
                    flags: 0,
                    damage: 0.,
                    health: 0,
                    casing_model: None,
                }),
                projectile: None,
                source_errors: vec![],
            },
        );
        cache.ballistics.insert(
            FormKey {
                local: 0x481F,
                plugin: key.plugin,
            },
            SourceBallisticRecordContract {
                source_raw: 0x481F,
                signature: "PROJ".into(),
                fields: vec![],
                ammo: None,
                projectile: None,
                source_errors: vec![],
            },
        );
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: "Starfield.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                preserve_source_ids: true,
                target_master_names: vec!["Fallout4.esm".into()],
                ..Default::default()
            },
            &interner,
        );
        let mut record = Record::new(SigCode(*b"AMMO"), key);
        record.fields.push(raw_field(*b"EDID", b"Ammo\0"));
        let before = record.fields.clone();
        let error = cache
            .restore_instance_fields(key, &mut record, &mut mapper, None)
            .unwrap_err();
        assert!(error.contains("no explicit target mapping"), "{error}");
        assert_eq!(record.fields, before);
    }

    #[test]
    fn cached_template_restoration_keeps_decoded_names_and_target_group_order() {
        use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
        let interner = StringInterner::new();
        let (cache, mut record) = cached_weapon(&interner, empty_template());
        let key = record.form_key;
        let metadata = cache.capture_instance_metadata(key, &record).unwrap();
        cache.strip_instance_fields(key, &mut record).unwrap();
        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            ["FULL", "DNAM"]
        );
        cache
            .restore_instance_fields(key, &mut record, &mut mapper(&interner), metadata.as_ref())
            .unwrap();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let TargetRecordNormalization::Keep(record) =
            TargetRecordNormalizer::target_only_with_interner(&schema, &interner).normalize(record)
        else {
            panic!("WEAP retained")
        };
        let group = record
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"OBTE")
            .unwrap();
        assert_eq!(
            record.fields[group..]
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            ["OBTE", "OBTF", "FULL", "OBTS", "STOP"]
        );
        let names = record
            .fields
            .iter()
            .filter_map(|field| match (&field.sig.0, &field.value) {
                (b"FULL", FieldValue::String(value)) => interner.resolve(*value).map(str::to_owned),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(names, ["Weapon name", "Template name"]);
    }

    #[test]
    fn cached_template_restoration_rejects_unresolved_names_and_is_atomic_on_reference_failure() {
        let interner = StringInterner::new();
        let mut template = empty_template();
        template.keywords_raw.push(0x123);
        let (cache, mut record) = cached_weapon(&interner, template);
        let key = record.form_key;
        record.fields[3].value = FieldValue::Uint(42);
        assert!(
            cache
                .capture_instance_metadata(key, &record)
                .unwrap_err()
                .contains("resolved")
        );
        record.fields[3].value = FieldValue::String(interner.intern("Template name"));
        let metadata = cache.capture_instance_metadata(key, &record).unwrap();
        cache.strip_instance_fields(key, &mut record).unwrap();
        let before = record.fields.clone();
        assert!(
            cache
                .restore_instance_fields(
                    key,
                    &mut record,
                    &mut mapper(&interner),
                    metadata.as_ref()
                )
                .unwrap_err()
                .contains("mapping")
        );
        assert_eq!(record.fields, before);
        let mut absent = Record::new(SigCode(*b"WEAP"), key);
        absent.form_key.local += 1;
        assert!(
            cache
                .strip_instance_fields(absent.form_key, &mut absent)
                .is_err()
        );
    }

    #[test]
    fn cache_instance_hooks_only_own_weapon_mods_and_restore_after_strict_lowering() {
        let interner = StringInterner::new();
        let key = FormKey {
            local: 0x900,
            plugin: interner.intern("Starfield.esm"),
        };
        let mut cache = SourceWeaponContracts::default();
        cache.omod_raw_by_key.insert(key, key.local);
        cache.omods.insert(
            key.local,
            omod::SourceWeaponOmod {
                unknown_header: [0; 2],
                instance_class: "TESObjectWEAP_InstanceData".into(),
                max_rank: 0,
                level_tier_offset: 0,
                attach_point_raw: 0,
                attach_parent_slots_raw: vec![],
                unknown_item: 0,
                includes: vec![],
                properties: vec![],
            },
        );
        let mut record = Record::new(SigCode(*b"OMOD"), key);
        record.fields.push(raw_field(*b"DATA", &[0xCC; 4]));
        cache.strip_instance_fields(key, &mut record).unwrap();
        cache
            .restore_instance_fields(key, &mut record, &mut mapper(&interner), None)
            .unwrap();
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("raw target payload")
        };
        assert_eq!(&bytes[10..14], b"WEAP");
        let other = FormKey {
            local: key.local + 1,
            ..key
        };
        record.form_key = other;
        let before = record.fields.clone();
        cache.strip_instance_fields(other, &mut record).unwrap();
        cache
            .restore_instance_fields(other, &mut record, &mut mapper(&interner), None)
            .unwrap();
        assert_eq!(record.fields, before);
    }

    #[test]
    fn absent_deferred_weapon_retains_error_without_ready_admission() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };
        let handle = plugin_handle_new_native("MissingWeaponPlan.esp", Some("starfield")).unwrap();
        let interner = StringInterner::new();
        let plan = {
            let mut session = crate::session::open_session(handle, None).unwrap();
            let scan = session.handle_raw_scan(handle).unwrap();
            plan_weapon(
                &scan,
                0x800,
                FormKey {
                    local: 0x800,
                    plugin: interner.intern("MissingWeaponPlan.esp"),
                },
                &interner,
                Path::new("unused"),
            )
        };
        assert!(!plan.source_errors.is_empty());
        assert!(!plan.functionality_certified);
        let receipt = serde_json::to_value(&plan).unwrap();
        assert!(
            receipt["source_errors"][0]
                .as_str()
                .unwrap()
                .contains("absent")
        );
        assert_eq!(receipt["functionality_certified"], false);
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn template_parent_cycles_and_missing_parents_are_rejected() {
        let mut bytes = vec![0; 21];
        bytes[12..14].copy_from_slice(&(-1_i16).to_le_bytes());
        bytes[14] = 1;
        let mut template = omod::decode_source_object_template(&bytes).unwrap();
        assert!(validate_template_parents(&[template.clone()]).is_ok());
        template.parent_combination_index = 0;
        assert!(
            validate_template_parents(&[template.clone()])
                .unwrap_err()
                .contains("cycle")
        );
        template.parent_combination_index = 2;
        assert!(
            validate_template_parents(&[template])
                .unwrap_err()
                .contains("out of range")
        );
    }

    #[test]
    fn model_claims_cannot_escape_source_mesh_root() {
        let root = Path::new("unused");
        for path in [
            "../secret.nif",
            "Weapons/../../secret.nif",
            "C:/secret.nif",
            "/secret.nif",
        ] {
            assert!(mesh_path(root, path).is_none());
        }
    }

    #[test]
    #[ignore = "requires source Starfield.esm and extracted weapon assets"]
    fn real_eon_and_shotty_plans_follow_modular_source_models() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_load_lazy_records_no_py,
        };
        let path = "N:/Steam Games/steamapps/common/Starfield/Data/Starfield.esm";
        let handle = plugin_handle_load_lazy_records_no_py(
            path,
            "starfield",
            None,
            &[*b"WEAP", *b"OMOD", *b"KYWD"],
        )
        .unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let interner = StringInterner::new();
        let plans = {
            let mut session = crate::session::open_session(handle, None).unwrap();
            let scan = session.handle_raw_scan(handle).unwrap();
            let mut plans = [0x0476C4, 0x26D960]
                .into_iter()
                .map(|raw| {
                    plan_weapon(
                        &scan,
                        raw,
                        FormKey {
                            local: raw,
                            plugin: interner.intern("Starfield.esm"),
                        },
                        &interner,
                        &root.join("extracted/starfield"),
                    )
                })
                .collect::<Vec<_>>();
            let mods = plans
                .iter()
                .flat_map(|plan| {
                    plan.omods
                        .iter()
                        .map(|item| (item.source_raw, item.source.clone()))
                })
                .collect();
            let rows = starfield_subgraphs::parse_generated_subgraphs(
                &std::fs::read_to_string(root.join(
                    "extracted/starfield/Meshes/AnimTextData/GeneratedSubgraphData/HumanRace.txt",
                ))
                .unwrap(),
            )
            .unwrap();
            for plan in &mut plans {
                plan.source_actor_profiles = plan_actor_profiles(
                    &scan,
                    plan,
                    &mods,
                    &scan.own_editor_ids(),
                    &rows,
                    &root.join("extracted/starfield"),
                )
                .unwrap();
            }
            plans
        };
        assert!(plugin_handle_close_native(handle));
        for plan in &plans {
            assert!(
                plan.source_errors.is_empty(),
                "{}: {:?}",
                plan.editor_id,
                plan.source_errors
            );
            assert!(plan.models.iter().any(|claim| {
                claim
                    .source_model
                    .to_ascii_lowercase()
                    .contains("dummyreceiver")
            }));
            assert!(plan.models.len() >= 5);
            assert!(plan.omods.len() >= 12);
            assert!(!plan.source_fx_graphs.is_empty() && !plan.source_fx_rigs.is_empty());
            assert!(!plan.functionality_certified);
            assert_eq!(plan.source_actor_profiles.len(), 3);
            if plan.editor_id == "Eon" {
                for profile in &plan.source_actor_profiles {
                    assert!(profile.clip_errors.is_empty(), "{:?}", profile.clip_errors);
                    assert!(
                        profile
                            .target_keywords
                            .iter()
                            .any(|keyword| keyword == "AnimStinger")
                    );
                    assert!(
                        profile
                            .target_keywords
                            .iter()
                            .any(|keyword| keyword == "AnimGripPistol")
                    );
                    assert_eq!(profile.inspected_clips.len(), 2);
                    assert!(
                        profile
                            .inspected_clips
                            .iter()
                            .all(|clip| clip.animated_bone_count > 0)
                    );
                }
            }
            println!(
                "{}: {} templates, {} recursive OMODs, {} model claims",
                plan.editor_id,
                plan.templates.len(),
                plan.omods.len(),
                plan.models.len()
            );
        }
        std::fs::write(
            std::env::temp_dir().join("b21_sf_weapon_native_plan_receipt.json"),
            serde_json::to_vec_pretty(&plans).unwrap(),
        )
        .unwrap();
    }
}
