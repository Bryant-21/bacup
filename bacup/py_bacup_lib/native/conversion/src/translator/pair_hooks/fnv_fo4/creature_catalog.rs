use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

// The source plugins contain 3,129 winners before the cross-game merge maps 446
// duplicate FO3 EditorIDs onto their FNV counterparts.
pub const EXPECTED_FULL_SOURCE_CREA_WINNERS: usize = 3_129;
pub const EXPECTED_FULL_MERGED_CREA_WINNERS: usize = 2_683;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyCreatureGame {
    Fnv,
    Fo3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureProvenance {
    pub game: LegacyCreatureGame,
    pub source_plugin: String,
    pub precedence: u32,
}

#[derive(Debug, Clone)]
pub struct LegacyRecordSource<'a> {
    pub record: &'a Record,
    pub provenance: CreatureProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StableFormKey {
    pub local: u32,
    pub plugin: String,
}

impl fmt::Display for StableFormKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:06X}@{}", self.local, self.plugin)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RigFamilyKey {
    pub game: LegacyCreatureGame,
    pub skeleton_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BodyVariantKey {
    pub rig: RigFamilyKey,
    pub body_paths: Vec<String>,
    pub nift: Vec<u8>,
    pub nifz_supported: bool,
    pub nift_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetReadiness {
    Ready,
    AssetRootNotConfigured,
    MissingSkeleton(String),
    ScanFailed(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyReadiness {
    Ready,
    AssetRootNotConfigured,
    MissingBodyDeclaration,
    MissingAssets(Vec<String>),
    UnsupportedNifzEncoding,
    UnsupportedNiftEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureFamilyKind {
    Organic,
    RobotOrTurret,
    HumanoidWeaponOverlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackProfile {
    Melee,
    Ranged,
    Mixed,
    Undetermined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphReadiness {
    NeedsMeleeGraph,
    NeedsRangedGraph,
    NeedsWeaponOverlayGraph,
    NeedsClipClassification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigFamily {
    pub key: RigFamilyKey,
    pub members: Vec<StableFormKey>,
    pub direct_kf_paths: Vec<String>,
    pub recursive_kf_paths: Vec<String>,
    pub asset_dependencies: Vec<String>,
    pub readiness: AssetReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyVariant {
    pub key: BodyVariantKey,
    pub members: Vec<StableFormKey>,
    pub asset_dependencies: Vec<String>,
    pub readiness: BodyReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackSet {
    pub rig: RigFamilyKey,
    pub family_kind: CreatureFamilyKind,
    pub profile: AttackProfile,
    pub animation_paths: Vec<String>,
    pub melee_candidates: Vec<String>,
    pub ranged_candidates: Vec<String>,
    pub looping_candidates: Vec<String>,
    pub graph_readiness: GraphReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProxyBlocker {
    MissingTarget(StableFormKey),
    UnsupportedTarget {
        target: StableFormKey,
        signature: String,
    },
    EmptyLeveledList(StableFormKey),
    UndecodableLeveledEntry(StableFormKey),
    TerminalSpecial(StableFormKey),
    TemplateCycle(Vec<StableFormKey>),
    MalformedTemplate(StableFormKey),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyReadiness {
    Ready,
    Blocked(Vec<ProxyBlocker>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecialCreatureReason {
    NoVisualTemplate,
    StaticOrMarkerModel(String),
    MalformedModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatureDisposition {
    VisualOwner {
        rig: RigFamilyKey,
        body: BodyVariantKey,
    },
    Proxy {
        terminal_visual_owners: Vec<StableFormKey>,
        readiness: ProxyReadiness,
    },
    Special {
        reason: SpecialCreatureReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordVariant {
    pub source: StableFormKey,
    pub editor_id: Option<String>,
    pub provenance: CreatureProvenance,
    pub record_dependencies: Vec<StableFormKey>,
    pub disposition: CreatureDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CorpusAccounting {
    pub input_records: usize,
    pub winning_records: usize,
    pub winning_creatures: usize,
    pub dispositions: usize,
    pub fnv_creatures: usize,
    pub fo3_creatures: usize,
    pub visual_owners: usize,
    pub proxies: usize,
    pub blocked_proxies: usize,
    pub specials: usize,
    pub rig_families: usize,
    pub body_variants: usize,
    pub attack_sets: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureCorpusPlan {
    pub records: Vec<RecordVariant>,
    pub rig_families: Vec<RigFamily>,
    pub body_variants: Vec<BodyVariant>,
    pub attack_sets: Vec<AttackSet>,
    pub accounting: CorpusAccounting,
}

impl CreatureCorpusPlan {
    pub fn record(&self, source: &StableFormKey) -> Option<&RecordVariant> {
        self.records.iter().find(|record| &record.source == source)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CreatureCatalogOptions<'a> {
    pub mesh_root: Option<&'a Path>,
    pub expected_creature_winners: Option<usize>,
}

impl Default for CreatureCatalogOptions<'_> {
    fn default() -> Self {
        Self {
            mesh_root: None,
            expected_creature_winners: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CreatureCatalogError {
    #[error("record FormKey uses an unresolved plugin symbol")]
    UnresolvedPlugin,
    #[error("record {record} uses an unresolved EditorID symbol")]
    UnresolvedEditorId { record: StableFormKey },
    #[error(
        "winning-record collision for {record} at precedence {precedence}: {first_plugin} and {second_plugin}"
    )]
    WinnerCollision {
        record: StableFormKey,
        precedence: u32,
        first_plugin: String,
        second_plugin: String,
    },
    #[error("merged CREA census is {actual}, expected {expected}")]
    WinnerCountMismatch { expected: usize, actual: usize },
    #[error("creature corpus accounting drifted: {detail}")]
    AccountingDrift { detail: String },
}

pub fn build_full_merged_creature_corpus_plan(
    records: &[LegacyRecordSource<'_>],
    mesh_root: Option<&Path>,
    interner: &StringInterner,
) -> Result<CreatureCorpusPlan, CreatureCatalogError> {
    build_creature_corpus_plan(
        records,
        CreatureCatalogOptions {
            mesh_root,
            expected_creature_winners: Some(EXPECTED_FULL_MERGED_CREA_WINNERS),
        },
        interner,
    )
}

pub fn build_creature_corpus_plan(
    records: &[LegacyRecordSource<'_>],
    options: CreatureCatalogOptions<'_>,
    interner: &StringInterner,
) -> Result<CreatureCorpusPlan, CreatureCatalogError> {
    let winners = select_winners(records, interner)?;
    let creature_keys = winners
        .iter()
        .filter_map(|(key, winner)| (winner.record.sig.as_str() == "CREA").then(|| key.clone()))
        .collect::<Vec<_>>();
    if let Some(expected) = options.expected_creature_winners
        && creature_keys.len() != expected
    {
        return Err(CreatureCatalogError::WinnerCountMismatch {
            expected,
            actual: creature_keys.len(),
        });
    }

    let seeds = winners
        .iter()
        .map(|(key, winner)| (key.clone(), record_seed(winner.record, key, interner)))
        .collect::<BTreeMap<_, _>>();

    let mut proxy_memo = BTreeMap::new();
    let mut record_variants = Vec::with_capacity(creature_keys.len());
    let mut rig_members = BTreeMap::<RigFamilyKey, BTreeSet<StableFormKey>>::new();
    let mut body_members = BTreeMap::<BodyVariantKey, BTreeSet<StableFormKey>>::new();

    for key in creature_keys {
        let winner = winners.get(&key).expect("CREA key came from winners");
        let seed = seeds.get(&key).expect("every winner has a seed");
        let disposition = match &seed.model {
            PathEvidence::Resolved(model) if is_skeleton_path(model) => {
                let rig = RigFamilyKey {
                    game: winner.provenance.game,
                    skeleton_path: model.clone(),
                };
                let body = BodyVariantKey {
                    rig: rig.clone(),
                    body_paths: seed.body_paths.clone(),
                    nift: seed.nift.clone(),
                    nifz_supported: seed.nifz_supported,
                    nift_supported: seed.nift_supported,
                };
                rig_members
                    .entry(rig.clone())
                    .or_default()
                    .insert(key.clone());
                body_members
                    .entry(body.clone())
                    .or_default()
                    .insert(key.clone());
                CreatureDisposition::VisualOwner { rig, body }
            }
            PathEvidence::Resolved(model) => CreatureDisposition::Special {
                reason: SpecialCreatureReason::StaticOrMarkerModel(model.clone()),
            },
            PathEvidence::Invalid => CreatureDisposition::Special {
                reason: SpecialCreatureReason::MalformedModel,
            },
            PathEvidence::Absent if seed.template.is_some() || seed.template_malformed => {
                let resolution = resolve_proxy(
                    &key,
                    &winners,
                    &seeds,
                    &mut proxy_memo,
                    &mut Vec::new(),
                    interner,
                );
                CreatureDisposition::Proxy {
                    terminal_visual_owners: resolution.owners.into_iter().collect(),
                    readiness: if resolution.blockers.is_empty() {
                        ProxyReadiness::Ready
                    } else {
                        ProxyReadiness::Blocked(resolution.blockers.into_iter().collect())
                    },
                }
            }
            PathEvidence::Absent => CreatureDisposition::Special {
                reason: SpecialCreatureReason::NoVisualTemplate,
            },
        };
        let editor_id = winner
            .record
            .eid
            .map(|symbol| {
                interner.resolve(symbol).map(str::to_owned).ok_or_else(|| {
                    CreatureCatalogError::UnresolvedEditorId {
                        record: key.clone(),
                    }
                })
            })
            .transpose()?;
        record_variants.push(RecordVariant {
            source: key,
            editor_id,
            provenance: winner.provenance.clone(),
            record_dependencies: seed.dependencies.clone(),
            disposition,
        });
    }

    let mut rig_families = Vec::with_capacity(rig_members.len());
    let mut attack_sets = Vec::with_capacity(rig_members.len());
    for (key, members) in rig_members {
        let mesh_root = options.mesh_root.map(|root| game_mesh_root(root, key.game));
        let scan = scan_rig_assets(&key.skeleton_path, mesh_root.as_deref());
        let attack_set = build_attack_set(&key, &scan.recursive_kfs);
        let mut dependencies = vec![key.skeleton_path.clone()];
        dependencies.extend(scan.recursive_kfs.iter().cloned());
        dependencies.sort();
        dependencies.dedup();
        rig_families.push(RigFamily {
            key: key.clone(),
            members: members.into_iter().collect(),
            direct_kf_paths: scan.direct_kfs.clone(),
            recursive_kf_paths: scan.recursive_kfs.clone(),
            asset_dependencies: dependencies,
            readiness: scan.readiness.clone(),
        });
        attack_sets.push(attack_set);
    }

    let body_variants = body_members
        .into_iter()
        .map(|(key, members)| {
            let mesh_root = options
                .mesh_root
                .map(|root| game_mesh_root(root, key.rig.game));
            let (readiness, dependencies) = body_readiness(&key, mesh_root.as_deref());
            BodyVariant {
                key,
                members: members.into_iter().collect(),
                asset_dependencies: dependencies,
                readiness,
            }
        })
        .collect::<Vec<_>>();

    record_variants.sort_by(|left, right| left.source.cmp(&right.source));
    rig_families.sort_by(|left, right| left.key.cmp(&right.key));
    attack_sets.sort_by(|left, right| left.rig.cmp(&right.rig));
    let mut body_variants = body_variants;
    body_variants.sort_by(|left, right| left.key.cmp(&right.key));

    let accounting = account(
        records.len(),
        winners.len(),
        &record_variants,
        rig_families.len(),
        body_variants.len(),
        attack_sets.len(),
    );
    validate_accounting(&accounting)?;

    Ok(CreatureCorpusPlan {
        records: record_variants,
        rig_families,
        body_variants,
        attack_sets,
        accounting,
    })
}

pub(super) fn game_mesh_root(root: &Path, game: LegacyCreatureGame) -> PathBuf {
    let game_root = game_data_root(root, game);
    for meshes in ["Meshes", "meshes"] {
        let candidate = game_root.join(meshes);
        if candidate.is_dir() {
            return candidate;
        }
    }
    game_root
}

pub(super) fn game_data_root(root: &Path, game: LegacyCreatureGame) -> PathBuf {
    let aliases: &[&str] = match game {
        LegacyCreatureGame::Fnv => &["fnv", "falloutnv"],
        LegacyCreatureGame::Fo3 => &["fo3", "fallout3"],
    };
    for alias in aliases {
        let candidate = root.join(alias);
        if candidate.is_dir() {
            return candidate;
        }
    }
    root.to_path_buf()
}

struct Winner<'a> {
    record: &'a Record,
    provenance: CreatureProvenance,
}

fn select_winners<'record>(
    records: &[LegacyRecordSource<'record>],
    interner: &StringInterner,
) -> Result<BTreeMap<StableFormKey, Winner<'record>>, CreatureCatalogError> {
    let mut winners = BTreeMap::<StableFormKey, Winner<'record>>::new();
    for source in records {
        let key = stable_form_key(source.record.form_key, interner)?;
        if let Some(existing) = winners.get(&key) {
            match source
                .provenance
                .precedence
                .cmp(&existing.provenance.precedence)
            {
                std::cmp::Ordering::Less => continue,
                std::cmp::Ordering::Equal => {
                    return Err(CreatureCatalogError::WinnerCollision {
                        record: key,
                        precedence: source.provenance.precedence,
                        first_plugin: existing.provenance.source_plugin.clone(),
                        second_plugin: source.provenance.source_plugin.clone(),
                    });
                }
                std::cmp::Ordering::Greater => {}
            }
        }
        winners.insert(
            key,
            Winner {
                record: source.record,
                provenance: source.provenance.clone(),
            },
        );
    }
    Ok(winners)
}

#[derive(Clone)]
enum PathEvidence {
    Absent,
    Resolved(String),
    Invalid,
}

#[derive(Clone)]
struct RecordSeed {
    model: PathEvidence,
    body_paths: Vec<String>,
    nift: Vec<u8>,
    nifz_supported: bool,
    nift_supported: bool,
    template: Option<StableFormKey>,
    template_malformed: bool,
    dependencies: Vec<StableFormKey>,
}

fn record_seed(record: &Record, key: &StableFormKey, interner: &StringInterner) -> RecordSeed {
    let model_values = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "MODL")
        .flat_map(|field| strings_from_value(&field.value, interner))
        .filter_map(|value| normalize_asset_path(&value))
        .collect::<Vec<_>>();
    let model = match model_values.as_slice() {
        [] if record
            .fields
            .iter()
            .any(|field| field.sig.as_str() == "MODL") =>
        {
            PathEvidence::Invalid
        }
        [] => PathEvidence::Absent,
        [only] => PathEvidence::Resolved(only.clone()),
        _ => PathEvidence::Invalid,
    };
    let model_parent = match &model {
        PathEvidence::Resolved(path) => path.rsplit_once('/').map(|(parent, _)| parent),
        _ => None,
    };
    let nifz_fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "NIFZ")
        .collect::<Vec<_>>();
    let nifz_supported = nifz_fields
        .iter()
        .all(|field| supports_string_value(&field.value));
    let mut body_paths = nifz_fields
        .iter()
        .flat_map(|field| strings_from_value(&field.value, interner))
        .filter_map(|body| resolve_body_path(&body, model_parent))
        .collect::<Vec<_>>();
    let mut seen_body_paths = BTreeSet::new();
    body_paths.retain(|path| seen_body_paths.insert(path.clone()));
    let nift_field = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "NIFT");
    let nift = nift_field
        .and_then(|field| bytes_from_value(&field.value))
        .unwrap_or_default();
    let nift_supported = nift_field.is_none_or(|field| bytes_from_value(&field.value).is_some());
    let template_values = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "TPLT")
        .flat_map(|field| form_keys_from_value(&field.value))
        .filter_map(|form_key| stable_form_key(form_key, interner).ok())
        .collect::<Vec<_>>();
    let template = (template_values.len() == 1).then(|| template_values[0].clone());
    let template_malformed = record
        .fields
        .iter()
        .any(|field| field.sig.as_str() == "TPLT")
        && template.is_none();
    let mut dependencies = record
        .fields
        .iter()
        .flat_map(|field| form_keys_from_value(&field.value))
        .filter_map(|form_key| stable_form_key(form_key, interner).ok())
        .filter(|dependency| dependency != key)
        .collect::<Vec<_>>();
    dependencies.sort();
    dependencies.dedup();
    RecordSeed {
        model,
        body_paths,
        nift,
        nifz_supported,
        nift_supported,
        template,
        template_malformed,
        dependencies,
    }
}

#[derive(Clone, Default)]
struct ProxyResolution {
    owners: BTreeSet<StableFormKey>,
    blockers: BTreeSet<ProxyBlocker>,
}

fn resolve_proxy(
    key: &StableFormKey,
    winners: &BTreeMap<StableFormKey, Winner<'_>>,
    seeds: &BTreeMap<StableFormKey, RecordSeed>,
    memo: &mut BTreeMap<StableFormKey, ProxyResolution>,
    stack: &mut Vec<StableFormKey>,
    interner: &StringInterner,
) -> ProxyResolution {
    if let Some(cached) = memo.get(key) {
        return cached.clone();
    }
    if let Some(cycle_start) = stack.iter().position(|candidate| candidate == key) {
        let mut cycle = stack[cycle_start..].to_vec();
        cycle.push(key.clone());
        return ProxyResolution {
            blockers: BTreeSet::from([ProxyBlocker::TemplateCycle(cycle)]),
            ..ProxyResolution::default()
        };
    }
    let Some(winner) = winners.get(key) else {
        return ProxyResolution {
            blockers: BTreeSet::from([ProxyBlocker::MissingTarget(key.clone())]),
            ..ProxyResolution::default()
        };
    };
    stack.push(key.clone());
    let resolution = match winner.record.sig.as_str() {
        "CREA" => resolve_creature_proxy(key, winners, seeds, memo, stack, interner),
        "LVLC" => resolve_leveled_proxy(key, winner.record, winners, seeds, memo, stack, interner),
        signature => ProxyResolution {
            blockers: BTreeSet::from([ProxyBlocker::UnsupportedTarget {
                target: key.clone(),
                signature: signature.to_owned(),
            }]),
            ..ProxyResolution::default()
        },
    };
    stack.pop();
    if !resolution
        .blockers
        .iter()
        .any(|blocker| matches!(blocker, ProxyBlocker::TemplateCycle(cycle) if cycle.contains(key)))
    {
        memo.insert(key.clone(), resolution.clone());
    }
    resolution
}

fn resolve_creature_proxy(
    key: &StableFormKey,
    winners: &BTreeMap<StableFormKey, Winner<'_>>,
    seeds: &BTreeMap<StableFormKey, RecordSeed>,
    memo: &mut BTreeMap<StableFormKey, ProxyResolution>,
    stack: &mut Vec<StableFormKey>,
    interner: &StringInterner,
) -> ProxyResolution {
    let seed = seeds.get(key).expect("winner seed exists");
    match &seed.model {
        PathEvidence::Resolved(model) if is_skeleton_path(model) => ProxyResolution {
            owners: BTreeSet::from([key.clone()]),
            ..ProxyResolution::default()
        },
        PathEvidence::Resolved(_) | PathEvidence::Invalid => ProxyResolution {
            blockers: BTreeSet::from([ProxyBlocker::TerminalSpecial(key.clone())]),
            ..ProxyResolution::default()
        },
        PathEvidence::Absent if seed.template_malformed => ProxyResolution {
            blockers: BTreeSet::from([ProxyBlocker::MalformedTemplate(key.clone())]),
            ..ProxyResolution::default()
        },
        PathEvidence::Absent => seed.template.as_ref().map_or_else(
            || ProxyResolution {
                blockers: BTreeSet::from([ProxyBlocker::TerminalSpecial(key.clone())]),
                ..ProxyResolution::default()
            },
            |target| resolve_proxy(target, winners, seeds, memo, stack, interner),
        ),
    }
}

fn resolve_leveled_proxy(
    key: &StableFormKey,
    record: &Record,
    winners: &BTreeMap<StableFormKey, Winner<'_>>,
    seeds: &BTreeMap<StableFormKey, RecordSeed>,
    memo: &mut BTreeMap<StableFormKey, ProxyResolution>,
    stack: &mut Vec<StableFormKey>,
    interner: &StringInterner,
) -> ProxyResolution {
    let entries = record
        .fields
        .iter()
        .filter(|field| matches!(field.sig.as_str(), "LVLO" | "LVLE"))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return ProxyResolution {
            blockers: BTreeSet::from([ProxyBlocker::EmptyLeveledList(key.clone())]),
            ..ProxyResolution::default()
        };
    }
    let mut resolution = ProxyResolution::default();
    for entry in entries {
        let targets = leveled_entry_form_keys(&entry.value, interner);
        if targets.is_empty() {
            resolution
                .blockers
                .insert(ProxyBlocker::UndecodableLeveledEntry(key.clone()));
            continue;
        }
        for target in targets {
            let Ok(target) = stable_form_key(target, interner) else {
                resolution
                    .blockers
                    .insert(ProxyBlocker::UndecodableLeveledEntry(key.clone()));
                continue;
            };
            let nested = resolve_proxy(&target, winners, seeds, memo, stack, interner);
            resolution.owners.extend(nested.owners);
            resolution.blockers.extend(nested.blockers);
        }
    }
    resolution
}

struct RigAssetScan {
    direct_kfs: Vec<String>,
    recursive_kfs: Vec<String>,
    readiness: AssetReadiness,
}

fn scan_rig_assets(skeleton_path: &str, mesh_root: Option<&Path>) -> RigAssetScan {
    let Some(mesh_root) = mesh_root else {
        return RigAssetScan {
            direct_kfs: Vec::new(),
            recursive_kfs: Vec::new(),
            readiness: AssetReadiness::AssetRootNotConfigured,
        };
    };
    let skeleton = join_asset_path(mesh_root, skeleton_path);
    if !skeleton.is_file() {
        return RigAssetScan {
            direct_kfs: Vec::new(),
            recursive_kfs: Vec::new(),
            readiness: AssetReadiness::MissingSkeleton(skeleton_path.to_owned()),
        };
    }
    let directory = skeleton.parent().unwrap_or(mesh_root);
    let mut direct = Vec::new();
    let mut recursive = Vec::new();
    let mut errors = Vec::new();
    collect_kfs(
        directory,
        mesh_root,
        directory,
        &mut direct,
        &mut recursive,
        &mut errors,
    );
    direct.sort();
    direct.dedup();
    recursive.sort();
    recursive.dedup();
    RigAssetScan {
        direct_kfs: direct,
        recursive_kfs: recursive,
        readiness: if errors.is_empty() {
            AssetReadiness::Ready
        } else {
            errors.sort();
            errors.dedup();
            AssetReadiness::ScanFailed(errors)
        },
    }
}

fn collect_kfs(
    directory: &Path,
    mesh_root: &Path,
    family_root: &Path,
    direct: &mut Vec<String>,
    recursive: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("{}: {error}", directory.display()));
            return;
        }
    };
    let mut paths = entries
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry.path()),
            Err(error) => {
                errors.push(format!("{}: {error}", directory.display()));
                None
            }
        })
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_kfs(&path, mesh_root, family_root, direct, recursive, errors);
            continue;
        }
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kf"))
        {
            continue;
        }
        let relative = path
            .strip_prefix(mesh_root)
            .ok()
            .and_then(|path| normalize_asset_path(&path.to_string_lossy()));
        if let Some(relative) = relative {
            if path.parent() == Some(family_root) {
                direct.push(relative.clone());
            }
            recursive.push(relative);
        }
    }
}

fn body_readiness(key: &BodyVariantKey, mesh_root: Option<&Path>) -> (BodyReadiness, Vec<String>) {
    let dependencies = key.body_paths.clone();
    if !key.nifz_supported {
        return (BodyReadiness::UnsupportedNifzEncoding, dependencies);
    }
    if !key.nift_supported {
        return (BodyReadiness::UnsupportedNiftEncoding, dependencies);
    }
    if key.body_paths.is_empty() {
        return (BodyReadiness::MissingBodyDeclaration, dependencies);
    }
    let Some(mesh_root) = mesh_root else {
        return (BodyReadiness::AssetRootNotConfigured, dependencies);
    };
    let missing = key
        .body_paths
        .iter()
        .filter(|path| !join_asset_path(mesh_root, path).is_file())
        .cloned()
        .collect::<Vec<_>>();
    if missing.len() < key.body_paths.len() {
        (BodyReadiness::Ready, dependencies)
    } else {
        (BodyReadiness::MissingAssets(missing), dependencies)
    }
}

fn build_attack_set(rig: &RigFamilyKey, animations: &[String]) -> AttackSet {
    let family_kind = family_kind(&rig.skeleton_path);
    let mut melee = Vec::new();
    let mut ranged = Vec::new();
    let mut looping = Vec::new();
    for path in animations {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        let is_ranged = ["fire", "shoot", "spit", "throw", "attackauto", "attackloop"]
            .iter()
            .any(|token| name.contains(token));
        let is_attack = name.contains("attack") || is_ranged;
        if is_ranged {
            ranged.push(path.clone());
        } else if is_attack {
            melee.push(path.clone());
        }
        if is_attack && (name.contains("loop") || name.contains("auto")) {
            looping.push(path.clone());
        }
    }
    let profile = match (melee.is_empty(), ranged.is_empty()) {
        (false, true) => AttackProfile::Melee,
        (true, false) => AttackProfile::Ranged,
        (false, false) => AttackProfile::Mixed,
        (true, true) => AttackProfile::Undetermined,
    };
    let graph_readiness = match family_kind {
        CreatureFamilyKind::RobotOrTurret => GraphReadiness::NeedsRangedGraph,
        CreatureFamilyKind::HumanoidWeaponOverlay => GraphReadiness::NeedsWeaponOverlayGraph,
        CreatureFamilyKind::Organic if profile == AttackProfile::Melee => {
            GraphReadiness::NeedsMeleeGraph
        }
        CreatureFamilyKind::Organic
            if matches!(profile, AttackProfile::Ranged | AttackProfile::Mixed) =>
        {
            GraphReadiness::NeedsRangedGraph
        }
        CreatureFamilyKind::Organic => GraphReadiness::NeedsClipClassification,
    };
    AttackSet {
        rig: rig.clone(),
        family_kind,
        profile,
        animation_paths: animations.to_vec(),
        melee_candidates: melee,
        ranged_candidates: ranged,
        looping_candidates: looping,
        graph_readiness,
    }
}

fn family_kind(path: &str) -> CreatureFamilyKind {
    let lower = path.to_ascii_lowercase();
    if [
        "robot",
        "gutsy",
        "protectron",
        "securitron",
        "sentry",
        "eyebot",
        "brainbot",
        "braintank",
        "computer",
        "edeclone",
        "robo",
    ]
    .iter()
    .any(|token| lower.contains(token))
    {
        CreatureFamilyKind::RobotOrTurret
    } else if [
        "characters/_male",
        "smspinebreaker",
        "smbonecrusher",
        "alien",
    ]
    .iter()
    .any(|token| lower.contains(token))
    {
        CreatureFamilyKind::HumanoidWeaponOverlay
    } else {
        CreatureFamilyKind::Organic
    }
}

fn account(
    input_records: usize,
    winning_records: usize,
    records: &[RecordVariant],
    rig_families: usize,
    body_variants: usize,
    attack_sets: usize,
) -> CorpusAccounting {
    let mut accounting = CorpusAccounting {
        input_records,
        winning_records,
        winning_creatures: records.len(),
        dispositions: records.len(),
        rig_families,
        body_variants,
        attack_sets,
        ..CorpusAccounting::default()
    };
    for record in records {
        match record.provenance.game {
            LegacyCreatureGame::Fnv => accounting.fnv_creatures += 1,
            LegacyCreatureGame::Fo3 => accounting.fo3_creatures += 1,
        }
        match &record.disposition {
            CreatureDisposition::VisualOwner { .. } => accounting.visual_owners += 1,
            CreatureDisposition::Proxy { readiness, .. } => {
                accounting.proxies += 1;
                if matches!(readiness, ProxyReadiness::Blocked(_)) {
                    accounting.blocked_proxies += 1;
                }
            }
            CreatureDisposition::Special { .. } => accounting.specials += 1,
        }
    }
    accounting
}

fn validate_accounting(accounting: &CorpusAccounting) -> Result<(), CreatureCatalogError> {
    if accounting.dispositions != accounting.winning_creatures {
        return Err(CreatureCatalogError::AccountingDrift {
            detail: format!(
                "{} dispositions for {} winning CREA",
                accounting.dispositions, accounting.winning_creatures
            ),
        });
    }
    if accounting.visual_owners + accounting.proxies + accounting.specials
        != accounting.winning_creatures
    {
        return Err(CreatureCatalogError::AccountingDrift {
            detail: "visual/proxy/special partition does not cover every winner".to_owned(),
        });
    }
    if accounting.fnv_creatures + accounting.fo3_creatures != accounting.winning_creatures {
        return Err(CreatureCatalogError::AccountingDrift {
            detail: "FNV/FO3 provenance partition does not cover every winner".to_owned(),
        });
    }
    if accounting.rig_families != accounting.attack_sets {
        return Err(CreatureCatalogError::AccountingDrift {
            detail: "every rig family must have exactly one attack set".to_owned(),
        });
    }
    Ok(())
}

fn stable_form_key(
    form_key: FormKey,
    interner: &StringInterner,
) -> Result<StableFormKey, CreatureCatalogError> {
    Ok(StableFormKey {
        local: form_key.local,
        plugin: interner
            .resolve(form_key.plugin)
            .ok_or(CreatureCatalogError::UnresolvedPlugin)?
            .to_ascii_lowercase(),
    })
}

fn strings_from_value(value: &FieldValue, interner: &StringInterner) -> Vec<String> {
    match value {
        FieldValue::String(symbol) => interner
            .resolve(*symbol)
            .map(|value| vec![value.to_owned()])
            .unwrap_or_default(),
        FieldValue::Bytes(bytes) => bytes
            .split(|byte| *byte == 0)
            .filter(|value| !value.is_empty())
            .filter_map(|value| std::str::from_utf8(value).ok().map(str::to_owned))
            .collect(),
        FieldValue::List(values) => values
            .iter()
            .flat_map(|value| strings_from_value(value, interner))
            .collect(),
        FieldValue::Struct(fields) => fields
            .iter()
            .flat_map(|(_, value)| strings_from_value(value, interner))
            .collect(),
        _ => Vec::new(),
    }
}

fn bytes_from_value(value: &FieldValue) -> Option<Vec<u8>> {
    match value {
        FieldValue::None => Some(Vec::new()),
        FieldValue::Bytes(bytes) => Some(bytes.to_vec()),
        FieldValue::Uint(value) => u32::try_from(*value)
            .ok()
            .map(u32::to_le_bytes)
            .map(Vec::from),
        FieldValue::Int(value) => u32::try_from(*value)
            .ok()
            .map(u32::to_le_bytes)
            .map(Vec::from),
        _ => None,
    }
}

fn supports_string_value(value: &FieldValue) -> bool {
    match value {
        FieldValue::String(_) | FieldValue::Bytes(_) => true,
        FieldValue::List(values) => values.iter().all(supports_string_value),
        _ => false,
    }
}

fn form_keys_from_value(value: &FieldValue) -> Vec<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => vec![*form_key],
        FieldValue::List(values) => values.iter().flat_map(form_keys_from_value).collect(),
        FieldValue::Struct(fields) => fields
            .iter()
            .flat_map(|(_, value)| form_keys_from_value(value))
            .collect(),
        _ => Vec::new(),
    }
}

fn leveled_entry_form_keys(value: &FieldValue, interner: &StringInterner) -> Vec<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => vec![*form_key],
        FieldValue::List(values) => values
            .iter()
            .flat_map(|value| leveled_entry_form_keys(value, interner))
            .collect(),
        FieldValue::Struct(fields) => fields
            .iter()
            .filter(|(name, _)| {
                interner.resolve(*name).is_some_and(|name| {
                    matches!(
                        name.to_ascii_lowercase().as_str(),
                        "reference" | "creature" | "npc" | "item"
                    )
                })
            })
            .flat_map(|(_, value)| form_keys_from_value(value))
            .collect(),
        FieldValue::Bytes(_) => Vec::new(),
        _ => Vec::new(),
    }
}

fn normalize_asset_path(path: &str) -> Option<String> {
    let normalized = path.trim().replace('\\', "/");
    let mut parts = normalized
        .split('/')
        .filter(|part| !part.is_empty() && !part.eq_ignore_ascii_case("meshes"))
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| matches!(part.as_str(), "." | "..")) {
        return None;
    }
    if parts.first().is_some_and(|part| part.ends_with(':')) {
        return None;
    }
    Some(parts.drain(..).collect::<Vec<_>>().join("/"))
}

fn resolve_body_path(body: &str, model_parent: Option<&str>) -> Option<String> {
    let body = normalize_asset_path(body)?;
    if body.contains('/') {
        Some(body)
    } else {
        model_parent.map_or(Some(body.clone()), |parent| {
            Some(format!("{parent}/{body}"))
        })
    }
}

fn is_skeleton_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("skeleton") && name.ends_with(".nif"))
}

