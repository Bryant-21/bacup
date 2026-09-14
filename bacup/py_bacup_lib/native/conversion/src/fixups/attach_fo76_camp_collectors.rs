//! Fixup: rebuild FO76 C.A.M.P. collector production on the converted `CONT`.
//!
//! FO76 collectors run `WorkshopCollectorScript`, whose client `.pex` is stripped
//! and whose VMAD binding has no properties. Output and rate live in the
//! FO76-only `RESO` record:
//!
//! | `RESO` subrecord | Target | Meaning                                  |
//! |------------------|--------|------------------------------------------|
//! | `NAM1`           | `AVIF` | resource identity (`Type = Resource`)    |
//! | `NAM2`           | `LVLI` | what the collector produces              |
//! | `NAM4`           | `GLOB` | production interval, in hours            |
//!
//! FO4 has no `RESO`, so the translator drops it and collectors stay empty. This
//! pass matches each `WorkshopCollectorObject` `CONT`'s resource `PRPS` row to its
//! `RESO` and attaches `B21:WorkshopCollector` with `Produce` and `IntervalHours`
//! bound to the mapped `LVLI` and `GLOB`. A collector with no `RESO`, or whose
//! `LVLI`/`GLOB` did not survive, is left unbound and counted in diagnostics.

use rustc_hash::FxHashMap;

use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;

use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const SOURCE_PLUGIN: &str = "SeventySix.esm";
const SCRIPT_NAME: &str = "B21:WorkshopCollector";
/// `SeventySix.esm:52EE0B` — `WorkshopCollectorObject`, the keyword every
/// collector container carries.
const COLLECTOR_KEYWORD_LOCAL: u32 = 0x52_EE0B;
const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

/// One collector's production data, already mapped into target FormKeys.
#[derive(Clone, Copy)]
struct ResourceSpec {
    produce: FormKey,
    interval: FormKey,
}

pub struct AttachFo76CampCollectorsFixup;

impl Fixup for AttachFo76CampCollectorsFixup {
    fn name(&self) -> &'static str {
        "attach_fo76_camp_collectors"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        source_game == Some("fo76") && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        apply(session, mapper)
    }
}

fn apply(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let source_plugin = mapper.interner.intern(SOURCE_PLUGIN);
    let Some(collector_keyword) = mapper.lookup(FormKey {
        local: COLLECTOR_KEYWORD_LOCAL,
        plugin: source_plugin,
    }) else {
        report.warnings.push(
            mapper
                .interner
                .intern("attach_fo76_camp_collectors:keyword_unmapped"),
        );
        return Ok(report);
    };

    let specs = build_resource_specs(session, mapper, &mut report)?;
    if specs.is_empty() {
        return Ok(report);
    }

    let target_schema = session
        .schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let cont_sig =
        SigCode::from_str("CONT").map_err(|error| FixupError::Other(error.to_string()))?;
    let cont_keys = session
        .form_keys_of_sig(cont_sig, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;

    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let mut changed_records = Vec::new();
    let mut unresolved = 0u32;
    let mut conflicts = 0u32;

    for form_key in cont_keys {
        let Ok(mut record) =
            session.record_decoded(&form_key, target_schema.as_ref(), mapper.interner)
        else {
            continue;
        };
        if !has_keyword(
            &record,
            collector_keyword,
            session.target_masters(),
            &target_plugin,
            mapper.interner,
        ) {
            continue;
        }
        let Some(spec) = resource_spec_for(
            &record,
            &specs,
            session.target_masters(),
            &target_plugin,
            mapper.interner,
        ) else {
            unresolved = unresolved.saturating_add(1);
            continue;
        };
        let Some(vmad) = collector_vmad(
            spec,
            session.target_masters(),
            &target_plugin,
            mapper.interner,
        ) else {
            conflicts = conflicts.saturating_add(1);
            continue;
        };
        match attach_collector_script(&mut record, &vmad)? {
            AttachResult::Changed => changed_records.push(record),
            AttachResult::AlreadyPresent => {}
            AttachResult::Conflict(_) => conflicts = conflicts.saturating_add(1),
        }
    }

    let expected = changed_records.len();
    let replaced = session
        .replace_records_contents(changed_records, target_schema.as_ref(), mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    if replaced != expected {
        return Err(FixupError::HandleError(format!(
            "attach_fo76_camp_collectors replaced {replaced} of {expected} expected records"
        )));
    }

    report.records_changed = replaced as u32;
    report.diagnostics.push(
        mapper
            .interner
            .intern(&format!("attach_fo76_camp_collectors:attached={replaced}")),
    );
    if unresolved > 0 || conflicts > 0 {
        report.warnings.push(mapper.interner.intern(&format!(
            "attach_fo76_camp_collectors:unresolved={unresolved}:conflicts={conflicts}"
        )));
    }
    Ok(report)
}

/// Index the source `RESO` records by the target FormKey of their resource
/// `AVIF`, keeping only rows whose produce list and interval global both survived
/// conversion — a collector bound to a missing list would be worse than an
/// unbound one.
fn build_resource_specs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    report: &mut FixupReport,
) -> Result<FxHashMap<FormKey, ResourceSpec>, FixupError> {
    let source_schema = session
        .source_schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let reso_sig =
        SigCode::from_str("RESO").map_err(|error| FixupError::Other(error.to_string()))?;
    let reso_keys = session
        .source_form_keys_of_sig(reso_sig, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;

    let mut specs: FxHashMap<FormKey, ResourceSpec> = FxHashMap::default();
    let mut dropped = 0u32;
    for form_key in reso_keys {
        let Ok(record) =
            session.source_record_decoded(&form_key, source_schema.as_ref(), mapper.interner)
        else {
            dropped = dropped.saturating_add(1);
            continue;
        };
        let (Some(actor_value), Some(produce), Some(interval)) = (
            source_form_key_field(&record, b"NAM1"),
            source_form_key_field(&record, b"NAM2"),
            source_form_key_field(&record, b"NAM4"),
        ) else {
            dropped = dropped.saturating_add(1);
            continue;
        };
        let (Some(actor_value), Some(produce), Some(interval)) = (
            mapper.lookup(actor_value),
            mapper.lookup(produce),
            mapper.lookup(interval),
        ) else {
            dropped = dropped.saturating_add(1);
            continue;
        };
        specs.insert(actor_value, ResourceSpec { produce, interval });
    }

    report.diagnostics.push(mapper.interner.intern(&format!(
        "attach_fo76_camp_collectors:resources={}:dropped={dropped}",
        specs.len()
    )));
    Ok(specs)
}

/// Read a `formid`-codec subrecord off a decoded source record.
fn source_form_key_field(record: &Record, sig: &[u8; 4]) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *sig)
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(fk) => Some(*fk),
            _ => None,
        })
}

