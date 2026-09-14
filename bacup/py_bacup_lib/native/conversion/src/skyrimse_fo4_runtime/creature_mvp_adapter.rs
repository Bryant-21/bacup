use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use nif_core_native::creature_closure::CreatureClosureReceipt;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::source_rig::{CreatureTargetRecordReference, SourceRigExecutableRecipe};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::skyrimse_fo4::{MagicSupport, classify_magic_component};

use super::creature_catalog::{
    CreatureCatalogIssue, CreatureCorpusPlan, CreatureNpcPlan, CreatureRacePlan,
};
use super::source_rig_bridge::{
    SkyrimSourceRigBridgeError, SkyrimSourceRigBridgeInput,
    build_skyrim_source_rig_executable_recipe,
};

pub const SKYRIM_CREATURE_DEPENDENCY_SCHEMA: &str = "skyrim_creature_dependencies_v1";
// Must match the same-named constants in `creature_recipe`.
pub const SKYRIM_CREATURE_CANDIDATE_COUNT: usize = 121;
pub const SKYRIM_CURATED_EXCLUSION_COUNT: usize = 4;
pub const SKYRIM_CREATURE_FAMILY_COUNT: usize = 46;

const REQUIRED_SIGNATURES: &[&str] = &[
    "ACTI", "ALCH", "AMMO", "ARMA", "ARMO", "ARTO", "AVIF", "BOOK", "BPTD", "CLAS", "CSTY", "EFSH",
    "ENCH", "EQUP", "EXPL", "FACT", "FLST", "GLOB", "HAZD", "IDLE", "IMAD", "INGR", "IPCT", "IPDS",
    "KYWD", "KEYM", "LIGH", "LVLI", "LVLN", "MGEF", "MISC", "MOVT", "MSTT", "NPC_", "OTFT", "PACK",
    "PERK", "PROJ", "RACE", "SCOL", "SCRL", "SHOU", "SLGM", "SNCT", "SNDR", "SPEL", "STAT", "TXST",
    "VTYP", "WEAP",
];
const CNTO_REFERENCE_OFFSET: usize = 0;
const LVLO_REFERENCE_OFFSET: usize = 4;
const LVLO_ROW_SIZE: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimSourceRecordLocator {
    pub form_key: FormKey,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimSourceReferenceLocator {
    pub owner: SkyrimSourceRecordLocator,
    pub field_path: String,
    pub target: FormKey,
    pub target_signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimSourceFieldLocator {
    pub owner: SkyrimSourceRecordLocator,
    pub field_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkyrimRecordLoweringKind {
    SourceRigProjection,
    SkyrimPairHook,
    SkyrimMagicContract,
    SkyrimTargetProjection,
    SchemaIdentical,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkyrimCreatureDependencyBlocker {
    CuratedNonCreatureRace,
    CatalogIssue(String),
    MissingSourceRecord { form_key: FormKey },
    TargetRecordUnsupported { signature: String },
    TargetFieldUnsupported { signature: String, field: String },
    TargetFieldLayoutMismatch { signature: String, field: String },
    UnsupportedMagicLowering { signature: String },
    UnsupportedCreatureWeapon,
    SourceScriptLoweringUnavailable,
    MissingFamily,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkyrimRecordLoweringDisposition {
    Ready {
        kind: SkyrimRecordLoweringKind,
    },
    Blocked {
        blockers: Vec<SkyrimCreatureDependencyBlocker>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureDependencyRecord {
    pub source: SkyrimSourceRecordLocator,
    pub owners: Vec<String>,
    pub fields: Vec<SkyrimSourceFieldLocator>,
    pub references: Vec<SkyrimSourceReferenceLocator>,
    pub disposition: SkyrimRecordLoweringDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkyrimCreatureDependencyCandidateDisposition {
    Ready,
    Blocked {
        blockers: Vec<SkyrimCreatureDependencyBlocker>,
    },
    CuratedExclusion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureDependencyCandidate {
    pub source_race: FormKey,
    pub family_id: Option<String>,
    pub npc_sources: Vec<FormKey>,
    pub template_sources: Vec<FormKey>,
    pub seeds: Vec<FormKey>,
    pub closure: Vec<FormKey>,
    pub disposition: SkyrimCreatureDependencyCandidateDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureDependencyFamily {
    pub family_id: String,
    pub source_races: Vec<FormKey>,
    pub npc_sources: Vec<FormKey>,
    pub closure: Vec<FormKey>,
    pub disposition: SkyrimCreatureDependencyCandidateDisposition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkyrimCreatureDependencyAccounting {
    pub candidate_races: usize,
    pub curated_exclusions: usize,
    pub families: usize,
    pub npc_records: usize,
    pub template_records: usize,
    pub closure_records: usize,
    pub ready_records: usize,
    pub blocked_records: usize,
    pub ready_candidates: usize,
    pub blocked_candidates: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureDependencyLedger {
    pub schema: &'static str,
    pub candidates: Vec<SkyrimCreatureDependencyCandidate>,
    pub families: Vec<SkyrimCreatureDependencyFamily>,
    pub records: Vec<SkyrimCreatureDependencyRecord>,
    pub accounting: SkyrimCreatureDependencyAccounting,
}

impl SkyrimCreatureDependencyLedger {
    pub fn ready_form_keys(&self) -> Vec<FormKey> {
        self.records
            .iter()
            .filter_map(|record| {
                matches!(
                    record.disposition,
                    SkyrimRecordLoweringDisposition::Ready { .. }
                )
                .then_some(record.source.form_key)
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureTargetDependencyProjection {
    pub source: SkyrimSourceRecordLocator,
    pub target: CreatureTargetRecordReference,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkyrimCreatureSourceFieldDecision {
    Preserved {
        target_signature: String,
        target_field: String,
    },
    Lowered {
        target_signature: String,
        target_field: String,
        policy_id: String,
    },
    Derived {
        target_signature: String,
        target_field: String,
        policy_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureSourceFieldReceipt {
    pub source: SkyrimSourceFieldLocator,
    pub decision: SkyrimCreatureSourceFieldDecision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureConvertedArtifact {
    pub runtime_path: String,
    pub source_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct SkyrimCreatureMvpPreparationInput {
    pub source_race: FormKey,
    pub source_key: String,
    pub output_slug: String,
    pub family_id: String,
    pub publish_root: String,
    pub bridge: SkyrimSourceRigBridgeInput,
    pub closure_staged_data_root: PathBuf,
    pub converted_artifacts: Vec<SkyrimCreatureConvertedArtifact>,
    pub dependency_projections: Vec<SkyrimCreatureTargetDependencyProjection>,
    pub source_field_receipts: Vec<SkyrimCreatureSourceFieldReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkyrimCreatureMvpPreparationBlocker {
    CuratedExclusion,
    Dependency(SkyrimCreatureDependencyBlocker),
    MissingPreparationInput,
    SourceKeyMismatch,
    OutputSlugMismatch,
    FamilyMismatch,
    MissingSourceFieldReceipt(SkyrimSourceFieldLocator),
    UnknownSourceFieldReceipt(SkyrimSourceFieldLocator),
    InvalidSourceFieldReceipt(SkyrimSourceFieldLocator),
    MissingTargetDependency(SkyrimSourceRecordLocator),
    UnknownTargetDependency(SkyrimSourceRecordLocator),
    TargetDependencySignatureMismatch {
        source: SkyrimSourceRecordLocator,
        actual: String,
    },
    RecipeMissingTargetDependency(CreatureTargetRecordReference),
    EmptyClosureStagedDataRoot,
    MissingConvertedArtifacts,
    InvalidConvertedArtifact(String),
    SourceRigBridge(String),
}

#[derive(Clone, Debug)]
pub enum SkyrimCreatureMvpPreparationDisposition {
    Ready {
        recipe: SourceRigExecutableRecipe,
        closure: CreatureClosureReceipt,
        closure_staged_data_root: PathBuf,
        converted_artifacts: Vec<SkyrimCreatureConvertedArtifact>,
        dependency_projections: Vec<SkyrimCreatureTargetDependencyProjection>,
        source_field_receipts: Vec<SkyrimCreatureSourceFieldReceipt>,
    },
    Blocked {
        blockers: Vec<SkyrimCreatureMvpPreparationBlocker>,
    },
}

#[derive(Clone, Debug)]
pub struct SkyrimCreatureMvpPreparedCandidate {
    pub source_race: FormKey,
    pub source_key: String,
    pub output_slug: String,
    pub family_id: Option<String>,
    pub publish_root: Option<String>,
    pub disposition: SkyrimCreatureMvpPreparationDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkyrimCreatureMvpPreparedFamilyDisposition {
    Ready,
    Blocked { candidate_source_keys: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureMvpPreparedFamily {
    pub family_id: String,
    pub candidate_source_keys: Vec<String>,
    pub disposition: SkyrimCreatureMvpPreparedFamilyDisposition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkyrimCreatureMvpPreparationAccounting {
    pub candidates: usize,
    pub curated_exclusions: usize,
    pub ready_candidates: usize,
    pub blocked_candidates: usize,
    pub families: usize,
    pub ready_families: usize,
    pub blocked_families: usize,
}

#[derive(Clone, Debug)]
pub struct SkyrimCreatureMvpPreparationLedger {
    pub candidates: Vec<SkyrimCreatureMvpPreparedCandidate>,
    pub families: Vec<SkyrimCreatureMvpPreparedFamily>,
    pub accounting: SkyrimCreatureMvpPreparationAccounting,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SkyrimCreatureMvpAdapterError {
    #[error("duplicate source record {signature} {form_key:?}")]
    DuplicateSourceRecord {
        signature: String,
        form_key: FormKey,
    },
    #[error("Skyrim creature dependency input accounting drifted: {0}")]
    Accounting(String),
    #[error("Skyrim or Fallout 4 schema unavailable: {0}")]
    Schema(String),
    #[error("duplicate Skyrim creature preparation input for {0:?}")]
    DuplicatePreparationInput(FormKey),
    #[error("Skyrim creature preparation input is not a catalog candidate: {0:?}")]
    UnknownPreparationInput(FormKey),
}

pub fn required_skyrim_creature_dependency_signatures() -> &'static [&'static str] {
    REQUIRED_SIGNATURES
}

pub fn build_skyrim_creature_dependency_ledger(
    records: &[Record],
    catalog: &CreatureCorpusPlan,
    interner: &StringInterner,
) -> Result<SkyrimCreatureDependencyLedger, SkyrimCreatureMvpAdapterError> {
    validate_catalog_shape(catalog)?;
    let source_schema =
        AuthoringSchema::for_game("skyrimse").map_err(SkyrimCreatureMvpAdapterError::Schema)?;
    let target_schema =
        AuthoringSchema::for_game("fo4").map_err(SkyrimCreatureMvpAdapterError::Schema)?;
    let mut records_by_key = HashMap::new();
    for record in records {
        if records_by_key.insert(record.form_key, record).is_some() {
            return Err(SkyrimCreatureMvpAdapterError::DuplicateSourceRecord {
                signature: record.sig.as_str().to_string(),
                form_key: record.form_key,
            });
        }
    }

    let family_by_race = catalog
        .families
        .iter()
        .flat_map(|family| {
            family
                .races
                .iter()
                .map(move |race| (*race, family.family_id.clone()))
        })
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::with_capacity(
        catalog
            .races
            .len()
            .saturating_add(catalog.excluded_races.len()),
    );
    let mut owners = HashMap::<FormKey, BTreeSet<String>>::new();
    let mut all_references = HashMap::<FormKey, Vec<SkyrimSourceReferenceLocator>>::new();
    for record in records {
        all_references.insert(
            record.form_key,
            record_references(record, &records_by_key, interner),
        );
    }

    for race in &catalog.races {
        let family_id = family_by_race.get(&race.source_race).cloned();
        let owner = source_key(race.source_race, interner);
        let npcs = catalog.npcs_for_race(race.source_race).collect::<Vec<_>>();
        let mut npc_sources = npcs.iter().map(|npc| npc.source_npc).collect::<Vec<_>>();
        sort_form_keys(&mut npc_sources, interner);
        let mut template_sources = npcs
            .iter()
            .flat_map(|npc| npc.template_records.iter().copied())
            .collect::<Vec<_>>();
        sort_form_keys(&mut template_sources, interner);
        let mut seeds = race_seeds(race, &npcs);
        sort_form_keys(&mut seeds, interner);
        let (mut closure, mut blockers) =
            dependency_closure(&seeds, &records_by_key, &all_references, interner);
        blockers.extend(
            race.issues
                .iter()
                .map(|issue| SkyrimCreatureDependencyBlocker::CatalogIssue(issue_code(issue))),
        );
        if family_id.is_none() {
            blockers.push(SkyrimCreatureDependencyBlocker::MissingFamily);
        }
        for form_key in &closure {
            owners.entry(*form_key).or_default().insert(owner.clone());
        }
        canonical_blockers(&mut blockers);
        sort_form_keys(&mut closure, interner);
        candidates.push(SkyrimCreatureDependencyCandidate {
            source_race: race.source_race,
            family_id,
            npc_sources,
            template_sources,
            seeds,
            closure,
            disposition: if blockers.is_empty() {
                SkyrimCreatureDependencyCandidateDisposition::Ready
            } else {
                SkyrimCreatureDependencyCandidateDisposition::Blocked { blockers }
            },
        });
    }
    for source_race in &catalog.excluded_races {
        candidates.push(SkyrimCreatureDependencyCandidate {
            source_race: *source_race,
            family_id: None,
            npc_sources: Vec::new(),
            template_sources: Vec::new(),
            seeds: vec![*source_race],
            closure: Vec::new(),
            disposition: SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion,
        });
    }
    candidates.sort_by_key(|candidate| form_key_sort_key(candidate.source_race, interner));

    let closure_keys = owners.keys().copied().collect::<HashSet<_>>();
    let mut dependency_records = Vec::with_capacity(closure_keys.len());
    for form_key in closure_keys {
        let Some(record) = records_by_key.get(&form_key).copied() else {
            continue;
        };
        dependency_records.push(SkyrimCreatureDependencyRecord {
            source: locator(record),
            owners: owners
                .remove(&form_key)
                .unwrap_or_default()
                .into_iter()
                .collect(),
            fields: record
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| SkyrimSourceFieldLocator {
                    owner: locator(record),
                    field_path: format!("{}[{index}]", field.sig.as_str()),
                })
                .collect(),
            references: all_references.remove(&form_key).unwrap_or_default(),
            disposition: record_lowering(record, &source_schema, &target_schema, interner),
        });
    }
    dependency_records.sort_by_key(|record| {
        (
            form_key_sort_key(record.source.form_key, interner),
            record.source.signature.clone(),
        )
    });
    let disposition_by_key = dependency_records
        .iter()
        .map(|record| (record.source.form_key, &record.disposition))
        .collect::<HashMap<_, _>>();
    for candidate in &mut candidates {
        let SkyrimCreatureDependencyCandidateDisposition::Ready = candidate.disposition else {
            continue;
        };
        let blockers = candidate
            .closure
            .iter()
            .flat_map(|form_key| match disposition_by_key.get(form_key) {
                Some(SkyrimRecordLoweringDisposition::Ready { .. }) => Vec::new(),
                Some(SkyrimRecordLoweringDisposition::Blocked { blockers }) => blockers.clone(),
                None => vec![SkyrimCreatureDependencyBlocker::MissingSourceRecord {
                    form_key: *form_key,
                }],
            })
            .collect::<Vec<_>>();
        if !blockers.is_empty() {
            let mut blockers = blockers;
            canonical_blockers(&mut blockers);
            candidate.disposition =
                SkyrimCreatureDependencyCandidateDisposition::Blocked { blockers };
        }
    }

    let mut families = catalog
        .families
        .iter()
        .map(|family| {
            let family_candidates = candidates
                .iter()
                .filter(|candidate| {
                    candidate.family_id.as_deref() == Some(family.family_id.as_str())
                })
                .collect::<Vec<_>>();
            let mut npc_sources = family_candidates
                .iter()
                .flat_map(|candidate| candidate.npc_sources.iter().copied())
                .collect::<Vec<_>>();
            sort_form_keys(&mut npc_sources, interner);
            let mut closure = family_candidates
                .iter()
                .flat_map(|candidate| candidate.closure.iter().copied())
                .collect::<Vec<_>>();
            sort_form_keys(&mut closure, interner);
            let blockers = family_candidates
                .iter()
                .flat_map(|candidate| match &candidate.disposition {
                    SkyrimCreatureDependencyCandidateDisposition::Blocked { blockers } => {
                        blockers.clone()
                    }
                    SkyrimCreatureDependencyCandidateDisposition::Ready
                    | SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion => Vec::new(),
                })
                .collect::<Vec<_>>();
            let disposition = if blockers.is_empty() && !family_candidates.is_empty() {
                SkyrimCreatureDependencyCandidateDisposition::Ready
            } else {
                let mut blockers = blockers;
                canonical_blockers(&mut blockers);
                SkyrimCreatureDependencyCandidateDisposition::Blocked { blockers }
            };
            SkyrimCreatureDependencyFamily {
                family_id: family.family_id.clone(),
                source_races: family.races.clone(),
                npc_sources,
                closure,
                disposition,
            }
        })
        .collect::<Vec<_>>();
    families.sort_by(|left, right| left.family_id.cmp(&right.family_id));

    let accounting = account(&candidates, &families, &dependency_records);
    let ledger = SkyrimCreatureDependencyLedger {
        schema: SKYRIM_CREATURE_DEPENDENCY_SCHEMA,
        candidates,
        families,
        records: dependency_records,
        accounting,
    };
    validate_ledger(&ledger, catalog)?;
    Ok(ledger)
}

pub fn prepare_skyrim_creature_mvp_candidates(
    catalog: &CreatureCorpusPlan,
    dependency: &SkyrimCreatureDependencyLedger,
    inputs: Vec<SkyrimCreatureMvpPreparationInput>,
    interner: &StringInterner,
) -> Result<SkyrimCreatureMvpPreparationLedger, SkyrimCreatureMvpAdapterError> {
    validate_catalog_shape(catalog)?;
    validate_ledger(dependency, catalog)?;
    let candidate_keys = dependency
        .candidates
        .iter()
        .map(|candidate| candidate.source_race)
        .collect::<HashSet<_>>();
    let mut inputs_by_race = HashMap::new();
    for input in inputs {
        if !candidate_keys.contains(&input.source_race) {
            return Err(SkyrimCreatureMvpAdapterError::UnknownPreparationInput(
                input.source_race,
            ));
        }
        let source_race = input.source_race;
        if inputs_by_race.insert(source_race, input).is_some() {
            return Err(SkyrimCreatureMvpAdapterError::DuplicatePreparationInput(
                source_race,
            ));
        }
    }
    let dependency_records = dependency
        .records
        .iter()
        .map(|record| (record.source.form_key, record))
        .collect::<HashMap<_, _>>();
    let race_plans = catalog
        .races
        .iter()
        .map(|race| (race.source_race, race))
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::with_capacity(dependency.candidates.len());
    for candidate in &dependency.candidates {
        let source_key = source_key(candidate.source_race, interner);
        let output_slug = race_plans
            .get(&candidate.source_race)
            .map(|race| creature_output_slug(race.editor_id.as_deref(), candidate.source_race))
            .unwrap_or_else(|| creature_output_slug(None, candidate.source_race));
        if matches!(
            candidate.disposition,
            SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion
        ) {
            candidates.push(blocked_prepared_candidate(
                candidate,
                source_key,
                output_slug,
                None,
                vec![SkyrimCreatureMvpPreparationBlocker::CuratedExclusion],
            ));
            continue;
        }
        let mut blockers = match &candidate.disposition {
            SkyrimCreatureDependencyCandidateDisposition::Ready => Vec::new(),
            SkyrimCreatureDependencyCandidateDisposition::Blocked { blockers } => blockers
                .iter()
                .cloned()
                .map(SkyrimCreatureMvpPreparationBlocker::Dependency)
                .collect(),
            SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion => unreachable!(),
        };
        let Some(input) = inputs_by_race.remove(&candidate.source_race) else {
            blockers.push(SkyrimCreatureMvpPreparationBlocker::MissingPreparationInput);
            candidates.push(blocked_prepared_candidate(
                candidate,
                source_key,
                output_slug,
                None,
                blockers,
            ));
            continue;
        };
        if input.source_key != source_key {
            blockers.push(SkyrimCreatureMvpPreparationBlocker::SourceKeyMismatch);
        }
        if input.output_slug != output_slug {
            blockers.push(SkyrimCreatureMvpPreparationBlocker::OutputSlugMismatch);
        }
        if candidate.family_id.as_deref() != Some(input.family_id.as_str()) {
            blockers.push(SkyrimCreatureMvpPreparationBlocker::FamilyMismatch);
        }
        validate_field_receipts(
            candidate,
            &dependency_records,
            &input.source_field_receipts,
            &mut blockers,
        );
        let required_dependencies = validate_target_projections(
            candidate,
            &dependency_records,
            &input.dependency_projections,
            &mut blockers,
        );
        if input.closure_staged_data_root.as_os_str().is_empty() {
            blockers.push(SkyrimCreatureMvpPreparationBlocker::EmptyClosureStagedDataRoot);
        }
        validate_converted_artifacts(&input.converted_artifacts, &mut blockers);
        let closure = input.bridge.creature_closure.clone();
        let recipe = match build_skyrim_source_rig_executable_recipe(input.bridge.clone()) {
            Ok(recipe) => {
                for required in required_dependencies {
                    if !recipe
                        .batch_intent
                        .required_target_records
                        .contains(&required)
                    {
                        blockers.push(
                            SkyrimCreatureMvpPreparationBlocker::RecipeMissingTargetDependency(
                                required,
                            ),
                        );
                    }
                }
                Some(recipe)
            }
            Err(error) => {
                blockers.push(SkyrimCreatureMvpPreparationBlocker::SourceRigBridge(
                    bridge_error_detail(error),
                ));
                None
            }
        };
        canonical_preparation_blockers(&mut blockers);
        let publish_root = Some(input.publish_root.clone());
        let disposition = match (recipe, blockers.is_empty()) {
            (Some(recipe), true) => SkyrimCreatureMvpPreparationDisposition::Ready {
                recipe,
                closure,
                closure_staged_data_root: input.closure_staged_data_root,
                converted_artifacts: input.converted_artifacts,
                dependency_projections: input.dependency_projections,
                source_field_receipts: input.source_field_receipts,
            },
            _ => SkyrimCreatureMvpPreparationDisposition::Blocked { blockers },
        };
        candidates.push(SkyrimCreatureMvpPreparedCandidate {
            source_race: candidate.source_race,
            source_key,
            output_slug,
            family_id: candidate.family_id.clone(),
            publish_root,
            disposition,
        });
    }
    candidates.sort_by_key(|candidate| form_key_sort_key(candidate.source_race, interner));
    let mut families = catalog
        .families
        .iter()
        .map(|family| {
            let family_candidates = candidates
                .iter()
                .filter(|candidate| candidate.family_id.as_deref() == Some(&family.family_id))
                .collect::<Vec<_>>();
            let candidate_source_keys = family_candidates
                .iter()
                .map(|candidate| candidate.source_key.clone())
                .collect::<Vec<_>>();
            let blocked = family_candidates
                .iter()
                .filter(|candidate| {
                    matches!(
                        candidate.disposition,
                        SkyrimCreatureMvpPreparationDisposition::Blocked { .. }
                    )
                })
                .map(|candidate| candidate.source_key.clone())
                .collect::<Vec<_>>();
            SkyrimCreatureMvpPreparedFamily {
                family_id: family.family_id.clone(),
                candidate_source_keys,
                disposition: if blocked.is_empty() && !family_candidates.is_empty() {
                    SkyrimCreatureMvpPreparedFamilyDisposition::Ready
                } else {
                    SkyrimCreatureMvpPreparedFamilyDisposition::Blocked {
                        candidate_source_keys: blocked,
                    }
                },
            }
        })
        .collect::<Vec<_>>();
    families.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    let accounting = SkyrimCreatureMvpPreparationAccounting {
        candidates: candidates.len(),
        curated_exclusions: dependency.accounting.curated_exclusions,
        ready_candidates: candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.disposition,
                    SkyrimCreatureMvpPreparationDisposition::Ready { .. }
                )
            })
            .count(),
        blocked_candidates: candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.disposition,
                    SkyrimCreatureMvpPreparationDisposition::Blocked { .. }
                )
            })
            .count(),
        families: families.len(),
        ready_families: families
            .iter()
            .filter(|family| {
                family.disposition == SkyrimCreatureMvpPreparedFamilyDisposition::Ready
            })
            .count(),
        blocked_families: families
            .iter()
            .filter(|family| {
                matches!(
                    family.disposition,
                    SkyrimCreatureMvpPreparedFamilyDisposition::Blocked { .. }
                )
            })
            .count(),
    };
    if accounting.candidates != dependency.candidates.len()
        || accounting.ready_candidates + accounting.blocked_candidates != accounting.candidates
        || accounting.families != dependency.families.len()
    {
        return Err(SkyrimCreatureMvpAdapterError::Accounting(
            "terminal preparation omitted a candidate or family".to_string(),
        ));
    }
    Ok(SkyrimCreatureMvpPreparationLedger {
        candidates,
        families,
        accounting,
    })
}

fn validate_field_receipts(
    candidate: &SkyrimCreatureDependencyCandidate,
    records: &HashMap<FormKey, &SkyrimCreatureDependencyRecord>,
    receipts: &[SkyrimCreatureSourceFieldReceipt],
    blockers: &mut Vec<SkyrimCreatureMvpPreparationBlocker>,
) {
    let expected = candidate
        .closure
        .iter()
        .filter_map(|form_key| records.get(form_key).copied())
        .filter(|record| {
            matches!(
                record.disposition,
                SkyrimRecordLoweringDisposition::Ready {
                    kind: SkyrimRecordLoweringKind::SourceRigProjection
                }
            )
        })
        .flat_map(|record| record.fields.iter().cloned())
        .collect::<Vec<_>>();
    for field in &expected {
        let matching = receipts
            .iter()
            .filter(|receipt| receipt.source == *field)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            blockers.push(
                SkyrimCreatureMvpPreparationBlocker::MissingSourceFieldReceipt(field.clone()),
            );
        } else if !valid_field_decision(&matching[0].decision) {
            blockers.push(
                SkyrimCreatureMvpPreparationBlocker::InvalidSourceFieldReceipt(field.clone()),
            );
        }
    }
    for receipt in receipts {
        if !expected.contains(&receipt.source) {
            blockers.push(
                SkyrimCreatureMvpPreparationBlocker::UnknownSourceFieldReceipt(
                    receipt.source.clone(),
                ),
            );
        }
    }
}

fn valid_field_decision(decision: &SkyrimCreatureSourceFieldDecision) -> bool {
    let (signature, field, policy) = match decision {
        SkyrimCreatureSourceFieldDecision::Preserved {
            target_signature,
            target_field,
        } => (target_signature, target_field, None),
        SkyrimCreatureSourceFieldDecision::Lowered {
            target_signature,
            target_field,
            policy_id,
        }
        | SkyrimCreatureSourceFieldDecision::Derived {
            target_signature,
            target_field,
            policy_id,
        } => (target_signature, target_field, Some(policy_id)),
    };
    !signature.trim().is_empty()
        && !field.trim().is_empty()
        && policy.is_none_or(|policy| !policy.trim().is_empty())
}

fn validate_target_projections(
    candidate: &SkyrimCreatureDependencyCandidate,
    records: &HashMap<FormKey, &SkyrimCreatureDependencyRecord>,
    projections: &[SkyrimCreatureTargetDependencyProjection],
    blockers: &mut Vec<SkyrimCreatureMvpPreparationBlocker>,
) -> Vec<CreatureTargetRecordReference> {
    let expected = candidate
        .closure
        .iter()
        .filter_map(|form_key| records.get(form_key).copied())
        .filter(|record| {
            matches!(
                record.disposition,
                SkyrimRecordLoweringDisposition::Ready { kind }
                    if kind != SkyrimRecordLoweringKind::SourceRigProjection
            )
        })
        .collect::<Vec<_>>();
    let mut required = Vec::new();
    for record in &expected {
        let matching = projections
            .iter()
            .filter(|projection| projection.source.form_key == record.source.form_key)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            blockers.push(
                SkyrimCreatureMvpPreparationBlocker::MissingTargetDependency(record.source.clone()),
            );
            continue;
        }
        let projection = matching[0];
        if projection.source.signature != record.source.signature
            || projection.target.signature != record.source.signature
        {
            blockers.push(
                SkyrimCreatureMvpPreparationBlocker::TargetDependencySignatureMismatch {
                    source: record.source.clone(),
                    actual: projection.target.signature.clone(),
                },
            );
            continue;
        }
        required.push(projection.target.clone());
    }
    for projection in projections {
        if !expected
            .iter()
            .any(|record| record.source.form_key == projection.source.form_key)
        {
            blockers.push(
                SkyrimCreatureMvpPreparationBlocker::UnknownTargetDependency(
                    projection.source.clone(),
                ),
            );
        }
    }
    required.sort_by_key(|reference| {
        (
            reference.form_key.plugin.to_ascii_lowercase(),
            reference.form_key.local,
            reference.signature.clone(),
        )
    });
    required.dedup();
    required
}

fn validate_converted_artifacts(
    artifacts: &[SkyrimCreatureConvertedArtifact],
    blockers: &mut Vec<SkyrimCreatureMvpPreparationBlocker>,
) {
    if artifacts.is_empty() {
        blockers.push(SkyrimCreatureMvpPreparationBlocker::MissingConvertedArtifacts);
        return;
    }
    let mut runtime_paths = HashSet::new();
    for artifact in artifacts {
        let runtime_path = artifact.runtime_path.replace('\\', "/");
        if runtime_path.trim_matches('/').is_empty()
            || artifact.source_path.as_os_str().is_empty()
            || !runtime_paths.insert(runtime_path.to_ascii_lowercase())
        {
            blockers.push(
                SkyrimCreatureMvpPreparationBlocker::InvalidConvertedArtifact(
                    artifact.runtime_path.clone(),
                ),
            );
        }
    }
}

fn blocked_prepared_candidate(
    candidate: &SkyrimCreatureDependencyCandidate,
    source_key: String,
    output_slug: String,
    publish_root: Option<String>,
    mut blockers: Vec<SkyrimCreatureMvpPreparationBlocker>,
) -> SkyrimCreatureMvpPreparedCandidate {
    canonical_preparation_blockers(&mut blockers);
    SkyrimCreatureMvpPreparedCandidate {
        source_race: candidate.source_race,
        source_key,
        output_slug,
        family_id: candidate.family_id.clone(),
        publish_root,
        disposition: SkyrimCreatureMvpPreparationDisposition::Blocked { blockers },
    }
}

fn canonical_preparation_blockers(blockers: &mut Vec<SkyrimCreatureMvpPreparationBlocker>) {
    blockers.sort_by_key(|blocker| format!("{blocker:?}"));
    blockers.dedup();
}

fn bridge_error_detail(error: SkyrimSourceRigBridgeError) -> String {
    error.to_string()
}

fn creature_output_slug(editor_id: Option<&str>, form_key: FormKey) -> String {
    let base = editor_id
        .map(slug_component)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "creature".to_string());
    format!("{base}-{:06x}", form_key.local)
}

fn slug_component(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('-') && !output.is_empty() {
            output.push('-');
        }
    }
    output.trim_matches('-').to_string()
}

fn validate_catalog_shape(
    catalog: &CreatureCorpusPlan,
) -> Result<(), SkyrimCreatureMvpAdapterError> {
    if catalog.races.len() != catalog.summary.candidate_races
        || catalog.excluded_races.len() != catalog.summary.curated_exclusions
        || catalog.families.len() != catalog.summary.family_count
    {
        return Err(SkyrimCreatureMvpAdapterError::Accounting(
            "catalog vectors differ from summary".to_string(),
        ));
    }
    Ok(())
}

fn validate_ledger(
    ledger: &SkyrimCreatureDependencyLedger,
    catalog: &CreatureCorpusPlan,
) -> Result<(), SkyrimCreatureMvpAdapterError> {
    if ledger.candidates.len()
        != catalog
            .races
            .len()
            .saturating_add(catalog.excluded_races.len())
        || ledger.families.len() != catalog.families.len()
        || ledger.accounting.npc_records != catalog.npcs.len()
    {
        return Err(SkyrimCreatureMvpAdapterError::Accounting(
            "dependency ledger omitted a race, family, or NPC".to_string(),
        ));
    }
    let candidate_races = ledger
        .candidates
        .iter()
        .map(|candidate| candidate.source_race)
        .collect::<HashSet<_>>();
    if candidate_races.len() != ledger.candidates.len() {
        return Err(SkyrimCreatureMvpAdapterError::Accounting(
            "dependency ledger contains duplicate race candidates".to_string(),
        ));
    }
    Ok(())
}

fn race_seeds(race: &CreatureRacePlan, npcs: &[&CreatureNpcPlan]) -> Vec<FormKey> {
    let mut seeds = vec![race.source_race];
    seeds.extend(race.skin);
    seeds.extend(race.body_part_data);
    seeds.extend(race.armor_addons.iter().copied());
    seeds.extend(race.attack_spells.iter().copied());
    for npc in npcs {
        seeds.push(npc.source_npc);
        seeds.extend(npc.template_records.iter().copied());
    }
    seeds
}

fn dependency_closure(
    seeds: &[FormKey],
    records: &HashMap<FormKey, &Record>,
    references: &HashMap<FormKey, Vec<SkyrimSourceReferenceLocator>>,
    interner: &StringInterner,
) -> (Vec<FormKey>, Vec<SkyrimCreatureDependencyBlocker>) {
    let mut queue = VecDeque::from_iter(seeds.iter().copied());
    let mut closure = HashSet::new();
    let mut blockers = Vec::new();
    while let Some(current) = queue.pop_front() {
        if current.local == 0 {
            continue;
        }
        if records
            .get(&current)
            .is_some_and(|record| is_source_only_optional_record(record, interner))
        {
            continue;
        }
        if !closure.insert(current) {
            continue;
        }
        if !records.contains_key(&current) {
            blockers
                .push(SkyrimCreatureDependencyBlocker::MissingSourceRecord { form_key: current });
            continue;
        }
        if let Some(edges) = references.get(&current) {
            queue.extend(
                edges
                    .iter()
                    .filter(|edge| edge.target_signature.is_some())
                    .map(|edge| edge.target),
            );
        }
    }
    let mut closure = closure.into_iter().collect::<Vec<_>>();
    sort_form_keys(&mut closure, interner);
    canonical_blockers(&mut blockers);
    (closure, blockers)
}

fn is_source_only_optional_record(record: &Record, interner: &StringInterner) -> bool {
    record.sig.as_str() == "SHOU"
        || (matches!(record.sig.as_str(), "SPEL" | "MGEF" | "SCRL")
            && matches!(
                classify_magic_component(record, interner),
                MagicSupport::Unsupported { .. }
            ))
}

fn record_references(
    record: &Record,
    records: &HashMap<FormKey, &Record>,
    interner: &StringInterner,
) -> Vec<SkyrimSourceReferenceLocator> {
    let owner = locator(record);
    let mut references = Vec::new();
    for (field_index, field) in record.fields.iter().enumerate() {
        let reference_count = references.len();
        collect_value_references(
            &field.value,
            &format!("{}[{field_index}]", field.sig.as_str()),
            &owner,
            records,
            interner,
            &mut references,
        );
        if references.len() == reference_count {
            let raw_layout = match field.sig.as_str() {
                "CNTO" => Some((CNTO_REFERENCE_OFFSET, 0, "item")),
                "LVLO" => Some((LVLO_REFERENCE_OFFSET, LVLO_ROW_SIZE, "reference")),
                _ => None,
            };
            if let Some((offset, stride, member)) = raw_layout {
                for raw in embedded_form_ids(&field.value, offset, stride) {
                    let Some(target) = resolve_embedded_form_id(record.form_key, raw, records)
                    else {
                        continue;
                    };
                    references.push(SkyrimSourceReferenceLocator {
                        owner: owner.clone(),
                        field_path: format!("{}[{field_index}].{member}", field.sig.as_str()),
                        target,
                        target_signature: records
                            .get(&target)
                            .map(|record| record.sig.as_str().to_string()),
                    });
                }
            }
        }
    }
    references.sort_by_key(|reference| {
        (
            form_key_sort_key(reference.owner.form_key, interner),
            reference.owner.signature.clone(),
            reference.field_path.clone(),
            form_key_sort_key(reference.target, interner),
            reference.target_signature.clone(),
        )
    });
    references.dedup();
    references
}

pub(crate) fn embedded_form_ids(value: &FieldValue, offset: usize, stride: usize) -> Vec<u32> {
    match value {
        FieldValue::FormKey(form_key) => (form_key.local != 0)
            .then_some(form_key.local)
            .into_iter()
            .collect(),
        FieldValue::List(values) => values
            .iter()
            .flat_map(|value| embedded_form_ids(value, offset, stride))
            .collect(),
        FieldValue::Struct(fields) => fields
            .iter()
            .flat_map(|(_, value)| embedded_form_ids(value, offset, stride))
            .collect(),
        FieldValue::Bytes(bytes) => {
            if stride == 0 {
                return (bytes.len() >= offset + 4)
                    .then(|| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()))
                    .into_iter()
                    .collect();
            }
            let stride = stride.max(offset + 4);
            bytes
                .chunks(stride)
                .filter(|chunk| chunk.len() >= offset + 4)
                .map(|chunk| u32::from_le_bytes(chunk[offset..offset + 4].try_into().unwrap()))
                .collect()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn resolve_embedded_form_id(
    owner: FormKey,
    raw: u32,
    records: &HashMap<FormKey, &Record>,
) -> Option<FormKey> {
    let local = raw & 0x00FF_FFFF;
    if local == 0 {
        return None;
    }
    let owner_local = FormKey {
        local,
        plugin: owner.plugin,
    };
    if records.contains_key(&owner_local) {
        return Some(owner_local);
    }
    let mut matches = records.keys().filter(|key| key.local == local).copied();
    let target = matches.next()?;
    matches.next().is_none().then_some(target)
}

fn collect_value_references(
    value: &FieldValue,
    path: &str,
    owner: &SkyrimSourceRecordLocator,
    records: &HashMap<FormKey, &Record>,
    interner: &StringInterner,
    output: &mut Vec<SkyrimSourceReferenceLocator>,
) {
    match value {
        FieldValue::FormKey(target) if target.local != 0 => {
            output.push(SkyrimSourceReferenceLocator {
                owner: owner.clone(),
                field_path: path.to_string(),
                target: *target,
                target_signature: records
                    .get(target)
                    .map(|record| record.sig.as_str().to_string()),
            });
        }
        FieldValue::List(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_value_references(
                    value,
                    &format!("{path}[{index}]"),
                    owner,
                    records,
                    interner,
                    output,
                );
            }
        }
        FieldValue::Struct(fields) => {
            for (name, value) in fields {
                let name = interner.resolve(*name).unwrap_or("<unresolved>");
                collect_value_references(
                    value,
                    &format!("{path}.{name}"),
                    owner,
                    records,
                    interner,
                    output,
                );
            }
        }
        _ => {}
    }
}

fn record_lowering(
    record: &Record,
    source_schema: &AuthoringSchema,
    target_schema: &AuthoringSchema,
    interner: &StringInterner,
) -> SkyrimRecordLoweringDisposition {
    let signature = record.sig.as_str();
    if matches!(signature, "RACE" | "NPC_" | "ARMO" | "ARMA" | "BPTD") {
        return SkyrimRecordLoweringDisposition::Ready {
            kind: SkyrimRecordLoweringKind::SourceRigProjection,
        };
    }
    if matches!(signature, "SPEL" | "MGEF" | "SCRL") {
        return match classify_magic_component(record, interner) {
            MagicSupport::Supported(_) | MagicSupport::RequiresLinkedRecords(_) => {
                SkyrimRecordLoweringDisposition::Ready {
                    kind: SkyrimRecordLoweringKind::SkyrimMagicContract,
                }
            }
            MagicSupport::Unsupported { .. } => SkyrimRecordLoweringDisposition::Blocked {
                blockers: vec![SkyrimCreatureDependencyBlocker::UnsupportedMagicLowering {
                    signature: signature.to_string(),
                }],
            },
        };
    }
    if signature == "WEAP" {
        return SkyrimRecordLoweringDisposition::Ready {
            kind: SkyrimRecordLoweringKind::SkyrimPairHook,
        };
    }
    if matches!(signature, "PROJ" | "AMMO" | "EXPL" | "CSTY" | "ALCH") {
        return SkyrimRecordLoweringDisposition::Ready {
            kind: SkyrimRecordLoweringKind::SkyrimPairHook,
        };
    }
    if skyrim_creature_target_signature(signature) != signature {
        return SkyrimRecordLoweringDisposition::Ready {
            kind: SkyrimRecordLoweringKind::SkyrimTargetProjection,
        };
    }
    schema_identical_lowering(record, source_schema, target_schema)
}

pub fn skyrim_creature_target_signature(source_signature: &str) -> &str {
    match source_signature {
        "SLGM" => "MISC",
        _ => source_signature,
    }
}

fn schema_identical_lowering(
    record: &Record,
    source_schema: &AuthoringSchema,
    target_schema: &AuthoringSchema,
) -> SkyrimRecordLoweringDisposition {
    let signature = record.sig.as_str();
    if source_schema.record_def(signature).is_none() {
        return SkyrimRecordLoweringDisposition::Blocked {
            blockers: vec![SkyrimCreatureDependencyBlocker::TargetRecordUnsupported {
                signature: signature.to_string(),
            }],
        };
    }
    if target_schema.record_def(signature).is_none() {
        return SkyrimRecordLoweringDisposition::Blocked {
            blockers: vec![SkyrimCreatureDependencyBlocker::TargetRecordUnsupported {
                signature: signature.to_string(),
            }],
        };
    }
    SkyrimRecordLoweringDisposition::Ready {
        kind: SkyrimRecordLoweringKind::SchemaIdentical,
    }
}

fn locator(record: &Record) -> SkyrimSourceRecordLocator {
    SkyrimSourceRecordLocator {
        form_key: record.form_key,
        signature: record.sig.as_str().to_string(),
    }
}

fn source_key(form_key: FormKey, interner: &StringInterner) -> String {
    format!(
        "skyrimse:{}:{:06X}",
        interner.resolve(form_key.plugin).unwrap_or("<unresolved>"),
        form_key.local
    )
}

fn issue_code(issue: &CreatureCatalogIssue) -> String {
    let value = format!("{issue:?}");
    value
        .split([' ', '{'])
        .next()
        .unwrap_or("unknown_catalog_issue")
        .to_string()
}

fn form_key_sort_key(form_key: FormKey, interner: &StringInterner) -> (String, u32) {
    (
        interner
            .resolve(form_key.plugin)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        form_key.local,
    )
}

fn sort_form_keys(form_keys: &mut Vec<FormKey>, interner: &StringInterner) {
    form_keys.sort_by_key(|form_key| form_key_sort_key(*form_key, interner));
    form_keys.dedup();
}

fn canonical_blockers(blockers: &mut Vec<SkyrimCreatureDependencyBlocker>) {
    blockers.sort_by_key(|blocker| format!("{blocker:?}"));
    blockers.dedup();
}

fn account(
    candidates: &[SkyrimCreatureDependencyCandidate],
    families: &[SkyrimCreatureDependencyFamily],
    records: &[SkyrimCreatureDependencyRecord],
) -> SkyrimCreatureDependencyAccounting {
    let npc_records = candidates
        .iter()
        .flat_map(|candidate| candidate.npc_sources.iter().copied())
        .collect::<HashSet<_>>()
        .len();
    let template_records = candidates
        .iter()
        .flat_map(|candidate| candidate.template_sources.iter().copied())
        .collect::<HashSet<_>>()
        .len();
    SkyrimCreatureDependencyAccounting {
        candidate_races: candidates
            .iter()
            .filter(|candidate| {
                !matches!(
                    candidate.disposition,
                    SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion
                )
            })
            .count(),
        curated_exclusions: candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.disposition,
                    SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion
                )
            })
            .count(),
        families: families.len(),
        npc_records,
        template_records,
        closure_records: records.len(),
        ready_records: records
            .iter()
            .filter(|record| {
                matches!(
                    record.disposition,
                    SkyrimRecordLoweringDisposition::Ready { .. }
                )
            })
            .count(),
        blocked_records: records
            .iter()
            .filter(|record| {
                matches!(
                    record.disposition,
                    SkyrimRecordLoweringDisposition::Blocked { .. }
                )
            })
            .count(),
        ready_candidates: candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.disposition,
                    SkyrimCreatureDependencyCandidateDisposition::Ready
                )
            })
            .count(),
        blocked_candidates: candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.disposition,
                    SkyrimCreatureDependencyCandidateDisposition::Blocked { .. }
                )
            })
            .count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    #[test]
    fn creature_dependency_input_includes_inventory_items() {
        for signature in ["BOOK", "INGR", "KEYM", "SLGM"] {
            assert!(required_skyrim_creature_dependency_signatures().contains(&signature));
        }
    }

    #[test]
    fn skyrim_soul_gems_use_the_declared_misc_target_projection() {
        let interner = StringInterner::new();
        let source = record(&interner, "SLGM", 0x2E500);
        let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();

        assert_eq!(skyrim_creature_target_signature("SLGM"), "MISC");
        assert_eq!(
            record_lowering(&source, &source_schema, &target_schema, &interner),
            SkyrimRecordLoweringDisposition::Ready {
                kind: SkyrimRecordLoweringKind::SkyrimTargetProjection,
            }
        );
    }

    fn key(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("Skyrim.esm"),
        }
    }

    fn record(interner: &StringInterner, signature: &str, local: u32) -> Record {
        Record::new(SigCode::from_str(signature).unwrap(), key(interner, local))
    }

    fn push_form(record: &mut Record, signature: &str, value: FormKey) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value: FieldValue::FormKey(value),
        });
    }

    fn fixture(interner: &StringInterner) -> (Vec<Record>, CreatureCorpusPlan) {
        let race_key = key(interner, 0x100);
        let npc_key = key(interner, 0x101);
        let package_key = key(interner, 0x102);
        let mut race = record(interner, "RACE", 0x100);
        let mut npc = record(interner, "NPC_", 0x101);
        push_form(&mut npc, "RNAM", race_key);
        push_form(&mut npc, "PKID", package_key);
        let package = record(interner, "PACK", 0x102);
        let catalog = CreatureCorpusPlan {
            races: vec![CreatureRacePlan {
                source_race: race_key,
                source_plugin: "Skyrim.esm".to_string(),
                editor_id: Some("FixtureRace".to_string()),
                skin: None,
                armor_addons: Vec::new(),
                body_models: Vec::new(),
                body_model_parts: Vec::new(),
                body_part_data: None,
                project_paths: vec!["actors/fixture/project.hkx".to_string()],
                skeleton_paths: vec!["actors/fixture/skeleton.nif".to_string()],
                attack_events: Vec::new(),
                attack_contract: Vec::new(),
                attack_data: Vec::new(),
                attack_spells: Vec::new(),
                issues: Vec::new(),
            }],
            npcs: vec![CreatureNpcPlan {
                source_npc: npc_key,
                effective_races: vec![race_key],
                template_records: Vec::new(),
                family_ids: vec!["family".to_string()],
                issues: Vec::new(),
            }],
            families: vec![super::super::creature_catalog::CreatureFamilyPlan {
                family_id: "family".to_string(),
                key: super::super::creature_catalog::CreatureFamilyKey {
                    project_paths: vec!["actors/fixture/project.hkx".to_string()],
                    skeleton_paths: vec!["actors/fixture/skeleton.nif".to_string()],
                    body_models: Vec::new(),
                    attack_contract: Vec::new(),
                },
                races: vec![race_key],
                issues: Vec::new(),
            }],
            summary: super::super::creature_catalog::CreatureCorpusSummary {
                candidate_races: 1,
                family_count: 1,
                candidate_npcs: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        race.eid = Some(interner.intern("FixtureRace"));
        (vec![race, npc, package], catalog)
    }

    #[test]
    fn dependency_ledger_closes_npc_package_edges_without_dropping_the_npc() {
        let interner = StringInterner::new();
        let (records, catalog) = fixture(&interner);
        let ledger =
            build_skyrim_creature_dependency_ledger(&records, &catalog, &interner).unwrap();
        assert_eq!(ledger.accounting.candidate_races, 1);
        assert_eq!(ledger.accounting.npc_records, 1);
        assert_eq!(
            ledger.candidates[0].npc_sources,
            vec![key(&interner, 0x101)]
        );
        assert!(
            ledger.candidates[0]
                .closure
                .contains(&key(&interner, 0x102))
        );
        assert!(ledger.records.iter().any(|record| {
            record.source.form_key == key(&interner, 0x101)
                && record.references.iter().any(|reference| {
                    reference.field_path == "PKID[1]" && reference.target == key(&interner, 0x102)
                })
        }));
    }

    #[test]
    fn raw_cnto_and_lvlo_form_ids_join_the_dependency_graph() {
        let interner = StringInterner::new();
        let mut npc = record(&interner, "NPC_", 0x101);
        let mut cnto = vec![0_u8; 8];
        cnto[..4].copy_from_slice(&0x102_u32.to_le_bytes());
        cnto[4..].copy_from_slice(&1_i32.to_le_bytes());
        npc.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CNTO").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(cnto)),
        });
        let mut lvln = record(&interner, "LVLN", 0x103);
        let mut lvlo = vec![0_u8; 12];
        lvlo[4..8].copy_from_slice(&0x101_u32.to_le_bytes());
        lvln.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(lvlo)),
        });
        let item = record(&interner, "MISC", 0x102);
        let records = [&npc, &item, &lvln]
            .into_iter()
            .map(|record| (record.form_key, record))
            .collect::<HashMap<_, _>>();

        let npc_references = record_references(&npc, &records, &interner);
        assert!(npc_references.iter().any(|reference| {
            reference.field_path == "CNTO[0].item" && reference.target == item.form_key
        }));
        let lvln_references = record_references(&lvln, &records, &interner);
        assert!(lvln_references.iter().any(|reference| {
            reference.field_path == "LVLO[0].reference" && reference.target == npc.form_key
        }));

        assert_eq!(
            embedded_form_ids(&FieldValue::FormKey(item.form_key), 0, 0),
            [item.form_key.local]
        );
    }

    #[test]
    fn dependency_ledger_does_not_block_on_references_outside_the_selected_record_set() {
        let interner = StringInterner::new();
        let (mut records, catalog) = fixture(&interner);
        push_form(&mut records[1], "INAM", key(&interner, 0x200));

        let ledger =
            build_skyrim_creature_dependency_ledger(&records, &catalog, &interner).unwrap();

        assert!(
            !ledger.candidates[0]
                .closure
                .contains(&key(&interner, 0x200))
        );
        assert!(matches!(
            ledger.candidates[0].disposition,
            SkyrimCreatureDependencyCandidateDisposition::Ready
        ));
    }

    #[test]
    fn source_rig_projection_accepts_source_vmad() {
        let interner = StringInterner::new();
        let (mut records, catalog) = fixture(&interner);
        records[1].fields.push(FieldEntry {
            sig: SubrecordSig::from_str("VMAD").unwrap(),
            value: FieldValue::Bytes(Default::default()),
        });
        let ledger =
            build_skyrim_creature_dependency_ledger(&records, &catalog, &interner).unwrap();
        assert!(ledger.records.iter().any(|record| {
            record.source.form_key == key(&interner, 0x101)
                && matches!(
                    &record.disposition,
                    SkyrimRecordLoweringDisposition::Ready {
                        kind: SkyrimRecordLoweringKind::SourceRigProjection
                    }
                )
        }));
        assert!(matches!(
            ledger.candidates[0].disposition,
            SkyrimCreatureDependencyCandidateDisposition::Ready
        ));
    }

    #[test]
    fn source_only_shout_attack_is_omitted_from_the_target_closure() {
        let interner = StringInterner::new();
        let (mut records, mut catalog) = fixture(&interner);
        let shout = key(&interner, 0x103);
        push_form(&mut records[0], "SPLO", shout);
        records.push(record(&interner, "SHOU", 0x103));
        catalog.races[0].attack_spells.push(shout);

        let ledger =
            build_skyrim_creature_dependency_ledger(&records, &catalog, &interner).unwrap();

        assert!(!ledger.candidates[0].closure.contains(&shout));
        assert!(
            !ledger
                .records
                .iter()
                .any(|record| record.source.form_key == shout)
        );
        assert!(matches!(
            ledger.candidates[0].disposition,
            SkyrimCreatureDependencyCandidateDisposition::Ready
        ));
    }

    #[test]
    fn terminal_preparation_accounts_for_missing_inputs_without_omitting_candidates() {
        let interner = StringInterner::new();
        let (records, catalog) = fixture(&interner);
        let dependency =
            build_skyrim_creature_dependency_ledger(&records, &catalog, &interner).unwrap();
        let prepared =
            prepare_skyrim_creature_mvp_candidates(&catalog, &dependency, Vec::new(), &interner)
                .unwrap();

        assert_eq!(prepared.accounting.candidates, 1);
        assert_eq!(prepared.accounting.ready_candidates, 0);
        assert_eq!(prepared.accounting.blocked_candidates, 1);
        assert_eq!(prepared.accounting.families, 1);
        assert!(matches!(
            &prepared.candidates[0].disposition,
            SkyrimCreatureMvpPreparationDisposition::Blocked { blockers }
                if blockers.contains(
                    &SkyrimCreatureMvpPreparationBlocker::MissingPreparationInput
                )
        ));
    }
}
