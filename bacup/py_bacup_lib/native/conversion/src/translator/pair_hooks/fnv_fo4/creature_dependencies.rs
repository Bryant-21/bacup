//! Deterministic FNV/FO3 creature runtime-record dependency evidence.
//!
//! This is intentionally independent of target FormKey allocation, asset
//! conversion, and publication.  The pre-translation planner can therefore
//! install the exact source allow-set before records which require dedicated
//! legacy lowering would otherwise be skipped.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_conditions::LegacyConditionFamily;
use crate::translator::pair_hooks::fnv_pack::LegacyPackSourceFamily;

use super::creature_catalog::{
    CreatureProvenance, EXPECTED_FULL_MERGED_CREA_WINNERS, LegacyCreatureGame, LegacyRecordSource,
    StableFormKey,
};
use super::legacy_ammo::{
    LegacyAmmoSourceFamily, classify_legacy_ammo, decode_legacy_ammo_dat2,
    legacy_ammo_embedded_references, legacy_ammo_fallback_warnings,
    source_form_key_for_raw_form_id,
};
use super::legacy_ammo_effects::{
    LegacyAmmoEffectChannel, LegacyAmmoEffectOperation, LegacyAmmoEffectSupport,
    classify_legacy_ammo_effect,
};
use super::legacy_body_parts::{
    classify_legacy_body_part_data, legacy_body_part_embedded_references,
    legacy_body_part_fallback_warnings,
};
use super::legacy_combat_styles::{
    classify_legacy_combat_style, legacy_combat_style_fallback_warnings,
};
use super::legacy_debris::classify_legacy_debris;
use super::legacy_explosions::{
    classify_legacy_explosion, legacy_explosion_embedded_references,
    legacy_explosion_fallback_warnings,
};
use super::legacy_factions::{
    classify_legacy_faction, legacy_faction_embedded_references, legacy_faction_fallback_warnings,
};
use super::legacy_idles::{
    classify_legacy_idle, legacy_idle_embedded_references, legacy_idle_fallback_warnings,
};
use super::legacy_impact_datasets::{
    classify_legacy_impact_dataset, legacy_impact_dataset_embedded_references,
};
use super::legacy_impacts::{
    classify_legacy_impact, classify_legacy_texture_set, legacy_impact_fallback_warnings,
    legacy_texture_set_fallback_warnings,
};
use super::legacy_keys::classify_legacy_key;
use super::legacy_leveled_items::{
    classify_legacy_leveled_item, legacy_leveled_item_embedded_references,
};
use super::legacy_misc::classify_legacy_misc;
use super::legacy_package_references::legacy_package_embedded_references;
use super::legacy_packages::{
    GenericLegacyPackageSupport, classify_generic_legacy_package, legacy_package_fallback_warnings,
};
use super::legacy_ragdolls::classify_legacy_ragdoll;
use super::leveled_creatures::classify_legacy_lvlc;
use super::weapon::classify_legacy_creature_weapon;

pub const FNV_FO3_CREATURE_DEPENDENCY_SCHEMA_VERSION: u32 = 4;

const REQUIRED_RUNTIME_DEPENDENCY_SIGNATURES: &[&str] = &[
    "CREA", "LVLC", "PACK", "CSTY", "SPEL", "MGEF", "ENCH", "ALCH", "AMEF", "WEAP", "PROJ", "AMMO",
    "ARMO", "ARMA", "BPTD", "BOOK", "CCRD", "CHIP", "CMNY", "IDLE", "IDLM", "ANIO", "FACT", "EYES",
    "HAIR", "HDPT", "IMOD", "KEYM", "LVLI", "LVSP", "MISC", "MSTT", "NOTE", "NPC_", "SCPT", "STAT",
    "EFSH", "LIGH", "SOUN", "SNDR", "IMGS", "IPCT", "IPDS", "EXPL", "DEBR", "AVIF", "GLOB", "FLST",
    "KYWD", "TXST", "RGDL", "GMST",
];

const SOURCE_RECORD_RESOLUTION_SIGNATURES: &[&str] = &[
    "ACHR", "ACRE", "ACTI", "ADDN", "ALCH", "ALOC", "AMEF", "AMMO", "ANIO", "ARMA", "ARMO", "ASPC",
    "AVIF", "BOOK", "BPTD", "CAMS", "CCRD", "CDCK", "CELL", "CHAL", "CHIP", "CLAS", "CLMT", "CMNY",
    "CONT", "CPTH", "CREA", "CSNO", "CSTY", "DEBR", "DEHY", "DIAL", "DOBJ", "DOOR", "ECZN", "EFSH",
    "ENCH", "EXPL", "EYES", "FACT", "FLOR", "FLST", "FURN", "GLOB", "GMST", "GRAS", "HAIR", "HDPT",
    "HUNG", "IDLE", "IDLM", "IMAD", "IMGS", "IMOD", "INGR", "IPCT", "IPDS", "KEYM", "KYWD", "LAND",
    "LGTM", "LIGH", "LSCR", "LSCT", "LVLC", "LVLI", "LVLN", "LVSP", "MESG", "MGEF", "MICN", "MISC",
    "MSET", "MSTT", "MUSC", "NAVI", "NAVM", "NOTE", "NPC_", "PACK", "PARW", "PBAR", "PBEA", "PCON",
    "PERK", "PGRE", "PHZD", "PMIS", "PROJ", "PWAT", "QUST", "RACE", "RADS", "RCCT", "RCPE", "REFR",
    "REGN", "REPU", "RGDL", "SCOL", "SCPT", "SLPD", "SNDR", "SOUN", "SPEL", "STAT", "TACT", "TERM",
    "TREE", "TXST", "VTYP", "WATR", "WEAP", "WRLD", "WTHR",
];

pub fn required_fnv_fo3_creature_dependency_signatures() -> &'static [&'static str] {
    SOURCE_RECORD_RESOLUTION_SIGNATURES
}