/// `KWDA` is a `formid_array`. It reaches a fixup either fully typed or still as
/// raw bytes depending on how far the record was decoded, so read both — the
/// silent failure mode of guessing wrong is a fixup that quietly matches nothing.
fn has_keyword(
    record: &Record,
    keyword: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> bool {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"KWDA")
        .any(|entry| match &entry.value {
            FieldValue::List(items) => items
                .iter()
                .any(|item| matches!(item, FieldValue::FormKey(fk) if *fk == keyword)),
            FieldValue::FormKey(fk) => *fk == keyword,
            FieldValue::Bytes(bytes) => bytes.chunks_exact(4).any(|chunk| {
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                resolve_raw_form_id(raw, target_masters, target_plugin, interner) == keyword
            }),
            _ => false,
        })
}

/// Find the collector's resource by scanning `PRPS` for the one actor-value row
/// that names a known resource. The other rows are ordinary FO4 workshop
/// properties (`CarryWeight`, budget multipliers) and must not match.
///
/// `PRPS` is `array_struct:I,f` — 8-byte rows of FormID + float. Like `KWDA` it
/// can arrive typed or raw, and the raw form is read the same way.
fn resource_spec_for(
    record: &Record,
    specs: &FxHashMap<FormKey, ResourceSpec>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<ResourceSpec> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"PRPS")
        .find_map(|entry| {
            resource_from_value(entry, specs, target_masters, target_plugin, interner)
        })
}

fn resource_from_value(
    entry: &FieldEntry,
    specs: &FxHashMap<FormKey, ResourceSpec>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<ResourceSpec> {
    match &entry.value {
        FieldValue::List(rows) => rows
            .iter()
            .find_map(|row| row_resource(row, specs, target_masters, target_plugin, interner)),
        other => row_resource(other, specs, target_masters, target_plugin, interner),
    }
}

const PRPS_ROW_LEN: usize = 8;

fn row_resource(
    row: &FieldValue,
    specs: &FxHashMap<FormKey, ResourceSpec>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<ResourceSpec> {
    match row {
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| match value {
            FieldValue::FormKey(fk) => specs.get(fk).copied(),
            _ => None,
        }),
        FieldValue::FormKey(fk) => specs.get(fk).copied(),
        FieldValue::Bytes(bytes) => bytes.chunks_exact(PRPS_ROW_LEN).find_map(|chunk| {
            let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            specs
                .get(&resolve_raw_form_id(
                    raw,
                    target_masters,
                    target_plugin,
                    interner,
                ))
                .copied()
        }),
        _ => None,
    }
}

fn resolve_raw_form_id(
    raw: u32,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> FormKey {
    let master_index = ((raw >> 24) & 0xFF) as usize;
    let plugin_name = target_masters
        .get(master_index)
        .map(String::as_str)
        .unwrap_or(target_plugin);
    FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin_name),
    }
}

