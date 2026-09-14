use super::*;
use crate::fixups::face::build_additive_race_record::{
    SubgraphBlock, block_to_entries, parse_canonical_subgraphs,
};
use crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub struct PreserveFo76BehaviorsFixup;

fn error(e: impl std::fmt::Display) -> FixupError {
    FixupError::Other(e.to_string())
}
fn field(record: &Record, sig: &str, interner: &StringInterner) -> String {
    record
        .fields
        .iter()
        .find_map(|f| match &f.value {
            FieldValue::String(s) if f.sig.as_str() == sig => {
                interner.resolve(*s).map(str::to_string)
            }
            _ => None,
        })
        .unwrap_or_default()
}
fn put(record: &mut Record, sig: &str, value: &str, interner: &StringInterner) {
    let sig = SubrecordSig::from_str(sig).unwrap();
    let value = FieldValue::String(interner.intern(value));
    if let Some(f) = record.fields.iter_mut().find(|f| f.sig == sig) {
        f.value = value
    } else {
        record.fields.push(FieldEntry { sig, value });
    }
}
fn project_paths(value: &FieldValue, interner: &StringInterner, paths: &mut Vec<String>) {
    match value {
        FieldValue::String(s) => {
            if let Some(s) = interner.resolve(*s) {
                let path = clean(s);
                if path.starts_with("actors/")
                    && path.ends_with(".hkx")
                    && !path.contains("/behaviors/")
                    && !path.contains("/animations/")
                {
                    paths.push(path)
                }
            }
        }
        FieldValue::Struct(fields) => {
            for (_, v) in fields {
                project_paths(v, interner, paths)
            }
        }
        FieldValue::List(values) => {
            for v in values {
                project_paths(v, interner, paths)
            }
        }
        _ => {}
    }
}

fn race_projects(record: &Record, interner: &StringInterner) -> Vec<String> {
    let mut projects = Vec::new();
    for f in &record.fields {
        if !matches!(f.sig.as_str(), "SGNM" | "SAPT") {
            project_paths(&f.value, interner, &mut projects);
        }
    }
    let mut seen = HashSet::new();
    projects.retain(|path| seen.insert(path.clone()));
    projects
}

fn select_creature_project(
    original: &Record,
    repaired: &Record,
    schema: &crate::schema::AuthoringSchema,
    source: &Path,
    interner: &StringInterner,
) -> Result<Option<String>, FixupError> {
    let projects = race_projects(original, interner);
    if projects.len() <= 1 {
        return Ok(projects.into_iter().next());
    }
    let repaired_projects = race_projects(repaired, interner);
    if let [project] = repaired_projects.as_slice() {
        // CatPet's source filename is stale; its existing repair names a real
        // source project in the same actor directory.
        if projects.contains(project)
            || (source.join(project).is_file()
                && projects.iter().any(|p| {
                    p.rsplit_once('/').map(|(dir, _)| dir)
                        == project.rsplit_once('/').map(|(dir, _)| dir)
                }))
        {
            return Ok(Some(project.clone()));
        }
    }
    let flags_offset = schema
        .struct_field_layout_versioned("RACE", "DATA", Some(FO4_TARGET_FORM_VERSION))
        .into_iter()
        .find(|field| field.field_id == "flags_2" && field.width == 4)
        .map(|field| field.offset);
    let ungendered = flags_offset.is_some_and(|offset| {
        repaired.fields.iter().any(|f| match &f.value {
            FieldValue::Bytes(bytes) if f.sig.as_str() == "DATA" => bytes
                .get(offset..offset + 4)
                .is_some_and(|flags| u32::from_le_bytes(flags.try_into().unwrap()) & (1 << 9) != 0),
            _ => false,
        })
    });
    if ungendered {
        // Ungendered races use the primary (male) slot. FO76 leaves unrelated
        // donor projects in the unused female slot, so retain authored order.
        return Ok(projects.into_iter().next());
    }
    Err(error(format!(
        "FO76 behavior port: race {:06X} ({}) needs separate gender projects: {projects:?}",
        repaired.form_key.local,
        repaired
            .eid
            .and_then(|eid| interner.resolve(eid))
            .unwrap_or_default(),
    )))
}

fn rewrite_strings(
    value: &mut FieldValue,
    mapping: &BTreeMap<String, String>,
    interner: &StringInterner,
) {
    match value {
        FieldValue::String(s) => {
            if let Some(new) = interner.resolve(*s).and_then(|s| mapping.get(&clean(s))) {
                *s = interner.intern(&new.replace('/', "\\"));
            }
        }
        FieldValue::Struct(fields) => {
            for (_, v) in fields {
                rewrite_strings(v, mapping, interner)
            }
        }
        FieldValue::List(values) => {
            for v in values {
                rewrite_strings(v, mapping, interner)
            }
        }
        _ => {}
    }
}

fn missing_source_cores(
    source: &Path,
    blocks: &[SubgraphBlock],
    interner: &StringInterner,
) -> Result<Vec<String>, FixupError> {
    let mut missing = Vec::new();
    for block in blocks {
        let graph = clean(interner.resolve(block.behaviour_graph).unwrap_or_default());
        if preserved_graph(&graph)
            && !graph.contains("b21_fo76")
            && !source.join(&graph).try_exists().map_err(error)?
        {
            missing.push(graph);
        }
    }
    missing.sort();
    missing.dedup();
    Ok(missing)
}

fn uses_target_project(race: &Record, has_source: bool) -> bool {
    race.fields.iter().any(|f| match f.value {
        FieldValue::FormKey(parent) if f.sig.as_str() == "SADD" => {
            // Source creatures can inherit another converted creature's graphs.
            !has_source || parent.plugin != race.form_key.plugin
        }
        _ => false,
    })
}

fn target_project_directory(race: &Record, interner: &StringInterner) -> Option<String> {
    race_projects(race, interner)
        .first()?
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
}

fn perspective_project_directory(
    dir: &str,
    block: &SubgraphBlock,
    interner: &StringInterner,
) -> String {
    let first_person = block
        .flags_bytes
        .as_deref()
        .is_some_and(|flags| flags.get(2..4) == Some(&[1, 0]))
        || interner
            .resolve(block.behaviour_graph)
            .is_some_and(|g| clean(g).contains("/_1stperson/"));
    if first_person && !dir.ends_with("/_1stperson") {
        format!("{dir}/_1stperson")
    } else {
        dir.to_string()
    }
}

struct ActionTemplateIndex<'a> {
    records: Vec<&'a Record>,
    graphs: Vec<String>,
    by_dependency: HashMap<(String, bool), Vec<usize>>,
    by_graph: HashMap<String, Vec<usize>>,
}