pub fn required_fnv_fo3_creature_runtime_dependency_signatures() -> &'static [&'static str] {
    REQUIRED_RUNTIME_DEPENDENCY_SIGNATURES
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum PairRecordLoweringDisposition {
    Ready { mechanism: String },
    Consumed { mechanism: String },
    Blocked { reason_codes: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum DependencyReferenceResolution {
    Followed {
        signature: String,
    },
    OutOfRuntimeScope {
        signature: String,
    },
    TargetIntrinsic {
        signature: String,
        mapped_target: StableFormKey,
    },
    MissingSourceRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyReferenceReceipt {
    pub target: StableFormKey,
    pub source_locators: Vec<String>,
    pub resolution: DependencyReferenceResolution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDependencyRecordReceipt {
    pub source: StableFormKey,
    pub signature: String,
    pub provenance: CreatureProvenance,
    pub lowering: PairRecordLoweringDisposition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degradations: Vec<crate::source_rig::CreatureDegradationReceipt>,
    pub references: Vec<DependencyReferenceReceipt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmmoEffectFoldChannel {
    Damage,
    Spread,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmmoEffectFoldOperation {
    Add,
    Multiply,
    Subtract,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AmmoEffectOwnerFoldReceipt {
    pub source_effect: StableFormKey,
    pub source_ammo: StableFormKey,
    pub consuming_weapon: StableFormKey,
    pub rcil_locator: String,
    pub ammo_locator: String,
    pub channel: AmmoEffectFoldChannel,
    pub operation: AmmoEffectFoldOperation,
    pub value_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum CreatureDependencyBlocker {
    MissingSourceRecord {
        source: StableFormKey,
    },
    LoweringUnavailable {
        source: StableFormKey,
        signature: String,
        reason_code: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureDependencyCandidateReceipt {
    pub source: StableFormKey,
    pub provenance: CreatureProvenance,
    pub dependency_seed_form_keys: Vec<StableFormKey>,
    pub closure_form_keys: Vec<StableFormKey>,
    pub ammo_effect_folds: Vec<AmmoEffectOwnerFoldReceipt>,
    pub blockers: Vec<CreatureDependencyBlocker>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FnvFo3CreatureDependencyLedger {
    pub schema_version: u32,
    pub candidates: Vec<CreatureDependencyCandidateReceipt>,
    pub records: Vec<RuntimeDependencyRecordReceipt>,
    pub winning_creatures: usize,
    pub ready_candidates: usize,
    pub blocked_candidates: usize,
    pub content_hash_blake3: String,
}

impl FnvFo3CreatureDependencyLedger {
    pub fn canonical_json(&self) -> Result<String, CreatureDependencyError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CreatureDependencyError::Serialization(error.to_string()))
    }

    pub fn validate(&self) -> Result<(), CreatureDependencyError> {
        if self.schema_version != FNV_FO3_CREATURE_DEPENDENCY_SCHEMA_VERSION {
            return Err(CreatureDependencyError::InvalidLedger(
                "unsupported schema version".to_string(),
            ));
        }
        if !strictly_sorted_unique(self.candidates.iter().map(|entry| &entry.source)) {
            return Err(CreatureDependencyError::InvalidLedger(
                "candidate order is not canonical and unique".to_string(),
            ));
        }
        if !strictly_sorted_unique(self.records.iter().map(|entry| &entry.source)) {
            return Err(CreatureDependencyError::InvalidLedger(
                "record order is not canonical and unique".to_string(),
            ));
        }
        if self.winning_creatures != self.candidates.len()
            || self.ready_candidates + self.blocked_candidates != self.winning_creatures
            || self.ready_candidates
                != self
                    .candidates
                    .iter()
                    .filter(|entry| entry.blockers.is_empty())
                    .count()
        {
            return Err(CreatureDependencyError::InvalidLedger(
                "candidate accounting drifted".to_string(),
            ));
        }
        let records = self
            .records
            .iter()
            .map(|entry| (&entry.source, entry))
            .collect::<BTreeMap<_, _>>();
        for candidate in &self.candidates {
            if !strictly_sorted_unique(candidate.dependency_seed_form_keys.iter())
                || !strictly_sorted_unique(candidate.closure_form_keys.iter())
                || !strictly_sorted_unique(candidate.ammo_effect_folds.iter())
                || !strictly_sorted_unique(candidate.blockers.iter())
                || candidate
                    .closure_form_keys
                    .iter()
                    .any(|source| !records.contains_key(source))
            {
                return Err(CreatureDependencyError::InvalidLedger(format!(
                    "candidate {} has noncanonical or unresolved closure evidence",
                    candidate.source
                )));
            }
            for fold in &candidate.ammo_effect_folds {
                let valid = candidate.closure_form_keys.contains(&fold.source_effect)
                    && candidate.closure_form_keys.contains(&fold.source_ammo)
                    && candidate.closure_form_keys.contains(&fold.consuming_weapon)
                    && records.get(&fold.source_effect).is_some_and(|record| {
                        record.signature == "AMEF"
                            && matches!(
                                record.lowering,
                                PairRecordLoweringDisposition::Consumed { .. }
                            )
                    })
                    && records
                        .get(&fold.source_ammo)
                        .is_some_and(|record| record.signature == "AMMO")
                    && records
                        .get(&fold.consuming_weapon)
                        .is_some_and(|record| record.signature == "WEAP")
                    && is_indexed_locator(&fold.rcil_locator, "RCIL")
                    && is_indexed_locator(&fold.ammo_locator, "NAM0");
                if !valid {
                    return Err(CreatureDependencyError::InvalidLedger(format!(
                        "candidate {} has an invalid ammo-effect owner fold",
                        candidate.source
                    )));
                }
            }
        }
        for record in &self.records {
            if !strictly_sorted_unique(record.references.iter().map(|entry| &entry.target)) {
                return Err(CreatureDependencyError::InvalidLedger(format!(
                    "record {} reference order is not canonical and unique",
                    record.source
                )));
            }
            if record.references.iter().any(|entry| {
                entry.source_locators.is_empty()
                    || !strictly_sorted_unique(entry.source_locators.iter())
            }) {
                return Err(CreatureDependencyError::InvalidLedger(format!(
                    "record {} has missing or noncanonical reference locators",
                    record.source
                )));
            }
            if record.references.iter().any(|entry| {
                matches!(
                    &entry.resolution,
                    DependencyReferenceResolution::TargetIntrinsic {
                        signature,
                        mapped_target,
                    } if !is_legacy_player_ref(&entry.target)
                        || !entry
                            .source_locators
                            .iter()
                            .all(|locator| is_legacy_player_intrinsic_locator(locator))
                        || signature != "REFR"
                        || mapped_target.plugin != "Fallout4.esm"
                        || mapped_target.local != 0x14
                )
            }) {
                return Err(CreatureDependencyError::InvalidLedger(format!(
                    "record {} has invalid target-intrinsic reference evidence",
                    record.source
                )));
            }
            match &record.lowering {
                PairRecordLoweringDisposition::Ready { mechanism }
                | PairRecordLoweringDisposition::Consumed { mechanism }
                    if mechanism.trim().is_empty() =>
                {
                    return Err(CreatureDependencyError::InvalidLedger(format!(
                        "record {} has an empty lowering mechanism",
                        record.source
                    )));
                }
                PairRecordLoweringDisposition::Blocked { reason_codes }
                    if reason_codes.is_empty() || !strictly_sorted_unique(reason_codes.iter()) =>
                {
                    return Err(CreatureDependencyError::InvalidLedger(format!(
                        "record {} has noncanonical lowering blockers",
                        record.source
                    )));
                }
                _ => {}
            }
        }
        if !canonical_blake3(&self.content_hash_blake3) {
            return Err(CreatureDependencyError::InvalidLedger(
                "content hash is not lowercase BLAKE3".to_string(),
            ));
        }
        let actual = self.compute_content_hash()?;
        if self.content_hash_blake3 != actual {
            return Err(CreatureDependencyError::HashMismatch {
                expected: self.content_hash_blake3.clone(),
                actual,
            });
        }
        Ok(())
    }

    fn compute_content_hash(&self) -> Result<String, CreatureDependencyError> {
        let mut unhashed = self.clone();
        unhashed.content_hash_blake3.clear();
        let bytes = serde_json::to_vec(&unhashed)
            .map_err(|error| CreatureDependencyError::Serialization(error.to_string()))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CreatureDependencyOptions {
    pub expected_creature_winners: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyPluginLoadOrder {
    pub game: LegacyCreatureGame,
    pub source_plugin: String,
    pub source_master_names: Vec<String>,
}

impl Default for CreatureDependencyOptions {
    fn default() -> Self {
        Self {
            expected_creature_winners: None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CreatureDependencyError {
    #[error("record FormKey uses an unresolved plugin symbol")]
    UnresolvedPlugin,
    #[error(
        "winning-record collision for {record} at precedence {precedence}: {first_plugin} and {second_plugin}"
    )]
    WinnerCollision {
        record: StableFormKey,
        precedence: u32,
        first_plugin: String,
        second_plugin: String,
    },
    #[error("merged CREA dependency census is {actual}, expected {expected}")]
    WinnerCountMismatch { expected: usize, actual: usize },
    #[error("creature dependency ledger is invalid: {0}")]
    InvalidLedger(String),
    #[error("creature dependency content hash mismatch: expected {expected}, actual {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("creature dependency evidence is not JSON-serializable: {0}")]
    Serialization(String),
    #[error("duplicate source load-order evidence for {source_plugin} ({game:?})")]
    DuplicateSourceLoadOrder {
        game: LegacyCreatureGame,
        source_plugin: String,
    },
}

pub fn build_full_merged_creature_dependency_ledger(
    records: &[LegacyRecordSource<'_>],
    interner: &StringInterner,
) -> Result<FnvFo3CreatureDependencyLedger, CreatureDependencyError> {
    build_creature_dependency_ledger_with_load_orders(
        records,
        CreatureDependencyOptions {
            expected_creature_winners: Some(EXPECTED_FULL_MERGED_CREA_WINNERS),
        },
        &[],
        interner,
    )
}

pub fn build_creature_dependency_ledger(
    records: &[LegacyRecordSource<'_>],
    options: CreatureDependencyOptions,
    interner: &StringInterner,
) -> Result<FnvFo3CreatureDependencyLedger, CreatureDependencyError> {
    build_creature_dependency_ledger_with_load_orders(records, options, &[], interner)
}

pub fn build_creature_dependency_ledger_with_load_orders(
    records: &[LegacyRecordSource<'_>],
    options: CreatureDependencyOptions,
    source_load_orders: &[LegacyPluginLoadOrder],
    interner: &StringInterner,
) -> Result<FnvFo3CreatureDependencyLedger, CreatureDependencyError> {
    build_creature_dependency_ledger_with_load_orders_and_exclusions(
        records,
        options,
        source_load_orders,
        &BTreeSet::new(),
        interner,
    )
}

pub fn build_creature_dependency_ledger_with_load_orders_and_exclusions(
    records: &[LegacyRecordSource<'_>],
    options: CreatureDependencyOptions,
    source_load_orders: &[LegacyPluginLoadOrder],
    excluded_signatures: &BTreeSet<String>,
    interner: &StringInterner,
) -> Result<FnvFo3CreatureDependencyLedger, CreatureDependencyError> {
    let source_load_orders = canonical_source_load_orders(source_load_orders)?;
    let excluded_signatures = excluded_signatures
        .iter()
        .map(|signature| signature.trim().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let winners = select_winners(records, interner)?;
    let creature_keys = winners
        .iter()
        .filter_map(|(source, winner)| {
            (winner.record.sig.as_str() == "CREA").then_some(source.clone())
        })
        .collect::<Vec<_>>();
    if let Some(expected) = options.expected_creature_winners
        && creature_keys.len() != expected
    {
        return Err(CreatureDependencyError::WinnerCountMismatch {
            expected,
            actual: creature_keys.len(),
        });
    }

    let mut record_receipts = BTreeMap::<StableFormKey, RuntimeDependencyRecordReceipt>::new();
    let mut candidates = Vec::with_capacity(creature_keys.len());
    for source in creature_keys {
        let winner = winners.get(&source).expect("CREA key came from winners");
        let seeds = runtime_references(
            winner.record,
            &winner.provenance,
            &winners,
            &source_load_orders,
            interner,
        )?;
        let dependency_seed_form_keys = seeds
            .iter()
            .map(|reference| reference.target.clone())
            .collect::<Vec<_>>();
        let mut queue = VecDeque::from([source.clone()]);
        queue.extend(dependency_seed_form_keys.iter().cloned());
        let mut visited = BTreeSet::new();
        let mut closure = BTreeSet::new();
        let mut blockers = BTreeSet::new();
        while let Some(key) = queue.pop_front() {
            if !visited.insert(key.clone()) {
                continue;
            }
            let Some(dependency) = winners.get(&key) else {
                blockers.insert(CreatureDependencyBlocker::MissingSourceRecord { source: key });
                continue;
            };
            if dependency.record.sig.as_str() != "CREA"
                && !is_runtime_dependency_signature(dependency.record.sig.as_str())
            {
                continue;
            }
            closure.insert(key.clone());
            let receipt =
                build_record_receipt(&key, dependency, &winners, &source_load_orders, interner)?;
            let explicitly_excluded = excluded_signatures.contains(&receipt.signature);
            if !explicitly_excluded
                && let PairRecordLoweringDisposition::Blocked { reason_codes } = &receipt.lowering
            {
                blockers.extend(reason_codes.iter().cloned().map(|reason_code| {
                    CreatureDependencyBlocker::LoweringUnavailable {
                        source: key.clone(),
                        signature: receipt.signature.clone(),
                        reason_code,
                    }
                }));
            }
            if !explicitly_excluded
                || matches!(
                    receipt.lowering,
                    PairRecordLoweringDisposition::Ready { .. }
                )
            {
                queue.extend(receipt.references.iter().filter_map(|reference| {
                    matches!(
                        reference.resolution,
                        DependencyReferenceResolution::Followed { .. }
                    )
                    .then(|| reference.target.clone())
                }));
            }
            record_receipts.entry(key).or_insert(receipt);
        }
        let (ammo_effect_folds, owner_fold_blockers) =
            build_candidate_ammo_effect_folds(&closure, &winners, interner)?;
        blockers.extend(owner_fold_blockers);
        candidates.push(CreatureDependencyCandidateReceipt {
            source,
            provenance: winner.provenance.clone(),
            dependency_seed_form_keys,
            closure_form_keys: closure.into_iter().collect(),
            ammo_effect_folds,
            blockers: blockers.into_iter().collect(),
        });
    }
    candidates.sort_by(|left, right| left.source.cmp(&right.source));
    let records = record_receipts.into_values().collect::<Vec<_>>();
    let ready_candidates = candidates
        .iter()
        .filter(|candidate| candidate.blockers.is_empty())
        .count();
    let mut ledger = FnvFo3CreatureDependencyLedger {
        schema_version: FNV_FO3_CREATURE_DEPENDENCY_SCHEMA_VERSION,
        winning_creatures: candidates.len(),
        ready_candidates,
        blocked_candidates: candidates.len() - ready_candidates,
        candidates,
        records,
        content_hash_blake3: String::new(),
    };
    ledger.content_hash_blake3 = ledger.compute_content_hash()?;
    ledger.validate()?;
    Ok(ledger)
}

fn build_candidate_ammo_effect_folds(
    closure: &BTreeSet<StableFormKey>,
    winners: &BTreeMap<StableFormKey, Winner<'_>>,
    interner: &StringInterner,
) -> Result<
    (
        Vec<AmmoEffectOwnerFoldReceipt>,
        BTreeSet<CreatureDependencyBlocker>,
    ),
    CreatureDependencyError,
> {
    let mut ammo_owners = BTreeMap::<StableFormKey, Vec<(StableFormKey, String)>>::new();
    for weapon in closure {
        let Some(winner) = winners.get(weapon) else {
            continue;
        };
        if winner.record.sig.as_str() != "WEAP" {
            continue;
        }
        for (ammo, locator) in references_from_signature(winner.record, "NAM0", interner)? {
            if closure.contains(&ammo)
                && winners
                    .get(&ammo)
                    .is_some_and(|winner| winner.record.sig.as_str() == "AMMO")
            {
                ammo_owners
                    .entry(ammo)
                    .or_default()
                    .push((weapon.clone(), locator));
            }
        }
    }
    for owners in ammo_owners.values_mut() {
        owners.sort();
        owners.dedup();
    }

    let mut folds = BTreeSet::new();
    let blockers = BTreeSet::new();
    for ammo in closure {
        let Some(winner) = winners.get(ammo) else {
            continue;
        };
        if winner.record.sig.as_str() != "AMMO" {
            continue;
        }
        for (effect, rcil_locator) in references_from_signature(winner.record, "RCIL", interner)? {
            if !closure.contains(&effect) {
                continue;
            }
            let Some(effect_record) = winners.get(&effect) else {
                continue;
            };
            let LegacyAmmoEffectSupport::Foldable(fold) =
                classify_legacy_ammo_effect(effect_record.record, interner)
            else {
                continue;
            };
            let Some(owners) = ammo_owners.get(ammo) else {
                continue;
            };
            for (weapon, ammo_locator) in owners {
                folds.insert(AmmoEffectOwnerFoldReceipt {
                    source_effect: effect.clone(),
                    source_ammo: ammo.clone(),
                    consuming_weapon: weapon.clone(),
                    rcil_locator: rcil_locator.clone(),
                    ammo_locator: ammo_locator.clone(),
                    channel: match fold.channel {
                        LegacyAmmoEffectChannel::Damage => AmmoEffectFoldChannel::Damage,
                        LegacyAmmoEffectChannel::Spread => AmmoEffectFoldChannel::Spread,
                    },
                    operation: match fold.operation {
                        LegacyAmmoEffectOperation::Add => AmmoEffectFoldOperation::Add,
                        LegacyAmmoEffectOperation::Multiply => AmmoEffectFoldOperation::Multiply,
                        LegacyAmmoEffectOperation::Subtract => AmmoEffectFoldOperation::Subtract,
                    },
                    value_bits: fold.value_bits,
                });
            }
        }
    }
    Ok((folds.into_iter().collect(), blockers))
}

fn references_from_signature(
    record: &Record,
    signature: &str,
    interner: &StringInterner,
) -> Result<Vec<(StableFormKey, String)>, CreatureDependencyError> {
    let mut references = Vec::new();
    let mut occurrence = 0usize;
    for field in &record.fields {
        if field.sig.as_str() != signature {
            continue;
        }
        let locator = format!("{signature}[{occurrence}]");
        occurrence += 1;
        let mut form_keys = Vec::new();
        form_keys_from_value(&field.value, &locator, interner, &mut form_keys);
        for (form_key, locator) in form_keys {
            references.push((stable_form_key(form_key, interner)?, locator));
        }
    }
    references.sort();
    references.dedup();
    Ok(references)
}

type SourceLoadOrders<'a> = BTreeMap<(LegacyCreatureGame, &'a str), &'a [String]>;

fn legacy_condition_family(game: LegacyCreatureGame) -> LegacyConditionFamily {
    match game {
        LegacyCreatureGame::Fnv => LegacyConditionFamily::Fnv,
        LegacyCreatureGame::Fo3 => LegacyConditionFamily::Fo3,
    }
}

fn canonical_source_load_orders(
    source_load_orders: &[LegacyPluginLoadOrder],
) -> Result<SourceLoadOrders<'_>, CreatureDependencyError> {
    let mut canonical = BTreeMap::new();
    for load_order in source_load_orders {
        if canonical
            .insert(
                (load_order.game, load_order.source_plugin.as_str()),
                load_order.source_master_names.as_slice(),
            )
            .is_some()
        {
            return Err(CreatureDependencyError::DuplicateSourceLoadOrder {
                game: load_order.game,
                source_plugin: load_order.source_plugin.clone(),
            });
        }
    }
    Ok(canonical)
}

struct Winner<'a> {
    record: &'a Record,
    provenance: CreatureProvenance,
}

fn select_winners<'a>(
    records: &[LegacyRecordSource<'a>],
    interner: &StringInterner,
) -> Result<BTreeMap<StableFormKey, Winner<'a>>, CreatureDependencyError> {
    let mut winners = BTreeMap::<StableFormKey, Winner<'a>>::new();
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
                    return Err(CreatureDependencyError::WinnerCollision {
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

fn build_record_receipt(
    source: &StableFormKey,
    winner: &Winner<'_>,
    winners: &BTreeMap<StableFormKey, Winner<'_>>,
    source_load_orders: &SourceLoadOrders<'_>,
    interner: &StringInterner,
) -> Result<RuntimeDependencyRecordReceipt, CreatureDependencyError> {
    let lowering = lowering_disposition(
        winner.record,
        &winner.provenance,
        source_load_orders,
        interner,
    );
    Ok(RuntimeDependencyRecordReceipt {
        source: source.clone(),
        signature: winner.record.sig.as_str().to_string(),
        provenance: winner.provenance.clone(),
        degradations: record_degradations(
            source,
            winner.record,
            &winner.provenance,
            source_load_orders,
            interner,
            &lowering,
        ),
        lowering,
        references: runtime_references(
            winner.record,
            &winner.provenance,
            winners,
            source_load_orders,
            interner,
        )?,
    })
}

fn record_degradations(
    source: &StableFormKey,
    record: &Record,
    provenance: &CreatureProvenance,
    source_load_orders: &SourceLoadOrders<'_>,
    interner: &StringInterner,
    lowering: &PairRecordLoweringDisposition,
) -> Vec<crate::source_rig::CreatureDegradationReceipt> {
    if matches!(lowering, PairRecordLoweringDisposition::Blocked { .. }) {
        return Vec::new();
    }
    use crate::source_rig::CreatureFallbackDisposition;

    let (warnings, policy_id, disposition) = match record.sig.as_str() {
        "CSTY" => (
            legacy_combat_style_fallback_warnings(record),
            "fnv_fo3_combat_style_standard_defaults_v1",
            CreatureFallbackDisposition::TargetDefault,
        ),
        "PACK" => {
            let Some(source_master_names) =
                source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
            else {
                return Vec::new();
            };
            let pack_source = match provenance.game {
                LegacyCreatureGame::Fnv => LegacyPackSourceFamily::Fnv,
                LegacyCreatureGame::Fo3 => LegacyPackSourceFamily::Fo3,
            };
            (
                legacy_package_fallback_warnings(
                    record,
                    pack_source,
                    &provenance.source_plugin,
                    source_master_names,
                    interner,
                ),
                "fnv_fo3_package_current_location_travel_v1",
                CreatureFallbackDisposition::SupportedSubset,
            )
        }
        "AMMO" => {
            let source_family = match provenance.game {
                LegacyCreatureGame::Fnv => LegacyAmmoSourceFamily::Fnv,
                LegacyCreatureGame::Fo3 => LegacyAmmoSourceFamily::Fo3,
            };
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Ready { mechanism }
                    if mechanism == "legacy_ammo_target_default_fallback"
            ) {
                return degradation_receipts(
                    source,
                    record,
                    ["legacy_ammo_unreadable_optional_fields_use_target_defaults"],
                    "fnv_fo3_ammo_target_defaults_v1",
                    CreatureFallbackDisposition::TargetDefault,
                );
            }
            let mut decoded = record.clone();
            if source_family == LegacyAmmoSourceFamily::Fnv {
                let Some(source_master_names) =
                    source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
                else {
                    return Vec::new();
                };
                if decode_legacy_ammo_dat2(
                    &mut decoded,
                    &provenance.source_plugin,
                    source_master_names,
                    interner,
                )
                .is_err()
                {
                    return Vec::new();
                }
            }
            (
                legacy_ammo_fallback_warnings(&decoded, source_family, interner),
                "fnv_fo3_ammo_supported_subset_v1",
                CreatureFallbackDisposition::SupportedSubset,
            )
        }
        "BPTD" => (
            legacy_body_part_fallback_warnings(record),
            "fnv_fo3_body_part_source_semantics_omitted_v1",
            CreatureFallbackDisposition::Omitted,
        ),
        "EXPL" => (
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Ready { mechanism }
                    if mechanism == "legacy_explosion_supported_subset_fallback"
            ) {
                vec!["legacy_explosion_unrecognized_optional_fields_omitted".to_string()]
            } else {
                legacy_explosion_fallback_warnings(record)
            },
            "fnv_fo3_explosion_source_semantics_omitted_v1",
            CreatureFallbackDisposition::Omitted,
        ),
        "FACT" => (
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Ready { mechanism }
                    if mechanism == "legacy_faction_supported_subset_fallback"
            ) {
                vec!["legacy_faction_unrecognized_rows_omitted".to_string()]
            } else {
                legacy_faction_fallback_warnings(record)
            },
            "fnv_fo3_faction_source_semantics_omitted_v1",
            CreatureFallbackDisposition::Omitted,
        ),
        "IPCT" => (
            legacy_impact_fallback_warnings(record, interner),
            "fnv_fo3_impact_source_semantics_omitted_v1",
            CreatureFallbackDisposition::Omitted,
        ),
        "TXST" => (
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Ready { mechanism }
                    if mechanism == "legacy_texture_set_supported_subset_fallback"
            ) {
                vec!["legacy_texture_set_duplicate_or_unrecognized_fields_omitted".to_string()]
            } else {
                legacy_texture_set_fallback_warnings(record, interner)
            },
            "fnv_fo3_texture_set_source_semantics_omitted_v1",
            CreatureFallbackDisposition::Omitted,
        ),
        "AMEF" => (
            vec!["legacy_ammo_effect_nonportable_semantics_omitted".to_string()],
            "fnv_fo3_ammo_effect_owner_fallback_v1",
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Consumed { mechanism }
                    if mechanism == "owner_folded_legacy_ammo_effect"
            ) {
                CreatureFallbackDisposition::SupportedSubset
            } else {
                CreatureFallbackDisposition::Omitted
            },
        ),
        "WEAP" => (
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Consumed { mechanism }
                    if mechanism == "generated_creature_weapon_replacement"
            ) {
                vec!["legacy_weapon_replaced_by_generated_creature_weapon".to_string()]
            } else {
                Vec::new()
            },
            "fnv_fo3_generated_creature_weapon_v1",
            CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
        ),
        "EYES" | "HAIR" | "HDPT" => (
            vec!["legacy_appearance_replaced_by_generated_actor_defaults".to_string()],
            "fnv_fo3_generated_actor_appearance_v1",
            CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
        ),
        "NPC_" => (
            vec!["legacy_npc_rebuilt_by_source_rig".to_string()],
            "fnv_fo3_source_rig_actor_projection_v1",
            CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
        ),
        "RGDL" => (
            vec!["legacy_ragdoll_controls_replaced_by_generated_behavior".to_string()],
            "fnv_fo3_generated_behavior_ragdoll_v1",
            CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
        ),
        "IMOD" => (
            vec!["legacy_item_mod_omitted_with_generated_weapon".to_string()],
            "fnv_fo3_generated_creature_weapon_v1",
            CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
        ),
        "NOTE" => (
            vec!["legacy_note_omitted_from_creature_runtime".to_string()],
            "fnv_fo3_optional_creature_dependency_omission_v1",
            CreatureFallbackDisposition::Omitted,
        ),
        "MISC" => (
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Ready { mechanism }
                    if mechanism == "legacy_misc_supported_subset_fallback"
            ) {
                vec!["legacy_misc_duplicate_or_script_fields_omitted".to_string()]
            } else {
                Vec::new()
            },
            "fnv_fo3_misc_supported_subset_v1",
            CreatureFallbackDisposition::SupportedSubset,
        ),
        "DEBR" => (
            if matches!(
                lowering,
                PairRecordLoweringDisposition::Ready { mechanism }
                    if mechanism == "legacy_debris_supported_subset_fallback"
            ) {
                vec!["legacy_debris_extra_model_metadata_omitted".to_string()]
            } else {
                Vec::new()
            },
            "fnv_fo3_debris_supported_subset_v1",
            CreatureFallbackDisposition::SupportedSubset,
        ),
        "LIGH" | "STAT" => (
            vec!["legacy_common_record_uses_schema_supported_subset".to_string()],
            "fnv_fo3_common_record_supported_subset_v1",
            CreatureFallbackDisposition::SupportedSubset,
        ),
        "IDLE" => (
            legacy_idle_fallback_warnings(record),
            "fnv_fo3_idle_generated_behavior_replacement_v1",
            CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
        ),
        "SCPT" => match provenance.game {
            LegacyCreatureGame::Fnv => (
                vec!["legacy_scpt_consumed_by_scripting_bridge".to_string()],
                "fnv_legacy_scripting_bridge_v1",
                CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
            ),
            LegacyCreatureGame::Fo3 => (
                vec!["legacy_fo3_scpt_omitted_nonportable_script".to_string()],
                "fo3_legacy_script_omission_v1",
                CreatureFallbackDisposition::Omitted,
            ),
        },
        _ => return Vec::new(),
    };
    let mut receipts = warnings
        .into_iter()
        .map(|code| crate::source_rig::CreatureDegradationReceipt {
            detail: format!("{code} for {source}"),
            code,
            policy_id: policy_id.to_string(),
            disposition,
            affected_source_keys: vec![source.to_string()],
            source_signature: Some(record.sig.as_str().to_string()),
        })
        .collect::<Vec<_>>();
    for receipt in &mut receipts {
        receipt.canonicalize();
        debug_assert!(receipt.validate().is_ok());
    }
    receipts.sort();
    receipts.dedup();
    receipts
}

fn degradation_receipts<const N: usize>(
    source: &StableFormKey,
    record: &Record,
    warnings: [&str; N],
    policy_id: &str,
    disposition: crate::source_rig::CreatureFallbackDisposition,
) -> Vec<crate::source_rig::CreatureDegradationReceipt> {
    let mut receipts = warnings
        .into_iter()
        .map(|code| crate::source_rig::CreatureDegradationReceipt {
            detail: format!("{code} for {source}"),
            code: code.to_string(),
            policy_id: policy_id.to_string(),
            disposition,
            affected_source_keys: vec![source.to_string()],
            source_signature: Some(record.sig.as_str().to_string()),
        })
        .collect::<Vec<_>>();
    for receipt in &mut receipts {
        receipt.canonicalize();
        debug_assert!(receipt.validate().is_ok());
    }
    receipts.sort();
    receipts.dedup();
    receipts
}

fn runtime_references(
    record: &Record,
    provenance: &CreatureProvenance,
    winners: &BTreeMap<StableFormKey, Winner<'_>>,
    source_load_orders: &SourceLoadOrders<'_>,
    interner: &StringInterner,
) -> Result<Vec<DependencyReferenceReceipt>, CreatureDependencyError> {
    let mut occurrences = Vec::new();
    let mut signature_indices = BTreeMap::<String, usize>::new();
    for field in &record.fields {
        let signature = field.sig.as_str().to_string();
        let occurrence = signature_indices.entry(signature.clone()).or_default();
        let locator = format!("{signature}[{occurrence}]");
        *occurrence += 1;
        if !is_runtime_owned_reference_field(record.sig.as_str(), &signature) {
            continue;
        }
        form_keys_from_value(&field.value, &locator, interner, &mut occurrences);
    }
    if record.sig.as_str() == "CREA"
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
    {
        let mut conto_index = 0usize;
        for field in &record.fields {
            if field.sig.as_str() != "CNTO" {
                continue;
            }
            if let FieldValue::Bytes(bytes) = &field.value
                && bytes.len() == 8
            {
                let raw = u32::from_le_bytes(
                    bytes[..4]
                        .try_into()
                        .expect("CREA.CNTO item slice has exact size"),
                );
                if let Some(form_key) = source_form_key_for_raw_form_id(
                    raw,
                    &provenance.source_plugin,
                    source_master_names,
                    interner,
                ) {
                    occurrences.push((form_key, format!("CNTO[{conto_index}].item@0")));
                }
            }
            conto_index += 1;
        }
    }
    if record.sig.as_str() == "AMMO"
        && provenance.game == LegacyCreatureGame::Fnv
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        && let Ok(embedded) = legacy_ammo_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        )
    {
        occurrences.extend(
            embedded
                .into_iter()
                .map(|reference| (reference.form_key, reference.locator.to_string())),
        );
    }
    if record.sig.as_str() == "LVLI"
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        && let Ok(embedded) = legacy_leveled_item_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        )
    {
        occurrences.extend(
            embedded
                .into_iter()
                .map(|reference| (reference.form_key, reference.locator)),
        );
    }
    if record.sig.as_str() == "IPDS"
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        && let Ok(embedded) = legacy_impact_dataset_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        )
    {
        occurrences.extend(
            embedded
                .into_iter()
                .map(|reference| (reference.form_key, reference.locator)),
        );
    }
    if record.sig.as_str() == "BPTD"
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        && let Ok(embedded) = legacy_body_part_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        )
    {
        occurrences.extend(
            embedded
                .into_iter()
                .map(|reference| (reference.form_key, reference.locator)),
        );
    }
    if record.sig.as_str() == "EXPL"
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        && let Ok(embedded) = legacy_explosion_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        )
    {
        occurrences.extend(
            embedded
                .into_iter()
                .map(|reference| (reference.form_key, reference.locator)),
        );
    }
    if record.sig.as_str() == "FACT"
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        && let Ok(embedded) = legacy_faction_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        )
    {
        occurrences.extend(
            embedded
                .into_iter()
                .map(|reference| (reference.form_key, reference.locator)),
        );
    }
    if record.sig.as_str() == "IDLE"
        && let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        && let Ok(embedded) = legacy_idle_embedded_references(
            record,
            legacy_condition_family(provenance.game),
            &provenance.source_plugin,
            source_master_names,
            interner,
        )
    {
        occurrences.extend(
            embedded
                .into_iter()
                .map(|reference| (reference.form_key, reference.locator)),
        );
    }
    if record.sig.as_str() == "PACK" {
        if let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        {
            let source = match provenance.game {
                LegacyCreatureGame::Fnv => LegacyPackSourceFamily::Fnv,
                LegacyCreatureGame::Fo3 => LegacyPackSourceFamily::Fo3,
            };
            if matches!(
                classify_generic_legacy_package(
                    record,
                    source,
                    &provenance.source_plugin,
                    source_master_names,
                    interner,
                ),
                GenericLegacyPackageSupport::Ready { .. }
            ) && let Ok(embedded) = legacy_package_embedded_references(
                record,
                legacy_condition_family(provenance.game),
                &provenance.source_plugin,
                source_master_names,
                interner,
            ) {
                occurrences.extend(
                    embedded
                        .into_iter()
                        .map(|reference| (reference.form_key, reference.locator)),
                );
            }
        }
    }
    let mut targets = BTreeMap::<StableFormKey, BTreeSet<String>>::new();
    for (target, locator) in occurrences {
        if target == record.form_key {
            continue;
        }
        targets
            .entry(stable_form_key(target, interner)?)
            .or_default()
            .insert(locator);
    }
    Ok(targets
        .into_iter()
        .map(|(target, source_locators)| {
            let resolution = if is_legacy_player_ref(&target)
                && source_locators
                    .iter()
                    .all(|locator| is_legacy_player_intrinsic_locator(locator))
            {
                DependencyReferenceResolution::TargetIntrinsic {
                    signature: "REFR".to_string(),
                    mapped_target: StableFormKey {
                        plugin: "Fallout4.esm".to_string(),
                        local: 0x14,
                    },
                }
            } else {
                winners.get(&target).map_or(
                    DependencyReferenceResolution::MissingSourceRecord,
                    |winner| {
                        let signature = winner.record.sig.as_str().to_string();
                        if is_runtime_dependency_signature(&signature) {
                            DependencyReferenceResolution::Followed { signature }
                        } else {
                            DependencyReferenceResolution::OutOfRuntimeScope { signature }
                        }
                    },
                )
            };
            DependencyReferenceReceipt {
                target,
                source_locators: source_locators.into_iter().collect(),
                resolution,
            }
        })
        .collect())
}

fn is_runtime_owned_reference_field(record_signature: &str, field_signature: &str) -> bool {
    if record_signature != "WEAP" {
        return true;
    }
    !matches!(field_signature, "REPL" | "BIPL" | "WNAM")
}

fn is_legacy_player_ref(target: &StableFormKey) -> bool {
    target.local == 0x14 && matches!(target.plugin.as_str(), "FalloutNV.esm" | "Fallout3.esm")
}

fn is_scro_locator(locator: &str) -> bool {
    is_indexed_locator(locator, "SCRO")
}

fn is_legacy_player_intrinsic_locator(locator: &str) -> bool {
    is_scro_locator(locator)
        || [
            ("condition_comparison_global", 4usize),
            ("condition_parameter_1", 12usize),
            ("condition_parameter_2", 16usize),
            ("condition_run_on_reference", 24usize),
        ]
        .iter()
        .any(|(field, offset)| {
            locator
                .strip_suffix(&format!(".{field}@{offset}"))
                .is_some_and(|prefix| {
                    is_indexed_locator(prefix, "CTDA") || is_indexed_locator(prefix, "CTDT")
                })
        })
}

fn is_indexed_locator(locator: &str, signature: &str) -> bool {
    let prefix = format!("{signature}[");
    locator
        .strip_prefix(&prefix)
        .and_then(|index| index.strip_suffix(']'))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}

fn lowering_disposition(
    record: &Record,
    provenance: &CreatureProvenance,
    source_load_orders: &SourceLoadOrders<'_>,
    interner: &StringInterner,
) -> PairRecordLoweringDisposition {
    let signature = record.sig.as_str();
    let ready = match signature {
        "CREA" => Some("source_rig_creature_projection"),
        "ARMO" | "ARMA" => Some("legacy_armor_pair_relayout"),
        "PROJ" => Some("legacy_projectile_data_relayout"),
        "MGEF" | "ALCH" | "ENCH" | "SPEL" => Some("legacy_effect_serial_normalization"),
        "EFSH" => Some("legacy_effect_shader_layout_normalization"),
        "SOUN" => Some("legacy_sound_descriptor_rewrite"),
        "LIGH" | "STAT" => Some("legacy_common_record_supported_subset"),
        _ => None,
    };
    if let Some(mechanism) = ready {
        return PairRecordLoweringDisposition::Ready {
            mechanism: mechanism.to_string(),
        };
    }
    if signature == "AMEF" {
        return match classify_legacy_ammo_effect(record, interner) {
            LegacyAmmoEffectSupport::Foldable(_) => PairRecordLoweringDisposition::Consumed {
                mechanism: "owner_folded_legacy_ammo_effect".to_string(),
            },
            LegacyAmmoEffectSupport::Blocked(_) => PairRecordLoweringDisposition::Consumed {
                mechanism: "legacy_ammo_effect_omission_fallback".to_string(),
            },
        };
    }
    if signature == "PACK" {
        let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        else {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec!["legacy_pack_source_load_order_unavailable".to_string()],
            };
        };
        let source = match provenance.game {
            LegacyCreatureGame::Fnv => LegacyPackSourceFamily::Fnv,
            LegacyCreatureGame::Fo3 => LegacyPackSourceFamily::Fo3,
        };
        return match classify_generic_legacy_package(
            record,
            source,
            &provenance.source_plugin,
            source_master_names,
            interner,
        ) {
            GenericLegacyPackageSupport::Ready { .. } => {
                if let Err(reason_code) = legacy_package_embedded_references(
                    record,
                    legacy_condition_family(provenance.game),
                    &provenance.source_plugin,
                    source_master_names,
                    interner,
                ) {
                    PairRecordLoweringDisposition::Blocked {
                        reason_codes: vec![reason_code],
                    }
                } else {
                    PairRecordLoweringDisposition::Ready {
                        mechanism: "generic_legacy_travel_patrol_procedure_tree".to_string(),
                    }
                }
            }
            GenericLegacyPackageSupport::Fallback { warning_codes, .. } => {
                let mut reason_codes = warning_codes;
                reason_codes.push("legacy_pack_generic_fallback_forbidden".to_string());
                reason_codes.sort();
                reason_codes.dedup();
                PairRecordLoweringDisposition::Blocked { reason_codes }
            }
        };
    }
    if signature == "LVLC" {
        let support = classify_legacy_lvlc(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_lvlc_to_fo4_lvln".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked {
                reason_codes: support.reason_codes,
            }
        };
    }
    if signature == "AMMO" {
        let source = match provenance.game {
            LegacyCreatureGame::Fnv => LegacyAmmoSourceFamily::Fnv,
            LegacyCreatureGame::Fo3 => LegacyAmmoSourceFamily::Fo3,
        };
        let mut decoded = record.clone();
        if source == LegacyAmmoSourceFamily::Fnv
            && decoded.fields.iter().any(|field| {
                field.sig.as_str() == "DAT2" && matches!(field.value, FieldValue::Bytes(_))
            })
        {
            let Some(source_master_names) =
                source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
            else {
                return PairRecordLoweringDisposition::Blocked {
                    reason_codes: vec!["legacy_ammo_source_load_order_unavailable".to_string()],
                };
            };
            if let Err(reason_code) = decode_legacy_ammo_dat2(
                &mut decoded,
                &provenance.source_plugin,
                source_master_names,
                interner,
            ) {
                let _ = reason_code;
                return PairRecordLoweringDisposition::Ready {
                    mechanism: "legacy_ammo_target_default_fallback".to_string(),
                };
            }
        }
        let support = classify_legacy_ammo(&decoded, source, interner);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_ammo_to_fo4".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_ammo_target_default_fallback".to_string(),
            }
        };
    }
    if signature == "WEAP" {
        let support = classify_legacy_creature_weapon(record, interner);
        return PairRecordLoweringDisposition::Ready {
            mechanism: if support.is_ready() {
                "verified_legacy_creature_weapon_lowerer".to_string()
            } else {
                "legacy_weapon_fieldwise_fo4_rebuild".to_string()
            },
        };
    }
    if signature == "FLST" {
        let reason_codes = classify_legacy_form_id_list(record);
        return if reason_codes.is_empty() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "schema_identical_form_id_list_with_mapper".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked { reason_codes }
        };
    }
    if signature == "GLOB" {
        let reason_codes = classify_legacy_global(record);
        return if reason_codes.is_empty() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "schema_identical_global_scalar".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked { reason_codes }
        };
    }
    if signature == "MISC" {
        let support = classify_legacy_misc(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "schema_compatible_legacy_misc_with_native_asset_fixups".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_misc_supported_subset_fallback".to_string(),
            }
        };
    }
    if signature == "KEYM" {
        let support = classify_legacy_key(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "schema_compatible_legacy_key_with_native_asset_fixups".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_key_supported_subset_fallback".to_string(),
            }
        };
    }
    if signature == "BOOK" {
        return PairRecordLoweringDisposition::Ready {
            mechanism: "legacy_book_target_projection_with_fo4_defaults".to_string(),
        };
    }
    if signature == "CMNY" {
        return PairRecordLoweringDisposition::Ready {
            mechanism: "legacy_currency_to_misc_projection".to_string(),
        };
    }
    if signature == "EYES" {
        return PairRecordLoweringDisposition::Consumed {
            mechanism: "generated_actor_appearance_replacement".to_string(),
        };
    }
    if signature == "HAIR" {
        return PairRecordLoweringDisposition::Consumed {
            mechanism: "generated_actor_appearance_replacement".to_string(),
        };
    }
    if signature == "HDPT" {
        return PairRecordLoweringDisposition::Consumed {
            mechanism: "generated_actor_appearance_replacement".to_string(),
        };
    }
    if signature == "NPC_" {
        return PairRecordLoweringDisposition::Consumed {
            mechanism: "source_rig_actor_projection".to_string(),
        };
    }
    if signature == "LVLI" {
        if record.fields.iter().any(|field| {
            field.sig.as_str() == "LVLO" && matches!(field.value, FieldValue::Bytes(_))
        }) {
            let Some(source_master_names) =
                source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
            else {
                return PairRecordLoweringDisposition::Blocked {
                    reason_codes: vec![
                        "legacy_leveled_item_source_load_order_unavailable".to_string(),
                    ],
                };
            };
            if let Err(reason_code) = legacy_leveled_item_embedded_references(
                record,
                &provenance.source_plugin,
                source_master_names,
                interner,
            ) {
                return PairRecordLoweringDisposition::Blocked {
                    reason_codes: vec![reason_code],
                };
            }
        }
        let support = classify_legacy_leveled_item(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_leveled_item_relayout".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked {
                reason_codes: support.reason_codes,
            }
        };
    }
    if signature == "IPDS" {
        let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        else {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec![
                    "legacy_impact_dataset_source_load_order_unavailable".to_string(),
                ],
            };
        };
        if record.fields.iter().any(|field| {
            field.sig.as_str() == "DATA" && matches!(field.value, FieldValue::Bytes(_))
        }) {
            if let Err(reason_code) = legacy_impact_dataset_embedded_references(
                record,
                &provenance.source_plugin,
                source_master_names,
                interner,
            ) {
                return PairRecordLoweringDisposition::Blocked {
                    reason_codes: vec![reason_code],
                };
            }
        }
        let support = classify_legacy_impact_dataset(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        );
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_impact_dataset_material_projection".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked {
                reason_codes: support.reason_codes,
            }
        };
    }
    if signature == "IPCT" {
        let support = classify_legacy_impact(record, interner);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_impact_layout_projection".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked {
                reason_codes: support.reason_codes,
            }
        };
    }
    if signature == "TXST" {
        let support = classify_legacy_texture_set(record, interner);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_texture_set_projection".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_texture_set_supported_subset_fallback".to_string(),
            }
        };
    }
    if signature == "BPTD" {
        let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        else {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec!["legacy_body_part_source_load_order_unavailable".to_string()],
            };
        };
        if let Err(reason_code) = legacy_body_part_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        ) {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec![reason_code],
            };
        }
        let support = classify_legacy_body_part_data(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_body_part_node_projection".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked {
                reason_codes: support.reason_codes,
            }
        };
    }
    if signature == "RGDL" {
        let support = classify_legacy_ragdoll(record, interner);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Consumed {
                mechanism: "source_rig_powered_ragdoll_evidence".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Consumed {
                mechanism: "generated_behavior_ragdoll_replacement".to_string(),
            }
        };
    }
    if signature == "CSTY" {
        let support = classify_legacy_combat_style(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_combat_style_projection".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked {
                reason_codes: support.reason_codes,
            }
        };
    }
    if signature == "DEBR" {
        let support = classify_legacy_debris(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_debris_projection_with_runtime_model_closure".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_debris_supported_subset_fallback".to_string(),
            }
        };
    }
    if signature == "EXPL" {
        let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        else {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec!["legacy_explosion_source_load_order_unavailable".to_string()],
            };
        };
        if let Err(reason_code) = legacy_explosion_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        ) {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec![reason_code],
            };
        }
        let support = classify_legacy_explosion(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_explosion_data_projection".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_explosion_supported_subset_fallback".to_string(),
            }
        };
    }
    if signature == "FACT" {
        let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        else {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec!["legacy_faction_source_load_order_unavailable".to_string()],
            };
        };
        if let Err(reason_code) = legacy_faction_embedded_references(
            record,
            &provenance.source_plugin,
            source_master_names,
            interner,
        ) {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec![reason_code],
            };
        }
        let support = classify_legacy_faction(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Ready {
                mechanism: "verified_legacy_faction_flag_and_reaction_projection".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_faction_supported_subset_fallback".to_string(),
            }
        };
    }
    if signature == "IDLE" {
        let Some(source_master_names) =
            source_load_orders.get(&(provenance.game, provenance.source_plugin.as_str()))
        else {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec!["legacy_idle_source_load_order_unavailable".to_string()],
            };
        };
        if let Err(reason_code) = legacy_idle_embedded_references(
            record,
            legacy_condition_family(provenance.game),
            &provenance.source_plugin,
            source_master_names,
            interner,
        ) {
            return PairRecordLoweringDisposition::Blocked {
                reason_codes: vec![reason_code],
            };
        }
        let support = classify_legacy_idle(record);
        return if support.is_ready() {
            PairRecordLoweringDisposition::Consumed {
                mechanism: "legacy_idle_replaced_by_generated_behavior".to_string(),
            }
        } else {
            PairRecordLoweringDisposition::Blocked {
                reason_codes: support.reason_codes,
            }
        };
    }
    if signature == "SCPT" {
        return PairRecordLoweringDisposition::Consumed {
            mechanism: match provenance.game {
                LegacyCreatureGame::Fnv => "fnv_legacy_scripting_bridge",
                LegacyCreatureGame::Fo3 => "fo3_legacy_script_omission",
            }
            .to_string(),
        };
    }
    if matches!(signature, "IMOD" | "NOTE") {
        return PairRecordLoweringDisposition::Consumed {
            mechanism: "optional_creature_dependency_omission".to_string(),
        };
    }
    PairRecordLoweringDisposition::Blocked {
        reason_codes: vec![
            match signature {
                "LVLC" => "legacy_lvlc_to_fo4_lvln_lowering_unavailable",
                "WEAP" => "legacy_creature_weapon_lowering_unverified",
                "AMMO" => "legacy_ammo_lowering_unverified",
                "AMEF" => "legacy_ammo_effect_owner_fold_unverified",
                "CSTY" => "legacy_combat_style_lowering_unverified",
                "BPTD" => "legacy_body_part_data_lowering_unverified",
                "IDLE" | "IDLM" | "ANIO" => "legacy_idle_record_lowering_unverified",
                _ => "legacy_runtime_dependency_lowering_unverified",
            }
            .to_string(),
        ],
    }
}