fn collector_vmad(
    spec: ResourceSpec,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let produce_plugin = interner.resolve(spec.produce.plugin)?;
    let interval_plugin = interner.resolve(spec.interval.plugin)?;
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": SCRIPT_NAME,
                "Flags": 0,
                "Properties": [
                    {
                        "propertyName": "Produce",
                        "Type": "Object",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": {
                            "Alias": -1,
                            "FormID": {"reference": {
                                "plugin": produce_plugin,
                                "object_id": format!("{:06X}", spec.produce.local),
                            }},
                        },
                    },
                    {
                        "propertyName": "IntervalHours",
                        "Type": "Object",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": {
                            "Alias": -1,
                            "FormID": {"reference": {
                                "plugin": interval_plugin,
                                "object_id": format!("{:06X}", spec.interval.local),
                            }},
                        },
                    },
                ],
            }],
        }),
        target_masters,
        target_plugin,
    )
}

fn attach_collector_script(record: &mut Record, vmad: &[u8]) -> Result<AttachResult, FixupError> {
    let vmad_sig = SubrecordSig::from_str("VMAD")
        .map_err(|error| FixupError::SchemaError(error.to_string()))?;
    for field in &mut record.fields {
        if field.sig != vmad_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut field.value else {
            return Ok(AttachResult::Conflict("existing_vmad_not_bytes"));
        };
        let mut patched = bytes.to_vec();
        let result = attach_script_bytes(&mut patched, SCRIPT_NAME, vmad);
        if result == AttachResult::Changed {
            *bytes = patched.into();
        }
        return Ok(result);
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: vmad_sig,
            value: FieldValue::Bytes(vmad.into()),
        },
    );
    Ok(AttachResult::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;
    use smallvec::smallvec;

    fn interner() -> StringInterner {
        StringInterner::new()
    }

    fn fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    fn cont(fields: smallvec::SmallVec<[FieldEntry; 8]>, interner: &StringInterner) -> Record {
        let mut record = Record::new(SigCode::from_str("CONT").unwrap(), fk(interner, 0x5F_BD09));
        record.fields = fields;
        record
    }

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn prps_row(interner: &StringInterner, actor_value: u32, value: f32) -> FieldValue {
        FieldValue::Struct(vec![
            (
                interner.intern("PropertiesActorValue"),
                FieldValue::FormKey(fk(interner, actor_value)),
            ),
            (interner.intern("PropertiesValue"), FieldValue::Float(value)),
        ])
    }

    fn beehive_specs(interner: &StringInterner) -> FxHashMap<FormKey, ResourceSpec> {
        let mut specs = FxHashMap::default();
        specs.insert(
            fk(interner, 0x5F_E788),
            ResourceSpec {
                produce: fk(interner, 0x5F_E790),
                interval: fk(interner, 0x5F_E78F),
            },
        );
        specs
    }

    #[test]
    fn matches_the_resource_row_and_ignores_carry_weight() {
        let interner = interner();
        // Real shape of SCORE_S4_Collector_Beehive: CarryWeight first, the
        // resource AV second. Only the resource row may select the spec.
        let record = cont(
            smallvec![field(
                "PRPS",
                FieldValue::List(vec![
                    prps_row(&interner, 0x00_02DC, 1.0),
                    prps_row(&interner, 0x5F_E788, 1.0),
                ]),
            )],
            &interner,
        );
        let spec = resource_spec_for(
            &record,
            &beehive_specs(&interner),
            &[],
            "SeventySix.esm",
            &interner,
        )
        .expect("resource matched");
        assert_eq!(spec.produce.local, 0x5F_E790);
        assert_eq!(spec.interval.local, 0x5F_E78F);
    }

    #[test]
    fn leaves_a_container_with_no_resource_row_unresolved() {
        let interner = interner();
        let record = cont(
            smallvec![field(
                "PRPS",
                FieldValue::List(vec![prps_row(&interner, 0x00_02DC, 2.0)]),
            )],
            &interner,
        );
        assert!(
            resource_spec_for(
                &record,
                &beehive_specs(&interner),
                &[],
                "SeventySix.esm",
                &interner
            )
            .is_none()
        );
    }

    #[test]
    fn reads_prps_and_kwda_that_are_still_raw_bytes() {
        // Whether a subrecord reaches a fixup typed or raw depends on how far the
        // record was decoded; matching only the typed shape would make this fixup
        // silently do nothing.
        let interner = interner();
        // Output-plugin encoding: master index 0 is Fallout4.esm; own records sit
        // one past the master list and fall back to the target plugin name.
        let masters = ["Fallout4.esm".to_string()];
        let mut prps = Vec::new();
        prps.extend_from_slice(&0x0000_02DCu32.to_le_bytes()); // Fallout4.esm CarryWeight
        prps.extend_from_slice(&1.0f32.to_le_bytes());
        prps.extend_from_slice(&0x015F_E788u32.to_le_bytes()); // own-plugin resource AV
        prps.extend_from_slice(&1.0f32.to_le_bytes());

        let mut kwda = Vec::new();
        kwda.extend_from_slice(&0x013D_019Du32.to_le_bytes());
        kwda.extend_from_slice(&(0x0100_0000 | COLLECTOR_KEYWORD_LOCAL).to_le_bytes());

        let record = cont(
            smallvec![
                field("KWDA", FieldValue::Bytes(kwda.into())),
                field("PRPS", FieldValue::Bytes(prps.into())),
            ],
            &interner,
        );

        assert!(has_keyword(
            &record,
            fk(&interner, COLLECTOR_KEYWORD_LOCAL),
            &masters,
            "SeventySix.esm",
            &interner
        ));
        let spec = resource_spec_for(
            &record,
            &beehive_specs(&interner),
            &masters,
            "SeventySix.esm",
            &interner,
        )
        .expect("raw resource row matched");
        assert_eq!(spec.produce.local, 0x5F_E790);
    }

    #[test]
    fn detects_the_collector_keyword_in_a_kwda_list() {
        let interner = interner();
        let keyword = fk(&interner, COLLECTOR_KEYWORD_LOCAL);
        let record = cont(
            smallvec![field(
                "KWDA",
                FieldValue::List(vec![
                    FieldValue::FormKey(fk(&interner, 0x3D_019D)),
                    FieldValue::FormKey(keyword),
                ]),
            )],
            &interner,
        );
        assert!(has_keyword(
            &record,
            keyword,
            &[],
            "SeventySix.esm",
            &interner
        ));
        assert!(!has_keyword(
            &record,
            fk(&interner, 0x00_1234),
            &[],
            "SeventySix.esm",
            &interner
        ));
    }

    #[test]
    fn attaches_alongside_the_hollow_fo76_script_without_replacing_it() {
        let interner = interner();
        // VMAD carrying WorkshopCollectorScript with zero properties — exactly
        // what every converted collector ships.
        let mut existing: Vec<u8> = Vec::new();
        existing.extend_from_slice(&VMAD_VERSION.to_le_bytes());
        existing.extend_from_slice(&VMAD_OBJECT_FORMAT.to_le_bytes());
        existing.extend_from_slice(&1u16.to_le_bytes());
        let name = b"WorkshopCollectorScript";
        existing.extend_from_slice(&(name.len() as u16).to_le_bytes());
        existing.extend_from_slice(name);
        existing.push(0);
        existing.extend_from_slice(&0u16.to_le_bytes());

        let mut record = cont(
            smallvec![field("VMAD", FieldValue::Bytes(existing.into()))],
            &interner,
        );

        let mut addition: Vec<u8> = Vec::new();
        addition.extend_from_slice(&VMAD_VERSION.to_le_bytes());
        addition.extend_from_slice(&VMAD_OBJECT_FORMAT.to_le_bytes());
        addition.extend_from_slice(&1u16.to_le_bytes());
        addition.extend_from_slice(&(SCRIPT_NAME.len() as u16).to_le_bytes());
        addition.extend_from_slice(SCRIPT_NAME.as_bytes());
        addition.push(0);
        addition.extend_from_slice(&0u16.to_le_bytes());

        assert_eq!(
            attach_collector_script(&mut record, &addition).unwrap(),
            AttachResult::Changed
        );
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("VMAD is not bytes");
        };
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 2);
        let text = String::from_utf8_lossy(bytes);
        assert!(text.contains("WorkshopCollectorScript"));
        assert!(text.contains(SCRIPT_NAME));

        // Re-running must not stack a second copy.
        assert_eq!(
            attach_collector_script(&mut record, &addition).unwrap(),
            AttachResult::AlreadyPresent
        );
    }
}
