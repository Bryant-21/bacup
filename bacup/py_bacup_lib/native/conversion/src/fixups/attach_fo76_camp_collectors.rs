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
//!
//! FO76 caps storage by the `CarryWeight` `PRPS` value read as pounds of stored
//! produce. Papyrus cannot read item weight without F4SE, so the cap is turned
//! into an item count here and bound as `MaxStoredItems`.

use rustc_hash::FxHashMap;

use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;

use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

const SOURCE_PLUGIN: &str = "SeventySix.esm";
const SCRIPT_NAME: &str = "B21:WorkshopCollector";
/// `SeventySix.esm:52EE0B` — `WorkshopCollectorObject`, the keyword every
/// collector container carries.
const COLLECTOR_KEYWORD_LOCAL: u32 = 0x52_EE0B;
/// `SeventySix.esm:3A123C` — `WorkshopCollectorHardpoint`, carried only by the
/// extractors that mount on a resource deposit.
const HARDPOINT_KEYWORD_LOCAL: u32 = 0x3A_123C;
/// `Fallout4.esm:249E73` — `WorkshopIgnoreNonRefOccupiedSnap`. Deposit snap
/// points sit at the deposit origin inside terrain, which FO4's snap code
/// otherwise treats as occupied.
const IGNORE_OCCUPIED_SNAP_LOCAL: u32 = 0x24_9E73;
const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;
/// `CarryWeight` is `0002DC` in both games: `Fallout4.esm` on the target side,
/// the master-less `SeventySix.esm` on the source side.
const CARRY_WEIGHT_LOCAL: u32 = 0x00_02DC;
const FALLOUT4_PLUGIN: &str = "Fallout4.esm";
/// FO76 `PRPS` is `array_struct:I,f,I` — actor value, value, curve table.
const SOURCE_PRPS_ROW_LEN: usize = 12;
const MAX_PRODUCE_LIST_DEPTH: u32 = 4;

/// One collector's production data, already mapped into target FormKeys.
/// `source_produce` is the unmapped FO76 list: the storage cap is FO76 pounds
/// of FO76-weighted items, so it is weighed in the source plugin.
#[derive(Clone, Copy)]
struct ResourceSpec {
    produce: FormKey,
    interval: FormKey,
    source_produce: FormKey,
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
    let hardpoint_keyword = mapper.lookup(FormKey {
        local: HARDPOINT_KEYWORD_LOCAL,
        plugin: source_plugin,
    });

    let specs = build_resource_specs(session, mapper, &mut report)?;