fn classify_legacy_form_id_list(record: &Record) -> Vec<String> {
    let mut reason_codes = BTreeSet::new();
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => {}
            "LNAM" if matches!(field.value, FieldValue::FormKey(_)) => {}
            "EDID" => {
                reason_codes.insert("legacy_form_id_list_editor_id_shape_unverified".to_string());
            }
            "LNAM" => {
                reason_codes.insert("legacy_form_id_list_entry_shape_unverified".to_string());
            }
            _ => {
                reason_codes.insert("legacy_form_id_list_subrecord_unverified".to_string());
            }
        }
    }
    reason_codes.into_iter().collect()
}

fn classify_legacy_global(record: &Record) -> Vec<String> {
    let mut reason_codes = BTreeSet::new();
    let mut edid_count = 0usize;
    let mut type_count = 0usize;
    let mut value_count = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => edid_count += 1,
            "FNAM"
                if legacy_global_u8(&field.value).is_some_and(|value| b"fls".contains(&value)) =>
            {
                type_count += 1;
            }
            "FLTV" if legacy_global_f32(&field.value).is_some() => value_count += 1,
            "EDID" | "FNAM" | "FLTV" => {
                reason_codes.insert("legacy_global_field_shape_unverified".to_string());
            }
            _ => {
                reason_codes.insert("legacy_global_subrecord_unverified".to_string());
            }
        }
    }
    if edid_count != 1 || type_count != 1 || value_count != 1 {
        reason_codes.insert("legacy_global_field_multiplicity_unverified".to_string());
    }
    reason_codes.into_iter().collect()
}

