use super::*;
use crate::fixups::havok::normalize_weapon_behavior_contracts::merge_behavior_contracts;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

#[cfg(test)]
thread_local! {
    static SERIALIZED_CONVERSION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn convert(path: &Path) -> Result<HkxFile, String> {
    #[cfg(test)]
    if SERIALIZED_CONVERSION.get() {
        let report = havok_native::api::havok_convert_bytes_report(&source_bytes(path)?, VERSION)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        return HkxFile::read(&report.bytes).map_err(|e| e.to_string());
    }
    havok_native::api::havok_convert_for_editing(&source_bytes(path)?, VERSION)
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn write(root: &Path, relative: &str, file: &HkxFile) -> Result<(), String> {
    let path = root.join(relative.replace('\\', "/"));
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, file.save()).map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Default)]
pub struct BehaviorGenerationReport {
    pub assets_written: u32,
    pub discovery: Duration,
    pub parsing: Duration,
    pub route_construction: Duration,
    pub writes: Duration,
    pub parsed_inputs: u32,
    pub shared_input_cache_hits: u32,
    pub bone_order_cache_hits: u32,
}

#[derive(Default)]
struct ParsedInputs {
    reuse_shared_inputs: bool,
    cache_animation_inputs: bool,
    route_converted: HashMap<String, HkxFile>,
    shared_converted: HashMap<String, HkxFile>,
    parsed: BTreeMap<PathBuf, HkxFile>,
    bone_orders: HashMap<String, Vec<Option<usize>>>,
    parsing: Duration,
    writes: Duration,
    parsed_inputs: u32,
    shared_input_cache_hits: u32,
    bone_order_cache_hits: u32,
}

impl ParsedInputs {
    fn new(reuse_shared_inputs: bool, cache_animation_inputs: bool) -> Self {
        Self {
            reuse_shared_inputs,
            cache_animation_inputs,
            ..Self::default()
        }
    }

    fn convert(&mut self, source: &Path, relative: &str) -> Result<HkxFile, String> {
        if self.reuse_shared_inputs {
            if let Some(file) = self.shared_converted.get(relative) {
                self.shared_input_cache_hits += 1;
                return Ok(file.clone());
            }
        }
        let started = Instant::now();
        let file = convert(&source.join(relative))?;
        self.parsing += started.elapsed();
        self.parsed_inputs += 1;
        if self.reuse_shared_inputs {
            self.shared_converted
                .insert(relative.to_owned(), file.clone());
        }
        Ok(file)
    }

    fn convert_transient(&mut self, source: &Path, relative: &str) -> Result<HkxFile, String> {
        let started = Instant::now();
        let file = convert(&source.join(relative))?;
        self.parsing += started.elapsed();
        self.parsed_inputs += 1;
        Ok(file)
    }

    fn route_convert(&mut self, source: &Path, relative: &str) -> Result<HkxFile, String> {
        if self.reuse_shared_inputs {
            return self.convert(source, relative);
        }
        if let Some(file) = self.route_converted.get(relative) {
            return Ok(file.clone());
        }
        let started = Instant::now();
        let file = convert(&source.join(relative))?;
        self.parsing += started.elapsed();
        self.parsed_inputs += 1;
        self.route_converted
            .insert(relative.to_owned(), file.clone());
        Ok(file)
    }

    fn read(&mut self, path: &Path) -> Result<HkxFile, String> {
        if self.reuse_shared_inputs {
            if let Some(file) = self.parsed.get(path) {
                self.shared_input_cache_hits += 1;
                return Ok(file.clone());
            }
        }
        let started = Instant::now();
        let file = read(path)?;
        self.parsing += started.elapsed();
        self.parsed_inputs += 1;
        if self.reuse_shared_inputs {
            self.parsed.insert(path.to_owned(), file.clone());
        }
        Ok(file)
    }

    fn read_transient(&mut self, path: &Path) -> Result<HkxFile, String> {
        let started = Instant::now();
        let file = read(path)?;
        self.parsing += started.elapsed();
        self.parsed_inputs += 1;
        Ok(file)
    }

    fn convert_animation(&mut self, source: &Path, relative: &str) -> Result<HkxFile, String> {
        if self.cache_animation_inputs {
            self.convert(source, relative)
        } else {
            self.convert_transient(source, relative)
        }
    }

    fn read_animation(&mut self, path: &Path) -> Result<HkxFile, String> {
        if self.cache_animation_inputs {
            self.read(path)
        } else {
            self.read_transient(path)
        }
    }

    fn bone_order(
        &mut self,
        source: &Path,
        target: &Path,
        dir: &str,
    ) -> Result<Vec<Option<usize>>, String> {
        if self.reuse_shared_inputs {
            if let Some(order) = self.bone_orders.get(dir) {
                self.bone_order_cache_hits += 1;
                return Ok(order.clone());
            }
        }
        let started = Instant::now();
        let order = bone_order(source, target, dir)?;
        self.parsing += started.elapsed();
        self.parsed_inputs += 2;
        if self.reuse_shared_inputs {
            self.bone_orders.insert(dir.to_owned(), order.clone());
        }
        Ok(order)
    }

    fn dependencies(&mut self, source: &Path, graph: &str) -> Result<Vec<String>, String> {
        let mut cache = BTreeMap::new();
        let started = Instant::now();
        let result = dependencies(source, graph, &mut cache);
        self.parsing += started.elapsed();
        self.parsed_inputs += cache.len() as u32;
        result
    }