impl<'a> ActionTemplateIndex<'a> {
    fn new(templates: &'a [Record], interner: &StringInterner) -> Self {
        let mut index = Self {
            records: Vec::with_capacity(templates.len()),
            graphs: Vec::with_capacity(templates.len()),
            by_dependency: HashMap::new(),
            by_graph: HashMap::new(),
        };
        for template in templates {
            let graph = clean(&field(template, "DNAM", interner));
            let position = index.records.len();
            index
                .by_dependency
                .entry(dependency_template_key(&graph))
                .or_default()
                .push(position);
            index
                .by_graph
                .entry(graph.clone())
                .or_default()
                .push(position);
            index.records.push(template);
            index.graphs.push(graph);
        }
        index
    }

    fn family_for_dependencies<'b>(
        &self,
        dependencies: impl Iterator<Item = &'b str>,
    ) -> Vec<&'a Record> {
        let mut positions = Vec::new();
        for dependency in dependencies {
            if let Some(matches) = self.by_dependency.get(&dependency_template_key(dependency)) {
                positions.extend(matches.iter().copied());
            }
        }
        positions.sort_unstable();
        positions.dedup();
        positions
            .into_iter()
            .map(|position| self.records[position])
            .collect()
    }

    fn root_graphs<'b>(&'b self, roots: &[String]) -> Vec<&'b str> {
        let mut positions: Vec<_> = roots
            .iter()
            .filter_map(|root| self.by_graph.get(root).and_then(|matches| matches.first()))
            .copied()
            .collect();
        positions.sort_unstable();
        positions.dedup();
        positions
            .into_iter()
            .map(|position| self.graphs[position].as_str())
            .collect()
    }

    fn family_for_graph(&self, graph: &str) -> Vec<&'a Record> {
        self.by_graph
            .get(graph)
            .into_iter()
            .flatten()
            .map(|position| self.records[*position])
            .collect()
    }
}

fn dependency_template_key(path: &str) -> (String, bool) {
    (
        path.rsplit('/').next().unwrap_or_default().to_string(),
        path.contains("_1stperson"),
    )
}

