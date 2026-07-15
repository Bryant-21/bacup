use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::{Duration, SystemTime};

use nif_core_native::model::NifFile;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use rusqlite::{Connection, OptionalExtension, params};

use crate::relocation::normalize_rel;

pub const CATALOG_SCHEMA_VERSION: i64 = 2;

#[derive(Clone, Hash, PartialEq, Eq)]
struct TargetAssetStoreKey {
    target_data_dir: PathBuf,
    catalog_path: PathBuf,
    cache_dir: PathBuf,
    overlay_dir: Option<PathBuf>,
}

static SHARED_TARGET_ASSET_STORES: LazyLock<
    Mutex<HashMap<TargetAssetStoreKey, Weak<TargetAssetStore>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
struct ArchiveSpec {
    name: String,
    content_pack: String,
    required: bool,
    expected_size: u64,
    priority: i64,
}

#[derive(Debug, Clone)]
struct AssetEntry {
    canonical_path: String,
    archive_path: PathBuf,
    priority: i64,
    catalog_dependencies_valid: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TargetAssetStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub files_extracted: u64,
    pub bytes_extracted: u64,
    pub archives_reindexed: u64,
}

pub struct TargetAssetStore {
    assets: HashMap<String, AssetEntry>,
    dependencies: HashMap<String, Vec<String>>,
    overlay_files: HashMap<String, PathBuf>,
    cache_data_root: PathBuf,
    membership_root: PathBuf,
    archive_readers: Mutex<HashMap<PathBuf, Arc<Mutex<bsarchive_native::python::ArchiveReader>>>>,
    warnings: Vec<String>,
    stats: Mutex<TargetAssetStats>,
}

impl TargetAssetStore {
    pub fn open_shared(
        target_data_dir: &Path,
        catalog_path: &Path,
        cache_dir: &Path,
        overlay_dir: Option<&Path>,
    ) -> Result<Arc<Self>, String> {
        Self::open_shared_with_status(target_data_dir, catalog_path, cache_dir, overlay_dir)
            .map(|(store, _)| store)
    }

    fn open_shared_with_status(
        target_data_dir: &Path,
        catalog_path: &Path,
        cache_dir: &Path,
        overlay_dir: Option<&Path>,
    ) -> Result<(Arc<Self>, bool), String> {
        let key = TargetAssetStoreKey {
            target_data_dir: target_data_dir.to_path_buf(),
            catalog_path: catalog_path.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            overlay_dir: overlay_dir.map(Path::to_path_buf),
        };
        if let Some(store) = SHARED_TARGET_ASSET_STORES
            .lock()
            .map_err(|_| "target asset store registry lock poisoned".to_string())?
            .get(&key)
            .and_then(Weak::upgrade)
        {
            return Ok((store, false));
        }
        let store = Arc::new(Self::open(
            target_data_dir,
            catalog_path,
            cache_dir,
            overlay_dir,
        )?);
        let mut stores = SHARED_TARGET_ASSET_STORES
            .lock()
            .map_err(|_| "target asset store registry lock poisoned".to_string())?;
        if let Some(existing) = stores.get(&key).and_then(Weak::upgrade) {
            return Ok((existing, false));
        }
        stores.insert(key, Arc::downgrade(&store));
        Ok((store, true))
    }

    pub fn open(
        target_data_dir: &Path,
        catalog_path: &Path,
        cache_dir: &Path,
        overlay_dir: Option<&Path>,
    ) -> Result<Self, String> {
        if !target_data_dir.is_dir() {
            return Err(format!(
                "target asset store: FO4 Data directory is missing: {}",
                target_data_dir.display()
            ));
        }
        if !catalog_path.is_file() {
            return Err(format!(
                "target asset store: packaged catalog is missing: {}",
                catalog_path.display()
            ));
        }

        let connection = Connection::open_with_flags(
            catalog_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| format!("open target asset catalog: {error}"))?;
        let schema_version: i64 = connection
            .query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| format!("read target asset catalog schema: {error}"))?
            .parse()
            .map_err(|error| format!("parse target asset catalog schema: {error}"))?;
        if schema_version != CATALOG_SCHEMA_VERSION {
            return Err(format!(
                "target asset catalog schema {schema_version} is unsupported; expected {CATALOG_SCHEMA_VERSION}"
            ));
        }

        let archive_specs = load_archive_specs(&connection)?;
        let installed = installed_archives(target_data_dir)?;
        let mut active_archives: HashMap<String, (PathBuf, bool, i64)> = HashMap::new();
        let mut warnings = Vec::new();
        let mut reindexed = 0u64;
        for spec in &archive_specs {
            let key = spec.name.to_ascii_lowercase();
            let Some(path) = installed.get(&key) else {
                if spec.required {
                    return Err(format!(
                        "target asset store: required FO4 archive is missing: {} ({})",
                        spec.name, spec.content_pack
                    ));
                }
                continue;
            };
            let actual_size = path
                .metadata()
                .map_err(|error| format!("stat {}: {error}", path.display()))?
                .len();
            let catalog_valid = spec.expected_size == 0 || actual_size == spec.expected_size;
            if !catalog_valid {
                reindexed += 1;
                warnings.push(format!(
                    "target asset store: {} size changed (catalog={}, installed={}); runtime index used",
                    spec.name, spec.expected_size, actual_size
                ));
            }
            active_archives.insert(key, (path.clone(), catalog_valid, spec.priority));
        }

        let mut assets = load_catalog_assets(&connection, &active_archives)?;
        let mut mismatch_dependency_members: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
        for (archive_path, catalog_valid, priority) in active_archives.values() {
            if *catalog_valid {
                continue;
            }
            let files = bsarchive_native::list_archive_files(archive_path)
                .map_err(|error| format!("index {}: {error}", archive_path.display()))?;
            for canonical_path in files {
                let key = normalize_rel(&canonical_path);
                if key.is_empty() {
                    continue;
                }
                if matches!(asset_kind(&canonical_path), "nif" | "material") {
                    mismatch_dependency_members
                        .entry(archive_path.clone())
                        .or_default()
                        .push(canonical_path.clone());
                }
                let candidate = AssetEntry {
                    canonical_path,
                    archive_path: archive_path.clone(),
                    priority: *priority,
                    catalog_dependencies_valid: false,
                };
                insert_asset_by_priority(&mut assets, key, candidate);
            }
        }

