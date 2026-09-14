use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use smallvec::SmallVec;

use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

const REWARD_SCRIPT_NAME: &str = "B21:ExpeditionMissionRewards";
const XP_NONE_LOCAL_ID: u32 = 0x098952;
const STAMP_ITEM_LOCAL_ID: u32 = 0x645E16;
const COMPLETION_STAGE: i32 = 9000;
const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MissionRewardSpec {
    source_quest: u32,
    source_quest_eid: &'static str,
    optional_stages: [i32; 3],
    xp_globals: [u32; 4],
    stamp_counts: [i32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetRewardContract {
    optional_stages: [i32; 3],
    xp_globals: [FormKey; 4],
    stamp_counts: [i32; 4],
    stamp_item: FormKey,
    xp_none: FormKey,
}

const MISSION_REWARD_SPECS: &[MissionRewardSpec] = &[
    MissionRewardSpec {
        source_quest: 0x6BAA3D,
        source_quest_eid: "XPD_AC01_Mission_Tax",
        optional_stages: [5000, 3690, 2650],
        xp_globals: [0x641F0A, 0x641F09, 0x6371B3, 0x6F8B30],
        stamp_counts: [3, 5, 10, 15],
    },
    MissionRewardSpec {
        source_quest: 0x6B231C,
        source_quest_eid: "XPD_AC02_Mission_Sensation",
        optional_stages: [1800, 1805, 1810],
        xp_globals: [0x641F0A, 0x641F09, 0x6371B3, 0x6371B4],
        stamp_counts: [3, 5, 10, 15],
    },
    MissionRewardSpec {
        source_quest: 0x6274EC,
        source_quest_eid: "XPD_Pitt01_Mission",
        optional_stages: [1675, 4950, 4531],
        xp_globals: [0x641F0A, 0x641F09, 0x6371B3, 0x6371B4],
        stamp_counts: [1, 2, 5, 8],
    },
    MissionRewardSpec {
        source_quest: 0x648280,
        source_quest_eid: "XPD_Pitt02_Mission",
        optional_stages: [1810, 7300, 5100],
        xp_globals: [0x6D21E1, 0x6D21E2, 0x6D21E3, 0x6D21E4],
        stamp_counts: [2, 3, 6, 9],
    },
];

pub struct RepairExpeditionMissionRewardsFixup;

impl Fixup for RepairExpeditionMissionRewardsFixup {
    fn name(&self) -> &'static str {
        "repair_expedition_mission_rewards"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.source_id().is_some()
            && session
                .source_schema()
                .is_ok_and(|schema| schema.record_def("QUST").is_some())
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let source_schema = session
            .source_schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let source_plugin_name = session
            .source_slot_opt()
            .map(|slot| slot.parsed.plugin_name.clone())
            .ok_or_else(|| FixupError::HandleError("source plugin missing".into()))?;
        let source_plugin = mapper.interner.intern(&source_plugin_name);
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let qust_sig = SigCode::from_str("QUST")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let glob_sig = SigCode::from_str("GLOB")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let misc_sig = SigCode::from_str("MISC")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;

        let mut changed = 0u32;
        let mut present = 0u32;
        let mut unresolved = 0u32;
        for spec in MISSION_REWARD_SPECS {
            let source_quest = FormKey {
                local: spec.source_quest,
                plugin: source_plugin,
            };
            let source_record = match session.source_record_decoded(
                &source_quest,
                source_schema.as_ref(),
                mapper.interner,
            ) {
                Ok(record) => record,
                Err(_) => {
                    unresolved += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        spec.source_quest,
                        "source_quest_unreadable",
                    );
                    continue;
                }
            };
            if !source_contract_matches(&source_record, spec, mapper.interner) {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    spec.source_quest,
                    "source_quest_contract_mismatch",
                );
                continue;
            }

            let Some(target_quest) = mapper.lookup(source_quest) else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    spec.source_quest,
                    "quest_unmapped",
                );
                continue;
            };
            let Some(contract) =
                map_reward_contract(spec, source_plugin, |source| mapper.lookup(source))
            else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    spec.source_quest,
                    "reward_reference_unmapped",
                );
                continue;
            };
            if !target_form_has_sig(
                session,
                target_schema.as_ref(),
                mapper.interner,
                target_quest,
                qust_sig,
            ) || !target_form_has_sig(
                session,
                target_schema.as_ref(),
                mapper.interner,
                contract.xp_none,
                glob_sig,
            ) || !target_form_has_sig(
                session,
                target_schema.as_ref(),
                mapper.interner,
                contract.stamp_item,
                misc_sig,
            ) || !contract.xp_globals.iter().all(|form| {
                target_form_has_sig(
                    session,
                    target_schema.as_ref(),
                    mapper.interner,
                    *form,
                    glob_sig,
                )
            }) {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    spec.source_quest,
                    "target_reference_type_mismatch",
                );
                continue;
            }
            let Some(script_vmad) = reward_script_vmad(
                &contract,
                &target_masters,
                &target_plugin_name,
                mapper.interner,
            ) else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    spec.source_quest,
                    "reward_vmad_encode_failed",
                );
                continue;
            };
            let mut target_record = match session.record_decoded(
                &target_quest,
                target_schema.as_ref(),
                mapper.interner,
            ) {
                Ok(record) => record,
                Err(_) => {
                    unresolved += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        spec.source_quest,
                        "target_quest_unreadable",
                    );
                    continue;
                }
            };
            match apply_target_contract(&mut target_record, &script_vmad, contract.xp_none) {
                Ok(false) => present += 1,
                Ok(true) => {
                    let replaced = session
                        .replace_record_contents(
                            target_record,
                            target_schema.as_ref(),
                            mapper.interner,
                        )
                        .map_err(|error| FixupError::HandleError(error.to_string()))?;
                    if replaced {
                        report.records_changed += 1;
                        changed += 1;
                    } else {
                        unresolved += 1;
                        warn(
                            &mut report,
                            mapper.interner,
                            spec.source_quest,
                            "target_quest_replace_failed",
                        );
                    }
                }
                Err(reason) => {
                    unresolved += 1;
                    warn(&mut report, mapper.interner, spec.source_quest, reason);
                }
            }
        }

        report.message = Some(mapper.interner.intern(&format!(
            "expedition_mission_rewards:changed={changed};present={present};unresolved={unresolved};quests={}",
            MISSION_REWARD_SPECS.len()
        )));
        Ok(report)
    }
}