impl Fixup for PreserveFo76BehaviorsFixup {
    fn name(&self) -> &'static str {
        "preserve_fo76_behaviors"
    }
    fn uses_session(&self) -> bool {
        true
    }
    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        super::enabled(session, config)
    }
    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let source = meshes(config.source_extracted_dir.as_deref().unwrap());
        let schema = session.schema().map_err(error)?;
        let source_schema = session.source_schema().map_err(error)?;
        let interner = mapper.interner;
        let source_plugin = interner.intern(&session.source_slot_opt().unwrap().parsed.plugin_name);
        let grip_aliases: HashMap<_, _> = [0x01F948, 0x0464EF, 0x01F947, 0x0AA937]
            .into_iter()
            .filter_map(|local| {
                mapper
                    .lookup(FormKey {
                        plugin: source_plugin,
                        local,
                    })
                    .map(|mapped| {
                        (
                            mapped,
                            FormKey {
                                plugin: interner.intern("Fallout4.esm"),
                                local,
                            },
                        )
                    })
            })
            .collect();
        let race_sig = SigCode::from_str("RACE").unwrap();
        let mut sources = HashMap::new();
        for fk in session
            .source_form_keys_of_sig(race_sig, interner)
            .map_err(error)?
        {
            if let Some(target) = mapper.lookup(fk) {
                sources.insert(
                    target,
                    session
                        .source_record_decoded(&fk, &source_schema, interner)
                        .map_err(error)?,
                );
            }
        }
        let template_prepare_started = std::time::Instant::now();
        let templates = action_templates(session, mapper, config)?;
        let template_prepare_ms = template_prepare_started.elapsed().as_millis();
        let template_index_started = std::time::Instant::now();
        let template_index = ActionTemplateIndex::new(&templates, interner);
        let template_index_ms = template_index_started.elapsed().as_millis();
        let plan_path = config.mod_path.as_ref().unwrap().join(PLAN_PATH);
        let previous: Plan = if plan_path.is_file() {
            serde_json::from_slice(&std::fs::read(&plan_path).map_err(error)?).map_err(error)?
        } else {
            Plan::default()
        };
        let mut plan = Plan::default();
        let mut new_routes = HashSet::new();
        let mut cache = BTreeMap::new();
        let mut seen = HashSet::new();
        let mut report = FixupReport::empty();
        let mut replacements = Vec::new();
        let mut inherited_candidates = Vec::new();
        let mut skipped_races = 0;
        for fk in session
            .form_keys_of_sig(race_sig, interner)
            .map_err(error)?
        {
            let mut race = session
                .record_decoded(&fk, &schema, interner)
                .map_err(error)?;
            let additive = uses_target_project(&race, sources.contains_key(&fk));
            let mut projects = Vec::new();
            let mut blocks = parse_canonical_subgraphs(&race);
            let missing_graphs = missing_source_cores(&source, &blocks, interner)?;
            if !missing_graphs.is_empty() {
                // FO76 ships unfinished races such as MechTest without their
                // behavior assets. Keep the entire race out of the private plan.
                report.warnings.push(interner.intern(&format!(
                    "FO76 behavior port: skipped race {:06X} ({}); missing source cores: {}; existing race paths retained",
                    fk.local,
                    race.eid.and_then(|eid| interner.resolve(eid)).unwrap_or_default(),
                    missing_graphs.join(", "),
                )));
                skipped_races += 1;
                continue;
            }
            let mut needs_conversion = false;
            for block in &blocks {
                let graph = clean(interner.resolve(block.behaviour_graph).unwrap_or_default());
                if !preserved_graph(&graph) {
                    continue;
                }
                if !graph.contains("b21_fo76") {
                    needs_conversion = true;
                    continue;
                }
                let route = previous
                    .routes
                    .iter()
                    .find(|r| clean(&r.destination) == graph)
                    .ok_or_else(|| error(format!("missing saved behavior route for {graph}")))?;
                if seen.insert(route.destination.clone()) {
                    plan.routes.push(route.clone());
                    plan.projects.insert(
                        route.project.clone(),
                        previous.projects[&route.project].clone(),
                    );
                }
            }
            if !needs_conversion {
                if !race.fields.iter().any(|f| f.sig.as_str() == "SGNM")
                    && race.fields.iter().any(|f| {
                        matches!(f.value,
                            FieldValue::FormKey(parent) if f.sig.as_str() == "SADD" && parent.plugin == fk.plugin)
                    })
                {
                    inherited_candidates.push(race);
                }
                continue;
            }
            let target_directory = if additive {
                let mut dir = target_project_directory(&race, interner);
                if dir.is_none() {
                    let parent = race
                        .fields
                        .iter()
                        .find_map(|f| match f.value {
                            FieldValue::FormKey(parent) if f.sig.as_str() == "SADD" => Some(parent),
                            _ => None,
                        })
                        .unwrap();
                    dir = crate::fixups::face::generate_additive_races::load_target_race_template_from_masters(
                        session, &schema, mapper, config, parent,
                    ).and_then(|parent| target_project_directory(&parent, interner));
                }
                Some(dir.ok_or_else(|| {
                    error(format!(
                        "FO76 behavior port: additive race {:06X} has no target race project",
                        fk.local,
                    ))
                })?)
            } else {
                None
            };
            if !additive {
                let Some(original) = sources.get(&fk) else {
                    continue;
                };
                let Some(project) =
                    select_creature_project(original, &race, &schema, &source, interner)?
                else {
                    continue;
                };
                projects.push(project);
            }
            let mut project_rewrites = BTreeMap::new();
            let mut changed = false;
            for block in &mut blocks {
                let graph = clean(interner.resolve(block.behaviour_graph).unwrap_or_default());
                if !preserved_graph(&graph) || graph.contains("b21_fo76") {
                    continue;
                }
                let project = if additive {
                    let dir = perspective_project_directory(
                        target_directory.as_deref().unwrap(),
                        block,
                        interner,
                    );
                    Project {
                        source: dir.clone(),
                        destination: dir,
                        vanilla: true,
                    }
                } else {
                    let original = &projects[0];
                    let dir = original.rsplit_once('/').unwrap().0;
                    let destination = format!(
                        "actors/B21_FO76/{}",
                        dir.strip_prefix("actors/").unwrap_or(dir)
                    );
                    project_rewrites.insert(
                        original.clone(),
                        format!("{destination}/{}", original.rsplit('/').next().unwrap()),
                    );
                    Project {
                        source: original.clone(),
                        destination,
                        vanilla: false,
                    }
                };
                let key = project.destination.clone();
                plan.projects.entry(key.clone()).or_insert(project.clone());
                let route = make_route(&source, &graph, block, &project, interner, &mut cache)
                    .map_err(error)?;
                block.behaviour_graph = interner.intern(&route.destination.replace('/', "\\"));
                block.paths = block
                    .paths
                    .iter()
                    .filter_map(|s| interner.resolve(*s))
                    .filter(|s| !s.to_ascii_lowercase().contains("animations\\fo76\\"))
                    .map(|s| {
                        interner.intern(&format!(
                            "Actors\\B21_FO76\\Source\\{}\\{}",
                            if project.vanilla {
                                "fo4rig"
                            } else {
                                "source_rig"
                            },
                            clean(s).replace('/', "\\")
                        ))
                    })
                    .collect();
                if seen.insert(route.destination.clone()) {
                    new_routes.insert(route.destination.clone());
                    plan.routes.push(route);
                }
                changed = true;
            }
            if !changed {
                continue;
            }
            if !additive {
                let mut aliases = Vec::new();
                for block in &blocks {
                    let mut alias = block.clone();
                    for keyword in &mut alias.target_keywords {
                        if let Some(alias) = grip_aliases.get(keyword) {
                            *keyword = *alias;
                        }
                    }
                    if alias != *block && !blocks.contains(&alias) && !aliases.contains(&alias) {
                        aliases.push(alias);
                    }
                }
                blocks.extend(aliases);
            }
            race.fields
                .retain(|f| !["SGNM", "SAPT", "SAKD", "STKD", "SRAF"].contains(&f.sig.as_str()));
            for block in &blocks {
                race.fields.extend(block_to_entries(block));
            }
            if !additive {
                // The earlier compatibility pass may have substituted a target project.
                let mut old_projects = Vec::new();
                for f in &race.fields {
                    if f.sig.as_str() != "SGNM" && f.sig.as_str() != "SAPT" {
                        project_paths(&f.value, interner, &mut old_projects);
                    }
                }
                if let Some(destination) = project_rewrites.values().next().cloned() {
                    for old in old_projects {
                        project_rewrites.insert(old, destination.clone());
                    }
                }
                for f in &mut race.fields {
                    if f.sig.as_str() != "SGNM" && f.sig.as_str() != "SAPT" {
                        rewrite_strings(&mut f.value, &project_rewrites, interner);
                    }
                }
            }
            replacements.push(race);
        }
        // Inherited-only races still need the host contract of their ported project.
        let project_rewrites: BTreeMap<_, _> = plan
            .projects
            .values()
            .filter(|project| !project.vanilla)
            .map(|project| {
                (
                    clean(&project.source),
                    format!(
                        "{}/{}",
                        project.destination,
                        project.source.rsplit('/').next().unwrap()
                    ),
                )
            })
            .collect();
        for mut race in inherited_candidates {
            let fk = race.form_key;
            if race.fields.iter().any(|f| f.sig.as_str() == "SGNM")
                || !race.fields.iter().any(|f| matches!(f.value,
                    FieldValue::FormKey(parent) if f.sig.as_str() == "SADD" && parent.plugin == fk.plugin))
                || !race_projects(&race, interner).iter().any(|path| project_rewrites.contains_key(path))
            {
                continue;
            }
            for f in &mut race.fields {
                rewrite_strings(&mut f.value, &project_rewrites, interner);
            }
            replacements.push(race);
        }
        plan.routes
            .sort_by(|a, b| a.destination.cmp(&b.destination));
        if config.is_whole_plugin && !config.asset_phases.havok && !plan.routes.is_empty() {
            let mod_path = config.mod_path.as_ref().unwrap();
            let missing_assets = !mod_path.join("debug/fo76_behaviors/assets.json").is_file()
                || plan
                    .routes
                    .iter()
                    .flat_map(|r| r.dependencies.values())
                    .any(|path| !mod_path.join("data/Meshes").join(path).is_file());
            if missing_assets {
                return Err(error(
                    "FO76 behavior assets have not been built for these records; enable Havok conversion before writing the new RACE routes",
                ));
            }
        }
        let action_build_started = std::time::Instant::now();
        let mut route_dependency_keys = 0usize;
        let mut route_template_matches = 0usize;
        let mut root_graph_matches = 0usize;
        let mut root_template_matches = 0usize;
        let mut actions = Vec::new();
        for route in plan
            .routes
            .iter()
            .filter(|r| new_routes.contains(&r.destination))
        {
            route_dependency_keys += route.dependencies.len();
            let family = template_index
                .family_for_dependencies(route.dependencies.keys().map(String::as_str));
            route_template_matches += family.len();
            actions.extend(clone_actions(family, route, mapper));
        }
        for project in plan.projects.values().filter(|p| !p.vanilla) {
            if !plan
                .routes
                .iter()
                .any(|r| r.project == project.destination && new_routes.contains(&r.destination))
            {
                continue;
            }
            let roots = project_roots(&source, &project.source).map_err(error)?;
            for source_graph in template_index.root_graphs(&roots) {
                root_graph_matches += 1;
                let destination = format!(
                    "{}/behaviors/{}",
                    project.destination,
                    source_graph.rsplit('/').next().unwrap()
                );
                let route = Route {
                    source: source_graph.to_owned(),
                    destination,
                    project: project.destination.clone(),
                    dependencies: BTreeMap::new(),
                    animations: BTreeMap::new(),
                    draw_event: None,
                };
                // Root families are cloned together so internal IDLE links remain local.
                if !seen.insert(route.destination.clone()) {
                    continue;
                }
                let family = template_index.family_for_graph(&route.source);
                root_template_matches += family.len();
                actions.extend(clone_actions(family, &route, mapper));
            }
        }
        let action_build_ms = action_build_started.elapsed().as_millis();
        report.records_changed = session
            .replace_records_contents(replacements, &schema, interner)
            .map_err(error)? as u32;
        let existence_filter_started = std::time::Instant::now();
        let action_candidates = actions.len();
        actions.retain(|r| {
            session
                .record_decoded(&r.form_key, &schema, interner)
                .is_err()
        });
        let existence_filter_ms = existence_filter_started.elapsed().as_millis();
        report.records_added = actions.len() as u32;
        let insertion_started = std::time::Instant::now();
        session
            .add_records(actions, &schema, interner)
            .map_err(error)?;
        let insertion_ms = insertion_started.elapsed().as_millis();
        eprintln!(
            "[preserve_fo76_behaviors_timing] template_prepare_ms={template_prepare_ms} template_index_ms={template_index_ms} action_build_ms={action_build_ms} existence_filter_ms={existence_filter_ms} insertion_ms={insertion_ms} templates={} route_dependency_keys={route_dependency_keys} route_template_matches={route_template_matches} root_graph_matches={root_graph_matches} root_template_matches={root_template_matches} action_candidates={action_candidates} existing_actions={} inserted_actions={}",
            templates.len(),
            action_candidates - report.records_added as usize,
            report.records_added,
        );
        let path = config.mod_path.as_ref().unwrap().join(PLAN_PATH);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(error)?;
        std::fs::write(path, serde_json::to_vec_pretty(&plan).map_err(error)?).map_err(error)?;
        report.message = Some(interner.intern(&format!(
            "FO76 behavior routes: {}; projects: {}; cloned actions: {}; skipped races: {}",
            plan.routes.len(),
            plan.projects.len(),
            report.records_added,
            skipped_races,
        )));
        Ok(report)
    }
}

