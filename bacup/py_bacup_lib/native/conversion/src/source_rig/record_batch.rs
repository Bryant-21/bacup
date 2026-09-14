//! Atomic publication of validated FO4 creature record families.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::SourceRigExecutableRecipe;
use super::{
    CreatureRecordProjectionClosure, CreatureTargetRecordReference, ProjectedRecordIdentity,
    SourceCreatureIdentity, TargetFormKey,
};
use crate::formkey_mapper::{FormKeyMapper, MapperState};
use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::{PluginSession, open_session};
use crate::sym::StringInterner;
use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
use crate::target_write::{PreparedRecordBatch, prepare_record_batch_in_slot};

const MAX_LOCAL_FORM_ID: u32 = 0x00ff_ffff;
pub const CREATURE_RECORD_COMMIT_LEDGER_RELATIVE_PATH: &str =
    "debug/creature_corpus/record_commit_ledger.json";
pub const CREATURE_RECORD_COMMIT_LEDGER_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureProjectedRecordCommitment {
    pub family_id: String,
    pub identity: ProjectedRecordIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureTargetDependencyCommitment {
    pub family_id: String,
    pub dependency: CreatureTargetRecordReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureEncodedRecordCommitment {
    pub form_key: TargetFormKey,
    pub signature: String,
    pub encoded_blake3: String,
}

/// Structural and content-addressed proof that a prepared batch is exactly the
/// family batch reconstructed from its recipe.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRecordRecipeBinding {
    pub family_ids: Vec<String>,
    pub projected_identities: Vec<CreatureProjectedRecordCommitment>,
    pub required_target_records: Vec<CreatureTargetDependencyCommitment>,
    pub primary_mappings: Vec<CreaturePrimaryRecordMapping>,
    pub encoded_records: Vec<CreatureEncodedRecordCommitment>,
}

/// The canonical statement made before target records are committed.
///
/// It is deliberately filesystem-free: creating it cannot make a ledger appear
/// for a batch that is later abandoned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRecordCommitIntent {
    pub version: u32,
    pub target_plugin: String,
    pub schema_id: String,
    pub recipe_blake3: String,
    pub family_ids: Vec<String>,
    pub record_count: usize,
    pub mapping_count: usize,
    pub reserved_form_keys: Vec<TargetFormKey>,
    pub primary_mappings: Vec<CreaturePrimaryRecordMapping>,
    pub races: Vec<CreatureRaceRecordReceipt>,
    pub recipe_binding: CreatureRecordRecipeBinding,
}

/// The durable, post-commit proof.  A ledger never implies that its target can
/// be rolled back: persistence failure is reported after the target commit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRecordCommitLedger {
    pub version: u32,
    pub intent: CreatureRecordCommitIntent,
    pub intent_blake3: String,
    pub receipt: CreatureRecordBatchReceipt,
    pub receipt_blake3: String,
}

/// A coordinator retaining the prepared target mutation and its precommit
/// intent. Dropping it abandons the target batch just like `PreparedCreatureRecordBatch`.
pub struct PreparedCreatureRecordCommit<'session, 'mapper, 'store> {
    prepared: PreparedCreatureRecordBatch<'session, 'mapper, 'store>,
    intent: CreatureRecordCommitIntent,
}

/// Result of an infallible target commit. The caller may now stage the ledger;
/// an error there never claims to undo the already committed target records.
#[derive(Clone, Debug)]
pub struct CommittedCreatureRecordBatch {
    receipt: CreatureRecordBatchReceipt,
    ledger: CreatureRecordCommitLedger,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreaturePrimaryRecordMapping {
    pub source: SourceCreatureIdentity,
    pub target: TargetFormKey,
}

#[derive(Clone, Debug)]
pub struct CreatureRecordFamilyBatch {
    pub family_id: String,
    pub closures: Vec<CreatureRecordProjectionClosure>,
    pub primary_mappings: Vec<CreaturePrimaryRecordMapping>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRecordBatchReceipt {
    pub family_ids: Vec<String>,
    pub families: Vec<CreatureRecordFamilyReceipt>,
    pub record_count: usize,
    pub mapping_count: usize,
    pub reserved_form_keys: Vec<TargetFormKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRecordFamilyReceipt {
    pub family_id: String,
    pub record_count: usize,
    pub mapping_count: usize,
    pub primary_mappings: Vec<CreaturePrimaryRecordMapping>,
    pub races: Vec<CreatureRaceRecordReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRaceRecordReceipt {
    pub form_key: TargetFormKey,
    pub attack_events: Vec<String>,
    pub attack_data_entries: usize,
}

pub struct PreparedCreatureRecordBatch<'session, 'mapper, 'store> {
    session: &'session mut PluginSession<'store>,
    mapper_state: &'mapper mut MapperState,
    records: PreparedRecordBatch,
    next_mapper_state: MapperState,
    receipt: CreatureRecordBatchReceipt,
    recipe_binding: CreatureRecordRecipeBinding,
}

impl PreparedCreatureRecordBatch<'_, '_, '_> {
    pub fn receipt(&self) -> &CreatureRecordBatchReceipt {
        &self.receipt
    }

    pub fn commit(self) -> CreatureRecordBatchReceipt {
        let inserted = self.session.commit_prepared_record_batch(self.records);
        debug_assert_eq!(inserted, self.receipt.record_count);
        *self.mapper_state = self.next_mapper_state;
        self.receipt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureRecordBatchStage {
    EncodeRecord,
    AddRecord,
    AddMapping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreatureRecordBatchFailureInjection {
    None,
    At {
        stage: CreatureRecordBatchStage,
        index: usize,
    },
}

#[derive(Debug, Error)]
pub enum CreatureRecordBatchError {
    #[error("invalid creature record family batch ({code}): {message}")]
    InvalidBatch { code: &'static str, message: String },
    #[error("duplicate creature family id {family_id:?}")]
    DuplicateFamily { family_id: String },
    #[error(
        "record FormKey {local:06X}@{plugin} is duplicated inside creature family {family_id:?}"
    )]
    DuplicateRecordFormKey {
        family_id: String,
        local: u32,
        plugin: String,
    },
    #[error(
        "record FormKey {local:06X}@{plugin} collides between creature families {first_family:?} and {second_family:?}"
    )]
    CrossFamilyRecordCollision {
        first_family: String,
        second_family: String,
        local: u32,
        plugin: String,
    },
    #[error(
        "record FormKey {local:06X}@{plugin} collides with existing target record {existing_signature}"
    )]
    ExistingTargetCollision {
        local: u32,
        plugin: String,
        existing_signature: String,
    },
    #[error("record FormKey {local:06X}@{plugin} is already reserved in mapper state")]
    ReservedTargetCollision { local: u32, plugin: String },
    #[error("source mapping {source_identity} is duplicated inside creature family {family_id:?}")]
    DuplicateSourceMapping {
        family_id: String,
        source_identity: String,
    },
    #[error(
        "source mapping {source_identity} collides between creature families {first_family:?} and {second_family:?}"
    )]
    CrossFamilyMappingCollision {
        first_family: String,
        second_family: String,
        source_identity: String,
    },
    #[error("source mapping {source_identity} already exists in mapper state")]
    ExistingSourceMapping { source_identity: String },
    #[error(
        "record {record} references unresolved output helper {local:06X}@{plugin} after normalization"
    )]
    DanglingHelperReference {
        record: String,
        local: u32,
        plugin: String,
    },
    #[error("record {record} references plugin {plugin:?}, which is not an output master")]
    UnlistedReferencePlugin { record: String, plugin: String },
    #[error(
        "creature family {family_id:?} requires missing target {signature} {local:06X}@{plugin}"
    )]
    MissingTargetDependency {
        family_id: String,
        signature: String,
        local: u32,
        plugin: String,
    },
    #[error(
        "creature family {family_id:?} requires {expected_signature} {local:06X}@{plugin}, but the target record is {actual_signature}"
    )]
    TargetDependencySignatureMismatch {
        family_id: String,
        expected_signature: String,
        actual_signature: String,
        local: u32,
        plugin: String,
    },
    #[error("record {record_index} ({record}) failed target encoding: {message}")]
    Encode {
        record_index: usize,
        record: String,
        message: String,
    },
    #[error("injected {stage:?} failure at batch index {index}")]
    InjectedFailure {
        stage: CreatureRecordBatchStage,
        index: usize,
    },
    #[error("failed to serialize creature record commit receipt: {0}")]
    Serialization(String),
}

#[derive(Debug, Error)]
pub enum CreatureRecordCommitLedgerError {
    #[error("record commit ledger is invalid ({code}): {message}")]
    Invalid { code: &'static str, message: String },
    #[error("record commit ledger recipe hash is stale: expected {expected}, found {actual}")]
    StaleRecipeHash { expected: String, actual: String },
    #[error("record commit ledger serialization failed: {0}")]
    Serialization(String),
    #[error("record commit ledger JSON is not canonical")]
    NonCanonicalJson,
    #[error("record commit ledger I/O at {path}: {message}")]
    Io { path: PathBuf, message: String },
    #[error("record commit ledger destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("target records are committed; record commit ledger persistence failed: {0}")]
    PostCommitPersistence(Box<CreatureRecordCommitLedgerError>),
}

impl CreatureRecordBatchReceipt {
    pub fn canonical_json(&self) -> Result<String, CreatureRecordBatchError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| CreatureRecordBatchError::Serialization(error.to_string()))
    }
}

impl CreatureRecordCommitIntent {
    fn from_receipt(
        target_plugin: impl Into<String>,
        schema_id: impl Into<String>,
        recipe_blake3: impl Into<String>,
        receipt: &CreatureRecordBatchReceipt,
        recipe_binding: CreatureRecordRecipeBinding,
    ) -> Result<Self, CreatureRecordCommitLedgerError> {
        validate_receipt_structure(receipt)?;
        let mut intent = Self {
            version: CREATURE_RECORD_COMMIT_LEDGER_VERSION,
            target_plugin: target_plugin.into(),
            schema_id: schema_id.into(),
            recipe_blake3: recipe_blake3.into(),
            family_ids: receipt.family_ids.clone(),
            record_count: receipt.record_count,
            mapping_count: receipt.mapping_count,
            reserved_form_keys: receipt.reserved_form_keys.clone(),
            primary_mappings: receipt
                .families
                .iter()
                .flat_map(|family| family.primary_mappings.clone())
                .collect(),
            races: receipt
                .families
                .iter()
                .flat_map(|family| family.races.clone())
                .collect(),
            recipe_binding,
        };
        intent.normalize();
        intent.validate()?;
        Ok(intent)
    }

    pub fn canonical_json(&self) -> Result<String, CreatureRecordCommitLedgerError> {
        let mut intent = self.clone();
        intent.normalize();
        intent.validate()?;
        serde_json::to_string(&intent)
            .map_err(|error| CreatureRecordCommitLedgerError::Serialization(error.to_string()))
    }