    let target_schema = session
        .schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let cont_sig =
        SigCode::from_str("CONT").map_err(|error| FixupError::Other(error.to_string()))?;
    let cont_keys = session
        .form_keys_of_sig(cont_sig, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;

    let source_schema = session
        .source_schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let mut produce_weights: FxHashMap<FormKey, Option<f32>> = FxHashMap::default();
    let mut changed_records = Vec::new();
    let mut unresolved = 0u32;
    let mut conflicts = 0u32;
    let mut uncapped = 0u32;
    let mut snap_rows = 0u32;

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
        let snap_added = hardpoint_keyword.is_some_and(|keyword| {
            has_keyword(
                &record,
                keyword,
                session.target_masters(),
                &target_plugin,
                mapper.interner,
            )
        }) && add_ignore_occupied_snap_row(
            &mut record,
            session.target_masters(),
            &target_plugin,
            mapper.interner,
        );
        if snap_added {
            snap_rows = snap_rows.saturating_add(1);
        }
        let Some(spec) = resource_spec_for(
            &record,
            &specs,
            session.target_masters(),
            &target_plugin,
            mapper.interner,
        ) else {
            unresolved = unresolved.saturating_add(1);
            if snap_added {
                changed_records.push(record);
            }
            continue;
        };
        let average_weight = match produce_weights.get(&spec.source_produce) {
            Some(weight) => *weight,
            None => {
                let weight = average_source_item_weight(
                    session,
                    source_schema.as_ref(),
                    spec.source_produce,
                    mapper.interner,
                    0,
                );
                produce_weights.insert(spec.source_produce, weight);
                weight
            }
        };
        let source_collector = FormKey {
            local: record.form_key.local,
            plugin: source_plugin,
        };
        let source_capacity = if mapper.lookup(source_collector) == Some(record.form_key) {
            session
                .source_record_decoded(&source_collector, source_schema.as_ref(), mapper.interner)
                .ok()
                .and_then(|source| {
                    carry_weight(
                        &source,
                        FormKey {
                            local: CARRY_WEIGHT_LOCAL,
                            plugin: source_plugin,
                        },
                        SOURCE_PRPS_ROW_LEN,
                        &[],
                        SOURCE_PLUGIN,
                        mapper.interner,
                    )
                })
        } else {
            None
        };
        let max_stored = source_capacity
            .or_else(|| {
                carry_weight(
                    &record,
                    FormKey {
                        local: CARRY_WEIGHT_LOCAL,
                        plugin: mapper.interner.intern(FALLOUT4_PLUGIN),
                    },
                    PRPS_ROW_LEN,
                    session.target_masters(),
                    &target_plugin,
                    mapper.interner,
                )
            })
            .zip(average_weight)
            .and_then(|(capacity, weight)| max_stored_items(capacity, weight));
        if max_stored.is_none() {
            uncapped = uncapped.saturating_add(1);
        }
        let Some(vmad) = collector_vmad(
            spec,
            max_stored,
            session.target_masters(),
            &target_plugin,
            mapper.interner,
        ) else {
            conflicts = conflicts.saturating_add(1);
            if snap_added {
                changed_records.push(record);
            }
            continue;
        };
        let script_changed = match attach_collector_script(&mut record, &vmad)? {
            AttachResult::Changed => true,
            AttachResult::AlreadyPresent => false,
            AttachResult::Conflict(_) => {
                conflicts = conflicts.saturating_add(1);
                false
            }
        };
        if script_changed || snap_added {
            changed_records.push(record);
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
    report.diagnostics.push(mapper.interner.intern(&format!(
        "attach_fo76_camp_collectors:changed={replaced}:uncapped={uncapped}:snap_rows={snap_rows}"
    )));
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
        let source_produce = produce;
        let (Some(actor_value), Some(produce), Some(interval)) = (
            mapper.lookup(actor_value),
            mapper.lookup(produce),
            mapper.lookup(interval),
        ) else {
            dropped = dropped.saturating_add(1);
            continue;
        };
        specs.insert(
            actor_value,
            ResourceSpec {
                produce,
                interval,
                source_produce,
            },
        );
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

/// A collector's `CarryWeight` `PRPS` value, typed or raw. `row_len` is the
/// game's raw row width; the actor value is always the leading FormID.
fn carry_weight(
    record: &Record,
    carry_weight: FormKey,
    row_len: usize,
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
) -> Option<f32> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"PRPS")
        .find_map(|entry| match &entry.value {
            FieldValue::List(rows) => rows.iter().find_map(|row| {
                let FieldValue::Struct(fields) = row else {
                    return None;
                };
                fields
                    .iter()
                    .any(|(_, value)| matches!(value, FieldValue::FormKey(fk) if *fk == carry_weight))
                    .then(|| {
                        fields.iter().find_map(|(_, value)| match value {
                            FieldValue::Float(weight) => Some(*weight),
                            _ => None,
                        })
                    })
                    .flatten()
            }),
            FieldValue::Bytes(bytes) => bytes.chunks_exact(row_len).find_map(|chunk| {
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                (resolve_raw_form_id(raw, masters, plugin, interner) == carry_weight)
                    .then(|| f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]))
            }),
            _ => None,
        })
}