pub(super) fn make_route(
    source: &Path,
    graph: &str,
    block: &SubgraphBlock,
    project: &Project,
    interner: &StringInterner,
    cache: &mut BTreeMap<String, HkxFile>,
) -> Result<Route, String> {
    let paths: Vec<_> = block
        .paths
        .iter()
        .filter_map(|s| interner.resolve(*s))
        .map(clean)
        .filter(|s| !s.contains("/animations/fo76/"))
        .collect();
    let graph_dependencies = dependencies(source, graph, cache)?;
    let mut animations = BTreeMap::new();
    for dependency in &graph_dependencies {
        for clip in cache[dependency]
            .objects()
            .iter()
            .filter(|o| o.class_name == "hkbClipGenerator")
        {
            let name = string(clip, "animationName");
            if let Some(origin) = resolve_animation(source, dependency, &name, &paths) {
                animations.insert(name, origin);
            }
        }
    }
    if furniture_graph(graph) {
        let mut leaves: HashSet<_> = animations
            .values()
            .filter_map(|path| path.rsplit('/').next().map(str::to_string))
            .collect();
        for path in &paths {
            let directory = source.join(path);
            if !directory.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&directory).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let leaf = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if entry.path().is_file() && leaf.ends_with(".hkx") && leaves.insert(leaf.clone()) {
                    animations.insert(
                        format!("Animations\\{leaf}"),
                        clean(&format!("{path}/{leaf}")),
                    );
                }
            }
        }
    }
    let digest = Sha256::digest(serde_json::to_vec(&animations).map_err(|e| e.to_string())?);
    let fingerprint = hex::encode(digest)[..12].to_string();
    let graph_dependencies: BTreeMap<_, _> = graph_dependencies
        .into_iter()
        .map(|path| {
            let stem = Path::new(&path).file_stem().unwrap().to_string_lossy();
            let destination = format!(
                "{}/behaviors/B21_FO76_{stem}_{fingerprint}.hkx",
                project.destination
            );
            (path, destination)
        })
        .collect();
    let draw_event = graph_dependencies
        .keys()
        .any(|s| s.ends_with("/gunbehavior.hkx") || s.ends_with("/meleebehavior.hkx"))
        .then(|| {
            format!(
                "B21_FO76_{}_{}_weapEquip",
                clean(&project.destination).replace('/', "_"),
                fingerprint
            )
        });
    Ok(Route {
        source: graph.to_string(),
        destination: graph_dependencies[graph].clone(),
        project: project.destination.clone(),
        dependencies: graph_dependencies,
        animations,
        draw_event,
    })
}

fn action_templates(
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
    config: &FixupConfig,
) -> Result<Vec<Record>, FixupError> {
    let schema = session.schema().map_err(error)?;
    let source_schema = session.source_schema().map_err(error)?;
    let interner = mapper.interner;
    let idle = SigCode::from_str("IDLE").unwrap();
    let mut templates = HashMap::new();
    for fk in session
        .source_form_keys_of_sig(idle, interner)
        .map_err(error)?
    {
        let mut source = session
            .source_record_decoded(&fk, &source_schema, interner)
            .map_err(error)?;
        let graph = field(&source, "DNAM", interner);
        if !preserved_graph(&graph) {
            continue;
        }
        let Some(mapped) = mapper.lookup(fk) else {
            continue;
        };
        let mut target = session.record_decoded(&mapped, &schema, interner).ok();
        let mut handle = session.target_id();
        if target.is_none() {
            for master_handle in &config.target_master_handle_ids {
                if let Ok(record) =
                    session.record_decoded_in_handle(*master_handle, &mapped, &schema, interner)
                {
                    target = Some(record);
                    handle = *master_handle;
                    break;
                }
            }
        }
        if let Some(mut target) = target {
            decode_raw_anchors(&mut target, session, handle, interner)?;
            let source_handle = session.source_id().unwrap();
            decode_raw_anchors(&mut source, session, source_handle, interner)?;
            restore_source_action_order(&source, &mut target, mapper);
            put(&mut target, "DNAM", &graph, interner);
            put(
                &mut target,
                "ENAM",
                &field(&source, "ENAM", interner),
                interner,
            );
            templates.entry(mapped).or_insert(target);
        }
    }
    for handle in &config.target_master_handle_ids {
        for fk in session
            .form_keys_of_sig_in_handle(*handle, idle, interner)
            .map_err(error)?
        {
            let mut record = session
                .record_decoded_in_handle(*handle, &fk, &schema, interner)
                .map_err(error)?;
            let graph = clean(&field(&record, "DNAM", interner));
            if graph.ends_with("/mtbehavior.hkx")
                || graph.ends_with("/meleebehavior.hkx")
                || furniture_graph(&graph)
            {
                decode_raw_anchors(&mut record, session, *handle, interner)?;
                templates.entry(fk).or_insert(record);
            }
        }
    }
    let mut records: Vec<_> = templates.into_values().collect();
    records.sort_by_key(|r| {
        (
            interner
                .resolve(r.form_key.plugin)
                .unwrap_or_default()
                .to_string(),
            r.form_key.local,
        )
    });
    Ok(records)
}