        let mut dependencies = load_dependencies(&connection, &assets)?;
        for (archive_path, mut members) in mismatch_dependency_members {
            members.retain(|path| {
                assets
                    .get(&normalize_rel(path))
                    .is_some_and(|entry| entry.archive_path == archive_path)
            });
            bsarchive_native::python::visit_archive_files(
                &archive_path,
                &members,
                |path, bytes| {
                    let source = normalize_rel(path);
                    dependencies.entry(source).or_default().extend(
                        referenced_dependencies(path, bytes)
                            .into_iter()
                            .map(|(path, _)| path),
                    );
                    Ok(())
                },
            )?;
        }
        for values in dependencies.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
        let overlay_files = overlay_dir
            .filter(|path| path.is_dir())
            .map(index_overlay)
            .transpose()?
            .unwrap_or_default();
        let fingerprint = archive_fingerprint(&active_archives, &overlay_files, schema_version)?;
        let cache_data_root = cache_dir.join("fo4").join(fingerprint).join("Data");
        let membership_root = cache_data_root
            .parent()
            .unwrap_or(&cache_data_root)
            .join("Membership");
        fs::create_dir_all(&cache_data_root)
            .map_err(|error| format!("create target asset cache: {error}"))?;
        if reindexed > 0 {
            write_runtime_catalog_overlay(
                cache_data_root
                    .parent()
                    .unwrap_or(&cache_data_root)
                    .join("catalog_overlay.sqlite3"),
                &assets,
                &dependencies,
            )?;
        }

        Ok(Self {
            assets,
            dependencies,
            overlay_files,
            cache_data_root,
            membership_root,
            archive_readers: Mutex::new(HashMap::new()),
            warnings,
            stats: Mutex::new(TargetAssetStats {
                archives_reindexed: reindexed,
                ..TargetAssetStats::default()
            }),
        })
    }

    pub fn has_asset(&self, path: &str) -> bool {
        let key = normalize_rel(path);
        self.overlay_files.contains_key(&key) || self.assets.contains_key(&key)
    }

    pub fn list_assets(&self, prefix: &str, suffix: &str) -> Vec<String> {
        let prefix = normalize_rel(prefix);
        let suffix = suffix.to_ascii_lowercase();
        let mut paths: HashSet<String> = self
            .assets
            .keys()
            .chain(self.overlay_files.keys())
            .filter(|path| prefix.is_empty() || path.starts_with(&prefix))
            .filter(|path| suffix.is_empty() || path.ends_with(&suffix))
            .cloned()
            .collect();
        let mut paths: Vec<String> = paths.drain().collect();
        paths.sort_unstable();
        paths
    }

    pub fn asset_count(&self) -> usize {
        self.assets.len()
            + self
                .overlay_files
                .keys()
                .filter(|path| !self.assets.contains_key(*path))
                .count()
    }

    pub fn dependency_closure(&self, roots: &[String]) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut queue: VecDeque<String> = roots.iter().map(|path| normalize_rel(path)).collect();
        while let Some(path) = queue.pop_front() {
            if path.is_empty() || !seen.insert(path.clone()) {
                continue;
            }
            if let Some(dependencies) = self.dependencies.get(&path) {
                queue.extend(dependencies.iter().cloned());
            }
        }
        let mut paths: Vec<String> = seen.into_iter().collect();
        paths.sort_unstable();
        paths
    }

    pub fn materialize(&self, path: &str) -> Result<Option<PathBuf>, String> {
        let paths = self.materialize_many(&[path.to_string()])?;
        Ok(paths.into_iter().next())
    }

    pub fn materialize_many(&self, requested: &[String]) -> Result<Vec<PathBuf>, String> {
        let mut resolved: HashMap<String, PathBuf> = HashMap::new();
        let mut grouped: BTreeMap<PathBuf, Vec<(String, String, PathBuf)>> = BTreeMap::new();

        for requested_path in requested {
            let key = normalize_rel(requested_path);
            if key.is_empty() {
                continue;
            }
            if let Some(path) = self.overlay_files.get(&key) {
                let output = self.cache_data_root.join(path_from_data_relative(&key));
                if output.is_file() {
                    self.with_stats(|stats| stats.cache_hits += 1);
                } else {
                    self.with_stats(|stats| stats.cache_misses += 1);
                    let bytes = fs::read(path)
                        .map_err(|error| format!("read overlay {}: {error}", path.display()))?;
                    if write_cached_file(&output, &bytes)? {
                        self.with_stats(|stats| {
                            stats.files_extracted += 1;
                            stats.bytes_extracted += bytes.len() as u64;
                        });
                    }
                }
                resolved.insert(key, output);
                continue;
            }
            let Some(entry) = self.assets.get(&key) else {
                continue;
            };
            let output = self
                .cache_data_root
                .join(path_from_data_relative(&entry.canonical_path));
            if output.is_file() {
                resolved.insert(key, output);
                self.with_stats(|stats| stats.cache_hits += 1);
                continue;
            }
            self.with_stats(|stats| stats.cache_misses += 1);
            grouped
                .entry(entry.archive_path.clone())
                .or_default()
                .push((key, entry.canonical_path.clone(), output));
        }

        for (archive_path, entries) in grouped {
            let archive_members: Vec<String> = entries
                .iter()
                .map(|(_, canonical, _)| canonical.clone())
                .collect();
            let outputs: HashMap<String, (String, PathBuf)> = entries
                .into_iter()
                .map(|(key, canonical, output)| (normalize_rel(&canonical), (key, output)))
                .collect();
            let reader = {
                let mut readers = self
                    .archive_readers
                    .lock()
                    .map_err(|_| "target asset archive-reader lock poisoned".to_string())?;
                if let Some(reader) = readers.get(&archive_path) {
                    Arc::clone(reader)
                } else {
                    let reader = Arc::new(Mutex::new(
                        bsarchive_native::python::ArchiveReader::open(&archive_path)
                            .map_err(|error| format!("open {}: {error}", archive_path.display()))?,
                    ));
                    readers.insert(archive_path.clone(), Arc::clone(&reader));
                    reader
                }
            };
            reader
                .lock()
                .map_err(|_| format!("archive reader lock poisoned: {}", archive_path.display()))?
                .visit_files(&archive_members, |canonical, bytes| {
                    let lookup = normalize_rel(canonical);
                    let Some((key, output)) = outputs.get(&lookup) else {
                        return Err(format!("unexpected archive member returned: {canonical}"));
                    };
                    let wrote = write_cached_file(output, bytes)?;
                    resolved.insert(key.clone(), output.clone());
                    if wrote {
                        self.with_stats(|stats| {
                            stats.files_extracted += 1;
                            stats.bytes_extracted += bytes.len() as u64;
                        });
                    }
                    Ok(())
                })
                .map_err(|error| format!("extract {}: {error}", archive_path.display()))?;
        }

        Ok(requested
            .iter()
            .filter_map(|path| resolved.get(&normalize_rel(path)).cloned())
            .collect())
    }

    pub fn materialize_with_dependencies(
        &self,
        requested: &[String],
    ) -> Result<Vec<PathBuf>, String> {
        self.materialize_many(&self.dependency_closure(requested))
    }

    pub fn cache_data_root(&self) -> &Path {
        &self.cache_data_root
    }

    pub fn prepare_membership_tree(&self, prefix: &str, suffix: &str) -> Result<&Path, String> {
        for path in self.list_assets(prefix, suffix) {
            let marker = self.membership_root.join(path_from_data_relative(&path));
            if marker.is_file() {
                continue;
            }
            if let Some(parent) = marker.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create membership directory: {error}"))?;
            }
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&marker)
                .map_err(|error| format!("create membership marker: {error}"))?;
        }
        Ok(&self.membership_root)
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn stats(&self) -> TargetAssetStats {
        self.stats
            .lock()
            .map(|stats| stats.clone())
            .unwrap_or_default()
    }

    pub fn dependencies_for(&self, path: &str) -> &[String] {
        self.dependencies
            .get(&normalize_rel(path))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn with_stats(&self, update: impl FnOnce(&mut TargetAssetStats)) {
        if let Ok(mut stats) = self.stats.lock() {
            update(&mut stats);
        }
    }
}

