//! Source-neutral evidence and record-batch carrier for ancillary creature NPCs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use nif_core_native::model::NifFile;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    CreaturePrimaryRecordMapping, CreatureRecordBatchReceipt, CreatureRecordFamilyBatch,
    CreatureRecordProjectionClosure, CreatureTargetRecordReference, SourceCreatureIdentity,
    TargetFormKey,
};

pub const CREATURE_ANCILLARY_NPC_LEDGER_VERSION: u32 = 1;
pub const CREATURE_ANCILLARY_NPC_COMMIT_LEDGER_VERSION: u32 = 1;
pub const CREATURE_ANCILLARY_NPC_FAMILY_ID: &str = "creature_ancillary_npcs";
pub const CREATURE_ANCILLARY_NPC_FACEGEN_PIPELINE_ID: &str = "legacy-creature-facegen-to-fo4-v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CreatureAncillaryNpcReservedIdentity {
    pub source: SourceCreatureIdentity,
    pub target_npc: TargetFormKey,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CreatureAncillaryNpcRequestId {
    pub candidate: SourceCreatureIdentity,
    pub ancillary_npc: SourceCreatureIdentity,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAncillaryNpcEffectiveField {
    Template,
    ActorBaseConfig,
    AiData,
    Stats,
    Class,
    Voice,
    Race,
    Inventory,
    Spells,
    DeathItem,
    Packages,
    CombatStyle,
    Sex,
    HairColor,
    FaceMorphSymmetric,
    FaceMorphAsymmetric,
    FaceTextureSymmetry,
    LegacyHeadPart,
    Eye,
    Hair,
}

impl CreatureAncillaryNpcEffectiveField {
    fn all() -> BTreeSet<Self> {
        [
            Self::Template,
            Self::ActorBaseConfig,
            Self::AiData,
            Self::Stats,
            Self::Class,
            Self::Voice,
            Self::Race,
            Self::Inventory,
            Self::Spells,
            Self::DeathItem,
            Self::Packages,
            Self::CombatStyle,
            Self::Sex,
            Self::HairColor,
            Self::FaceMorphSymmetric,
            Self::FaceMorphAsymmetric,
            Self::FaceTextureSymmetry,
            Self::LegacyHeadPart,
            Self::Eye,
            Self::Hair,
        ]
        .into_iter()
        .collect()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcEffectiveFieldReceipt {
    pub field: CreatureAncillaryNpcEffectiveField,
    pub effective_source: SourceCreatureIdentity,
    pub source_locator: String,
    pub value_blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcTemplateReceipt {
    pub source_chain: Vec<SourceCreatureIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_template: Option<TargetFormKey>,
    pub effective_fields: Vec<CreatureAncillaryNpcEffectiveFieldReceipt>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAncillaryNpcSex {
    Male,
    Female,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum CreatureAncillaryNpcFacegenDisposition {
    Generated {
        generation: CreatureAncillaryNpcFacegenGenerationReceipt,
    },
    ProvenFo4RuntimeAssetless {
        morph_inputs_blake3: String,
        target_appearance_closure_blake3: String,
        proof_blake3: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcFacegenGenerationReceipt {
    pub pipeline_id: String,
    pub pipeline_resources_blake3: String,
    pub morph_inputs_blake3: String,
    pub target_appearance_closure_blake3: String,
    pub artifacts_blake3: String,
}

pub fn creature_ancillary_npc_facegen_generation_receipt(
    template: &CreatureAncillaryNpcTemplateReceipt,
    pipeline_resources_blake3: String,
    target_appearance_closure_blake3: String,
    artifacts: &[CreatureAncillaryNpcArtifactReceipt],
) -> Result<CreatureAncillaryNpcFacegenGenerationReceipt, CreatureAncillaryNpcError> {
    validate_hash(&pipeline_resources_blake3, "facegen pipeline resources")?;
    validate_hash(
        &target_appearance_closure_blake3,
        "target appearance closure",
    )?;
    Ok(CreatureAncillaryNpcFacegenGenerationReceipt {
        pipeline_id: CREATURE_ANCILLARY_NPC_FACEGEN_PIPELINE_ID.to_string(),
        pipeline_resources_blake3,
        morph_inputs_blake3: creature_ancillary_npc_morph_inputs_blake3(template)?,
        target_appearance_closure_blake3,
        artifacts_blake3: creature_ancillary_npc_facegen_artifacts_blake3(artifacts)?,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAncillaryNpcArtifactKind {
    Nif,
    Tri,
    Bgsm,
    Bgem,
    Dds,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAncillaryNpcSourceArtifactKind {
    Nif,
    Tri,
    Egm,
    Egt,
    Bgsm,
    Bgem,
    Dds,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcSourceArtifactReceipt {
    pub kind: CreatureAncillaryNpcSourceArtifactKind,
    pub source_game: String,
    pub source_data_path: String,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcArtifactReceipt {
    pub kind: CreatureAncillaryNpcArtifactKind,
    pub source_artifacts: Vec<CreatureAncillaryNpcSourceArtifactReceipt>,
    pub target_data_path: String,
    pub target_byte_len: u64,
    pub target_blake3: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAncillaryNpcAppearancePartKind {
    Eye,
    Hair,
    LegacyHeadPart,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcAppearancePartReceipt {
    pub kind: CreatureAncillaryNpcAppearancePartKind,
    pub owner_npc: SourceCreatureIdentity,
    pub effective_source: SourceCreatureIdentity,
    pub source_part: SourceCreatureIdentity,
    pub source_locator: String,
    pub source_record_blake3: String,
    pub source_has_model: bool,
    pub target_head_part: CreatureTargetRecordReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_texture_set: Option<CreatureTargetRecordReference>,
    pub target_valid_races: CreatureTargetRecordReference,
    pub artifacts: Vec<CreatureAncillaryNpcArtifactReceipt>,
    pub closure_blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcAppearanceProjection {
    pub target_race: CreatureTargetRecordReference,
    pub sex: CreatureAncillaryNpcSex,
    pub facegen_disposition: CreatureAncillaryNpcFacegenDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hair_color: Option<[u8; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_morph_symmetric: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_morph_asymmetric: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_texture_symmetry: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub head_parts: Vec<CreatureAncillaryNpcAppearancePartReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eye: Option<CreatureAncillaryNpcAppearancePartReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hair: Option<CreatureAncillaryNpcAppearancePartReceipt>,
    pub facegen_artifacts: Vec<CreatureAncillaryNpcArtifactReceipt>,
}

pub fn creature_ancillary_npc_morph_inputs_blake3(
    template: &CreatureAncillaryNpcTemplateReceipt,
) -> Result<String, CreatureAncillaryNpcError> {
    let fields = template
        .effective_fields
        .iter()
        .filter(|receipt| {
            matches!(
                receipt.field,
                CreatureAncillaryNpcEffectiveField::Race
                    | CreatureAncillaryNpcEffectiveField::Sex
                    | CreatureAncillaryNpcEffectiveField::HairColor
                    | CreatureAncillaryNpcEffectiveField::FaceMorphSymmetric
                    | CreatureAncillaryNpcEffectiveField::FaceMorphAsymmetric
                    | CreatureAncillaryNpcEffectiveField::FaceTextureSymmetry
                    | CreatureAncillaryNpcEffectiveField::LegacyHeadPart
                    | CreatureAncillaryNpcEffectiveField::Eye
                    | CreatureAncillaryNpcEffectiveField::Hair
            )
        })
        .collect::<Vec<_>>();
    let json = serde_json::to_string(&fields)
        .map_err(|error| CreatureAncillaryNpcError::Serialization(error.to_string()))?;
    Ok(blake3::hash(json.as_bytes()).to_hex().to_string())
}

pub fn creature_ancillary_npc_facegen_artifacts_blake3(
    artifacts: &[CreatureAncillaryNpcArtifactReceipt],
) -> Result<String, CreatureAncillaryNpcError> {
    let json = serde_json::to_string(artifacts)
        .map_err(|error| CreatureAncillaryNpcError::Serialization(error.to_string()))?;
    Ok(blake3::hash(json.as_bytes()).to_hex().to_string())
}

pub fn creature_ancillary_npc_assetless_facegen_proof_blake3(
    target_race: &CreatureTargetRecordReference,
    morph_inputs_blake3: &str,
    target_appearance_closure_blake3: &str,
) -> Result<String, CreatureAncillaryNpcError> {
    validate_hash(morph_inputs_blake3, "assetless facegen morph inputs")?;
    validate_hash(
        target_appearance_closure_blake3,
        "assetless target appearance closure",
    )?;
    let json = serde_json::to_string(&(
        target_race,
        morph_inputs_blake3,
        target_appearance_closure_blake3,
    ))
    .map_err(|error| CreatureAncillaryNpcError::Serialization(error.to_string()))?;
    Ok(blake3::hash(json.as_bytes()).to_hex().to_string())
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcProjectionReceipt {
    pub reservation: CreatureAncillaryNpcReservedIdentity,
    pub template: CreatureAncillaryNpcTemplateReceipt,
    pub appearance: CreatureAncillaryNpcAppearanceProjection,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcProjectionLedger {
    pub version: u32,
    pub target_plugin: String,
    pub projections: Vec<CreatureAncillaryNpcProjectionReceipt>,
    pub candidate_requests: Vec<CreatureAncillaryNpcRequestId>,
}

impl CreatureAncillaryNpcProjectionLedger {
    pub fn canonical_json(&self) -> Result<String, CreatureAncillaryNpcError> {
        self.validate_structure()?;
        serde_json::to_string(self)
            .map_err(|error| CreatureAncillaryNpcError::Serialization(error.to_string()))
    }

    pub fn stable_hash_blake3(&self) -> Result<String, CreatureAncillaryNpcError> {
        Ok(blake3::hash(self.canonical_json()?.as_bytes())
            .to_hex()
            .to_string())
    }

    pub fn validate_structure(&self) -> Result<(), CreatureAncillaryNpcError> {
        validate_ledger_structure(self)
    }

    pub fn canonical_target_artifacts(
        &self,
    ) -> Result<Vec<CreatureAncillaryNpcArtifactReceipt>, CreatureAncillaryNpcError> {
        self.validate_structure()?;
        let mut artifacts = BTreeMap::new();
        for artifact in ancillary_artifacts(self) {
            artifacts
                .entry(runtime_key(&artifact.target_data_path))
                .or_insert_with(|| artifact.clone());
        }
        Ok(artifacts.into_values().collect())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAncillaryNpcCommitLedger {
    pub version: u32,
    pub projection_ledger: CreatureAncillaryNpcProjectionLedger,
    pub projection_ledger_blake3: String,
    pub record_receipt: CreatureRecordBatchReceipt,
    pub record_receipt_blake3: String,
    pub target_artifacts: Vec<CreatureAncillaryNpcArtifactReceipt>,
}

impl CreatureAncillaryNpcCommitLedger {
    pub fn new(
        projection_ledger: CreatureAncillaryNpcProjectionLedger,
        record_receipt: CreatureRecordBatchReceipt,
    ) -> Result<Self, CreatureAncillaryNpcError> {
        let projection_ledger_blake3 = projection_ledger.stable_hash_blake3()?;
        let record_receipt_blake3 = hash_record_receipt(&record_receipt)?;
        let target_artifacts = projection_ledger.canonical_target_artifacts()?;
        let ledger = Self {
            version: CREATURE_ANCILLARY_NPC_COMMIT_LEDGER_VERSION,
            projection_ledger,
            projection_ledger_blake3,
            record_receipt,
            record_receipt_blake3,
            target_artifacts,
        };
        ledger.validate()?;
        Ok(ledger)
    }

    pub fn validate(&self) -> Result<(), CreatureAncillaryNpcError> {
        if self.version != CREATURE_ANCILLARY_NPC_COMMIT_LEDGER_VERSION {
            return Err(invalid(format!(
                "unsupported ancillary NPC commit ledger version {}",
                self.version
            )));
        }
        self.projection_ledger.validate_structure()?;
        if self.projection_ledger_blake3 != self.projection_ledger.stable_hash_blake3()? {
            return Err(invalid(
                "ancillary NPC projection ledger hash does not match",
            ));
        }
        if self.record_receipt_blake3 != hash_record_receipt(&self.record_receipt)? {
            return Err(invalid("ancillary NPC record receipt hash does not match"));
        }
        let expected_artifacts = self.projection_ledger.canonical_target_artifacts()?;
        if self.target_artifacts != expected_artifacts {
            return Err(invalid(
                "ancillary NPC target artifacts do not match the projection ledger",
            ));
        }
        validate_record_receipt_binding(&self.projection_ledger, &self.record_receipt)
    }

    pub fn canonical_json(&self) -> Result<String, CreatureAncillaryNpcError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CreatureAncillaryNpcError::Serialization(error.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Self, CreatureAncillaryNpcError> {
        let ledger: Self = serde_json::from_str(json)
            .map_err(|error| CreatureAncillaryNpcError::Serialization(error.to_string()))?;
        if ledger.canonical_json()? != json {
            return Err(invalid("ancillary NPC commit ledger JSON is not canonical"));
        }
        Ok(ledger)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CreatureAncillaryNpcError {
    #[error("invalid ancillary NPC projection: {0}")]
    Invalid(String),
    #[error("ancillary NPC projection serialization failed: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug)]
pub struct PreparedCreatureAncillaryNpcBatch {
    projection_ledger: CreatureAncillaryNpcProjectionLedger,
    record_family_batch: CreatureRecordFamilyBatch,
    staged_data_root: PathBuf,
}

impl PreparedCreatureAncillaryNpcBatch {
    pub fn projection_ledger(&self) -> &CreatureAncillaryNpcProjectionLedger {
        &self.projection_ledger
    }

    pub fn staged_data_root(&self) -> &Path {
        &self.staged_data_root
    }

    pub fn into_parts(
        self,
    ) -> (
        CreatureAncillaryNpcProjectionLedger,
        CreatureRecordFamilyBatch,
        PathBuf,
    ) {
        (
            self.projection_ledger,
            self.record_family_batch,
            self.staged_data_root,
        )
    }
}

pub fn prepare_creature_ancillary_npc_batch(
    expected_reservations: &[CreatureAncillaryNpcReservedIdentity],
    expected_candidate_requests: &[CreatureAncillaryNpcRequestId],
    projection_ledger: CreatureAncillaryNpcProjectionLedger,
    closures: Vec<CreatureRecordProjectionClosure>,
    source_data_roots: &BTreeMap<String, PathBuf>,
    staged_data_root: PathBuf,
) -> Result<PreparedCreatureAncillaryNpcBatch, CreatureAncillaryNpcError> {
    let record_family_batch = build_creature_ancillary_npc_family_batch(
        expected_reservations,
        expected_candidate_requests,
        &projection_ledger,
        closures,
    )?;
    validate_creature_ancillary_npc_artifacts(
        &projection_ledger,
        source_data_roots,
        &staged_data_root,
    )?;
    Ok(PreparedCreatureAncillaryNpcBatch {
        projection_ledger,
        record_family_batch,
        staged_data_root,
    })
}

pub fn build_creature_ancillary_npc_family_batch(
    expected_reservations: &[CreatureAncillaryNpcReservedIdentity],
    expected_candidate_requests: &[CreatureAncillaryNpcRequestId],
    ledger: &CreatureAncillaryNpcProjectionLedger,
    closures: Vec<CreatureRecordProjectionClosure>,
) -> Result<CreatureRecordFamilyBatch, CreatureAncillaryNpcError> {
    ledger.validate_structure()?;
    validate_exact_reservations(expected_reservations, ledger)?;
    validate_exact_requests(expected_candidate_requests, ledger)?;

    let projections = ledger
        .projections
        .iter()
        .map(|projection| (projection.reservation.source.stable_key(), projection))
        .collect::<BTreeMap<_, _>>();
    if closures.len() != projections.len() {
        return Err(invalid(format!(
            "received {} NPC record closures for {} projections",
            closures.len(),
            projections.len()
        )));
    }

    let mut ordered = BTreeMap::new();
    for closure in closures {
        let source_key = closure.source_primary_identity.stable_key();
        let projection = projections
            .get(&source_key)
            .ok_or_else(|| invalid(format!("unexpected NPC closure {source_key}")))?;
        validate_projection_closure(projection, &closure, &ledger.target_plugin)?;
        if ordered.insert(source_key.clone(), closure).is_some() {
            return Err(invalid(format!("duplicate NPC closure {source_key}")));
        }
    }
    let projected = ordered
        .values()
        .flat_map(|closure| &closure.projected_identities)
        .map(|identity| {
            (
                identity.signature.as_str(),
                identity.target_form_key.local,
                identity.target_form_key.plugin.to_ascii_lowercase(),
            )
        })
        .collect::<BTreeSet<_>>();
    for projection in &ledger.projections {
        for part in [
            projection.appearance.eye.as_ref(),
            projection.appearance.hair.as_ref(),
        ]
        .into_iter()
        .flatten()
        .chain(&projection.appearance.head_parts)
        {
            require_projected(&projected, &part.target_head_part)?;
            require_projected(&projected, &part.target_valid_races)?;
            if let Some(texture) = &part.target_texture_set {
                require_projected(&projected, texture)?;
            }
        }
    }

    let primary_mappings = ledger
        .projections
        .iter()
        .map(|projection| CreaturePrimaryRecordMapping {
            source: projection.reservation.source.clone(),
            target: projection.reservation.target_npc.clone(),
        })
        .collect();
    Ok(CreatureRecordFamilyBatch {
        family_id: CREATURE_ANCILLARY_NPC_FAMILY_ID.to_string(),
        closures: ordered.into_values().collect(),
        primary_mappings,
    })
}

pub fn validate_creature_ancillary_npc_artifacts(
    ledger: &CreatureAncillaryNpcProjectionLedger,
    source_data_roots: &BTreeMap<String, PathBuf>,
    staged_data_root: &Path,
) -> Result<(), CreatureAncillaryNpcError> {
    ledger.validate_structure()?;
    let mut targets = BTreeMap::<String, &CreatureAncillaryNpcArtifactReceipt>::new();
    for artifact in ancillary_artifacts(ledger) {
        for source in &artifact.source_artifacts {
            let source_root = source_data_roots
                .iter()
                .find_map(|(game, root)| {
                    game.eq_ignore_ascii_case(&source.source_game)
                        .then_some(root)
                })
                .ok_or_else(|| {
                    invalid(format!(
                        "missing source Data root for appearance game {:?}",
                        source.source_game
                    ))
                })?;
            validate_file_receipt(
                &source_root.join(path_from_runtime(&source.source_data_path)),
                source.byte_len,
                &source.blake3,
                "source appearance artifact",
            )?;
        }
        let target_path = staged_data_root.join(path_from_runtime(&artifact.target_data_path));
        let target_bytes = validate_file_receipt(
            &target_path,
            artifact.target_byte_len,
            &artifact.target_blake3,
            "target appearance artifact",
        )?;
        match artifact.kind {
            CreatureAncillaryNpcArtifactKind::Nif => {
                let nif = NifFile::from_bytes(&target_bytes, Some(target_path.clone())).map_err(
                    |error| invalid(format!("target appearance NIF does not parse: {error}")),
                )?;
                if nif.header.version != (20, 2, 0, 7)
                    || nif.header.user_version != 12
                    || nif.header.bs_version != 130
                {
                    return Err(invalid(format!(
                        "target appearance NIF {:?} is not Fallout 4",
                        artifact.target_data_path
                    )));
                }
            }
            CreatureAncillaryNpcArtifactKind::Bgsm => {
                materials_native::bgsm::parse(&target_bytes).map_err(|error| {
                    invalid(format!(
                        "target appearance BGSM {:?} does not parse: {error}",
                        artifact.target_data_path
                    ))
                })?;
            }
            CreatureAncillaryNpcArtifactKind::Bgem => {
                materials_native::bgem::parse(&target_bytes).map_err(|error| {
                    invalid(format!(
                        "target appearance BGEM {:?} does not parse: {error}",
                        artifact.target_data_path
                    ))
                })?;
            }
            CreatureAncillaryNpcArtifactKind::Tri | CreatureAncillaryNpcArtifactKind::Dds => {}
        }
        targets
            .entry(runtime_key(&artifact.target_data_path))
            .or_insert(artifact);
    }

    let available = targets.keys().cloned().collect::<BTreeSet<_>>();
    for artifact in targets.values() {
        let path = staged_data_root.join(path_from_runtime(&artifact.target_data_path));
        let required = match artifact.kind {
            CreatureAncillaryNpcArtifactKind::Nif => {
                let bytes = fs::read(&path).map_err(|error| {
                    invalid(format!(
                        "read target appearance artifact {}: {error}",
                        path.display()
                    ))
                })?;
                let nif = NifFile::from_bytes(&bytes, Some(path.clone())).map_err(|error| {
                    invalid(format!("target appearance NIF does not parse: {error}"))
                })?;
                let references = nif.referenced_asset_paths();
                references
                    .materials
                    .iter()
                    .chain(&references.textures)
                    .map(|path| runtime_key(path))
                    .collect::<BTreeSet<_>>()
            }
            CreatureAncillaryNpcArtifactKind::Bgsm | CreatureAncillaryNpcArtifactKind::Bgem => {
                crate::relocation::read_material_texture_paths(&path)
                    .into_iter()
                    .map(|path| runtime_key(&path))
                    .collect::<BTreeSet<_>>()
            }
            CreatureAncillaryNpcArtifactKind::Tri | CreatureAncillaryNpcArtifactKind::Dds => {
                BTreeSet::new()
            }
        };
        if !required.is_subset(&available) {
            return Err(invalid(format!(
                "target appearance artifact {:?} has unbound material/texture references {:?}",
                artifact.target_data_path,
                required.difference(&available).collect::<Vec<_>>()
            )));
        }
    }
    Ok(())
}

fn validate_ledger_structure(
    ledger: &CreatureAncillaryNpcProjectionLedger,
) -> Result<(), CreatureAncillaryNpcError> {
    if ledger.version != CREATURE_ANCILLARY_NPC_LEDGER_VERSION {
        return Err(invalid(format!(
            "unsupported ledger version {}; expected {}",
            ledger.version, CREATURE_ANCILLARY_NPC_LEDGER_VERSION
        )));
    }
    if ledger.target_plugin.trim().is_empty() || ledger.projections.is_empty() {
        return Err(invalid("target plugin and projections must be nonempty"));
    }
    let projection_keys = ledger
        .projections
        .iter()
        .map(|projection| projection.reservation.source.stable_key())
        .collect::<Vec<_>>();
    validate_sorted_unique(&projection_keys, "ancillary NPC projections")?;
    let reservation_map = ledger
        .projections
        .iter()
        .map(|projection| {
            (
                projection.reservation.source.stable_key(),
                &projection.reservation,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for projection in &ledger.projections {
        validate_projection(projection, &ledger.target_plugin, &reservation_map)?;
    }
    validate_global_artifact_dedup(ledger)?;
    let request_keys = ledger
        .candidate_requests
        .iter()
        .map(request_key)
        .collect::<Vec<_>>();
    validate_sorted_unique(&request_keys, "ancillary NPC candidate requests")?;
    for request in &ledger.candidate_requests {
        validate_source(&request.candidate, "candidate request owner")?;
        if !reservation_map.contains_key(&request.ancillary_npc.stable_key()) {
            return Err(invalid(format!(
                "candidate {} requests unreserved ancillary NPC {}",
                request.candidate.stable_key(),
                request.ancillary_npc.stable_key()
            )));
        }
    }
    Ok(())
}

fn hash_record_receipt(
    receipt: &CreatureRecordBatchReceipt,
) -> Result<String, CreatureAncillaryNpcError> {
    let json = receipt.canonical_json().map_err(|error| {
        invalid(format!(
            "canonicalize ancillary NPC record receipt: {error}"
        ))
    })?;
    Ok(blake3::hash(json.as_bytes()).to_hex().to_string())
}

fn validate_record_receipt_binding(
    projection_ledger: &CreatureAncillaryNpcProjectionLedger,
    receipt: &CreatureRecordBatchReceipt,
) -> Result<(), CreatureAncillaryNpcError> {
    if receipt.family_ids != [CREATURE_ANCILLARY_NPC_FAMILY_ID]
        || receipt.families.len() != 1
        || receipt.families[0].family_id != CREATURE_ANCILLARY_NPC_FAMILY_ID
    {
        return Err(invalid(
            "ancillary NPC record receipt must contain only the global ancillary family",
        ));
    }
    if receipt.mapping_count != projection_ledger.projections.len()
        || receipt.families[0].mapping_count != projection_ledger.projections.len()
        || receipt.record_count < projection_ledger.projections.len()
    {
        return Err(invalid(
            "ancillary NPC record receipt counts do not match its projections",
        ));
    }
    let mut expected = projection_ledger
        .projections
        .iter()
        .map(|projection| CreaturePrimaryRecordMapping {
            source: projection.reservation.source.clone(),
            target: projection.reservation.target_npc.clone(),
        })
        .collect::<Vec<_>>();
    expected.sort_by_key(|mapping| {
        (
            mapping.source.stable_key(),
            mapping.target.plugin.to_ascii_lowercase(),
            mapping.target.local,
        )
    });
    let mut actual = receipt.families[0].primary_mappings.clone();
    actual.sort_by_key(|mapping| {
        (
            mapping.source.stable_key(),
            mapping.target.plugin.to_ascii_lowercase(),
            mapping.target.local,
        )
    });
    if actual != expected {
        return Err(invalid(
            "ancillary NPC record receipt primary mappings do not match reservations",
        ));
    }
    Ok(())
}

fn validate_projection(
    projection: &CreatureAncillaryNpcProjectionReceipt,
    target_plugin: &str,
    reservations: &BTreeMap<String, &CreatureAncillaryNpcReservedIdentity>,
) -> Result<(), CreatureAncillaryNpcError> {
    let owner = &projection.reservation.source;
    validate_source(owner, "ancillary NPC reservation")?;
    validate_target_key(
        &projection.reservation.target_npc,
        target_plugin,
        "reserved NPC",
    )?;
    let chain = &projection.template.source_chain;
    if chain.is_empty() || chain[0] != *owner {
        return Err(invalid(format!(
            "NPC {} template chain must start with its owner",
            owner.stable_key()
        )));
    }
    let chain_keys = chain
        .iter()
        .map(SourceCreatureIdentity::stable_key)
        .collect::<Vec<_>>();
    let unique_chain = chain_keys.iter().collect::<BTreeSet<_>>();
    if chain_keys.len() != unique_chain.len() {
        return Err(invalid(format!(
            "NPC {} template chain contains a cycle",
            owner.stable_key()
        )));
    }
    for source in chain {
        validate_source(source, "template chain member")?;
        if !reservations.contains_key(&source.stable_key()) {
            return Err(invalid(format!(
                "NPC {} template chain member {} is not reserved",
                owner.stable_key(),
                source.stable_key()
            )));
        }
    }
    match (chain.get(1), &projection.template.target_template) {
        (None, None) => {}
        (Some(source), Some(target)) => {
            let expected = reservations[&source.stable_key()];
            if expected.target_npc != *target {
                return Err(invalid(format!(
                    "NPC {} target template does not match the direct source template reservation",
                    owner.stable_key()
                )));
            }
        }
        _ => {
            return Err(invalid(format!(
                "NPC {} source and target template receipts disagree",
                owner.stable_key()
            )));
        }
    }
    let fields = projection
        .template
        .effective_fields
        .iter()
        .map(|receipt| receipt.field)
        .collect::<Vec<_>>();
    let mut ordered_fields = fields.clone();
    ordered_fields.sort();
    let actual_fields = fields.iter().copied().collect::<BTreeSet<_>>();
    if fields != ordered_fields
        || fields.len() != actual_fields.len()
        || actual_fields != CreatureAncillaryNpcEffectiveField::all()
    {
        return Err(invalid(format!(
            "NPC {} effective fields do not exactly cover the ancillary actor contract",
            owner.stable_key()
        )));
    }
    for receipt in &projection.template.effective_fields {
        if !unique_chain.contains(&receipt.effective_source.stable_key())
            || receipt.source_locator.trim().is_empty()
        {
            return Err(invalid(format!(
                "NPC {} has an unbound effective {:?} field receipt",
                owner.stable_key(),
                receipt.field
            )));
        }
        validate_hash(&receipt.value_blake3, "effective field")?;
    }

    validate_reference(&projection.appearance.target_race, "RACE", target_plugin)?;
    validate_optional_source_bytes(
        projection.appearance.face_morph_symmetric.as_deref(),
        "symmetric face morph",
    )?;
    validate_optional_source_bytes(
        projection.appearance.face_morph_asymmetric.as_deref(),
        "asymmetric face morph",
    )?;
    validate_optional_source_bytes(
        projection.appearance.face_texture_symmetry.as_deref(),
        "face texture symmetry",
    )?;
    for artifact in &projection.appearance.facegen_artifacts {
        validate_artifact(artifact)?;
    }
    validate_artifact_set(&projection.appearance.facegen_artifacts, "facegen")?;
    match &projection.appearance.facegen_disposition {
        CreatureAncillaryNpcFacegenDisposition::Generated { generation } => {
            if !projection
                .appearance
                .facegen_artifacts
                .iter()
                .any(|artifact| artifact.kind == CreatureAncillaryNpcArtifactKind::Nif)
            {
                return Err(invalid(
                    "generated ancillary NPC facegen requires a target FaceGeom NIF receipt",
                ));
            }
            if generation.pipeline_id != CREATURE_ANCILLARY_NPC_FACEGEN_PIPELINE_ID {
                return Err(invalid(format!(
                    "generated ancillary NPC facegen uses unsupported pipeline {}",
                    generation.pipeline_id
                )));
            }
            validate_hash(
                &generation.pipeline_resources_blake3,
                "facegen pipeline resources",
            )?;
            validate_hash(
                &generation.target_appearance_closure_blake3,
                "target appearance closure",
            )?;
            let expected_morph_inputs =
                creature_ancillary_npc_morph_inputs_blake3(&projection.template)?;
            if generation.morph_inputs_blake3 != expected_morph_inputs {
                return Err(invalid(
                    "generated ancillary NPC facegen morph-input hash does not match effective source fields",
                ));
            }
            let expected_artifacts = creature_ancillary_npc_facegen_artifacts_blake3(
                &projection.appearance.facegen_artifacts,
            )?;
            if generation.artifacts_blake3 != expected_artifacts {
                return Err(invalid(
                    "generated ancillary NPC facegen receipt hash does not match its artifacts",
                ));
            }
        }
        CreatureAncillaryNpcFacegenDisposition::ProvenFo4RuntimeAssetless {
            morph_inputs_blake3,
            target_appearance_closure_blake3,
            proof_blake3,
        } => {
            if !projection.appearance.facegen_artifacts.is_empty() {
                return Err(invalid(
                    "assetless ancillary NPC facegen cannot carry generated artifacts",
                ));
            }
            let expected_morph_inputs =
                creature_ancillary_npc_morph_inputs_blake3(&projection.template)?;
            if morph_inputs_blake3 != &expected_morph_inputs {
                return Err(invalid(
                    "assetless ancillary NPC facegen morph-input hash does not match effective source fields",
                ));
            }
            let expected_proof = creature_ancillary_npc_assetless_facegen_proof_blake3(
                &projection.appearance.target_race,
                morph_inputs_blake3,
                target_appearance_closure_blake3,
            )?;
            if proof_blake3 != &expected_proof {
                return Err(invalid(
                    "assetless ancillary NPC FO4 runtime proof hash does not match",
                ));
            }
        }
    }
    match &projection.appearance.eye {
        Some(part) => validate_appearance_part(
            part,
            CreatureAncillaryNpcAppearancePartKind::Eye,
            owner,
            &unique_chain,
            target_plugin,
        )?,
        None => {}
    }
    match &projection.appearance.hair {
        Some(part) => validate_appearance_part(
            part,
            CreatureAncillaryNpcAppearancePartKind::Hair,
            owner,
            &unique_chain,
            target_plugin,
        )?,
        None => {}
    }
    let mut head_part_locators = BTreeSet::new();
    for (index, part) in projection.appearance.head_parts.iter().enumerate() {
        if !part.source_locator.contains(&format!("PNAM[{index}]"))
            || !head_part_locators.insert(part.source_locator.to_ascii_lowercase())
        {
            return Err(invalid(format!(
                "NPC {} legacy head-part receipts do not preserve exact PNAM order",
                owner.stable_key()
            )));
        }
        validate_appearance_part(
            part,
            CreatureAncillaryNpcAppearancePartKind::LegacyHeadPart,
            owner,
            &unique_chain,
            target_plugin,
        )?;
    }
    Ok(())
}

fn validate_appearance_part(
    part: &CreatureAncillaryNpcAppearancePartReceipt,
    expected_kind: CreatureAncillaryNpcAppearancePartKind,
    owner: &SourceCreatureIdentity,
    template_chain: &BTreeSet<&String>,
    target_plugin: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    if part.kind != expected_kind || part.owner_npc != *owner {
        return Err(invalid(format!(
            "NPC {} has a mismatched {:?} owner-fold receipt",
            owner.stable_key(),
            expected_kind
        )));
    }
    validate_source(&part.source_part, "appearance part")?;
    if !template_chain.contains(&part.effective_source.stable_key())
        || part.source_locator.trim().is_empty()
    {
        return Err(invalid(format!(
            "NPC {} {:?} receipt is not bound to its effective template source",
            owner.stable_key(),
            expected_kind
        )));
    }
    validate_hash(&part.source_record_blake3, "appearance source record")?;
    validate_hash(&part.closure_blake3, "appearance artifact closure")?;
    validate_reference(&part.target_head_part, "HDPT", target_plugin)?;
    validate_reference(&part.target_valid_races, "FLST", target_plugin)?;
    match (expected_kind, &part.target_texture_set) {
        (CreatureAncillaryNpcAppearancePartKind::Eye, Some(texture)) => {
            validate_reference(texture, "TXST", target_plugin)?;
        }
        (CreatureAncillaryNpcAppearancePartKind::Eye, None) => {
            return Err(invalid("eye owner-fold receipt requires a target TXST"));
        }
        (CreatureAncillaryNpcAppearancePartKind::Hair, Some(texture)) => {
            validate_reference(texture, "TXST", target_plugin)?;
        }
        (CreatureAncillaryNpcAppearancePartKind::Hair, None)
        | (CreatureAncillaryNpcAppearancePartKind::LegacyHeadPart, None) => {}
        (CreatureAncillaryNpcAppearancePartKind::LegacyHeadPart, Some(texture)) => {
            validate_reference(texture, "TXST", target_plugin)?;
        }
    }
    if part.artifacts.is_empty() && part.source_has_model {
        return Err(invalid("appearance part artifact closure is empty"));
    }
    for artifact in &part.artifacts {
        validate_artifact(artifact)?;
    }
    validate_artifact_set(&part.artifacts, "appearance part")?;
    if matches!(
        expected_kind,
        CreatureAncillaryNpcAppearancePartKind::Hair
            | CreatureAncillaryNpcAppearancePartKind::LegacyHeadPart
    ) && part.source_has_model
        && !part
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == CreatureAncillaryNpcArtifactKind::Nif)
    {
        return Err(invalid(
            "modeled hair/head-part owner-fold receipt lacks its converted NIF",
        ));
    }
    if expected_kind == CreatureAncillaryNpcAppearancePartKind::Eye
        && !part
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == CreatureAncillaryNpcArtifactKind::Dds)
    {
        return Err(invalid(
            "eye owner-fold receipt lacks its converted texture",
        ));
    }
    Ok(())
}

fn validate_projection_closure(
    projection: &CreatureAncillaryNpcProjectionReceipt,
    closure: &CreatureRecordProjectionClosure,
    target_plugin: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    if closure.source_primary_identity != projection.reservation.source
        || !closure
            .closure
            .target_plugin
            .eq_ignore_ascii_case(target_plugin)
    {
        return Err(invalid(format!(
            "NPC {} record closure owner/target does not match its projection",
            projection.reservation.source.stable_key()
        )));
    }
    let primary = closure
        .projected_identities
        .iter()
        .filter(|identity| identity.primary)
        .collect::<Vec<_>>();
    if primary.len() != 1
        || primary[0].signature != "NPC_"
        || primary[0].source_identity != projection.reservation.source
        || primary[0].target_form_key != projection.reservation.target_npc
    {
        return Err(invalid(format!(
            "NPC {} closure must have its exact reserved NPC_ as the sole primary",
            projection.reservation.source.stable_key()
        )));
    }
    if closure
        .projected_identities
        .iter()
        .any(|identity| identity.signature == "EYES" || identity.signature == "HAIR")
    {
        return Err(invalid(
            "EYES and HAIR are owner-context folds and cannot be projected records",
        ));
    }
    Ok(())
}

fn validate_global_artifact_dedup(
    ledger: &CreatureAncillaryNpcProjectionLedger,
) -> Result<(), CreatureAncillaryNpcError> {
    let mut targets = BTreeMap::<String, (CreatureAncillaryNpcArtifactKind, u64, String)>::new();
    for artifact in ancillary_artifacts(ledger) {
        let key = artifact.target_data_path.to_ascii_lowercase();
        let value = (
            artifact.kind,
            artifact.target_byte_len,
            artifact.target_blake3.to_ascii_lowercase(),
        );
        match targets.get(&key) {
            Some(existing) if existing != &value => {
                return Err(invalid(format!(
                    "appearance target path {:?} has conflicting artifact receipts",
                    artifact.target_data_path
                )));
            }
            Some(_) => {}
            None => {
                targets.insert(key, value);
            }
        }
    }
    Ok(())
}

fn ancillary_artifacts(
    ledger: &CreatureAncillaryNpcProjectionLedger,
) -> impl Iterator<Item = &CreatureAncillaryNpcArtifactReceipt> {
    ledger.projections.iter().flat_map(|projection| {
        projection
            .appearance
            .facegen_artifacts
            .iter()
            .chain(
                projection
                    .appearance
                    .eye
                    .iter()
                    .flat_map(|part| &part.artifacts),
            )
            .chain(
                projection
                    .appearance
                    .hair
                    .iter()
                    .flat_map(|part| &part.artifacts),
            )
            .chain(
                projection
                    .appearance
                    .head_parts
                    .iter()
                    .flat_map(|part| &part.artifacts),
            )
    })
}

fn validate_exact_reservations(
    expected: &[CreatureAncillaryNpcReservedIdentity],
    ledger: &CreatureAncillaryNpcProjectionLedger,
) -> Result<(), CreatureAncillaryNpcError> {
    let expected_keys = expected.iter().map(reservation_key).collect::<Vec<_>>();
    validate_sorted_unique(&expected_keys, "expected ancillary NPC reservations")?;
    let actual = ledger
        .projections
        .iter()
        .map(|projection| reservation_key(&projection.reservation))
        .collect::<Vec<_>>();
    if actual != expected_keys {
        return Err(invalid(
            "ancillary NPC projections do not exactly cover the immutable reservation slice",
        ));
    }
    Ok(())
}

fn validate_exact_requests(
    expected: &[CreatureAncillaryNpcRequestId],
    ledger: &CreatureAncillaryNpcProjectionLedger,
) -> Result<(), CreatureAncillaryNpcError> {
    let expected_keys = expected.iter().map(request_key).collect::<Vec<_>>();
    validate_sorted_unique(&expected_keys, "expected ancillary NPC requests")?;
    let actual = ledger
        .candidate_requests
        .iter()
        .map(request_key)
        .collect::<Vec<_>>();
    if actual != expected_keys {
        return Err(invalid(
            "ancillary NPC candidate requests do not exactly cover the dependency closure",
        ));
    }
    Ok(())
}

fn validate_artifact(
    artifact: &CreatureAncillaryNpcArtifactReceipt,
) -> Result<(), CreatureAncillaryNpcError> {
    if artifact.source_artifacts.is_empty() || artifact.target_byte_len == 0 {
        return Err(invalid("appearance artifact receipt is empty"));
    }
    validate_runtime_path(&artifact.target_data_path, artifact.kind)?;
    validate_hash(&artifact.target_blake3, "target appearance artifact")?;
    let source_keys = artifact
        .source_artifacts
        .iter()
        .map(|source| {
            (
                source.source_game.to_ascii_lowercase(),
                source.source_data_path.to_ascii_lowercase(),
                source.kind,
            )
        })
        .collect::<Vec<_>>();
    validate_sorted_unique(&source_keys, "appearance artifact source paths")?;
    for source in &artifact.source_artifacts {
        if source.byte_len == 0 {
            return Err(invalid("source appearance artifact is empty"));
        }
        if source.source_game.trim().is_empty() {
            return Err(invalid("source appearance artifact game is empty"));
        }
        validate_source_runtime_path(&source.source_data_path, source.kind)?;
        validate_hash(&source.blake3, "source appearance artifact")?;
    }
    Ok(())
}

fn validate_artifact_set(
    artifacts: &[CreatureAncillaryNpcArtifactReceipt],
    label: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    let keys = artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.target_data_path.to_ascii_lowercase(),
                artifact.kind,
            )
        })
        .collect::<Vec<_>>();
    validate_sorted_unique(&keys, &format!("{label} artifacts"))
}

fn validate_optional_source_bytes(
    value: Option<&[u8]>,
    label: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    if value.is_some_and(|value| value.is_empty()) {
        return Err(invalid(format!(
            "{label} must use None for source absence instead of an empty payload"
        )));
    }
    Ok(())
}

fn require_projected(
    projected: &BTreeSet<(&str, u32, String)>,
    reference: &CreatureTargetRecordReference,
) -> Result<(), CreatureAncillaryNpcError> {
    let key = (
        reference.signature.as_str(),
        reference.form_key.local,
        reference.form_key.plugin.to_ascii_lowercase(),
    );
    if !projected.contains(&key) {
        return Err(invalid(format!(
            "generated appearance record {} {:06X}@{} is absent from the NPC closure",
            reference.signature, reference.form_key.local, reference.form_key.plugin
        )));
    }
    Ok(())
}

fn validate_reference(
    reference: &CreatureTargetRecordReference,
    signature: &str,
    target_plugin: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    if reference.signature != signature {
        return Err(invalid(format!(
            "appearance reference has signature {:?}, expected {signature}",
            reference.signature
        )));
    }
    validate_target_key(&reference.form_key, target_plugin, signature)
}

fn validate_target_key(
    target: &TargetFormKey,
    target_plugin: &str,
    label: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    if target.local == 0
        || target.local > 0x00ff_ffff
        || !target.plugin.eq_ignore_ascii_case(target_plugin)
    {
        return Err(invalid(format!(
            "{label} has invalid target {:08X}@{:?}",
            target.local, target.plugin
        )));
    }
    Ok(())
}

fn validate_source(
    source: &SourceCreatureIdentity,
    label: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    if !source.is_valid() {
        return Err(invalid(format!("{label} is invalid")));
    }
    Ok(())
}

fn validate_hash(hash: &str, label: &str) -> Result<(), CreatureAncillaryNpcError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(format!(
            "{label} BLAKE3 must be 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_file_receipt(
    path: &Path,
    expected_len: u64,
    expected_hash: &str,
    label: &str,
) -> Result<Vec<u8>, CreatureAncillaryNpcError> {
    let bytes = fs::read(path)
        .map_err(|error| invalid(format!("read {label} {}: {error}", path.display())))?;
    let actual_hash = blake3::hash(&bytes).to_hex().to_string();
    if bytes.len() as u64 != expected_len || !actual_hash.eq_ignore_ascii_case(expected_hash) {
        return Err(invalid(format!(
            "{label} {} byte commitment does not match",
            path.display()
        )));
    }
    Ok(bytes)
}

fn runtime_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn path_from_runtime(path: &str) -> PathBuf {
    path.replace('/', "\\").split('\\').collect()
}

fn validate_runtime_path(
    path: &str,
    kind: CreatureAncillaryNpcArtifactKind,
) -> Result<(), CreatureAncillaryNpcError> {
    let canonical = path.replace('/', "\\");
    if canonical.trim().is_empty()
        || canonical.starts_with('\\')
        || canonical.contains(':')
        || canonical
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid(format!("invalid appearance data path {path:?}")));
    }
    let extension = canonical
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let expected = match kind {
        CreatureAncillaryNpcArtifactKind::Nif => "nif",
        CreatureAncillaryNpcArtifactKind::Tri => "tri",
        CreatureAncillaryNpcArtifactKind::Bgsm => "bgsm",
        CreatureAncillaryNpcArtifactKind::Bgem => "bgem",
        CreatureAncillaryNpcArtifactKind::Dds => "dds",
    };
    if extension.as_deref() != Some(expected) {
        return Err(invalid(format!(
            "appearance artifact path {path:?} does not match {kind:?}"
        )));
    }
    Ok(())
}

fn validate_source_runtime_path(
    path: &str,
    kind: CreatureAncillaryNpcSourceArtifactKind,
) -> Result<(), CreatureAncillaryNpcError> {
    let canonical = path.replace('/', "\\");
    if canonical.trim().is_empty()
        || canonical.starts_with('\\')
        || canonical.contains(':')
        || canonical
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid(format!(
            "invalid source appearance data path {path:?}"
        )));
    }
    let extension = canonical
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let expected = match kind {
        CreatureAncillaryNpcSourceArtifactKind::Nif => "nif",
        CreatureAncillaryNpcSourceArtifactKind::Tri => "tri",
        CreatureAncillaryNpcSourceArtifactKind::Egm => "egm",
        CreatureAncillaryNpcSourceArtifactKind::Egt => "egt",
        CreatureAncillaryNpcSourceArtifactKind::Bgsm => "bgsm",
        CreatureAncillaryNpcSourceArtifactKind::Bgem => "bgem",
        CreatureAncillaryNpcSourceArtifactKind::Dds => "dds",
    };
    if extension.as_deref() != Some(expected) {
        return Err(invalid(format!(
            "source appearance artifact path {path:?} does not match {kind:?}"
        )));
    }
    Ok(())
}

fn validate_sorted_unique<T: Ord + std::fmt::Debug>(
    values: &[T],
    label: &str,
) -> Result<(), CreatureAncillaryNpcError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid(format!("{label} are not sorted and unique")));
    }
    Ok(())
}

fn reservation_key(reservation: &CreatureAncillaryNpcReservedIdentity) -> (String, String, u32) {
    (
        reservation.source.stable_key(),
        reservation.target_npc.plugin.to_ascii_lowercase(),
        reservation.target_npc.local,
    )
}

fn request_key(request: &CreatureAncillaryNpcRequestId) -> (String, String) {
    (
        request.candidate.stable_key(),
        request.ancillary_npc.stable_key(),
    )
}

fn invalid(message: impl Into<String>) -> CreatureAncillaryNpcError {
    CreatureAncillaryNpcError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::record::Record;
    use crate::source_rig::{
        CreatureRecordClosure, CreatureRecordFamilyReceipt, ProjectedRecordIdentity,
    };
    use crate::sym::StringInterner;

    fn source(local_form_id: u32) -> SourceCreatureIdentity {
        SourceCreatureIdentity {
            namespace: "fnv".to_string(),
            plugin: "FalloutNV.esm".to_string(),
            local_form_id,
        }
    }

    fn target(local: u32) -> TargetFormKey {
        TargetFormKey::new(local, "Output.esm")
    }

    fn reference(signature: &str, local: u32) -> CreatureTargetRecordReference {
        CreatureTargetRecordReference {
            signature: signature.to_string(),
            form_key: target(local),
        }
    }

    fn artifact() -> CreatureAncillaryNpcArtifactReceipt {
        CreatureAncillaryNpcArtifactReceipt {
            kind: CreatureAncillaryNpcArtifactKind::Dds,
            source_artifacts: vec![CreatureAncillaryNpcSourceArtifactReceipt {
                kind: CreatureAncillaryNpcSourceArtifactKind::Dds,
                source_game: "fnv".to_string(),
                source_data_path: "textures\\eyes\\blue.dds".to_string(),
                byte_len: 4,
                blake3: "1".repeat(64),
            }],
            target_data_path: "textures\\actors\\legacy\\blue.dds".to_string(),
            target_byte_len: 4,
            target_blake3: "2".repeat(64),
        }
    }

    fn projection(
        owner: SourceCreatureIdentity,
        target_npc: u32,
    ) -> CreatureAncillaryNpcProjectionReceipt {
        let effective_fields = CreatureAncillaryNpcEffectiveField::all()
            .into_iter()
            .map(|field| CreatureAncillaryNpcEffectiveFieldReceipt {
                field,
                effective_source: owner.clone(),
                source_locator: format!("NPC_.{field:?}"),
                value_blake3: "0".repeat(64),
            })
            .collect();
        let template = CreatureAncillaryNpcTemplateReceipt {
            source_chain: vec![owner.clone()],
            target_template: None,
            effective_fields,
        };
        let target_race = reference("RACE", 0x500);
        let morph_inputs_blake3 = creature_ancillary_npc_morph_inputs_blake3(&template).unwrap();
        let target_appearance_closure_blake3 = "8".repeat(64);
        let proof_blake3 = creature_ancillary_npc_assetless_facegen_proof_blake3(
            &target_race,
            &morph_inputs_blake3,
            &target_appearance_closure_blake3,
        )
        .unwrap();
        CreatureAncillaryNpcProjectionReceipt {
            reservation: CreatureAncillaryNpcReservedIdentity {
                source: owner.clone(),
                target_npc: target(target_npc),
            },
            template,
            appearance: CreatureAncillaryNpcAppearanceProjection {
                target_race,
                sex: CreatureAncillaryNpcSex::Male,
                facegen_disposition:
                    CreatureAncillaryNpcFacegenDisposition::ProvenFo4RuntimeAssetless {
                        morph_inputs_blake3,
                        target_appearance_closure_blake3,
                        proof_blake3,
                    },
                hair_color: Some([1, 2, 3, 4]),
                face_morph_symmetric: Some(vec![1]),
                face_morph_asymmetric: Some(vec![2]),
                face_texture_symmetry: Some(vec![3]),
                head_parts: vec![CreatureAncillaryNpcAppearancePartReceipt {
                    kind: CreatureAncillaryNpcAppearancePartKind::LegacyHeadPart,
                    owner_npc: owner.clone(),
                    effective_source: owner.clone(),
                    source_part: source(0x901),
                    source_locator: "NPC_.PNAM[0]".to_string(),
                    source_record_blake3: "6".repeat(64),
                    source_has_model: false,
                    target_head_part: reference("HDPT", 0x203),
                    target_texture_set: None,
                    target_valid_races: reference("FLST", 0x202),
                    artifacts: Vec::new(),
                    closure_blake3: "7".repeat(64),
                }],
                eye: Some(CreatureAncillaryNpcAppearancePartReceipt {
                    kind: CreatureAncillaryNpcAppearancePartKind::Eye,
                    owner_npc: owner.clone(),
                    effective_source: owner,
                    source_part: source(0x900),
                    source_locator: "NPC_.ENAM[0]".to_string(),
                    source_record_blake3: "3".repeat(64),
                    source_has_model: false,
                    target_head_part: reference("HDPT", 0x200),
                    target_texture_set: Some(reference("TXST", 0x201)),
                    target_valid_races: reference("FLST", 0x202),
                    artifacts: vec![artifact()],
                    closure_blake3: "4".repeat(64),
                }),
                hair: None,
                facegen_artifacts: Vec::new(),
            },
        }
    }

    fn set_generated_facegen(projection: &mut CreatureAncillaryNpcProjectionReceipt) {
        let generation = creature_ancillary_npc_facegen_generation_receipt(
            &projection.template,
            "9".repeat(64),
            "8".repeat(64),
            &projection.appearance.facegen_artifacts,
        )
        .unwrap();
        projection.appearance.facegen_disposition =
            CreatureAncillaryNpcFacegenDisposition::Generated { generation };
    }

    fn closure(
        interner: &StringInterner,
        owner: SourceCreatureIdentity,
        target_npc: u32,
        include_shared_parts: bool,
    ) -> CreatureRecordProjectionClosure {
        let plugin = interner.intern("Output.esm");
        let mut projected_identities = vec![ProjectedRecordIdentity {
            signature: "NPC_".to_string(),
            source_identity: owner.clone(),
            target_form_key: target(target_npc),
            primary: true,
        }];
        let mut records = vec![Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey {
                local: target_npc,
                plugin,
            },
        )];
        if include_shared_parts {
            for (signature, local) in [
                ("HDPT", 0x200),
                ("TXST", 0x201),
                ("FLST", 0x202),
                ("HDPT", 0x203),
            ] {
                projected_identities.push(ProjectedRecordIdentity {
                    signature: signature.to_string(),
                    source_identity: source(0x900),
                    target_form_key: target(local),
                    primary: false,
                });
                records.push(Record::new(
                    SigCode::from_str(signature).unwrap(),
                    FormKey { local, plugin },
                ));
            }
        }
        CreatureRecordProjectionClosure {
            source_primary_identity: owner,
            projected_identities,
            required_target_records: Vec::new(),
            closure: CreatureRecordClosure {
                target_plugin: "Output.esm".to_string(),
                records,
            },
        }
    }

    fn fixture() -> (
        Vec<CreatureAncillaryNpcReservedIdentity>,
        Vec<CreatureAncillaryNpcRequestId>,
        CreatureAncillaryNpcProjectionLedger,
        Vec<CreatureRecordProjectionClosure>,
    ) {
        let interner = StringInterner::new();
        let first = source(0x100);
        let second = source(0x101);
        let projections = vec![
            projection(first.clone(), 0x100),
            projection(second.clone(), 0x101),
        ];
        let reservations = projections
            .iter()
            .map(|projection| projection.reservation.clone())
            .collect();
        let requests = vec![
            CreatureAncillaryNpcRequestId {
                candidate: source(0x10),
                ancillary_npc: first.clone(),
            },
            CreatureAncillaryNpcRequestId {
                candidate: source(0x10),
                ancillary_npc: second.clone(),
            },
        ];
        let ledger = CreatureAncillaryNpcProjectionLedger {
            version: CREATURE_ANCILLARY_NPC_LEDGER_VERSION,
            target_plugin: "Output.esm".to_string(),
            projections,
            candidate_requests: requests.clone(),
        };
        let closures = vec![
            closure(&interner, first, 0x100, true),
            closure(&interner, second, 0x101, false),
        ];
        (reservations, requests, ledger, closures)
    }

    fn record_receipt(
        reservations: &[CreatureAncillaryNpcReservedIdentity],
    ) -> CreatureRecordBatchReceipt {
        let primary_mappings = reservations
            .iter()
            .map(|reservation| CreaturePrimaryRecordMapping {
                source: reservation.source.clone(),
                target: reservation.target_npc.clone(),
            })
            .collect::<Vec<_>>();
        CreatureRecordBatchReceipt {
            family_ids: vec![CREATURE_ANCILLARY_NPC_FAMILY_ID.to_string()],
            families: vec![CreatureRecordFamilyReceipt {
                family_id: CREATURE_ANCILLARY_NPC_FAMILY_ID.to_string(),
                record_count: 6,
                mapping_count: primary_mappings.len(),
                primary_mappings,
                races: Vec::new(),
            }],
            record_count: 6,
            mapping_count: reservations.len(),
            reserved_form_keys: reservations
                .iter()
                .map(|reservation| reservation.target_npc.clone())
                .collect(),
        }
    }

    #[test]
    fn ancillary_npc_batch_exactly_covers_reservations_and_dedupes_shared_parts() {
        let (reservations, requests, ledger, closures) = fixture();
        let first = build_creature_ancillary_npc_family_batch(
            &reservations,
            &requests,
            &ledger,
            closures.clone(),
        )
        .unwrap();
        let second = build_creature_ancillary_npc_family_batch(
            &reservations,
            &requests,
            &ledger,
            closures.into_iter().rev().collect(),
        )
        .unwrap();
        assert_eq!(first.family_id, CREATURE_ANCILLARY_NPC_FAMILY_ID);
        assert_eq!(first.primary_mappings, second.primary_mappings);
        assert_eq!(first.closures.len(), 2);
        assert_eq!(
            first
                .closures
                .iter()
                .flat_map(|closure| &closure.projected_identities)
                .filter(|identity| identity.signature == "HDPT")
                .count(),
            2
        );
        assert!(first.closures.iter().all(|closure| {
            closure
                .projected_identities
                .iter()
                .all(|identity| identity.signature != "EYES" && identity.signature != "HAIR")
        }));
    }

    #[test]
    fn ancillary_npc_commit_ledger_binds_projection_records_and_artifacts() {
        let (reservations, _, ledger, _) = fixture();
        let committed =
            CreatureAncillaryNpcCommitLedger::new(ledger.clone(), record_receipt(&reservations))
                .unwrap();
        let json = committed.canonical_json().unwrap();
        assert_eq!(
            CreatureAncillaryNpcCommitLedger::from_json(&json).unwrap(),
            committed
        );

        let mut wrong_mapping = committed.clone();
        wrong_mapping.record_receipt.families[0].primary_mappings[0]
            .target
            .local += 1;
        assert!(wrong_mapping.validate().is_err());

        let mut wrong_artifact = committed;
        wrong_artifact.target_artifacts[0].target_blake3 = "9".repeat(64);
        assert!(wrong_artifact.validate().is_err());
    }

    #[test]
    fn ancillary_npc_batch_rejects_missing_reservations_and_requests() {
        let (mut reservations, requests, ledger, closures) = fixture();
        reservations.pop();
        assert!(
            build_creature_ancillary_npc_family_batch(
                &reservations,
                &requests,
                &ledger,
                closures.clone(),
            )
            .is_err()
        );

        let (reservations, mut requests, ledger, closures) = fixture();
        requests.pop();
        assert!(
            build_creature_ancillary_npc_family_batch(&reservations, &requests, &ledger, closures,)
                .is_err()
        );
    }

    #[test]
    fn ancillary_npc_batch_rejects_template_cycles_and_artifact_collisions() {
        let (reservations, requests, mut ledger, closures) = fixture();
        let owner = ledger.projections[0].reservation.source.clone();
        ledger.projections[0].template.source_chain.push(owner);
        assert!(
            build_creature_ancillary_npc_family_batch(
                &reservations,
                &requests,
                &ledger,
                closures.clone(),
            )
            .is_err()
        );

        let (reservations, requests, mut ledger, closures) = fixture();
        ledger.projections[1]
            .appearance
            .eye
            .as_mut()
            .unwrap()
            .artifacts[0]
            .target_blake3 = "5".repeat(64);
        assert!(
            build_creature_ancillary_npc_family_batch(&reservations, &requests, &ledger, closures,)
                .is_err()
        );
    }

    #[test]
    fn ancillary_npc_batch_rejects_standalone_legacy_appearance_mapping() {
        let (reservations, requests, ledger, mut closures) = fixture();
        closures[0].projected_identities[0].signature = "EYES".to_string();
        assert!(
            build_creature_ancillary_npc_family_batch(&reservations, &requests, &ledger, closures,)
                .is_err()
        );
    }

    #[test]
    fn ancillary_npc_facegen_requires_generated_artifacts_or_runtime_proof() {
        let (reservations, requests, mut ledger, closures) = fixture();
        let CreatureAncillaryNpcFacegenDisposition::ProvenFo4RuntimeAssetless {
            proof_blake3, ..
        } = &mut ledger.projections[0].appearance.facegen_disposition
        else {
            panic!("fixture must use the assetless proof path");
        };
        *proof_blake3 = "f".repeat(64);
        assert!(
            build_creature_ancillary_npc_family_batch(
                &reservations,
                &requests,
                &ledger,
                closures.clone(),
            )
            .unwrap_err()
            .to_string()
            .contains("runtime proof")
        );

        let (reservations, requests, mut ledger, closures) = fixture();
        set_generated_facegen(&mut ledger.projections[0]);
        assert!(
            build_creature_ancillary_npc_family_batch(&reservations, &requests, &ledger, closures,)
                .unwrap_err()
                .to_string()
                .contains("FaceGeom NIF")
        );
    }

    #[test]
    fn ancillary_npc_artifacts_are_byte_bound_and_target_nifs_are_fo4() {
        let (reservations, requests, mut ledger, closures) = fixture();
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let staged_root = temp.path().join("staged");
        let source_dds = [1u8, 2, 3, 4];
        let target_dds = [5u8, 6, 7, 8];
        let source_dds_path = source_root.join(path_from_runtime("textures\\eyes\\blue.dds"));
        let target_dds_path =
            staged_root.join(path_from_runtime("textures\\actors\\legacy\\blue.dds"));
        fs::create_dir_all(source_dds_path.parent().unwrap()).unwrap();
        fs::create_dir_all(target_dds_path.parent().unwrap()).unwrap();
        fs::write(&source_dds_path, source_dds).unwrap();
        fs::write(&target_dds_path, target_dds).unwrap();
        let source_dds_hash = blake3::hash(&source_dds).to_hex().to_string();
        let target_dds_hash = blake3::hash(&target_dds).to_hex().to_string();
        for projection in &mut ledger.projections {
            let artifact = &mut projection.appearance.eye.as_mut().unwrap().artifacts[0];
            artifact.source_artifacts[0].blake3 = source_dds_hash.clone();
            artifact.target_blake3 = target_dds_hash.clone();
        }

        let source_nif = NifFile::new("fnv").to_bytes().unwrap();
        let target_nif = NifFile::new("fo4").to_bytes().unwrap();
        let source_nif_path = source_root.join(path_from_runtime("meshes\\legacy\\head.nif"));
        let target_nif_path = staged_root.join(path_from_runtime(
            "meshes\\actors\\character\\facegendata\\facegeom\\Output.esm\\00000100.nif",
        ));
        fs::create_dir_all(source_nif_path.parent().unwrap()).unwrap();
        fs::create_dir_all(target_nif_path.parent().unwrap()).unwrap();
        fs::write(&source_nif_path, &source_nif).unwrap();
        fs::write(&target_nif_path, &target_nif).unwrap();
        ledger.projections[0].appearance.facegen_artifacts.push(
            CreatureAncillaryNpcArtifactReceipt {
                kind: CreatureAncillaryNpcArtifactKind::Nif,
                source_artifacts: vec![CreatureAncillaryNpcSourceArtifactReceipt {
                    kind: CreatureAncillaryNpcSourceArtifactKind::Nif,
                    source_game: "fnv".to_string(),
                    source_data_path: "meshes\\legacy\\head.nif".to_string(),
                    byte_len: source_nif.len() as u64,
                    blake3: blake3::hash(&source_nif).to_hex().to_string(),
                }],
                target_data_path:
                    "meshes\\actors\\character\\facegendata\\facegeom\\Output.esm\\00000100.nif"
                        .to_string(),
                target_byte_len: target_nif.len() as u64,
                target_blake3: blake3::hash(&target_nif).to_hex().to_string(),
            },
        );
        set_generated_facegen(&mut ledger.projections[0]);
        let source_roots = BTreeMap::from([("fnv".to_string(), source_root)]);
        validate_creature_ancillary_npc_artifacts(&ledger, &source_roots, &staged_root).unwrap();
        let prepared = prepare_creature_ancillary_npc_batch(
            &reservations,
            &requests,
            ledger.clone(),
            closures,
            &source_roots,
            staged_root.clone(),
        )
        .unwrap();
        assert_eq!(
            prepared.projection_ledger().projections.len(),
            reservations.len()
        );

        let mut mismatched_generation = ledger.clone();
        let CreatureAncillaryNpcFacegenDisposition::Generated { generation } =
            &mut mismatched_generation.projections[0]
                .appearance
                .facegen_disposition
        else {
            panic!("fixture must use generated FaceGen");
        };
        generation.morph_inputs_blake3 = "f".repeat(64);
        assert!(
            validate_creature_ancillary_npc_artifacts(
                &mismatched_generation,
                &source_roots,
                &staged_root,
            )
            .unwrap_err()
            .to_string()
            .contains("morph-input hash")
        );

        fs::write(&target_nif_path, &source_nif).unwrap();
        let artifact = &mut ledger.projections[0].appearance.facegen_artifacts[0];
        artifact.target_byte_len = source_nif.len() as u64;
        artifact.target_blake3 = blake3::hash(&source_nif).to_hex().to_string();
        set_generated_facegen(&mut ledger.projections[0]);
        assert!(
            validate_creature_ancillary_npc_artifacts(&ledger, &source_roots, &staged_root)
                .unwrap_err()
                .to_string()
                .contains("is not Fallout 4")
        );
    }
}
