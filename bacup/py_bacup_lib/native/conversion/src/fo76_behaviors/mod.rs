use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, HkxMember, HkxObject};
use serde::{Deserialize, Serialize};

pub mod assets;
mod bow;
mod furniture;
mod locomotion;
pub mod records;

#[cfg(test)]
mod furniture_tests;

pub const PLAN_PATH: &str = "debug/fo76_behaviors/plan.json";
pub const VERSION: &str = "hk_2014.1.0-r1";

pub(crate) fn enabled(
    session: &crate::session::PluginSession,
    config: &crate::fixups::FixupConfig,
) -> bool {
    session
        .source_slot_opt()
        .and_then(|s| s.parsed.game.as_deref())
        == Some("fo76")
        && session.target_slot().parsed.game.as_deref() == Some("fo4")
        && config.mod_path.is_some()
        && config.source_extracted_dir.is_some()
}

#[derive(Default, Serialize, Deserialize)]
pub struct Plan {
    pub routes: Vec<Route>,
    pub projects: BTreeMap<String, Project>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    pub source: String,
    pub destination: String,
    pub vanilla: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Route {
    pub source: String,
    pub destination: String,
    pub project: String,
    pub dependencies: BTreeMap<String, String>,
    pub animations: BTreeMap<String, String>,
    pub draw_event: Option<String>,
}

pub fn clean(path: &str) -> String {
    let forward = path.replace('\\', "/").to_ascii_lowercase();
    let mut parts = Vec::new();
    for part in forward.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

pub fn combat_graph(path: &str) -> bool {
    let path = clean(path);
    path.starts_with("actors/")
        && path.contains("/behaviors/")
        && ![
            "furniture",
            "facegen",
            "pipboy",
            "workshop",
            "terminal",
            "paired",
            "flavor",
        ]
        .iter()
        .any(|part| path.contains(part))
}

pub fn furniture_graph(path: &str) -> bool {
    let path = clean(path);
    path.starts_with("actors/") && path.contains("/behaviors/") && path.contains("furniture")
}

pub fn preserved_graph(path: &str) -> bool {
    combat_graph(path) || furniture_graph(path)
}

pub fn project_dir(graph: &str) -> String {
    clean(graph)
        .split("/behaviors/")
        .next()
        .unwrap()
        .to_string()
}

pub fn relative_asset(owner_dir: &str, path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = if path.to_ascii_lowercase().starts_with("actors/") {
        path
    } else {
        format!("{owner_dir}/{path}")
    };
    let path = clean(&path);
    if path.ends_with(".hkt") {
        format!("{}.hkx", &path[..path.len() - 4])
    } else if !path.ends_with(".hkx") {
        format!("{path}.hkx")
    } else {
        path
    }
}

pub fn relative_path(owner: &str, target: &str) -> String {
    let owner = clean(owner);
    let target = clean(target);
    let a: Vec<_> = owner.split('/').collect();
    let b: Vec<_> = target.split('/').collect();
    let shared = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    std::iter::repeat_n("..", a.len() - shared)
        .chain(b[shared..].iter().copied())
        .collect::<Vec<_>>()
        .join("\\")
}

pub fn meshes(root: &Path) -> PathBuf {
    if root.join("Meshes").is_dir() {
        root.join("Meshes")
    } else if root.join("meshes").is_dir() {
        root.join("meshes")
    } else {
        root.to_path_buf()
    }
}

pub fn read(path: &Path) -> Result<HkxFile, String> {
    HkxFile::read(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn value<'a>(object: &'a HkxObject, name: &str) -> Option<&'a HkxValue> {
    object
        .members
        .iter()
        .find(|m| m.name == name)
        .map(|m| &m.value)
}
pub fn text(value: &HkxValue) -> Option<&str> {
    match value {
        HkxValue::String { value, .. } => Some(value),
        _ => None,
    }
}
pub fn string(object: &HkxObject, name: &str) -> String {
    value(object, name)
        .and_then(text)
        .unwrap_or_default()
        .to_string()
}
pub fn number(value: &HkxValue) -> Option<f32> {
    match value {
        HkxValue::F32(x) => Some(*x),
        HkxValue::I8(x) => Some(*x as f32),
        HkxValue::I16(x) => Some(*x as f32),
        HkxValue::I32(x) => Some(*x as f32),
        HkxValue::U8(x) => Some(*x as f32),
        HkxValue::U16(x) => Some(*x as f32),
        HkxValue::U32(x) => Some(*x as f32),
        _ => None,
    }
}
pub fn pointer(object: &HkxObject, name: &str) -> Option<usize> {
    match value(object, name)? {
        HkxValue::Pointer(p) => *p,
        _ => None,
    }
}
pub fn array(object: &HkxObject, name: &str) -> Vec<HkxValue> {
    match value(object, name) {
        Some(HkxValue::Array(v)) => v.clone(),
        _ => vec![],
    }
}
pub fn pointers(object: &HkxObject, name: &str) -> Vec<usize> {
    array(object, name)
        .iter()
        .filter_map(|v| match v {
            HkxValue::Pointer(p) => *p,
            _ => None,
        })
        .collect()
}
pub fn set(object: &mut HkxObject, name: &str, value: HkxValue) {
    if let Some(member) = object.members.iter_mut().find(|m| m.name == name) {
        member.value = value;
    } else {
        object.members.push(HkxMember {
            name: name.into(),
            value,
        });
    }
}
pub fn set_string(object: &mut HkxObject, name: &str, value: &str) {
    set(
        object,
        name,
        HkxValue::String {
            value: value.into(),
            is_null: false,
        },
    );
}
pub fn add(file: &mut HkxFile, mut object: HkxObject) -> usize {
    let index = file.objects().len();
    object.name = Some(format!("#B21_{index}"));
    file.push_object(object);
    index
}

pub fn dependencies(
    source: &Path,
    graph: &str,
    cache: &mut BTreeMap<String, HkxFile>,
) -> Result<Vec<String>, String> {
    let mut pending = vec![clean(graph)];
    let mut found = std::collections::BTreeSet::new();
    while let Some(path) = pending.pop() {
        if !found.insert(path.clone()) {
            continue;
        }
        if !cache.contains_key(&path) {
            cache.insert(path.clone(), read(&source.join(&path))?);
        }
        for object in cache[&path].objects() {
            if object.class_name == "hkbBehaviorReferenceGenerator" {
                pending.push(relative_asset(
                    &project_dir(&path),
                    &string(object, "behaviorName"),
                ));
            }
        }
    }
    Ok(found.into_iter().collect())
}

pub fn project_roots(source: &Path, project: &str) -> Result<Vec<String>, String> {
    let file = read(&source.join(project))?;
    let dir = project
        .rsplit_once('/')
        .ok_or("project has no parent directory")?
        .0;
    let mut roots = Vec::new();
    for strings in file
        .objects()
        .iter()
        .filter(|o| o.class_name == "hkbProjectStringData")
    {
        for name in array(strings, "characterFilenames").iter().filter_map(text) {
            let character = read(&source.join(relative_asset(dir, name)))?;
            for strings in character
                .objects()
                .iter()
                .filter(|o| o.class_name == "hkbCharacterStringData")
            {
                roots.push(relative_asset(dir, &string(strings, "behaviorFilename")));
            }
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

pub fn resolve_animation(
    source: &Path,
    graph: &str,
    name: &str,
    paths: &[String],
) -> Option<String> {
    let name = clean(name).replace(".hkt", ".hkx");
    let leaf = name.rsplit('/').next()?;
    paths
        .iter()
        .map(|path| clean(&format!("{path}/{leaf}")))
        .chain(std::iter::once(relative_asset(&project_dir(graph), &name)))
        .find(|candidate| source.join(candidate).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combat_scope_stays_separate_from_furniture() {
        for path in [
            "Actors/Character/Behaviors/MeleeBehavior.hkx",
            "Actors/MoleMiner/Behaviors/MoleMinerRootBehavior.hkx",
            "Actors/Character/_1stPerson/Behaviors/GunBehavior.hkx",
        ] {
            assert!(combat_graph(path));
        }
        for path in [
            "Actors/Character/Behaviors/FurnitureBed.hkx",
            "Actors/Character/Behaviors/FaceGen.hkx",
        ] {
            assert!(!combat_graph(path));
        }
    }
    #[test]
    fn furniture_scope_includes_private_cores_without_claiming_object_graphs() {
        for path in [
            "Actors/Character/Behaviors/WorkbenchFurnitureBehavior.hkx",
            "Actors/Character/Behaviors/FurnitureBed.hkx",
            r"Actors\Character\Behaviors\B21_FO76_workbenchfurniturebehavior_123.hkx",
        ] {
            assert!(furniture_graph(path));
            assert!(preserved_graph(path));
            assert!(!combat_graph(path));
        }
        assert!(!preserved_graph(
            "Furniture/Workstations/WorkbenchTinkers/WorkbenchTinkers.hkx"
        ));
        assert!(!preserved_graph("Actors/Character/Behaviors/FaceGen.hkx"));
    }
    #[test]
    fn resolves_nested_project_paths_without_losing_namespace() {
        assert_eq!(
            project_dir("actors/B21_FO76/moleminer/behaviors/gun_abc.hkx"),
            "actors/b21_fo76/moleminer"
        );
        assert_eq!(
            relative_path(
                "actors/B21_FO76/moleminer",
                "actors/B21_FO76/source/actors/moleminer/animations/run.hkx"
            ),
            "..\\source\\actors\\moleminer\\animations\\run.hkx"
        );
    }
}