    fn write(&mut self, root: &Path, relative: &str, file: &HkxFile) -> Result<(), String> {
        let started = Instant::now();
        write(root, relative, file)?;
        self.writes += started.elapsed();
        Ok(())
    }
}
fn animation_path(project: &Project, source: &str) -> String {
    format!(
        "actors/B21_FO76/Source/{}/{}",
        if project.vanilla {
            "fo4rig"
        } else {
            "source_rig"
        },
        clean(source)
    )
}

pub fn namespace_draw(file: &mut HkxFile, event: &str) -> bool {
    let Some(strings) = file
        .objects_mut()
        .iter_mut()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
    else {
        return false;
    };
    let Some(member) = strings.members.iter_mut().find(|m| m.name == "eventNames") else {
        return false;
    };
    let HkxValue::Array(names) = &mut member.value else {
        return false;
    };
    let Some(name) = names
        .iter_mut()
        .find(|v| text(v).is_some_and(|s| s.eq_ignore_ascii_case("weapEquip")))
    else {
        return false;
    };
    *name = HkxValue::String {
        value: event.to_string(),
        is_null: false,
    };
    true
}

fn restore_character_values(source: &HkxFile, target: &mut HkxFile) {
    let Some(data) = source
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbCharacterData")
    else {
        return;
    };
    let Some(source_values) = pointer(data, "characterPropertyValues") else {
        return;
    };
    let infos = array(data, "characterPropertyInfos");
    let words = array(&source.objects()[source_values], "wordVariableValues");
    let Some(target_values) = target
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbCharacterData")
        .and_then(|o| pointer(o, "characterPropertyValues"))
    else {
        return;
    };
    let mut converted = array(&target.objects()[target_values], "wordVariableValues");
    for (i, info) in infos.iter().enumerate() {
        let pointer_type = info
            .as_object_members()
            .and_then(|m| m.iter().find(|m| m.name == "type"))
            .and_then(|m| number(&m.value))
            == Some(5.0);
        if !pointer_type && i < converted.len() && i < words.len() {
            converted[i] = words[i].clone();
        }
    }
    set(
        &mut target.objects_mut()[target_values],
        "wordVariableValues",
        HkxValue::Array(converted),
    );
}