fn legacy_global_u8(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if bytes.len() == 1 => Some(bytes[0]),
        _ => None,
    }
}

fn legacy_global_f32(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(f32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => None,
    }
}

fn is_runtime_dependency_signature(signature: &str) -> bool {
    REQUIRED_RUNTIME_DEPENDENCY_SIGNATURES.contains(&signature)
}

fn stable_form_key(
    form_key: FormKey,
    interner: &StringInterner,
) -> Result<StableFormKey, CreatureDependencyError> {
    let plugin = interner
        .resolve(form_key.plugin)
        .ok_or(CreatureDependencyError::UnresolvedPlugin)?;
    Ok(StableFormKey {
        local: form_key.local,
        plugin: plugin.to_string(),
    })
}

fn form_keys_from_value(
    value: &FieldValue,
    locator: &str,
    interner: &StringInterner,
    results: &mut Vec<(FormKey, String)>,
) {
    match value {
        FieldValue::FormKey(form_key) => results.push((*form_key, locator.to_string())),
        FieldValue::List(values) => {
            for (index, value) in values.iter().enumerate() {
                form_keys_from_value(value, &format!("{locator}[{index}]"), interner, results);
            }
        }
        FieldValue::Struct(fields) => {
            for (name, value) in fields {
                let name = interner.resolve(*name).unwrap_or("<unresolved>");
                form_keys_from_value(value, &format!("{locator}.{name}"), interner, results);
            }
        }
        FieldValue::None
        | FieldValue::Bool(_)
        | FieldValue::Int(_)
        | FieldValue::Uint(_)
        | FieldValue::Float(_)
        | FieldValue::String(_)
        | FieldValue::Bytes(_) => {}
    }
}