pub fn prepare_anim_text_data_assets(
    store: &TargetAssetStore,
    handle_id: u64,
    base_race_handle_ids: &[u64],
    src_meshes_root: &Path,
) -> Result<PathBuf, String> {
    use crate::ids::SigCode;
    use crate::sym::StringInterner;

    let interner = StringInterner::new();
    let race_sig = SigCode::from_str("RACE").map_err(|e| e.to_string())?;
    let mut requested = HashSet::new();
    let mut hkx_directories = HashSet::new();
    let mut subgraph_ids = HashSet::new();
    for (race_handle_id, prefer_source_assets) in
        anim_text_data_race_handles(handle_id, base_race_handle_ids)
    {
        let mut session =
            crate::session::open_session(race_handle_id, None).map_err(|e| e.to_string())?;
        let schema = session.schema().map_err(|e| e.to_string())?;
        let form_keys = session
            .form_keys_of_sig(race_sig, &interner)
            .map_err(|e| e.to_string())?;
        for form_key in form_keys {
            let Ok(record) = session.record_decoded(&form_key, schema.as_ref(), &interner) else {
                continue;
            };
            for subgraph in crate::fixups::havok::anim_text_data_emit::subgraphs_from_race_record(
                &record, &interner,
            ) {
                subgraph_ids.insert(subgraph.id());
                if prefer_source_assets
                    && src_meshes_root
                        .join(subgraph.core_behavior.replace('\\', "/"))
                        .is_file()
                {
                    continue;
                }
                let core = data_meshes_path(&subgraph.core_behavior);
                if store.has_asset(&core) {
                    requested.insert(core.clone());
                }
                if let Some((behavior_dir, _)) = core.rsplit_once('/') {
                    hkx_directories.insert(behavior_dir.to_string());
                }
                for animation_path in &subgraph.sapt_chain {
                    let animation_path = data_meshes_path(animation_path);
                    hkx_directories.insert(animation_path.trim_end_matches('/').to_string());
                }
            }
        }
    }
    if subgraph_ids.is_empty() {
        return Ok(store.cache_data_root().join("Meshes"));
    }
    for path in store.list_assets("meshes/", ".hkx") {
        if asset_is_under_requested_directory(&path, &hkx_directories) {
            requested.insert(path);
        }
    }
    add_fixed_anim_text_data_requests(&mut requested, subgraph_ids);
    let mut requested: Vec<String> = requested.into_iter().collect();
    requested.sort_unstable();
    store.materialize_many(&requested)?;
    let meshes_root = store.cache_data_root().join("Meshes");
    let skeleton = meshes_root.join("Actors/Character/CharacterAssets/skeleton.hkx");
    if !skeleton.is_file() {
        return Err(format!(
            "FO4 target asset store did not materialize required character skeleton: {}",
            skeleton.display()
        ));
    }
    Ok(meshes_root)
}

const CHARACTER_SKELETON_ASSET: &str = "meshes/actors/character/characterassets/skeleton.hkx";
const ANIM_TEXT_DATA_OFFSETS_ASSET: &str =
    "meshes/animtextdata/animationoffsets/persistantsubgraphinfoandoffsetdata.txt";

fn add_fixed_anim_text_data_requests(
    requested: &mut HashSet<String>,
    subgraph_ids: impl IntoIterator<Item = u64>,
) {
    requested.insert(CHARACTER_SKELETON_ASSET.to_string());
    requested.insert(ANIM_TEXT_DATA_OFFSETS_ASSET.to_string());
    for id in subgraph_ids {
        requested.insert(format!("meshes/animtextdata/animationstancedata/{id}.txt"));
    }
}

fn anim_text_data_race_handles(handle_id: u64, base_race_handle_ids: &[u64]) -> Vec<(u64, bool)> {
    std::iter::once((handle_id, true))
        .chain(base_race_handle_ids.iter().copied().map(|id| (id, false)))
        .collect()
}

fn data_meshes_path(path: &str) -> String {
    let normalized = normalize_rel(path);
    if normalized.starts_with("meshes/") {
        normalized
    } else {
        format!("meshes/{normalized}")
    }
}