fn join_asset_path(mesh_root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(mesh_root.to_path_buf(), |path, part| path.join(part))
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;
    use tempfile::TempDir;

    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    #[test]
    fn merged_winner_census_matches_cross_game_dedup_accounting() {
        assert_eq!(
            EXPECTED_FULL_SOURCE_CREA_WINNERS - EXPECTED_FULL_MERGED_CREA_WINNERS,
            446
        );
    }

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn record(
        signature: &str,
        local: u32,
        plugin: &str,
        editor_id: &str,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str(signature).unwrap(),
            fk(local, plugin, interner),
        );
        record.eid = Some(interner.intern(editor_id));
        record.fields = SmallVec::from_vec(fields);
        record
    }

    fn source<'a>(
        record: &'a Record,
        game: LegacyCreatureGame,
        source_plugin: &str,
        precedence: u32,
    ) -> LegacyRecordSource<'a> {
        LegacyRecordSource {
            record,
            provenance: CreatureProvenance {
                game,
                source_plugin: source_plugin.to_owned(),
                precedence,
            },
        }
    }

    #[test]
    fn game_namespaced_roots_keep_identical_runtime_paths_source_distinct() {
        let temp = TempDir::new().unwrap();
        let relative = Path::new("creatures").join("shared").join("skeleton.nif");
        let fnv_root = temp.path().join("fnv").join("Meshes");
        let fo3_root = temp.path().join("fo3").join("Meshes");
        fs::create_dir_all(fnv_root.join(relative.parent().unwrap())).unwrap();
        fs::create_dir_all(fo3_root.join(relative.parent().unwrap())).unwrap();
        fs::write(fnv_root.join(&relative), b"fnv").unwrap();
        fs::write(fo3_root.join(&relative), b"fo3").unwrap();

        let resolved_fnv = game_mesh_root(temp.path(), LegacyCreatureGame::Fnv);
        let resolved_fo3 = game_mesh_root(temp.path(), LegacyCreatureGame::Fo3);
        assert_ne!(resolved_fnv, resolved_fo3);
        assert_eq!(fs::read(resolved_fnv.join(&relative)).unwrap(), b"fnv");
        assert_eq!(fs::read(resolved_fo3.join(&relative)).unwrap(), b"fo3");
    }

    fn visual(
        local: u32,
        plugin: &str,
        editor_id: &str,
        skeleton: &str,
        body: &str,
        interner: &StringInterner,
    ) -> Record {
        record(
            "CREA",
            local,
            plugin,
            editor_id,
            vec![
                field("MODL", FieldValue::String(interner.intern(skeleton))),
                field(
                    "NIFZ",
                    FieldValue::Bytes(SmallVec::from_slice(body.as_bytes())),
                ),
                field(
                    "NIFT",
                    FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 0, 0])),
                ),
            ],
            interner,
        )
    }

    #[test]
    fn accounts_for_every_expected_merged_winner_exactly_once() {
        let interner = StringInterner::new();
        let records = (0..EXPECTED_FULL_MERGED_CREA_WINNERS)
            .map(|offset| {
                record(
                    "CREA",
                    0x800 + offset as u32,
                    "FalloutNV.esm",
                    &format!("Creature{offset}"),
                    Vec::new(),
                    &interner,
                )
            })
            .collect::<Vec<_>>();
        let sources = records
            .iter()
            .map(|record| source(record, LegacyCreatureGame::Fnv, "FalloutNV.esm", 0))
            .collect::<Vec<_>>();
        let plan = build_full_merged_creature_corpus_plan(&sources, None, &interner).unwrap();
        assert_eq!(
            plan.accounting.winning_creatures,
            EXPECTED_FULL_MERGED_CREA_WINNERS
        );
        assert_eq!(
            plan.accounting.dispositions,
            EXPECTED_FULL_MERGED_CREA_WINNERS
        );
        assert_eq!(plan.accounting.specials, EXPECTED_FULL_MERGED_CREA_WINNERS);
        assert_eq!(plan.records.len(), EXPECTED_FULL_MERGED_CREA_WINNERS);
    }

    #[test]
    fn resolves_crea_template_and_nested_lvlc_proxies_and_reports_cycles() {
        let interner = StringInterner::new();
        let owner = visual(
            0x800,
            "FalloutNV.esm",
            "Owner",
            "creatures\\dog\\skeleton.nif",
            "dog.nif\0",
            &interner,
        );
        let list = record(
            "LVLC",
            0x900,
            "FalloutNV.esm",
            "DogList",
            vec![field(
                "LVLO",
                FieldValue::Struct(vec![(
                    interner.intern("reference"),
                    FieldValue::FormKey(owner.form_key),
                )]),
            )],
            &interner,
        );
        let proxy = record(
            "CREA",
            0x801,
            "FalloutNV.esm",
            "Proxy",
            vec![field("TPLT", FieldValue::FormKey(list.form_key))],
            &interner,
        );
        let chained = record(
            "CREA",
            0x802,
            "FalloutNV.esm",
            "ChainedProxy",
            vec![field("TPLT", FieldValue::FormKey(proxy.form_key))],
            &interner,
        );
        let cycle_a = record(
            "CREA",
            0x803,
            "FalloutNV.esm",
            "CycleA",
            vec![field(
                "TPLT",
                FieldValue::FormKey(fk(0x804, "FalloutNV.esm", &interner)),
            )],
            &interner,
        );
        let cycle_b = record(
            "CREA",
            0x804,
            "FalloutNV.esm",
            "CycleB",
            vec![field("TPLT", FieldValue::FormKey(cycle_a.form_key))],
            &interner,
        );
        let leveled_cycle_a = record(
            "LVLC",
            0x901,
            "FalloutNV.esm",
            "LeveledCycleA",
            vec![field(
                "LVLO",
                FieldValue::Struct(vec![(
                    interner.intern("reference"),
                    FieldValue::FormKey(fk(0x902, "FalloutNV.esm", &interner)),
                )]),
            )],
            &interner,
        );
        let leveled_cycle_b = record(
            "LVLC",
            0x902,
            "FalloutNV.esm",
            "LeveledCycleB",
            vec![field(
                "LVLO",
                FieldValue::Struct(vec![(
                    interner.intern("reference"),
                    FieldValue::FormKey(leveled_cycle_a.form_key),
                )]),
            )],
            &interner,
        );
        let leveled_cycle_proxy = record(
            "CREA",
            0x805,
            "FalloutNV.esm",
            "LeveledCycleProxy",
            vec![field("TPLT", FieldValue::FormKey(leveled_cycle_a.form_key))],
            &interner,
        );
        let records = [
            &owner,
            &list,
            &proxy,
            &chained,
            &cycle_a,
            &cycle_b,
            &leveled_cycle_a,
            &leveled_cycle_b,
            &leveled_cycle_proxy,
        ];
        let sources = records
            .into_iter()
            .map(|record| source(record, LegacyCreatureGame::Fnv, "FalloutNV.esm", 0))
            .collect::<Vec<_>>();
        let plan =
            build_creature_corpus_plan(&sources, CreatureCatalogOptions::default(), &interner)
                .unwrap();
        let proxy = plan
            .records
            .iter()
            .find(|record| record.editor_id.as_deref() == Some("Proxy"))
            .unwrap();
        let CreatureDisposition::Proxy {
            terminal_visual_owners,
            readiness,
        } = &proxy.disposition
        else {
            panic!("proxy disposition");
        };
        assert_eq!(
            terminal_visual_owners,
            &[stable_form_key(owner.form_key, &interner).unwrap()]
        );
        assert_eq!(readiness, &ProxyReadiness::Ready);
        let chained = plan
            .records
            .iter()
            .find(|record| record.editor_id.as_deref() == Some("ChainedProxy"))
            .unwrap();
        assert!(matches!(
            chained.disposition,
            CreatureDisposition::Proxy {
                readiness: ProxyReadiness::Ready,
                ..
            }
        ));
        assert_eq!(plan.accounting.blocked_proxies, 3);
        for editor_id in ["CycleA", "CycleB"] {
            let record = plan
                .records
                .iter()
                .find(|record| record.editor_id.as_deref() == Some(editor_id))
                .unwrap();
            assert!(matches!(
                record.disposition,
                CreatureDisposition::Proxy { readiness: ProxyReadiness::Blocked(ref blockers), .. }
                    if blockers.iter().any(|blocker| matches!(blocker, ProxyBlocker::TemplateCycle(_)))
            ));
        }
        let leveled_cycle_proxy = plan
            .records
            .iter()
            .find(|record| record.editor_id.as_deref() == Some("LeveledCycleProxy"))
            .unwrap();
        assert!(matches!(
            leveled_cycle_proxy.disposition,
            CreatureDisposition::Proxy { readiness: ProxyReadiness::Blocked(ref blockers), .. }
                if blockers.iter().any(|blocker| matches!(blocker, ProxyBlocker::TemplateCycle(cycle)
                    if cycle.contains(&stable_form_key(leveled_cycle_a.form_key, &interner).unwrap())
                        && cycle.contains(&stable_form_key(leveled_cycle_b.form_key, &interner).unwrap())))
        ));
    }

    #[test]
    fn dlc_robot_and_missing_body_remain_typed_catalog_entries() {
        let interner = StringInterner::new();
        let temp = TempDir::new().unwrap();
        let family = temp.path().join("nvdlc03/creatures/brainbot");
        fs::create_dir_all(family.join("idleanims")).unwrap();
        fs::write(family.join("skeleton.nif"), b"nif").unwrap();
        fs::write(family.join("attackloop.kf"), b"kf").unwrap();
        fs::write(family.join("idleanims/idle.kf"), b"kf").unwrap();
        let robot = visual(
            0x900,
            "OldWorldBlues.esm",
            "NVDLC03Brainbot",
            "NVDLC03\\Creatures\\BrainBot\\Skeleton.nif",
            "missing_brainbot.nif\0",
            &interner,
        );
        let sources = [source(
            &robot,
            LegacyCreatureGame::Fnv,
            "OldWorldBlues.esm",
            3,
        )];
        let plan = build_creature_corpus_plan(
            &sources,
            CreatureCatalogOptions {
                mesh_root: Some(temp.path()),
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert_eq!(plan.accounting.fnv_creatures, 1);
        assert_eq!(plan.rig_families[0].recursive_kf_paths.len(), 2);
        assert_eq!(
            plan.attack_sets[0].family_kind,
            CreatureFamilyKind::RobotOrTurret
        );
        assert_eq!(
            plan.attack_sets[0].graph_readiness,
            GraphReadiness::NeedsRangedGraph
        );
        assert!(matches!(
            plan.body_variants[0].readiness,
            BodyReadiness::MissingAssets(ref missing)
                if missing == &["nvdlc03/creatures/brainbot/missing_brainbot.nif"]
        ));
    }

    #[test]
    fn body_variant_is_ready_when_at_least_one_declared_part_exists() {
        let temp = TempDir::new().unwrap();
        let family = temp.path().join("creatures/ghoul");
        fs::create_dir_all(&family).unwrap();
        fs::write(family.join("ghoulvariant.nif"), b"nif").unwrap();
        let key = BodyVariantKey {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/ghoul/skeleton.nif".to_string(),
            },
            body_paths: vec![
                "creatures/ghoul/ghoulvariant.nif".to_string(),
                "creatures/ghoul/mmm_ghoulvariant_v1.nif".to_string(),
            ],
            nift: Vec::new(),
            nifz_supported: true,
            nift_supported: true,
        };

        assert_eq!(
            body_readiness(&key, Some(temp.path())).0,
            BodyReadiness::Ready
        );
    }

    #[test]
    fn fnv_and_grafted_fo3_path_collision_produce_distinct_rig_families() {
        let interner = StringInterner::new();
        let fnv = visual(
            0x800,
            "FalloutNV.esm",
            "FNVRobot",
            "creatures\\protectron\\skeleton.nif",
            "protectron.nif\0",
            &interner,
        );
        let fo3 = visual(
            0x801,
            "Fallout3.esm",
            "FO3Robot",
            "creatures\\protectron\\skeleton.nif",
            "protectron.nif\0",
            &interner,
        );
        let sources = [
            source(&fnv, LegacyCreatureGame::Fnv, "FalloutNV.esm", 0),
            source(&fo3, LegacyCreatureGame::Fo3, "Fallout3.esm", 0),
        ];
        let plan =
            build_creature_corpus_plan(&sources, CreatureCatalogOptions::default(), &interner)
                .unwrap();
        assert_eq!(plan.rig_families.len(), 2);
        assert_eq!(plan.accounting.fnv_creatures, 1);
        assert_eq!(plan.accounting.fo3_creatures, 1);
        assert_ne!(plan.rig_families[0].key, plan.rig_families[1].key);
    }

    #[test]
    fn equal_precedence_override_collision_fails_closed() {
        let interner = StringInterner::new();
        let base = record(
            "CREA",
            0x800,
            "FalloutNV.esm",
            "Base",
            Vec::new(),
            &interner,
        );
        let override_record = record(
            "CREA",
            0x800,
            "FalloutNV.esm",
            "Override",
            Vec::new(),
            &interner,
        );
        let sources = [
            source(&base, LegacyCreatureGame::Fnv, "FalloutNV.esm", 1),
            source(
                &override_record,
                LegacyCreatureGame::Fnv,
                "DeadMoney.esm",
                1,
            ),
        ];
        assert!(matches!(
            build_creature_corpus_plan(&sources, CreatureCatalogOptions::default(), &interner),
            Err(CreatureCatalogError::WinnerCollision { .. })
        ));
    }

    #[test]
    fn plan_order_is_independent_of_input_order() {
        let interner = StringInterner::new();
        let first = visual(
            0x802,
            "FalloutNV.esm",
            "SecondById",
            "creatures\\dog\\skeleton.nif",
            "dog.nif\0",
            &interner,
        );
        let second = visual(
            0x801,
            "FalloutNV.esm",
            "FirstById",
            "creatures\\brahmin\\skeleton.nif",
            "brahmin.nif\0",
            &interner,
        );
        let forward = [
            source(&first, LegacyCreatureGame::Fnv, "FalloutNV.esm", 0),
            source(&second, LegacyCreatureGame::Fnv, "FalloutNV.esm", 0),
        ];
        let reverse = [forward[1].clone(), forward[0].clone()];
        let left =
            build_creature_corpus_plan(&forward, CreatureCatalogOptions::default(), &interner)
                .unwrap();
        let right =
            build_creature_corpus_plan(&reverse, CreatureCatalogOptions::default(), &interner)
                .unwrap();
        assert_eq!(left, right);
        assert_eq!(left.records[0].source.local, 0x801);
    }
}
