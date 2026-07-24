//! FNV/FO3 CREA architecture and FO4 RACE donor policy.
//!
//! Classification uses normalized asset architecture only. Legacy CREA `RNAM` is attack reach,
//! never a RACE reference, and is always reported for removal.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::Game;

pub const EXPECTED_FULL_MERGED_CREA_CANDIDATES: usize = 2_683;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyCreatureFamily {
    Fnv,
    Fo3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureArchitecture {
    Deathclaw,
    Dog,
    Mirelurk,
    Molerat,
    Radscorpion,
    YaoGuai,
    FeralGhoul,
    Bloatfly,
    Cazador,
    Stingwing,
    Gecko,
    Nightstalker,
    Robot,
    Humanoid,
    Unknown,
}

impl CreatureArchitecture {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Deathclaw => "deathclaw",
            Self::Dog => "dog",
            Self::Mirelurk => "mirelurk",
            Self::Molerat => "molerat",
            Self::Radscorpion => "radscorpion",
            Self::YaoGuai => "yao_guai",
            Self::FeralGhoul => "feral_ghoul",
            Self::Bloatfly => "bloatfly",
            Self::Cazador => "cazador",
            Self::Stingwing => "stingwing",
            Self::Gecko => "gecko",
            Self::Nightstalker => "nightstalker",
            Self::Robot => "robot",
            Self::Humanoid => "humanoid",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedCreatureReason {
    WrongRecordType,
    ConflictingArchitectureEvidence,
    HumanoidCreatureHasNoAuditedFo4RaceDonor,
    CazadorHasNoAuditedFo4RaceDonor,
    StingwingHasNoAuditedLegacyArchitecture,
    GeckoRetargetIsNotAnAuditedRaceDonor,
    NightstalkerRetargetIsNotAnAuditedRaceDonor,
    RobotRacePolicyNotAudited,
    UnknownArchitecture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum LegacyRnamPolicy {
    Absent,
    AttackReachMustDrop { raw: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct CreatureRaceEvidence<'a> {
    pub family: LegacyCreatureFamily,
    pub source_plugin: &'a str,
    pub source_record_type: &'a str,
    pub model_path: Option<&'a str>,
    pub skeleton_path: Option<&'a str>,
    pub behavior_path: Option<&'a str>,
    pub legacy_rnam: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatureRacePolicy {
    AuditedDonor {
        architecture: CreatureArchitecture,
        race: FormKey,
    },
    Unsupported {
        architecture: CreatureArchitecture,
        reason: UnsupportedCreatureReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureRaceDecision {
    pub family: LegacyCreatureFamily,
    pub source_plugin: String,
    pub normalized_model: Option<String>,
    pub normalized_skeleton: Option<String>,
    pub normalized_behavior: Option<String>,
    pub legacy_rnam: LegacyRnamPolicy,
    pub policy: CreatureRacePolicy,
}

impl CreatureRaceDecision {
    pub fn audited_race(&self) -> Result<Option<FormKey>, UnsupportedCreatureReason> {
        match self.policy {
            CreatureRacePolicy::AuditedDonor { race, .. } => Ok(Some(race)),
            CreatureRacePolicy::Unsupported { reason, .. } => Err(reason),
        }
    }

    pub(crate) fn architecture(&self) -> CreatureArchitecture {
        match self.policy {
            CreatureRacePolicy::AuditedDonor { architecture, .. }
            | CreatureRacePolicy::Unsupported { architecture, .. } => architecture,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CreatureRaceGateEvent {
    pub decision: CreatureRaceDecision,
    pub source_form_id: u32,
    pub editor_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CreatureRaceGateDiagnostic {
    pub source_plugin: String,
    pub source_form_id: String,
    pub editor_id: Option<String>,
    pub model_path: Option<String>,
    pub skeleton_path: Option<String>,
    pub behavior_path: Option<String>,
    pub architecture: CreatureArchitecture,
    pub reason: UnsupportedCreatureReason,
    pub coverage_reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CreatureRaceGateError {
    pub decision: CreatureRaceDecision,
    pub diagnostic: CreatureRaceGateDiagnostic,
}

impl std::fmt::Display for CreatureRaceGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let diagnostic = serde_json::to_string(&self.diagnostic).map_err(|_| std::fmt::Error)?;
        write!(f, "legacy_creature_race_gate:{diagnostic}")
    }
}

impl std::error::Error for CreatureRaceGateError {}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CreatureRaceTargetValidationError {
    pub source_plugin: String,
    pub source_form_id: String,
    pub editor_id: Option<String>,
    pub model_path: Option<String>,
    pub skeleton_path: Option<String>,
    pub behavior_path: Option<String>,
    pub reason: String,
    pub coverage_reason: String,
}

impl std::fmt::Display for CreatureRaceTargetValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let diagnostic = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        write!(f, "legacy_creature_race_target_validation:{diagnostic}")
    }
}

impl std::error::Error for CreatureRaceTargetValidationError {}

pub fn classify_legacy_creature_race(
    evidence: &CreatureRaceEvidence<'_>,
    interner: &StringInterner,
) -> CreatureRaceDecision {
    let normalized_model = evidence.model_path.map(normalize_asset_path);
    let normalized_skeleton = evidence.skeleton_path.map(normalize_asset_path);
    let normalized_behavior = evidence.behavior_path.map(normalize_asset_path);
    let legacy_rnam = match evidence.legacy_rnam {
        Some(bytes) => LegacyRnamPolicy::AttackReachMustDrop {
            raw: bytes.to_vec(),
        },
        None => LegacyRnamPolicy::Absent,
    };

    let policy = if evidence.source_record_type != "CREA" {
        CreatureRacePolicy::Unsupported {
            architecture: CreatureArchitecture::Unknown,
            reason: UnsupportedCreatureReason::WrongRecordType,
        }
    } else {
        let architectures = [
            normalized_model.as_deref(),
            normalized_skeleton.as_deref(),
            normalized_behavior.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter_map(architecture_from_path)
        .collect::<Vec<_>>();
        let architecture = architectures
            .first()
            .copied()
            .unwrap_or(CreatureArchitecture::Unknown);
        if architectures
            .iter()
            .any(|candidate| *candidate != architecture)
        {
            CreatureRacePolicy::Unsupported {
                architecture,
                reason: UnsupportedCreatureReason::ConflictingArchitectureEvidence,
            }
        } else {
            policy_for_architecture(architecture, interner)
        }
    };

    CreatureRaceDecision {
        family: evidence.family,
        source_plugin: evidence.source_plugin.to_owned(),
        normalized_model,
        normalized_skeleton,
        normalized_behavior,
        legacy_rnam,
        policy,
    }
}

pub(crate) fn apply_legacy_creature_race_policy(
    source: Game,
    target: Game,
    record: &mut Record,
    interner: &StringInterner,
) -> Result<Option<CreatureRaceGateEvent>, CreatureRaceGateError> {
    if !matches!(source, Game::Fnv | Game::Fo3)
        || target != Game::Fo4
        || record.sig.as_str() != "CREA"
    {
        return Ok(None);
    }

    let family = match source {
        Game::Fnv => LegacyCreatureFamily::Fnv,
        Game::Fo3 => LegacyCreatureFamily::Fo3,
        _ => unreachable!("legacy creature gate is isolated above"),
    };
    let source_plugin = interner
        .resolve(record.form_key.plugin)
        .unwrap_or("<unresolved-plugin>")
        .to_owned();
    let editor_id = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .map(str::to_owned);
    let model_path = first_asset_path(record, interner, |field, path| {
        field == "MODL" && path.to_ascii_lowercase().ends_with(".nif")
    });
    let skeleton_path = first_asset_path(record, interner, |_, path| {
        path.to_ascii_lowercase().contains("skeleton")
    });
    let behavior_path = first_asset_path(record, interner, |_, path| {
        let lower = path.to_ascii_lowercase();
        lower.ends_with(".hkx") || lower.contains("behavior")
    });
    let legacy_rnam = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"RNAM")
        .map(|field| legacy_rnam_bytes(&field.value));

    record.fields.retain(|field| field.sig.0 != *b"RNAM");

    let evidence = CreatureRaceEvidence {
        family,
        source_plugin: &source_plugin,
        source_record_type: "CREA",
        model_path: model_path.as_deref(),
        skeleton_path: skeleton_path.as_deref(),
        behavior_path: behavior_path.as_deref(),
        legacy_rnam: legacy_rnam.as_deref(),
    };
    let decision = classify_legacy_creature_race(&evidence, interner);
    let event = CreatureRaceGateEvent {
        decision: decision.clone(),
        source_form_id: record.form_key.local,
        editor_id: editor_id.clone(),
    };

    match decision.policy.clone() {
        CreatureRacePolicy::AuditedDonor { race, .. } => {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"RNAM"),
                value: FieldValue::FormKey(race),
            });
            Ok(Some(event))
        }
        CreatureRacePolicy::Unsupported {
            architecture,
            reason,
        } => Err(CreatureRaceGateError {
            decision,
            diagnostic: CreatureRaceGateDiagnostic {
                source_plugin,
                source_form_id: format!("{:06X}", record.form_key.local),
                editor_id,
                model_path,
                skeleton_path,
                behavior_path,
                architecture,
                reason,
                coverage_reason: format!(
                    "unsupported_or_unresolved_architecture:{}",
                    architecture.label()
                ),
            },
        }),
    }
}

pub(crate) fn validate_crea_derived_npc_race(
    record: &Record,
    event: Option<&CreatureRaceGateEvent>,
    interner: &StringInterner,
    resolves_to_race: impl Fn(FormKey) -> bool,
) -> Result<(), CreatureRaceTargetValidationError> {
    let Some(event) = event else {
        return Ok(());
    };

    let error = |reason: String| CreatureRaceTargetValidationError {
        source_plugin: event.decision.source_plugin.clone(),
        source_form_id: format!("{:06X}", event.source_form_id),
        editor_id: event.editor_id.clone(),
        model_path: event.decision.normalized_model.clone(),
        skeleton_path: event.decision.normalized_skeleton.clone(),
        behavior_path: event.decision.normalized_behavior.clone(),
        reason,
        coverage_reason: "target_rnam_signature_validation_failed".to_owned(),
    };

    if record.sig != SigCode(*b"NPC_") {
        return Err(error(format!(
            "CREA-derived target signature is {}, expected NPC_",
            record.sig.as_str()
        )));
    }

    let rnam = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"RNAM")
        .collect::<Vec<_>>();
    if rnam.len() != 1 {
        return Err(error(format!(
            "CREA-derived NPC_ has {} RNAM fields, expected exactly one",
            rnam.len()
        )));
    }
    let FieldValue::FormKey(race) = rnam[0].value else {
        return Err(error(
            "CREA-derived NPC_.RNAM is not a typed FormKey".to_owned(),
        ));
    };
    let plugin = interner.resolve(race.plugin).unwrap_or("");
    if race.local == 0 || plugin.is_empty() || plugin.eq_ignore_ascii_case("__null__") {
        return Err(error("CREA-derived NPC_.RNAM is null".to_owned()));
    }
    if !resolves_to_race(race) {
        return Err(error(format!(
            "CREA-derived NPC_.RNAM {:06X}@{} does not resolve to RACE",
            race.local, plugin
        )));
    }
    Ok(())
}

fn legacy_rnam_bytes(value: &FieldValue) -> Vec<u8> {
    match value {
        FieldValue::Bytes(bytes) => bytes.to_vec(),
        FieldValue::Uint(value) => vec![*value as u8],
        FieldValue::Int(value) => vec![*value as u8],
        FieldValue::FormKey(value) => value.local.to_le_bytes().to_vec(),
        _ => Vec::new(),
    }
}

fn first_asset_path(
    record: &Record,
    interner: &StringInterner,
    predicate: impl Fn(&str, &str) -> bool,
) -> Option<String> {
    for field in &record.fields {
        let field_sig = field.sig.as_str();
        match &field.value {
            FieldValue::String(value) => {
                let Some(path) = interner.resolve(*value) else {
                    continue;
                };
                if predicate(field_sig, path) {
                    return Some(path.to_owned());
                }
            }
            FieldValue::Bytes(bytes) => {
                for raw in bytes.split(|byte| *byte == 0).filter(|raw| !raw.is_empty()) {
                    let path = String::from_utf8_lossy(raw);
                    if predicate(field_sig, &path) {
                        return Some(path.into_owned());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn policy_for_architecture(
    architecture: CreatureArchitecture,
    interner: &StringInterner,
) -> CreatureRacePolicy {
    let donor = match architecture {
        CreatureArchitecture::Deathclaw => Some(0x01_DB4A),
        CreatureArchitecture::Dog => Some(0x01_D698),
        CreatureArchitecture::Mirelurk => Some(0x02_3FFC),
        CreatureArchitecture::Molerat => Some(0x01_D810),
        CreatureArchitecture::Radscorpion => Some(0x06_36AB),
        CreatureArchitecture::YaoGuai => Some(0x0A_0F2F),
        CreatureArchitecture::FeralGhoul => Some(0x06_B4EC),
        CreatureArchitecture::Bloatfly => Some(0x02_9463),
        _ => None,
    };
    if let Some(local) = donor {
        return CreatureRacePolicy::AuditedDonor {
            architecture,
            race: FormKey {
                local,
                plugin: interner.intern("Fallout4.esm"),
            },
        };
    }
    let reason = match architecture {
        CreatureArchitecture::Humanoid => {
            UnsupportedCreatureReason::HumanoidCreatureHasNoAuditedFo4RaceDonor
        }
        CreatureArchitecture::Cazador => UnsupportedCreatureReason::CazadorHasNoAuditedFo4RaceDonor,
        CreatureArchitecture::Stingwing => {
            UnsupportedCreatureReason::StingwingHasNoAuditedLegacyArchitecture
        }
        CreatureArchitecture::Gecko => {
            UnsupportedCreatureReason::GeckoRetargetIsNotAnAuditedRaceDonor
        }
        CreatureArchitecture::Nightstalker => {
            UnsupportedCreatureReason::NightstalkerRetargetIsNotAnAuditedRaceDonor
        }
        CreatureArchitecture::Robot => UnsupportedCreatureReason::RobotRacePolicyNotAudited,
        _ => UnsupportedCreatureReason::UnknownArchitecture,
    };
    CreatureRacePolicy::Unsupported {
        architecture,
        reason,
    }
}

fn normalize_asset_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && !part.eq_ignore_ascii_case("meshes"))
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("/")
}

fn architecture_from_path(path: &str) -> Option<CreatureArchitecture> {
    let parts = path.split('/').collect::<Vec<_>>();
    let has = |names: &[&str]| parts.iter().any(|part| names.contains(part));
    if has(&["cazador"]) {
        Some(CreatureArchitecture::Cazador)
    } else if has(&["stingwing"]) {
        Some(CreatureArchitecture::Stingwing)
    } else if has(&[
        "gecko",
        "younggecko",
        "goldengecko",
        "firegecko",
        "geckopowder",
    ]) {
        Some(CreatureArchitecture::Gecko)
    } else if has(&["nightstalker"]) {
        Some(CreatureArchitecture::Nightstalker)
    } else if has(&[
        "misterhandy",
        "mistergutsy",
        "securitron",
        "robobrain",
        "protectron",
    ]) {
        Some(CreatureArchitecture::Robot)
    } else if has(&["deathclaw"]) {
        Some(CreatureArchitecture::Deathclaw)
    } else if has(&["dog", "dogmeat"]) {
        Some(CreatureArchitecture::Dog)
    } else if has(&["mirelurk"]) {
        Some(CreatureArchitecture::Mirelurk)
    } else if has(&["molerat"]) {
        Some(CreatureArchitecture::Molerat)
    } else if has(&["radscorpion", "radscorpionalbino"]) {
        Some(CreatureArchitecture::Radscorpion)
    } else if has(&["yaoguai", "yao_guai"]) {
        Some(CreatureArchitecture::YaoGuai)
    } else if has(&["feralghoul", "feral_ghoul"]) {
        Some(CreatureArchitecture::FeralGhoul)
    } else if has(&["bloatfly"]) {
        Some(CreatureArchitecture::Bloatfly)
    } else if has(&["characters", "characterassets", "_male", "_female"]) {
        Some(CreatureArchitecture::Humanoid)
    } else {
        None
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreatureRaceCoverageReport {
    pub expected_candidates: usize,
    pub candidates: usize,
    pub audited_donors: usize,
    pub humanoid_no_ops: usize,
    pub explicit_unsupported: usize,
    pub unresolved_architecture: usize,
    pub wrong_record_type: usize,
    pub donor_on_wrong_record_type: usize,
    pub by_architecture: BTreeMap<String, usize>,
    pub by_model: BTreeMap<String, usize>,
    pub by_plugin: BTreeMap<String, usize>,
}

impl CreatureRaceCoverageReport {
    pub fn coverage_gate_passes(&self) -> bool {
        self.candidates == self.expected_candidates
            && self.wrong_record_type == 0
            && self.donor_on_wrong_record_type == 0
            && self.unresolved_architecture == 0
            && self.explicit_unsupported == 0
    }

    pub(crate) fn observe_decision(
        &mut self,
        decision: &CreatureRaceDecision,
        source_record_type: &str,
    ) {
        self.candidates += 1;
        *self
            .by_architecture
            .entry(decision.architecture().label().to_owned())
            .or_default() += 1;
        *self
            .by_model
            .entry(
                decision
                    .normalized_model
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            )
            .or_default() += 1;
        *self
            .by_plugin
            .entry(decision.source_plugin.clone())
            .or_default() += 1;
        match &decision.policy {
            CreatureRacePolicy::AuditedDonor { .. } => {
                self.audited_donors += 1;
                if source_record_type != "CREA" {
                    self.donor_on_wrong_record_type += 1;
                }
            }
            CreatureRacePolicy::Unsupported { reason, .. } => {
                self.explicit_unsupported += 1;
                match reason {
                    UnsupportedCreatureReason::WrongRecordType => self.wrong_record_type += 1,
                    UnsupportedCreatureReason::UnknownArchitecture
                    | UnsupportedCreatureReason::ConflictingArchitectureEvidence => {
                        self.unresolved_architecture += 1;
                    }
                    _ => {}
                }
            }
        }
    }
}

pub fn build_creature_race_coverage(
    evidence: &[CreatureRaceEvidence<'_>],
    expected_candidates: usize,
    interner: &StringInterner,
) -> CreatureRaceCoverageReport {
    let mut report = CreatureRaceCoverageReport {
        expected_candidates,
        ..CreatureRaceCoverageReport::default()
    };
    for candidate in evidence {
        let decision = classify_legacy_creature_race(candidate, interner);
        report.observe_decision(&decision, candidate.source_record_type);
    }
    report
}

pub fn build_full_creature_race_coverage(
    evidence: &[CreatureRaceEvidence<'_>],
    interner: &StringInterner,
) -> CreatureRaceCoverageReport {
    build_creature_race_coverage(evidence, EXPECTED_FULL_MERGED_CREA_CANDIDATES, interner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence<'a>(
        family: LegacyCreatureFamily,
        plugin: &'a str,
        record_type: &'a str,
        model: Option<&'a str>,
    ) -> CreatureRaceEvidence<'a> {
        CreatureRaceEvidence {
            family,
            source_plugin: plugin,
            source_record_type: record_type,
            model_path: model,
            skeleton_path: None,
            behavior_path: None,
            legacy_rnam: None,
        }
    }

    fn donor_local(decision: &CreatureRaceDecision) -> u32 {
        decision.audited_race().unwrap().unwrap().local
    }

    fn creature_record(
        interner: &StringInterner,
        sig: &str,
        plugin: &str,
        editor_id: &str,
        model: &str,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey {
                local: 0x12_3456,
                plugin: interner.intern(plugin),
            },
        );
        record.eid = Some(interner.intern(editor_id));
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"MODL"),
            value: FieldValue::String(interner.intern(model)),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"RNAM"),
            value: FieldValue::Uint(0x4A),
        });
        record
    }

    #[test]
    fn deathclaw_dog_and_radscorpion_goldens_cover_fnv_and_fo3() {
        let interner = StringInterner::new();
        let deathclaw = classify_legacy_creature_race(
            &evidence(
                LegacyCreatureFamily::Fnv,
                "FalloutNV.esm",
                "CREA",
                Some(r"Meshes\Creatures\Deathclaw\characterassets\deathclaw.nif"),
            ),
            &interner,
        );
        let dog = classify_legacy_creature_race(
            &evidence(
                LegacyCreatureFamily::Fo3,
                "Fallout3.esm",
                "CREA",
                Some(r"creatures\dog\dog.nif"),
            ),
            &interner,
        );
        let scorpion = classify_legacy_creature_race(
            &evidence(
                LegacyCreatureFamily::Fnv,
                "FalloutNV.esm",
                "CREA",
                Some(r"Creatures/RadScorpion/RadScorpion.nif"),
            ),
            &interner,
        );
        assert_eq!(donor_local(&deathclaw), 0x01_DB4A);
        assert_eq!(donor_local(&dog), 0x01_D698);
        assert_eq!(donor_local(&scorpion), 0x06_36AB);
    }

    #[test]
    fn every_audited_architecture_has_the_verified_fo4_race() {
        let interner = StringInterner::new();
        let cases = [
            ("Creatures/Deathclaw/model.nif", 0x01_DB4A),
            ("Creatures/Dog/model.nif", 0x01_D698),
            ("Creatures/Mirelurk/model.nif", 0x02_3FFC),
            ("Creatures/Molerat/model.nif", 0x01_D810),
            ("Creatures/RadScorpion/model.nif", 0x06_36AB),
            ("Creatures/YaoGuai/model.nif", 0x0A_0F2F),
            ("Creatures/FeralGhoul/model.nif", 0x06_B4EC),
            ("Creatures/Bloatfly/model.nif", 0x02_9463),
        ];
        for (model, local) in cases {
            let decision = classify_legacy_creature_race(
                &evidence(
                    LegacyCreatureFamily::Fnv,
                    "FalloutNV.esm",
                    "CREA",
                    Some(model),
                ),
                &interner,
            );
            let donor = decision.audited_race().unwrap().unwrap();
            assert_eq!(donor.local, local);
            assert_eq!(interner.resolve(donor.plugin), Some("Fallout4.esm"));
        }
    }

    #[test]
    fn cazador_stingwing_gecko_nightstalker_and_robot_are_explicitly_unsupported() {
        let interner = StringInterner::new();
        let cases = [
            (
                "Creatures/Cazador/cazador.nif",
                UnsupportedCreatureReason::CazadorHasNoAuditedFo4RaceDonor,
            ),
            (
                "Actors/Stingwing/stingwing.nif",
                UnsupportedCreatureReason::StingwingHasNoAuditedLegacyArchitecture,
            ),
            (
                "Creatures/Gecko/gecko.nif",
                UnsupportedCreatureReason::GeckoRetargetIsNotAnAuditedRaceDonor,
            ),
            (
                "Creatures/NightStalker/nightstalker.nif",
                UnsupportedCreatureReason::NightstalkerRetargetIsNotAnAuditedRaceDonor,
            ),
            (
                "Creatures/Securitron/securitron.nif",
                UnsupportedCreatureReason::RobotRacePolicyNotAudited,
            ),
        ];
        for (model, expected) in cases {
            let decision = classify_legacy_creature_race(
                &evidence(
                    LegacyCreatureFamily::Fnv,
                    "FalloutNV.esm",
                    "CREA",
                    Some(model),
                ),
                &interner,
            );
            assert_eq!(decision.audited_race(), Err(expected));
        }
    }

    #[test]
    fn humanoid_creature_without_an_audited_race_donor_is_unsupported() {
        let interner = StringInterner::new();
        let decision = classify_legacy_creature_race(
            &evidence(
                LegacyCreatureFamily::Fo3,
                "Fallout3.esm",
                "CREA",
                Some(r"Characters\_Male\skeleton.nif"),
            ),
            &interner,
        );
        assert_eq!(
            decision.audited_race(),
            Err(UnsupportedCreatureReason::HumanoidCreatureHasNoAuditedFo4RaceDonor)
        );
    }

    #[test]
    fn legion_creature_is_fail_closed() {
        let interner = StringInterner::new();
        let mut record = creature_record(
            &interner,
            "CREA",
            "FNV_FO3_Merged.esm",
            "LegionCreature",
            "characters/_male/skeleton.nif",
        );

        let error = apply_legacy_creature_race_policy(Game::Fnv, Game::Fo4, &mut record, &interner)
            .unwrap_err();
        assert_eq!(
            error.diagnostic.editor_id.as_deref(),
            Some("LegionCreature")
        );
        assert_eq!(
            error.diagnostic.reason,
            UnsupportedCreatureReason::HumanoidCreatureHasNoAuditedFo4RaceDonor
        );

        assert!(!record.fields.iter().any(|field| field.sig.0 == *b"RNAM"));
    }

    #[test]
    fn legacy_rnam_u8_is_attack_reach_and_never_a_race_reference() {
        let interner = StringInterner::new();
        let mut source = evidence(
            LegacyCreatureFamily::Fnv,
            "FalloutNV.esm",
            "CREA",
            Some("Creatures/Deathclaw/deathclaw.nif"),
        );
        let attack_reach = [0x4A];
        source.legacy_rnam = Some(&attack_reach);
        let decision = classify_legacy_creature_race(&source, &interner);
        assert_eq!(donor_local(&decision), 0x01_DB4A);
        assert_eq!(
            decision.legacy_rnam,
            LegacyRnamPolicy::AttackReachMustDrop { raw: vec![0x4A] }
        );
    }

    #[test]
    fn coverage_gate_is_serializable_and_rejects_furniture_or_unknown_rows() {
        let interner = StringInterner::new();
        let rows = [
            evidence(
                LegacyCreatureFamily::Fnv,
                "FalloutNV.esm",
                "CREA",
                Some("Creatures/Dog/dog.nif"),
            ),
            evidence(
                LegacyCreatureFamily::Fo3,
                "Fallout3.esm",
                "CREA",
                Some("Creatures/Cazador/cazador.nif"),
            ),
        ];
        let report = build_creature_race_coverage(&rows, 2, &interner);
        assert!(!report.coverage_gate_passes());
        assert_eq!(report.audited_donors, 1);
        assert_eq!(report.explicit_unsupported, 1);
        assert!(serde_json::to_string(&report).unwrap().contains("Fallout"));

        let wrong = [evidence(
            LegacyCreatureFamily::Fnv,
            "FalloutNV.esm",
            "FURN",
            Some("Creatures/Deathclaw/deathclaw.nif"),
        )];
        let report = build_creature_race_coverage(&wrong, 1, &interner);
        assert!(!report.coverage_gate_passes());
        assert_eq!(report.wrong_record_type, 1);
        assert_eq!(report.donor_on_wrong_record_type, 0);

        let substring = [evidence(
            LegacyCreatureFamily::Fnv,
            "FalloutNV.esm",
            "CREA",
            Some("Architecture/DeathclawStatue/model.nif"),
        )];
        let report = build_creature_race_coverage(&substring, 1, &interner);
        assert!(!report.coverage_gate_passes());
        assert_eq!(report.audited_donors, 0);
        assert_eq!(report.unresolved_architecture, 1);
    }

    #[test]
    fn conflicting_model_and_behavior_architectures_fail_closed() {
        let interner = StringInterner::new();
        let mut source = evidence(
            LegacyCreatureFamily::Fnv,
            "FalloutNV.esm",
            "CREA",
            Some("Creatures/Dog/dog.nif"),
        );
        source.behavior_path = Some("Actors/Deathclaw/DeathclawProject.hkx");
        let decision = classify_legacy_creature_race(&source, &interner);
        assert_eq!(
            decision.audited_race(),
            Err(UnsupportedCreatureReason::ConflictingArchitectureEvidence)
        );
    }

    #[test]
    fn production_gate_removes_attack_reach_and_synthesizes_verified_races() {
        let interner = StringInterner::new();
        for (model, expected) in [
            ("Creatures/Deathclaw/deathclaw.nif", 0x01_DB4A),
            ("Creatures/Dog/dog.nif", 0x01_D698),
            ("Creatures/RadScorpion/radscorpion.nif", 0x06_36AB),
        ] {
            let mut record =
                creature_record(&interner, "CREA", "FalloutNV.esm", "CreatureFixture", model);
            let event =
                apply_legacy_creature_race_policy(Game::Fnv, Game::Fo4, &mut record, &interner)
                    .unwrap()
                    .unwrap();
            assert_eq!(
                event.decision.legacy_rnam,
                LegacyRnamPolicy::AttackReachMustDrop { raw: vec![0x4A] }
            );
            let rnams = record
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"RNAM")
                .collect::<Vec<_>>();
            assert_eq!(rnams.len(), 1);
            assert!(matches!(rnams[0].value, FieldValue::FormKey(fk) if fk.local == expected));
        }
    }

    #[test]
    fn unsupported_cazador_and_robot_are_fatal_with_source_evidence() {
        let interner = StringInterner::new();
        for model in [
            "Creatures/Cazador/cazador.nif",
            "Creatures/Securitron/securitron.nif",
        ] {
            let mut record = creature_record(
                &interner,
                "CREA",
                "FNV_FO3_Merged.esm",
                "BadCreature",
                model,
            );
            let error =
                apply_legacy_creature_race_policy(Game::Fnv, Game::Fo4, &mut record, &interner)
                    .unwrap_err();
            let diagnostic = error.to_string();
            assert!(diagnostic.contains("FNV_FO3_Merged.esm"));
            assert!(diagnostic.contains("123456"));
            assert!(diagnostic.contains("BadCreature"));
            assert!(diagnostic.contains("coverage_reason"));
            assert!(!record.fields.iter().any(|field| field.sig.0 == *b"RNAM"));
        }
    }

    #[test]
    fn production_gate_isolated_from_non_crea_and_fo76() {
        let interner = StringInterner::new();
        let mut furniture = creature_record(
            &interner,
            "FURN",
            "FalloutNV.esm",
            "Furniture",
            "Creatures/Deathclaw/deathclaw.nif",
        );
        let furniture_before = furniture.fields.clone();
        assert!(
            apply_legacy_creature_race_policy(Game::Fnv, Game::Fo4, &mut furniture, &interner)
                .unwrap()
                .is_none()
        );
        assert_eq!(furniture.fields, furniture_before);

        let mut fo76 = creature_record(
            &interner,
            "CREA",
            "SeventySix.esm",
            "Creature",
            "Creatures/Deathclaw/deathclaw.nif",
        );
        let fo76_before = fo76.fields.clone();
        assert!(
            apply_legacy_creature_race_policy(Game::Fo76, Game::Fo4, &mut fo76, &interner)
                .unwrap()
                .is_none()
        );
        assert_eq!(fo76.fields, fo76_before);
    }

    #[test]
    fn shared_gate_gives_legacy_and_v2_identical_results() {
        let interner = StringInterner::new();
        let source = creature_record(
            &interner,
            "CREA",
            "Fallout3.esm",
            "DogFixture",
            "Creatures/Dog/dog.nif",
        );
        let mut legacy = source.clone();
        let mut v2 = source;
        let legacy_event =
            apply_legacy_creature_race_policy(Game::Fo3, Game::Fo4, &mut legacy, &interner)
                .unwrap()
                .unwrap();
        let v2_event = apply_legacy_creature_race_policy(Game::Fo3, Game::Fo4, &mut v2, &interner)
            .unwrap()
            .unwrap();
        assert_eq!(legacy.fields, v2.fields);
        assert_eq!(legacy_event.decision, v2_event.decision);
    }

    #[test]
    fn target_validation_requires_one_non_null_rnam_resolving_to_race() {
        let interner = StringInterner::new();
        let mut source = creature_record(
            &interner,
            "CREA",
            "FalloutNV.esm",
            "DeathclawFixture",
            "Creatures/Deathclaw/deathclaw.nif",
        );
        let event = apply_legacy_creature_race_policy(Game::Fnv, Game::Fo4, &mut source, &interner)
            .unwrap()
            .unwrap();
        source.sig = SigCode(*b"NPC_");
        assert!(
            validate_crea_derived_npc_race(&source, Some(&event), &interner, |fk| {
                fk.local == 0x01_DB4A
            })
            .is_ok()
        );

        let mut wrong_type = source.clone();
        wrong_type.sig = SigCode(*b"FURN");
        assert!(
            validate_crea_derived_npc_race(&wrong_type, Some(&event), &interner, |_| true)
                .unwrap_err()
                .reason
                .contains("expected NPC_")
        );

        let mut duplicate = source.clone();
        duplicate
            .fields
            .push(duplicate.fields.last().unwrap().clone());
        assert!(
            validate_crea_derived_npc_race(&duplicate, Some(&event), &interner, |_| true)
                .unwrap_err()
                .reason
                .contains("exactly one")
        );
        assert!(
            validate_crea_derived_npc_race(&source, Some(&event), &interner, |_| false)
                .unwrap_err()
                .reason
                .contains("does not resolve to RACE")
        );

        let mut null_race = source.clone();
        null_race
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"RNAM")
            .unwrap()
            .value = FieldValue::FormKey(FormKey {
            local: 0,
            plugin: interner.intern("__null__"),
        });
        assert!(
            validate_crea_derived_npc_race(&null_race, Some(&event), &interner, |_| true)
                .unwrap_err()
                .reason
                .contains("is null")
        );

        let mut untyped_race = source.clone();
        untyped_race
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"RNAM")
            .unwrap()
            .value = FieldValue::Uint(0x01_DB4A);
        assert!(
            validate_crea_derived_npc_race(&untyped_race, Some(&event), &interner, |_| true)
                .unwrap_err()
                .reason
                .contains("not a typed FormKey")
        );
    }

    #[test]
    fn complete_coverage_gate_enforces_explicit_expected_count() {
        let interner = StringInterner::new();
        let decision = classify_legacy_creature_race(
            &evidence(
                LegacyCreatureFamily::Fnv,
                "FNV_FO3_Merged.esm",
                "CREA",
                Some("Creatures/Dog/dog.nif"),
            ),
            &interner,
        );
        let mut complete = CreatureRaceCoverageReport {
            expected_candidates: EXPECTED_FULL_MERGED_CREA_CANDIDATES,
            ..Default::default()
        };
        for _ in 0..EXPECTED_FULL_MERGED_CREA_CANDIDATES {
            complete.observe_decision(&decision, "CREA");
        }
        assert!(complete.coverage_gate_passes());
        complete.expected_candidates -= 1;
        assert!(!complete.coverage_gate_passes());
    }

    #[test]
    fn full_merged_gate_counts_source_candidates_not_prior_output_survivors() {
        const PRIOR_OUTPUT_CREA_SURVIVORS: usize = 2_667;

        assert_eq!(EXPECTED_FULL_MERGED_CREA_CANDIDATES, 2_683);
        assert_eq!(
            EXPECTED_FULL_MERGED_CREA_CANDIDATES - PRIOR_OUTPUT_CREA_SURVIVORS,
            16
        );
    }
}