fn source_contract_matches(
    record: &Record,
    spec: &MissionRewardSpec,
    interner: &StringInterner,
) -> bool {
    if record.sig.0 != *b"QUST"
        || record.eid.and_then(|eid| interner.resolve(eid)) != Some(spec.source_quest_eid)
    {
        return false;
    }
    let stages = record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"INDX")
        .filter_map(|entry| field_value_i32(&entry.value))
        .collect::<Vec<_>>();
    stages.contains(&COMPLETION_STAGE)
        && spec
            .optional_stages
            .iter()
            .all(|stage| stages.contains(stage))
}

fn map_reward_contract(
    spec: &MissionRewardSpec,
    source_plugin: Sym,
    mut map: impl FnMut(FormKey) -> Option<FormKey>,
) -> Option<TargetRewardContract> {
    let xp_globals = spec
        .xp_globals
        .map(|local| {
            map(FormKey {
                local,
                plugin: source_plugin,
            })
        })
        .into_iter()
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()?;
    Some(TargetRewardContract {
        optional_stages: spec.optional_stages,
        xp_globals,
        stamp_counts: spec.stamp_counts,
        stamp_item: map(FormKey {
            local: STAMP_ITEM_LOCAL_ID,
            plugin: source_plugin,
        })?,
        xp_none: map(FormKey {
            local: XP_NONE_LOCAL_ID,
            plugin: source_plugin,
        })?,
    })
}

fn target_form_has_sig(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
    form_key: FormKey,
    sig: SigCode,
) -> bool {
    session
        .record_decoded(&form_key, target_schema, interner)
        .is_ok_and(|record| record.sig == sig)
}

fn reward_script_vmad(
    contract: &TargetRewardContract,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let xp_values = contract
        .xp_globals
        .iter()
        .map(|form| object_value(*form, interner))
        .collect::<Option<Vec<_>>>()?;
    let stamp_item = object_value(contract.stamp_item, interner)?;
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": REWARD_SCRIPT_NAME,
                "Flags": 0,
                "Properties": [
                    int_array_property("OptionalStages", contract.optional_stages.to_vec()),
                    object_array_property("TierXP", xp_values),
                    int_array_property("TierStampCounts", contract.stamp_counts.to_vec()),
                    serde_json::json!({
                        "propertyName": "StampItem",
                        "Type": "Object",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": stamp_item,
                    }),
                ],
            }],
        }),
        target_masters,
        target_plugin,
    )
}

fn int_array_property(name: &str, values: Vec<i32>) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Array of Int32",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": values,
    })
}

fn object_array_property(name: &str, values: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Array of Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": values,
    })
}

fn object_value(form_key: FormKey, interner: &StringInterner) -> Option<serde_json::Value> {
    let plugin = interner.resolve(form_key.plugin)?;
    Some(serde_json::json!({
        "Alias": -1,
        "FormID": {
            "reference": {
                "plugin": plugin,
                "object_id": format!("{:06X}", form_key.local),
            },
        },
    }))
}