fn asset_is_under_requested_directory(path: &str, directories: &HashSet<String>) -> bool {
    let mut current = path;
    while let Some((parent, _)) = current.rsplit_once('/') {
        if directories.contains(parent) {
            return true;
        }
        current = parent;
    }
    false
}

fn load_archive_specs(connection: &Connection) -> Result<Vec<ArchiveSpec>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name, content_pack, required, expected_size, priority FROM archives ORDER BY priority",
        )
        .map_err(|error| format!("prepare target archive query: {error}"))?;
    statement
        .query_map([], |row| {
            Ok(ArchiveSpec {
                name: row.get(0)?,
                content_pack: row.get(1)?,
                required: row.get::<_, i64>(2)? != 0,
                expected_size: row.get::<_, i64>(3)?.max(0) as u64,
                priority: row.get(4)?,
            })
        })
        .map_err(|error| format!("query target archives: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read target archives: {error}"))
}

fn installed_archives(target_data_dir: &Path) -> Result<HashMap<String, PathBuf>, String> {
    let mut archives = HashMap::new();
    for entry in fs::read_dir(target_data_dir)
        .map_err(|error| format!("scan {}: {error}", target_data_dir.display()))?
    {
        let entry = entry.map_err(|error| format!("scan target Data entry: {error}"))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ba2"))
        {
            archives.insert(
                entry.file_name().to_string_lossy().to_ascii_lowercase(),
                path,
            );
        }
    }
    Ok(archives)
}

fn load_catalog_assets(
    connection: &Connection,
    active_archives: &HashMap<String, (PathBuf, bool, i64)>,
) -> Result<HashMap<String, AssetEntry>, String> {
    let mut assets = HashMap::new();
    let mut statement = connection
        .prepare(
            "SELECT d.path_key, a.name_key, ar.name, ao.priority
             FROM asset_owners ao
             JOIN assets a ON a.id = ao.asset_id
             JOIN directories d ON d.id = a.directory_id
             JOIN archives ar ON ar.id = ao.archive_id
             ORDER BY ao.priority",
        )
        .map_err(|error| format!("prepare target asset query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|error| format!("query target assets: {error}"))?;
    for row in rows {
        let (directory, name, archive_name, priority) =
            row.map_err(|error| format!("read target asset row: {error}"))?;
        let Some((archive_path, catalog_valid, _)) =
            active_archives.get(&archive_name.to_ascii_lowercase())
        else {
            continue;
        };
        if !catalog_valid {
            continue;
        }
        let path_key = join_asset_path(&directory, &name);
        insert_asset_by_priority(
            &mut assets,
            path_key.clone(),
            AssetEntry {
                canonical_path: path_key,
                archive_path: archive_path.clone(),
                priority,
                catalog_dependencies_valid: true,
            },
        );
    }
    Ok(assets)
}

fn insert_asset_by_priority(
    assets: &mut HashMap<String, AssetEntry>,
    key: String,
    candidate: AssetEntry,
) {
    if assets
        .get(&key)
        .is_some_and(|current| current.priority > candidate.priority)
    {
        return;
    }
    assets.insert(key, candidate);
}

fn load_dependencies(
    connection: &Connection,
    assets: &HashMap<String, AssetEntry>,
) -> Result<HashMap<String, Vec<String>>, String> {
    let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
    let mut statement = connection
        .prepare(
            "SELECT sd.path_key, sa.name_key, td.path_key, ta.name_key
             FROM asset_dependencies dep
             JOIN assets sa ON sa.id = dep.source_asset_id
             JOIN directories sd ON sd.id = sa.directory_id
             JOIN assets ta ON ta.id = dep.target_asset_id
             JOIN directories td ON td.id = ta.directory_id
             ORDER BY dep.source_asset_id, dep.target_asset_id",
        )
        .map_err(|error| format!("prepare target dependency query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| format!("query target dependencies: {error}"))?;
    for row in rows {
        let (source_dir, source_name, target_dir, target_name) =
            row.map_err(|error| format!("read dependency row: {error}"))?;
        let source = join_asset_path(&source_dir, &source_name);
        if !assets
            .get(&source)
            .is_some_and(|asset| asset.catalog_dependencies_valid)
        {
            continue;
        }
        let target = join_asset_path(&target_dir, &target_name);
        if target.is_empty() {
            continue;
        }
        dependencies.entry(source).or_default().push(target);
    }
    for values in dependencies.values_mut() {
        values.sort_unstable();
        values.dedup();
    }
    Ok(dependencies)
}

fn split_asset_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn join_asset_path(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_string()
    } else {
        format!("{directory}/{name}")
    }
}

fn index_overlay(root: &Path) -> Result<HashMap<String, PathBuf>, String> {
    let mut files = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("scan overlay {}: {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("scan overlay entry: {error}"))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                if let Ok(relative) = path.strip_prefix(root) {
                    files.insert(normalize_rel(&relative.to_string_lossy()), path);
                }
            }
        }
    }
    Ok(files)
}

fn archive_fingerprint(
    active_archives: &HashMap<String, (PathBuf, bool, i64)>,
    overlay_files: &HashMap<String, PathBuf>,
    schema_version: i64,
) -> Result<String, String> {
    let mut entries: Vec<_> = active_archives.iter().collect();
    entries.sort_unstable_by_key(|(name, _)| *name);
    let mut hasher = blake3::Hasher::new();
    hasher.update(schema_version.to_string().as_bytes());
    for (name, (path, _, priority)) in entries {
        let metadata = path
            .metadata()
            .map_err(|error| format!("fingerprint {}: {error}", path.display()))?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        hasher.update(name.as_bytes());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(&metadata.len().to_le_bytes());
        hasher.update(&modified.to_le_bytes());
        hasher.update(&priority.to_le_bytes());
    }
    let mut overlay_entries: Vec<_> = overlay_files.iter().collect();
    overlay_entries.sort_unstable_by_key(|(path, _)| *path);
    for (relative, path) in overlay_entries {
        let metadata = path
            .metadata()
            .map_err(|error| format!("fingerprint overlay {}: {error}", path.display()))?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        hasher.update(relative.as_bytes());
        hasher.update(&metadata.len().to_le_bytes());
        hasher.update(&modified.to_le_bytes());
    }
    Ok(hasher.finalize().to_hex()[..24].to_string())
}