/// Append `(WorkshopIgnoreNonRefOccupiedSnap, 1.0)` to the collector's `PRPS`
/// unless it is already there; returns whether the record changed. Raw rows
/// encode the actor value against `Fallout4.esm`'s master index, so a record
/// whose plugin does not master it is left alone.
fn add_ignore_occupied_snap_row(
    record: &mut Record,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> bool {
    let actor_value = FormKey {
        local: IGNORE_OCCUPIED_SNAP_LOCAL,
        plugin: interner.intern(FALLOUT4_PLUGIN),
    };
    let typed_row = |names: Option<(Sym, Sym)>| {
        let (actor_value_name, value_name) = names.unwrap_or_else(|| {
            (
                interner.intern("properties_actor_value"),
                interner.intern("properties_value"),
            )
        });
        FieldValue::Struct(vec![
            (actor_value_name, FieldValue::FormKey(actor_value)),
            (value_name, FieldValue::Float(1.0)),
        ])
    };
    let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"PRPS")
    else {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"PRPS"),
            value: FieldValue::List(vec![typed_row(None)]),
        });
        return true;
    };
    match &mut entry.value {
        FieldValue::List(rows) => {
            let present = rows.iter().any(|row| {
                matches!(row, FieldValue::Struct(fields) if fields.iter().any(
                    |(_, value)| matches!(value, FieldValue::FormKey(fk) if *fk == actor_value)
                ))
            });
            if present {
                return false;
            }
            let names = rows.iter().find_map(|row| match row {
                FieldValue::Struct(fields) if fields.len() == 2 => Some((fields[0].0, fields[1].0)),
                _ => None,
            });
            rows.push(typed_row(names));
            true
        }
        FieldValue::Bytes(bytes) if bytes.len() % PRPS_ROW_LEN == 0 => {
            let present = bytes.chunks_exact(PRPS_ROW_LEN).any(|chunk| {
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                resolve_raw_form_id(raw, target_masters, target_plugin, interner) == actor_value
            });
            let Some(master_index) = target_masters
                .iter()
                .position(|master| master.eq_ignore_ascii_case(FALLOUT4_PLUGIN))
            else {
                return false;
            };
            if present {
                return false;
            }
            let raw = ((master_index as u32) << 24) | IGNORE_OCCUPIED_SNAP_LOCAL;
            bytes.extend_from_slice(&raw.to_le_bytes());
            bytes.extend_from_slice(&1.0_f32.to_le_bytes());
            true
        }
        _ => false,
    }
}

/// Mean FO76 weight of what a source produce list yields; a nested list counts
/// as one entry at its own mean. `None` if any entry's weight is unreadable, so
/// the script keeps its default cap instead of a wrong one.
fn average_source_item_weight(
    session: &mut PluginSession,
    source_schema: &AuthoringSchema,
    form_key: FormKey,
    interner: &StringInterner,
    depth: u32,
) -> Option<f32> {
    if depth > MAX_PRODUCE_LIST_DEPTH {
        return None;
    }
    let record = session
        .source_record_decoded(&form_key, source_schema, interner)
        .ok()?;
    if record.sig.0 == *b"CMPO" {
        // CMPO carries no weight; a component entry is weighed as its scrap MISC.
        let scrap_item = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"MNAM")
            .and_then(|entry| source_form_key(&entry.value, form_key.plugin))?;
        return average_source_item_weight(session, source_schema, scrap_item, interner, depth + 1);
    }
    if record.sig.0 != *b"LVLI" {
        return item_weight(&record, interner);
    }
    let entries = record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"LVLO")
        .map(|entry| source_lvlo_item(&entry.value, form_key.plugin, interner))
        .collect::<Option<Vec<_>>>()?;
    if entries.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for entry in &entries {
        total += average_source_item_weight(session, source_schema, *entry, interner, depth + 1)?;
    }
    Some(total / entries.len() as f32)
}

/// A FormID in the master-less FO76 plugin, decoded or still raw.
fn source_form_key(value: &FieldValue, source_plugin: Sym) -> Option<FormKey> {
    let local = match value {
        FieldValue::FormKey(fk) => return (fk.local != 0).then_some(*fk),
        FieldValue::Uint(raw) => *raw as u32,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        _ => return None,
    } & 0x00FF_FFFF;
    (local != 0).then_some(FormKey {
        local,
        plugin: source_plugin,
    })
}