fn apply_target_contract(
    record: &mut Record,
    script_vmad: &[u8],
    xp_none: FormKey,
) -> Result<bool, &'static str> {
    let mut vmad_indices = record
        .fields
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (entry.sig.0 == *b"VMAD").then_some(index));
    let Some(vmad_index) = vmad_indices.next() else {
        return Err("quest_vmad_missing");
    };
    if vmad_indices.next().is_some() {
        return Err("multiple_vmad_subrecords");
    }
    let FieldValue::Bytes(existing) = &record.fields[vmad_index].value else {
        return Err("quest_vmad_not_bytes");
    };
    let mut patched_vmad = existing.to_vec();
    let attachment = attach_script_bytes(&mut patched_vmad, REWARD_SCRIPT_NAME, script_vmad);
    let vmad_changed = match attachment {
        AttachResult::Changed => true,
        AttachResult::AlreadyPresent => false,
        AttachResult::Conflict(reason) => return Err(reason),
    };
    let xnam_changed = set_xnam(record, xp_none);
    if vmad_changed {
        record.fields[vmad_index].value = FieldValue::Bytes(SmallVec::from_vec(patched_vmad));
    }
    Ok(vmad_changed || xnam_changed)
}

fn set_xnam(record: &mut Record, xp_none: FormKey) -> bool {
    if let Some(existing) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"XNAM")
    {
        let replacement = FieldValue::FormKey(xp_none);
        if existing.value == replacement {
            return false;
        }
        existing.value = replacement;
        return true;
    }
    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(&entry.sig.0, b"QTGL" | b"INDX" | b"QOBJ" | b"ANAM"))
        .unwrap_or(record.fields.len());
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"XNAM"),
            value: FieldValue::FormKey(xp_none),
        },
    );
    true
}

fn field_value_i32(value: &FieldValue) -> Option<i32> {
    match value {
        FieldValue::Int(value) => i32::try_from(*value).ok(),
        FieldValue::Uint(value) => i32::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => {
            Some(i16::from_le_bytes(bytes.get(0..2)?.try_into().ok()?) as i32)
        }
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| field_value_i32(value)),
        _ => None,
    }
}