fn path_from_data_relative(path: &str) -> PathBuf {
    path.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect()
}

fn write_cached_file(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    if path.is_file() {
        return Ok(false);
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("cache path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create cache directory: {error}"))?;
    let lock_path = path.with_extension(format!(
        "{}.modkit-lock",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
    ));
    let started = std::time::Instant::now();
    let lock = loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(lock) => break lock,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if path.is_file() {
                    return Ok(false);
                }
                if started.elapsed() > Duration::from_secs(30) {
                    return Err(format!(
                        "timed out waiting for cache lock {}",
                        lock_path.display()
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(format!("create cache lock: {error}")),
        }
    };
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or(""),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("create cache file: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("write cache file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync cache file: {error}"))?;
        match fs::rename(&temporary, path) {
            Ok(()) => Ok(true),
            Err(_) if path.is_file() => {
                let _ = fs::remove_file(&temporary);
                Ok(false)
            }
            Err(error) => Err(format!("publish cache file: {error}")),
        }
    })();
    drop(lock);
    let _ = fs::remove_file(&lock_path);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn asset_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".nif") {
        "nif"
    } else if lower.ends_with(".hkx") {
        "hkx"
    } else if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
        "material"
    } else if lower.ends_with(".dds") {
        "texture"
    } else if lower.ends_with(".pex") || lower.ends_with(".psc") {
        "script"
    } else if lower.ends_with(".xwm") || lower.ends_with(".wav") {
        "sound"
    } else {
        "other"
    }
}

fn official_archive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".ba2") && (lower.starts_with("fallout4 - ") || lower.starts_with("dlc"))
}

fn content_pack(name: &str) -> String {
    let stem = name.strip_suffix(".ba2").unwrap_or(name);
    if stem.to_ascii_lowercase().starts_with("fallout4 - ") {
        return "Fallout4".to_string();
    }
    stem.split(" - ").next().unwrap_or(stem).to_string()
}

fn archive_load_order(name: &str) -> (u8, String) {
    let lower = name.to_ascii_lowercase();
    let order = if lower.starts_with("fallout4 - ") {
        0
    } else if lower.starts_with("dlcrobot - ") {
        10
    } else if lower.starts_with("dlcworkshop01 - ") {
        20
    } else if lower.starts_with("dlccoast - ") {
        30
    } else if lower.starts_with("dlcworkshop02 - ") {
        40
    } else if lower.starts_with("dlcworkshop03 - ") {
        50
    } else if lower.starts_with("dlcnukaworld - ") {
        60
    } else {
        u8::MAX
    };
    (order, lower)
}

fn normalize_texture(path: &str) -> String {
    let path = normalize_rel(path);
    if path.is_empty() || path.starts_with("textures/") {
        path
    } else {
        format!("textures/{path}")
    }
}

fn normalize_material(path: &str) -> String {
    let path = normalize_rel(path);
    if path.is_empty() || path.starts_with("materials/") {
        path
    } else {
        format!("materials/{path}")
    }
}