fn strictly_sorted_unique<'a, T: Ord + 'a>(values: impl IntoIterator<Item = &'a T>) -> bool {
    let mut previous = None;
    for value in values {
        if previous.is_some_and(|previous| previous >= value) {
            return false;
        }
        previous = Some(value);
    }
    true
}

fn canonical_blake3(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, Record};

    use super::*;

    fn record(
        interner: &StringInterner,
        signature: &str,
        local: u32,
        plugin: &str,
        references: &[(&str, u32)],
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey {
                local,
                plugin: interner.intern(plugin),
            },
        );
        record
            .fields
            .extend(references.iter().map(|(plugin, local)| FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::FormKey(FormKey {
                    local: *local,
                    plugin: interner.intern(plugin),
                }),
            }));
        record
    }

    fn source<'a>(
        record: &'a Record,
        game: LegacyCreatureGame,
        precedence: u32,
    ) -> LegacyRecordSource<'a> {
        LegacyRecordSource {
            record,
            provenance: CreatureProvenance {
                game,
                source_plugin: match game {
                    LegacyCreatureGame::Fnv => "FalloutNV.esm",
                    LegacyCreatureGame::Fo3 => "Fallout3.esm",
                }
                .to_string(),
                precedence,
            },
        }
    }

    #[test]
    fn recursive_runtime_dependencies_are_canonical_and_flag_unlowered_records() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200), ("FalloutNV.esm", 0x300)],
        );
        let spell = record(
            &interner,
            "SPEL",
            0x200,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x201)],
        );
        let magic_effect = record(&interner, "MGEF", 0x201, "FalloutNV.esm", &[]);
        let weapon = record(&interner, "WEAP", 0x300, "FalloutNV.esm", &[]);
        let records = [
            source(&weapon, LegacyCreatureGame::Fnv, 1),
            source(&magic_effect, LegacyCreatureGame::Fnv, 1),
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&spell, LegacyCreatureGame::Fnv, 1),
        ];
        let ledger = build_creature_dependency_ledger(
            &records,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert_eq!(ledger.winning_creatures, 1);
        assert_eq!(ledger.ready_candidates, 1);
        assert_eq!(ledger.records.len(), 4);
        let weapon_receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "WEAP")
            .unwrap();
        assert_eq!(
            weapon_receipt.lowering,
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_weapon_fieldwise_fo4_rebuild".to_string(),
            }
        );
        assert!(weapon_receipt.degradations.is_empty());
        let creature_receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "CREA")
            .unwrap();
        assert_eq!(
            creature_receipt
                .references
                .iter()
                .map(|reference| reference.source_locators.clone())
                .collect::<Vec<_>>(),
            vec![vec!["DATA[0]".to_string()], vec!["DATA[1]".to_string()]]
        );
        assert!(ledger.canonical_json().is_ok());
    }

    #[test]
    fn ancillary_npc_pnam_head_parts_are_runtime_dependencies() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut npc = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey {
                local: 0x200,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        npc.fields.push(FieldEntry {
            sig: SubrecordSig(*b"PNAM"),
            value: FieldValue::FormKey(FormKey {
                local: 0x300,
                plugin: interner.intern("FalloutNV.esm"),
            }),
        });
        let head_part = record(&interner, "HDPT", 0x300, "FalloutNV.esm", &[]);
        let ledger = build_creature_dependency_ledger(
            &[
                source(&creature, LegacyCreatureGame::Fnv, 1),
                source(&npc, LegacyCreatureGame::Fnv, 1),
                source(&head_part, LegacyCreatureGame::Fnv, 1),
            ],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();

        let npc = ledger
            .records
            .iter()
            .find(|record| record.signature == "NPC_")
            .unwrap();
        assert!(npc.references.iter().any(|reference| {
            reference.target.local == 0x300
                && reference.source_locators.len() == 1
                && reference.source_locators[0] == "PNAM[0]"
                && matches!(
                    reference.resolution,
                    DependencyReferenceResolution::Followed { ref signature }
                        if signature == "HDPT"
                )
        }));
        assert!(ledger.records.iter().any(|record| {
            record.signature == "HDPT"
                && matches!(
                    &record.lowering,
                    PairRecordLoweringDisposition::Consumed { mechanism }
                        if mechanism == "generated_actor_appearance_replacement"
                )
                && record.degradations.iter().any(|degradation| {
                    degradation.code == "legacy_appearance_replaced_by_generated_actor_defaults"
                })
        }));
    }

    #[test]
    fn generic_travel_package_is_ready_without_source_identity_allowlisting() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut package = Record::new(
            SigCode::from_str("PACK").unwrap(),
            FormKey {
                local: 0x200,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let editor_id = interner.intern("ArbitraryCreatureTravel");
        package.eid = Some(editor_id);
        let mut pkdt = [0_u8; 12];
        pkdt[..4].copy_from_slice(&0x204_u32.to_le_bytes());
        pkdt[4] = 6;
        let mut schedule = [0_u8; 8];
        schedule[..4].copy_from_slice(&[u8::MAX, u8::MAX, 0, u8::MAX]);
        let mut location = [0_u8; 12];
        location[..4].copy_from_slice(&6_u32.to_le_bytes());
        location[8..12].copy_from_slice(&128_i32.to_le_bytes());
        package.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(editor_id),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PKDT"),
                value: FieldValue::Bytes(pkdt.to_vec().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PSDT"),
                value: FieldValue::Bytes(schedule.to_vec().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PLDT"),
                value: FieldValue::Bytes(location.to_vec().into()),
            },
        ]);
        for marker in [b"POBA", b"POEA", b"POCA"] {
            package.fields.push(FieldEntry {
                sig: SubrecordSig(*marker),
                value: FieldValue::Bytes(Vec::new().into()),
            });
            package.fields.push(FieldEntry {
                sig: SubrecordSig(*b"SCHR"),
                value: FieldValue::Bytes(vec![0; 20].into()),
            });
        }

        let ledger = build_creature_dependency_ledger_with_load_orders(
            &[
                source(&creature, LegacyCreatureGame::Fnv, 1),
                source(&package, LegacyCreatureGame::Fnv, 1),
            ],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &interner,
        )
        .unwrap();

        assert_eq!(ledger.ready_candidates, 1, "{:#?}", ledger.candidates);
        assert_eq!(ledger.blocked_candidates, 0);
        assert!(matches!(
            &ledger
                .records
                .iter()
                .find(|record| record.signature == "PACK")
                .unwrap()
                .lowering,
            PairRecordLoweringDisposition::Ready { mechanism }
                if mechanism == "generic_legacy_travel_patrol_procedure_tree"
        ));
    }

    #[test]
    fn raw_creature_inventory_item_is_a_locator_exact_runtime_edge() {
        let interner = StringInterner::new();
        let mut creature = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let mut conto = [0_u8; 8];
        conto[..4].copy_from_slice(&0x300_u32.to_le_bytes());
        conto[4..].copy_from_slice(&2_i32.to_le_bytes());
        creature.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CNTO"),
            value: FieldValue::Bytes(conto.to_vec().into()),
        });
        let item = Record::new(
            SigCode::from_str("MISC").unwrap(),
            FormKey {
                local: 0x300,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );

        let ledger = build_creature_dependency_ledger_with_load_orders(
            &[
                source(&creature, LegacyCreatureGame::Fnv, 1),
                source(&item, LegacyCreatureGame::Fnv, 1),
            ],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &interner,
        )
        .unwrap();
        let receipt = ledger
            .records
            .iter()
            .find(|record| record.signature == "CREA")
            .unwrap();

        assert!(receipt.references.iter().any(|reference| {
            reference.target.local == 0x300
                && reference.source_locators == ["CNTO[0].item@0"]
                && matches!(
                    reference.resolution,
                    DependencyReferenceResolution::Followed { ref signature }
                        if signature == "MISC"
                )
        }));
    }

    #[test]
    fn merged_precedence_and_game_provenance_are_stable_under_reversed_input() {
        let interner = StringInterner::new();
        let fnv = record(&interner, "CREA", 0x100, "Shared.esm", &[]);
        let fo3 = record(&interner, "CREA", 0x100, "Shared.esm", &[]);
        let first = [
            source(&fnv, LegacyCreatureGame::Fnv, 1),
            source(&fo3, LegacyCreatureGame::Fo3, 2),
        ];
        let second = [
            source(&fo3, LegacyCreatureGame::Fo3, 2),
            source(&fnv, LegacyCreatureGame::Fnv, 1),
        ];
        let build = |records: &[LegacyRecordSource<'_>]| {
            build_creature_dependency_ledger(
                records,
                CreatureDependencyOptions {
                    expected_creature_winners: Some(1),
                },
                &interner,
            )
            .unwrap()
        };
        let first = build(&first);
        let second = build(&second);
        assert_eq!(first, second);
        assert_eq!(first.candidates[0].provenance.game, LegacyCreatureGame::Fo3);
    }

    #[test]
    fn full_census_has_one_terminal_receipt_per_winning_creature() {
        let interner = StringInterner::new();
        let records = (0..EXPECTED_FULL_MERGED_CREA_WINNERS)
            .map(|index| {
                record(
                    &interner,
                    "CREA",
                    0x1000 + index as u32,
                    "FalloutNV.esm",
                    &[],
                )
            })
            .collect::<Vec<_>>();
        let sources = records
            .iter()
            .map(|record| source(record, LegacyCreatureGame::Fnv, 1))
            .collect::<Vec<_>>();
        let ledger = build_full_merged_creature_dependency_ledger(&sources, &interner).unwrap();
        assert_eq!(ledger.candidates.len(), EXPECTED_FULL_MERGED_CREA_WINNERS);
        assert_eq!(ledger.ready_candidates, EXPECTED_FULL_MERGED_CREA_WINNERS);
        assert_eq!(ledger.blocked_candidates, 0);
    }

    #[test]
    fn public_runtime_signatures_are_a_canonical_subset_of_resolution_signatures() {
        let runtime = required_fnv_fo3_creature_runtime_dependency_signatures();
        let resolution = required_fnv_fo3_creature_dependency_signatures();
        assert_eq!(runtime.len(), 52);
        assert!(runtime.contains(&"GMST"));
        assert!(runtime.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(
            runtime
                .iter()
                .all(|signature| resolution.contains(signature))
        );
        assert!(resolution.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn legacy_global_scalar_shape_is_schema_identical_and_strict() {
        let interner = StringInterner::new();
        let mut global = Record::new(
            SigCode::from_str("GLOB").unwrap(),
            FormKey {
                local: 0x38,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        global.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("GameHour")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"FNAM"),
                value: FieldValue::Bytes(vec![b's'].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"FLTV"),
                value: FieldValue::Float(12.0),
            },
        ]);
        assert!(classify_legacy_global(&global).is_empty());

        global
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "FNAM")
            .unwrap()
            .value = FieldValue::Bytes(vec![0].into());
        assert_eq!(
            classify_legacy_global(&global),
            vec![
                "legacy_global_field_multiplicity_unverified".to_string(),
                "legacy_global_field_shape_unverified".to_string(),
            ]
        );
    }

    #[test]
    fn legacy_weapon_repair_list_is_terminal_evidence_not_runtime_weapon_fanout() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut weapon = record(&interner, "WEAP", 0x200, "FalloutNV.esm", &[]);
        weapon.fields.push(FieldEntry {
            sig: SubrecordSig(*b"REPL"),
            value: FieldValue::FormKey(FormKey {
                local: 0x300,
                plugin: interner.intern("FalloutNV.esm"),
            }),
        });
        let repair_list = record(
            &interner,
            "FLST",
            0x300,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x400)],
        );
        let unrelated_weapon = record(&interner, "WEAP", 0x400, "FalloutNV.esm", &[]);
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&weapon, LegacyCreatureGame::Fnv, 1),
            source(&repair_list, LegacyCreatureGame::Fnv, 1),
            source(&unrelated_weapon, LegacyCreatureGame::Fnv, 1),
        ];

        let ledger = build_creature_dependency_ledger(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();

        assert_eq!(
            ledger
                .records
                .iter()
                .map(|record| (record.signature.as_str(), record.source.local))
                .collect::<Vec<_>>(),
            [("CREA", 0x100), ("WEAP", 0x200)]
        );
        assert!(
            ledger
                .records
                .iter()
                .find(|record| record.signature == "WEAP")
                .unwrap()
                .references
                .is_empty()
        );
        assert!(ledger.candidates[0].blockers.is_empty());
        let weapon = ledger
            .records
            .iter()
            .find(|record| record.signature == "WEAP")
            .unwrap();
        assert_eq!(
            weapon.lowering,
            PairRecordLoweringDisposition::Ready {
                mechanism: "legacy_weapon_fieldwise_fo4_rebuild".to_string(),
            }
        );
        assert!(weapon.degradations.is_empty());
    }

    #[test]
    fn schema_identical_form_id_list_is_mapper_ready_and_preserves_entry_order() {
        for (game, plugin) in [
            (LegacyCreatureGame::Fnv, "FalloutNV.esm"),
            (LegacyCreatureGame::Fo3, "Fallout3.esm"),
        ] {
            let interner = StringInterner::new();
            let creature = record(&interner, "CREA", 0x100, plugin, &[(plugin, 0x200)]);
            let mut list = Record::new(
                SigCode::from_str("FLST").unwrap(),
                FormKey {
                    local: 0x200,
                    plugin: interner.intern(plugin),
                },
            );
            list.fields.extend([
                FieldEntry {
                    sig: SubrecordSig(*b"EDID"),
                    value: FieldValue::String(interner.intern("CreatureWeapons")),
                },
                FieldEntry {
                    sig: SubrecordSig(*b"LNAM"),
                    value: FieldValue::FormKey(FormKey {
                        local: 0x301,
                        plugin: interner.intern(plugin),
                    }),
                },
                FieldEntry {
                    sig: SubrecordSig(*b"LNAM"),
                    value: FieldValue::FormKey(FormKey {
                        local: 0x300,
                        plugin: interner.intern(plugin),
                    }),
                },
            ]);
            let first = record(&interner, "MISC", 0x300, plugin, &[]);
            let second = record(&interner, "MISC", 0x301, plugin, &[]);
            let records = [
                source(&creature, game, 1),
                source(&list, game, 1),
                source(&first, game, 1),
                source(&second, game, 1),
            ];

            let ledger = build_creature_dependency_ledger(
                &records,
                CreatureDependencyOptions {
                    expected_creature_winners: Some(1),
                },
                &interner,
            )
            .unwrap();
            let receipt = ledger
                .records
                .iter()
                .find(|receipt| receipt.signature == "FLST")
                .unwrap();
            assert!(matches!(
                &receipt.lowering,
                PairRecordLoweringDisposition::Ready { mechanism }
                    if mechanism == "schema_identical_form_id_list_with_mapper"
            ));
            assert_eq!(
                receipt
                    .references
                    .iter()
                    .flat_map(|reference| reference.source_locators.iter())
                    .cloned()
                    .collect::<Vec<_>>(),
                ["LNAM[1]", "LNAM[0]"]
            );
        }
    }

    #[test]
    fn legacy_body_part_embedded_runtime_edges_are_authoritative_and_locator_exact() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut body_parts = Record::new(
            SigCode::from_str("BPTD").unwrap(),
            FormKey {
                local: 0x200,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let string = |signature: &[u8; 4], value: &str| FieldEntry {
            sig: SubrecordSig(*signature),
            value: FieldValue::String(interner.intern(value)),
        };
        body_parts.fields.extend([
            string(b"EDID", "CreatureBodyParts"),
            string(b"MODL", "Creatures\\Test\\Skeleton.nif"),
            string(b"BPTN", "Head"),
            string(b"BPNN", "Head"),
            string(b"BPNT", "Head"),
            string(b"BPNI", "Head"),
        ]);
        let mut bpnd = vec![0_u8; 84];
        bpnd[0..4].copy_from_slice(&1.0_f32.to_le_bytes());
        bpnd[4] = 0x09;
        bpnd[6] = 1;
        bpnd[7] = 25;
        bpnd[12..16].copy_from_slice(&0x300_u32.to_le_bytes());
        bpnd[36..40].copy_from_slice(&0x301_u32.to_le_bytes());
        bpnd[68..72].copy_from_slice(&0x302_u32.to_le_bytes());
        body_parts.fields.push(FieldEntry {
            sig: SubrecordSig(*b"BPND"),
            value: FieldValue::Bytes(bpnd.into()),
        });
        body_parts.fields.extend([
            string(b"NAM1", "Gore\\Head.nif"),
            string(b"NAM4", "Head"),
            FieldEntry {
                sig: SubrecordSig(*b"NAM5"),
                value: FieldValue::Bytes(Vec::new().into()),
            },
        ]);
        let debris = record(&interner, "DEBR", 0x300, "FalloutNV.esm", &[]);
        let explosion = record(&interner, "EXPL", 0x301, "FalloutNV.esm", &[]);
        let impacts = record(&interner, "IPDS", 0x302, "FalloutNV.esm", &[]);
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&body_parts, LegacyCreatureGame::Fnv, 1),
            source(&debris, LegacyCreatureGame::Fnv, 1),
            source(&explosion, LegacyCreatureGame::Fnv, 1),
            source(&impacts, LegacyCreatureGame::Fnv, 1),
        ];
        let ledger = build_creature_dependency_ledger_with_load_orders(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &interner,
        )
        .unwrap();
        let receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "BPTD")
            .unwrap();
        assert!(matches!(
            &receipt.lowering,
            PairRecordLoweringDisposition::Ready { mechanism }
                if mechanism == "verified_legacy_body_part_node_projection"
        ));
        assert_eq!(
            receipt
                .references
                .iter()
                .map(|reference| {
                    (
                        reference.target.local,
                        reference.source_locators.clone(),
                        match &reference.resolution {
                            DependencyReferenceResolution::Followed { signature } => {
                                signature.as_str()
                            }
                            other => panic!("expected followed BPND reference, got {other:?}"),
                        },
                    )
                })
                .collect::<Vec<_>>(),
            [
                (
                    0x300,
                    vec!["BPND[0].explodable_debris@12".to_string()],
                    "DEBR",
                ),
                (
                    0x301,
                    vec!["BPND[0].severable_explosion@36".to_string()],
                    "EXPL",
                ),
                (
                    0x302,
                    vec!["BPND[0].severable_impact_dataset@68".to_string()],
                    "IPDS",
                ),
            ]
        );
    }

    #[test]
    fn legacy_debris_runtime_closure_and_explosion_lowering_are_authoritative_and_locator_exact() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200), ("FalloutNV.esm", 0x201)],
        );
        let mut debris = Record::new(
            SigCode::from_str("DEBR").unwrap(),
            FormKey {
                local: 0x200,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        debris.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("CreatureGoreBits")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(
                    [50_u8]
                        .into_iter()
                        .chain(b"Gore\\CreatureBit01.nif\0".iter().copied())
                        .chain([1])
                        .collect::<Vec<_>>()
                        .into(),
                ),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODT"),
                value: FieldValue::Bytes(vec![1].into()),
            },
        ]);
        let mut explosion = Record::new(
            SigCode::from_str("EXPL").unwrap(),
            FormKey {
                local: 0x201,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let mut data = vec![0_u8; 52];
        data[8..12].copy_from_slice(&55.0_f32.to_le_bytes());
        data[12..16].copy_from_slice(&0x300_u32.to_le_bytes());
        data[16..20].copy_from_slice(&0x301_u32.to_le_bytes());
        data[20..24].copy_from_slice(&0x43_u32.to_le_bytes());
        data[28..32].copy_from_slice(&0x302_u32.to_le_bytes());
        data[32..36].copy_from_slice(&0x303_u32.to_le_bytes());
        data[48..52].copy_from_slice(&1_u32.to_le_bytes());
        explosion.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("CreatureLimbExplosion")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"OBND"),
                value: FieldValue::Bytes(vec![0; 12].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODL"),
                value: FieldValue::String(interner.intern("Gore\\CreatureExplosion.nif")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(data.into()),
            },
        ]);
        let light = record(&interner, "LIGH", 0x300, "FalloutNV.esm", &[]);
        let sound_1 = record(&interner, "SOUN", 0x301, "FalloutNV.esm", &[]);
        let impacts = record(&interner, "IPDS", 0x302, "FalloutNV.esm", &[]);
        let sound_2 = record(&interner, "SOUN", 0x303, "FalloutNV.esm", &[]);
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&debris, LegacyCreatureGame::Fnv, 1),
            source(&explosion, LegacyCreatureGame::Fnv, 1),
            source(&light, LegacyCreatureGame::Fnv, 1),
            source(&sound_1, LegacyCreatureGame::Fnv, 1),
            source(&impacts, LegacyCreatureGame::Fnv, 1),
            source(&sound_2, LegacyCreatureGame::Fnv, 1),
        ];
        let ledger = build_creature_dependency_ledger_with_load_orders(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &interner,
        )
        .unwrap();

        let debris_receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "DEBR")
            .unwrap();
        assert!(matches!(
            &debris_receipt.lowering,
            PairRecordLoweringDisposition::Ready { mechanism }
                if mechanism == "legacy_debris_projection_with_runtime_model_closure"
        ));
        let explosion_receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "EXPL")
            .unwrap();
        assert!(matches!(
            &explosion_receipt.lowering,
            PairRecordLoweringDisposition::Ready { mechanism }
                if mechanism == "verified_legacy_explosion_data_projection"
        ));
        assert_eq!(
            explosion_receipt
                .references
                .iter()
                .map(|reference| {
                    (
                        reference.target.local,
                        reference.source_locators.clone(),
                        match &reference.resolution {
                            DependencyReferenceResolution::Followed { signature } => {
                                signature.as_str()
                            }
                            other => panic!("expected followed EXPL reference, got {other:?}"),
                        },
                    )
                })
                .collect::<Vec<_>>(),
            [
                (0x300, vec!["DATA[0].light@12".to_string()], "LIGH"),
                (0x301, vec!["DATA[0].sound_1@16".to_string()], "SOUN"),
                (0x302, vec!["DATA[0].impact_dataset@28".to_string()], "IPDS",),
                (0x303, vec!["DATA[0].sound_2@32".to_string()], "SOUN"),
            ]
        );
    }

    #[test]
    fn malformed_form_id_list_remains_terminally_blocked() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut list = Record::new(
            SigCode::from_str("FLST").unwrap(),
            FormKey {
                local: 0x200,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        list.fields.push(FieldEntry {
            sig: SubrecordSig(*b"LNAM"),
            value: FieldValue::Uint(0x300),
        });
        let records = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&list, LegacyCreatureGame::Fnv, 1),
        ];

        let ledger = build_creature_dependency_ledger(
            &records,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert!(ledger.candidates[0].blockers.iter().any(|blocker| matches!(
            blocker,
            CreatureDependencyBlocker::LoweringUnavailable { reason_code, .. }
                if reason_code == "legacy_form_id_list_entry_shape_unverified"
        )));
    }

    #[test]
    fn legacy_scro_player_ref_is_remapped_to_the_exact_fo4_intrinsic() {
        for (game, plugin) in [
            (LegacyCreatureGame::Fnv, "FalloutNV.esm"),
            (LegacyCreatureGame::Fo3, "Fallout3.esm"),
        ] {
            let interner = StringInterner::new();
            let creature = record(&interner, "CREA", 0x100, plugin, &[(plugin, 0x200)]);
            let mut script = record(&interner, "SCPT", 0x200, plugin, &[]);
            script.fields.push(FieldEntry {
                sig: SubrecordSig(*b"SCRO"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x14,
                    plugin: interner.intern(plugin),
                }),
            });
            let records = [source(&creature, game, 1), source(&script, game, 1)];
            let ledger = build_creature_dependency_ledger(
                &records,
                CreatureDependencyOptions {
                    expected_creature_winners: Some(1),
                },
                &interner,
            )
            .unwrap();
            let reference = ledger
                .records
                .iter()
                .find(|receipt| receipt.signature == "SCPT")
                .unwrap()
                .references
                .first()
                .unwrap();
            let script_receipt = ledger
                .records
                .iter()
                .find(|receipt| receipt.signature == "SCPT")
                .unwrap();
            let (mechanism, code, disposition) = match game {
                LegacyCreatureGame::Fnv => (
                    "fnv_legacy_scripting_bridge",
                    "legacy_scpt_consumed_by_scripting_bridge",
                    crate::source_rig::CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
                ),
                LegacyCreatureGame::Fo3 => (
                    "fo3_legacy_script_omission",
                    "legacy_fo3_scpt_omitted_nonportable_script",
                    crate::source_rig::CreatureFallbackDisposition::Omitted,
                ),
            };
            assert_eq!(
                script_receipt.lowering,
                PairRecordLoweringDisposition::Consumed {
                    mechanism: mechanism.to_string(),
                }
            );
            assert_eq!(script_receipt.degradations.len(), 1);
            assert_eq!(script_receipt.degradations[0].code, code);
            assert_eq!(script_receipt.degradations[0].disposition, disposition);
            assert_eq!(reference.source_locators, ["SCRO[0]"]);
            assert_eq!(
                reference.resolution,
                DependencyReferenceResolution::TargetIntrinsic {
                    signature: "REFR".to_string(),
                    mapped_target: StableFormKey {
                        plugin: "Fallout4.esm".to_string(),
                        local: 0x14,
                    },
                }
            );
            assert!(
                !ledger.candidates[0].blockers.iter().any(|blocker| matches!(
                    blocker,
                    CreatureDependencyBlocker::MissingSourceRecord { source }
                        if source.local == 0x14
                ))
            );
        }
    }

    #[test]
    fn legacy_idle_uses_generated_behavior_fallback_but_malformed_topology_stays_blocked() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut idle = Record::new(
            SigCode::from_str("IDLE").unwrap(),
            FormKey {
                local: 0x200,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        idle.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("CreatureSpecialIdle")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODL"),
                value: FieldValue::String(interner.intern("Creatures\\Test\\IdleAnims\\Idle.kf")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"ANAM"),
                value: FieldValue::Bytes(vec![0; 8].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(vec![7, 0, 0, 0xcd, 0, 0].into()),
            },
        ]);
        let load_orders = [LegacyPluginLoadOrder {
            game: LegacyCreatureGame::Fnv,
            source_plugin: "FalloutNV.esm".to_string(),
            source_master_names: Vec::new(),
        }];

        let ready = build_creature_dependency_ledger_with_load_orders(
            &[
                source(&creature, LegacyCreatureGame::Fnv, 1),
                source(&idle, LegacyCreatureGame::Fnv, 1),
            ],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &load_orders,
            &interner,
        )
        .unwrap();
        let idle_receipt = ready
            .records
            .iter()
            .find(|receipt| receipt.signature == "IDLE")
            .unwrap();
        assert_eq!(
            idle_receipt.lowering,
            PairRecordLoweringDisposition::Consumed {
                mechanism: "legacy_idle_replaced_by_generated_behavior".to_string(),
            }
        );
        assert_eq!(
            idle_receipt
                .degradations
                .iter()
                .map(|degradation| degradation.code.as_str())
                .collect::<Vec<_>>(),
            [
                "legacy_idle_source_data_semantics_omitted",
                "legacy_idle_source_kf_replaced_by_generated_behavior",
            ]
        );
        assert_eq!(ready.ready_candidates, 1);

        idle.fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "ANAM")
            .unwrap()
            .value = FieldValue::Bytes(vec![0; 4].into());
        let blocked = build_creature_dependency_ledger_with_load_orders(
            &[
                source(&creature, LegacyCreatureGame::Fnv, 1),
                source(&idle, LegacyCreatureGame::Fnv, 1),
            ],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &load_orders,
            &interner,
        )
        .unwrap();
        assert!(
            blocked.candidates[0]
                .blockers
                .iter()
                .any(|blocker| matches!(
                    blocker,
                    CreatureDependencyBlocker::LoweringUnavailable { reason_code, .. }
                        if reason_code == "legacy_idle_anam_shape_unverified"
                ))
        );
    }

    #[test]
    fn legacy_ctda_player_ref_package_fallback_is_blocked() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut package = record(&interner, "PACK", 0x200, "FalloutNV.esm", &[]);
        package.eid = Some(interner.intern("CreatureTravel"));
        let mut pkdt = [0_u8; 12];
        pkdt[..4].copy_from_slice(&0x0000_0204_u32.to_le_bytes());
        pkdt[4] = 6;
        pkdt[6..8].copy_from_slice(&0x20_u16.to_le_bytes());
        let mut schedule = [0_u8; 8];
        schedule[..4].copy_from_slice(&[u8::MAX, u8::MAX, 0, u8::MAX]);
        let mut location = [0_u8; 12];
        location[8..12].copy_from_slice(&128_i32.to_le_bytes());
        let mut condition = vec![0; 28];
        condition[20..24].copy_from_slice(&2_u32.to_le_bytes());
        condition[24..28].copy_from_slice(&0x14_u32.to_le_bytes());
        package.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"PKDT"),
                value: FieldValue::Bytes(pkdt.to_vec().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PSDT"),
                value: FieldValue::Bytes(schedule.to_vec().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"CTDA"),
                value: FieldValue::Bytes(condition.into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PLDT"),
                value: FieldValue::Bytes(location.to_vec().into()),
            },
        ]);
        for marker in [b"POBA", b"POEA", b"POCA"] {
            package.fields.push(FieldEntry {
                sig: SubrecordSig(*marker),
                value: FieldValue::Bytes(Vec::new().into()),
            });
            package.fields.push(FieldEntry {
                sig: SubrecordSig(*b"SCHR"),
                value: FieldValue::Bytes(vec![0; 20].into()),
            });
        }

        let ledger = build_creature_dependency_ledger_with_load_orders(
            &[
                source(&creature, LegacyCreatureGame::Fnv, 1),
                source(&package, LegacyCreatureGame::Fnv, 1),
            ],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &interner,
        )
        .unwrap();
        let package_receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "PACK")
            .unwrap();
        assert!(matches!(
            package_receipt.lowering,
            PairRecordLoweringDisposition::Blocked { ref reason_codes }
                if reason_codes
                    .iter()
                    .any(|reason| reason == "legacy_pack_generic_fallback_forbidden")
        ));
        assert!(package_receipt.references.is_empty());
        assert!(ledger.candidates[0].blockers.iter().any(|blocker| matches!(
            blocker,
            CreatureDependencyBlocker::LoweringUnavailable { reason_code, .. }
                if reason_code == "legacy_pack_generic_fallback_forbidden"
        )));

        let excluded = build_creature_dependency_ledger_with_load_orders_and_exclusions(
            &[
                source(&creature, LegacyCreatureGame::Fnv, 1),
                source(&package, LegacyCreatureGame::Fnv, 1),
            ],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &BTreeSet::from(["pack".to_string()]),
            &interner,
        )
        .unwrap();
        assert_eq!(excluded.ready_candidates, 1);
        assert!(excluded.candidates[0].blockers.is_empty());
        assert!(matches!(
            excluded
                .records
                .iter()
                .find(|receipt| receipt.signature == "PACK")
                .unwrap()
                .lowering,
            PairRecordLoweringDisposition::Blocked { .. }
        ));
    }

    #[test]
    fn player_ref_number_outside_scro_remains_unresolved() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x14)],
        );
        let ledger = build_creature_dependency_ledger(
            &[source(&creature, LegacyCreatureGame::Fnv, 1)],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert_eq!(
            ledger.records[0].references[0].resolution,
            DependencyReferenceResolution::MissingSourceRecord
        );
        assert!(ledger.candidates[0].blockers.iter().any(|blocker| matches!(
            blocker,
            CreatureDependencyBlocker::MissingSourceRecord { source }
                if source.plugin == "FalloutNV.esm" && source.local == 0x14
        )));
    }

    #[test]
    fn fnv_ammo_dat2_projectile_is_followed_with_exact_embedded_locator() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut ammo = record(&interner, "AMMO", 0x200, "FalloutNV.esm", &[]);
        ammo.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Struct(vec![
                (interner.intern("speed"), FieldValue::Float(1.0)),
                (interner.intern("flags"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_4"), FieldValue::Uint(0)),
                (interner.intern("value"), FieldValue::Uint(1)),
                (interner.intern("clip_rounds"), FieldValue::Uint(0)),
            ]),
        });
        let mut dat2 = Vec::new();
        dat2.extend_from_slice(&1u32.to_le_bytes());
        dat2.extend_from_slice(&0x0000_0400u32.to_le_bytes());
        dat2.extend_from_slice(&0.1f32.to_le_bytes());
        ammo.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DAT2"),
            value: FieldValue::Bytes(dat2.into()),
        });
        let projectile = record(&interner, "PROJ", 0x400, "FalloutNV.esm", &[]);
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&ammo, LegacyCreatureGame::Fnv, 1),
            source(&projectile, LegacyCreatureGame::Fnv, 1),
        ];
        let load_orders = [LegacyPluginLoadOrder {
            game: LegacyCreatureGame::Fnv,
            source_plugin: "FalloutNV.esm".to_string(),
            source_master_names: Vec::new(),
        }];
        let ledger = build_creature_dependency_ledger_with_load_orders(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &load_orders,
            &interner,
        )
        .unwrap();
        let ammo_receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "AMMO")
            .unwrap();
        assert_eq!(
            ammo_receipt.references[0].source_locators,
            ["DAT2[0].projectile@4"]
        );
        assert!(matches!(
            ammo_receipt.references[0].resolution,
            DependencyReferenceResolution::Followed { ref signature } if signature == "PROJ"
        ));
        assert!(matches!(
            ammo_receipt.lowering,
            PairRecordLoweringDisposition::Ready { ref mechanism }
                if mechanism == "verified_legacy_ammo_to_fo4"
        ));
        assert_eq!(ledger.ready_candidates, 1);

        let without_load_order = build_creature_dependency_ledger(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert!(
            without_load_order.candidates[0]
                .blockers
                .iter()
                .any(|blocker| matches!(
                    blocker,
                    CreatureDependencyBlocker::LoweringUnavailable { reason_code, .. }
                        if reason_code == "legacy_ammo_source_load_order_unavailable"
                ))
        );
    }

    #[test]
    fn fnv_lvlo_item_is_followed_with_exact_embedded_locator() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut leveled_item = record(&interner, "LVLI", 0x200, "FalloutNV.esm", &[]);
        let mut lvlo = Vec::new();
        lvlo.extend_from_slice(&1u16.to_le_bytes());
        lvlo.extend_from_slice(&0u16.to_le_bytes());
        lvlo.extend_from_slice(&0x0000_0300u32.to_le_bytes());
        lvlo.extend_from_slice(&1i16.to_le_bytes());
        lvlo.extend_from_slice(&0u16.to_le_bytes());
        leveled_item.fields.push(FieldEntry {
            sig: SubrecordSig(*b"LVLO"),
            value: FieldValue::Bytes(lvlo.into()),
        });
        let item = record(&interner, "ARMO", 0x300, "FalloutNV.esm", &[]);
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&leveled_item, LegacyCreatureGame::Fnv, 1),
            source(&item, LegacyCreatureGame::Fnv, 1),
        ];
        let load_orders = [LegacyPluginLoadOrder {
            game: LegacyCreatureGame::Fnv,
            source_plugin: "FalloutNV.esm".to_string(),
            source_master_names: Vec::new(),
        }];

        let ledger = build_creature_dependency_ledger_with_load_orders(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &load_orders,
            &interner,
        )
        .unwrap();
        let leveled_item_receipt = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "LVLI")
            .unwrap();
        assert_eq!(
            leveled_item_receipt.references[0].source_locators,
            ["LVLO[0].item@4"]
        );
        assert!(matches!(
            leveled_item_receipt.references[0].resolution,
            DependencyReferenceResolution::Followed { ref signature } if signature == "ARMO"
        ));
        assert!(matches!(
            leveled_item_receipt.lowering,
            PairRecordLoweringDisposition::Ready { ref mechanism }
                if mechanism == "verified_legacy_leveled_item_relayout"
        ));
        assert_eq!(ledger.ready_candidates, 1);

        let without_load_order = build_creature_dependency_ledger(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert!(
            without_load_order.candidates[0]
                .blockers
                .iter()
                .any(|blocker| matches!(
                    blocker,
                    CreatureDependencyBlocker::LoweringUnavailable { reason_code, .. }
                        if reason_code == "legacy_leveled_item_source_load_order_unavailable"
                ))
        );
    }

    #[test]
    fn fnv_ipds_material_impacts_are_followed_with_exact_embedded_locators() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut impact_dataset = record(&interner, "IPDS", 0x200, "FalloutNV.esm", &[]);
        let mut data = vec![0_u8; 40];
        data[0..4].copy_from_slice(&0x0000_0300_u32.to_le_bytes());
        data[36..40].copy_from_slice(&0x0000_0301_u32.to_le_bytes());
        impact_dataset.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(data.into()),
        });
        let stone = record(&interner, "IPCT", 0x300, "FalloutNV.esm", &[]);
        let hollow_metal = record(&interner, "IPCT", 0x301, "FalloutNV.esm", &[]);
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&impact_dataset, LegacyCreatureGame::Fnv, 1),
            source(&stone, LegacyCreatureGame::Fnv, 1),
            source(&hollow_metal, LegacyCreatureGame::Fnv, 1),
        ];
        let load_orders = [LegacyPluginLoadOrder {
            game: LegacyCreatureGame::Fnv,
            source_plugin: "FalloutNV.esm".to_string(),
            source_master_names: Vec::new(),
        }];

        let ledger = build_creature_dependency_ledger_with_load_orders(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &load_orders,
            &interner,
        )
        .unwrap();
        let impact_dataset = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "IPDS")
            .unwrap();
        assert_eq!(
            impact_dataset
                .references
                .iter()
                .map(|reference| reference.source_locators.clone())
                .collect::<Vec<_>>(),
            [
                vec!["DATA[0].stone@0".to_string()],
                vec!["DATA[0].hollow_metal@36".to_string()],
            ]
        );
        assert!(impact_dataset.references.iter().all(|reference| matches!(
            reference.resolution,
            DependencyReferenceResolution::Followed { ref signature } if signature == "IPCT"
        )));
        assert!(matches!(
            impact_dataset.lowering,
            PairRecordLoweringDisposition::Ready { ref mechanism }
                if mechanism == "verified_legacy_impact_dataset_material_projection"
        ));

        let without_load_order = build_creature_dependency_ledger(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert!(
            without_load_order.candidates[0]
                .blockers
                .iter()
                .any(|blocker| matches!(
                    blocker,
                    CreatureDependencyBlocker::LoweringUnavailable { reason_code, .. }
                        if reason_code == "legacy_impact_dataset_source_load_order_unavailable"
                ))
        );
    }

    #[test]
    fn verified_impact_texture_set_is_a_followed_runtime_dependency() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut impact = record(&interner, "IPCT", 0x200, "FalloutNV.esm", &[]);
        let mut data = Vec::new();
        data.extend_from_slice(&0.25f32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&15.0f32.to_le_bytes());
        data.extend_from_slice(&16.0f32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        impact.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(data.into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DNAM"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x300,
                    plugin: interner.intern("FalloutNV.esm"),
                }),
            },
        ]);
        let mut texture = record(&interner, "TXST", 0x300, "FalloutNV.esm", &[]);
        texture.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("ImpactTexture")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"OBND"),
                value: FieldValue::Bytes([0; 12].as_slice().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"TX00"),
                value: FieldValue::String(interner.intern("textures\\impact_d.dds")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"TX01"),
                value: FieldValue::String(interner.intern("textures\\impact_n.dds")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DNAM"),
                value: FieldValue::Uint(0),
            },
        ]);
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&impact, LegacyCreatureGame::Fnv, 1),
            source(&texture, LegacyCreatureGame::Fnv, 1),
        ];

        let ledger = build_creature_dependency_ledger(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        let impact = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "IPCT")
            .unwrap();
        assert!(matches!(
            impact.lowering,
            PairRecordLoweringDisposition::Ready { ref mechanism }
                if mechanism == "verified_legacy_impact_layout_projection"
        ));
        assert!(matches!(
            impact.references[0].resolution,
            DependencyReferenceResolution::Followed { ref signature } if signature == "TXST"
        ));
        assert_eq!(impact.references[0].source_locators, ["DNAM[0]"]);
        let texture = ledger
            .records
            .iter()
            .find(|receipt| receipt.signature == "TXST")
            .unwrap();
        assert!(matches!(
            texture.lowering,
            PairRecordLoweringDisposition::Ready { ref mechanism }
                if mechanism == "verified_legacy_texture_set_projection"
        ));
        assert_eq!(ledger.ready_candidates, 1);
    }

    #[test]
    fn foldable_ammo_effect_is_receipt_bound_to_the_exact_weapon_owner() {
        let interner = StringInterner::new();
        let creature = record(
            &interner,
            "CREA",
            0x100,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200)],
        );
        let mut weapon = record(&interner, "WEAP", 0x200, "FalloutNV.esm", &[]);
        weapon.fields.push(FieldEntry {
            sig: SubrecordSig(*b"NAM0"),
            value: FieldValue::FormKey(FormKey {
                local: 0x300,
                plugin: interner.intern("FalloutNV.esm"),
            }),
        });
        let mut ammo = record(&interner, "AMMO", 0x300, "FalloutNV.esm", &[]);
        ammo.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Struct(vec![
                    (interner.intern("speed"), FieldValue::Float(1.0)),
                    (interner.intern("flags"), FieldValue::Uint(0)),
                    (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                    (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
                    (interner.intern("unknown_u8_4"), FieldValue::Uint(0)),
                    (interner.intern("value"), FieldValue::Uint(1)),
                    (interner.intern("clip_rounds"), FieldValue::Uint(0)),
                ]),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DAT2"),
                value: FieldValue::Struct(vec![
                    (interner.intern("proj_per_shot"), FieldValue::Uint(1)),
                    (interner.intern("projectile"), FieldValue::Uint(0)),
                    (interner.intern("weight"), FieldValue::Float(0.1)),
                    (interner.intern("consumed_ammo"), FieldValue::Uint(0)),
                    (
                        interner.intern("consumed_percentage"),
                        FieldValue::Float(0.0),
                    ),
                ]),
            },
            FieldEntry {
                sig: SubrecordSig(*b"RCIL"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x400,
                    plugin: interner.intern("FalloutNV.esm"),
                }),
            },
        ]);
        let mut effect = record(&interner, "AMEF", 0x400, "FalloutNV.esm", &[]);
        effect.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Struct(vec![
                (interner.intern("type"), FieldValue::Uint(0)),
                (interner.intern("operation"), FieldValue::Uint(1)),
                (interner.intern("value"), FieldValue::Float(1.25)),
            ]),
        });
        let sources = [
            source(&creature, LegacyCreatureGame::Fnv, 1),
            source(&weapon, LegacyCreatureGame::Fnv, 1),
            source(&ammo, LegacyCreatureGame::Fnv, 1),
            source(&effect, LegacyCreatureGame::Fnv, 1),
        ];
        let ledger = build_creature_dependency_ledger(
            &sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert_eq!(
            ledger.candidates[0].ammo_effect_folds,
            [AmmoEffectOwnerFoldReceipt {
                source_effect: StableFormKey {
                    local: 0x400,
                    plugin: "FalloutNV.esm".to_string(),
                },
                source_ammo: StableFormKey {
                    local: 0x300,
                    plugin: "FalloutNV.esm".to_string(),
                },
                consuming_weapon: StableFormKey {
                    local: 0x200,
                    plugin: "FalloutNV.esm".to_string(),
                },
                rcil_locator: "RCIL[0]".to_string(),
                ammo_locator: "NAM0[0]".to_string(),
                channel: AmmoEffectFoldChannel::Damage,
                operation: AmmoEffectFoldOperation::Multiply,
                value_bits: 1.25f32.to_bits(),
            }]
        );
        assert!(matches!(
            ledger
                .records
                .iter()
                .find(|record| record.signature == "AMEF")
                .unwrap()
                .lowering,
            PairRecordLoweringDisposition::Consumed { .. }
        ));
        assert!(
            !ledger.candidates[0].blockers.iter().any(|blocker| matches!(
                blocker,
                CreatureDependencyBlocker::LoweringUnavailable { reason_code, .. }
                    if reason_code == "legacy_ammo_effect_missing_owner_weapon_projection"
            ))
        );

        let winners = select_winners(&sources, &interner).unwrap();
        let partial_closure = BTreeSet::from([
            StableFormKey {
                local: 0x200,
                plugin: "FalloutNV.esm".to_string(),
            },
            StableFormKey {
                local: 0x300,
                plugin: "FalloutNV.esm".to_string(),
            },
        ]);
        assert!(
            build_candidate_ammo_effect_folds(&partial_closure, &winners, &interner)
                .unwrap()
                .0
                .is_empty()
        );

        let excluded_creature = record(
            &interner,
            "CREA",
            0x101,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x200), ("FalloutNV.esm", 0x300)],
        );
        let mut excluded_ammo = ammo.clone();
        excluded_ammo
            .fields
            .retain(|field| field.sig.as_str() == "RCIL");
        let excluded_sources = [
            source(&excluded_creature, LegacyCreatureGame::Fnv, 1),
            source(&weapon, LegacyCreatureGame::Fnv, 1),
            source(&excluded_ammo, LegacyCreatureGame::Fnv, 1),
            source(&effect, LegacyCreatureGame::Fnv, 1),
        ];
        let excluded = build_creature_dependency_ledger_with_load_orders_and_exclusions(
            &excluded_sources,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &BTreeSet::from(["AMMO".to_string(), "WEAP".to_string()]),
            &interner,
        )
        .unwrap();
        assert_eq!(excluded.candidates[0].ammo_effect_folds.len(), 1);
        assert!(
            excluded.candidates[0]
                .closure_form_keys
                .contains(&StableFormKey {
                    local: 0x400,
                    plugin: "FalloutNV.esm".to_string(),
                })
        );

        let direct_ammo_creature = record(
            &interner,
            "CREA",
            0x101,
            "FalloutNV.esm",
            &[("FalloutNV.esm", 0x300)],
        );
        let without_weapon = [
            source(&direct_ammo_creature, LegacyCreatureGame::Fnv, 1),
            source(&ammo, LegacyCreatureGame::Fnv, 1),
            source(&effect, LegacyCreatureGame::Fnv, 1),
        ];
        let ledger = build_creature_dependency_ledger(
            &without_weapon,
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        assert!(ledger.candidates[0].blockers.is_empty());
        assert!(ledger.candidates[0].ammo_effect_folds.is_empty());
    }
}
