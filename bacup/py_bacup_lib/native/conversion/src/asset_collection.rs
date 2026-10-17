use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use unicode_casefold::UnicodeCaseFold;

use crate::source_inventory::{Inventory, SourceAssetInventory};

#[derive(Default, Deserialize)]
#[serde(default)]
struct Options {
    source_game: String,
    target_game: String,
    source_plugin: PathBuf,
    source_roots: Vec<PathBuf>,
    source_extracted_dir: Option<PathBuf>,
    source_data_dir: Option<PathBuf>,
    extracted_dir: Option<PathBuf>,
    additional_source_asset_roots: Vec<PathBuf>,
    output_plugin_name: String,
    full_plugin_nifs: bool,
    nifs: bool,
    sounds: bool,
    havok: bool,
    textures: bool,
    workers: usize,
    lod_rows: Vec<(String, String, String)>,
    legacy_music_tracks: Vec<Value>,
    mvp_creature_profile: Option<String>,
    mvp_creature_corpus_policy: Option<Value>,
}

impl Options {
    fn roots(&self) -> Vec<PathBuf> {
        let mut roots = self.source_roots.clone();
        roots.extend(
            [
                &self.source_extracted_dir,
                &self.source_data_dir,
                &self.extracted_dir,
            ]
            .into_iter()
            .flatten()
            .cloned(),
        );
        roots.extend(self.additional_source_asset_roots.iter().cloned());
        if !self.source_plugin.as_os_str().is_empty() {
            roots.push(
                self.source_plugin
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
            );
        }
        let mut seen = HashSet::new();
        roots.retain(|root| seen.insert(path_key(root)));
        roots
    }
}

#[derive(Clone, Default, Serialize)]
struct Provenance {
    added_by_record_fk: String,
    added_by_record_eid: String,
    added_by_field: String,
    walk_depth: usize,
    walker_pass: String,
    added_by_record_sig: String,
}

#[derive(Clone, Default, Serialize)]
struct Asset {
    asset_type: String,
    source_path: String,
    resolved_path: Option<String>,
    resolution_error: Option<String>,
    provenance: Option<Provenance>,
    owner_claims: Vec<Value>,
    cloth_companion_claims: Vec<Value>,
    cloth_companion_errors: Vec<String>,
    force_convert: bool,
    force_reason: String,
    output_subpath: Option<String>,
    workshop_wire_point: Option<[f64; 3]>,
}

#[derive(Serialize)]
struct Collection {
    assets: Vec<Asset>,
    timings: Vec<Value>,
    logs: Vec<(String, String)>,
}

fn string(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}

pub(crate) fn normalized(source: &str) -> String {
    let path = source.replace('\\', "/").replace('\0', "");
    let path = path.trim().trim_matches('"');
    let lower = path.to_lowercase();
    let bytes = path.as_bytes();
    let absolute = path.starts_with('/')
        || (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":/");
    let path = if let Some(index) = lower.rfind("/data/") {
        path.chars()
            .skip(lower[..index].chars().count() + 6)
            .collect::<String>()
    } else if lower.starts_with("data/") {
        path[5..].into()
    } else {
        path.into()
    };
    let parts: Vec<_> = path
        .trim_start_matches('/')
        .split('/')
        .map(str::trim)
        .collect();
    let data_root = |part: &str| {
        matches!(
            part.to_lowercase().as_str(),
            "meshes" | "textures" | "materials" | "sound"
        )
    };
    if parts.first().is_some_and(|part| data_root(part)) {
        return parts.join("/");
    }
    if absolute || parts.join("/").to_lowercase().contains("/build/pc/") {
        if let Some(index) = parts.iter().position(|part| data_root(part)) {
            return parts[index..].join("/");
        }
    }
    parts.join("/")
}

fn strip_meshes(source: &str) -> String {
    let path = normalized(source);
    if path.to_lowercase().starts_with("meshes/") {
        path[7..].into()
    } else {
        path
    }
}

fn mesh_key(source: &str) -> String {
    strip_meshes(source).to_lowercase()
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn casefold(value: &str) -> String {
    value.case_fold().collect()
}

fn sound_key(source: &str) -> String {
    let path = source
        .replace('\\', "/")
        .trim()
        .trim_start_matches('/')
        .to_lowercase();
    if let Some(tail) = path.strip_prefix("data/music/") {
        format!("music/{tail}")
    } else if path.starts_with("music/") || path.starts_with("sound/") {
        path
    } else {
        format!("sound/{path}")
    }
}

fn plugin_name(name: &str) -> bool {
    Path::new(name).extension().is_some_and(|ext| {
        matches!(
            ext.to_string_lossy().to_lowercase().as_str(),
            "esm" | "esp" | "esl"
        )
    })
}

fn prefix(kind: &str) -> Option<&'static str> {
    match kind {
        "nif" | "behavior" | "animation" | "support" => Some("Meshes"),
        "texture" => Some("Textures"),
        "material" => Some("Materials"),
        "sound" => Some("Sound"),
        _ => None,
    }
}

struct Collector<'a> {
    inventory: &'a mut Inventory,
    options: Options,
    roots: Vec<PathBuf>,
    legacy_nifs: Option<HashMap<String, Vec<PathBuf>>>,
    timings: Vec<Value>,
    logs: Vec<(String, String)>,
}