fn material_texture_paths(bytes: &[u8], extension: &str) -> Vec<String> {
    let mut raw = Vec::new();
    if extension.eq_ignore_ascii_case("bgem") {
        if let Ok(material) = materials_native::bgem::parse(bytes) {
            raw.extend([
                material.BaseTexture,
                material.GrayscaleTexture,
                material.EnvmapTexture,
                material.NormalTexture,
                material.EnvmapMaskTexture,
            ]);
            raw.extend(
                [
                    material.SpecularTexture,
                    material.LightingTexture,
                    material.GlowTexture,
                ]
                .into_iter()
                .flatten(),
            );
        }
    } else if let Ok(material) = materials_native::bgsm::parse(bytes) {
        raw.extend([
            material.DiffuseTexture,
            material.NormalTexture,
            material.SmoothSpecTexture,
            material.GreyscaleTexture,
        ]);
        raw.extend(
            [
                material.EnvmapTexture,
                material.GlowTexture,
                material.SpecularTexture,
                material.LightingTexture,
            ]
            .into_iter()
            .flatten(),
        );
    }
    let mut paths: Vec<String> = raw
        .into_iter()
        .map(|path| normalize_texture(&path))
        .filter(|path| !path.is_empty())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn referenced_dependencies(path: &str, bytes: &[u8]) -> Vec<(String, &'static str)> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".nif") {
        return match NifFile::from_bytes(bytes, None) {
            Ok(nif) => {
                let refs = nif.referenced_asset_paths();
                refs.materials
                    .into_iter()
                    .map(|value| (normalize_material(&value), "nif_material"))
                    .chain(
                        refs.textures
                            .into_iter()
                            .map(|value| (normalize_texture(&value), "nif_texture")),
                    )
                    .collect()
            }
            Err(_) => Vec::new(),
        };
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    material_texture_paths(bytes, extension)
        .into_iter()
        .map(|value| (value, "material_texture"))
        .collect()
}

fn write_runtime_catalog_overlay(
    path: PathBuf,
    assets: &HashMap<String, AssetEntry>,
    dependencies: &HashMap<String, Vec<String>>,
) -> Result<(), String> {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let temporary = path.with_extension(format!("sqlite3.{}.{}.tmp", std::process::id(), nonce));
    let _ = fs::remove_file(&temporary);
    let mut connection = Connection::open(&temporary)
        .map_err(|error| format!("create runtime target catalog overlay: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=OFF;
             PRAGMA synchronous=OFF;
             CREATE TABLE assets(
                 path_key TEXT PRIMARY KEY,
                 canonical_path TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 archive_name TEXT NOT NULL,
                 priority INTEGER NOT NULL
             ) WITHOUT ROWID;
             CREATE TABLE asset_dependencies(
                 source_key TEXT NOT NULL,
                 target_key TEXT NOT NULL,
                 PRIMARY KEY(source_key, target_key)
             ) WITHOUT ROWID;",
        )
        .map_err(|error| format!("initialize runtime target catalog overlay: {error}"))?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("begin runtime target catalog overlay: {error}"))?;
    {
        let mut insert_asset = transaction
            .prepare_cached("INSERT INTO assets VALUES (?1, ?2, ?3, ?4, ?5)")
            .map_err(|error| format!("prepare runtime asset insert: {error}"))?;
        for (path_key, entry) in assets {
            if entry.catalog_dependencies_valid {
                continue;
            }
            let archive_name = entry
                .archive_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            insert_asset
                .execute(params![
                    path_key,
                    entry.canonical_path,
                    asset_kind(path_key),
                    archive_name,
                    entry.priority
                ])
                .map_err(|error| format!("insert runtime asset {path_key}: {error}"))?;
        }
        let mut insert_dependency = transaction
            .prepare_cached("INSERT OR IGNORE INTO asset_dependencies VALUES (?1, ?2)")
            .map_err(|error| format!("prepare runtime dependency insert: {error}"))?;
        for (source, targets) in dependencies {
            if !assets
                .get(source)
                .is_some_and(|entry| !entry.catalog_dependencies_valid)
            {
                continue;
            }
            for target in targets {
                insert_dependency
                    .execute(params![source, target])
                    .map_err(|error| {
                        format!("insert runtime dependency {source} -> {target}: {error}")
                    })?;
            }
        }
    }
    transaction
        .commit()
        .map_err(|error| format!("commit runtime target catalog overlay: {error}"))?;
    connection
        .close()
        .map_err(|(_, error)| format!("close runtime target catalog overlay: {error}"))?;
    if path.is_file() {
        let _ = fs::remove_file(&temporary);
        return Ok(());
    }
    match fs::rename(&temporary, &path) {
        Ok(()) => Ok(()),
        Err(_) if path.is_file() => {
            let _ = fs::remove_file(&temporary);
            Ok(())
        }
        Err(error) => Err(format!("publish runtime target catalog overlay: {error}")),
    }
}

pub fn build_catalog(
    target_data_dir: &Path,
    output_path: &Path,
    game_build: &str,
) -> Result<(), String> {
    if !target_data_dir.is_dir() {
        return Err(format!(
            "FO4 Data directory is missing: {}",
            target_data_dir.display()
        ));
    }
    let temporary = output_path.with_extension("sqlite3.tmp");
    let _ = fs::remove_file(&temporary);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create catalog directory: {error}"))?;
    }
    let mut connection = Connection::open(&temporary)
        .map_err(|error| format!("create target asset catalog: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=OFF;
             PRAGMA synchronous=OFF;
             CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE archives(
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 content_pack TEXT NOT NULL,
                 required INTEGER NOT NULL,
                 expected_size INTEGER NOT NULL,
                 priority INTEGER NOT NULL
             );
             CREATE TABLE directories(
                 id INTEGER PRIMARY KEY,
                 path_key TEXT NOT NULL UNIQUE
             );
             CREATE TABLE assets(
                 id INTEGER PRIMARY KEY,
                 directory_id INTEGER NOT NULL,
                 name_key TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 UNIQUE(directory_id, name_key)
             );
             CREATE TABLE asset_owners(
                 asset_id INTEGER NOT NULL,
                 archive_id INTEGER NOT NULL,
                 priority INTEGER NOT NULL,
                 PRIMARY KEY(asset_id, archive_id)
             ) WITHOUT ROWID;
             CREATE INDEX idx_asset_owners_archive ON asset_owners(archive_id);
             CREATE TABLE asset_dependencies(
                 source_asset_id INTEGER NOT NULL,
                 target_asset_id INTEGER NOT NULL,
                 ref_kind TEXT NOT NULL,
                 PRIMARY KEY(source_asset_id, target_asset_id, ref_kind)
             ) WITHOUT ROWID;
             CREATE VIEW catalog_assets AS
                 SELECT
                     CASE WHEN d.path_key = '' THEN a.name_key
                          ELSE d.path_key || '/' || a.name_key END AS path_key,
                     CASE WHEN d.path_key = '' THEN a.name_key
                          ELSE d.path_key || '/' || a.name_key END AS canonical_path,
                     a.kind AS asset_type,
                     ar.name AS archive_owner,
                     ao.priority AS load_priority
                 FROM assets a
                 JOIN directories d ON d.id = a.directory_id
                 JOIN asset_owners ao ON ao.asset_id = a.id
                 JOIN archives ar ON ar.id = ao.archive_id;
             CREATE VIEW catalog_dependencies AS
                 SELECT
                     CASE WHEN sd.path_key = '' THEN sa.name_key
                          ELSE sd.path_key || '/' || sa.name_key END AS source_key,
                     CASE WHEN td.path_key = '' THEN ta.name_key
                          ELSE td.path_key || '/' || ta.name_key END AS target_key,
                     dep.ref_kind
                 FROM asset_dependencies dep
                 JOIN assets sa ON sa.id = dep.source_asset_id
                 JOIN directories sd ON sd.id = sa.directory_id
                 JOIN assets ta ON ta.id = dep.target_asset_id
                 JOIN directories td ON td.id = ta.directory_id;",
        )
        .map_err(|error| format!("initialize target asset catalog: {error}"))?;
    connection
        .execute(
            "INSERT INTO metadata(key, value) VALUES ('schema_version', ?1), ('target_game', 'fo4'), ('game_build', ?2)",
            params![CATALOG_SCHEMA_VERSION.to_string(), game_build],
        )
        .map_err(|error| format!("write target asset metadata: {error}"))?;

    let mut archives: Vec<PathBuf> = fs::read_dir(target_data_dir)
        .map_err(|error| format!("scan FO4 Data: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(official_archive_name)
        })
        .collect();
    archives.sort_by_key(|path| {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(archive_load_order)
            .unwrap_or_else(|| (u8::MAX, String::new()))
    });
    if archives.is_empty() {
        return Err("no official FO4 BA2 archives found".to_string());
    }

    let transaction = connection
        .transaction()
        .map_err(|error| format!("begin target asset catalog transaction: {error}"))?;
    let mut archive_rows = Vec::new();
    for (index, archive_path) in archives.iter().enumerate() {
        let name = archive_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("invalid archive filename: {}", archive_path.display()))?;
        let priority = index as i64;
        let required = name.to_ascii_lowercase().starts_with("fallout4 - ");
        let expected_size = archive_path
            .metadata()
            .map_err(|error| format!("stat {}: {error}", archive_path.display()))?
            .len();
        transaction
            .execute(
                "INSERT INTO archives(name, content_pack, required, expected_size, priority) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![name, content_pack(name), i64::from(required), expected_size as i64, priority],
            )
            .map_err(|error| format!("insert archive {name}: {error}"))?;
        let archive_id = transaction.last_insert_rowid();
        archive_rows.push((archive_path.clone(), archive_id, priority));
    }

    let mut selected: HashMap<String, (i64, PathBuf, String, i64)> = HashMap::new();
    let mut directory_ids = HashMap::new();
    {
        let mut insert_directory = transaction
            .prepare_cached(
                "INSERT INTO directories(path_key) VALUES (?1)
                 ON CONFLICT(path_key) DO UPDATE SET path_key=excluded.path_key
                 RETURNING id",
            )
            .map_err(|error| format!("prepare directory insert: {error}"))?;
        let mut insert_asset = transaction
            .prepare_cached(
                "INSERT INTO assets(directory_id, name_key, kind) VALUES (?1, ?2, ?3)
                 ON CONFLICT(directory_id, name_key) DO UPDATE SET kind=excluded.kind
                 RETURNING id",
            )
            .map_err(|error| format!("prepare asset insert: {error}"))?;
        let mut insert_owner = transaction
            .prepare_cached(
                "INSERT OR REPLACE INTO asset_owners(asset_id, archive_id, priority)
                 VALUES (?1, ?2, ?3)",
            )
            .map_err(|error| format!("prepare asset-owner insert: {error}"))?;

        for (archive_path, archive_id, priority) in archive_rows {
            let files = bsarchive_native::list_archive_files(&archive_path)
                .map_err(|error| format!("list {}: {error}", archive_path.display()))?;
            for canonical_path in files {
                let path_key = normalize_rel(&canonical_path);
                if path_key.is_empty() {
                    continue;
                }
                let (directory, name) = split_asset_path(&path_key);
                let directory_id = if let Some(id) = directory_ids.get(directory) {
                    *id
                } else {
                    let id = insert_directory
                        .query_row(params![directory], |row| row.get::<_, i64>(0))
                        .map_err(|error| format!("insert directory {directory}: {error}"))?;
                    directory_ids.insert(directory.to_string(), id);
                    id
                };
                let asset_id = insert_asset
                    .query_row(
                        params![directory_id, name, asset_kind(&canonical_path)],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(|error| format!("insert asset {canonical_path}: {error}"))?;
                insert_owner
                    .execute(params![asset_id, archive_id, priority])
                    .map_err(|error| format!("insert asset owner {canonical_path}: {error}"))?;
                let dependency_source = canonical_path.to_ascii_lowercase().ends_with(".nif")
                    || canonical_path.to_ascii_lowercase().ends_with(".bgsm")
                    || canonical_path.to_ascii_lowercase().ends_with(".bgem");
                if dependency_source
                    && selected
                        .get(&path_key)
                        .is_none_or(|(current_priority, _, _, _)| priority >= *current_priority)
                {
                    selected.insert(
                        path_key,
                        (priority, archive_path.clone(), canonical_path, asset_id),
                    );
                }
            }
        }
    }

    let mut by_archive: BTreeMap<PathBuf, Vec<(i64, String)>> = BTreeMap::new();
    for (_, (_, archive_path, canonical_path, asset_id)) in selected {
        by_archive
            .entry(archive_path)
            .or_default()
            .push((asset_id, canonical_path));
    }
    {
        let mut lookup_asset = transaction
            .prepare_cached(
                "SELECT a.id FROM assets a
                 JOIN directories d ON d.id = a.directory_id
                 WHERE d.path_key = ?1 AND a.name_key = ?2",
            )
            .map_err(|error| format!("prepare dependency target lookup: {error}"))?;
        let mut insert_dependency = transaction
            .prepare_cached(
                "INSERT OR IGNORE INTO asset_dependencies(
                    source_asset_id, target_asset_id, ref_kind
                 ) VALUES (?1, ?2, ?3)",
            )
            .map_err(|error| format!("prepare dependency insert: {error}"))?;
        for (archive_path, entries) in by_archive {
            let requested: Vec<String> = entries.iter().map(|(_, path)| path.clone()).collect();
            let source_ids: HashMap<String, i64> = entries
                .into_iter()
                .map(|(id, path)| (normalize_rel(&path), id))
                .collect();
            bsarchive_native::python::visit_archive_files(
                &archive_path,
                &requested,
                |path, bytes| {
                    let source_id =
                        source_ids
                            .get(&normalize_rel(path))
                            .copied()
                            .ok_or_else(|| {
                                format!("catalog builder received unexpected member {path}")
                            })?;
                    for (target_key, ref_kind) in referenced_dependencies(path, bytes) {
                        let (directory, name) = split_asset_path(&target_key);
                        let target_id = lookup_asset
                            .query_row(params![directory, name], |row| row.get::<_, i64>(0))
                            .optional()
                            .map_err(|error| {
                                format!("lookup dependency target {target_key}: {error}")
                            })?;
                        if let Some(target_id) = target_id {
                            insert_dependency
                                .execute(params![source_id, target_id, ref_kind])
                                .map_err(|error| {
                                    format!("insert dependency for {path}: {error}")
                                })?;
                        }
                    }
                    Ok(())
                },
            )?;
        }
    }
    transaction
        .commit()
        .map_err(|error| format!("commit target asset catalog: {error}"))?;
    connection
        .execute_batch("PRAGMA optimize; VACUUM;")
        .map_err(|error| format!("finalize target asset catalog: {error}"))?;
    connection
        .close()
        .map_err(|(_, error)| format!("close target asset catalog: {error}"))?;
    if output_path.is_file() {
        fs::remove_file(output_path)
            .map_err(|error| format!("replace target asset catalog: {error}"))?;
    }
    fs::rename(&temporary, output_path)
        .map_err(|error| format!("publish target asset catalog: {error}"))
}

