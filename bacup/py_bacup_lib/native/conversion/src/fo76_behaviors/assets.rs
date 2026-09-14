use super::*;
use crate::fixups::havok::normalize_weapon_behavior_contracts::merge_behavior_contracts;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

fn convert(path: &Path) -> Result<HkxFile, String> {
    let report = havok_native::api::havok_convert_bytes_report(
        &std::fs::read(path).map_err(|e| e.to_string())?,
        VERSION,
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    HkxFile::read(&report.bytes).map_err(|e| e.to_string())
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
    fn new(reuse_shared_inputs: bool) -> Self {
        Self {
            reuse_shared_inputs,
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
fn project_rig(root: &Path, dir: &str) -> Result<HkxFile, String> {
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
        if target_names.iter().any(|n| {
            text(n)
                .zip(text(name))
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        }) {
            continue;
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
        target_names.push(name.clone());
        target_infos.push(infos[i].clone());
        target_words.push(word);
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
        inputs.convert(source, origin)?
    } else {
        inputs.read(&source.join(origin))?
    };
    animation.set_contents_version(VERSION);
    remove_shredder_swing_events(&mut animation, origin);
    inputs.write(output, &destination, &animation)
}

pub fn build(mod_path: &Path, source_root: &Path, target_root: &Path) -> Result<u32, String> {
    Ok(build_profiled(mod_path, source_root, target_root)?.assets_written)
}

pub fn build_profiled(
    mod_path: &Path,
    source_root: &Path,
    target_root: &Path,
) -> Result<BehaviorGenerationReport, String> {
    build_with_profile(mod_path, source_root, target_root, true)
}

fn build_with_profile(
    mod_path: &Path,
    source_root: &Path,
    target_root: &Path,
    reuse_shared_inputs: bool,
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
    let mut inputs = ParsedInputs::new(reuse_shared_inputs);
    let vanilla_blends =
        inputs.read(&target.join("actors/character/behaviors/weaponbehavior.hkx"))?;
    let mut written = BTreeSet::new();
    let mut contracts: BTreeMap<String, Vec<HkxFile>> = BTreeMap::new();
    for route in &plan.routes {
        let project = &plan.projects[&route.project];
        let order = if project.vanilla && !project.source.contains("_1stperson") {
            Some(inputs.bone_order(&source, &target, &project.source)?)
        } else {
            None
        };
        let mut graphs = BTreeMap::new();
        for (origin, destination) in &route.dependencies {
            let mut graph = inputs.route_convert(&source, origin)?;
            super::locomotion::repair(
                &mut graph,
                &source,
                &route.animations,
                origin,
                &vanilla_blends,
            )?;
            super::bow::repair_player_locomotion(&mut graph, route);
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
                            &relative_path(&project.destination, &animation_path(project, &origin)),
                        );
                    }
                    _ => {}
                }
            }
            super::furniture::repair_gas_pump_exit(&mut graph, origin, &target)?;
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
        if !project.vanilla || !furniture_graph(&route.source) {
            contracts.entry(route.project.clone()).or_default().push(
                graphs
                    .remove(&route.destination)
                    .ok_or("route has no direct graph")?,
            );
        }
        for origin in route.animations.values() {
            copy_animation(&source, &output, project, origin, &mut written, &mut inputs)?;
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
            let behavior_dir = target.join(&project.source).join("behaviors");
            for entry in std::fs::read_dir(behavior_dir).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                if !name.to_ascii_lowercase().contains("root")
                    || !name.to_ascii_lowercase().ends_with(".hkx")
                {
                    continue;
                }
                let mut root = inputs.read(&path)?;
                for graph in graphs {
                    merge_behavior_contracts(&mut root, graph)?;
                }
                let destination = format!("{}/behaviors/{name}", project.destination);
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

    fn output_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(root, &path, files);
                } else {
                    files.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                        std::fs::read(path).unwrap(),
                    );
                }
            }
        }

        let mut files = BTreeMap::new();
        collect(root, &root.join("data/Meshes"), &mut files);
        files.insert(
            "debug/fo76_behaviors/assets.json".into(),
            std::fs::read(root.join("debug/fo76_behaviors/assets.json")).unwrap(),
        );
        files
    }

    #[test]
    #[ignore = "requires fixed scratch trees, a behavior plan, and extracted FO76/FO4 assets"]
    fn benchmark_shared_behavior_inputs_preserve_outputs() {
        let source = PathBuf::from(std::env::var("B21_FO76_BEHAVIOR_BENCH_SOURCE").unwrap());
        let target = PathBuf::from(std::env::var("B21_FO76_BEHAVIOR_BENCH_TARGET").unwrap());
        let plan = PathBuf::from(std::env::var("B21_FO76_BEHAVIOR_BENCH_PLAN").unwrap());
        let baseline = PathBuf::from(std::env::var("B21_FO76_BEHAVIOR_BENCH_BASELINE").unwrap());
        let optimized = PathBuf::from(std::env::var("B21_FO76_BEHAVIOR_BENCH_OPTIMIZED").unwrap());
        let reverse_order = std::env::var_os("B21_FO76_BEHAVIOR_BENCH_REVERSE").is_some();
        for root in [&baseline, &optimized] {
            assert!(
                !root.join("data").exists(),
                "benchmark scratch must not already contain generated data: {}",
                root.display()
            );
            std::fs::create_dir_all(root.join("debug/fo76_behaviors")).unwrap();
            std::fs::copy(&plan, root.join(PLAN_PATH)).unwrap();
        }

        let (baseline_report, optimized_report) = if reverse_order {
            let optimized_report = build_profiled(&optimized, &source, &target).unwrap();
            let baseline_report = build_with_profile(&baseline, &source, &target, false).unwrap();
            (baseline_report, optimized_report)
        } else {
            let baseline_report = build_with_profile(&baseline, &source, &target, false).unwrap();
            let optimized_report = build_profiled(&optimized, &source, &target).unwrap();
            (baseline_report, optimized_report)
        };
        let baseline_bytes = output_bytes(&baseline);
        let optimized_bytes = output_bytes(&optimized);
        println!(
            "[fo76_behavior_generation_benchmark] reverse_order={} baseline_ms={} optimized_ms={} baseline_parse_ms={} optimized_parse_ms={} baseline_route_ms={} optimized_route_ms={} baseline_write_ms={} optimized_write_ms={} optimized_parsed_inputs={} optimized_shared_input_cache_hits={} optimized_bone_order_cache_hits={} files={}",
            reverse_order,
            baseline_report.discovery.as_millis()
                + baseline_report.parsing.as_millis()
                + baseline_report.route_construction.as_millis()
                + baseline_report.writes.as_millis(),
            optimized_report.discovery.as_millis()
                + optimized_report.parsing.as_millis()
                + optimized_report.route_construction.as_millis()
                + optimized_report.writes.as_millis(),
            baseline_report.parsing.as_millis(),
            optimized_report.parsing.as_millis(),
            baseline_report.route_construction.as_millis(),
            optimized_report.route_construction.as_millis(),
            baseline_report.writes.as_millis(),
            optimized_report.writes.as_millis(),
            optimized_report.parsed_inputs,
            optimized_report.shared_input_cache_hits,
            optimized_report.bone_order_cache_hits,
            optimized_bytes.len(),
        );
        assert_eq!(baseline_report.shared_input_cache_hits, 0);
        assert_eq!(baseline_report.bone_order_cache_hits, 0);
        assert!(optimized_report.shared_input_cache_hits > 0);
        assert!(optimized_report.bone_order_cache_hits > 0);
        assert_eq!(baseline_bytes, optimized_bytes);
        assert_eq!(
            baseline_report.assets_written,
            optimized_report.assets_written
        );
    }

    #[test]
    #[ignore = "requires the extracted FO76 animation fixtures"]
    fn live_shredder_swing_events_preserve_damage_and_motion() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        for name in ["wpnmeleeshredder.hkx", "wpnmeleeshredder_additive.hkx"] {
            let origin = format!("actors/character/animations/weapon/chainsaw/{name}");
            let original = convert(&repo.join("extracted/fo76/meshes").join(&origin)).unwrap();
            let mut patched = original.clone();
            remove_shredder_swing_events(&mut patched, &origin);
            let events = |file: &HkxFile| {
                file.objects()
                    .iter()
                    .flat_map(|object| array(object, "annotationTracks"))
                    .filter_map(|track| track.as_object_members().map(|fields| fields.to_vec()))
                    .flat_map(|fields| {
                        fields
                            .into_iter()
                            .filter_map(|field| {
                                if field.name == "annotations" {
                                    if let HkxValue::Array(events) = field.value {
                                        return Some(events);
                                    }
                                }
                                None
                            })
                            .flatten()
                    })
                    .collect::<Vec<_>>()
            };
            let mut expected = events(&original);
            expected.retain(|event| {
                !event
                    .as_object_members()
                    .unwrap()
                    .iter()
                    .any(|field| field.name == "text" && text(&field.value) == Some("weaponSwing"))
            });
            assert_eq!(events(&original).len(), expected.len() + 1);
            let roundtrip = HkxFile::read(&patched.save()).unwrap();
            assert_eq!(events(&roundtrip), expected);
            let stripped = |mut file: HkxFile| {
                for object in file.objects_mut() {
                    object
                        .members
                        .retain(|member| member.name != "annotationTracks");
                }
                file.save()
            };
            assert_eq!(stripped(original.clone()), stripped(roundtrip));
            let once = patched.save();
            remove_shredder_swing_events(&mut patched, &origin);
            assert_eq!(once, patched.save());
            let mut unrelated = original.clone();
            remove_shredder_swing_events(&mut unrelated, "actors/character/animations/attack.hkx");
            assert_eq!(original.save(), unrelated.save());
            let destination = repo.join("tmp/autoaxe-voice/native").join(name);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::write(destination, patched.save()).unwrap();
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
        HkxFile::from_tagxml(11, VERSION, objects)
    }

    #[test]
    fn imported_aim_source_uses_target_weapon_bone_and_preserves_properties() {
        let source = character_properties(&[
            ("DirectAtWeaponBoneIndex", 99),
            ("AimSource", 12),
            ("Other", 7),
        ]);
        for weapon_bone in [28, 41] {
            let mut target =
                character_properties(&[("DirectAtWeaponBoneIndex", weapon_bone), ("Existing", 4)]);
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

    #[test]
    fn existing_aim_source_and_characters_without_target_weapon_bone_are_preserved() {
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

    #[test]
    fn imported_mirrored_grip_uses_target_bone_order() {
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

    #[test]
    fn remapped_human_masks_survive_packfile_round_trip() {
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

    #[test]
    #[ignore = "requires extracted FO76/FO4 assets and the runtime fixture inventory"]
    fn build_converted_fo76_fixture_routes() {
        use crate::fixups::face::build_additive_race_record::SubgraphBlock;
        use crate::sym::StringInterner;
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .unwrap();
        let source = repo.join("extracted/fo76/meshes");
        let inventory: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                repo.join("mods/B21_FO76HumanoidBehaviors/testing/generated/asset-plan.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let interner = StringInterner::new();
        let mut plan = Plan::default();
        let mut cache = BTreeMap::new();
        for race in ["moleminer", "scorched"] {
            let project = Project {
                source: format!("actors/{race}/{race}project.hkx"),
                destination: format!("actors/B21_FO76/{race}"),
                vanilla: false,
            };
            for route in inventory[race]["routes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| combat_graph(r["graph"].as_str().unwrap()))
            {
                let block = SubgraphBlock {
                    behaviour_graph: interner.intern(route["graph"].as_str().unwrap()),
                    paths: route["paths"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| interner.intern(s.as_str().unwrap()))
                        .collect(),
                    subgraph_keywords: vec![],
                    target_keywords: vec![],
                    flags_bytes: None,
                };
                plan.routes.push(
                    super::super::records::make_route(
                        &source,
                        route["graph"].as_str().unwrap(),
                        &block,
                        &project,
                        &interner,
                        &mut cache,
                    )
                    .unwrap(),
                );
            }
            plan.projects.insert(project.destination.clone(), project);
        }
        let human_paths: serde_json::Value =
            serde_json::from_slice(
                &std::fs::read(repo.join(
                    "mods/B21_FO76BinocularsTest/testing/generated/source-animation-paths.json",
                ))
                .unwrap(),
            )
            .unwrap();
        for (perspective, dir, graph) in [
            ("3P", "actors/character", "binocularbehavior.hkx"),
            ("1P", "actors/character/_1stperson", "gunbehavior.hkx"),
        ] {
            let project = Project {
                source: dir.into(),
                destination: dir.into(),
                vanilla: true,
            };
            let source_graph = format!("{dir}/behaviors/{graph}");
            let block = SubgraphBlock {
                behaviour_graph: interner.intern(&source_graph),
                paths: human_paths[perspective]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| interner.intern(s.as_str().unwrap()))
                    .collect(),
                subgraph_keywords: vec![],
                target_keywords: vec![],
                flags_bytes: None,
            };
            plan.routes.push(
                super::super::records::make_route(
                    &source,
                    &source_graph,
                    &block,
                    &project,
                    &interner,
                    &mut cache,
                )
                .unwrap(),
            );
            plan.projects.insert(project.destination.clone(), project);
        }
        let floater_project = "actors/B21_FO76/floater";
        plan.projects.insert(
            floater_project.into(),
            Project {
                source: "actors/floater/floaterproject.hkx".into(),
                destination: floater_project.into(),
                vanilla: false,
            },
        );
        let temporary = tempfile::Builder::new()
            .prefix("fo76_behavior_pipeline_")
            .tempdir_in(repo.join("tmp"))
            .unwrap();
        let output = temporary.path();
        std::fs::create_dir_all(output.join("debug/fo76_behaviors")).unwrap();
        std::fs::write(
            output.join(PLAN_PATH),
            serde_json::to_vec_pretty(&plan).unwrap(),
        )
        .unwrap();
        use crate::phase::{DispatchParams, run_phase};
        use crate::run::{RunConfig, RunParams, create_run, drop_run};
        use crate::translator::Game;
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();
        let postprocess = || {
            run_phase(
                id,
                "postprocess_havok_assets",
                DispatchParams {
                    mod_path: output.to_path_buf(),
                    source_extracted_dir: repo.join("extracted/fo76"),
                    target_extracted_dir: Some(repo.join("extracted/fo4")),
                    target_data_dir: None,
                    params: serde_json::json!({}),
                },
            )
            .unwrap()
        };
        assert!(postprocess().assets_written > 100);
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(output.join("debug/fo76_behaviors/assets.json")).unwrap(),
        )
        .unwrap();
        let files: Vec<_> = receipt["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file.as_str().unwrap().to_owned())
            .collect();
        assert!(
            files
                .iter()
                .any(|file| file == "actors/B21_FO76/floater/characters/floatercharacter.hkx")
        );
        assert!(!files.iter().any(|file| file.ends_with(" .hkx")));
        let hashes = || {
            files
                .iter()
                .map(|file| {
                    use std::hash::{Hash, Hasher};
                    let bytes = std::fs::read(output.join("data/Meshes").join(file)).unwrap();
                    HkxFile::read(&bytes).unwrap();
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    bytes.hash(&mut hash);
                    (file.clone(), hash.finish())
                })
                .collect::<BTreeMap<_, _>>()
        };
        let first_hashes = hashes();
        use crate::fixups::havok::{
            inject_attack_stop_events, inject_hitframe_events, sanitize_ragdoll_contact_bones,
        };
        assert!(
            inject_hitframe_events::inject_hitframe_events_in_mod_path(output)
                .unwrap()
                .is_no_op()
        );
        assert!(
            inject_attack_stop_events::inject_attack_stop_events_in_mod_path(output)
                .unwrap()
                .is_no_op()
        );
        assert!(
            sanitize_ragdoll_contact_bones::sanitize_ragdoll_contact_bones_in_mod_path(output)
                .unwrap()
                .is_no_op()
        );
        for route in &plan.routes {
            let graph = read(&output.join("data/Meshes").join(&route.destination)).unwrap();
            assert!(
                graph
                    .objects()
                    .iter()
                    .all(|o| o.class_name != "BSAssignBoneWeightsModifier"
                        && o.class_name != "BSLocomotionBlendGenerator")
            );
            let graph_name = graph
                .objects()
                .iter()
                .find(|o| o.class_name == "hkbBehaviorGraph")
                .map(|o| string(o, "name"))
                .unwrap();
            assert_eq!(
                graph_name,
                format!(
                    "{}.hkb",
                    Path::new(&route.destination)
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                )
            );
            if let Some(event) = &route.draw_event {
                assert!(
                    graph
                        .objects()
                        .iter()
                        .filter(|o| o.class_name == "hkbBehaviorGraphStringData")
                        .any(|o| array(o, "eventNames")
                            .iter()
                            .any(|s| text(s) == Some(event)))
                );
            }
        }
        let mesh_root = output.join("data/Meshes");
        let inputs = ck_native::anim_text_data::emit::AnimTextDataInputs {
            target_plugin_name: "B21_FO76BehaviorPipelineTest.esp".into(),
            subgraphs: plan
                .routes
                .iter()
                .map(|r| ck_native::anim_text_data::emit::SubgraphInput {
                    core_behavior: r.destination.clone(),
                    sapt_chain: vec![],
                    race_dir: Some(r.project.clone()),
                })
                .collect(),
            ..Default::default()
        };
        let metadata = ck_native::anim_text_data::emit::generate_anim_text_data(
            &inputs,
            &mesh_root,
            &mesh_root,
            Some(&repo.join("extracted/fo4/meshes")),
            Some("B21"),
        )
        .unwrap();
        assert!(metadata.written > 0);
        assert!(metadata.stance_builder_error.is_none());
        std::fs::write(
            output.join("data/Meshes").join(&plan.routes[0].destination),
            b"stale output",
        )
        .unwrap();
        assert!(postprocess().assets_written > 100);
        assert_eq!(first_hashes, hashes());
        let second_receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(output.join("debug/fo76_behaviors/assets.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt, second_receipt);
        drop_run(id).unwrap();
    }
}