fn restore_source_action_order(source: &Record, target: &mut Record, mapper: &FormKeyMapper) {
    let anchors = source.fields.iter().find_map(|field| match &field.value {
        FieldValue::Struct(fields) if field.sig.as_str() == "ANAM" => Some(fields),
        _ => None,
    });
    let previous = anchors
        .and_then(|fields| {
            fields.iter().find(|(name, _)| {
                mapper
                    .interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("Previous"))
            })
        })
        .map(|(_, value)| value.clone())
        .unwrap_or(FieldValue::Uint(0));
    let previous = match previous {
        FieldValue::FormKey(key) => mapper
            .lookup(key)
            .map(FieldValue::FormKey)
            .unwrap_or(FieldValue::Uint(0)),
        value => value,
    };
    let Some(FieldValue::Struct(fields)) = target
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "ANAM")
        .map(|field| &mut field.value)
    else {
        return;
    };
    // Vanilla IDLE deduplication can put a generic FO4 leaf ahead of a new FO76 sibling.
    if let Some((_, value)) = fields.iter_mut().find(|(name, _)| {
        mapper
            .interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case("Previous"))
    }) {
        *value = previous;
    } else {
        fields.push((mapper.interner.intern("Previous"), previous));
    }
}

fn decode_raw_anchors(
    record: &mut Record,
    session: &mut PluginSession,
    handle: u64,
    interner: &StringInterner,
) -> Result<(), FixupError> {
    let Some(entry) = record.fields.iter_mut().find(|f| f.sig.as_str() == "ANAM") else {
        return Ok(());
    };
    let FieldValue::Bytes(bytes) = &entry.value else {
        return Ok(());
    };
    if bytes.len() != 8 {
        return Err(error("IDLE ANAM must contain parent and previous FormIDs"));
    }
    let (masters, plugin) = session.handle_load_order(handle).map_err(error)?;
    let key = format!(
        "{:06X}:{}",
        record.form_key.local,
        interner.resolve(record.form_key.plugin).unwrap()
    );
    let mut fields = Vec::new();
    for (index, name) in ["Parent", "Previous"].into_iter().enumerate() {
        let raw = u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
        let value = if raw == 0 {
            FieldValue::Uint(0)
        } else {
            let owner = (raw >> 24) as usize;
            let owner = if owner == masters.len() {
                plugin
            } else {
                masters
                    .get(owner)
                    .ok_or_else(|| error(format!("invalid master index in IDLE {key}")))?
            };
            FieldValue::FormKey(FormKey {
                plugin: interner.intern(owner),
                local: raw & 0x00FF_FFFF,
            })
        };
        fields.push((interner.intern(name), value));
    }
    entry.value = FieldValue::Struct(fields);
    Ok(())
}