impl Collector<'_> {
    fn armor_cloth_companions(&mut self, assets: &mut [Asset]) -> Result<(), String> {
        if !self.options.source_game.eq_ignore_ascii_case("starfield")
            || !self.options.target_game.eq_ignore_ascii_case("fo4")
        {
            return Ok(());
        }
        let selected_owners: HashSet<_> = assets.iter()
            .filter(|asset| asset.asset_type == "nif")
            .filter_map(|asset| asset.resolved_path.as_ref().map(|path| (asset, path)))
            .flat_map(|(asset, path)| asset.owner_claims.iter()
                .filter(|claim| string(claim, "source_record_signature") == "ARMA")
                .map(move |claim| (path_key(Path::new(path)),
                    string(claim, "source_form_key").to_lowercase(),
                    string(claim, "source_subrecord_sig"))))
            .collect();
        let mut nifs = HashMap::new();
        for asset in assets.iter_mut().filter(|a| a.asset_type == "nif") {
            let candidates: Vec<_> = asset.owner_claims.iter()
                .filter(|claim| string(claim, "source_record_signature") == "ARMA")
                .flat_map(|claim| claim.get("armor_family").and_then(Value::as_array)
                    .into_iter().flatten().map(move |candidate| (string(claim, "source_subrecord_sig"), candidate.clone())))
                .collect();
            if candidates.is_empty() { continue }
            let verified = (|| -> Result<Vec<Value>, String> {
            let Some(consumer_path) = &asset.resolved_path else { return Ok(Vec::new()) };
            if !nifs.contains_key(consumer_path) {
                nifs.insert(consumer_path.clone(), nif_core_native::model::NifFile::load(PathBuf::from(consumer_path)).map_err(|e|e.to_string())?);
            }
            if !nifs[consumer_path].find_blocks("BSClothExtraData").is_empty() { return Ok(Vec::new()) }
            let mut claims: HashMap<(String,String), Value> = HashMap::new();
            let mut owners_by_bone = HashMap::new();
            for (role, candidate) in candidates {
                if !matches!(role.as_str(), "MOD2" | "MOD3" | "MOD4" | "MOD5")
                    || string(&candidate,"source_subrecord_sig") != role
                {
                    return Err("Starfield cloth family candidate has a different model role".into());
                }
                let source_path = normalized(&string(&candidate,"source_path"));
                let owner_key = string(&candidate,"owner_form_key");
                let (resolved, error) = self.resolve("nif", &source_path, &owner_key);
                let resolved = resolved.ok_or_else(|| format!("Starfield cloth family owner {owner_key} {source_path} is missing: {}",error.unwrap_or_default()))?;
                if path_key(Path::new(&resolved)) == path_key(Path::new(consumer_path)) { continue }
                if !nifs.contains_key(&resolved) {
                    nifs.insert(resolved.clone(), nif_core_native::model::NifFile::load(PathBuf::from(&resolved)).map_err(|e|e.to_string())?);
                }
                let bones = crate::starfield_armor_closure::verify_companion_bindings(&nifs[consumer_path], &nifs[&resolved])?;
                if bones.is_empty() { continue }
                if !selected_owners.contains(&(path_key(Path::new(&resolved)), owner_key.to_lowercase(), role.clone())) {
                    return Err(format!("Starfield cloth family owner {owner_key} {source_path} is not a selected ARMA asset for {role}"));
                }
                let disk_key = path_key(Path::new(&resolved));
                for bone in &bones {
                    if let Some(previous) = owners_by_bone.insert((role.clone(),bone.clone()),disk_key.clone()) {
                        if previous != disk_key {
                            return Err(format!("ambiguous Starfield cloth companion for {} {role} {bone}",asset.source_path));
                        }
                    }
                }
                let claim = claims.entry((role.clone(),disk_key)).or_insert_with(|| json!({
                    "consumer_field":role,"source_path":source_path,"resolved_path":resolved,
                    "owner_form_key":owner_key,"armor_form_keys":[],"verified_bones":bones,
                }));
                let armor = string(&candidate,"armor_form_key");
                let armors = claim["armor_form_keys"].as_array_mut().unwrap();
                if !armors.iter().any(|v|v.as_str()==Some(&armor)) { armors.push(Value::String(armor)); }
            }
            let mut claims: Vec<_> = claims.into_iter().collect();
            claims.sort_by(|(a,_),(b,_)|a.cmp(b));
            Ok(claims.into_iter().map(|(_,v)|v).collect())
            })();
            match verified {
                Ok(claims) => asset.cloth_companion_claims = claims,
                Err(error) => {
                    self.logs.push(("WARN".into(),format!("Starfield cloth family deferred for {}: {error}",asset.source_path)));
                    asset.cloth_companion_claims.clear();
                    asset.cloth_companion_errors.push(error);
                }
            }
        }
        Ok(())
    }

    fn resolve(
        &mut self,
        kind: &str,
        source: &str,
        form_key: &str,
    ) -> (Option<String>, Option<String>) {
        let relative = source.replace('\\', "/");
        let path = Path::new(&relative);
        if path.is_absolute() {
            return if self.inventory.exists(path, kind == "sound") {
                (Some(path.to_string_lossy().into_owned()), None)
            } else {
                (None, Some(format!("File not found: {}", path.display())))
            };
        }
        let mut raw_candidates = vec![relative.clone()];
        let parts: Vec<_> = relative.trim().trim_start_matches('/').split('/').collect();
        let offset = usize::from(
            parts
                .first()
                .is_some_and(|part| part.eq_ignore_ascii_case("meshes")),
        );
        if parts.len() >= offset + 3
            && parts[offset].eq_ignore_ascii_case("scol")
            && plugin_name(parts[offset + 1])
        {
            let source_name = self
                .options
                .source_plugin
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let form_key = form_key.replace('\\', "/");
            let mut names: Vec<String> = form_key
                .split(':')
                .map(str::trim)
                .find(|part| plugin_name(part))
                .map(str::to_owned)
                .into_iter()
                .collect();
            names.push(source_name.to_string());
            if Path::new(source_name.as_ref()).extension().is_some() {
                names.push(
                    Path::new(source_name.as_ref())
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
            } else {
                names.extend([format!("{source_name}.esm"), format!("{source_name}.esp")]);
            }
            let mut seen = HashSet::new();
            for name in names {
                if name.eq_ignore_ascii_case(parts[offset + 1]) || !seen.insert(name.to_lowercase())
                {
                    continue;
                }
                raw_candidates.push(format!(
                    "{}SCOL/{name}/{}",
                    if offset == 1 { "Meshes/" } else { "" },
                    parts[offset + 2..].join("/")
                ));
            }
        }
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        for candidate in raw_candidates {
            if seen.insert(candidate.to_lowercase()) {
                candidates.push(candidate.clone());
            }
            if let Some(prefix) = prefix(kind) {
                if !candidate
                    .to_lowercase()
                    .starts_with(&format!("{}/", prefix.to_lowercase()))
                {
                    let candidate = format!("{prefix}/{candidate}");
                    if seen.insert(candidate.to_lowercase()) {
                        candidates.push(candidate);
                    }
                }
            }
        }
        let mut tried = Vec::new();
        for root in &self.roots {
            for base in [root.clone(), root.join("Data")] {
                for candidate in &candidates {
                    let full: PathBuf = base.join(candidate).components().collect();
                    tried.push(full.to_string_lossy().into_owned());
                    if self.inventory.exists(&full, kind == "sound") {
                        return (tried.pop(), None);
                    }
                    if kind == "sound" {
                        let ext = full
                            .extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();
                        if ["wav", "xwm", "fuz"].contains(&ext.as_str()) {
                            // Path.with_suffix twice also replaces a dotted stem's last suffix.
                            let stem = full.with_extension("");
                            for alternative in ["wav", "xwm", "fuz"] {
                                if alternative == ext {
                                    continue;
                                }
                                let alt = stem.with_extension(alternative);
                                tried.push(alt.to_string_lossy().into_owned());
                                if self.inventory.exists(&alt, false) {
                                    return (tried.pop(), None);
                                }
                            }
                        }
                    }
                }
            }
        }
        if kind == "nif"
            && relative.to_lowercase().ends_with(".nif")
            && matches!(
                self.options.source_game.to_lowercase().as_str(),
                "fnv" | "fo3"
            )
            && self.options.target_game.eq_ignore_ascii_case("fo4")
        {
            if self.legacy_nifs.is_none() {
                let mut by_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
                let mut seen = HashSet::new();
                for root in self.mesh_roots() {
                    for child in self.sorted_nifs(&root) {
                        let rel = child.strip_prefix(&root).unwrap();
                        if seen.insert(path_key(rel)) {
                            by_name
                                .entry(
                                    child
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_lowercase(),
                                )
                                .or_default()
                                .push(child);
                        }
                    }
                }
                self.legacy_nifs = Some(by_name);
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if let Some(paths) = self.legacy_nifs.as_ref().unwrap().get(&name) {
                if paths.len() == 1 {
                    return (Some(paths[0].to_string_lossy().into_owned()), None);
                }
            }
        }
        if self.roots.is_empty() {
            return (None, Some("Source asset root not configured.".into()));
        }
        let mut preview = tried.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
        if tried.len() > 6 {
            preview.push_str(&format!(" (+{} more)", tried.len() - 6));
        }
        (
            None,
            Some(format!(
                "File not found in source roots. Searched: {preview}"
            )),
        )
    }

    fn mesh_roots(&mut self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        for root in &self.roots {
            for base in [root.clone(), root.join("Data")] {
                let candidates = if base
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("meshes"))
                    && base.is_dir()
                {
                    vec![base]
                } else {
                    self.inventory
                        .child_directories_case_insensitive(&base, "meshes")
                };
                for candidate in candidates {
                    let canonical = candidate
                        .canonicalize()
                        .unwrap_or_else(|_| candidate.clone());
                    if seen.insert(path_key(&canonical)) {
                        roots.push(candidate);
                    }
                }
            }
        }
        roots
    }

    fn sorted_nifs(&mut self, root: &Path) -> Vec<PathBuf> {
        let mut files: Vec<_> = self
            .inventory
            .files(root, &[".nif".into()], false, true, false)
            .into_iter()
            .map(PathBuf::from)
            .collect();
        files.sort_by_cached_key(|path| path_key(path));
        files
    }

    fn record(&mut self, name: &str, start: Instant, mut fields: Value) {
        fields["name"] = name.into();
        fields["elapsed_seconds"] = start.elapsed().as_secs_f64().into();
        self.timings.push(fields);
    }

    fn log(&mut self, message: String) {
        self.logs.push(("INFO".into(), message));
    }

    fn audio(&mut self, assets: &mut Vec<Asset>) {
        if !self.options.source_game.eq_ignore_ascii_case("fo76")
            || !self.options.target_game.eq_ignore_ascii_case("fo4")
        {
            return;
        }
        let source_name = self
            .options
            .source_plugin
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let output = if self.options.output_plugin_name.is_empty() {
            source_name.clone()
        } else {
            self.options.output_plugin_name.clone()
        };
        let output = Path::new(&output)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut seen: HashSet<_> = assets
            .iter()
            .map(|asset| (asset.asset_type.clone(), sound_key(&asset.source_path)))
            .collect();
        for (parts, output_prefix, extensions, pass, sig, label) in [
            (
                vec!["Sound", "Voice", source_name.as_str()],
                format!("Sound/Voice/{output}"),
                vec![".fuz".into(), ".wav".into(), ".xwm".into()],
                "voice_asset_tree",
                "INFO",
                "voice",
            ),
            (
                vec!["Music"],
                "Music".into(),
                vec![".xwm".into(), ".wav".into()],
                "music_asset_tree",
                "MUSC",
                "music",
            ),
            (
                vec!["Sound", "FX"],
                "Sound/FX".into(),
                vec![".xwm".into(), ".wav".into()],
                "sound_fx_asset_tree",
                "SNDR",
                "sound FX",
            ),
        ] {
            let mut roots = Vec::new();
            let mut root_keys = HashSet::new();
            for root in &self.roots {
                for base in [root.clone(), root.join("Data")] {
                    let candidate = parts.iter().try_fold(base, |parent, part| {
                        self.inventory.child_case_insensitive(&parent, part)
                    });
                    if let Some(candidate) = candidate.filter(|path| path.is_dir()) {
                        let key = path_key(
                            &candidate
                                .canonicalize()
                                .unwrap_or_else(|_| candidate.clone()),
                        );
                        if root_keys.insert(key) {
                            roots.push(candidate);
                        }
                    }
                }
            }
            let mut added = 0;
            for root in roots {
                let mut files = self.inventory.files(&root, &extensions, true, true, false);
                files.sort_by_cached_key(|path| {
                    Path::new(path)
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
                        .collect::<Vec<_>>()
                });
                for file in files {
                    let relative = Path::new(&file)
                        .strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    let source_path = format!("{output_prefix}/{relative}");
                    if !seen.insert(("sound".into(), sound_key(&source_path))) {
                        continue;
                    }
                    assets.push(Asset {
                        asset_type: "sound".into(),
                        source_path,
                        resolved_path: Some(file),
                        provenance: Some(Provenance {
                            walker_pass: pass.into(),
                            added_by_record_sig: sig.into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                    added += 1;
                }
            }
            if added > 0 {
                self.log(format!(
                    "Expanded {added} FO76 {label} asset(s) from {output_prefix}"
                ));
            }
        }
    }

    fn nifs(&mut self, assets: &mut Vec<Asset>) {
        let mut seen: HashSet<_> = assets
            .iter()
            .filter(|asset| asset.asset_type == "nif")
            .map(|asset| mesh_key(&asset.source_path))
            .collect();
        if self.options.full_plugin_nifs {
            let start = Instant::now();
            let roots = self.mesh_roots();
            let mut candidates = 0;
            let mut added = 0;
            for root in &roots {
                for path in self.sorted_nifs(root) {
                    candidates += 1;
                    let source = format!(
                        "Meshes/{}",
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/")
                    );
                    if !seen.insert(mesh_key(&source)) {
                        continue;
                    }
                    assets.push(Asset {
                        asset_type: "nif".into(),
                        source_path: source,
                        resolved_path: Some(path.to_string_lossy().into_owned()),
                        provenance: Some(Provenance {
                            added_by_field: "filesystem".into(),
                            walker_pass: "full_plugin_nif_sweep".into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                    added += 1;
                }
            }
            self.log(format!(
                "Expanded {added} full-plugin filesystem NIF(s) from {candidates} candidate(s)"
            ));
            self.record("collect_assets_full_plugin_nifs", start, json!({"roots":roots.len(), "candidates":candidates,"added":added,"workers":self.options.workers}));
        }
        let mut scanned = HashSet::new();
        let mut added = 0;
        for index in 0..assets.len() {
            let asset = &assets[index];
            if asset.asset_type != "nif" {
                continue;
            }
            let Some(resolved) = &asset.resolved_path else {
                continue;
            };
            let source = normalized(&asset.source_path);
            let parts: Vec<_> = source.split('/').filter(|part| !part.is_empty()).collect();
            let Some(component) = parts
                .iter()
                .position(|part| part.eq_ignore_ascii_case("characterassets"))
            else {
                continue;
            };
            let source_dir = parts[..=component].join("/");
            let Some(disk_dir) = Path::new(resolved).parent().and_then(|parent| {
                parent.ancestors().find(|path| {
                    path.file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("characterassets"))
                })
            }) else {
                continue;
            };
            if !disk_dir.is_dir() || !scanned.insert(path_key(disk_dir)) {
                continue;
            }
            let mut provenance = asset.provenance.clone();
            if let Some(provenance) = &mut provenance {
                provenance.walker_pass = "character_assets".into();
            }
            let mut files = self
                .inventory
                .files(disk_dir, &[".nif".into()], true, false, false);
            files.sort_by_cached_key(|path| {
                Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
            });
            for file in files {
                let source_path = format!(
                    "{source_dir}/{}",
                    Path::new(&file).file_name().unwrap().to_string_lossy()
                );
                if !seen.insert(mesh_key(&source_path)) {
                    continue;
                }
                assets.push(Asset {
                    asset_type: "nif".into(),
                    source_path,
                    resolved_path: Some(file),
                    provenance: provenance.clone(),
                    ..Default::default()
                });
                added += 1;
            }
        }
        if added > 0 {
            self.log(format!("Expanded {added} CharacterAssets companion NIF(s)"));
        }
    }

    fn legacy_music(&self, assets: &mut Vec<Asset>) {
        if self.options.legacy_music_tracks.is_empty() {
            return;
        }
        let is_music = |asset: &Asset| {
            matches!(asset.asset_type.as_str(), "sound" | "audio")
                && asset.provenance.as_ref().is_some_and(|provenance| {
                    provenance.added_by_record_sig == "MUSC" && provenance.added_by_field == "FNAM"
                })
        };
        let references: Vec<_> = assets
            .iter()
            .filter(|asset| is_music(asset))
            .map(|asset| legacy_path(&asset.source_path))
            .collect();
        assets.retain(|asset| !is_music(asset));
        let mut seen: HashSet<_> = assets
            .iter()
            .map(|asset| {
                (
                    casefold(&asset.asset_type),
                    casefold(&asset.source_path.replace('\\', "/")),
                )
            })
            .collect();
        for track in &self.options.legacy_music_tracks {
            let relative = legacy_path(&string(track, "legacy_relative_path"));
            if !references.iter().any(|wanted| {
                if wanted.is_empty() {
                    return false;
                }
                if wanted.rsplit('/').next().unwrap_or_default().contains('.') {
                    without_audio_extension(&relative) == without_audio_extension(wanted)
                } else {
                    relative.starts_with(&format!("{}/", wanted.trim_end_matches('/')))
                }
            }) {
                continue;
            }
            let source_path = string(track, "asset_path");
            if !seen.insert(("sound".into(), casefold(&source_path))) {
                continue;
            }
            assets.push(Asset {
                asset_type: "sound".into(),
                source_path,
                resolved_path: Some(string(track, "source_path")),
                provenance: Some(Provenance {
                    added_by_field: "FNAM".into(),
                    walker_pass: "legacy_music_tree".into(),
                    added_by_record_sig: "MUSC".into(),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
    }

    fn creature(&mut self, assets: &mut Vec<Asset>) {
        if self.options.mvp_creature_corpus_policy.is_some() {
            return;
        }
        let (plugin, id, sig, label, entries, outputs): (
            &str,
            u32,
            &str,
            &str,
            &[(&str, &str)],
            &[(&str, &str)],
        ) = match self
            .options
            .mvp_creature_profile
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "skyrim_wolf" => (
                "Skyrim.esm",
                0x04E885,
                "ARMA",
                "Skyrim Wolf",
                &[
                    ("nif", "meshes/actors/canine/character assets wolf/wolf.nif"),
                    (
                        "nif",
                        "meshes/actors/canine/character assets wolf/skeleton.nif",
                    ),
                    (
                        "behavior",
                        "actors/canine/character assets wolf/skeleton.hkx",
                    ),
                    ("behavior", "actors/canine/animations/mt_idle_wolf.hkx"),
                    ("behavior", "actors/canine/animations/walkforward_wolf.hkx"),
                    ("behavior", "actors/canine/animations/turncannedl90.hkx"),
                    ("behavior", "actors/canine/animations/turncannedr90.hkx"),
                    ("behavior", "actors/canine/animations/attack1.hkx"),
                    ("texture", "textures/actors/wolf/wolf.dds"),
                    ("texture", "textures/actors/wolf/wolf_n.dds"),
                    ("texture", "textures/actors/wolf/wolf_sk.dds"),
                ],
                &[
                    (
                        "meshes/actors/canine/character assets wolf/wolf.nif",
                        "Meshes/Actors/B21_SkyrimWolf/CharacterAssets/B21_SkyrimWolf.nif",
                    ),
                    (
                        "meshes/actors/canine/character assets wolf/skeleton.nif",
                        "Meshes/Actors/B21_SkyrimWolf/CharacterAssets/Skeleton.nif",
                    ),
                ],
            ),
            "fnv_gecko" => (
                "FalloutNV.esm",
                0x10CD73,
                "CREA",
                "FNV Gecko",
                &[
                    ("nif", "meshes/creatures/nvgecko/skeleton.nif"),
                    ("nif", "meshes/creatures/nvgecko/nvgecko.nif"),
                    ("kf_animation", "creatures/nvgecko/mtidle.kf"),
                    ("kf_animation", "creatures/nvgecko/swimmtforward.kf"),
                    ("kf_animation", "creatures/nvgecko/mtturnleft.kf"),
                    ("kf_animation", "creatures/nvgecko/mtturnright.kf"),
                    ("kf_animation", "creatures/nvgecko/h2hattackforwardpower.kf"),
                    ("texture", "textures/creatures/gecko/gecko_d.dds"),
                    ("texture", "textures/creatures/gecko/gecko_fire_d.dds"),
                    ("texture", "textures/creatures/gecko/gecko_fire_n.dds"),
                    ("texture", "textures/gore/meatcapgore01.dds"),
                    ("texture", "textures/gore/meatcapgore01_n.dds"),
                ],
                &[
                    (
                        "meshes/creatures/nvgecko/nvgecko.nif",
                        "Meshes/Actors/B21_FNVGecko/CharacterAssets/B21_FNVGecko.nif",
                    ),
                    (
                        "meshes/creatures/nvgecko/skeleton.nif",
                        "Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.nif",
                    ),
                ],
            ),
            _ => return,
        };
        let Some(activator) = assets.iter().find(|asset| {
            creature_key(asset) == (entries[0].0.into(), entries[0].1.into())
                && asset.provenance.as_ref().is_some_and(|provenance| {
                    provenance.added_by_record_sig.eq_ignore_ascii_case(sig)
                        && form_key_matches(&provenance.added_by_record_fk, plugin, id)
                })
        }) else {
            return;
        };
        let provenance = activator.provenance.clone().unwrap();
        let mut wanted: Vec<_> = entries
            .iter()
            .map(|(kind, path)| (kind.to_string(), path.to_string()))
            .collect();
        wanted.sort();
        let mut seen = HashSet::new();
        for asset in assets.iter_mut() {
            let key = creature_key(asset);
            seen.insert(key.clone());
            if !wanted.contains(&key) {
                continue;
            }
            if let Some((_, output)) = outputs.iter().find(|(source, _)| {
                asset.asset_type.eq_ignore_ascii_case("nif") && *source == key.1
            }) {
                asset.output_subpath = Some((*output).into());
            }
            if asset.resolved_path.is_none() {
                (asset.resolved_path, asset.resolution_error) = self.resolve(
                    &asset.asset_type,
                    &asset.source_path,
                    &format!("000000:{plugin}"),
                );
            }
        }
        let mut added = 0;
        for (kind, source) in wanted {
            if seen.contains(&(kind.clone(), source.clone())) {
                continue;
            }
            let (resolved_path, resolution_error) =
                self.resolve(&kind, &source, &format!("000000:{plugin}"));
            let output_subpath = outputs
                .iter()
                .find(|(path, _)| kind == "nif" && *path == source)
                .map(|(_, output)| (*output).into());
            assets.push(Asset {
                asset_type: kind,
                source_path: source,
                resolved_path,
                resolution_error,
                output_subpath,
                provenance: Some(Provenance {
                    added_by_field: "source-rig creature MVP closure".into(),
                    walker_pass: "creature_mvp_asset_closure".into(),
                    ..provenance.clone()
                }),
                ..Default::default()
            });
            added += 1;
        }
        if added > 0 {
            self.log(format!("Expanded {added} exact {label} MVP asset(s)"));
        }
    }

    fn lod(&mut self, assets: &mut Vec<Asset>) {
        let start = Instant::now();
        let mut seen: HashSet<_> = assets
            .iter()
            .map(|asset| {
                (
                    asset.asset_type.clone(),
                    normalized(&asset.source_path).to_lowercase(),
                )
            })
            .collect();
        let mut added = 0;
        for (kind, source, resolved) in &self.options.lod_rows {
            let source = normalized(source);
            if !seen.insert((kind.clone(), source.to_lowercase())) {
                continue;
            }
            assets.push(Asset {
                asset_type: kind.clone(),
                source_path: source,
                resolved_path: (!resolved.is_empty()).then(|| resolved.clone()),
                provenance: Some(Provenance {
                    added_by_field: "MNAM".into(),
                    walker_pass: "lod_convention".into(),
                    ..Default::default()
                }),
                ..Default::default()
            });
            added += 1;
        }
        if !self.options.lod_rows.is_empty() {
            self.record(
                "collect_assets_lod_convention_merge",
                start,
                json!({"candidates":self.options.lod_rows.len(),"added":added}),
            );
        }
    }

    fn havok(&mut self, assets: &mut Vec<Asset>) {
        let start = Instant::now();
        let projects: Vec<_> = assets
            .iter()
            .filter(|asset| asset.asset_type == "behavior")
            .map(|asset| strip_meshes(&asset.source_path))
            .filter(|source| {
                let parts: Vec<_> = source.split('/').collect();
                source.to_lowercase().ends_with(".hkx")
                    && !parts[..parts.len().saturating_sub(1)].iter().any(|part| {
                        matches!(
                            part.to_lowercase().as_str(),
                            "animations" | "behaviors" | "characterassets" | "characters"
                        )
                    })
            })
            .collect();
        if projects.is_empty() || self.roots.is_empty() {
            return;
        }
        let mut seen: HashSet<_> = assets
            .iter()
            .map(|asset| mesh_key(&asset.source_path))
            .collect();
        let mut candidates = 0;
        let mut added = 0;
        let mut projects_expanded = 0;
        for project in &projects {
            let mut project_added = 0;
            let Some(parent) = Path::new(project)
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
            else {
                continue;
            };
            let mut companions = Vec::new();
            for root in &self.roots {
                let mut mesh_root = root.join("meshes");
                let mut base = mesh_root.join(parent);
                if !base.is_dir() {
                    let alternate = root.join("Meshes");
                    if alternate.join(parent).is_dir() {
                        mesh_root = alternate;
                        base = mesh_root.join(parent);
                    }
                }
                if !base.is_dir() {
                    continue;
                }
                for file in self
                    .inventory
                    .files(&base, &[".hkx".into()], true, true, true)
                {
                    let source = Path::new(&file)
                        .strip_prefix(&mesh_root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    if source.to_lowercase() == project.to_lowercase() {
                        continue;
                    }
                    companions.push(source);
                }
            }
            candidates += companions.len();
            for source in companions {
                let source_path = normalized(&source);
                if !seen.insert(mesh_key(&source_path)) {
                    continue;
                }
                let parts: Vec<_> = source_path.split('/').collect();
                let kind = if parts[..parts.len().saturating_sub(1)]
                    .iter()
                    .any(|part| part.eq_ignore_ascii_case("animations"))
                {
                    "animation"
                } else {
                    "behavior"
                };
                let (resolved_path, resolution_error) = self.resolve(kind, &source_path, "");
                assets.push(Asset {
                    asset_type: kind.into(),
                    source_path,
                    resolved_path,
                    resolution_error,
                    ..Default::default()
                });
                project_added += 1;
            }
            if project_added > 0 {
                projects_expanded += 1;
                added += project_added;
            }
        }
        self.log(format!("Expanded {added} Havok companion assets from {projects_expanded} behavior project(s) ({candidates} candidate(s))"));
        self.record("collect_assets_havok_bundles", start, json!({"projects":projects.len(),"projects_expanded":projects_expanded,"candidates":candidates,"added":added,"workers":self.options.workers}));
    }

    fn map(&mut self, assets: &mut Vec<Asset>) {
        if !self.options.source_game.eq_ignore_ascii_case("fo76")
            || !self.options.target_game.eq_ignore_ascii_case("fo4")
        {
            return;
        }
        let output = "textures/interface/pip-boy/papermap_city_d.dds";
        if assets
            .iter()
            .filter(|asset| asset.asset_type.eq_ignore_ascii_case("texture"))
            .any(|asset| {
                let path = asset.output_subpath.clone().unwrap_or_else(|| {
                    let path = normalized(&asset.source_path);
                    if path.to_lowercase().starts_with("textures/") {
                        path
                    } else {
                        format!("textures/{path}")
                    }
                });
                path.replace('\\', "/").eq_ignore_ascii_case(output)
            })
        {
            return;
        }
        let source = "Interface/Pip-Boy/papermap_city_d.dds";
        let (resolved_path, resolution_error) = self.resolve("texture", source, "");
        if resolved_path.is_some() {
            self.log("Queued FO76 Appalachia Pip-Boy map texture".into());
        } else {
            self.logs.push((
                "WARN".into(),
                format!(
                    "FO76 Appalachia Pip-Boy map texture not found: {}",
                    resolution_error.as_deref().unwrap_or_default()
                ),
            ));
        }
        assets.push(Asset {
            asset_type: "texture".into(),
            source_path: source.into(),
            resolved_path,
            resolution_error,
            provenance: Some(Provenance {
                added_by_record_eid: "APPALACHIA".into(),
                added_by_field: "WRLD.ICON".into(),
                walker_pass: "fo76_pipboy_map".into(),
                added_by_record_sig: "WRLD".into(),
                ..Default::default()
            }),
            force_convert: true,
            force_reason: "FO76 Appalachia Pip-Boy map image".into(),
            output_subpath: Some(output.into()),
            ..Default::default()
        });
    }
}

pub(crate) fn collect_json(
    inventory: &SourceAssetInventory,
    raw_json: &str,
    options_json: &str,
) -> Result<String, String> {
    let raw: Vec<Value> = serde_json::from_str(raw_json).map_err(|error| error.to_string())?;
    let options: Options = serde_json::from_str(options_json).map_err(|error| error.to_string())?;
    let roots = options.roots();
    let mut inventory = inventory.inner.lock().unwrap();
    let mut collector = Collector {
        inventory: &mut inventory,
        options,
        roots,
        legacy_nifs: None,
        timings: Vec::new(),
        logs: Vec::new(),
    };
    let start = Instant::now();
    let mut assets = Vec::with_capacity(raw.len());
    for row in raw {
        let asset_type = string(&row, "asset_type");
        let source_path = normalized(&string(&row, "source_path"));
        let form_key = string(&row, "source_form_key");
        let (resolved_path, resolution_error) =
            collector.resolve(&asset_type, &source_path, &form_key);
        let workshop_wire_point = row
            .get("workshop_wire_point")
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        assets.push(Asset {
            asset_type,
            source_path,
            resolved_path,
            resolution_error,
            workshop_wire_point,
            owner_claims: row
                .get("owner_claims")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            provenance: Some(Provenance {
                added_by_record_fk: form_key,
                added_by_field: string(&row, "source_subrecord_sig"),
                walker_pass: "native_asset_collect".into(),
                added_by_record_sig: string(&row, "source_record_signature"),
                ..Default::default()
            }),
            ..Default::default()
        });
    }
    collector.record(
        "collect_assets_reference_paths",
        start,
        json!({"assets":assets.len()}),
    );
    if collector.options.sounds {
        let audio_start = Instant::now();
        collector.audio(&mut assets);
        collector.legacy_music(&mut assets);
        collector.record(
            "collect_assets_audio_expansion",
            audio_start,
            json!({"assets":assets.len()}),
        );
    }
    collector.record(
        "collect_assets_resolve_paths",
        start,
        json!({"assets":assets.len()}),
    );
    if collector.options.nifs {
        collector.nifs(&mut assets);
        collector.armor_cloth_companions(&mut assets)?;
    }
    collector.lod(&mut assets);
    if collector.options.havok {
        collector.havok(&mut assets);
    }
    collector.creature(&mut assets);
    if collector.options.textures {
        collector.map(&mut assets);
    }
    serde_json::to_string(&Collection {
        assets,
        timings: collector.timings,
        logs: collector.logs,
    })
    .map_err(|error| error.to_string())
}

pub(crate) fn resolve_asset(
    inventory: &SourceAssetInventory,
    kind: &str,
    source: &str,
    options_json: &str,
    source_names: &[String],
) -> Result<(Option<String>, Option<String>), String> {
    let options: Options = serde_json::from_str(options_json).map_err(|error| error.to_string())?;
    let roots = options.roots();
    let mut inventory = inventory.inner.lock().unwrap();
    let mut collector = Collector {
        inventory: &mut inventory,
        options,
        roots,
        legacy_nifs: None,
        timings: Vec::new(),
        logs: Vec::new(),
    };
    Ok(collector.resolve(kind, source, &source_names.join(":")))
}

fn legacy_path(source: &str) -> String {
    let path = casefold(source.trim().replace('\\', "/").trim_matches('/'));
    path.strip_prefix("data/music/")
        .or_else(|| path.strip_prefix("music/"))
        .unwrap_or(&path)
        .trim_matches('/')
        .into()
}

fn without_audio_extension(path: &str) -> &str {
    path.rsplit_once('.')
        .filter(|(_, ext)| matches!(*ext, "mp3" | "ogg" | "wav" | "xwm"))
        .map_or(path, |(stem, _)| stem)
}

fn creature_key(asset: &Asset) -> (String, String) {
    let kind = casefold(&asset.asset_type);
    let mut path = casefold(&normalized(&asset.source_path));
    let prefix = match kind.as_str() {
        "nif" => Some("meshes"),
        "texture" => Some("textures"),
        _ => None,
    };
    if let Some(prefix) = prefix {
        if path.split('/').next() != Some(prefix) {
            path = format!("{prefix}/{path}");
        }
    }
    (kind, path)
}

fn form_key_matches(key: &str, plugin: &str, id: u32) -> bool {
    let parts: Vec<_> = key.trim().split([':', '@']).map(str::trim).collect();
    if parts.len() != 2 {
        return false;
    }
    [(parts[0], parts[1]), (parts[1], parts[0])]
        .into_iter()
        .any(|(number, name)| {
            let number = number.strip_prefix("0x").unwrap_or(number);
            !number.is_empty()
                && number.len() <= 8
                && u32::from_str_radix(number, 16) == Ok(id)
                && Path::new(name)
                    .file_name()
                    .is_some_and(|name| casefold(&name.to_string_lossy()) == casefold(plugin))
        })
}

#[derive(Deserialize)]
struct LegacyMusicOptions {
    source_game: String,
    target_game: String,
    output_plugin_name: String,
    primary_roots: Vec<PathBuf>,
    additional_roots: Vec<PathBuf>,
}

pub(crate) fn discover_legacy_music_json(
    inventory: &SourceAssetInventory,
    options_json: &str,
) -> Result<String, String> {
    let options: LegacyMusicOptions =
        serde_json::from_str(options_json).map_err(|error| error.to_string())?;
    if !matches!(options.source_game.to_lowercase().as_str(), "fnv" | "fo3")
        || !options.target_game.eq_ignore_ascii_case("fo4")
    {
        return Ok("[]".into());
    }
    let stem = Path::new(&options.output_plugin_name)
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Converted".into());
    let mut inventory = inventory.inner.lock().unwrap();
    let mut rows = Vec::new();
    let mut seen_roots = HashSet::new();
    let mut seen_targets = HashSet::new();
    let grafted = if options.source_game.eq_ignore_ascii_case("fnv") {
        "FO3"
    } else {
        "GRAFTED"
    };
    for (tag, roots) in [
        (options.source_game.to_uppercase(), options.primary_roots),
        (grafted.into(), options.additional_roots),
    ] {
        for root in roots {
            for base in [root.clone(), root.join("Data")] {
                for (parts, prefix) in [
                    (vec!["music"], ""),
                    (vec!["sound", "fx", "mus"], "Sound/fx/mus"),
                ] {
                    let candidate = parts.iter().try_fold(base.clone(), |parent, part| {
                        let direct = parent.join(part);
                        if direct.is_dir() {
                            Some(direct)
                        } else {
                            inventory
                                .child_case_folded(&parent, part)
                                .filter(|path| path.is_dir())
                        }
                    });
                    let Some(root) = candidate else {
                        continue;
                    };
                    if !seen_roots.insert(path_key(
                        &root.canonicalize().unwrap_or_else(|_| root.clone()),
                    )) {
                        continue;
                    }
                    let mut files = inventory.files(
                        &root,
                        &[".mp3".into(), ".ogg".into(), ".wav".into(), ".xwm".into()],
                        true,
                        true,
                        false,
                    );
                    files.sort_by_cached_key(|path| casefold(&path.replace('\\', "/")));
                    for source in files {
                        let relative = Path::new(&source).strip_prefix(&root).unwrap();
                        let mut target = Path::new(prefix).join(relative);
                        if target.extension().is_some_and(|ext| {
                            matches!(
                                ext.to_string_lossy().to_lowercase().as_str(),
                                "mp3" | "ogg" | "wav"
                            )
                        }) {
                            target.set_extension("xwm");
                        }
                        let asset_path = Path::new("Music").join(&stem).join(&tag).join(target);
                        if !seen_targets
                            .insert(casefold(&asset_path.to_string_lossy().replace('\\', "/")))
                        {
                            continue;
                        }
                        rows.push(json!({"legacy_relative_path":relative.to_string_lossy().replace('\\',"/"),"source_path":source,"asset_path":asset_path.to_string_lossy().replace('\\',"/"),"track_path":format!("Data\\{}",asset_path.with_extension("wav").to_string_lossy().replace('/',"\\"))}));
                    }
                }
            }
        }
    }
    serde_json::to_string(&rows).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn file(root: &Path, relative: &str) -> PathBuf {
        let path: PathBuf = root.join(relative).components().collect();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"fixture").unwrap();
        path
    }

    fn collect(raw: Value, options: Value) -> Value {
        serde_json::from_str(
            &collect_json(
                &SourceAssetInventory::default(),
                &raw.to_string(),
                &options.to_string(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn source_root_precedence_scol_aliases_audio_alternatives_and_errors() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("primary");
        let grafted = temp.path().join("grafted");
        let preferred = file(&primary, "Meshes/Clutter/shared.nif");
        file(&grafted, "Meshes/Clutter/shared.nif");
        let alias = file(&grafted, "Data/Meshes/SCOL/Original.esm/group.nif");
        let sound = file(&primary, "Sound/FX/a.xwm");
        fs::create_dir_all(primary.join("Sound/FX/folder")).unwrap();
        let result = collect(
            json!([
                {"asset_type":"nif","source_path":"Clutter/shared.nif"},
                {"asset_type":"nif","source_path":"SCOL/Merged.esm/group.nif","source_form_key":"000123:Original.esm"},
                {"asset_type":"sound","source_path":"FX/a.wav"},
                {"asset_type":"sound","source_path":"FX/folder"},
                {"asset_type":"nif","source_path":"missing.nif"}
            ]),
            json!({"source_roots":[primary,grafted],"source_plugin":temp.path().join("Merged.esm")}),
        );
        let assets = result["assets"].as_array().unwrap();
        assert_eq!(
            assets[0]["resolved_path"],
            preferred.to_string_lossy().as_ref()
        );
        assert_eq!(assets[1]["resolved_path"], alias.to_string_lossy().as_ref());
        assert_eq!(assets[2]["resolved_path"], sound.to_string_lossy().as_ref());
        assert!(assets[3]["resolved_path"].is_string());
        assert!(assets[4]["resolved_path"].is_null());
        assert!(
            assets[4]["resolution_error"]
                .as_str()
                .unwrap()
                .contains("(+6 more)")
        );
    }

    #[test]
    fn normalizes_source_paths_and_preserves_wire_point_float_bits() {
        assert_eq!(
            normalized("\" C:/Data/Materials/ nonascii_\u{06b9}.BGSM\0\""),
            "Materials/nonascii_\u{06b9}.BGSM"
        );
        assert_eq!(
            normalized("C:/Projects/Build/PC/Textures/Test.dds"),
            "Textures/Test.dds"
        );
        assert_eq!(
            normalized("C:/İ/Data/Textures/test.dds"),
            "extures/test.dds"
        );
        let raw = r#"[{"asset_type":"nif","source_path":"test.nif","workshop_wire_point":[-4.371138828673793e-08,90.80000305175781,-0.0]}]"#;
        let result: Value = serde_json::from_str(
            &collect_json(&SourceAssetInventory::default(), raw, "{}").unwrap(),
        )
        .unwrap();
        assert_eq!(
            result["assets"][0]["workshop_wire_point"][0]
                .as_f64()
                .unwrap()
                .to_bits(),
            (-4.371138828673793e-08_f64).to_bits()
        );
        assert_eq!(
            result["assets"][0]["workshop_wire_point"][1]
                .as_f64()
                .unwrap()
                .to_bits(),
            90.80000305175781_f64.to_bits()
        );
        assert_eq!(
            result["assets"][0]["workshop_wire_point"][2]
                .as_f64()
                .unwrap()
                .to_bits(),
            (-0.0_f64).to_bits()
        );
    }

    #[test]
    fn shared_armor_model_retains_each_source_perspective() {
        let owners = json!([
            {"source_form_key":"Skyrim.esm:0139A2","source_record_signature":"ARMA","source_subrecord_sig":"MOD2"},
            {"source_form_key":"Skyrim.esm:0139A2","source_record_signature":"ARMA","source_subrecord_sig":"MOD4"}
        ]);
        let result = collect(
            json!([{"asset_type":"nif","source_path":"Armor/Shared.nif","source_record_signature":"ARMA","source_subrecord_sig":"MOD2","owner_claims":owners}]),
            json!({"source_game":"skyrimse","target_game":"fo4"}),
        );
        assert_eq!(result["assets"][0]["owner_claims"], owners);
    }

    #[test]
    #[ignore = "requires extracted Starfield source outfits"]
    fn starfield_armor_cloth_collection_verifies_role_and_defers_conflicts() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../extracted/starfield");
        let consumer = "Clothes/outfit_ryujin_businesssuit_manager_02/Outfit_Ryujin_BusinessSuit_Manager_UpperBody_02_F_1st.nif";
        let owner = "Clothes/outfit_ryujin_businesssuit_manager_02/Outfit_Ryujin_BusinessSuit_Manager_Sleeves_02_F_1st.nif";
        let options = json!({"source_game":"starfield","target_game":"fo4","source_roots":[source],"nifs":true});
        let raw = json!([{"asset_type":"nif","source_path":consumer,"source_form_key":"Starfield.esm:194280", "owner_claims":[{
            "source_form_key":"Starfield.esm:194280","source_record_signature":"ARMA","source_subrecord_sig":"MOD5",
            "armor_family":[{"armor_form_key":"Starfield.esm:19F9C1","owner_form_key":"Starfield.esm:194281","source_subrecord_sig":"MOD5","source_path":owner}]
        }]},{"asset_type":"nif","source_path":owner,"owner_claims":[{
            "source_form_key":"Starfield.esm:194281","source_record_signature":"ARMA","source_subrecord_sig":"MOD5"
        }]}]);
        let result = collect(raw.clone(),options.clone());
        let claims = result["assets"][0]["cloth_companion_claims"].as_array().unwrap();
        assert_eq!(claims.len(),1);
        assert_eq!(claims[0]["consumer_field"],"MOD5");
        assert_eq!(claims[0]["verified_bones"],json!(["L_Cloth_Sleeve","R_Cloth_Sleeve"]));
        assert!(result["assets"][0]["cloth_companion_errors"].as_array().unwrap().is_empty());

        let mut bounded = raw.clone();
        bounded.as_array_mut().unwrap().truncate(1);
        let result = collect(bounded,options.clone());
        assert!(result["assets"][0]["cloth_companion_claims"].as_array().unwrap().is_empty());
        assert!(result["assets"][0]["cloth_companion_errors"][0].as_str().unwrap().contains("not a selected ARMA asset for MOD5"));

        let mut wrong_owner_role = raw.clone();
        wrong_owner_role[1]["owner_claims"][0]["source_subrecord_sig"] = json!("MOD3");
        let result = collect(wrong_owner_role,options.clone());
        assert!(result["assets"][0]["cloth_companion_claims"].as_array().unwrap().is_empty());
        assert!(result["assets"][0]["cloth_companion_errors"][0].as_str().unwrap().contains("not a selected ARMA asset for MOD5"));

        let mut wrong_owner_record = raw.clone();
        wrong_owner_record[1]["owner_claims"][0]["source_form_key"] = json!("Starfield.esm:194280");
        let result = collect(wrong_owner_record,options.clone());
        assert!(result["assets"][0]["cloth_companion_claims"].as_array().unwrap().is_empty());
        assert!(result["assets"][0]["cloth_companion_errors"][0].as_str().unwrap().contains("not a selected ARMA asset for MOD5"));

        let mut wrong_role = raw.clone();
        wrong_role[0]["owner_claims"][0]["armor_family"][0]["source_subrecord_sig"] = json!("MOD3");
        let result = collect(wrong_role,options.clone());
        assert!(result["assets"][0]["cloth_companion_claims"].as_array().unwrap().is_empty());
        assert!(result["assets"][0]["cloth_companion_errors"][0].as_str().unwrap().contains("different model role"));
        assert!(result["logs"].as_array().unwrap().iter().any(|log|log[0]=="WARN"));

        let mut missing = raw.clone();
        missing[0]["owner_claims"][0]["armor_family"][0]["source_path"] = json!("Clothes/DoesNotExist.nif");
        let result = collect(missing,options.clone());
        assert!(result["assets"][0]["cloth_companion_claims"].as_array().unwrap().is_empty());
        assert!(result["assets"][0]["cloth_companion_errors"][0].as_str().unwrap().contains("is missing"));

        let temp = tempfile::tempdir().unwrap();
        let duplicate = temp.path().join("other_owner.nif");
        std::fs::copy(source.join("meshes").join(owner),&duplicate).unwrap();
        let mut ambiguous = raw.clone();
        let mut second = ambiguous[0]["owner_claims"][0]["armor_family"][0].clone();
        second["source_path"] = json!(duplicate);
        second["owner_form_key"] = json!("Starfield.esm:000802");
        ambiguous[0]["owner_claims"][0]["armor_family"].as_array_mut().unwrap().push(second);
        ambiguous.as_array_mut().unwrap().push(json!({"asset_type":"nif","source_path":duplicate,"owner_claims":[{
            "source_form_key":"Starfield.esm:000802","source_record_signature":"ARMA","source_subrecord_sig":"MOD5"
        }]}));
        let result = collect(ambiguous,options.clone());
        assert!(result["assets"][0]["cloth_companion_claims"].as_array().unwrap().is_empty());
        assert!(result["assets"][0]["cloth_companion_errors"][0].as_str().unwrap().contains("ambiguous"));

        let mut no_supplier = raw;
        no_supplier[0]["owner_claims"][0]["armor_family"][0]["source_path"] = json!(consumer);
        let result = collect(no_supplier,options);
        assert!(result["assets"][0]["cloth_companion_claims"].as_array().unwrap().is_empty());
    }

    #[test]
    fn full_inventory_keeps_order_provenance_and_deduplication() {
        let temp = tempfile::tempdir().unwrap();
        file(temp.path(), "Meshes/Actors/CharacterAssets/body.nif");
        file(temp.path(), "Meshes/Actors/CharacterAssets/skeleton.nif");
        file(temp.path(), "Meshes/Actors/Test.hkx");
        file(temp.path(), "Meshes/Actors/Behaviors/graph.hkx");
        file(temp.path(), "Meshes/Actors/Animations/idle.hkx");
        file(temp.path(), "Sound/Voice/Source.esm/line.fuz");
        file(temp.path(), "Music/track.xwm");
        file(
            temp.path(),
            "Textures/Interface/Pip-Boy/papermap_city_d.dds",
        );
        let result = collect(
            json!([
                {"asset_type":"nif","source_path":"Actors/CharacterAssets/body.nif"},
                {"asset_type":"behavior","source_path":"Actors/Test.hkx"}
            ]),
            json!({"source_game":"fo76","target_game":"fo4","source_roots":[temp.path()],"source_plugin":"Source.esm","output_plugin_name":"Output.esm","sounds":true,"nifs":true,"full_plugin_nifs":true,"havok":true,"textures":true}),
        );
        let assets = result["assets"].as_array().unwrap();
        assert_eq!(assets.len(), 8);
        assert_eq!(assets[2]["source_path"], "Sound/Voice/Output.esm/line.fuz");
        assert_eq!(assets[3]["source_path"], "Music/track.xwm");
        assert_eq!(
            assets[4]["source_path"],
            "Meshes/Actors/CharacterAssets/skeleton.nif"
        );
        assert_eq!(
            assets[4]["provenance"]["walker_pass"],
            "full_plugin_nif_sweep"
        );
        assert!(
            assets[5..7]
                .iter()
                .any(|asset| asset["asset_type"] == "animation")
        );
        assert_eq!(assets[7]["force_convert"], true);
        assert_eq!(
            assets[7]["output_subpath"],
            "textures/interface/pip-boy/papermap_city_d.dds"
        );
    }

    #[test]
    fn legacy_nif_fallback_rejects_ambiguous_relative_paths() {
        let temp = tempfile::tempdir().unwrap();
        let unique = file(temp.path(), "Meshes/Actual/unique.nif");
        file(temp.path(), "Meshes/Actual/shared.nif");
        file(temp.path(), "Meshes/Other/shared.nif");
        for source_game in ["fnv", "fo3"] {
            let result = collect(
                json!([
                    {"asset_type":"nif","source_path":"Wrong/unique.nif"},
                    {"asset_type":"nif","source_path":"Wrong/shared.nif"}
                ]),
                json!({"source_game":source_game,"target_game":"fo4","source_roots":[temp.path()]}),
            );
            assert_eq!(
                result["assets"][0]["resolved_path"],
                unique.to_string_lossy().as_ref()
            );
            assert!(result["assets"][1]["resolved_path"].is_null());
        }
    }

    #[test]
    fn exact_creature_closure_requires_profile_signature_and_form_identity() {
        let raw = json!([{"asset_type":"nif","source_path":"Actors/Canine/Character Assets Wolf/Wolf.nif","source_form_key":"Skyrim.esm@0004E885","source_record_signature":"ARMA"}]);
        let result = collect(raw.clone(), json!({"mvp_creature_profile":"skyrim_wolf"}));
        assert_eq!(result["assets"].as_array().unwrap().len(), 11);
        assert_eq!(
            result["assets"][0]["output_subpath"],
            "Meshes/Actors/B21_SkyrimWolf/CharacterAssets/B21_SkyrimWolf.nif"
        );
        let disabled = collect(
            raw.clone(),
            json!({"mvp_creature_profile":"skyrim_wolf","mvp_creature_corpus_policy":{}}),
        );
        assert_eq!(disabled["assets"].as_array().unwrap().len(), 1);
        let wrong = collect(raw, json!({"mvp_creature_profile":"fnv_gecko"}));
        assert_eq!(wrong["assets"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn legacy_music_discovery_and_reference_selection_preserve_primary_and_graft_order() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("fnv");
        let grafted = temp.path().join("fo3");
        file(&primary, "Music/Explore/Same.mp3");
        file(&grafted, "Music/Explore/Same.mp3");
        file(&primary, "Sound/fx/mus/Battle/Unused.ogg");
        let tracks: Value = serde_json::from_str(&discover_legacy_music_json(&SourceAssetInventory::default(),&json!({"source_game":"fnv","target_game":"fo4","output_plugin_name":"Port.esm","primary_roots":[primary],"additional_roots":[grafted]}).to_string()).unwrap()).unwrap();
        assert_eq!(tracks[0]["asset_path"], "Music/Port/FNV/Explore/Same.xwm");
        assert_eq!(
            tracks[1]["asset_path"],
            "Music/Port/FNV/Sound/fx/mus/Battle/Unused.xwm"
        );
        assert_eq!(tracks[2]["asset_path"], "Music/Port/FO3/Explore/Same.xwm");
        let result = collect(
            json!([{"asset_type":"sound","source_path":"Data/Music/Explore/","source_record_signature":"MUSC","source_subrecord_sig":"FNAM"}]),
            json!({"sounds":true,"legacy_music_tracks":tracks}),
        );
        assert_eq!(result["assets"].as_array().unwrap().len(), 2);
        assert_eq!(
            result["assets"][0]["source_path"],
            "Music/Port/FNV/Explore/Same.xwm"
        );
        assert_eq!(
            result["assets"][1]["source_path"],
            "Music/Port/FO3/Explore/Same.xwm"
        );
    }

    #[cfg(windows)]
    #[test]
    fn legacy_casefold_root_lookup_accepts_directory_links() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = temp.path().join("Muſic");
        if std::os::windows::fs::symlink_dir(&target, &link).is_err() {
            return;
        }
        let mut inventory = Inventory::default();
        assert_eq!(
            inventory.child_case_folded(temp.path(), "music"),
            Some(link)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn mesh_root_discovery_ignores_case_alias_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("meshes"), b"not a directory").unwrap();
        let mesh = file(temp.path(), "Meshes/actual.nif");
        let result = collect(
            json!([]),
            json!({"source_roots":[temp.path()],"nifs":true,"full_plugin_nifs":true}),
        );
        assert_eq!(
            result["assets"][0]["resolved_path"],
            mesh.to_string_lossy().as_ref()
        );
    }
}