fn skeleton_bones(file: &HkxFile) -> Vec<String> {
    file.objects()
        .iter()
        .find(|o| o.class_name == "hkaSkeleton")
        .map(|o| {
            array(o, "bones")
                .iter()
                .filter_map(|v| {
                    v.as_object_members()?
                        .iter()
                        .find(|m| m.name == "name")
                        .and_then(|m| text(&m.value))
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}
pub(super) fn project_rig(root: &Path, dir: &str) -> Result<HkxFile, String> {
    let conventional = root.join(format!("{dir}/characterassets/skeleton.hkx"));
    if conventional.try_exists().map_err(|e| e.to_string())? {
        return read(&conventional);
    }
    // RoboBrain's character references the shared CreateABot rig outside its project directory.
    let mut rigs = BTreeSet::new();
    for entry in
        std::fs::read_dir(root.join(dir).join("characters")).map_err(|e| format!("{dir}: {e}"))?
    {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("hkx"))
        {
            continue;
        }
        for object in read(&path)?
            .objects()
            .iter()
            .filter(|o| o.class_name == "hkbCharacterStringData")
        {
            let rig = string(object, "rigName");
            if !rig.is_empty() {
                rigs.insert(relative_asset(dir, &rig));
            }
        }
    }
    if rigs.len() != 1 {
        return Err(format!("{dir}: expected one character rig, found {rigs:?}"));
    }
    read(&root.join(rigs.first().unwrap()))
}

pub(super) fn bone_order(
    source: &Path,
    target: &Path,
    dir: &str,
) -> Result<Vec<Option<usize>>, String> {
    let source = project_rig(source, dir)?;
    let target = project_rig(target, dir)?;
    let source = skeleton_bones(&source);
    let target = skeleton_bones(&target);
    if source.is_empty() || target.is_empty() {
        return Err(format!("{dir}: missing skeleton bone table"));
    }
    Ok(target
        .iter()
        .map(|name| source.iter().position(|s| s == name))
        .collect())
}
fn remap_masks(file: &mut HkxFile, order: &[Option<usize>]) {
    for object in file
        .objects_mut()
        .iter_mut()
        .filter(|o| o.class_name == "hkbBoneWeightArray")
    {
        let weights: Vec<_> = match value(object, "boneWeights") {
            Some(HkxValue::F32List(values)) => values.clone(),
            Some(HkxValue::Array(values)) => values.iter().filter_map(number).collect(),
            _ => vec![],
        };
        if weights.is_empty() {
            continue;
        }
        set(
            object,
            "boneWeights",
            HkxValue::Array(
                order
                    .iter()
                    .map(|i| HkxValue::F32(i.and_then(|i| weights.get(i).copied()).unwrap_or(0.0)))
                    .collect(),
            ),
        );
    }
}

fn property_bone_weights(
    file: &HkxFile,
    info: &HkxValue,
    word: &HkxValue,
    variants: &[HkxValue],
) -> Option<Vec<HkxValue>> {
    let member_number = |value: &HkxValue, name: &str| {
        value
            .as_object_members()?
            .iter()
            .find(|m| m.name == name)
            .and_then(|m| number(&m.value))
    };
    if member_number(info, "type") != Some(5.0) {
        return None;
    }
    let slot = member_number(word, "value")? as usize;
    let HkxValue::Pointer(Some(index)) = variants.get(slot)? else {
        return None;
    };
    let object = file.objects().get(*index)?;
    (object.class_name == "hkbBoneWeightArray").then(|| array(object, "boneWeights"))
}

fn append_property_values(
    target: &mut HkxFile,
    source: &HkxFile,
    bone_order: Option<&[Option<usize>]>,
) -> Result<(), String> {
    let data_index = |f: &HkxFile| {
        f.objects()
            .iter()
            .position(|o| o.class_name == "hkbCharacterData")
            .ok_or("missing character data".to_string())
    };
    let source_data = data_index(source)?;
    let target_data = data_index(target)?;
    let source_strings = pointer(&source.objects()[source_data], "stringData")
        .ok_or("missing source character strings")?;
    let target_strings = pointer(&target.objects()[target_data], "stringData")
        .ok_or("missing target character strings")?;
    let source_values = pointer(&source.objects()[source_data], "characterPropertyValues")
        .ok_or("missing source character values")?;
    let target_values = pointer(&target.objects()[target_data], "characterPropertyValues")
        .ok_or("missing target character values")?;
    let names = array(&source.objects()[source_strings], "characterPropertyNames");
    let infos = array(&source.objects()[source_data], "characterPropertyInfos");
    let words = array(&source.objects()[source_values], "wordVariableValues");
    let variants = array(&source.objects()[source_values], "variantVariableValues");
    let mut target_names = array(&target.objects()[target_strings], "characterPropertyNames");
    let mut target_infos = array(&target.objects()[target_data], "characterPropertyInfos");
    let mut target_words = array(&target.objects()[target_values], "wordVariableValues");
    let mut target_variants = array(&target.objects()[target_values], "variantVariableValues");
    let target_aim_source = target_names
        .iter()
        .position(|n| text(n).is_some_and(|s| s.eq_ignore_ascii_case("DirectAtWeaponBoneIndex")))
        .and_then(|i| target_words.get(i))
        .cloned();
    if names.len() != infos.len() || names.len() != words.len() {
        return Err("misaligned source character properties".into());
    }
    for (i, name) in names.iter().enumerate() {
        let existing = target_names.iter().position(|n| {
            text(n)
                .zip(text(name))
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        });
        if let Some(index) = existing {
            let target_mask = property_bone_weights(
                target,
                &target_infos[index],
                &target_words[index],
                &target_variants,
            );
            let source_mask = property_bone_weights(source, &infos[i], &words[i], &variants);
            if !target_mask
                .zip(source_mask)
                .is_some_and(|(target, source)| target.is_empty() && !source.is_empty())
            {
                continue;
            }
        }
        let mut word = words[i].clone();
        // FO76's AimSource bone is absent on the FO4 rig; its numeric index is Neck there.
        if text(name).is_some_and(|s| s.eq_ignore_ascii_case("AimSource")) {
            if let Some(target_word) = &target_aim_source {
                word = target_word.clone();
            }
        } else if let Some(order) =
            bone_order.filter(|_| text(name).is_some_and(|s| s.ends_with("BoneIndex")))
        {
            let value = word
                .as_object_members_mut()
                .and_then(|members| members.iter_mut().find(|m| m.name == "value"))
                .ok_or("missing bone property value")?;
            let index = number(&value.value).ok_or("invalid bone property index")? as i32;
            if index >= 0 {
                let target_index = order
                    .iter()
                    .position(|source| *source == Some(index as usize))
                    .ok_or_else(|| {
                        format!(
                            "unmapped character bone property {}: {index}",
                            text(name).unwrap()
                        )
                    })?;
                value.value = HkxValue::I32(target_index as i32);
            }
        }
        let ty = infos[i]
            .as_object_members()
            .and_then(|m| m.iter().find(|m| m.name == "type"))
            .and_then(|m| number(&m.value));
        if ty == Some(5.0) {
            let fields = word
                .as_object_members_mut()
                .ok_or("invalid character property word")?;
            let value = fields
                .iter_mut()
                .find(|m| m.name == "value")
                .ok_or("missing character property word")?;
            let slot = number(&value.value).ok_or("invalid pointer property slot")? as usize;
            let Some(HkxValue::Pointer(Some(index))) = variants.get(slot) else {
                return Err(format!(
                    "{}: null source character property",
                    text(name).unwrap_or_default()
                ));
            };
            let object = &source.objects()[*index];
            if object.class_name != "hkbBoneWeightArray" {
                return Err(format!(
                    "unsupported source character property {}: {}",
                    text(name).unwrap_or_default(),
                    object.class_name
                ));
            }
            let copied = add(target, object.clone());
            value.value = HkxValue::I32(target_variants.len() as i32);
            target_variants.push(HkxValue::Pointer(Some(copied)));
        }
        if let Some(index) = existing {
            // FO4's empty placeholders (including the camera reload mask) can share a variant.
            target_words[index] = word;
        } else {
            target_names.push(name.clone());
            target_infos.push(infos[i].clone());
            target_words.push(word);
        }
    }
    set(
        &mut target.objects_mut()[target_strings],
        "characterPropertyNames",
        HkxValue::Array(target_names),
    );
    set(
        &mut target.objects_mut()[target_data],
        "characterPropertyInfos",
        HkxValue::Array(target_infos),
    );
    set(
        &mut target.objects_mut()[target_values],
        "wordVariableValues",
        HkxValue::Array(target_words),
    );
    set(
        &mut target.objects_mut()[target_values],
        "variantVariableValues",
        HkxValue::Array(target_variants),
    );
    Ok(())
}

fn remove_shredder_swing_events(animation: &mut HkxFile, origin: &str) {
    let origin = clean(origin);
    if !matches!(
        origin.as_str(),
        "actors/character/animations/weapon/chainsaw/wpnmeleeshredder.hkx"
            | "actors/character/animations/weapon/chainsaw/wpnmeleeshredder_additive.hkx"
    ) {
        return;
    }
    for object in animation.objects_mut() {
        let mut tracks = array(object, "annotationTracks");
        if tracks.is_empty() {
            continue;
        }
        for track in &mut tracks {
            let HkxValue::Object(members) = track else {
                continue;
            };
            for member in members {
                if let ("annotations", HkxValue::Array(events)) =
                    (member.name.as_str(), &mut member.value)
                {
                    // FO4 treats every loop's weaponSwing as another player combat grunt.
                    events.retain(|event| {
                        !event.as_object_members().is_some_and(|fields| {
                            fields.iter().any(|field| {
                                field.name == "text"
                                    && text(&field.value).is_some_and(|value| {
                                        value.eq_ignore_ascii_case("weaponSwing")
                                    })
                            })
                        })
                    });
                }
            }
        }
        set(object, "annotationTracks", HkxValue::Array(tracks));
    }
}

fn copy_animation(
    source: &Path,
    output: &Path,
    project: &Project,
    origin: &str,
    written: &mut BTreeSet<String>,
    inputs: &mut ParsedInputs,
) -> Result<(), String> {
    let destination = animation_path(project, origin);
    if !written.insert(destination.clone()) {
        return Ok(());
    }
    let mut animation = if project.vanilla {
        inputs.convert_animation(source, origin)?
    } else {
        inputs.read_animation(&source.join(origin))?
    };
    animation.set_contents_version(VERSION);
    remove_shredder_swing_events(&mut animation, origin);
    inputs.write(output, &destination, &animation)
}

pub(super) fn project_roots(root: &Path, project: &str) -> Result<BTreeSet<String>, String> {
    let mut roots = BTreeSet::new();
    for entry in
        std::fs::read_dir(root.join(project).join("characters")).map_err(|e| e.to_string())?
    {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("hkx"))
        {
            continue;
        }
        let character = read(&path)?;
        for object in character.objects() {
            if object.class_name == "hkbCharacterStringData" {
                let behavior = string(object, "behaviorFilename");
                if !behavior.is_empty() {
                    roots.insert(relative_asset(project, &behavior));
                }
            }
        }
    }
    for entry in
        std::fs::read_dir(root.join(project).join("behaviors")).map_err(|e| e.to_string())?
    {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        if name.contains("root") && name.ends_with(".hkx") {
            roots.insert(format!("{project}/behaviors/{name}"));
        }
    }
    Ok(roots)
}

pub fn build(mod_path: &Path, source_root: &Path, target_root: &Path) -> Result<u32, String> {
    Ok(build_profiled(mod_path, source_root, target_root)?.assets_written)
}

pub fn build_profiled(
    mod_path: &Path,
    source_root: &Path,
    target_root: &Path,
) -> Result<BehaviorGenerationReport, String> {
    build_with_profile(mod_path, source_root, target_root, true, false)
}

fn build_with_profile(
    mod_path: &Path,
    source_root: &Path,
    target_root: &Path,
    reuse_shared_inputs: bool,
    cache_animation_inputs: bool,
) -> Result<BehaviorGenerationReport, String> {
    let started = Instant::now();
    let plan_path = mod_path.join(PLAN_PATH);
    if !plan_path.is_file() {
        return Ok(BehaviorGenerationReport {
            discovery: started.elapsed(),
            ..BehaviorGenerationReport::default()
        });
    }
    let plan: Plan = serde_json::from_slice(&std::fs::read(&plan_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let source = meshes(source_root);
    let target = meshes(target_root);
    let output = mod_path.join("data/Meshes");
    let discovery = started.elapsed();
    let mut inputs = ParsedInputs::new(reuse_shared_inputs, cache_animation_inputs);
    let vanilla_blends =
        inputs.read(&target.join("actors/character/behaviors/weaponbehavior.hkx"))?;
    let mut written = BTreeSet::new();
    let mut contracts: BTreeMap<String, Vec<HkxFile>> = BTreeMap::new();
    for route in &plan.routes {
        let project = &plan.projects[&route.project];
        let bow_retarget = if project.vanilla && super::bow::power_armor_route(route) {
            Some(super::bow_power_armor::Retargeter::load(
                &source,
                &target,
                &project.source,
            )?)
        } else {
            None
        };
        let animation_destination = |origin: &str| {
            format!(
                "actors/B21_FO76/Source/{}/{}",
                super::bow::animation_namespace(project, route),
                clean(origin)
            )
        };
        let order = if bow_retarget.as_ref().is_some_and(|rig| !rig.is_identity()) {
            let source_rig =
                project_rig(&source, &project.source.replace("powerarmor", "character"))?;
            let target_rig = project_rig(&target, &project.source)?;
            let source_bones = skeleton_bones(&source_rig);
            Some(
                skeleton_bones(&target_rig)
                    .iter()
                    .map(|name| source_bones.iter().position(|source| source == name))
                    .collect(),
            )
        } else if project.vanilla && !project.source.contains("_1stperson") {
            Some(inputs.bone_order(&source, &target, &project.source)?)
        } else {
            None
        };
        let mut graphs = BTreeMap::new();
        for (origin, destination) in &route.dependencies {
            let mut graph = inputs.route_convert(&source, origin)?;
            super::boss::repair(&mut graph, &source, origin, &route.project)?;
            super::locomotion::repair(
                &mut graph,
                &source,
                &route.animations,
                origin,
                &vanilla_blends,
            )?;
            super::bow::repair_player_locomotion(&mut graph, route);
            super::bow::suppress_power_armor_first_person_fidgets(&mut graph, route);
            if let Some(order) = &order {
                remap_masks(&mut graph, order);
            }
            for object in graph.objects_mut() {
                match object.class_name.as_str() {
                    "hkbBehaviorGraph" => set_string(
                        object,
                        "name",
                        &format!(
                            "{}.hkb",
                            Path::new(destination)
                                .file_stem()
                                .unwrap()
                                .to_string_lossy()
                        ),
                    ),
                    "hkbBehaviorReferenceGenerator" => {
                        let dependency =
                            relative_asset(&project_dir(origin), &string(object, "behaviorName"));
                        let destination = route
                            .dependencies
                            .get(&dependency)
                            .ok_or_else(|| format!("unplanned graph dependency {dependency}"))?;
                        set_string(
                            object,
                            "behaviorName",
                            &relative_path(&project.destination, destination),
                        );
                    }
                    "hkbClipGenerator" => {
                        let name = string(object, "animationName");
                        let origin = route
                            .animations
                            .get(&name)
                            .cloned()
                            .unwrap_or_else(|| relative_asset(&project_dir(origin), &name));
                        set_string(
                            object,
                            "animationName",
                            &relative_path(&project.destination, &animation_destination(&origin)),
                        );
                    }
                    _ => {}
                }
            }
            super::furniture::repair_gas_pump_exit(&mut graph, origin, &target)?;
            super::draw::repair(&mut graph, origin)?;
            super::sneak::repair(&mut graph)?;
            if let Some(event) = &route.draw_event {
                namespace_draw(&mut graph, event);
            }
            graphs.insert(destination.clone(), graph);
        }
        // FO4 resolves wrapper events against its own tables; FO76 wrappers may
        // inherit empty tables. Merge the dependency closure before root union.
        let dependency_contracts: Vec<_> = graphs.values().cloned().collect();
        for (path, graph) in &mut graphs {
            for dependency in &dependency_contracts {
                merge_behavior_contracts(graph, dependency)?;
            }
            inputs.write(&output, path, graph)?;
            written.insert(path.clone());
        }
        // Fishing is driven by native events and variables absent from FO4's root.
        if !project.vanilla
            || !furniture_graph(&route.source)
            || clean(&route.source) == "actors/character/behaviors/furniturefishingbehavior.hkx"
        {
            contracts.entry(route.project.clone()).or_default().push(
                graphs
                    .remove(&route.destination)
                    .ok_or("route has no direct graph")?,
            );
        }
        for origin in route.animations.values() {
            if let Some(retarget) = &bow_retarget {
                let destination = animation_destination(origin);
                if written.insert(destination.clone()) {
                    let mut animation = if retarget.is_identity() {
                        inputs.convert_animation(&source, origin)?
                    } else {
                        let mut animation = inputs.read_animation(&source.join(origin))?;
                        retarget
                            .apply(&mut animation)
                            .map_err(|e| format!("{origin}: {e}"))?;
                        animation
                    };
                    animation.set_contents_version(VERSION);
                    inputs.write(&output, &destination, &animation)?;
                }
            } else {
                copy_animation(&source, &output, project, origin, &mut written, &mut inputs)?;
            }
        }
    }
    for (key, project) in &plan.projects {
        let graphs = contracts.get(key).map(Vec::as_slice).unwrap_or_default();
        if project.vanilla {
            if graphs.is_empty() {
                continue;
            }
            let order = if !project.source.contains("_1stperson") {
                Some(inputs.bone_order(&source, &target, &project.source)?)
            } else {
                None
            };
            for relative in project_roots(&target, &project.source)? {
                let path = target.join(&relative);
                let mut root = inputs.read(&path)?;
                for graph in graphs {
                    merge_behavior_contracts(&mut root, graph)?;
                }
                let destination = relative.replacen(&project.source, &project.destination, 1);
                inputs.write(&output, &destination, &root)?;
                written.insert(destination);
            }
            for entry in std::fs::read_dir(target.join(&project.source).join("characters"))
                .map_err(|e| e.to_string())?
            {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path
                    .extension()
                    .is_none_or(|e| !e.eq_ignore_ascii_case("hkx"))
                {
                    continue;
                }
                let relative = format!(
                    "{}/characters/{}",
                    project.source,
                    path.file_name().unwrap().to_string_lossy()
                );
                if !source.join(&relative).is_file() {
                    continue;
                }
                let mut donor = inputs.convert(&source, &relative)?;
                if let Some(order) = &order {
                    remap_masks(&mut donor, order);
                }
                let mut character = inputs.read(&path)?;
                append_property_values(&mut character, &donor, order.as_deref())?;
                inputs.write(&output, &relative, &character)?;
                written.insert(relative);
            }
        } else {
            let source_dir = project.source.rsplit_once('/').unwrap().0;
            let project_file = inputs.convert(&source, &project.source)?;
            let project_destination = format!(
                "{}/{}",
                project.destination,
                project.source.rsplit('/').next().unwrap()
            );
            inputs.write(&output, &project_destination, &project_file)?;
            written.insert(project_destination);
            let strings = project_file
                .objects()
                .iter()
                .find(|o| o.class_name == "hkbProjectStringData")
                .ok_or("missing project string data")?;
            for name in array(strings, "characterFilenames").iter().filter_map(text) {
                let relative = relative_asset(source_dir, name);
                let original = inputs.read(&source.join(&relative))?;
                let mut character = inputs.convert(&source, &relative)?;
                restore_character_values(&original, &mut character);
                let strings = character
                    .objects_mut()
                    .iter_mut()
                    .find(|o| o.class_name == "hkbCharacterStringData")
                    .ok_or("missing character string data")?;
                let root_name = string(strings, "behaviorFilename");
                let rig_name = original
                    .objects()
                    .iter()
                    .find(|o| o.class_name == "hkbCharacterStringData")
                    .map(|o| string(o, "rigName"))
                    .ok_or("missing source rig name")?;
                let rig_source = relative_asset(source_dir, &rig_name);
                if !source.join(&rig_source).is_file() {
                    return Err(format!("missing source rig {rig_source}"));
                }
                for name in ["rigName", "ragdollName"] {
                    set_string(strings, name, "CharacterAssets\\skeleton.hkx");
                }
                let mut skins = array(strings, "skinNames");
                for skin in &mut skins {
                    if let Some(fields) = skin.as_object_members_mut() {
                        if let Some(filename) = fields.iter_mut().find(|m| m.name == "fileName") {
                            filename.value = HkxValue::String {
                                value: "CharacterAssets\\skeleton.hkx".into(),
                                is_null: false,
                            };
                        }
                    }
                }
                set(strings, "skinNames", HkxValue::Array(skins));
                let rig_destination =
                    format!("{}/characterassets/skeleton.hkx", project.destination);
                let rig = inputs.convert(&source, &rig_source)?;
                inputs.write(&output, &rig_destination, &rig)?;
                written.insert(rig_destination);
                let root_source = relative_asset(source_dir, &root_name);
                let root_dependencies = inputs.dependencies(&source, &root_source)?;
                for root_source in root_dependencies {
                    let mut root = inputs.convert(&source, &root_source)?;
                    for graph in graphs {
                        merge_behavior_contracts(&mut root, graph)?;
                    }
                    for object in root.objects_mut() {
                        if object.class_name == "hkbClipGenerator" {
                            let name = string(object, "animationName");
                            if let Some(origin) =
                                resolve_animation(&source, &root_source, &name, &[])
                            {
                                copy_animation(
                                    &source,
                                    &output,
                                    project,
                                    &origin,
                                    &mut written,
                                    &mut inputs,
                                )?;
                                set_string(
                                    object,
                                    "animationName",
                                    &relative_path(
                                        &project.destination,
                                        &animation_path(project, &origin),
                                    ),
                                );
                            }
                        } else if object.class_name == "hkbBehaviorReferenceGenerator" {
                            let dependency = relative_asset(
                                &project_dir(&root_source),
                                &string(object, "behaviorName"),
                            );
                            set_string(
                                object,
                                "behaviorName",
                                &format!("Behaviors\\{}", dependency.rsplit('/').next().unwrap()),
                            );
                        }
                    }
                    let root_destination = format!(
                        "{}/behaviors/{}",
                        project.destination,
                        root_source.rsplit('/').next().unwrap()
                    );
                    inputs.write(&output, &root_destination, &root)?;
                    written.insert(root_destination);
                }
                let destination = format!("{}/{}", project.destination, clean(name));
                inputs.write(&output, &destination, &character)?;
                written.insert(destination);
            }
        }
    }
    let report = serde_json::json!({"routes":plan.routes.len(),"projects":plan.projects.len(),"files":written});
    let manifest_started = Instant::now();
    std::fs::write(
        mod_path.join("debug/fo76_behaviors/assets.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    inputs.writes += manifest_started.elapsed();
    let elapsed = started.elapsed();
    Ok(BehaviorGenerationReport {
        assets_written: report["files"].as_array().unwrap().len() as u32,
        discovery,
        parsing: inputs.parsing,
        route_construction: elapsed
            .saturating_sub(discovery)
            .saturating_sub(inputs.parsing)
            .saturating_sub(inputs.writes),
        writes: inputs.writes,
        parsed_inputs: inputs.parsed_inputs,
        shared_input_cache_hits: inputs.shared_input_cache_hits,
        bone_order_cache_hits: inputs.bone_order_cache_hits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn converted_fixture(root: &Path) -> (PathBuf, String) {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let relative = "actors/test/characterassets/ragdoll.hkx".to_string();
        let source = root.join("source");
        let path = source.join(&relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(
            repo.join("py_creation_lib/native/havok/tests/fixtures/fo76_antiairturret_ragdoll.hkx"),
            &path,
        )
        .unwrap();
        (source, relative)
    }

    #[test]
    fn animation_inputs_stay_out_of_shared_caches_unless_reused() {
        {
            let temp = tempfile::tempdir().unwrap();
            let (source, origin) = converted_fixture(temp.path());
            let output = temp.path().join("output");
            let mut inputs = ParsedInputs::new(true, false);
            let mut written = BTreeSet::new();
            let vanilla = Project {
                source: "actors/test".into(),
                destination: "actors/test".into(),
                vanilla: true,
            };
            let custom = Project {
                source: "actors/test".into(),
                destination: "actors/test".into(),
                vanilla: false,
            };

            copy_animation(
                &source,
                &output,
                &vanilla,
                &origin,
                &mut written,
                &mut inputs,
            )
            .unwrap();
            copy_animation(
                &source,
                &output,
                &custom,
                &origin,
                &mut written,
                &mut inputs,
            )
            .unwrap();

            assert_eq!(written.len(), 2);
            assert!(inputs.shared_converted.is_empty());
            assert!(inputs.parsed.is_empty());
            assert_eq!(inputs.shared_input_cache_hits, 0);

            let parsed_inputs = inputs.parsed_inputs;
            copy_animation(
                &source,
                &output,
                &vanilla,
                &origin,
                &mut written,
                &mut inputs,
            )
            .unwrap();
            copy_animation(
                &source,
                &output,
                &custom,
                &origin,
                &mut written,
                &mut inputs,
            )
            .unwrap();
            assert_eq!(inputs.parsed_inputs, parsed_inputs);
            assert_eq!(written.len(), 2);
        }
        {
            let temp = tempfile::tempdir().unwrap();
            let (source, origin) = converted_fixture(temp.path());
            let mut inputs = ParsedInputs::new(true, false);

            let first = inputs.convert(&source, &origin).unwrap();
            let second = inputs.convert(&source, &origin).unwrap();

            assert_eq!(first.save(), second.save());
            assert_eq!(inputs.shared_converted.len(), 1);
            assert_eq!(inputs.shared_input_cache_hits, 1);
        }
    }

    fn character_properties(properties: &[(&str, i32)]) -> HkxFile {
        let mut objects: Vec<_> = [
            "hkbCharacterData",
            "hkbCharacterStringData",
            "hkbVariableValueSet",
        ]
        .into_iter()
        .map(|class| HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class.into(),
            members: vec![],
        })
        .collect();
        set(&mut objects[0], "stringData", HkxValue::Pointer(Some(1)));
        set(
            &mut objects[0],
            "characterPropertyValues",
            HkxValue::Pointer(Some(2)),
        );
        set(
            &mut objects[0],
            "characterPropertyInfos",
            HkxValue::Array(
                properties
                    .iter()
                    .map(|_| {
                        HkxValue::Object(vec![HkxMember {
                            name: "type".into(),
                            value: HkxValue::I8(3),
                        }])
                    })
                    .collect(),
            ),
        );
        set(
            &mut objects[1],
            "characterPropertyNames",
            HkxValue::Array(
                properties
                    .iter()
                    .map(|(name, _)| HkxValue::String {
                        value: (*name).into(),
                        is_null: false,
                    })
                    .collect(),
            ),
        );
        set(
            &mut objects[2],
            "wordVariableValues",
            HkxValue::Array(
                properties
                    .iter()
                    .map(|(_, value)| {
                        HkxValue::Object(vec![HkxMember {
                            name: "value".into(),
                            value: HkxValue::I32(*value),
                        }])
                    })
                    .collect(),
            ),
        );
        set(
            &mut objects[2],
            "variantVariableValues",
            HkxValue::Array(vec![]),
        );
        HkxFile::from_tagxml(11, VERSION, objects)
    }

    #[test]
    fn aim_sources_and_mirrored_grips_use_target_weapon_bone() {
        {
            let source = character_properties(&[
                ("DirectAtWeaponBoneIndex", 99),
                ("AimSource", 12),
                ("Other", 7),
            ]);
            for weapon_bone in [28, 41] {
                let mut target = character_properties(&[
                    ("DirectAtWeaponBoneIndex", weapon_bone),
                    ("Existing", 4),
                ]);
                let expected = character_properties(&[
                    ("DirectAtWeaponBoneIndex", weapon_bone),
                    ("Existing", 4),
                    ("AimSource", weapon_bone),
                    ("Other", 7),
                ]);
                for _ in 0..2 {
                    append_property_values(&mut target, &source, None).unwrap();
                    for (index, member) in [
                        (0, "characterPropertyInfos"),
                        (1, "characterPropertyNames"),
                        (2, "wordVariableValues"),
                    ] {
                        assert_eq!(
                            array(&target.objects()[index], member),
                            array(&expected.objects()[index], member)
                        );
                    }
                }
            }
        }
        {
            let source = character_properties(&[("AimSource", 12)]);
            for properties in [
                vec![("AimSource", 15), ("DirectAtWeaponBoneIndex", 28)],
                vec![("AimSource", 12)],
            ] {
                let mut target = character_properties(&properties);
                let before = array(&target.objects()[2], "wordVariableValues");
                append_property_values(&mut target, &source, None).unwrap();
                assert_eq!(array(&target.objects()[2], "wordVariableValues"), before);
            }
            let mut target = character_properties(&[]);
            append_property_values(&mut target, &source, None).unwrap();
            assert_eq!(
                array(&target.objects()[2], "wordVariableValues"),
                array(&source.objects()[2], "wordVariableValues")
            );

            let mut creature = character_properties(&[("AimSource", 28)]);
            restore_character_values(&source, &mut creature);
            assert_eq!(
                array(&creature.objects()[2], "wordVariableValues"),
                array(&source.objects()[2], "wordVariableValues")
            );
        }
        {
            let source = character_properties(&[
                ("DirectAtWeaponBoneIndex", 29),
                ("WeaponGripMirroredBoneIndex", 32),
                ("Other", 32),
            ]);
            let mut order = vec![None; 95];
            order[28] = Some(29);
            order[84] = Some(32);
            for existing in [
                vec![("DirectAtWeaponBoneIndex", 28)],
                vec![
                    ("DirectAtWeaponBoneIndex", 28),
                    ("WeaponGripMirroredBoneIndex", 85),
                ],
            ] {
                let expected_grip = if existing.len() == 1 { 84 } else { 85 };
                let mut target = character_properties(&existing);
                for _ in 0..2 {
                    append_property_values(&mut target, &source, Some(&order)).unwrap();
                    let expected = character_properties(&[
                        ("DirectAtWeaponBoneIndex", 28),
                        ("WeaponGripMirroredBoneIndex", expected_grip),
                        ("Other", 32),
                    ]);
                    assert_eq!(
                        array(&target.objects()[2], "wordVariableValues"),
                        array(&expected.objects()[2], "wordVariableValues")
                    );
                }
            }
            let mut unchanged = character_properties(&[]);
            append_property_values(&mut unchanged, &source, None).unwrap();
            assert_eq!(
                array(&unchanged.objects()[2], "wordVariableValues"),
                array(&source.objects()[2], "wordVariableValues")
            );
        }
    }

    #[test]
    fn character_masks_import_remapped_weights_and_keep_target_values() {
        {
            let mut object = HkxObject {
                name: None,
                offset: 0,
                signature: 0,
                class_name: "hkbBoneWeightArray".into(),
                members: vec![],
            };
            set(
                &mut object,
                "boneWeights",
                HkxValue::Array(vec![HkxValue::F32(0.25), HkxValue::F32(1.0)]),
            );
            let mut file = HkxFile::from_tagxml(11, VERSION, vec![object]);
            remap_masks(&mut file, &[Some(1), None, Some(0)]);
            let packed = HkxFile::read(&file.save()).unwrap();
            let weights: Vec<_> = array(&packed.objects()[0], "boneWeights")
                .iter()
                .filter_map(number)
                .collect();
            assert_eq!(weights, [1.0, 0.0, 0.25]);
        }
        {
            let mut source = masked_character(vec![0.25, 1.0]);
            let mut target = masked_character(vec![]);
            // Only the first property is supplied by this source, but both target properties share a mask.
            for (index, member) in [
                (0, "characterPropertyInfos"),
                (1, "characterPropertyNames"),
                (2, "wordVariableValues"),
            ] {
                let values = array(&source.objects()[index], member);
                set(
                    &mut source.objects_mut()[index],
                    member,
                    HkxValue::Array(values[..1].to_vec()),
                );
            }
            remap_masks(&mut source, &[Some(1), None, Some(0)]);
            append_property_values(&mut target, &source, None).unwrap();
            assert_eq!(target.objects().len(), 5);
            assert_eq!(
                array(&target.objects()[4], "boneWeights"),
                vec![HkxValue::F32(1.0), HkxValue::F32(0.0), HkxValue::F32(0.25)]
            );
            assert!(array(&target.objects()[3], "boneWeights").is_empty());
            let expected =
                character_properties(&[("UpperBodyFeatheredSpine00", 1), ("OtherMask", 0)]);
            assert_eq!(
                array(&target.objects()[2], "wordVariableValues"),
                array(&expected.objects()[2], "wordVariableValues")
            );
            let once = target.save();
            append_property_values(&mut target, &source, None).unwrap();
            assert_eq!(target.save(), once);
        }
        {
            let source = masked_character(vec![1.0, 0.5]);
            for mut target in [
                masked_character(vec![0.0, 0.0]),
                character_properties(&[("UpperBodyFeatheredSpine00", 0), ("OtherMask", 0)]),
            ] {
                let before = target.save();
                append_property_values(&mut target, &source, None).unwrap();
                assert_eq!(target.save(), before);
            }
            let mut target = masked_character(vec![]);
            let before = target.save();
            append_property_values(&mut target, &masked_character(vec![]), None).unwrap();
            assert_eq!(target.save(), before);
        }
    }

    fn masked_character(weights: Vec<f32>) -> HkxFile {
        let mut file = character_properties(&[("UpperBodyFeatheredSpine00", 0), ("OtherMask", 0)]);
        let mut infos = array(&file.objects()[0], "characterPropertyInfos");
        for info in &mut infos {
            info.as_object_members_mut().unwrap()[0].value = HkxValue::I8(5);
        }
        set(
            &mut file.objects_mut()[0],
            "characterPropertyInfos",
            HkxValue::Array(infos),
        );
        let mask = HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: "hkbBoneWeightArray".into(),
            members: vec![HkxMember {
                name: "boneWeights".into(),
                value: HkxValue::F32List(weights),
            }],
        };
        let index = add(&mut file, mask);
        set(
            &mut file.objects_mut()[2],
            "variantVariableValues",
            HkxValue::Array(vec![HkxValue::Pointer(Some(index))]),
        );
        file
    }

    #[test]
    fn draw_namespace_keeps_event_indices_and_is_idempotent() {
        let mut strings = HkxObject {
            name: None,
            offset: 0,
            signature: 2,
            class_name: "hkbBehaviorGraphStringData".into(),
            members: vec![],
        };
        set(
            &mut strings,
            "eventNames",
            HkxValue::Array(
                ["idle", "WeapEquip", "WeaponFire"]
                    .into_iter()
                    .map(|s| HkxValue::String {
                        value: s.into(),
                        is_null: false,
                    })
                    .collect(),
            ),
        );
        let mut file = HkxFile::from_tagxml(11, VERSION, vec![strings]);
        assert!(namespace_draw(&mut file, "B21_route_weapEquip"));
        assert!(!namespace_draw(&mut file, "B21_route_weapEquip"));
        assert_eq!(
            array(&file.objects()[0], "eventNames")
                .iter()
                .filter_map(text)
                .collect::<Vec<_>>(),
            ["idle", "B21_route_weapEquip", "WeaponFire"]
        );
    }
}