fn warn(report: &mut FixupReport, interner: &StringInterner, quest: u32, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "repair_expedition_mission_rewards:{quest:06X}:{reason}"
    )));
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use super::*;
    use crate::record::RecordFlags;

    fn fk(plugin: Sym, local: u32) -> FormKey {
        FormKey { local, plugin }
    }

    fn empty_vmad(plugin: &str) -> Vec<u8> {
        build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [],
            }),
            &[],
            plugin,
        )
        .expect("empty VMAD")
    }

    fn quest_record(
        interner: &StringInterner,
        spec: &MissionRewardSpec,
        plugin: Sym,
        vmad: Vec<u8>,
    ) -> Record {
        let mut fields = vec![FieldEntry {
            sig: SubrecordSig(*b"VMAD"),
            value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
        }];
        fields.extend(
            spec.optional_stages
                .iter()
                .copied()
                .chain([COMPLETION_STAGE])
                .map(|stage| FieldEntry {
                    sig: SubrecordSig(*b"INDX"),
                    value: FieldValue::Int(i64::from(stage)),
                }),
        );
        Record {
            sig: SigCode(*b"QUST"),
            form_key: fk(plugin, spec.source_quest),
            eid: Some(interner.intern(spec.source_quest_eid)),
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec![],
        }
    }

    #[test]
    fn exact_mission_contracts_cover_only_the_four_audited_quests() {
        assert_eq!(MISSION_REWARD_SPECS.len(), 4);
        assert_eq!(
            MISSION_REWARD_SPECS
                .iter()
                .map(|spec| (
                    spec.source_quest,
                    spec.optional_stages,
                    spec.xp_globals,
                    spec.stamp_counts,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    0x6BAA3D,
                    [5000, 3690, 2650],
                    [0x641F0A, 0x641F09, 0x6371B3, 0x6F8B30],
                    [3, 5, 10, 15]
                ),
                (
                    0x6B231C,
                    [1800, 1805, 1810],
                    [0x641F0A, 0x641F09, 0x6371B3, 0x6371B4],
                    [3, 5, 10, 15]
                ),
                (
                    0x6274EC,
                    [1675, 4950, 4531],
                    [0x641F0A, 0x641F09, 0x6371B3, 0x6371B4],
                    [1, 2, 5, 8]
                ),
                (
                    0x648280,
                    [1810, 7300, 5100],
                    [0x6D21E1, 0x6D21E2, 0x6D21E3, 0x6D21E4],
                    [2, 3, 6, 9]
                ),
            ]
        );
        assert!(
            MISSION_REWARD_SPECS
                .iter()
                .all(|spec| { spec.stamp_counts.windows(2).all(|pair| pair[0] < pair[1]) })
        );
    }

    #[test]
    fn source_contract_requires_exact_editor_id_and_all_reward_stages() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let spec = &MISSION_REWARD_SPECS[0];
        let mut record = quest_record(&interner, spec, plugin, empty_vmad("SeventySix.esm"));
        assert!(source_contract_matches(&record, spec, &interner));

        record.eid = Some(interner.intern("XPD_Unrelated"));
        assert!(!source_contract_matches(&record, spec, &interner));
        record.eid = Some(interner.intern(spec.source_quest_eid));
        record
            .fields
            .retain(|entry| field_value_i32(&entry.value) != Some(spec.optional_stages[0]));
        assert!(!source_contract_matches(&record, spec, &interner));
    }

    #[test]
    fn reward_reference_mapping_is_all_or_nothing() {
        let interner = StringInterner::new();
        let source = interner.intern("SeventySix.esm");
        let target = interner.intern("Target.esm");
        let spec = &MISSION_REWARD_SPECS[3];
        let contract = map_reward_contract(spec, source, |form| Some(fk(target, form.local + 1)))
            .expect("complete mapping");
        assert_eq!(contract.xp_none, fk(target, XP_NONE_LOCAL_ID + 1));
        assert_eq!(contract.stamp_item, fk(target, STAMP_ITEM_LOCAL_ID + 1));
        assert_eq!(contract.xp_globals[0], fk(target, 0x6D21E2));

        assert!(
            map_reward_contract(spec, source, |form| {
                (form.local != STAMP_ITEM_LOCAL_ID).then_some(fk(target, form.local))
            })
            .is_none()
        );
        assert!(
            map_reward_contract(spec, source, |form| {
                (form.local != spec.xp_globals[2]).then_some(fk(target, form.local))
            })
            .is_none()
        );
    }

    #[test]
    fn target_patch_is_atomic_idempotent_and_preserves_other_scripts() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let spec = &MISSION_REWARD_SPECS[1];
        let contract = map_reward_contract(spec, plugin, Some).expect("identity mapping");
        let script_vmad =
            reward_script_vmad(&contract, &[], "SeventySix.esm", &interner).expect("reward VMAD");
        let generic_vmad = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [{
                    "ScriptName": "B21:QuestRewards",
                    "Flags": 0,
                    "Properties": [],
                }],
            }),
            &[],
            "SeventySix.esm",
        )
        .expect("generic reward VMAD");
        let mut record = quest_record(&interner, spec, plugin, generic_vmad);

        assert_eq!(
            apply_target_contract(&mut record, &script_vmad, contract.xp_none),
            Ok(true)
        );
        assert_eq!(
            apply_target_contract(&mut record, &script_vmad, contract.xp_none),
            Ok(false)
        );
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"XNAM")
                .count(),
            1
        );
        assert!(record.fields.iter().any(|entry| {
            entry.sig.0 == *b"XNAM" && entry.value == FieldValue::FormKey(contract.xp_none)
        }));
        let vmad = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"VMAD")
            .and_then(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => Some(bytes),
                _ => None,
            })
            .expect("quest VMAD");
        assert_eq!(u16::from_le_bytes([vmad[4], vmad[5]]), 2);
        assert!(
            vmad.windows("B21:QuestRewards".len())
                .any(|row| row == b"B21:QuestRewards")
        );
        assert!(
            vmad.windows(REWARD_SCRIPT_NAME.len())
                .any(|row| row == REWARD_SCRIPT_NAME.as_bytes())
        );
    }

    #[test]
    fn conflicting_binding_does_not_suppress_native_completion_xp() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let spec = &MISSION_REWARD_SPECS[0];
        let contract = map_reward_contract(spec, plugin, Some).expect("identity mapping");
        let desired =
            reward_script_vmad(&contract, &[], "SeventySix.esm", &interner).expect("desired VMAD");
        let mut conflicting_contract = contract.clone();
        conflicting_contract.stamp_counts[0] = 99;
        let conflicting =
            reward_script_vmad(&conflicting_contract, &[], "SeventySix.esm", &interner)
                .expect("conflicting VMAD");
        let mut record = quest_record(&interner, spec, plugin, conflicting);

        assert_eq!(
            apply_target_contract(&mut record, &desired, contract.xp_none),
            Err("same_script_different_binding")
        );
        assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"XNAM"));
    }
}