#[pyclass(name = "TargetAssetStore")]
pub struct PyTargetAssetStore {
    inner: Arc<TargetAssetStore>,
    baseline: TargetAssetStats,
    opened_store: bool,
}

#[pymethods]
impl PyTargetAssetStore {
    #[new]
    #[pyo3(signature = (target_data_dir, catalog_path, cache_dir, overlay_dir=None))]
    fn new(
        target_data_dir: &str,
        catalog_path: &str,
        cache_dir: &str,
        overlay_dir: Option<&str>,
    ) -> PyResult<Self> {
        let (inner, opened_store) = TargetAssetStore::open_shared_with_status(
            Path::new(target_data_dir),
            Path::new(catalog_path),
            Path::new(cache_dir),
            overlay_dir.map(Path::new),
        )
        .map_err(PyValueError::new_err)?;
        let baseline = inner.stats();
        Ok(Self {
            inner,
            baseline,
            opened_store,
        })
    }

    fn has_asset(&self, path: &str) -> bool {
        self.inner.has_asset(path)
    }

    #[pyo3(signature = (prefix="", suffix=""))]
    fn list_assets(&self, prefix: &str, suffix: &str) -> Vec<String> {
        self.inner.list_assets(prefix, suffix)
    }

    fn dependency_closure(&self, paths: Vec<String>) -> Vec<String> {
        self.inner.dependency_closure(&paths)
    }