    pub fn stable_hash(&self) -> Result<String, CreatureRecordCommitLedgerError> {
        Ok(blake3::hash(self.canonical_json()?.as_bytes())
            .to_hex()
            .to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, CreatureRecordCommitLedgerError> {
        let intent: Self = serde_json::from_str(json)
            .map_err(|error| CreatureRecordCommitLedgerError::Serialization(error.to_string()))?;
        if intent.canonical_json()? != json {
            return Err(CreatureRecordCommitLedgerError::NonCanonicalJson);
        }
        Ok(intent)
    }

    fn normalize(&mut self) {
        self.family_ids.sort_by_key(|id| id.to_ascii_lowercase());
        self.reserved_form_keys
            .sort_by_key(|key| (key.plugin.to_ascii_lowercase(), key.local));
        self.primary_mappings.sort_by_key(|mapping| {
            (
                mapping.source.stable_key(),
                mapping.target.plugin.to_ascii_lowercase(),
                mapping.target.local,
            )
        });
        self.races.sort_by_key(|race| {
            (
                race.form_key.plugin.to_ascii_lowercase(),
                race.form_key.local,
            )
        });
        normalize_recipe_binding(&mut self.recipe_binding);
        for race in &mut self.races {
            race.attack_events
                .sort_by_key(|event| event.to_ascii_lowercase());
            race.attack_events
                .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        }
    }

    fn validate(&self) -> Result<(), CreatureRecordCommitLedgerError> {
        if self.version != CREATURE_RECORD_COMMIT_LEDGER_VERSION {
            return Err(ledger_invalid(
                "version",
                format!(
                    "expected {}, found {}",
                    CREATURE_RECORD_COMMIT_LEDGER_VERSION, self.version
                ),
            ));
        }
        if self.target_plugin.trim().is_empty() || self.schema_id.trim().is_empty() {
            return Err(ledger_invalid(
                "target",
                "target plugin and schema id are required",
            ));
        }
        if !is_blake3_hex(&self.recipe_blake3) {
            return Err(ledger_invalid(
                "recipe_hash",
                "recipe hash must be lowercase BLAKE3 hex",
            ));
        }
        validate_unique_ci("family_id", &self.family_ids)?;
        if self.family_ids.is_empty() {
            return Err(ledger_invalid(
                "family_id",
                "at least one family is required",
            ));
        }
        if self.mapping_count != self.primary_mappings.len() {
            return Err(ledger_invalid(
                "mapping_count",
                "mapping count does not match primary mappings",
            ));
        }
        validate_recipe_binding(&self.recipe_binding, self.record_count)?;
        let mut reserved = BTreeSet::new();
        for key in &self.reserved_form_keys {
            if !key.plugin.eq_ignore_ascii_case(&self.target_plugin) {
                return Err(ledger_invalid(
                    "reserved_plugin",
                    "reserved key belongs to another target plugin",
                ));
            }
            if !reserved.insert((key.plugin.to_ascii_lowercase(), key.local)) {
                return Err(ledger_invalid(
                    "duplicate_reserved_key",
                    format!("{}@{:06X}", key.plugin, key.local),
                ));
            }
        }
        let mut sources = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for mapping in &self.primary_mappings {
            if !mapping
                .target
                .plugin
                .eq_ignore_ascii_case(&self.target_plugin)
            {
                return Err(ledger_invalid(
                    "mapping_plugin",
                    "primary mapping belongs to another target plugin",
                ));
            }
            if !sources.insert(mapping.source.stable_key()) {
                return Err(ledger_invalid(
                    "duplicate_mapping_source",
                    mapping.source.stable_key(),
                ));
            }
            let target = (
                mapping.target.plugin.to_ascii_lowercase(),
                mapping.target.local,
            );
            if !targets.insert(target.clone()) {
                return Err(ledger_invalid(
                    "mapping_target_collision",
                    format!("{}@{:06X}", target.0, target.1),
                ));
            }
            if !reserved.contains(&target) {
                return Err(ledger_invalid(
                    "unreserved_mapping_target",
                    format!("{}@{:06X}", target.0, target.1),
                ));
            }
        }
        if self.recipe_binding.family_ids != self.family_ids {
            return Err(ledger_invalid(
                "recipe_family",
                "recipe binding families do not match the intent",
            ));
        }
        if self.recipe_binding.primary_mappings != self.primary_mappings {
            return Err(ledger_invalid(
                "recipe_primary_mapping",
                "recipe binding primary mappings do not match the intent",
            ));
        }
        let mut races = BTreeSet::new();
        for race in &self.races {
            let key = (
                race.form_key.plugin.to_ascii_lowercase(),
                race.form_key.local,
            );
            if !race
                .form_key
                .plugin
                .eq_ignore_ascii_case(&self.target_plugin)
                || !reserved.contains(&key)
            {
                return Err(ledger_invalid(
                    "race_target",
                    "RACE evidence is not bound to a reserved target key",
                ));
            }
            if !races.insert(key) {
                return Err(ledger_invalid(
                    "duplicate_race",
                    "duplicate RACE attack evidence",
                ));
            }
            if race
                .attack_events
                .iter()
                .any(|event| event.trim().is_empty())
            {
                return Err(ledger_invalid("race_attack", "RACE attack event is empty"));
            }
        }
        Ok(())
    }
}

impl CreatureRecordCommitLedger {
    fn new(
        intent: CreatureRecordCommitIntent,
        mut receipt: CreatureRecordBatchReceipt,
    ) -> Result<Self, CreatureRecordCommitLedgerError> {
        canonicalize_receipt(&mut receipt);
        validate_receipt_matches_intent(&intent, &receipt)?;
        let intent_blake3 = intent.stable_hash()?;
        let receipt_blake3 = receipt_hash(&receipt)?;
        Ok(Self {
            version: CREATURE_RECORD_COMMIT_LEDGER_VERSION,
            intent,
            intent_blake3,
            receipt,
            receipt_blake3,
        })
    }

    pub fn canonical_json(&self) -> Result<String, CreatureRecordCommitLedgerError> {
        let mut ledger = self.clone();
        ledger.intent.normalize();
        canonicalize_receipt(&mut ledger.receipt);
        ledger.validate()?;
        serde_json::to_string(&ledger)
            .map_err(|error| CreatureRecordCommitLedgerError::Serialization(error.to_string()))
    }

    pub fn stable_hash(&self) -> Result<String, CreatureRecordCommitLedgerError> {
        Ok(blake3::hash(self.canonical_json()?.as_bytes())
            .to_hex()
            .to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, CreatureRecordCommitLedgerError> {
        let ledger: Self = serde_json::from_str(json)
            .map_err(|error| CreatureRecordCommitLedgerError::Serialization(error.to_string()))?;
        ledger.validate()?;
        if ledger.canonical_json()? != json {
            return Err(CreatureRecordCommitLedgerError::NonCanonicalJson);
        }
        Ok(ledger)
    }

    pub fn read(path: &Path) -> Result<Self, CreatureRecordCommitLedgerError> {
        let json = fs::read_to_string(path).map_err(|error| ledger_io(path, error))?;
        Self::from_json(&json)
    }

    pub fn validate_against_recipe(
        &self,
        recipe: &SourceRigExecutableRecipe,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<(), CreatureRecordCommitLedgerError> {
        self.validate()?;
        let actual = recipe
            .stable_hash()
            .map_err(|error| ledger_invalid("recipe", error.to_string()))?;
        if actual != self.intent.recipe_blake3 {
            return Err(CreatureRecordCommitLedgerError::StaleRecipeHash {
                expected: self.intent.recipe_blake3.clone(),
                actual,
            });
        }
        let expected_binding = recipe
            .rebuild_record_family_batch(interner)
            .map_err(|error| ledger_invalid("recipe", error.to_string()))
            .and_then(|family| recipe_binding_from_families(vec![family], schema, interner))?;
        if expected_binding != self.intent.recipe_binding {
            return Err(ledger_invalid(
                "recipe_binding",
                "ledger content commitments do not match the reconstructed recipe batch",
            ));
        }
        Ok(())
    }

    pub fn read_and_validate(
        path: &Path,
        recipe: &SourceRigExecutableRecipe,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<Self, CreatureRecordCommitLedgerError> {
        let ledger = Self::read(path)?;
        ledger.validate_against_recipe(recipe, schema, interner)?;
        Ok(ledger)
    }

    fn validate(&self) -> Result<(), CreatureRecordCommitLedgerError> {
        if self.version != CREATURE_RECORD_COMMIT_LEDGER_VERSION {
            return Err(ledger_invalid(
                "version",
                "unsupported record commit ledger version",
            ));
        }
        self.intent.validate()?;
        if self.intent.version != self.version {
            return Err(ledger_invalid(
                "intent_version",
                "intent and ledger versions differ",
            ));
        }
        if self.intent_blake3 != self.intent.stable_hash()? {
            return Err(ledger_invalid(
                "intent_hash",
                "intent hash does not match canonical intent",
            ));
        }
        if self.receipt_blake3 != receipt_hash(&self.receipt)? {
            return Err(ledger_invalid(
                "receipt_hash",
                "receipt hash does not match canonical receipt",
            ));
        }
        validate_receipt_matches_intent(&self.intent, &self.receipt)
    }
}

impl<'session, 'mapper, 'store> PreparedCreatureRecordCommit<'session, 'mapper, 'store> {
    pub fn receipt(&self) -> &CreatureRecordBatchReceipt {
        self.prepared.receipt()
    }

    pub fn intent(&self) -> &CreatureRecordCommitIntent {
        &self.intent
    }

    pub fn commit(self) -> CommittedCreatureRecordBatch {
        let receipt = self.prepared.commit();
        let ledger = CreatureRecordCommitLedger::new(self.intent, receipt.clone())
            .expect("prepared record commit intent was validated before target commit");
        CommittedCreatureRecordBatch { receipt, ledger }
    }
}

impl CommittedCreatureRecordBatch {
    pub fn receipt(&self) -> &CreatureRecordBatchReceipt {
        &self.receipt
    }

    pub fn ledger(&self) -> &CreatureRecordCommitLedger {
        &self.ledger
    }

    pub fn stage_ledger_to_private_path(
        &self,
        private_root: &Path,
        overwrite_existing: bool,
    ) -> Result<PathBuf, CreatureRecordCommitLedgerError> {
        stage_ledger_to_private_path(private_root, &self.ledger, overwrite_existing).map_err(
            |error| CreatureRecordCommitLedgerError::PostCommitPersistence(Box::new(error)),
        )
    }
}

pub fn prepare_creature_record_commit<'session, 'mapper, 'store>(
    prepared: PreparedCreatureRecordBatch<'session, 'mapper, 'store>,
    target_plugin: impl Into<String>,
    schema_id: impl Into<String>,
    recipe: &SourceRigExecutableRecipe,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<PreparedCreatureRecordCommit<'session, 'mapper, 'store>, CreatureRecordCommitLedgerError>
{
    let recipe_blake3 = recipe
        .stable_hash()
        .map_err(|error| ledger_invalid("recipe", error.to_string()))?;
    let expected_binding = recipe
        .rebuild_record_family_batch(interner)
        .map_err(|error| ledger_invalid("recipe", error.to_string()))
        .and_then(|family| recipe_binding_from_families(vec![family], schema, interner))?;
    if expected_binding != prepared.recipe_binding {
        return Err(ledger_invalid(
            "recipe_batch_mismatch",
            "prepared records do not exactly match the reconstructed recipe batch",
        ));
    }
    let intent = CreatureRecordCommitIntent::from_receipt(
        target_plugin,
        schema_id,
        recipe_blake3,
        prepared.receipt(),
        prepared.recipe_binding.clone(),
    )?;
    Ok(PreparedCreatureRecordCommit { prepared, intent })
}

pub fn prepare_creature_record_commit_batch<'session, 'mapper, 'store>(
    prepared: PreparedCreatureRecordBatch<'session, 'mapper, 'store>,
    target_plugin: impl Into<String>,
    schema_id: impl Into<String>,
    recipes: &[SourceRigExecutableRecipe],
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<PreparedCreatureRecordCommit<'session, 'mapper, 'store>, CreatureRecordCommitLedgerError>
{
    if recipes.is_empty() {
        return Err(ledger_invalid(
            "recipe_batch",
            "at least one executable creature recipe is required",
        ));
    }
    let mut recipe_hashes = recipes
        .iter()
        .map(|recipe| {
            recipe
                .stable_hash()
                .map_err(|error| ledger_invalid("recipe", error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    recipe_hashes.sort();
    if recipe_hashes.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ledger_invalid(
            "recipe_batch",
            "duplicate executable creature recipe",
        ));
    }
    let families = recipes
        .iter()
        .map(|recipe| {
            recipe
                .rebuild_record_family_batch(interner)
                .map_err(|error| ledger_invalid("recipe", error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let families = merge_creature_record_family_batches(families)
        .map_err(|error| ledger_invalid("recipe_batch", error.to_string()))?;
    let expected_binding = recipe_binding_from_families(families, schema, interner)?;
    if expected_binding != prepared.recipe_binding {
        return Err(ledger_invalid(
            "recipe_batch_mismatch",
            "prepared records do not exactly match the reconstructed recipe batch",
        ));
    }
    let recipe_blake3 = blake3::hash(
        serde_json::to_string(&recipe_hashes)
            .map_err(|error| CreatureRecordCommitLedgerError::Serialization(error.to_string()))?
            .as_bytes(),
    )
    .to_hex()
    .to_string();
    let intent = CreatureRecordCommitIntent::from_receipt(
        target_plugin,
        schema_id,
        recipe_blake3,
        prepared.receipt(),
        prepared.recipe_binding.clone(),
    )?;
    Ok(PreparedCreatureRecordCommit { prepared, intent })
}

pub fn stage_creature_record_commit_ledger(
    private_root: &Path,
    committed: &CommittedCreatureRecordBatch,
    overwrite_existing: bool,
) -> Result<PathBuf, CreatureRecordCommitLedgerError> {
    committed.stage_ledger_to_private_path(private_root, overwrite_existing)
}

fn stage_ledger_to_private_path(
    private_root: &Path,
    ledger: &CreatureRecordCommitLedger,
    overwrite_existing: bool,
) -> Result<PathBuf, CreatureRecordCommitLedgerError> {
    let destination = private_root.join(CREATURE_RECORD_COMMIT_LEDGER_RELATIVE_PATH);
    ledger.validate()?;
    if destination.exists() && !overwrite_existing {
        return Err(CreatureRecordCommitLedgerError::DestinationExists(
            destination,
        ));
    }
    let parent = destination
        .parent()
        .expect("relative ledger path has a parent");
    fs::create_dir_all(parent).map_err(|error| ledger_io(parent, error))?;
    let mut staged =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| ledger_io(parent, error))?;
    staged
        .write_all(ledger.canonical_json()?.as_bytes())
        .map_err(|error| ledger_io(staged.path(), error))?;
    staged
        .flush()
        .map_err(|error| ledger_io(staged.path(), error))?;
    if overwrite_existing {
        staged
            .persist(&destination)
            .map_err(|error| ledger_io(&destination, error.error))?;
    } else {
        staged.persist_noclobber(&destination).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                CreatureRecordCommitLedgerError::DestinationExists(destination.clone())
            } else {
                ledger_io(&destination, error.error)
            }
        })?;
    }
    Ok(destination)
}

pub fn read_and_validate_creature_record_commit_ledger(
    path: &Path,
    recipe: &SourceRigExecutableRecipe,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<CreatureRecordCommitLedger, CreatureRecordCommitLedgerError> {
    CreatureRecordCommitLedger::read_and_validate(path, recipe, schema, interner)
}

fn validate_receipt_matches_intent(
    intent: &CreatureRecordCommitIntent,
    receipt: &CreatureRecordBatchReceipt,
) -> Result<(), CreatureRecordCommitLedgerError> {
    validate_receipt_structure(receipt)?;
    let expected = CreatureRecordCommitIntent::from_receipt(
        intent.target_plugin.clone(),
        intent.schema_id.clone(),
        intent.recipe_blake3.clone(),
        receipt,
        intent.recipe_binding.clone(),
    )?;
    if expected.family_ids != intent.family_ids
        || expected.record_count != intent.record_count
        || expected.mapping_count != intent.mapping_count
        || expected.reserved_form_keys != intent.reserved_form_keys
        || expected.primary_mappings != intent.primary_mappings
        || expected.races != intent.races
    {
        return Err(ledger_invalid(
            "receipt_binding",
            "receipt does not match precommit intent",
        ));
    }
    Ok(())
}

fn validate_receipt_structure(
    receipt: &CreatureRecordBatchReceipt,
) -> Result<(), CreatureRecordCommitLedgerError> {
    let family_ids = receipt
        .families
        .iter()
        .map(|family| family.family_id.clone())
        .collect::<Vec<_>>();
    validate_unique_ci("family_id", &family_ids)?;
    if receipt.family_ids.len() != family_ids.len()
        || receipt
            .family_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
            != family_ids
                .iter()
                .map(|id| id.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
    {
        return Err(ledger_invalid(
            "family_ids",
            "receipt family ids do not exactly match family receipts",
        ));
    }
    if receipt.record_count
        != receipt
            .families
            .iter()
            .map(|family| family.record_count)
            .sum::<usize>()
    {
        return Err(ledger_invalid(
            "record_count",
            "receipt record count does not match family receipts",
        ));
    }
    if receipt.mapping_count
        != receipt
            .families
            .iter()
            .map(|family| family.mapping_count)
            .sum::<usize>()
    {
        return Err(ledger_invalid(
            "mapping_count",
            "receipt mapping count does not match family receipts",
        ));
    }
    for family in &receipt.families {
        if family.mapping_count != family.primary_mappings.len() {
            return Err(ledger_invalid(
                "family_mapping_count",
                format!(
                    "family {:?} mapping count does not match mappings",
                    family.family_id
                ),
            ));
        }
    }
    Ok(())
}

fn recipe_binding_from_families(
    families: Vec<CreatureRecordFamilyBatch>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<CreatureRecordRecipeBinding, CreatureRecordCommitLedgerError> {
    let normalizer = TargetRecordNormalizer::target_only_with_interner(schema, interner);
    let mut binding = CreatureRecordRecipeBinding::default();
    for family in families {
        binding.family_ids.push(family.family_id.clone());
        binding
            .primary_mappings
            .extend(family.primary_mappings.iter().cloned());
        for closure in family.closures {
            binding.required_target_records.extend(
                closure
                    .required_target_records
                    .into_iter()
                    .map(|dependency| CreatureTargetDependencyCommitment {
                        family_id: family.family_id.clone(),
                        dependency,
                    }),
            );
            binding
                .projected_identities
                .extend(closure.projected_identities.into_iter().map(|identity| {
                    CreatureProjectedRecordCommitment {
                        family_id: family.family_id.clone(),
                        identity,
                    }
                }));
            for record in closure.closure.records {
                let record = match normalizer.normalize(record) {
                    TargetRecordNormalization::Keep(record) => record,
                    TargetRecordNormalization::DropUnsupportedRecord => {
                        return Err(ledger_invalid(
                            "recipe_record",
                            "recipe emitted a record unsupported by the target schema",
                        ));
                    }
                };
                let plugin = interner.resolve(record.form_key.plugin).ok_or_else(|| {
                    ledger_invalid(
                        "recipe_record",
                        "recipe record has unresolved target plugin",
                    )
                })?;
                binding
                    .encoded_records
                    .push(CreatureEncodedRecordCommitment {
                        form_key: TargetFormKey::new(record.form_key.local, plugin),
                        signature: record.sig.as_str().to_string(),
                        encoded_blake3: normalized_record_hash(&record, interner)
                            .map_err(|error| ledger_invalid("recipe_record", error.to_string()))?,
                    });
            }
        }
    }
    normalize_recipe_binding(&mut binding);
    validate_recipe_binding(&binding, binding.encoded_records.len())?;
    Ok(binding)
}

fn normalize_recipe_binding(binding: &mut CreatureRecordRecipeBinding) {
    binding.family_ids.sort_by_key(|id| id.to_ascii_lowercase());
    binding.projected_identities.sort_by_key(|entry| {
        (
            entry.family_id.to_ascii_lowercase(),
            entry.identity.signature.clone(),
            entry.identity.source_identity.stable_key(),
            entry.identity.target_form_key.plugin.to_ascii_lowercase(),
            entry.identity.target_form_key.local,
        )
    });
    binding.required_target_records.sort_by_key(|entry| {
        (
            entry.family_id.to_ascii_lowercase(),
            entry.dependency.signature.clone(),
            entry.dependency.form_key.plugin.to_ascii_lowercase(),
            entry.dependency.form_key.local,
        )
    });
    binding.primary_mappings.sort_by_key(|mapping| {
        (
            mapping.source.stable_key(),
            mapping.target.plugin.to_ascii_lowercase(),
            mapping.target.local,
        )
    });
    binding.encoded_records.sort_by_key(|entry| {
        (
            entry.form_key.plugin.to_ascii_lowercase(),
            entry.form_key.local,
            entry.signature.clone(),
        )
    });
}

fn validate_recipe_binding(
    binding: &CreatureRecordRecipeBinding,
    record_count: usize,
) -> Result<(), CreatureRecordCommitLedgerError> {
    validate_unique_ci("recipe_family", &binding.family_ids)?;
    if binding.encoded_records.len() != record_count {
        return Err(ledger_invalid(
            "recipe_record_count",
            "recipe binding record count differs from the receipt",
        ));
    }
    let mut mapping_sources = BTreeSet::new();
    let mut mapping_targets = BTreeSet::new();
    for mapping in &binding.primary_mappings {
        if !mapping_sources.insert(mapping.source.stable_key())
            || !mapping_targets.insert((
                mapping.target.plugin.to_ascii_lowercase(),
                mapping.target.local,
            ))
        {
            return Err(ledger_invalid(
                "recipe_primary_mapping",
                "duplicate primary mapping source or target",
            ));
        }
    }
    let mut record_keys = BTreeSet::new();
    for record in &binding.encoded_records {
        if !is_blake3_hex(&record.encoded_blake3)
            || !record_keys.insert((
                record.form_key.plugin.to_ascii_lowercase(),
                record.form_key.local,
                record.signature.clone(),
            ))
        {
            return Err(ledger_invalid(
                "recipe_record_hash",
                "duplicate record key or invalid encoded record hash",
            ));
        }
    }
    Ok(())
}

fn normalized_record_hash(
    record: &Record,
    interner: &StringInterner,
) -> Result<String, CreatureRecordBatchError> {
    let form_key = |key: FormKey| {
        Ok(serde_json::json!({
            "local": key.local,
            "plugin": interner.resolve(key.plugin).ok_or_else(|| CreatureRecordBatchError::Serialization("unresolved FormKey plugin".to_string()))?,
        }))
    };
    fn value_json(
        value: &FieldValue,
        interner: &StringInterner,
    ) -> Result<serde_json::Value, CreatureRecordBatchError> {
        match value {
            FieldValue::None => Ok(serde_json::Value::Null),
            FieldValue::Bool(value) => Ok(serde_json::json!(["bool", value])),
            FieldValue::Int(value) => Ok(serde_json::json!(["int", value])),
            FieldValue::Uint(value) => Ok(serde_json::json!(["uint", value])),
            FieldValue::Float(value) => Ok(serde_json::json!(["float", value])),
            FieldValue::String(value) => Ok(serde_json::json!([
                "string",
                interner.resolve(*value).unwrap_or_default()
            ])),
            FieldValue::Bytes(value) => Ok(serde_json::json!(["bytes", value.as_slice()])),
            FieldValue::FormKey(value) => Ok(serde_json::json!([
                "form",
                value.local,
                interner.resolve(value.plugin).unwrap_or_default()
            ])),
            FieldValue::List(values) => values
                .iter()
                .map(|value| value_json(value, interner))
                .collect::<Result<Vec<_>, _>>()
                .map(|values| serde_json::json!(["list", values])),
            FieldValue::Struct(values) => values
                .iter()
                .map(|(name, value)| {
                    Ok(serde_json::json!([
                        interner.resolve(*name).unwrap_or_default(),
                        value_json(value, interner)?
                    ]))
                })
                .collect::<Result<Vec<_>, CreatureRecordBatchError>>()
                .map(|values| serde_json::json!(["struct", values])),
        }
    }
    let fields = record
        .fields
        .iter()
        .map(|field| {
            Ok(serde_json::json!([
                field.sig.as_str(),
                value_json(&field.value, interner)?
            ]))
        })
        .collect::<Result<Vec<_>, CreatureRecordBatchError>>()?;
    let json = serde_json::json!({
        "signature": record.sig.as_str(),
        "form_key": form_key(record.form_key)?,
        "editor_id": record.eid.and_then(|id| interner.resolve(id)),
        "flags": record.flags.bits(),
        "fields": fields,
    });
    let bytes = serde_json::to_vec(&json)
        .map_err(|error| CreatureRecordBatchError::Serialization(error.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn receipt_hash(
    receipt: &CreatureRecordBatchReceipt,
) -> Result<String, CreatureRecordCommitLedgerError> {
    let mut canonical = receipt.clone();
    canonicalize_receipt(&mut canonical);
    let json = serde_json::to_string(&canonical)
        .map_err(|error| CreatureRecordCommitLedgerError::Serialization(error.to_string()))?;
    Ok(blake3::hash(json.as_bytes()).to_hex().to_string())
}

fn canonicalize_receipt(receipt: &mut CreatureRecordBatchReceipt) {
    receipt.family_ids.sort_by_key(|id| id.to_ascii_lowercase());
    receipt
        .reserved_form_keys
        .sort_by_key(|key| (key.plugin.to_ascii_lowercase(), key.local));
    receipt
        .families
        .sort_by_key(|family| family.family_id.to_ascii_lowercase());
    for family in &mut receipt.families {
        family
            .primary_mappings
            .sort_by_key(|mapping| mapping.source.stable_key());
        family.races.sort_by_key(|race| {
            (
                race.form_key.plugin.to_ascii_lowercase(),
                race.form_key.local,
            )
        });
        for race in &mut family.races {
            race.attack_events
                .sort_by_key(|event| event.to_ascii_lowercase());
        }
    }
}

fn validate_unique_ci(
    label: &'static str,
    values: &[String],
) -> Result<(), CreatureRecordCommitLedgerError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() || !seen.insert(value.to_ascii_lowercase()) {
            return Err(ledger_invalid(
                label,
                format!("duplicate or empty value {value:?}"),
            ));
        }
    }
    Ok(())
}

fn is_blake3_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn ledger_invalid(
    code: &'static str,
    message: impl Into<String>,
) -> CreatureRecordCommitLedgerError {
    CreatureRecordCommitLedgerError::Invalid {
        code,
        message: message.into(),
    }
}

fn ledger_io(path: &Path, error: std::io::Error) -> CreatureRecordCommitLedgerError {
    CreatureRecordCommitLedgerError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// Legacy unbound publication API. New recipe-backed callers must use
/// `prepare_creature_record_commit` and persist the returned post-commit ledger.
pub fn publish_creature_record_family_batch(
    session: &mut PluginSession<'_>,
    mapper_state: &mut MapperState,
    family: CreatureRecordFamilyBatch,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<CreatureRecordBatchReceipt, CreatureRecordBatchError> {
    publish_creature_record_families_batch(session, mapper_state, vec![family], schema, interner)
}

/// Legacy unbound publication API. New recipe-backed callers must use
/// `prepare_creature_record_commit` and persist the returned post-commit ledger.
pub fn publish_creature_record_families_batch(
    session: &mut PluginSession<'_>,
    mapper_state: &mut MapperState,
    families: Vec<CreatureRecordFamilyBatch>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<CreatureRecordBatchReceipt, CreatureRecordBatchError> {
    Ok(
        prepare_creature_record_families_batch(session, mapper_state, families, schema, interner)?
            .commit(),
    )
}

pub fn prepare_creature_record_families_batch<'session, 'mapper, 'store>(
    session: &'session mut PluginSession<'store>,
    mapper_state: &'mapper mut MapperState,
    families: Vec<CreatureRecordFamilyBatch>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<PreparedCreatureRecordBatch<'session, 'mapper, 'store>, CreatureRecordBatchError> {
    prepare_creature_record_families_batch_with_failure(
        session,
        mapper_state,
        families,
        schema,
        interner,
        CreatureRecordBatchFailureInjection::None,
    )
}

pub fn merge_creature_record_family_batches(
    families: Vec<CreatureRecordFamilyBatch>,
) -> Result<Vec<CreatureRecordFamilyBatch>, CreatureRecordBatchError> {
    if families.is_empty() {
        return Err(invalid("empty_batch", "no creature families were supplied"));
    }
    let mut merged = BTreeMap::<String, CreatureRecordFamilyBatch>::new();
    for family in families {
        if family.family_id.trim().is_empty() {
            return Err(invalid("family_id", "family id is empty"));
        }
        let key = family.family_id.to_ascii_lowercase();
        if let Some(existing) = merged.get_mut(&key) {
            existing.closures.extend(family.closures);
            existing.primary_mappings.extend(family.primary_mappings);
        } else {
            merged.insert(key, family);
        }
    }
    Ok(merged.into_values().collect())
}

pub fn publish_creature_record_families_batch_for_target(
    target_handle_id: u64,
    mapper_state: &mut MapperState,
    families: Vec<CreatureRecordFamilyBatch>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<CreatureRecordBatchReceipt, CreatureRecordBatchError> {
    let mut session = open_session(target_handle_id, None)
        .map_err(|error| invalid("target_session", error.to_string()))?;
    publish_creature_record_families_batch(&mut session, mapper_state, families, schema, interner)
}

pub(crate) fn publish_creature_record_families_batch_with_failure(
    session: &mut PluginSession<'_>,
    mapper_state: &mut MapperState,
    families: Vec<CreatureRecordFamilyBatch>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    failure: CreatureRecordBatchFailureInjection,
) -> Result<CreatureRecordBatchReceipt, CreatureRecordBatchError> {
    Ok(prepare_creature_record_families_batch_with_failure(
        session,
        mapper_state,
        families,
        schema,
        interner,
        failure,
    )?
    .commit())
}

fn prepare_creature_record_families_batch_with_failure<'session, 'mapper, 'store>(
    session: &'session mut PluginSession<'store>,
    mapper_state: &'mapper mut MapperState,
    mut families: Vec<CreatureRecordFamilyBatch>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    failure: CreatureRecordBatchFailureInjection,
) -> Result<PreparedCreatureRecordBatch<'session, 'mapper, 'store>, CreatureRecordBatchError> {
    if families.is_empty() {
        return Err(invalid("empty_batch", "no creature families were supplied"));
    }
    families.sort_by_key(|family| family.family_id.to_ascii_lowercase());
    validate_family_ids(&families)?;

    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    if mapper_state
        .options
        .output_plugin_name
        .eq_ignore_ascii_case(&target_plugin)
        == false
    {
        return Err(invalid(
            "mapper_output_plugin",
            format!(
                "mapper output {:?} does not match target plugin {:?}",
                mapper_state.options.output_plugin_name, target_plugin
            ),
        ));
    }

    let target_id = session.target_id();
    let existing = session
        .own_record_signatures_in_handle(target_id)
        .map_err(|error| invalid("target_index", error.to_string()))?;
    let existing_ids = existing.keys().copied().collect::<BTreeSet<_>>();
    let master_names = session
        .target_masters()
        .iter()
        .map(|master| master.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();

    let mut record_owners = BTreeMap::<u32, (String, String)>::new();
    let mut planned_records = Vec::<(String, Record)>::new();
    let mut planned_mappings = Vec::<(String, CreaturePrimaryRecordMapping)>::new();
    let mut planned_dependencies = Vec::new();
    let mut planned_identities = Vec::new();
    let mut mapping_owners = BTreeMap::<String, String>::new();

    for family in families {
        validate_family_contents(
            &family,
            &target_plugin,
            interner,
            &mut record_owners,
            &mut mapping_owners,
        )?;
        for closure in family.closures {
            for dependency in closure.required_target_records {
                planned_dependencies.push((family.family_id.clone(), dependency));
            }
            for identity in closure.projected_identities {
                planned_identities.push((family.family_id.clone(), identity));
            }
            for record in closure.closure.records {
                planned_records.push((family.family_id.clone(), record));
            }
        }
        for mapping in family.primary_mappings {
            planned_mappings.push((family.family_id.clone(), mapping));
        }
    }

    planned_records.sort_by_key(|(family_id, record)| {
        (
            family_id.to_ascii_lowercase(),
            record.form_key.local,
            record.sig.as_str().to_string(),
        )
    });
    planned_mappings.sort_by_key(|(family_id, mapping)| {
        (
            family_id.to_ascii_lowercase(),
            mapping.source.stable_key(),
            mapping.target.local,
        )
    });
    planned_dependencies.sort_by_key(|(family_id, dependency)| {
        (
            family_id.to_ascii_lowercase(),
            dependency.form_key.plugin.to_ascii_lowercase(),
            dependency.form_key.local,
            dependency.signature.clone(),
        )
    });
    planned_identities.sort_by_key(|(family_id, identity)| {
        (
            family_id.to_ascii_lowercase(),
            identity.signature.clone(),
            identity.source_identity.stable_key(),
            identity.target_form_key.plugin.to_ascii_lowercase(),
            identity.target_form_key.local,
        )
    });

    validate_failure_index(failure, planned_records.len(), planned_mappings.len())?;
    for (local, (_, signature)) in &record_owners {
        if let Some(existing_signature) = existing.get(local) {
            let replaces_leased_support = mapper_state.leased_object_ids.contains(local)
                && existing_signature.as_str() == signature
                && matches!(signature.as_str(), "ARMO" | "ARMA" | "BPTD");
            if !replaces_leased_support {
                return Err(CreatureRecordBatchError::ExistingTargetCollision {
                    local: *local,
                    plugin: target_plugin.clone(),
                    existing_signature: existing_signature.as_str().to_string(),
                });
            }
        }
        if mapper_state.used_object_ids.contains(local) {
            return Err(CreatureRecordBatchError::ReservedTargetCollision {
                local: *local,
                plugin: target_plugin.clone(),
            });
        }
        debug_assert!(!signature.is_empty());
    }
    validate_target_dependencies(
        &planned_dependencies,
        &target_plugin,
        &record_owners,
        &existing,
        &master_names,
    )?;

    let normalizer = TargetRecordNormalizer::target_only_with_interner(schema, interner);
    let mut normalized_records = Vec::with_capacity(planned_records.len());
    let mut encoded_records = Vec::with_capacity(planned_records.len());
    let mut record_labels = Vec::with_capacity(planned_records.len());
    let mut family_receipts = mapping_owners
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|family_id| {
            (
                family_id.clone(),
                CreatureRecordFamilyReceipt {
                    family_id,
                    record_count: 0,
                    mapping_count: 0,
                    primary_mappings: Vec::new(),
                    races: Vec::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (index, (family_id, record)) in planned_records.into_iter().enumerate() {
        fail_if_requested(failure, CreatureRecordBatchStage::EncodeRecord, index)?;
        let label = format!(
            "{} {:06X}@{}",
            record.sig.as_str(),
            record.form_key.local,
            target_plugin
        );
        audit_record_references(
            &record,
            &label,
            &target_plugin,
            &record_owners,
            &existing_ids,
            &master_names,
            interner,
        )?;
        let mut fields_before = record
            .fields
            .iter()
            .map(|field| field.sig.as_str().to_string())
            .collect::<Vec<_>>();
        fields_before.sort();
        let record = match normalizer.normalize(record) {
            TargetRecordNormalization::Keep(record) => record,
            TargetRecordNormalization::DropUnsupportedRecord => {
                return Err(CreatureRecordBatchError::Encode {
                    record_index: index,
                    record: label,
                    message: "record is unsupported by the FO4 target schema".to_string(),
                });
            }
        };
        let mut fields_after = record
            .fields
            .iter()
            .map(|field| field.sig.as_str().to_string())
            .collect::<Vec<_>>();
        fields_after.sort();
        if fields_before != fields_after {
            return Err(CreatureRecordBatchError::Encode {
                record_index: index,
                record: label,
                message: format!(
                    "target normalization changed the record field closure from {fields_before:?} to {fields_after:?}"
                ),
            });
        }
        audit_record_references(
            &record,
            &label,
            &target_plugin,
            &record_owners,
            &existing_ids,
            &master_names,
            interner,
        )?;
        let family_receipt = family_receipts
            .get_mut(&family_id)
            .expect("validated family has a primary mapping");
        family_receipt.record_count += 1;
        if record.sig.as_str() == "RACE" {
            family_receipt
                .races
                .push(race_receipt(&record, &target_plugin, interner));
        }
        encoded_records.push(CreatureEncodedRecordCommitment {
            form_key: TargetFormKey::new(record.form_key.local, target_plugin.clone()),
            signature: record.sig.as_str().to_string(),
            encoded_blake3: normalized_record_hash(&record, interner)?,
        });
        record_labels.push(label);
        normalized_records.push(record);
    }

    let prepared =
        prepare_record_batch_in_slot(session.target_slot(), normalized_records, schema, interner)
            .map_err(|error| CreatureRecordBatchError::Encode {
            record_index: error.record_index,
            record: record_labels
                .get(error.record_index)
                .cloned()
                .unwrap_or_else(|| "unknown record".to_string()),
            message: error.source.to_string(),
        })?;

    for index in 0..prepared.len() {
        fail_if_requested(failure, CreatureRecordBatchStage::AddRecord, index)?;
    }

    let mut next_mapper_state = mapper_state.clone();
    for local in record_owners.keys() {
        next_mapper_state
            .reserved_generated_object_ids
            .remove(local);
        next_mapper_state.leased_object_ids.remove(local);
        next_mapper_state.used_object_ids.insert(*local);
    }
    {
        let mut mapper = FormKeyMapper::from_state(&mut next_mapper_state, interner);
        for (index, (family_id, mapping)) in planned_mappings.iter().enumerate() {
            fail_if_requested(failure, CreatureRecordBatchStage::AddMapping, index)?;
            let source = source_form_key(&mapping.source, interner);
            let target = target_form_key(&mapping.target, interner);
            match mapper_state.source_to_target.get(&source) {
                Some(existing) if *existing == target => {}
                Some(existing)
                    if is_skyrim_race_identity_primary_mapping(
                        mapper_state,
                        &mapping.source,
                        *existing,
                        target,
                        family_id,
                        &record_owners,
                    ) => {}
                Some(_) => {
                    return Err(CreatureRecordBatchError::ExistingSourceMapping {
                        source_identity: mapping.source.stable_key(),
                    });
                }
                None => mapper.add_mapping(source, target),
            }
            let family_receipt = family_receipts
                .get_mut(family_id)
                .expect("validated mapping belongs to a family");
            family_receipt.mapping_count += 1;
            family_receipt.primary_mappings.push(mapping.clone());
        }
    }

    let family_ids = family_receipts.keys().cloned().collect::<Vec<_>>();
    let families = family_receipts.into_values().collect();
    let reserved_form_keys = record_owners
        .keys()
        .map(|local| TargetFormKey::new(*local, target_plugin.clone()))
        .collect();
    let receipt = CreatureRecordBatchReceipt {
        family_ids,
        families,
        record_count: prepared.len(),
        mapping_count: planned_mappings.len(),
        reserved_form_keys,
    };
    let mut recipe_binding = CreatureRecordRecipeBinding {
        family_ids: receipt.family_ids.clone(),
        projected_identities: planned_identities
            .into_iter()
            .map(|(family_id, identity)| CreatureProjectedRecordCommitment {
                family_id,
                identity,
            })
            .collect(),
        required_target_records: planned_dependencies
            .into_iter()
            .map(
                |(family_id, dependency)| CreatureTargetDependencyCommitment {
                    family_id,
                    dependency,
                },
            )
            .collect(),
        primary_mappings: planned_mappings
            .iter()
            .map(|(_, mapping)| mapping.clone())
            .collect(),
        encoded_records,
    };
    normalize_recipe_binding(&mut recipe_binding);
    validate_recipe_binding(&recipe_binding, receipt.record_count)
        .map_err(|error| invalid("recipe_binding", error.to_string()))?;
    Ok(PreparedCreatureRecordBatch {
        session,
        mapper_state,
        records: prepared,
        next_mapper_state,
        receipt,
        recipe_binding,
    })
}

fn is_skyrim_race_identity_primary_mapping(
    mapper_state: &MapperState,
    source: &SourceCreatureIdentity,
    existing_target: FormKey,
    primary_target: FormKey,
    family_id: &str,
    record_owners: &BTreeMap<u32, (String, String)>,
) -> bool {
    if !source.namespace.eq_ignore_ascii_case("skyrimse")
        || existing_target.plugin != primary_target.plugin
        || !mapper_state
            .leased_object_ids
            .contains(&existing_target.local)
        || !mapper_state
            .leased_object_ids
            .contains(&primary_target.local)
    {
        return false;
    }

    matches!(
        (
            record_owners.get(&existing_target.local),
            record_owners.get(&primary_target.local),
        ),
        (Some((race_owner, race_signature)), Some((npc_owner, npc_signature)))
            if race_owner == family_id
                && npc_owner == family_id
                && race_signature == "RACE"
                && npc_signature == "NPC_"
    )
}

fn race_receipt(
    record: &Record,
    target_plugin: &str,
    interner: &StringInterner,
) -> CreatureRaceRecordReceipt {
    let mut attack_events = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "ATKE")
        .filter_map(|field| match &field.value {
            FieldValue::String(value) => interner.resolve(*value).map(str::to_string),
            _ => None,
        })
        .collect::<Vec<_>>();
    attack_events.sort_by_key(|event| event.to_ascii_lowercase());
    attack_events.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    CreatureRaceRecordReceipt {
        form_key: TargetFormKey::new(record.form_key.local, target_plugin),
        attack_events,
        attack_data_entries: record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "ATKD")
            .count(),
    }
}

fn validate_family_ids(
    families: &[CreatureRecordFamilyBatch],
) -> Result<(), CreatureRecordBatchError> {
    let mut seen = BTreeSet::new();
    for family in families {
        if family.family_id.trim().is_empty() {
            return Err(invalid("family_id", "family id is empty"));
        }
        if !seen.insert(family.family_id.to_ascii_lowercase()) {
            return Err(CreatureRecordBatchError::DuplicateFamily {
                family_id: family.family_id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_target_dependencies(
    dependencies: &[(String, CreatureTargetRecordReference)],
    target_plugin: &str,
    planned: &BTreeMap<u32, (String, String)>,
    existing: &rustc_hash::FxHashMap<u32, crate::ids::SigCode>,
    master_names: &BTreeSet<String>,
) -> Result<(), CreatureRecordBatchError> {
    for (family_id, dependency) in dependencies {
        let plugin = &dependency.form_key.plugin;
        if plugin.eq_ignore_ascii_case(target_plugin) {
            let actual_signature = planned
                .get(&dependency.form_key.local)
                .map(|(_, signature)| signature.as_str())
                .or_else(|| {
                    existing
                        .get(&dependency.form_key.local)
                        .map(crate::ids::SigCode::as_str)
                });
            let Some(actual_signature) = actual_signature else {
                return Err(CreatureRecordBatchError::MissingTargetDependency {
                    family_id: family_id.clone(),
                    signature: dependency.signature.clone(),
                    local: dependency.form_key.local,
                    plugin: plugin.clone(),
                });
            };
            if actual_signature != dependency.signature {
                return Err(
                    CreatureRecordBatchError::TargetDependencySignatureMismatch {
                        family_id: family_id.clone(),
                        expected_signature: dependency.signature.clone(),
                        actual_signature: actual_signature.to_string(),
                        local: dependency.form_key.local,
                        plugin: plugin.clone(),
                    },
                );
            }
        } else if !master_names.contains(&plugin.to_ascii_lowercase()) {
            return Err(CreatureRecordBatchError::UnlistedReferencePlugin {
                record: format!("creature family {family_id:?} target dependency"),
                plugin: plugin.clone(),
            });
        }
    }
    Ok(())
}

fn validate_family_contents(
    family: &CreatureRecordFamilyBatch,
    target_plugin: &str,
    interner: &StringInterner,
    record_owners: &mut BTreeMap<u32, (String, String)>,
    mapping_owners: &mut BTreeMap<String, String>,
) -> Result<(), CreatureRecordBatchError> {
    if family.closures.is_empty() {
        return Err(invalid(
            "empty_family",
            format!("family {:?} has no record closures", family.family_id),
        ));
    }

    let mut expected_primary = BTreeMap::<String, TargetFormKey>::new();
    for closure in &family.closures {
        if !closure
            .closure
            .target_plugin
            .eq_ignore_ascii_case(target_plugin)
        {
            return Err(invalid(
                "closure_target_plugin",
                format!(
                    "family {:?} closure targets {:?}, expected {:?}",
                    family.family_id, closure.closure.target_plugin, target_plugin
                ),
            ));
        }
        if !closure.source_primary_identity.is_valid() {
            return Err(invalid(
                "source_primary_identity",
                format!(
                    "family {:?} has invalid primary source identity {:?}",
                    family.family_id, closure.source_primary_identity
                ),
            ));
        }

        let primary = closure
            .projected_identities
            .iter()
            .filter(|identity| identity.primary)
            .collect::<Vec<_>>();
        if primary.len() != 1 || primary[0].source_identity != closure.source_primary_identity {
            return Err(invalid(
                "projected_primary_identity",
                format!(
                    "family {:?} closure must have exactly one projected primary matching {}",
                    family.family_id,
                    closure.source_primary_identity.stable_key()
                ),
            ));
        }
        let primary = primary[0];
        validate_target_key(&primary.target_form_key, target_plugin, "projected primary")?;
        if expected_primary
            .insert(
                closure.source_primary_identity.stable_key(),
                primary.target_form_key.clone(),
            )
            .is_some()
        {
            return Err(CreatureRecordBatchError::DuplicateSourceMapping {
                family_id: family.family_id.clone(),
                source_identity: closure.source_primary_identity.stable_key(),
            });
        }

        let mut closure_records = BTreeMap::<u32, String>::new();
        for record in &closure.closure.records {
            validate_record_key(record, target_plugin, interner)?;
            let local = record.form_key.local;
            if let Some((owner, _)) = record_owners.get(&local) {
                return if owner == &family.family_id {
                    Err(CreatureRecordBatchError::DuplicateRecordFormKey {
                        family_id: family.family_id.clone(),
                        local,
                        plugin: target_plugin.to_string(),
                    })
                } else {
                    Err(CreatureRecordBatchError::CrossFamilyRecordCollision {
                        first_family: owner.clone(),
                        second_family: family.family_id.clone(),
                        local,
                        plugin: target_plugin.to_string(),
                    })
                };
            }
            record_owners.insert(
                local,
                (family.family_id.clone(), record.sig.as_str().to_string()),
            );
            closure_records.insert(local, record.sig.as_str().to_string());
        }

        for identity in &closure.projected_identities {
            validate_target_key(
                &identity.target_form_key,
                target_plugin,
                "projected identity",
            )?;
            match closure_records.get(&identity.target_form_key.local) {
                Some(signature) if signature == &identity.signature => {}
                _ => {
                    return Err(invalid(
                        "projected_identity_record",
                        format!(
                            "family {:?} projected {} {:06X} has no matching record",
                            family.family_id, identity.signature, identity.target_form_key.local
                        ),
                    ));
                }
            }
        }
    }

    if family.primary_mappings.len() != expected_primary.len() {
        return Err(invalid(
            "primary_mapping_count",
            format!(
                "family {:?} has {} primary mappings for {} closures",
                family.family_id,
                family.primary_mappings.len(),
                expected_primary.len()
            ),
        ));
    }
    let mut family_sources = BTreeSet::new();
    let mut family_targets = BTreeSet::new();
    for mapping in &family.primary_mappings {
        let source_key = mapping.source.stable_key();
        validate_target_key(&mapping.target, target_plugin, "primary mapping")?;
        if !family_sources.insert(source_key.clone()) {
            return Err(CreatureRecordBatchError::DuplicateSourceMapping {
                family_id: family.family_id.clone(),
                source_identity: source_key,
            });
        }
        if !family_targets.insert(mapping.target.local) {
            return Err(invalid(
                "duplicate_mapping_target",
                format!(
                    "family {:?} maps more than one primary to {:06X}@{}",
                    family.family_id, mapping.target.local, target_plugin
                ),
            ));
        }
        match expected_primary.get(&source_key) {
            Some(expected) if target_keys_equal(expected, &mapping.target) => {}
            _ => {
                return Err(invalid(
                    "primary_mapping_mismatch",
                    format!(
                        "family {:?} mapping {} -> {:06X}@{} does not match its closure",
                        family.family_id, source_key, mapping.target.local, mapping.target.plugin
                    ),
                ));
            }
        }
        let mapper_source_key = mapper_source_identity_key(&mapping.source);
        if let Some(owner) = mapping_owners.get(&mapper_source_key) {
            return Err(CreatureRecordBatchError::CrossFamilyMappingCollision {
                first_family: owner.clone(),
                second_family: family.family_id.clone(),
                source_identity: source_key,
            });
        }
        mapping_owners.insert(mapper_source_key, family.family_id.clone());
    }
    Ok(())
}

fn validate_record_key(
    record: &Record,
    target_plugin: &str,
    interner: &StringInterner,
) -> Result<(), CreatureRecordBatchError> {
    if record.form_key.local == 0 || record.form_key.local > MAX_LOCAL_FORM_ID {
        return Err(invalid(
            "record_form_key",
            format!(
                "{} record has invalid local FormID {:08X}",
                record.sig.as_str(),
                record.form_key.local
            ),
        ));
    }
    let plugin = interner
        .resolve(record.form_key.plugin)
        .ok_or_else(|| invalid("record_form_key", "record plugin symbol is unresolved"))?;
    if !plugin.eq_ignore_ascii_case(target_plugin) {
        return Err(invalid(
            "record_form_key",
            format!(
                "{} record is owned by {:?}, expected {:?}",
                record.sig.as_str(),
                plugin,
                target_plugin
            ),
        ));
    }
    Ok(())
}

fn validate_target_key(
    target: &TargetFormKey,
    target_plugin: &str,
    label: &str,
) -> Result<(), CreatureRecordBatchError> {
    if target.local == 0
        || target.local > MAX_LOCAL_FORM_ID
        || !target.plugin.eq_ignore_ascii_case(target_plugin)
    {
        return Err(invalid(
            "target_form_key",
            format!(
                "{label} has invalid target {:08X}@{:?}; expected a nonzero local in {:?}",
                target.local, target.plugin, target_plugin
            ),
        ));
    }
    Ok(())
}

fn audit_record_references(
    record: &Record,
    record_label: &str,
    target_plugin: &str,
    record_owners: &BTreeMap<u32, (String, String)>,
    existing_ids: &BTreeSet<u32>,
    master_names: &BTreeSet<String>,
    interner: &StringInterner,
) -> Result<(), CreatureRecordBatchError> {
    let mut references = Vec::new();
    for field in &record.fields {
        collect_form_keys(&field.value, &mut references);
    }
    for reference in references {
        if reference.local == 0 {
            continue;
        }
        let plugin = interner.resolve(reference.plugin).ok_or_else(|| {
            invalid(
                "reference_plugin",
                format!("record {record_label} contains an unresolved plugin symbol"),
            )
        })?;
        if plugin.eq_ignore_ascii_case(target_plugin) {
            if !record_owners.contains_key(&reference.local)
                && !existing_ids.contains(&reference.local)
            {
                return Err(CreatureRecordBatchError::DanglingHelperReference {
                    record: record_label.to_string(),
                    local: reference.local,
                    plugin: target_plugin.to_string(),
                });
            }
        } else if !master_names.contains(&plugin.to_ascii_lowercase()) {
            return Err(CreatureRecordBatchError::UnlistedReferencePlugin {
                record: record_label.to_string(),
                plugin: plugin.to_string(),
            });
        }
    }
    Ok(())
}

fn collect_form_keys(value: &FieldValue, out: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(form_key) => out.push(*form_key),
        FieldValue::List(values) => {
            for value in values {
                collect_form_keys(value, out);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_form_keys(value, out);
            }
        }
        _ => {}
    }
}

fn validate_failure_index(
    failure: CreatureRecordBatchFailureInjection,
    record_count: usize,
    mapping_count: usize,
) -> Result<(), CreatureRecordBatchError> {
    let CreatureRecordBatchFailureInjection::At { stage, index } = failure else {
        return Ok(());
    };
    let count = match stage {
        CreatureRecordBatchStage::EncodeRecord | CreatureRecordBatchStage::AddRecord => {
            record_count
        }
        CreatureRecordBatchStage::AddMapping => mapping_count,
    };
    if index >= count {
        return Err(invalid(
            "failure_injection_index",
            format!("{stage:?} index {index} is outside batch length {count}"),
        ));
    }
    Ok(())
}

fn fail_if_requested(
    failure: CreatureRecordBatchFailureInjection,
    stage: CreatureRecordBatchStage,
    index: usize,
) -> Result<(), CreatureRecordBatchError> {
    if failure == (CreatureRecordBatchFailureInjection::At { stage, index }) {
        return Err(CreatureRecordBatchError::InjectedFailure { stage, index });
    }
    Ok(())
}

fn source_form_key(source: &SourceCreatureIdentity, interner: &StringInterner) -> FormKey {
    FormKey {
        local: source.local_form_id,
        plugin: interner.intern(&source.plugin),
    }
}

fn mapper_source_identity_key(source: &SourceCreatureIdentity) -> String {
    format!(
        "{}|{:08x}",
        source.plugin.to_ascii_lowercase(),
        source.local_form_id
    )
}

fn target_form_key(target: &TargetFormKey, interner: &StringInterner) -> FormKey {
    FormKey {
        local: target.local,
        plugin: interner.intern(&target.plugin),
    }
}

fn target_keys_equal(left: &TargetFormKey, right: &TargetFormKey) -> bool {
    left.local == right.local && left.plugin.eq_ignore_ascii_case(&right.plugin)
}

fn invalid(code: &'static str, message: impl Into<String>) -> CreatureRecordBatchError {
    CreatureRecordBatchError::InvalidBatch {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, RecordFlags};
    use crate::session::open_session;
    use crate::sym::Sym;
    use esp_authoring_core::plugin_runtime::{
        LocalizedStringsState, ParsedItem, plugin_handle_new_native, plugin_handle_store_ref,
    };
    use smallvec::{SmallVec, smallvec};

    fn source(local: u32) -> SourceCreatureIdentity {
        SourceCreatureIdentity {
            namespace: "test".to_string(),
            plugin: "SourceCreatures.esm".to_string(),
            local_form_id: local,
        }
    }

    fn form_key(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
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
        fields: SmallVec<[FieldEntry; 8]>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: SigCode::from_str(signature).unwrap(),
            form_key: form_key(local, plugin, interner),
            eid: Some(interner.intern(editor_id)),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::<[Sym; 2]>::new(),
        }
    }

    fn family(
        family_id: &str,
        plugin: &str,
        base_local: u32,
        source_local: u32,
        interner: &StringInterner,
    ) -> CreatureRecordFamilyBatch {
        let source = source(source_local);
        let primary_target = TargetFormKey::new(base_local, plugin);
        let npc = record(
            "NPC_",
            base_local,
            plugin,
            &format!("BatchNpc{base_local:06X}"),
            smallvec![
                field(
                    "EDID",
                    FieldValue::String(interner.intern(&format!("BatchNpc{base_local:06X}")))
                ),
                field(
                    "FULL",
                    FieldValue::String(interner.intern("Transactional Creature"))
                ),
                field(
                    "WNAM",
                    FieldValue::FormKey(form_key(base_local + 1, plugin, interner))
                ),
            ],
            interner,
        );
        let armor = record(
            "ARMO",
            base_local + 1,
            plugin,
            &format!("BatchSkin{base_local:06X}"),
            smallvec![field(
                "EDID",
                FieldValue::String(interner.intern(&format!("BatchSkin{base_local:06X}")))
            )],
            interner,
        );
        CreatureRecordFamilyBatch {
            family_id: family_id.to_string(),
            closures: vec![CreatureRecordProjectionClosure {
                source_primary_identity: source.clone(),
                projected_identities: vec![super::super::ProjectedRecordIdentity {
                    signature: "NPC_".to_string(),
                    source_identity: source.clone(),
                    target_form_key: primary_target.clone(),
                    primary: true,
                }],
                required_target_records: Vec::new(),
                closure: super::super::CreatureRecordClosure {
                    target_plugin: plugin.to_string(),
                    records: vec![armor, npc],
                },
            }],
            primary_mappings: vec![CreaturePrimaryRecordMapping {
                source,
                target: primary_target,
            }],
        }
    }

    fn mapper_state(plugin: &str) -> MapperState {
        MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: plugin.to_string(),
                ..MapperOptions::default()
            },
        )
    }

    fn create_localized_target(plugin: &str) -> u64 {
        let handle = plugin_handle_new_native(plugin, Some("fo4")).unwrap();
        let mut store = plugin_handle_store_ref().lock().unwrap();
        store.get_mut(&handle).unwrap().parsed.header.flags |= 0x0000_0080;
        handle
    }

    #[test]
    fn merge_family_batches_preserves_distinct_candidate_projections() {
        let interner = StringInterner::new();
        let merged = merge_creature_record_family_batches(vec![
            family("motion-family", "Output.esp", 0x800, 0x100, &interner),
            family("MOTION-FAMILY", "Output.esp", 0x900, 0x101, &interner),
        ])
        .expect("merge candidate batches");

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].closures.len(), 2);
        assert_eq!(merged[0].primary_mappings.len(), 2);
        assert_ne!(
            merged[0].primary_mappings[0].source,
            merged[0].primary_mappings[1].source
        );
    }

    fn record_count(items: &[ParsedItem]) -> usize {
        items
            .iter()
            .map(|item| match item {
                ParsedItem::Record(_) => 1,
                ParsedItem::Group(group) => record_count(&group.children),
            })
            .sum()
    }

    fn assert_strings_equal(left: &LocalizedStringsState, right: &LocalizedStringsState) {
        assert_eq!(left.by_language, right.by_language);
        assert_eq!(left.default_language, right.default_language);
        assert_eq!(left.table_types, right.table_types);
        assert_eq!(left.is_filtered, right.is_filtered);
        assert_eq!(left.requested_language, right.requested_language);
        assert_eq!(left.source_strings_dir, right.source_strings_dir);
    }

    fn ledger_receipt(plugin: &str) -> CreatureRecordBatchReceipt {
        let primary = CreaturePrimaryRecordMapping {
            source: source(0x1001),
            target: TargetFormKey::new(0xA00, plugin),
        };
        CreatureRecordBatchReceipt {
            family_ids: vec!["zeta".to_string(), "alpha".to_string()],
            families: vec![
                CreatureRecordFamilyReceipt {
                    family_id: "zeta".to_string(),
                    record_count: 1,
                    mapping_count: 1,
                    primary_mappings: vec![primary],
                    races: vec![CreatureRaceRecordReceipt {
                        form_key: TargetFormKey::new(0xA00, plugin),
                        attack_events: vec!["zetaBite".to_string()],
                        attack_data_entries: 1,
                    }],
                },
                CreatureRecordFamilyReceipt {
                    family_id: "alpha".to_string(),
                    record_count: 1,
                    mapping_count: 0,
                    primary_mappings: Vec::new(),
                    races: Vec::new(),
                },
            ],
            record_count: 2,
            mapping_count: 1,
            reserved_form_keys: vec![
                TargetFormKey::new(0xA01, plugin),
                TargetFormKey::new(0xA00, plugin),
            ],
        }
    }

    fn ledger_fixture(plugin: &str) -> CreatureRecordCommitLedger {
        let receipt = ledger_receipt(plugin);
        let intent = CreatureRecordCommitIntent::from_receipt(
            plugin,
            "fo4-authoring-v1",
            "a".repeat(64),
            &receipt,
            CreatureRecordRecipeBinding {
                family_ids: vec!["alpha".to_string(), "zeta".to_string()],
                primary_mappings: receipt
                    .families
                    .iter()
                    .flat_map(|family| family.primary_mappings.clone())
                    .collect(),
                encoded_records: vec![
                    CreatureEncodedRecordCommitment {
                        form_key: TargetFormKey::new(0xA00, plugin),
                        signature: "RACE".to_string(),
                        encoded_blake3: "b".repeat(64),
                    },
                    CreatureEncodedRecordCommitment {
                        form_key: TargetFormKey::new(0xA01, plugin),
                        signature: "ARMO".to_string(),
                        encoded_blake3: "c".repeat(64),
                    },
                ],
                ..CreatureRecordRecipeBinding::default()
            },
        )
        .unwrap();
        CreatureRecordCommitLedger::new(intent, receipt).unwrap()
    }

    #[test]
    fn record_commit_ledger_is_canonical_roundtrips_and_rejects_tampering() {
        let ledger = ledger_fixture("LedgerCanonical.esp");
        let json = ledger.canonical_json().unwrap();
        let reopened = CreatureRecordCommitLedger::from_json(&json).unwrap();
        assert_eq!(reopened, ledger);
        assert_eq!(reopened.intent.family_ids, ["alpha", "zeta"]);
        assert_eq!(reopened.receipt.family_ids, ["alpha", "zeta"]);

        let mut tampered: serde_json::Value = serde_json::from_str(&json).unwrap();
        tampered["receipt"]["record_count"] = serde_json::json!(99);
        let tampered = serde_json::to_string(&tampered).unwrap();
        assert!(matches!(
            CreatureRecordCommitLedger::from_json(&tampered),
            Err(CreatureRecordCommitLedgerError::Invalid {
                code: "receipt_hash",
                ..
            })
        ));
    }

    #[test]
    fn record_commit_ledger_rejects_duplicate_family_and_mapping_collision() {
        let ledger = ledger_fixture("LedgerCollision.esp");
        let mut duplicate_family = ledger.clone();
        duplicate_family.intent.family_ids.push("alpha".to_string());
        assert!(matches!(
            duplicate_family.canonical_json(),
            Err(CreatureRecordCommitLedgerError::Invalid {
                code: "family_id",
                ..
            })
        ));

        let mut duplicate_mapping = ledger;
        let mapping = duplicate_mapping.intent.primary_mappings[0].clone();
        duplicate_mapping.intent.primary_mappings.push(mapping);
        duplicate_mapping.intent.mapping_count += 1;
        assert!(matches!(
            duplicate_mapping.canonical_json(),
            Err(CreatureRecordCommitLedgerError::Invalid {
                code: "duplicate_mapping_source",
                ..
            })
        ));
    }

    #[test]
    fn record_commit_ledger_rejects_tampered_content_commitment() {
        let mut ledger = ledger_fixture("LedgerContentHash.esp");
        ledger.intent.recipe_binding.encoded_records[0].encoded_blake3 = "d".repeat(64);
        assert!(matches!(
            ledger.canonical_json(),
            Err(CreatureRecordCommitLedgerError::Invalid {
                code: "intent_hash",
                ..
            })
        ));
    }

    #[test]
    fn record_commit_intent_rejects_recipe_binding_with_same_counts_but_other_mapping() {
        let plugin = "LedgerBindingMismatch.esp";
        let receipt = ledger_receipt(plugin);
        let mut binding = CreatureRecordRecipeBinding {
            family_ids: receipt.family_ids.clone(),
            primary_mappings: receipt
                .families
                .iter()
                .flat_map(|family| family.primary_mappings.clone())
                .collect(),
            encoded_records: vec![
                CreatureEncodedRecordCommitment {
                    form_key: TargetFormKey::new(0xA00, plugin),
                    signature: "RACE".to_string(),
                    encoded_blake3: "b".repeat(64),
                },
                CreatureEncodedRecordCommitment {
                    form_key: TargetFormKey::new(0xA01, plugin),
                    signature: "ARMO".to_string(),
                    encoded_blake3: "c".repeat(64),
                },
            ],
            ..CreatureRecordRecipeBinding::default()
        };
        binding.primary_mappings[0].target = TargetFormKey::new(0xA01, plugin);

        assert!(matches!(
            CreatureRecordCommitIntent::from_receipt(
                plugin,
                "fo4-authoring-v1",
                "a".repeat(64),
                &receipt,
                binding,
            ),
            Err(CreatureRecordCommitLedgerError::Invalid {
                code: "recipe_primary_mapping",
                ..
            })
        ));
    }

    #[test]
    fn recipe_binding_hashes_normalized_record_content_not_just_keys_or_counts() {
        let plugin = "LedgerBinding.esp";
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let original = recipe_binding_from_families(
            vec![family("quadruped", plugin, 0xAB0, 0x1234, &interner)],
            &schema,
            &interner,
        )
        .unwrap();
        let mut changed_family = family("quadruped", plugin, 0xAB0, 0x1234, &interner);
        changed_family.closures[0]
            .closure
            .records
            .iter_mut()
            .find(|record| record.sig.as_str() == "NPC_")
            .unwrap()
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "FULL")
            .unwrap()
            .value = FieldValue::String(interner.intern("Different Creature"));
        let changed =
            recipe_binding_from_families(vec![changed_family], &schema, &interner).unwrap();

        assert_eq!(original.family_ids, changed.family_ids);
        assert_eq!(
            original.encoded_records.len(),
            changed.encoded_records.len()
        );
        assert_ne!(original.encoded_records, changed.encoded_records);
    }

    #[test]
    fn record_commit_ledger_stages_atomically_only_at_the_explicit_postcommit_boundary() {
        let private_root = tempfile::tempdir().unwrap();
        let ledger = ledger_fixture("LedgerStage.esp");
        let committed = CommittedCreatureRecordBatch {
            receipt: ledger.receipt.clone(),
            ledger,
        };
        let destination = private_root
            .path()
            .join(CREATURE_RECORD_COMMIT_LEDGER_RELATIVE_PATH);
        assert!(!destination.exists());

        stage_creature_record_commit_ledger(private_root.path(), &committed, false).unwrap();
        assert_eq!(
            CreatureRecordCommitLedger::read(&destination).unwrap(),
            committed.ledger().clone()
        );
        assert!(matches!(
            stage_creature_record_commit_ledger(private_root.path(), &committed, false),
            Err(CreatureRecordCommitLedgerError::PostCommitPersistence(error))
                if matches!(*error, CreatureRecordCommitLedgerError::DestinationExists(_))
        ));
        stage_creature_record_commit_ledger(private_root.path(), &committed, true).unwrap();
        assert_eq!(
            CreatureRecordCommitLedger::read(&destination).unwrap(),
            committed.ledger().clone()
        );
    }

    #[test]
    fn every_injected_record_and_mapping_failure_is_atomic() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let cases = [
            (CreatureRecordBatchStage::EncodeRecord, 0),
            (CreatureRecordBatchStage::EncodeRecord, 1),
            (CreatureRecordBatchStage::AddRecord, 0),
            (CreatureRecordBatchStage::AddRecord, 1),
            (CreatureRecordBatchStage::AddMapping, 0),
        ];

        for (case, (stage, index)) in cases.into_iter().enumerate() {
            let plugin = format!("BatchAtomicFailure{case}.esp");
            let handle = create_localized_target(&plugin);
            let strings_before = {
                let store = plugin_handle_store_ref().lock().unwrap();
                store.get(&handle).unwrap().strings_ref().clone()
            };
            let mut mapper_state = mapper_state(&plugin);
            let mappings_before = mapper_state.source_to_target.clone();
            let used_before = mapper_state.used_object_ids.clone();
            let next_before = mapper_state.next_object_id;

            let error = {
                let mut session = open_session(handle, None).unwrap();
                publish_creature_record_families_batch_with_failure(
                    &mut session,
                    &mut mapper_state,
                    vec![family("quadruped", &plugin, 0x900, 0x1234, &interner)],
                    &schema,
                    &interner,
                    CreatureRecordBatchFailureInjection::At { stage, index },
                )
                .unwrap_err()
            };

            assert!(matches!(
                error,
                CreatureRecordBatchError::InjectedFailure {
                    stage: actual_stage,
                    index: actual_index,
                } if actual_stage == stage && actual_index == index
            ));
            assert_eq!(mapper_state.source_to_target, mappings_before);
            assert_eq!(mapper_state.used_object_ids, used_before);
            assert_eq!(mapper_state.next_object_id, next_before);
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&handle).unwrap();
            assert_eq!(record_count(&slot.parsed.root_items), 0);
            assert_strings_equal(slot.strings_ref(), &strings_before);
        }
    }

    #[test]
    fn successful_batch_commits_records_strings_reservations_and_mapping() {
        let plugin = "BatchAtomicSuccess.esp";
        let handle = create_localized_target(plugin);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let mut mapper_state = mapper_state(plugin);
        mapper_state.next_object_id = 0x920;
        let receipt = {
            let mut session = open_session(handle, None).unwrap();
            publish_creature_record_family_batch(
                &mut session,
                &mut mapper_state,
                family("quadruped", plugin, 0x920, 0x1234, &interner),
                &schema,
                &interner,
            )
            .unwrap()
        };

        assert_eq!(receipt.family_ids, ["quadruped"]);
        assert_eq!(receipt.record_count, 2);
        assert_eq!(receipt.mapping_count, 1);
        assert_eq!(
            receipt
                .reserved_form_keys
                .iter()
                .map(|target| target.local)
                .collect::<Vec<_>>(),
            [0x920, 0x921]
        );
        let json = receipt.canonical_json().unwrap();
        assert!(json.contains("\"000920@BatchAtomicSuccess.esp\""));
        assert_eq!(
            serde_json::from_str::<CreatureRecordBatchReceipt>(&json).unwrap(),
            receipt
        );
        assert!(mapper_state.used_object_ids.contains(&0x920));
        assert!(mapper_state.used_object_ids.contains(&0x921));
        assert_eq!(
            mapper_state
                .source_to_target
                .get(&form_key(0x1234, "SourceCreatures.esm", &interner)),
            Some(&form_key(0x920, plugin, &interner))
        );
        let next_generated =
            FormKeyMapper::from_state(&mut mapper_state, &interner).allocate_generated();
        assert_eq!(next_generated.local, 0x922);
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle).unwrap();
        assert_eq!(record_count(&slot.parsed.root_items), 2);
        assert!(slot.strings_ref().by_language.values().any(|table| {
            table
                .values()
                .any(|value| value == "Transactional Creature")
        }));
    }

    #[test]
    fn exact_preallocated_creature_leases_commit_and_mismatched_mapping_stays_fatal() {
        let plugin = "BatchPreallocatedLease.esp";
        let handle = create_localized_target(plugin);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let mut leased_mapper = mapper_state(plugin);
        let source_key = form_key(0x1234, "SourceCreatures.esm", &interner);
        let target = form_key(0x925, plugin, &interner);
        leased_mapper.source_to_target.insert(source_key, target);
        leased_mapper
            .reserved_generated_object_ids
            .extend([0x925, 0x926]);
        leased_mapper.leased_object_ids.extend([0x925, 0x926]);

        {
            let mut session = open_session(handle, None).unwrap();
            publish_creature_record_family_batch(
                &mut session,
                &mut leased_mapper,
                family("quadruped", plugin, 0x925, 0x1234, &interner),
                &schema,
                &interner,
            )
            .expect("exact preallocation lease must commit");
        }
        assert!(!leased_mapper.reserved_generated_object_ids.contains(&0x925));
        assert!(!leased_mapper.reserved_generated_object_ids.contains(&0x926));
        assert!(!leased_mapper.leased_object_ids.contains(&0x925));
        assert!(!leased_mapper.leased_object_ids.contains(&0x926));
        assert!(leased_mapper.used_object_ids.contains(&0x925));
        assert!(leased_mapper.used_object_ids.contains(&0x926));
        assert_eq!(
            leased_mapper.source_to_target.get(&source_key),
            Some(&target)
        );

        let other_plugin = "BatchMismatchedLease.esp";
        let other_handle = create_localized_target(other_plugin);
        let mut other_mapper = mapper_state(other_plugin);
        let other_source = form_key(0x1234, "SourceCreatures.esm", &interner);
        other_mapper
            .source_to_target
            .insert(other_source, form_key(0xA00, other_plugin, &interner));
        other_mapper
            .reserved_generated_object_ids
            .extend([0x935, 0x936]);
        let before = other_mapper.clone();
        let error = {
            let mut session = open_session(other_handle, None).unwrap();
            publish_creature_record_family_batch(
                &mut session,
                &mut other_mapper,
                family("quadruped", other_plugin, 0x935, 0x1234, &interner),
                &schema,
                &interner,
            )
            .expect_err("mismatched preinstalled mapping must remain fatal")
        };
        assert!(matches!(
            error,
            CreatureRecordBatchError::ExistingSourceMapping { .. }
        ));
        assert_eq!(other_mapper.source_to_target, before.source_to_target);
        assert_eq!(
            other_mapper.reserved_generated_object_ids,
            before.reserved_generated_object_ids
        );
        assert_eq!(other_mapper.used_object_ids, before.used_object_ids);
    }

    #[test]
    fn skyrim_race_record_mapping_coexists_with_primary_npc_projection() {
        let plugin = "BatchSkyrimRaceIdentity.esp";
        let handle = create_localized_target(plugin);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let source_identity = SourceCreatureIdentity {
            namespace: "skyrimse".to_string(),
            plugin: "Skyrim.esm".to_string(),
            local_form_id: 0x131F5,
        };
        let source_key = source_form_key(&source_identity, &interner);
        let npc_local = 0x925;
        let armor_local = 0x926;
        let race_local = 0x927;
        let race_target = form_key(race_local, plugin, &interner);
        let mut batch = family("atronach_flame", plugin, npc_local, 0x131F5, &interner);
        batch.closures[0].source_primary_identity = source_identity.clone();
        batch.closures[0].projected_identities[0].source_identity = source_identity.clone();
        batch.primary_mappings[0].source = source_identity;
        batch.closures[0].closure.records.push(record(
            "RACE",
            race_local,
            plugin,
            "BatchAtronachFlameRace",
            smallvec![field(
                "EDID",
                FieldValue::String(interner.intern("BatchAtronachFlameRace"))
            )],
            &interner,
        ));

        let mut mapper_state = mapper_state(plugin);
        mapper_state
            .source_to_target
            .insert(source_key, race_target);
        mapper_state
            .reserved_generated_object_ids
            .extend([npc_local, armor_local, race_local]);
        mapper_state
            .leased_object_ids
            .extend([npc_local, armor_local, race_local]);

        {
            let mut session = open_session(handle, None).unwrap();
            publish_creature_record_family_batch(
                &mut session,
                &mut mapper_state,
                batch,
                &schema,
                &interner,
            )
            .expect("leased Skyrim RACE mapping and primary NPC projection must coexist");
        }

        assert_eq!(
            mapper_state.source_to_target.get(&source_key),
            Some(&race_target)
        );
        assert!(!mapper_state.leased_object_ids.contains(&npc_local));
        assert!(!mapper_state.leased_object_ids.contains(&race_local));
        let store = plugin_handle_store_ref().lock().unwrap();
        assert_eq!(
            record_count(&store.get(&handle).unwrap().parsed.root_items),
            3
        );
    }

    #[test]
    fn prepared_batch_can_be_abandoned_without_target_or_mapper_mutation() {
        let plugin = "BatchPreparedAbandon.esp";
        let handle = create_localized_target(plugin);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let strings_before = {
            let store = plugin_handle_store_ref().lock().unwrap();
            store.get(&handle).unwrap().strings_ref().clone()
        };
        let mut mapper_state = mapper_state(plugin);
        {
            let mut session = open_session(handle, None).unwrap();
            let prepared = prepare_creature_record_families_batch(
                &mut session,
                &mut mapper_state,
                vec![family("quadruped", plugin, 0x930, 0x1234, &interner)],
                &schema,
                &interner,
            )
            .unwrap();
            assert_eq!(prepared.receipt().record_count, 2);
            assert_eq!(prepared.receipt().mapping_count, 1);
            drop(prepared);
        }

        assert!(mapper_state.source_to_target.is_empty());
        assert!(mapper_state.used_object_ids.is_empty());
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle).unwrap();
        assert_eq!(record_count(&slot.parsed.root_items), 0);
        assert_strings_equal(slot.strings_ref(), &strings_before);
    }

    #[test]
    fn cross_family_and_existing_target_collisions_are_typed_and_atomic() {
        let plugin = "BatchCollision.esp";
        let handle = create_localized_target(plugin);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let mut mapper_state = mapper_state(plugin);
        let cross_family = {
            let mut session = open_session(handle, None).unwrap();
            publish_creature_record_families_batch(
                &mut session,
                &mut mapper_state,
                vec![
                    family("family-a", plugin, 0x940, 0x1001, &interner),
                    family("family-b", plugin, 0x940, 0x1002, &interner),
                ],
                &schema,
                &interner,
            )
            .unwrap_err()
        };
        assert!(matches!(
            cross_family,
            CreatureRecordBatchError::CrossFamilyRecordCollision { .. }
        ));
        assert!(mapper_state.source_to_target.is_empty());

        {
            let existing = record(
                "ARMO",
                0x941,
                plugin,
                "ExistingSkin",
                smallvec![field(
                    "EDID",
                    FieldValue::String(interner.intern("ExistingSkin"))
                )],
                &interner,
            );
            let mut session = open_session(handle, None).unwrap();
            session.add_record(existing, &schema, &interner).unwrap();
        }
        let existing_collision = {
            let mut session = open_session(handle, None).unwrap();
            publish_creature_record_family_batch(
                &mut session,
                &mut mapper_state,
                family("family-a", plugin, 0x940, 0x1001, &interner),
                &schema,
                &interner,
            )
            .unwrap_err()
        };
        assert!(matches!(
            existing_collision,
            CreatureRecordBatchError::ExistingTargetCollision { local: 0x941, .. }
        ));
        assert!(mapper_state.source_to_target.is_empty());
        let store = plugin_handle_store_ref().lock().unwrap();
        assert_eq!(
            record_count(&store.get(&handle).unwrap().parsed.root_items),
            1
        );
    }

    #[test]
    fn leased_support_record_is_replaced_without_creating_a_duplicate() {
        let plugin = "BatchSupportReplacement.esp";
        let handle = create_localized_target(plugin);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let armor_local = 0x951;
        {
            let existing = record(
                "ARMO",
                armor_local,
                plugin,
                "GenericTranslatedSkin",
                smallvec![field(
                    "EDID",
                    FieldValue::String(interner.intern("GenericTranslatedSkin"))
                )],
                &interner,
            );
            let mut session = open_session(handle, None).unwrap();
            session.add_record(existing, &schema, &interner).unwrap();
        }
        let mut mapper_state = mapper_state(plugin);
        mapper_state
            .reserved_generated_object_ids
            .insert(armor_local);
        mapper_state.leased_object_ids.insert(armor_local);

        let receipt = {
            let mut session = open_session(handle, None).unwrap();
            publish_creature_record_family_batch(
                &mut session,
                &mut mapper_state,
                family("quadruped", plugin, 0x950, 0x1234, &interner),
                &schema,
                &interner,
            )
            .expect("leased support record must be replaceable")
        };

        assert_eq!(receipt.record_count, 2);
        assert!(!mapper_state.leased_object_ids.contains(&armor_local));
        assert!(mapper_state.used_object_ids.contains(&armor_local));
        let store = plugin_handle_store_ref().lock().unwrap();
        assert_eq!(
            record_count(&store.get(&handle).unwrap().parsed.root_items),
            2
        );
    }

    #[test]
    fn dangling_helper_and_real_encode_failure_leave_everything_unpublished() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        for (case, mutate) in ["dangling", "encode"].into_iter().enumerate() {
            let plugin = format!("BatchValidationFailure{case}.esp");
            let handle = create_localized_target(&plugin);
            let mut mapper_state = mapper_state(&plugin);
            let mut batch = family("quadruped", &plugin, 0x960, 0x1234, &interner);
            let records = &mut batch.closures[0].closure.records;
            if mutate == "dangling" {
                let npc = records
                    .iter_mut()
                    .find(|record| record.sig.as_str() == "NPC_")
                    .unwrap();
                npc.fields
                    .iter_mut()
                    .find(|field| field.sig.as_str() == "WNAM")
                    .unwrap()
                    .value = FieldValue::FormKey(form_key(0x0fff, &plugin, &interner));
            } else {
                let armor = records
                    .iter_mut()
                    .find(|record| record.sig.as_str() == "ARMO")
                    .unwrap();
                armor.sig = SigCode::from_str("ZZZZ").unwrap();
            }

            let error = {
                let mut session = open_session(handle, None).unwrap();
                match prepare_creature_record_families_batch(
                    &mut session,
                    &mut mapper_state,
                    vec![batch],
                    &schema,
                    &interner,
                ) {
                    Err(error) => error,
                    Ok(_) => panic!("{mutate} batch unexpectedly prepared"),
                }
            };
            if mutate == "dangling" {
                assert!(matches!(
                    error,
                    CreatureRecordBatchError::DanglingHelperReference { .. }
                ));
            } else {
                assert!(matches!(error, CreatureRecordBatchError::Encode { .. }));
            }
            assert!(mapper_state.source_to_target.is_empty());
            assert!(mapper_state.used_object_ids.is_empty());
            let store = plugin_handle_store_ref().lock().unwrap();
            assert_eq!(
                record_count(&store.get(&handle).unwrap().parsed.root_items),
                0
            );
        }
    }

    #[test]
    fn family_order_does_not_change_explicit_formkey_reservation_or_mapping() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let plugin = "BatchDeterministic.esp";
        let first_handle = create_localized_target(plugin);
        let second_handle = create_localized_target(plugin);
        let mut first_mapper = mapper_state(plugin);
        let mut second_mapper = mapper_state(plugin);
        let first = {
            let mut session = open_session(first_handle, None).unwrap();
            publish_creature_record_families_batch(
                &mut session,
                &mut first_mapper,
                vec![
                    family("zeta", plugin, 0x980, 0x2002, &interner),
                    family("alpha", plugin, 0x970, 0x2001, &interner),
                ],
                &schema,
                &interner,
            )
            .unwrap()
        };
        let second = {
            let mut session = open_session(second_handle, None).unwrap();
            publish_creature_record_families_batch(
                &mut session,
                &mut second_mapper,
                vec![
                    family("alpha", plugin, 0x970, 0x2001, &interner),
                    family("zeta", plugin, 0x980, 0x2002, &interner),
                ],
                &schema,
                &interner,
            )
            .unwrap()
        };

        assert_eq!(first, second);
        assert_eq!(
            first_mapper.source_to_target,
            second_mapper.source_to_target
        );
        assert_eq!(first_mapper.used_object_ids, second_mapper.used_object_ids);
        assert_eq!(first_mapper.next_object_id, second_mapper.next_object_id);
    }

    #[test]
    fn race_receipt_preserves_normalized_attack_event_evidence() {
        let plugin = "BatchReceipt.esp";
        let interner = StringInterner::new();
        let race = record(
            "RACE",
            0x990,
            plugin,
            "BatchRace",
            smallvec![
                field(
                    "ATKE",
                    FieldValue::String(interner.intern("meleeSecondary"))
                ),
                field("ATKD", FieldValue::Struct(Vec::new())),
                field("ATKE", FieldValue::String(interner.intern("meleePrimary"))),
                field("ATKD", FieldValue::Struct(Vec::new())),
            ],
            &interner,
        );

        let receipt = race_receipt(&race, plugin, &interner);

        assert_eq!(receipt.form_key, TargetFormKey::new(0x990, plugin));
        assert_eq!(receipt.attack_events, ["meleePrimary", "meleeSecondary"]);
        assert_eq!(receipt.attack_data_entries, 2);
    }
}