/// FO76 `LVLO` comes in two shapes: the legacy 12-byte
/// `[level, item, count]` row and the current 4-byte bare item FormID (whose
/// struct decode misreads the FormID as `level` + `unknown_u8_1`).
fn source_lvlo_item(
    value: &FieldValue,
    source_plugin: Sym,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
            source_form_key(&FieldValue::Bytes(bytes[4..8].into()), source_plugin)
        }
        FieldValue::Struct(fields) => {
            if let Some(fk) = fields.iter().find_map(|(_, value)| match value {
                FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
                _ => None,
            }) {
                return Some(fk);
            }
            let field = |name: &str| {
                fields
                    .iter()
                    .find_map(|(field, value)| match (interner.resolve(*field), value) {
                        (Some(field), FieldValue::Uint(value)) if field == name => {
                            Some(*value as u32)
                        }
                        _ => None,
                    })
            };
            let local =
                (field("level")? & 0xFFFF) | ((field("unknown_u8_1").unwrap_or(0) & 0xFF) << 16);
            (local != 0).then_some(FormKey {
                local,
                plugin: source_plugin,
            })
        }
        other => source_form_key(other, source_plugin),
    }
}

/// `DATA` weight of the item kinds collectors produce: `MISC`/`AMMO` carry it at
/// offset 4 of `struct:i,f`, `ALCH` `DATA` is the weight itself.
fn item_weight(record: &Record, interner: &StringInterner) -> Option<f32> {
    let data = record.fields.iter().find(|entry| entry.sig.0 == *b"DATA")?;
    let weight_offset = match &record.sig.0 {
        b"MISC" | b"AMMO" => 4,
        b"ALCH" => 0,
        _ => return None,
    };
    match &data.value {
        FieldValue::Float(weight) if weight_offset == 0 => Some(*weight),
        FieldValue::Struct(fields) => {
            fields
                .iter()
                .find_map(|(name, value)| match (interner.resolve(*name), value) {
                    (Some("weight"), FieldValue::Float(weight)) => Some(*weight),
                    _ => None,
                })
        }
        FieldValue::Bytes(bytes) => bytes
            .get(weight_offset..weight_offset + 4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])),
        _ => None,
    }
}

fn max_stored_items(carry_weight: f32, average_weight: f32) -> Option<i32> {
    if !(carry_weight > 0.0 && average_weight > 0.0) {
        return None;
    }
    // f32 weights such as 0.1 put 0.5 / 0.1 just under 5; the epsilon keeps
    // whole ratios whole.
    let items = (f64::from(carry_weight) / f64::from(average_weight) + 1e-4).floor();
    Some(items.max(1.0) as i32)
}

fn object_property(
    name: &str,
    form_key: FormKey,
    interner: &StringInterner,
) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "propertyName": name,
        "Type": "Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": {
            "Alias": -1,
            "FormID": {"reference": {
                "plugin": interner.resolve(form_key.plugin)?,
                "object_id": format!("{:06X}", form_key.local),
            }},
        },
    }))
}

