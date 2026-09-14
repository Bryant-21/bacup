use std::collections::{BTreeMap, BTreeSet};

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::run::CreatureAncillaryNpcReservation;
use crate::source_rig::{CreatureAncillaryNpcRequestId, SourceCreatureIdentity, TargetFormKey};
use crate::sym::StringInterner;

use super::creature_catalog::{LegacyCreatureGame, StableFormKey};
use super::creature_dependencies::FnvFo3CreatureDependencyLedger;
use super::legacy_appearance::{
    LegacyEyeSourceEvidence, LegacyHairSourceEvidence, decode_legacy_eye, decode_legacy_hair,
};

const LEGACY_NPC_ACBS_SIZE: usize = 24;
const LEGACY_NPC_ACBS_FLAGS_OFFSET: usize = 0;
const LEGACY_NPC_ACBS_TEMPLATE_FLAGS_OFFSET: usize = 22;
const LEGACY_NPC_ACBS_FEMALE: u32 = 0x0000_0001;
const LEGACY_NPC_TEMPLATE_USE_TRAITS: u16 = 0x0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegacyNpcSex {
    Male,
    Female,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyNpcAppearanceTemplateHop {
    pub source: StableFormKey,
    pub template: StableFormKey,
    pub template_flags: u16,
    pub inherits_traits: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyNpcAppearancePart<T> {
    pub source: StableFormKey,
    pub evidence: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyNpcAppearanceEvidence {
    pub source: StableFormKey,
    pub effective_traits_source: StableFormKey,
    pub template_hops: Vec<LegacyNpcAppearanceTemplateHop>,
    pub sex: LegacyNpcSex,
    pub race: StableFormKey,
    pub eye: Option<LegacyNpcAppearancePart<LegacyEyeSourceEvidence>>,
    pub hair: Option<LegacyNpcAppearancePart<LegacyHairSourceEvidence>>,
    pub head_parts: Vec<StableFormKey>,
    pub hair_color: [u8; 4],
    pub face_symmetry: Vec<u8>,
    pub face_geometry: Vec<u8>,
    pub face_texture: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyNpcAppearanceEvidenceError {
    pub source: StableFormKey,
    pub reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyAncillaryNpcAppearancePreparation {
    pub game: LegacyCreatureGame,
    pub source: StableFormKey,
    pub target_npc: TargetFormKey,
    pub evidence: Result<LegacyNpcAppearanceEvidence, LegacyNpcAppearanceEvidenceError>,
}

pub(crate) fn build_legacy_ancillary_npc_appearance_preparations(
    reservations: &[CreatureAncillaryNpcReservation],
    records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    interner: &StringInterner,
) -> Result<Vec<LegacyAncillaryNpcAppearancePreparation>, String> {
    let mut previous_source = None::<StableFormKey>;
    let mut preparations = Vec::with_capacity(reservations.len());
    for reservation in reservations {
        let source = stable_form_key(reservation.source, interner).ok_or_else(|| {
            "legacy ancillary NPC reservation source plugin is not interned".to_string()
        })?;
        if previous_source.as_ref().is_some_and(|previous| {
            (previous.plugin.to_ascii_lowercase(), previous.local)
                >= (source.plugin.to_ascii_lowercase(), source.local)
        }) {
            return Err(
                "legacy ancillary NPC reservations are not canonical and unique".to_string(),
            );
        }
        previous_source = Some(source.clone());

        let target_plugin = interner
            .resolve(reservation.target.plugin)
            .ok_or_else(|| {
                "legacy ancillary NPC reservation target plugin is not interned".to_string()
            })?
            .to_string();
        let target_npc = TargetFormKey {
            plugin: target_plugin,
            local: reservation.target.local,
        };
        let games = [LegacyCreatureGame::Fnv, LegacyCreatureGame::Fo3]
            .into_iter()
            .filter(|game| {
                records
                    .get(&(*game, source.clone()))
                    .is_some_and(|record| record.sig.as_str() == "NPC_")
            })
            .collect::<Vec<_>>();
        let [game] = games.as_slice() else {
            return Err(format!(
                "legacy ancillary NPC reservation source must resolve to exactly one game: {source} matches={}",
                games.len()
            ));
        };
        preparations.push(LegacyAncillaryNpcAppearancePreparation {
            game: *game,
            source: source.clone(),
            target_npc,
            evidence: build_legacy_npc_appearance_evidence(*game, &source, records, interner),
        });
    }
    Ok(preparations)
}

pub(crate) fn build_legacy_ancillary_npc_candidate_requests(
    ledger: &FnvFo3CreatureDependencyLedger,
) -> Result<Vec<CreatureAncillaryNpcRequestId>, String> {
    let records = ledger
        .records
        .iter()
        .map(|record| (record.source.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut requests = Vec::new();
    for candidate in &ledger.candidates {
        let candidate_identity = source_identity(candidate.provenance.game, &candidate.source);
        for dependency in &candidate.closure_form_keys {
            let record = records.get(dependency).ok_or_else(|| {
                format!(
                    "candidate {} closure record {dependency} is absent from the dependency ledger",
                    candidate.source
                )
            })?;
            if record.signature == "NPC_" {
                requests.push(CreatureAncillaryNpcRequestId {
                    candidate: candidate_identity.clone(),
                    ancillary_npc: source_identity(record.provenance.game, &record.source),
                });
            }
        }
    }
    requests.sort();
    requests.dedup();
    Ok(requests)
}

fn source_identity(game: LegacyCreatureGame, source: &StableFormKey) -> SourceCreatureIdentity {
    SourceCreatureIdentity {
        namespace: match game {
            LegacyCreatureGame::Fnv => "fnv",
            LegacyCreatureGame::Fo3 => "fo3",
        }
        .to_string(),
        plugin: source.plugin.clone(),
        local_form_id: source.local,
    }
}

pub(crate) fn build_legacy_npc_appearance_evidence(
    game: LegacyCreatureGame,
    source: &StableFormKey,
    records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    interner: &StringInterner,
) -> Result<LegacyNpcAppearanceEvidence, LegacyNpcAppearanceEvidenceError> {
    let mut current = source.clone();
    let mut visited = BTreeSet::new();
    let mut template_hops = Vec::new();
    loop {
        if !visited.insert(current.clone()) {
            return appearance_error(source, "legacy_npc_appearance_template_cycle");
        }
        let record = npc_record(game, &current, source, records)?;
        let acbs = exact_bytes(record, "ACBS", LEGACY_NPC_ACBS_SIZE, source)?;
        let template_flags = u16::from_le_bytes(
            acbs[LEGACY_NPC_ACBS_TEMPLATE_FLAGS_OFFSET..LEGACY_NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                .try_into()
                .expect("validated ACBS width"),
        );
        let template = optional_form_key(record, "TPLT", interner, source)?;
        let Some(template) = template else {
            break;
        };
        let inherits_traits = template_flags & LEGACY_NPC_TEMPLATE_USE_TRAITS != 0;
        template_hops.push(LegacyNpcAppearanceTemplateHop {
            source: current.clone(),
            template: template.clone(),
            template_flags,
            inherits_traits,
        });
        if !inherits_traits {
            break;
        }
        current = template;
    }

    let effective_traits_source = current;
    let record = npc_record(game, &effective_traits_source, source, records)?;
    let acbs = exact_bytes(record, "ACBS", LEGACY_NPC_ACBS_SIZE, source)?;
    let flags = u32::from_le_bytes(
        acbs[LEGACY_NPC_ACBS_FLAGS_OFFSET..LEGACY_NPC_ACBS_FLAGS_OFFSET + 4]
            .try_into()
            .expect("validated ACBS width"),
    );
    let race = required_form_key(record, "RNAM", interner, source)?;
    let eye = appearance_part(
        game,
        optional_form_key(record, "ENAM", interner, source)?,
        source,
        records,
        |record| decode_legacy_eye(record, interner).map_err(|support| support.reason_codes),
    )?;
    let hair = appearance_part(
        game,
        optional_form_key(record, "HNAM", interner, source)?,
        source,
        records,
        |record| decode_legacy_hair(record, interner).map_err(|support| support.reason_codes),
    )?;
    let head_parts = form_keys(record, "PNAM", interner, source)?;
    let hair_color = exact_bytes(record, "HCLR", 4, source)?
        .try_into()
        .expect("validated HCLR width");
    let face_symmetry = exact_bytes(record, "FGGS", 200, source)?;
    let face_geometry = exact_bytes(record, "FGGA", 120, source)?;
    let face_texture = exact_bytes(record, "FGTS", 200, source)?;
    Ok(LegacyNpcAppearanceEvidence {
        source: source.clone(),
        effective_traits_source,
        template_hops,
        sex: if flags & LEGACY_NPC_ACBS_FEMALE != 0 {
            LegacyNpcSex::Female
        } else {
            LegacyNpcSex::Male
        },
        race,
        eye,
        hair,
        head_parts,
        hair_color,
        face_symmetry,
        face_geometry,
        face_texture,
    })
}

fn npc_record<'a>(
    game: LegacyCreatureGame,
    key: &StableFormKey,
    source: &StableFormKey,
    records: &'a BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
) -> Result<&'a Record, LegacyNpcAppearanceEvidenceError> {
    let Some(record) = records.get(&(game, key.clone())).copied() else {
        return appearance_error(source, "legacy_npc_appearance_source_record_missing");
    };
    if record.sig.as_str() != "NPC_" {
        return appearance_error(source, "legacy_npc_appearance_source_signature_mismatch");
    }
    Ok(record)
}

fn appearance_part<T>(
    game: LegacyCreatureGame,
    key: Option<StableFormKey>,
    owner: &StableFormKey,
    records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    decode: impl FnOnce(&Record) -> Result<T, Vec<String>>,
) -> Result<Option<LegacyNpcAppearancePart<T>>, LegacyNpcAppearanceEvidenceError> {
    let Some(key) = key else {
        return Ok(None);
    };
    let Some(record) = records.get(&(game, key.clone())).copied() else {
        return appearance_error(owner, "legacy_npc_appearance_part_source_missing");
    };
    let evidence = decode(record).map_err(|_| LegacyNpcAppearanceEvidenceError {
        source: owner.clone(),
        reason_code: "legacy_npc_appearance_part_shape_unverified".to_string(),
    })?;
    Ok(Some(LegacyNpcAppearancePart {
        source: key,
        evidence,
    }))
}

fn required_form_key(
    record: &Record,
    signature: &str,
    interner: &StringInterner,
    source: &StableFormKey,
) -> Result<StableFormKey, LegacyNpcAppearanceEvidenceError> {
    optional_form_key(record, signature, interner, source)?.ok_or_else(|| {
        LegacyNpcAppearanceEvidenceError {
            source: source.clone(),
            reason_code: format!("legacy_npc_appearance_{signature}_missing"),
        }
    })
}

fn optional_form_key(
    record: &Record,
    signature: &str,
    interner: &StringInterner,
    source: &StableFormKey,
) -> Result<Option<StableFormKey>, LegacyNpcAppearanceEvidenceError> {
    let values = form_keys(record, signature, interner, source)?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => appearance_error(
            source,
            "legacy_npc_appearance_field_multiplicity_unverified",
        ),
    }
}

fn form_keys(
    record: &Record,
    signature: &str,
    interner: &StringInterner,
    source: &StableFormKey,
) -> Result<Vec<StableFormKey>, LegacyNpcAppearanceEvidenceError> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .map(|field| match field.value {
            FieldValue::FormKey(value) => {
                stable_form_key(value, interner).ok_or_else(|| LegacyNpcAppearanceEvidenceError {
                    source: source.clone(),
                    reason_code: "legacy_npc_appearance_form_key_unresolved".to_string(),
                })
            }
            _ => appearance_error(source, "legacy_npc_appearance_form_key_shape_unverified"),
        })
        .collect()
}

fn exact_bytes(
    record: &Record,
    signature: &str,
    width: usize,
    source: &StableFormKey,
) -> Result<Vec<u8>, LegacyNpcAppearanceEvidenceError> {
    let values = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .collect::<Vec<_>>();
    let [field] = values.as_slice() else {
        return appearance_error(
            source,
            "legacy_npc_appearance_field_multiplicity_unverified",
        );
    };
    let FieldValue::Bytes(bytes) = &field.value else {
        return appearance_error(source, "legacy_npc_appearance_byte_field_shape_unverified");
    };
    if bytes.len() != width {
        return appearance_error(source, "legacy_npc_appearance_byte_field_width_unverified");
    }
    Ok(bytes.to_vec())
}

fn stable_form_key(value: FormKey, interner: &StringInterner) -> Option<StableFormKey> {
    Some(StableFormKey {
        local: value.local,
        plugin: interner.resolve(value.plugin)?.to_string(),
    })
}

fn appearance_error<T>(
    source: &StableFormKey,
    reason_code: &str,
) -> Result<T, LegacyNpcAppearanceEvidenceError> {
    Err(LegacyNpcAppearanceEvidenceError {
        source: source.clone(),
        reason_code: reason_code.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn key(local: u32) -> StableFormKey {
        StableFormKey {
            local,
            plugin: "FalloutNV.esm".to_string(),
        }
    }

    fn form_key(local: u32, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("FalloutNV.esm"),
        }
    }

    fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(sig),
            value,
        }
    }

    fn eye(local: u32, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("EYES").unwrap(),
            form_key(local, interner),
        );
        record.fields.extend([
            field(*b"EDID", FieldValue::String(interner.intern("EyeBlue"))),
            field(*b"FULL", FieldValue::String(interner.intern("Blue"))),
            field(
                *b"ICON",
                FieldValue::String(interner.intern("Characters\\Eyes\\EyeBlue.dds")),
            ),
            field(*b"DATA", FieldValue::Uint(1)),
        ]);
        record
    }

    fn hair(local: u32, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("HAIR").unwrap(),
            form_key(local, interner),
        );
        record.fields.extend([
            field(*b"EDID", FieldValue::String(interner.intern("HairWavy"))),
            field(*b"FULL", FieldValue::String(interner.intern("Smooth Wave"))),
            field(
                *b"ICON",
                FieldValue::String(interner.intern("Characters\\Hair\\HairWavy.dds")),
            ),
            field(
                *b"MODL",
                FieldValue::String(interner.intern("Characters\\Hair\\HairWavy.nif")),
            ),
            field(*b"MODT", FieldValue::Bytes(vec![0; 96].into())),
            field(*b"DATA", FieldValue::Uint(5)),
        ]);
        record
    }

    fn npc(
        local: u32,
        female: bool,
        template: Option<(u32, u16)>,
        eye: u32,
        hair: u32,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            form_key(local, interner),
        );
        let mut acbs = vec![0; LEGACY_NPC_ACBS_SIZE];
        acbs[..4].copy_from_slice(&u32::from(female).to_le_bytes());
        if let Some((_, flags)) = template {
            acbs[LEGACY_NPC_ACBS_TEMPLATE_FLAGS_OFFSET..LEGACY_NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                .copy_from_slice(&flags.to_le_bytes());
        }
        record.fields.extend([
            field(*b"ACBS", FieldValue::Bytes(acbs.into())),
            field(*b"RNAM", FieldValue::FormKey(form_key(0x19, interner))),
            field(*b"ENAM", FieldValue::FormKey(form_key(eye, interner))),
            field(*b"HNAM", FieldValue::FormKey(form_key(hair, interner))),
            field(*b"PNAM", FieldValue::FormKey(form_key(0x900, interner))),
            field(*b"HCLR", FieldValue::Bytes(vec![1, 2, 3, 0].into())),
            field(*b"FGGS", FieldValue::Bytes(vec![1; 200].into())),
            field(*b"FGGA", FieldValue::Bytes(vec![2; 120].into())),
            field(*b"FGTS", FieldValue::Bytes(vec![3; 200].into())),
        ]);
        if let Some((template, _)) = template {
            record.fields.push(field(
                *b"TPLT",
                FieldValue::FormKey(form_key(template, interner)),
            ));
        }
        record
    }

    fn reservation(
        source: u32,
        target: u32,
        interner: &StringInterner,
    ) -> CreatureAncillaryNpcReservation {
        CreatureAncillaryNpcReservation {
            source: form_key(source, interner),
            target: FormKey {
                local: target,
                plugin: interner.intern("B21_Output.esp"),
            },
        }
    }

    #[test]
    fn exact_local_traits_are_decoded_without_a_donor() {
        let interner = StringInterner::new();
        let npc = npc(0x100, true, None, 0x200, 0x300, &interner);
        let eye = eye(0x200, &interner);
        let hair = hair(0x300, &interner);
        let records = BTreeMap::from([
            ((LegacyCreatureGame::Fnv, key(0x100)), &npc),
            ((LegacyCreatureGame::Fnv, key(0x200)), &eye),
            ((LegacyCreatureGame::Fnv, key(0x300)), &hair),
        ]);

        let evidence = build_legacy_npc_appearance_evidence(
            LegacyCreatureGame::Fnv,
            &key(0x100),
            &records,
            &interner,
        )
        .unwrap();
        assert_eq!(evidence.effective_traits_source, key(0x100));
        assert_eq!(evidence.sex, LegacyNpcSex::Female);
        assert_eq!(evidence.eye.unwrap().source, key(0x200));
        assert_eq!(evidence.hair.unwrap().source, key(0x300));
        assert_eq!(evidence.head_parts, [key(0x900)]);
    }

    #[test]
    fn use_traits_follows_the_exact_template_chain() {
        let interner = StringInterner::new();
        let owner = npc(
            0x100,
            true,
            Some((0x101, LEGACY_NPC_TEMPLATE_USE_TRAITS)),
            0x200,
            0x300,
            &interner,
        );
        let template = npc(0x101, false, None, 0x201, 0x301, &interner);
        let owner_eye = eye(0x200, &interner);
        let template_eye = eye(0x201, &interner);
        let owner_hair = hair(0x300, &interner);
        let template_hair = hair(0x301, &interner);
        let records = BTreeMap::from([
            ((LegacyCreatureGame::Fnv, key(0x100)), &owner),
            ((LegacyCreatureGame::Fnv, key(0x101)), &template),
            ((LegacyCreatureGame::Fnv, key(0x200)), &owner_eye),
            ((LegacyCreatureGame::Fnv, key(0x201)), &template_eye),
            ((LegacyCreatureGame::Fnv, key(0x300)), &owner_hair),
            ((LegacyCreatureGame::Fnv, key(0x301)), &template_hair),
        ]);

        let evidence = build_legacy_npc_appearance_evidence(
            LegacyCreatureGame::Fnv,
            &key(0x100),
            &records,
            &interner,
        )
        .unwrap();
        assert_eq!(evidence.effective_traits_source, key(0x101));
        assert_eq!(evidence.sex, LegacyNpcSex::Male);
        assert_eq!(evidence.eye.unwrap().source, key(0x201));
        assert_eq!(evidence.hair.unwrap().source, key(0x301));
        assert_eq!(evidence.template_hops.len(), 1);
        assert!(evidence.template_hops[0].inherits_traits);
    }

    #[test]
    fn template_cycles_are_terminal_evidence_errors() {
        let interner = StringInterner::new();
        let first = npc(
            0x100,
            false,
            Some((0x101, LEGACY_NPC_TEMPLATE_USE_TRAITS)),
            0x200,
            0x300,
            &interner,
        );
        let second = npc(
            0x101,
            false,
            Some((0x100, LEGACY_NPC_TEMPLATE_USE_TRAITS)),
            0x200,
            0x300,
            &interner,
        );
        let records = BTreeMap::from([
            ((LegacyCreatureGame::Fnv, key(0x100)), &first),
            ((LegacyCreatureGame::Fnv, key(0x101)), &second),
        ]);
        assert_eq!(
            build_legacy_npc_appearance_evidence(
                LegacyCreatureGame::Fnv,
                &key(0x100),
                &records,
                &interner,
            )
            .unwrap_err()
            .reason_code,
            "legacy_npc_appearance_template_cycle"
        );
    }

    #[test]
    fn ancillary_reservations_bind_exact_source_game_and_target_npc() {
        let interner = StringInterner::new();
        let npc = npc(0x100, false, None, 0x200, 0x300, &interner);
        let eye = eye(0x200, &interner);
        let hair = hair(0x300, &interner);
        let records = BTreeMap::from([
            ((LegacyCreatureGame::Fnv, key(0x100)), &npc),
            ((LegacyCreatureGame::Fnv, key(0x200)), &eye),
            ((LegacyCreatureGame::Fnv, key(0x300)), &hair),
        ]);

        let preparations = build_legacy_ancillary_npc_appearance_preparations(
            &[reservation(0x100, 0x800, &interner)],
            &records,
            &interner,
        )
        .unwrap();
        assert_eq!(preparations.len(), 1);
        assert_eq!(preparations[0].game, LegacyCreatureGame::Fnv);
        assert_eq!(preparations[0].source, key(0x100));
        assert_eq!(preparations[0].target_npc.plugin, "B21_Output.esp");
        assert_eq!(preparations[0].target_npc.local, 0x800);
        assert!(preparations[0].evidence.is_ok());
    }

    #[test]
    fn ancillary_reservations_must_be_canonical_and_unique() {
        let interner = StringInterner::new();
        let first = npc(0x100, false, None, 0x200, 0x300, &interner);
        let second = npc(0x101, false, None, 0x200, 0x300, &interner);
        let eye = eye(0x200, &interner);
        let hair = hair(0x300, &interner);
        let records = BTreeMap::from([
            ((LegacyCreatureGame::Fnv, key(0x100)), &first),
            ((LegacyCreatureGame::Fnv, key(0x101)), &second),
            ((LegacyCreatureGame::Fnv, key(0x200)), &eye),
            ((LegacyCreatureGame::Fnv, key(0x300)), &hair),
        ]);

        let error = build_legacy_ancillary_npc_appearance_preparations(
            &[
                reservation(0x101, 0x801, &interner),
                reservation(0x100, 0x800, &interner),
            ],
            &records,
            &interner,
        )
        .unwrap_err();
        assert_eq!(
            error,
            "legacy ancillary NPC reservations are not canonical and unique"
        );
    }
}
