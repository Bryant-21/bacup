//! Read-only FNV/FO3 creature preparation adapter for the shared MVP producer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use nif_core_native::creature_closure::CreatureClosureReceipt;
use serde::Serialize;
use thiserror::Error;

use crate::source_rig::SourceRigExecutableRecipe;

use super::creature_catalog::{CreatureProvenance, RigFamilyKey, StableFormKey};
use super::creature_dependencies::{
    CreatureDependencyBlocker, CreatureDependencyCandidateReceipt, CreatureDependencyError,
    FnvFo3CreatureDependencyLedger, RuntimeDependencyRecordReceipt,
};
use super::creature_recipe::{
    CreatureFamilyRecipeLedger, CreatureRecipeBuildError, CreatureRecordRecipeDisposition,
    FamilyRecipeDisposition,
};
use super::source_rig_bridge::{
    FnvFo3SourceRigAdapterInput, build_fnv_fo3_source_rig_executable_recipe_prevalidated,
    canonical_fnv_fo3_family_id,
};

pub const FNV_FO3_MVP_PREPARATION_SCHEMA_VERSION: u32 = 3;

pub(super) fn canonical_recipe_source_key(source: &StableFormKey) -> StableFormKey {
    StableFormKey {
        local: source.local,
        plugin: source.plugin.to_ascii_lowercase(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnvFo3MvpCandidateDestination {
    pub source: StableFormKey,
    pub output_slug: String,
    pub publish_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FnvFo3MvpConvertedArtifact {
    pub runtime_path: String,
    pub source_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct FnvFo3MvpCandidatePreparationInput {
    pub source: StableFormKey,
    pub family: RigFamilyKey,
    pub source_rig: FnvFo3SourceRigAdapterInput,
    pub closure_staged_data_root: PathBuf,
    pub converted_artifacts: Vec<FnvFo3MvpConvertedArtifact>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum FnvFo3MvpCandidateBlocker {
    Dependency(CreatureDependencyBlocker),
    RecordDisposition { detail: String },
    FamilyDisposition { detail: String },
    MissingCandidatePreparation,
    FamilyPreparation { detail: String },
    MissingPrimaryNpcReservation,
    InvalidPrimaryNpcReservation { detail: String },
    RecordProjection { detail: String },
    AssetPreparation { detail: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum FnvFo3MvpCandidateDisposition {
    Ready {
        additional_branches: Vec<FnvFo3MvpPreparedBranch>,
        recipe: SourceRigExecutableRecipe,
        closure: CreatureClosureReceipt,
        closure_staged_data_root: PathBuf,
        converted_artifacts: Vec<FnvFo3MvpConvertedArtifact>,
    },
    Blocked {
        reasons: Vec<FnvFo3MvpCandidateBlocker>,
    },
    Accounted {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FnvFo3MvpPreparedBranch {
    pub family_id: String,
    pub recipe: SourceRigExecutableRecipe,
    pub closure: CreatureClosureReceipt,
    pub closure_staged_data_root: PathBuf,
    pub converted_artifacts: Vec<FnvFo3MvpConvertedArtifact>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FnvFo3MvpCandidatePreparation {
    pub source: StableFormKey,
    pub source_key: String,
    pub provenance: CreatureProvenance,
    pub output_slug: String,
    pub family_id: String,
    pub publish_root: PathBuf,
    pub dependency_seed_form_keys: Vec<StableFormKey>,
    pub dependency_records: Vec<RuntimeDependencyRecordReceipt>,
    pub disposition: FnvFo3MvpCandidateDisposition,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FnvFo3MvpPreparationLedger {
    pub schema_version: u32,
    pub candidates: Vec<FnvFo3MvpCandidatePreparation>,
    pub candidate_count: usize,
    pub ready_count: usize,
    pub accounted_count: usize,
    pub blocked_count: usize,
    pub content_hash_blake3: String,
}

impl FnvFo3MvpPreparationLedger {
    pub fn validate(&self) -> Result<(), FnvFo3MvpAdapterError> {
        if self.schema_version != FNV_FO3_MVP_PREPARATION_SCHEMA_VERSION
            || self.candidate_count != self.candidates.len()
            || self.ready_count + self.accounted_count + self.blocked_count != self.candidate_count
            || self.ready_count
                != self
                    .candidates
                    .iter()
                    .filter(|candidate| {
                        matches!(
                            candidate.disposition,
                            FnvFo3MvpCandidateDisposition::Ready { .. }
                        )
                    })
                    .count()
            || self.accounted_count
                != self
                    .candidates
                    .iter()
                    .filter(|candidate| {
                        matches!(
                            candidate.disposition,
                            FnvFo3MvpCandidateDisposition::Accounted { .. }
                        )
                    })
                    .count()
            || !strictly_sorted_unique(self.candidates.iter().map(|candidate| &candidate.source))
        {
            return Err(FnvFo3MvpAdapterError::InvalidInput(
                "preparation terminal accounting is not canonical".to_string(),
            ));
        }
        let actual = preparation_hash(self)?;
        if self.content_hash_blake3 != actual {
            return Err(FnvFo3MvpAdapterError::HashMismatch {
                expected: self.content_hash_blake3.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum FnvFo3MvpAdapterError {
    #[error(transparent)]
    Dependency(#[from] CreatureDependencyError),
    #[error(transparent)]
    Recipe(#[from] CreatureRecipeBuildError),
    #[error("invalid FNV/FO3 MVP preparation input: {0}")]
    InvalidInput(String),
    #[error("FNV/FO3 MVP preparation hash mismatch: expected {expected}, actual {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("FNV/FO3 MVP preparation is not serializable: {0}")]
    Serialization(String),
}

#[cfg(test)]
pub fn build_fnv_fo3_mvp_candidate_preparations(
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
    recipe_ledger: &CreatureFamilyRecipeLedger,
    destinations: Vec<FnvFo3MvpCandidateDestination>,
    candidate_inputs: Vec<FnvFo3MvpCandidatePreparationInput>,
) -> Result<FnvFo3MvpPreparationLedger, FnvFo3MvpAdapterError> {
    build_fnv_fo3_mvp_candidate_preparations_with_live_blockers(
        dependency_ledger,
        recipe_ledger,
        destinations,
        candidate_inputs,
        BTreeMap::new(),
    )
}

pub fn build_fnv_fo3_mvp_candidate_preparations_with_live_blockers(
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
    recipe_ledger: &CreatureFamilyRecipeLedger,
    destinations: Vec<FnvFo3MvpCandidateDestination>,
    candidate_inputs: Vec<FnvFo3MvpCandidatePreparationInput>,
    live_blockers: BTreeMap<StableFormKey, Vec<FnvFo3MvpCandidateBlocker>>,
) -> Result<FnvFo3MvpPreparationLedger, FnvFo3MvpAdapterError> {
    dependency_ledger.validate()?;
    recipe_ledger.validate()?;

    let destination_by_source = canonical_destinations(destinations, dependency_ledger)?;
    let record_by_source = recipe_ledger
        .records
        .iter()
        .map(|record| (canonical_recipe_source_key(&record.source), record))
        .collect::<BTreeMap<_, _>>();
    let family_by_rig = recipe_ledger
        .families
        .iter()
        .map(|family| (family.rig.clone(), family))
        .collect::<BTreeMap<_, _>>();
    let dependency_records = dependency_ledger
        .records
        .iter()
        .map(|record| (record.source.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let prepared_candidates = prepare_candidates(recipe_ledger, candidate_inputs)?;
    let live_blockers = canonical_live_blockers(live_blockers, dependency_ledger)?;

    let mut candidates = Vec::with_capacity(dependency_ledger.candidates.len());
    for dependency in &dependency_ledger.candidates {
        let destination = &destination_by_source[&dependency.source];
        let mut blockers = dependency
            .blockers
            .iter()
            .cloned()
            .map(FnvFo3MvpCandidateBlocker::Dependency)
            .collect::<BTreeSet<_>>();
        blockers.extend(
            live_blockers
                .get(&dependency.source)
                .into_iter()
                .flatten()
                .cloned(),
        );
        let recipe_source = canonical_recipe_source_key(&dependency.source);
        let record = record_by_source.get(&recipe_source).copied();
        let mut accounted_reason = None;
        let mut family = record.and_then(|record| match &record.disposition {
            CreatureRecordRecipeDisposition::VisualOwner { rig, .. }
            | CreatureRecordRecipeDisposition::Proxy { rig, .. } => Some(rig.clone()),
            CreatureRecordRecipeDisposition::RejectedProxy { blockers: rejected } => {
                accounted_reason = Some(format!("rejected_proxy:{rejected:?}"));
                None
            }
            CreatureRecordRecipeDisposition::Special { reason } => {
                accounted_reason = Some(format!("special:{reason:?}"));
                None
            }
        });
        if record.is_none() {
            blockers.insert(FnvFo3MvpCandidateBlocker::RecordDisposition {
                detail: "recipe_record_missing".to_string(),
            });
        }
        let candidate_preparations = prepared_candidates.get(&dependency.source);
        if let Some(PreparedFamily::Ready { family: template_family, template_branch: Some(_), .. }) = candidate_preparations.and_then(|items| items.first()) {
            family = Some(template_family.clone());
            accounted_reason = None;
        }

        if let Some(family) = &family {
            match family_by_rig.get(family).map(|family| &family.disposition) {
                Some(FamilyRecipeDisposition::Ready) => {}
                Some(FamilyRecipeDisposition::Rejected { blockers: rejected }) => {
                    blockers.insert(FnvFo3MvpCandidateBlocker::FamilyDisposition {
                        detail: format!("{rejected:?}"),
                    });
                }
                None => {
                    blockers.insert(FnvFo3MvpCandidateBlocker::FamilyDisposition {
                        detail: "recipe_family_missing".to_string(),
                    });
                }
            }
        }

        let family_id = family
            .as_ref()
            .map(canonical_fnv_fo3_family_id)
            .unwrap_or_else(|| terminal_family_id(dependency));
        let candidate_preparation = candidate_preparations.and_then(|items| items.first());
        for preparation in candidate_preparations.into_iter().flatten() {
            if let PreparedFamily::Blocked(detail) = preparation {
                blockers.insert(FnvFo3MvpCandidateBlocker::FamilyPreparation {
                    detail: detail.clone(),
                });
            }
        }
        if candidate_preparation.is_none() && family.is_some() && blockers.is_empty() {
            blockers.insert(FnvFo3MvpCandidateBlocker::MissingCandidatePreparation);
        }

        let dependency_records = dependency
            .closure_form_keys
            .iter()
            .filter_map(|source| {
                dependency_records
                    .get(source)
                    .map(|record| (*record).clone())
            })
            .collect::<Vec<_>>();
        let disposition = if blockers.is_empty() && accounted_reason.is_some() {
            FnvFo3MvpCandidateDisposition::Accounted {
                reason: accounted_reason.expect("accounted reason was checked"),
            }
        } else if blockers.is_empty() {
            let Some(PreparedFamily::Ready {
                recipe,
                closure,
                closure_staged_data_root,
                converted_artifacts,
                ..
            }) = candidate_preparation
            else {
                unreachable!("an unblocked candidate has a prepared family")
            };
            FnvFo3MvpCandidateDisposition::Ready {
                additional_branches: candidate_preparations.into_iter().flatten().skip(1).map(|branch| {
                    let PreparedFamily::Ready { recipe, closure, closure_staged_data_root, converted_artifacts, .. } = branch else {
                        unreachable!("all branch blockers were checked")
                    };
                    FnvFo3MvpPreparedBranch {
                        family_id: recipe.batch_intent.family_id.clone(), recipe: recipe.clone(), closure: closure.clone(),
                        closure_staged_data_root: closure_staged_data_root.clone(), converted_artifacts: converted_artifacts.clone(),
                    }
                }).collect(),
                recipe: recipe.clone(),
                closure: closure.clone(),
                closure_staged_data_root: closure_staged_data_root.clone(),
                converted_artifacts: converted_artifacts.clone(),
            }
        } else {
            FnvFo3MvpCandidateDisposition::Blocked {
                reasons: blockers.into_iter().collect(),
            }
        };
        candidates.push(FnvFo3MvpCandidatePreparation {
            source: dependency.source.clone(),
            source_key: dependency.source.to_string(),
            provenance: dependency.provenance.clone(),
            output_slug: destination.output_slug.clone(),
            family_id,
            publish_root: destination.publish_root.clone(),
            dependency_seed_form_keys: dependency.dependency_seed_form_keys.clone(),
            dependency_records,
            disposition,
        });
    }

    let ready_count = candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.disposition,
                FnvFo3MvpCandidateDisposition::Ready { .. }
            )
        })
        .count();
    let accounted_count = candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.disposition,
                FnvFo3MvpCandidateDisposition::Accounted { .. }
            )
        })
        .count();
    let mut ledger = FnvFo3MvpPreparationLedger {
        schema_version: FNV_FO3_MVP_PREPARATION_SCHEMA_VERSION,
        candidate_count: candidates.len(),
        ready_count,
        accounted_count,
        blocked_count: candidates.len() - ready_count - accounted_count,
        candidates,
        content_hash_blake3: String::new(),
    };
    ledger.content_hash_blake3 = preparation_hash(&ledger)?;
    ledger.validate()?;
    Ok(ledger)
}

fn canonical_live_blockers(
    mut blockers: BTreeMap<StableFormKey, Vec<FnvFo3MvpCandidateBlocker>>,
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
) -> Result<BTreeMap<StableFormKey, Vec<FnvFo3MvpCandidateBlocker>>, FnvFo3MvpAdapterError> {
    let candidates = dependency_ledger
        .candidates
        .iter()
        .map(|candidate| &candidate.source)
        .collect::<BTreeSet<_>>();
    if blockers.keys().any(|source| !candidates.contains(source)) {
        return Err(FnvFo3MvpAdapterError::InvalidInput(
            "live preparation blocker names a noncandidate source".to_string(),
        ));
    }
    for reasons in blockers.values_mut() {
        reasons.sort();
        reasons.dedup();
        if reasons.is_empty() {
            return Err(FnvFo3MvpAdapterError::InvalidInput(
                "live preparation blocker set is empty".to_string(),
            ));
        }
    }
    Ok(blockers)
}

#[derive(Clone, Debug)]
enum PreparedFamily {
    Ready {
        family: RigFamilyKey,
        template_branch: Option<super::creature_templates::LegacyTemplateBranchReceipt>,
        recipe: SourceRigExecutableRecipe,
        closure: CreatureClosureReceipt,
        closure_staged_data_root: PathBuf,
        converted_artifacts: Vec<FnvFo3MvpConvertedArtifact>,
    },
    Blocked(String),
}

fn prepare_candidates(
    recipe_ledger: &CreatureFamilyRecipeLedger,
    candidate_inputs: Vec<FnvFo3MvpCandidatePreparationInput>,
) -> Result<BTreeMap<StableFormKey, Vec<PreparedFamily>>, FnvFo3MvpAdapterError> {
    let expected_ledger_hash = recipe_ledger.content_hash_blake3.clone();
    let mut prepared = BTreeMap::new();
    let record_by_source = recipe_ledger
        .records
        .iter()
        .map(|record| (canonical_recipe_source_key(&record.source), record))
        .collect::<BTreeMap<_, _>>();
    let mut groups = BTreeMap::<StableFormKey, Vec<FnvFo3MvpCandidatePreparationInput>>::new();
    for input in candidate_inputs {
        groups.entry(input.source.clone()).or_default().push(input);
    }
    for (source, mut inputs) in groups {
        if inputs.iter().any(|input| input.source_rig.template_branch.is_some()) {
            let branches = inputs.iter().map(|input| input.source_rig.template_branch.as_ref())
                .collect::<Option<Vec<_>>>().ok_or_else(|| FnvFo3MvpAdapterError::InvalidInput("template and non-template preparations cannot share a root".into()))?;
            let first = branches[0];
            let paths = branches.iter().map(|branch| branch.entry_indices.clone()).collect::<BTreeSet<_>>();
            if branches.iter().any(|branch| branch.plan != first.plan || branch.profile().map_or(true, |profile| canonical_recipe_source_key(&profile.root) != canonical_recipe_source_key(&source)))
                || branches.iter().filter(|branch| branch.wrapper).count() != 1
                || paths.len() != branches.len() || paths != first.plan.branch_paths().into_iter().collect() {
                return Err(FnvFo3MvpAdapterError::InvalidInput(format!("template candidate {source} must cover every ordered source branch exactly once")));
            }
            inputs.sort_by_key(|input| {
                let branch = input.source_rig.template_branch.as_ref().unwrap();
                (!branch.wrapper, branch.entry_indices.clone())
            });
        } else if inputs.len() != 1 {
            return Err(FnvFo3MvpAdapterError::InvalidInput(format!("duplicate candidate preparation {source}")));
        }
        let mut preparations = Vec::new();
        for mut input in inputs {
        if input.family != input.source_rig.family {
            return Err(FnvFo3MvpAdapterError::InvalidInput(
                "family preparation and source-rig family differ".to_string(),
            ));
        }
        if input.source_rig.ledger.content_hash_blake3 != expected_ledger_hash {
            return Err(FnvFo3MvpAdapterError::InvalidInput(format!(
                "family {} uses a different recipe ledger",
                canonical_fnv_fo3_family_id(&input.family)
            )));
        }
        let template_branch = input.source_rig.template_branch.clone();
        let owner = template_branch.as_ref().map(|branch| &branch.profile().unwrap().owners[6]).unwrap_or(&input.source);
        let expected_family = record_by_source
            .get(&canonical_recipe_source_key(owner))
            .and_then(|record| match &record.disposition {
                CreatureRecordRecipeDisposition::VisualOwner { rig, .. }
                | CreatureRecordRecipeDisposition::Proxy { rig, .. } => Some(rig),
                CreatureRecordRecipeDisposition::RejectedProxy { .. }
                | CreatureRecordRecipeDisposition::Special { .. } => None,
            });
        let identity = &input.source_rig.projection.source_primary_identity;
        let identity_matches = if let Some(branch) = template_branch.as_ref().filter(|branch| !branch.wrapper) {
            identity == &branch.generated_identity
        } else {
            identity.local_form_id == input.source.local && identity.plugin.eq_ignore_ascii_case(&input.source.plugin)
        };
        if expected_family != Some(&input.family) || !identity_matches
        {
            return Err(FnvFo3MvpAdapterError::InvalidInput(format!(
                "candidate {} preparation identity or family differs from the recipe ledger",
                input.source
            )));
        }
        input
            .converted_artifacts
            .sort_by_key(|artifact| canonical_runtime_path(&artifact.runtime_path));
        let preparation = validate_family_paths(&input)
            .and_then(|()| {
                let closure = input.source_rig.creature_closure.clone();
                build_fnv_fo3_source_rig_executable_recipe_prevalidated(
                    input.source_rig,
                    &expected_ledger_hash,
                )
                .map(|recipe| (recipe, closure))
                .map_err(|error| error.to_string())
            })
            .map(|(recipe, closure)| PreparedFamily::Ready {
                family: input.family,
                template_branch,
                recipe,
                closure,
                closure_staged_data_root: input.closure_staged_data_root,
                converted_artifacts: input.converted_artifacts,
            })
            .unwrap_or_else(PreparedFamily::Blocked);
        preparations.push(preparation);
        }
        prepared.insert(source, preparations);
    }
    Ok(prepared)
}

fn validate_family_paths(input: &FnvFo3MvpCandidatePreparationInput) -> Result<(), String> {
    if input.closure_staged_data_root.as_os_str().is_empty() {
        return Err("closure_staged_data_root_empty".to_string());
    }
    let mut expected = input
        .source_rig
        .artifact_receipts
        .iter()
        .map(|receipt| canonical_runtime_path(&receipt.runtime_path))
        .collect::<BTreeSet<_>>();
    if let Some(receipt) = &input.source_rig.runtime_model_closure {
        expected.extend(
            receipt
                .canonical_target_artifacts()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|artifact| canonical_runtime_path(&artifact.target_data_path)),
        );
    }
    let actual = input
        .converted_artifacts
        .iter()
        .map(|artifact| canonical_runtime_path(&artifact.runtime_path))
        .collect::<BTreeSet<_>>();
    if expected != actual
        || actual.len() != input.converted_artifacts.len()
        || input
            .converted_artifacts
            .iter()
            .any(|artifact| artifact.source_path.as_os_str().is_empty())
    {
        return Err("converted_artifact_path_closure_mismatch".to_string());
    }
    Ok(())
}

fn canonical_destinations(
    destinations: Vec<FnvFo3MvpCandidateDestination>,
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
) -> Result<BTreeMap<StableFormKey, FnvFo3MvpCandidateDestination>, FnvFo3MvpAdapterError> {
    let mut result = BTreeMap::new();
    for destination in destinations {
        if destination.output_slug.trim().is_empty()
            || destination.publish_root.as_os_str().is_empty()
            || result
                .insert(destination.source.clone(), destination)
                .is_some()
        {
            return Err(FnvFo3MvpAdapterError::InvalidInput(
                "candidate destinations are empty or duplicated".to_string(),
            ));
        }
    }
    let expected = dependency_ledger
        .candidates
        .iter()
        .map(|candidate| &candidate.source)
        .collect::<BTreeSet<_>>();
    let actual = result.keys().collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(FnvFo3MvpAdapterError::InvalidInput(
            "candidate destinations do not exactly cover the dependency ledger".to_string(),
        ));
    }
    Ok(result)
}

fn terminal_family_id(candidate: &CreatureDependencyCandidateReceipt) -> String {
    format!(
        "{}|terminal|{}",
        match candidate.provenance.game {
            super::creature_catalog::LegacyCreatureGame::Fnv => "fnv",
            super::creature_catalog::LegacyCreatureGame::Fo3 => "fo3",
        },
        candidate.source
    )
}

fn canonical_runtime_path(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

fn preparation_hash(ledger: &FnvFo3MvpPreparationLedger) -> Result<String, FnvFo3MvpAdapterError> {
    let mut unhashed = ledger.clone();
    unhashed.content_hash_blake3.clear();
    let bytes = serde_json::to_vec(&unhashed)
        .map_err(|error| FnvFo3MvpAdapterError::Serialization(error.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn strictly_sorted_unique<'a, T: Ord + 'a>(values: impl Iterator<Item = &'a T>) -> bool {
    let values = values.collect::<Vec<_>>();
    values.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_source_keys_are_case_insensitive() {
        assert_eq!(
            canonical_recipe_source_key(&StableFormKey {
                local: 0x100,
                plugin: "FalloutNV.esm".to_string(),
            }),
            StableFormKey {
                local: 0x100,
                plugin: "falloutnv.esm".to_string(),
            }
        );
    }

    #[test]
    fn accounted_terminal_validates_separately_from_blocked() {
        let source = StableFormKey {
            local: 0x100,
            plugin: "FalloutNV.esm".to_string(),
        };
        let candidate = FnvFo3MvpCandidatePreparation {
            source: source.clone(),
            source_key: source.to_string(),
            provenance: CreatureProvenance {
                game: super::super::creature_catalog::LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                precedence: 0,
            },
            output_slug: "fixture".to_string(),
            family_id: "fnv|terminal|fixture".to_string(),
            publish_root: PathBuf::from("fixture"),
            dependency_seed_form_keys: Vec::new(),
            dependency_records: Vec::new(),
            disposition: FnvFo3MvpCandidateDisposition::Accounted {
                reason: "special:marker".to_string(),
            },
        };
        let mut ledger = FnvFo3MvpPreparationLedger {
            schema_version: FNV_FO3_MVP_PREPARATION_SCHEMA_VERSION,
            candidates: vec![candidate],
            candidate_count: 1,
            ready_count: 0,
            accounted_count: 1,
            blocked_count: 0,
            content_hash_blake3: String::new(),
        };
        ledger.content_hash_blake3 = preparation_hash(&ledger).unwrap();

        ledger.validate().unwrap();
    }
}