fn collector_vmad(
    spec: ResourceSpec,
    max_stored_items: Option<i32>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let mut properties = vec![
        object_property("Produce", spec.produce, interner)?,
        object_property("IntervalHours", spec.interval, interner)?,
    ];
    if let Some(max_stored_items) = max_stored_items {
        properties.push(serde_json::json!({
            "propertyName": "MaxStoredItems",
            "Type": "Int32",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": max_stored_items,
        }));
    }
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": SCRIPT_NAME,
                "Flags": 0,
                "Properties": properties,
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
                source_produce: fk(interner, 0x5F_E790),
            },
        );
        specs
    }

    #[test]
    fn resource_row_and_collector_keyword_match_typed_and_raw_fields() {
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

        // Whether a subrecord reaches a fixup typed or raw depends on how far the
        // record was decoded; matching only the typed shape would make this fixup
        // silently do nothing.
        let interner = self::interner();
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
    fn storage_cap_inputs_read_every_carry_weight_item_weight_and_lvlo_shape() {
        // Oil 0.5 / 0.1, Acid 0.5 / 0.05, Steel 1.0 / 0.05, Concrete 2.0 / 0.05.
        assert_eq!(max_stored_items(0.5, 0.1), Some(5));
        assert_eq!(max_stored_items(0.5, 0.05), Some(10));
        assert_eq!(max_stored_items(1.0, 0.05), Some(20));
        assert_eq!(max_stored_items(2.0, 0.05), Some(40));
        assert_eq!(max_stored_items(2.0, 0.3), Some(6));
        assert_eq!(max_stored_items(0.1, 5.0), Some(1));
        assert_eq!(max_stored_items(0.0, 0.1), None);
        assert_eq!(max_stored_items(2.0, 0.0), None);

        let interner = interner();
        let target_carry_weight = FormKey {
            local: CARRY_WEIGHT_LOCAL,
            plugin: interner.intern(FALLOUT4_PLUGIN),
        };
        let typed = cont(
            smallvec![field(
                "PRPS",
                FieldValue::List(vec![
                    prps_row(&interner, 0x3C_3FA4, 1.0),
                    FieldValue::Struct(vec![
                        (
                            interner.intern("PropertiesActorValue"),
                            FieldValue::FormKey(target_carry_weight),
                        ),
                        (interner.intern("PropertiesValue"), FieldValue::Float(2.0)),
                    ]),
                ]),
            )],
            &interner,
        );
        let read_target = |record: &Record, masters: &[String]| {
            carry_weight(
                record,
                target_carry_weight,
                PRPS_ROW_LEN,
                masters,
                "SeventySix.esm",
                &interner,
            )
        };
        assert_eq!(read_target(&typed, &[]), Some(2.0));

        let masters = ["Fallout4.esm".to_string()];
        let mut prps = Vec::new();
        prps.extend_from_slice(&0x0000_0330u32.to_le_bytes());
        prps.extend_from_slice(&10.0f32.to_le_bytes());
        prps.extend_from_slice(&0x0000_02DCu32.to_le_bytes());
        prps.extend_from_slice(&0.5f32.to_le_bytes());
        let raw = cont(
            smallvec![field("PRPS", FieldValue::Bytes(prps.into()))],
            &interner,
        );
        assert_eq!(read_target(&raw, &masters), Some(0.5));

        let without = cont(
            smallvec![field(
                "PRPS",
                FieldValue::List(vec![prps_row(&interner, 0x3C_3FA4, 1.0)]),
            )],
            &interner,
        );
        assert_eq!(read_target(&without, &[]), None);

        // Source WorkshopCollectorJunkPile: 12-byte FO76 rows in the master-less
        // plugin, CarryWeight third after PowerRequired and the resource AV.
        let mut source_prps = Vec::new();
        for (actor_value, value) in [(0x330_u32, 10.0_f32), (0x3C_3FA4, 1.0), (0x2DC, 2.0)] {
            source_prps.extend_from_slice(&actor_value.to_le_bytes());
            source_prps.extend_from_slice(&value.to_le_bytes());
            source_prps.extend_from_slice(&0_u32.to_le_bytes());
        }
        let source = cont(
            smallvec![field("PRPS", FieldValue::Bytes(source_prps.into()))],
            &interner,
        );
        assert_eq!(
            carry_weight(
                &source,
                fk(&interner, CARRY_WEIGHT_LOCAL),
                SOURCE_PRPS_ROW_LEN,
                &[],
                SOURCE_PLUGIN,
                &interner,
            ),
            Some(2.0)
        );

        let mut misc = Record::new(SigCode::from_str("MISC").unwrap(), fk(&interner, 0x1));
        let mut data = Vec::new();
        data.extend_from_slice(&3_i32.to_le_bytes());
        data.extend_from_slice(&0.1_f32.to_le_bytes());
        misc.fields = smallvec![field("DATA", FieldValue::Bytes(data.into()))];
        assert_eq!(item_weight(&misc, &interner), Some(0.1));

        misc.fields = smallvec![field(
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("value"), FieldValue::Int(3)),
                (interner.intern("weight"), FieldValue::Float(0.05)),
            ]),
        )];
        assert_eq!(item_weight(&misc, &interner), Some(0.05));

        let mut alch = Record::new(SigCode::from_str("ALCH").unwrap(), fk(&interner, 0x2));
        alch.fields = smallvec![field("DATA", FieldValue::Float(0.5))];
        assert_eq!(item_weight(&alch, &interner), Some(0.5));

        let mut cmpo = Record::new(SigCode::from_str("CMPO").unwrap(), fk(&interner, 0x3));
        cmpo.fields = smallvec![field("DATA", FieldValue::Float(1.0))];
        assert_eq!(item_weight(&cmpo, &interner), None);

        let plugin = interner.intern(SOURCE_PLUGIN);
        // WorkshopProduceOil: legacy [level=1, item=1BF732, count=1].
        let mut legacy = Vec::new();
        for word in [1_u32, 0x1B_F732, 1] {
            legacy.extend_from_slice(&word.to_le_bytes());
        }
        assert_eq!(
            source_lvlo_item(&FieldValue::Bytes(legacy.into()), plugin, &interner),
            Some(fk(&interner, 0x1B_F732))
        );
        // WorkshopProduceScavenge: bare 4-byte item FormID.
        assert_eq!(
            source_lvlo_item(
                &FieldValue::Bytes(0x06_9081_u32.to_le_bytes().as_slice().into()),
                plugin,
                &interner
            ),
            Some(fk(&interner, 0x06_9081))
        );
        // The same 4 bytes decoded through the legacy struct layout.
        let misread = FieldValue::Struct(vec![
            (interner.intern("level"), FieldValue::Uint(0x9081)),
            (interner.intern("unknown_u8_1"), FieldValue::Uint(0x06)),
        ]);
        assert_eq!(
            source_lvlo_item(&misread, plugin, &interner),
            Some(fk(&interner, 0x06_9081))
        );
    }

    #[test]
    fn appends_ignore_occupied_snap_row_once_to_typed_and_raw_prps() {
        let interner = interner();
        let snap = FormKey {
            local: IGNORE_OCCUPIED_SNAP_LOCAL,
            plugin: interner.intern(FALLOUT4_PLUGIN),
        };

        let mut typed = cont(
            smallvec![field(
                "PRPS",
                FieldValue::List(vec![prps_row(&interner, 0x3C_3FA4, 1.0)]),
            )],
            &interner,
        );
        assert!(add_ignore_occupied_snap_row(
            &mut typed,
            &[],
            "SeventySix.esm",
            &interner
        ));
        assert!(!add_ignore_occupied_snap_row(
            &mut typed,
            &[],
            "SeventySix.esm",
            &interner
        ));
        let FieldValue::List(rows) = &typed.fields[0].value else {
            panic!("PRPS should stay typed");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1],
            FieldValue::Struct(vec![
                (
                    interner.intern("PropertiesActorValue"),
                    FieldValue::FormKey(snap)
                ),
                (interner.intern("PropertiesValue"), FieldValue::Float(1.0)),
            ])
        );

        // Raw rows: Fallout4.esm is master index 1 here, so the new row must
        // encode 01249E73, not a bare 00249E73.
        let masters = ["DLCRobot.esm".to_string(), "Fallout4.esm".to_string()];
        let mut prps = Vec::new();
        prps.extend_from_slice(&0x0100_02DCu32.to_le_bytes());
        prps.extend_from_slice(&2.0f32.to_le_bytes());
        let mut raw = cont(
            smallvec![field("PRPS", FieldValue::Bytes(prps.into()))],
            &interner,
        );
        assert!(add_ignore_occupied_snap_row(
            &mut raw,
            &masters,
            "SeventySix.esm",
            &interner
        ));
        assert!(!add_ignore_occupied_snap_row(
            &mut raw,
            &masters,
            "SeventySix.esm",
            &interner
        ));
        let FieldValue::Bytes(bytes) = &raw.fields[0].value else {
            panic!("PRPS should stay raw");
        };
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[8..12], &0x0124_9E73u32.to_le_bytes());
        assert_eq!(&bytes[12..16], &1.0f32.to_le_bytes());

        let mut without = cont(smallvec![], &interner);
        assert!(add_ignore_occupied_snap_row(
            &mut without,
            &[],
            "SeventySix.esm",
            &interner
        ));
        assert!(matches!(
            &without.fields[0].value,
            FieldValue::List(rows) if rows.len() == 1
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

        let spec = beehive_specs(&interner).into_values().next().expect("spec");
        let with_cap =
            collector_vmad(spec, Some(5), &[], "SeventySix.esm", &interner).expect("vmad with cap");
        let without_cap =
            collector_vmad(spec, None, &[], "SeventySix.esm", &interner).expect("vmad");
        assert!(String::from_utf8_lossy(&with_cap).contains("MaxStoredItems"));
        assert!(with_cap.ends_with(&[3, VMAD_PROPERTY_FLAG_EDITED, 5, 0, 0, 0]));
        assert!(!String::from_utf8_lossy(&without_cap).contains("MaxStoredItems"));
    }
}