    fn materialize(&self, path: &str) -> PyResult<Option<String>> {
        self.inner
            .materialize(path)
            .map(|path| path.map(|path| path.to_string_lossy().to_string()))
            .map_err(PyRuntimeError::new_err)
    }

    #[pyo3(signature = (paths, include_dependencies=false))]
    fn materialize_many(
        &self,
        paths: Vec<String>,
        include_dependencies: bool,
    ) -> PyResult<Vec<String>> {
        let paths = if include_dependencies {
            self.inner.materialize_with_dependencies(&paths)
        } else {
            self.inner.materialize_many(&paths)
        }
        .map_err(PyRuntimeError::new_err)?;
        Ok(paths
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect())
    }

    #[getter]
    fn cache_data_root(&self) -> String {
        self.inner.cache_data_root().to_string_lossy().to_string()
    }

    #[getter]
    fn asset_count(&self) -> usize {
        self.inner.asset_count()
    }

    #[getter]
    fn warnings(&self) -> Vec<String> {
        self.inner.warnings().to_vec()
    }

    fn stats(&self) -> HashMap<String, u64> {
        let stats = self.inner.stats();
        HashMap::from([
            (
                "cache_hits".to_string(),
                stats.cache_hits.saturating_sub(self.baseline.cache_hits),
            ),
            (
                "cache_misses".to_string(),
                stats
                    .cache_misses
                    .saturating_sub(self.baseline.cache_misses),
            ),
            (
                "files_extracted".to_string(),
                stats
                    .files_extracted
                    .saturating_sub(self.baseline.files_extracted),
            ),
            (
                "bytes_extracted".to_string(),
                stats
                    .bytes_extracted
                    .saturating_sub(self.baseline.bytes_extracted),
            ),
            (
                "archives_reindexed".to_string(),
                if self.opened_store {
                    stats.archives_reindexed
                } else {
                    0
                },
            ),
        ])
    }
}

#[pyfunction(name = "conversion_target_asset_catalog_schema_version")]
pub fn catalog_schema_version_py() -> i64 {
    CATALOG_SCHEMA_VERSION
}

#[pyfunction(name = "conversion_build_target_asset_catalog")]
#[pyo3(signature = (target_data_dir, output_path, game_build=""))]
pub fn build_catalog_py(
    py: Python<'_>,
    target_data_dir: &str,
    output_path: &str,
    game_build: &str,
) -> PyResult<()> {
    let target_data_dir = PathBuf::from(target_data_dir);
    let output_path = PathBuf::from(output_path);
    let game_build = game_build.to_string();
    py.detach(move || build_catalog(&target_data_dir, &output_path, &game_build))
        .map_err(PyRuntimeError::new_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anim_text_data_scope_includes_target_and_base_race_handles() {
        assert_eq!(
            anim_text_data_race_handles(10, &[20, 30]),
            vec![(10, true), (20, false), (30, false)]
        );
    }

    #[test]
    fn anim_text_data_fixed_assets_include_skeleton_offsets_and_stance_files() {
        let mut requested = HashSet::new();
        add_fixed_anim_text_data_requests(&mut requested, [42]);
        assert!(requested.contains(CHARACTER_SKELETON_ASSET));
        assert!(requested.contains(ANIM_TEXT_DATA_OFFSETS_ASSET));
        assert!(requested.contains("meshes/animtextdata/animationstancedata/42.txt"));
    }

    #[test]
    fn anim_text_data_hkx_directory_matching_includes_descendants_only() {
        let directories = HashSet::from([
            "meshes/actors/character/behaviors".to_string(),
            "meshes/actors/character/animations/gauss".to_string(),
        ]);
        assert!(asset_is_under_requested_directory(
            "meshes/actors/character/behaviors/weapon.hkx",
            &directories
        ));
        assert!(asset_is_under_requested_directory(
            "meshes/actors/character/animations/gauss/reload/idle.hkx",
            &directories
        ));
        assert!(!asset_is_under_requested_directory(
            "meshes/actors/character/animations/minigun/idle.hkx",
            &directories
        ));
    }

    #[test]
    fn official_archive_filter_excludes_mod_archives() {
        assert!(official_archive_name("Fallout4 - Meshes.ba2"));
        assert!(official_archive_name("DLCCoast - Main.ba2"));
        assert!(!official_archive_name(
            "ccBGSFO4001-PipBoy(Black) - Main.ba2"
        ));
        assert!(!official_archive_name("B21_Test - Main.ba2"));
    }

    #[test]
    fn archive_priority_matches_official_master_order() {
        assert!(
            archive_load_order("Fallout4 - Main.ba2") < archive_load_order("DLCRobot - Main.ba2")
        );
        assert!(
            archive_load_order("DLCRobot - Main.ba2") < archive_load_order("DLCCoast - Main.ba2")
        );
        assert!(
            archive_load_order("DLCCoast - Main.ba2")
                < archive_load_order("DLCNukaWorld - Main.ba2")
        );
    }

    #[test]
    fn data_relative_paths_cannot_escape_cache_root() {
        assert_eq!(
            path_from_data_relative("Meshes/Actors/Test.nif"),
            PathBuf::from("Meshes/Actors/Test.nif")
        );
        assert_eq!(
            path_from_data_relative("../Meshes/Test.nif"),
            PathBuf::from("Meshes/Test.nif")
        );
    }
}