fn clone_actions<'a>(
    family: impl IntoIterator<Item = &'a Record>,
    route: &Route,
    mapper: &mut FormKeyMapper,
) -> Vec<Record> {
    let interner = mapper.interner;
    let mut mapping = HashMap::new();
    let mut records = Vec::new();
    for template in family {
        let mut record = template.clone();
        let seed = format!(
            "{}|{}|{:06X}",
            route.destination,
            interner.resolve(template.form_key.plugin).unwrap(),
            template.form_key.local
        );
        let hash = hex::encode(Sha256::digest(seed.as_bytes()));
        let eid = format!("B21_FO76_Action_{}", &hash[..24]);
        let synthetic = FormKey {
            plugin: interner.intern(&format!("__fo76_behavior_action_{hash}__")),
            local: 0x800,
        };
        record.form_key =
            mapper.allocate_or_resolve(synthetic, Some(interner.intern(&eid)), record.sig);
        record.eid = Some(interner.intern(&eid));
        put(&mut record, "EDID", &eid, interner);
        put(
            &mut record,
            "DNAM",
            &route.destination.replace('/', "\\"),
            interner,
        );
        if field(&record, "ENAM", interner).eq_ignore_ascii_case("weapEquip") {
            if let Some(event) = &route.draw_event {
                put(&mut record, "ENAM", event, interner);
            }
        }
        super::bow::repair_release_condition(&mut record, route, interner);
        mapping.insert(template.form_key, record.form_key);
        records.push(record);
    }
    fn rewrite(
        value: &mut FieldValue,
        mapping: &HashMap<FormKey, FormKey>,
        previous: bool,
        interner: &StringInterner,
    ) {
        match value {
            FieldValue::FormKey(fk) => {
                if let Some(mapped) = mapping.get(fk) {
                    *fk = *mapped
                } else if previous {
                    *value = FieldValue::Uint(0)
                }
            }
            FieldValue::List(values) => {
                for v in values {
                    rewrite(v, mapping, previous, interner)
                }
            }
            FieldValue::Struct(fields) => {
                for (name, v) in fields {
                    rewrite(
                        v,
                        mapping,
                        interner
                            .resolve(*name)
                            .is_some_and(|n| n.eq_ignore_ascii_case("Previous")),
                        interner,
                    )
                }
            }
            _ => {}
        }
    }
    for record in &mut records {
        for f in &mut record.fields {
            if f.sig.as_str() == "ANAM" {
                rewrite(&mut f.value, &mapping, false, interner);
            }
        }
    }
    records
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod project_tests;

#[cfg(test)]
#[path = "additive_tests.rs"]
mod additive_tests;

#[cfg(test)]
#[path = "fixer_tests.rs"]
mod fixer_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        ParsedRecord, ParsedSubrecord, insert_parsed_record_in_slot, plugin_handle_close_native,
        plugin_handle_new_native, plugin_handle_store_ref,
    };

    #[test]
    fn source_action_order_keeps_bow_before_vanilla_charge_leaf() {
        let interner = StringInterner::new();
        let key = |plugin: &str, local| FormKey {
            plugin: interner.intern(plugin),
            local,
        };
        let source_bow = key("SeventySix.esm", 0x5632F7);
        let target_bow = key("B21_Test.esp", 0x800);
        let target_parent = key("Fallout4.esm", 0x11A187);
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "B21_Test.esp".into(),
                ..Default::default()
            },
            &interner,
        );
        mapper.add_mapping(source_bow, target_bow);
        let mut source = Record::new(
            SigCode::from_str("IDLE").unwrap(),
            key("SeventySix.esm", 0x11A188),
        );
        source.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ANAM").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("Previous"),
                FieldValue::FormKey(source_bow),
            )]),
        });
        let mut target = Record::new(source.sig, key("Fallout4.esm", 0x11A188));
        target.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ANAM").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("Parent"),
                FieldValue::FormKey(target_parent),
            )]),
        });
        restore_source_action_order(&source, &mut target, &mapper);
        assert_eq!(
            target.fields[0].value,
            FieldValue::Struct(vec![
                (
                    interner.intern("Parent"),
                    FieldValue::FormKey(target_parent)
                ),
                (interner.intern("Previous"), FieldValue::FormKey(target_bow)),
            ])
        );
        let restored = target.fields.clone();
        restore_source_action_order(&source, &mut target, &mapper);
        assert_eq!(target.fields, restored);

        let bow = Record::new(source.sig, target_bow);
        let route = Route {
            source: "actors/character/_1stperson/behaviors/gunbehavior.hkx".into(),
            destination: "actors/character/_1stperson/behaviors/B21_Bow.hkx".into(),
            project: "actors/character/_1stperson".into(),
            dependencies: BTreeMap::new(),
            animations: BTreeMap::new(),
            draw_event: None,
        };
        let cloned = clone_actions(&[target.clone(), bow], &route, &mut mapper);
        assert!(cloned[0].fields.iter().any(|field| field.value
            == FieldValue::Struct(vec![
                (
                    interner.intern("Parent"),
                    FieldValue::FormKey(target_parent)
                ),
                (
                    interner.intern("Previous"),
                    FieldValue::FormKey(cloned[1].form_key)
                ),
            ])));

        source.fields.clear();
        restore_source_action_order(&source, &mut target, &mapper);
        assert_eq!(
            target.fields[0].value,
            FieldValue::Struct(vec![
                (
                    interner.intern("Parent"),
                    FieldValue::FormKey(target_parent)
                ),
                (interner.intern("Previous"), FieldValue::Uint(0)),
            ])
        );
    }

    #[test]
    fn action_template_index_matches_ordered_legacy_selection() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fallout4.esm");
        let idle = SigCode::from_str("IDLE").unwrap();
        let template = |local, graph: &str| {
            let mut record = Record::new(idle, FormKey { plugin, local });
            put(&mut record, "DNAM", graph, &interner);
            put(
                &mut record,
                "ENAM",
                &format!("B21_TestAction_{local:06X}"),
                &interner,
            );
            record
        };
        let templates = vec![
            template(0x100, r"Actors\Character\Behaviors\GunBehavior.hkx"),
            template(0x200, r"Actors\MoleRat\Behaviors\MoleRatBehavior.hkx"),
            template(
                0x101,
                r"Actors\Character\_1stPerson\Behaviors\GunBehavior.hkx",
            ),
            template(0x102, r"Actors\Character\Behaviors\GunBehavior.hkx"),
            template(0x201, r"Actors\MoleRat\Behaviors\MoleRatBehavior.hkx"),
            template(0x300, r"Actors\Other\Behaviors\OtherBehavior.hkx"),
            template(0x400, ""),
        ];
        let index = ActionTemplateIndex::new(&templates, &interner);
        let family_keys = |family: &[&Record]| {
            family
                .iter()
                .map(|record| record.form_key)
                .collect::<Vec<_>>()
        };
        let legacy_family = |dependencies: &BTreeMap<String, String>| {
            templates
                .iter()
                .filter(|record| {
                    let path = clean(&field(record, "DNAM", &interner));
                    dependencies.keys().any(|dependency| {
                        dependency.rsplit('/').next() == path.rsplit('/').next()
                            && dependency.contains("_1stperson") == path.contains("_1stperson")
                    })
                })
                .collect::<Vec<_>>()
        };

        let third_person_dependencies = BTreeMap::from([
            (
                "actors/source/behaviors/gunbehavior.hkx".to_string(),
                "unused".to_string(),
            ),
            (
                "actors/duplicate/behaviors/gunbehavior.hkx".to_string(),
                "unused".to_string(),
            ),
        ]);
        let first_person_dependencies = BTreeMap::from([(
            "actors/source/_1stperson/behaviors/gunbehavior.hkx".to_string(),
            "unused".to_string(),
        )]);
        let legacy_third = legacy_family(&third_person_dependencies);
        let indexed_third =
            index.family_for_dependencies(third_person_dependencies.keys().map(String::as_str));
        assert_eq!(family_keys(&indexed_third), family_keys(&legacy_third));
        assert_eq!(
            family_keys(&indexed_third),
            [
                FormKey {
                    plugin,
                    local: 0x100
                },
                FormKey {
                    plugin,
                    local: 0x102
                },
            ]
        );
        let legacy_first = legacy_family(&first_person_dependencies);
        let indexed_first =
            index.family_for_dependencies(first_person_dependencies.keys().map(String::as_str));
        assert_eq!(family_keys(&indexed_first), family_keys(&legacy_first));
        assert_eq!(indexed_first.len(), 1);
        assert_eq!(indexed_first[0].form_key.local, 0x101);
        for dependencies in [
            BTreeMap::from([(
                "actors/source/behaviors/unmatched.hkx".to_string(),
                "unused".to_string(),
            )]),
            BTreeMap::from([("".to_string(), "unused".to_string())]),
        ] {
            let legacy = legacy_family(&dependencies);
            let indexed = index.family_for_dependencies(dependencies.keys().map(String::as_str));
            assert_eq!(family_keys(&indexed), family_keys(&legacy));
        }

        let roots = vec![
            "actors/molerat/behaviors/moleratbehavior.hkx".to_string(),
            "actors/character/behaviors/gunbehavior.hkx".to_string(),
        ];
        let mut legacy_root_graphs = Vec::new();
        let mut seen_graphs = HashSet::new();
        for record in templates
            .iter()
            .filter(|record| roots.contains(&clean(&field(record, "DNAM", &interner))))
        {
            let graph = clean(&field(record, "DNAM", &interner));
            if seen_graphs.insert(graph.clone()) {
                legacy_root_graphs.push(graph);
            }
        }
        let indexed_root_graphs = index
            .root_graphs(&roots)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(indexed_root_graphs, legacy_root_graphs);
        assert!(
            index
                .root_graphs(&["actors/unmatched/behaviors/unmatched.hkx".to_string()])
                .is_empty()
        );
        for graph in indexed_root_graphs {
            let legacy = templates
                .iter()
                .filter(|record| clean(&field(record, "DNAM", &interner)) == graph)
                .collect::<Vec<_>>();
            assert_eq!(
                family_keys(&index.family_for_graph(&graph)),
                family_keys(&legacy)
            );
        }

        let route = Route {
            source: "actors/character/behaviors/gunbehavior.hkx".into(),
            destination: "actors/b21_test/behaviors/gunbehavior.hkx".into(),
            project: "actors/b21_test/project.hkx".into(),
            dependencies: third_person_dependencies,
            animations: BTreeMap::new(),
            draw_event: None,
        };
        let options = MapperOptions {
            output_plugin_name: "B21_Test.esp".into(),
            ..Default::default()
        };
        let mut legacy_mapper = FormKeyMapper::new([], options.clone(), &interner);
        let mut indexed_mapper = FormKeyMapper::new([], options, &interner);
        let legacy_clones = clone_actions(legacy_third, &route, &mut legacy_mapper);
        let indexed_clones = clone_actions(indexed_third, &route, &mut indexed_mapper);
        assert_eq!(indexed_clones.len(), legacy_clones.len());
        for (indexed, legacy) in indexed_clones.iter().zip(&legacy_clones) {
            assert_eq!(indexed.sig, legacy.sig);
            assert_eq!(indexed.form_key, legacy.form_key);
            assert_eq!(indexed.eid, legacy.eid);
            assert_eq!(indexed.flags, legacy.flags);
            assert_eq!(indexed.fields, legacy.fields);
            assert_eq!(indexed.warnings, legacy.warnings);
        }
    }

    #[test]
    fn session_keeps_repaired_race_gates_and_clones_action_trees_for_humans_and_creatures() {
        let interner = StringInterner::new();
        let key = |local| FormKey {
            plugin: interner.intern("B21_Test.esp"),
            local,
        };
        let graph = "Actors\\Character\\Behaviors\\GunBehavior.hkx";
        let project = "Actors\\MoleMiner\\MoleMinerProject.hkx";
        let block = SubgraphBlock {
            behaviour_graph: interner.intern(graph),
            paths: vec![interner.intern("Actors\\MoleMiner\\Animations\\MT")],
            subgraph_keywords: vec![],
            target_keywords: vec![key(0x1300)],
            flags_bytes: Some(smallvec::smallvec![1, 0, 0, 0]),
        };
        let mut draw = Record::new(SigCode::from_str("IDLE").unwrap(), key(0x1200));
        put(&mut draw, "EDID", "B21_TestDraw", &interner);
        put(&mut draw, "DNAM", graph, &interner);
        put(&mut draw, "ENAM", "WeapEquip", &interner);
        draw.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ANAM").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("Parent"), FieldValue::FormKey(key(0x1400))),
                (
                    interner.intern("Previous"),
                    FieldValue::FormKey(key(0x1401)),
                ),
            ]),
        });
        let mut fire = draw.clone();
        fire.form_key = key(0x1201);
        put(&mut fire, "EDID", "B21_TestFire", &interner);
        put(&mut fire, "ENAM", "WeaponFire", &interner);
        fire.fields
            .iter_mut()
            .find(|f| f.sig.as_str() == "ANAM")
            .unwrap()
            .value = FieldValue::Struct(vec![
            (
                interner.intern("Parent"),
                FieldValue::FormKey(draw.form_key),
            ),
            (
                interner.intern("Previous"),
                FieldValue::FormKey(draw.form_key),
            ),
        ]);
        let source = plugin_handle_new_native("B21_Test.esp", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("B21_Test.esp", Some("fo4")).unwrap();
        let raw = |sig: &str, data: Vec<u8>| ParsedSubrecord {
            signature: sig.into(),
            data: data.into(),
            semantic_type: None,
        };
        let zstring = |sig: &str, value: &str| raw(sig, value.bytes().chain([0]).collect());
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            for (handle, local, female, additive) in [
                (source, 0x1100, true, false),
                (target, 0x1100, false, false),
                (target, 0x1101, false, true),
                (source, 0x1103, false, false),
                (target, 0x1103, false, false),
                (source, 0x1104, false, false),
                (target, 0x1104, false, false),
                (source, 0x1105, false, false),
                (target, 0x1105, false, false),
            ] {
                let mut subrecords = vec![
                    zstring("EDID", &format!("B21_TestRace{local:X}")),
                    raw("MNAM", vec![]),
                    zstring("ANAM", r"Actors\MoleMiner\CharacterAssets\skeleton.nif"),
                    raw("MODT", vec![0; 4]),
                    raw("FNAM", vec![]),
                    zstring("ANAM", r"Actors\MoleMiner\CharacterAssets\skeleton.nif"),
                    raw("MODT", vec![0; 4]),
                    raw("NAM1", vec![]),
                    raw("GNAM", vec![0; 4]),
                    raw("MNAM", vec![]),
                    zstring(
                        "MODL",
                        if local == 0x1103 {
                            r"Actors\MechTest\MechTest.hkx"
                        } else if additive {
                            r"Actors\Character\RaiderProject.hkx"
                        } else {
                            project
                        },
                    ),
                    raw("MODT", vec![0; 4]),
                ];
                if female {
                    subrecords.extend([
                        raw("FNAM", vec![]),
                        zstring("MODL", r"Actors\WrongFemale\WrongProject.hkx"),
                        raw("MODT", vec![0; 4]),
                    ]);
                }
                if local != 0x1105 {
                    subrecords.extend([
                        raw(
                            "STKD",
                            if female { 0x1301u32 } else { 0x1300u32 }
                                .to_le_bytes()
                                .to_vec(),
                        ),
                        zstring("SGNM", graph),
                        zstring("SAPT", r"Actors\MoleMiner\Animations\MT"),
                        raw("SRAF", vec![1, 0, 0, 0]),
                    ]);
                }
                if additive {
                    subrecords.push(raw("SADD", 0x1102u32.to_le_bytes().to_vec()));
                }
                if local == 0x1104 || local == 0x1105 {
                    subrecords.push(raw("SADD", 0x1100u32.to_le_bytes().to_vec()));
                }
                if local == 0x1103 {
                    subrecords.extend([
                        zstring(
                            "SGNM",
                            r"Actors\MechTest\Behaviors\MechTestCoreBehavior.hkx",
                        ),
                        zstring("SAPT", r"Actors\MechTest\Animations"),
                        raw("SRAF", vec![0; 4]),
                    ]);
                }
                insert_parsed_record_in_slot(
                    store.get_mut(&handle).unwrap(),
                    ParsedRecord {
                        signature: "RACE".into(),
                        form_id: local,
                        flags: 0,
                        version_control: 0,
                        form_version: Some(131),
                        version2: None,
                        subrecords,
                        raw_payload: None,
                        parse_error: None,
                    },
                );
            }
        }
        {
            let mut session = open_session(source, None).unwrap();
            let schema = session.schema().unwrap();
            session
                .add_records(vec![draw.clone(), fire.clone()], &schema, &interner)
                .unwrap();
        }
        let temp = tempfile::tempdir().unwrap();
        let graph_file = temp
            .path()
            .join("source/meshes")
            .join(graph.replace('\\', "/"));
        std::fs::create_dir_all(graph_file.parent().unwrap()).unwrap();
        std::fs::write(
            &graph_file,
            HkxFile::from_tagxml(11, VERSION, vec![]).save(),
        )
        .unwrap();
        let mut session = open_session(target, Some(source)).unwrap();
        let schema = session.schema().unwrap();
        session
            .add_records(vec![draw, fire], &schema, &interner)
            .unwrap();
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "B21_Test.esp".into(),
                preserve_source_ids: true,
                ..Default::default()
            },
            &interner,
        );
        for (local, sig) in [
            (0x1100, "RACE"),
            (0x1103, "RACE"),
            (0x1104, "RACE"),
            (0x1105, "RACE"),
            (0x1200, "IDLE"),
            (0x1201, "IDLE"),
        ] {
            assert_eq!(
                mapper.allocate_or_resolve(key(local), None, SigCode::from_str(sig).unwrap()),
                key(local)
            );
        }
        let config = FixupConfig {
            mod_path: Some(temp.path().join("output")),
            source_extracted_dir: Some(temp.path().join("source")),
            ..Default::default()
        };
        let decoded_source = session
            .source_record_decoded(&key(0x1100), &session.source_schema().unwrap(), &interner)
            .unwrap();
        let mut source_projects = Vec::new();
        for f in &decoded_source.fields {
            if f.sig.as_str() != "SGNM" {
                project_paths(&f.value, &interner, &mut source_projects);
            }
        }
        assert_eq!(
            source_projects.len(),
            2,
            "source project fields: {:?}",
            decoded_source.fields
        );
        let project_file = temp
            .path()
            .join("source/meshes")
            .join(project.replace('\\', "/"));
        std::fs::create_dir_all(project_file.parent().unwrap()).unwrap();
        std::fs::write(
            project_file,
            HkxFile::from_tagxml(11, VERSION, vec![]).save(),
        )
        .unwrap();
        let missing_race_before = session
            .record_decoded(&key(0x1103), &schema, &interner)
            .unwrap();
        let report = PreserveFo76BehaviorsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(report.records_changed, 4);
        assert_eq!(report.records_added, 4);
        assert_eq!(report.warnings.len(), 1);
        let warning = interner.resolve(report.warnings[0]).unwrap();
        assert!(warning.contains("001103") && warning.contains("mechtestcorebehavior.hkx"));
        assert_eq!(
            session
                .record_decoded(&key(0x1103), &schema, &interner)
                .unwrap()
                .fields,
            missing_race_before.fields,
        );
        let saved_plan: Plan = serde_json::from_slice(
            &std::fs::read(config.mod_path.as_ref().unwrap().join(PLAN_PATH)).unwrap(),
        )
        .unwrap();
        assert!(
            !saved_plan
                .projects
                .values()
                .any(|p| p.source.contains("mechtest"))
        );
        assert!(
            !saved_plan
                .routes
                .iter()
                .any(|r| r.source.contains("mechtest"))
        );
        let child = session
            .record_decoded(&key(0x1104), &schema, &interner)
            .unwrap();
        assert_eq!(
            race_projects(&child, &interner),
            ["actors/b21_fo76/moleminer/moleminerproject.hkx"]
        );
        assert!(
            child
                .fields
                .iter()
                .any(|f| f.sig.as_str() == "SADD" && f.value == FieldValue::FormKey(key(0x1100)))
        );
        let inherited = session
            .record_decoded(&key(0x1105), &schema, &interner)
            .unwrap();
        assert_eq!(
            race_projects(&inherited, &interner),
            ["actors/b21_fo76/moleminer/moleminerproject.hkx"]
        );
        assert!(parse_canonical_subgraphs(&inherited).is_empty());
        assert!(
            inherited
                .fields
                .iter()
                .any(|f| f.sig.as_str() == "SADD" && f.value == FieldValue::FormKey(key(0x1100)))
        );
        let repaired = session
            .record_decoded(&key(0x1100), &schema, &interner)
            .unwrap();
        let blocks = parse_canonical_subgraphs(&repaired);
        assert_eq!(blocks[0].target_keywords, block.target_keywords);
        assert_eq!(blocks[0].flags_bytes, block.flags_bytes);
        let mut projects = Vec::new();
        for f in &repaired.fields {
            if f.sig.as_str() != "SGNM" {
                project_paths(&f.value, &interner, &mut projects);
            }
        }
        assert_eq!(projects, ["actors/b21_fo76/moleminer/moleminerproject.hkx"]);
        let mut families: BTreeMap<String, Vec<Record>> = BTreeMap::new();
        for fk in session
            .form_keys_of_sig(SigCode::from_str("IDLE").unwrap(), &interner)
            .unwrap()
        {
            if [0x1200, 0x1201].contains(&fk.local) {
                continue;
            }
            let mut record = session.record_decoded(&fk, &schema, &interner).unwrap();
            decode_raw_anchors(&mut record, &mut session, target, &interner).unwrap();
            families
                .entry(field(&record, "DNAM", &interner))
                .or_default()
                .push(record);
        }
        assert_eq!(families.len(), 2);
        for records in families.values() {
            let draw = records
                .iter()
                .find(|r| field(r, "ENAM", &interner).ends_with("_weapEquip"))
                .unwrap();
            let fire = records
                .iter()
                .find(|r| field(r, "ENAM", &interner) == "WeaponFire")
                .unwrap();
            let anchor = |record: &Record, name: &str| {
                let entry = record
                    .fields
                    .iter()
                    .find(|f| f.sig.as_str() == "ANAM")
                    .unwrap();
                let FieldValue::Struct(fields) = &entry.value else {
                    panic!("ANAM should decode as a typed tree")
                };
                fields
                    .iter()
                    .find(|(n, _)| interner.resolve(*n) == Some(name))
                    .unwrap()
                    .1
                    .clone()
            };
            assert_eq!(anchor(draw, "Parent"), FieldValue::FormKey(key(0x1400)));
            assert!(!matches!(anchor(draw, "Previous"), FieldValue::FormKey(fk) if fk.local != 0));
            assert_eq!(anchor(fire, "Parent"), FieldValue::FormKey(draw.form_key));
            assert_eq!(anchor(fire, "Previous"), FieldValue::FormKey(draw.form_key));
        }
        let plan_before = std::fs::read(config.mod_path.as_ref().unwrap().join(PLAN_PATH)).unwrap();
        let generated_action_snapshot = |session: &mut PluginSession| {
            let mut snapshot = Vec::new();
            for fk in session
                .form_keys_of_sig(SigCode::from_str("IDLE").unwrap(), &interner)
                .unwrap()
            {
                if [0x1200, 0x1201].contains(&fk.local) {
                    continue;
                }
                let record = session.record_decoded(&fk, &schema, &interner).unwrap();
                snapshot.push((
                    record.sig,
                    record.form_key,
                    record.eid,
                    record.flags,
                    record.fields,
                    record.warnings,
                ));
            }
            snapshot
        };
        let generated_before = generated_action_snapshot(&mut session);
        let second = PreserveFo76BehaviorsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(second.records_changed, 0);
        assert_eq!(second.records_added, 0);
        assert_eq!(
            std::fs::read(config.mod_path.as_ref().unwrap().join(PLAN_PATH)).unwrap(),
            plan_before
        );
        assert_eq!(generated_action_snapshot(&mut session), generated_before);
        let corrupt_graph = temp
            .path()
            .join("source/meshes/actors/mechtest/behaviors/mechtestcorebehavior.hkx");
        std::fs::create_dir_all(corrupt_graph.parent().unwrap()).unwrap();
        std::fs::write(&corrupt_graph, b"not a valid HKX").unwrap();
        let failure = PreserveFo76BehaviorsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap_err()
            .to_string();
        assert!(failure.contains("mechtestcorebehavior.hkx"), "{failure}");
        std::fs::remove_file(corrupt_graph).unwrap();
        let records_only = FixupConfig {
            is_whole_plugin: true,
            ..config
        };
        assert!(
            PreserveFo76BehaviorsFixup
                .run_with_session(&mut session, &mut mapper, &records_only)
                .unwrap_err()
                .to_string()
                .contains("enable Havok conversion")
        );
        drop(session);
        assert!(plugin_handle_close_native(target));
        assert!(plugin_handle_close_native(source));
    }
}
