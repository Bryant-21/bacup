//! Fixup: harvest the correct FO4 `MODT` (model texture-hash manifest) from the
//! target masters for every model-bearing converted record whose model path is a
//! reused vanilla FO4 mesh.
//!
//! `MODT` is a deterministic function of the model path, so two records with the
//! same `MODL` share the same `MODT`. For a reused vanilla mesh the correct
//! `MODT` already exists on a vanilla record — we copy the opaque bytes keyed by
//! normalized model path. Paths with no vanilla match get their stale FO76 hash
//! subrecord dropped (Plan B computes those from the converted mesh later).

use esp_authoring_core::plugin_runtime::ParsedSubrecord;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::sync::OnceLock;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::modt_compute::decode_modt;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::{AuthoringSchema, RecordDef};
use crate::session::PluginSession;
use crate::sym::StringInterner;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelSlot {
    pub path_sig: String,
    pub hash_sig: String,
}

/// Derive every FO4 model-path/model-info pair from the authoritative target
/// schema. A `model_info` subrecord immediately follows its path definition in
/// every FO4 record scope, including DEBR DATA rows and repeated RACE scopes.
/// This avoids a record-signature allowlist becoming a correctness boundary.
pub(crate) fn fo4_model_slots() -> &'static FxHashMap<String, Vec<ModelSlot>> {
    static SLOTS: OnceLock<FxHashMap<String, Vec<ModelSlot>>> = OnceLock::new();
    SLOTS.get_or_init(|| {
        let schema = esp_authoring_core::schema_registry::schema_json_for_game("fo4")
            .expect("FO4 authoring schema must be registered");
        let value: serde_json::Value =
            serde_json::from_str(schema).expect("FO4 authoring schema must be valid JSON");
        let mut by_record = FxHashMap::default();

        for record in value["records"].as_array().into_iter().flatten() {
            let Some(record_sig) = record["id"].as_str() else {
                continue;
            };
            let Some(subrecords) = record["subrecords"].as_array() else {
                continue;
            };
            let mut slots = Vec::new();
            for pair in subrecords.windows(2) {
                if pair[1]["codec"].as_str() != Some("model_info") {
                    continue;
                }
                let Some(path_sig) = pair[0]["id"].as_str() else {
                    continue;
                };
                let Some(hash_sig) = pair[1]["id"].as_str() else {
                    continue;
                };
                let slot = ModelSlot {
                    path_sig: path_sig.to_string(),
                    hash_sig: hash_sig.to_string(),
                };
                if !slots.contains(&slot) {
                    slots.push(slot);
                }
            }
            if !slots.is_empty() {
                by_record.insert(record_sig.to_string(), slots);
            }
        }
        by_record
    })
}

/// Normalize a model path to the index key: lowercase, forward slashes, trailing
/// NUL/whitespace trimmed, and a leading `meshes/` root stripped (model-path
/// subrecords are stored relative to `Meshes\`, but some sources include it).
pub fn normalize_model_path(raw: &str) -> String {
    let trimmed = raw.trim_end_matches(['\0']).trim();
    let mut s: String = trimmed
        .chars()
        .map(|c| {
            if c == '\\' {
                '/'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    if let Some(rest) = s.strip_prefix("meshes/") {
        s = rest.to_string();
    }
    s
}

pub fn harvest_modt_index(
    records: &[Record],
    interner: &StringInterner,
    out: &mut FxHashMap<String, SmallVec<[u8; 32]>>,
) {
    for record in records {
        let Some(slots) = fo4_model_slots().get(record.sig.as_str()) else {
            continue;
        };
        for (index, field) in record.fields.iter().enumerate() {
            let Some(slot) = slots
                .iter()
                .find(|slot| field.sig.as_str() == slot.path_sig)
            else {
                continue;
            };
            let Some(FieldEntry {
                sig: hash_sig,
                value: FieldValue::Bytes(bytes),
            }) = record.fields.get(index + 1)
            else {
                continue;
            };
            if hash_sig.as_str() != slot.hash_sig || decode_modt(bytes).is_none() {
                continue;
            }
            let Some(path) =
                decoded_model_path(record.sig.as_str(), &slot.path_sig, &field.value, interner)
            else {
                continue;
            };
            let key = normalize_model_path(path);
            if !key.is_empty() {
                out.entry(key).or_insert_with(|| bytes.clone());
            }
        }
    }
}

/// A single harvested `(normalized model path, paired hash bytes)` pair.
type SlotHit = (String, SmallVec<[u8; 32]>);

/// Decode a model-path subrecord's raw bytes exactly like the authoring
/// `zstring` codec (`source_read::decode_zstring`): cut at the first NUL, UTF-8
/// with a Latin-1 fallback. Kept in lockstep so the shallow raw scan produces
/// the same pre-normalization string the full schema decode would.
fn decode_model_zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    let slice = &data[..end];
    match std::str::from_utf8(slice) {
        Ok(s) => s.to_string(),
        Err(_) => slice.iter().map(|&b| b as char).collect(),
    }
}

pub(crate) fn decode_debr_model_path(data: &[u8]) -> Option<String> {
    let path = data.get(1..)?;
    let nul = path.iter().position(|byte| *byte == 0)?;
    if nul == 0 || path.get(nul + 1).is_none() {
        return None;
    }
    Some(decode_model_zstring(&path[..nul]))
}

fn decoded_model_path<'a>(
    record_sig: &str,
    path_sig: &str,
    value: &'a FieldValue,
    interner: &'a StringInterner,
) -> Option<&'a str> {
    if record_sig == "DEBR" && path_sig == "DATA" {
        let FieldValue::Struct(fields) = value else {
            return None;
        };
        return fields.iter().find_map(|(name, value)| {
            (interner.resolve(*name) == Some("model_filename"))
                .then_some(value)
                .and_then(|value| match value {
                    FieldValue::String(path) => interner.resolve(*path),
                    _ => None,
                })
        });
    }
    match value {
        FieldValue::String(path) => interner.resolve(*path),
        _ => None,
    }
}

/// The effective codec of `(record_def, sig)` — mirrors the decode dispatch:
/// a `"raw"`-kind subrecord (or one with no schema entry) has no codec.
fn slot_codec<'a>(record_def: Option<&'a RecordDef>, sig: &str) -> Option<&'a str> {
    record_def
        .and_then(|rd| rd.subrecord_def(sig))
        .and_then(|sd| {
            if sd.kind == "raw" {
                None
            } else {
                sd.codec.as_deref()
            }
        })
}

/// Whether the full decode would emit `FieldValue::String` for this codec.
/// Model-path subrecords are `zstring`; that is the only codec `harvest_modt_index`
/// accepts on the path side (a `String` `FieldValue`).
fn codec_yields_string(codec: Option<&str>) -> bool {
    matches!(codec, Some("zstring"))
}

/// Whether the full decode would emit `FieldValue::Bytes` for this codec — the
/// only `FieldValue` `harvest_modt_index` accepts on the hash side. Mirrors
/// `source_read::decode_subrecord`: scalar / string / formid / packed-list codecs
/// yield typed values; everything else (`model_info`, `struct:*`, unmodeled, or
/// absent codec) falls back to raw bytes. `formid_array` is bytes only when its
/// length is not a multiple of 4.
fn codec_yields_bytes(codec: Option<&str>, data_len: usize) -> bool {
    match codec {
        None => true,
        Some(name) => match name {
            "zstring" | "lstring" | "bool" | "int8" | "uint8" | "int16" | "uint16" | "int32"
            | "uint32" | "int64" | "uint64" | "float32" | "formid" => false,
            "formid_array" => data_len % 4 != 0,
            _ => true,
        },
    }
}

/// Extract every `(normalized model path, paired hash bytes)` from one record's
/// raw subrecords — the shallow counterpart of [`harvest_modt_index`]'s per-slot
/// logic. A slot contributes only when the record type's schema would decode the
/// path subrecord to a `String` and the hash subrecord to `Bytes` (exactly the
/// two `FieldValue` variants `harvest_modt_index` accepts). Each path occurrence
/// is paired only with the immediately following model-info subrecord, so
/// repeated scoped rows cannot cross-pair.
fn scan_record_slots(
    record_sig: &str,
    subs: &[ParsedSubrecord],
    schema: &AuthoringSchema,
) -> SmallVec<[SlotHit; 4]> {
    let record_def = schema.record_def(record_sig);
    let mut out = SmallVec::new();
    let Some(slots) = fo4_model_slots().get(record_sig) else {
        return out;
    };
    for (index, path_sub) in subs.iter().enumerate() {
        let Some(slot) = slots
            .iter()
            .find(|slot| path_sub.signature.as_str() == slot.path_sig)
        else {
            continue;
        };
        let is_debr_data = record_sig == "DEBR" && slot.path_sig == "DATA";
        if !is_debr_data && !codec_yields_string(slot_codec(record_def, &slot.path_sig)) {
            continue;
        }
        let Some(hash_sub) = subs.get(index + 1) else {
            continue;
        };
        if hash_sub.signature.as_str() != slot.hash_sig
            || !codec_yields_bytes(slot_codec(record_def, &slot.hash_sig), hash_sub.data.len())
            || decode_modt(&hash_sub.data).is_none()
        {
            continue;
        }
        let path = if is_debr_data {
            decode_debr_model_path(&path_sub.data)
        } else {
            Some(decode_model_zstring(&path_sub.data))
        };
        let Some(path) = path else {
            continue;
        };
        let key = normalize_model_path(&path);
        if key.is_empty() {
            continue;
        }
        out.push((key, hash_sub.data.iter().copied().collect()));
    }
    out
}

pub fn apply_harvested_modt(
    record: &mut Record,
    index: &FxHashMap<String, SmallVec<[u8; 32]>>,
    interner: &StringInterner,
) -> u32 {
    let Some(slots) = fo4_model_slots().get(record.sig.as_str()) else {
        return 0;
    };

    let mut source = std::mem::take(&mut record.fields).into_iter().peekable();
    let mut rebuilt = SmallVec::new();
    let mut changed = 0u32;
    while let Some(field) = source.next() {
        let Some(slot) = slots
            .iter()
            .find(|slot| field.sig.as_str() == slot.path_sig)
        else {
            if slots.iter().any(|slot| field.sig.as_str() == slot.hash_sig) {
                changed += 1;
            } else {
                rebuilt.push(field);
            }
            continue;
        };

        let key = decoded_model_path(record.sig.as_str(), &slot.path_sig, &field.value, interner)
            .map(normalize_model_path);
        rebuilt.push(field);

        let existing = source
            .peek()
            .is_some_and(|next| next.sig.as_str() == slot.hash_sig)
            .then(|| source.next().expect("peeked model-info field"));
        let replacement = key.as_deref().and_then(|path| index.get(path)).cloned();
        let existing_value = existing.as_ref().map(|entry| entry.value.clone());
        let replacement_value = replacement.clone().map(FieldValue::Bytes);
        if replacement_value != existing_value {
            changed += 1;
        }
        if let Some(bytes) = replacement {
            rebuilt.push(FieldEntry {
                sig: SubrecordSig::from_str(&slot.hash_sig).expect("schema subrecord signature"),
                value: FieldValue::Bytes(bytes),
            });
        }
    }
    record.fields = rebuilt;
    changed
}

pub struct HarvestModtFixup;

impl Fixup for HarvestModtFixup {
    fn name(&self) -> &'static str {
        "harvest_modt"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        // Needs the target masters to harvest from.
        !config.target_master_handle_ids.is_empty()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        use rayon::prelude::*;

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let interner: &StringInterner = mapper.interner;

        // ── 1. Build the harvest index from the masters via a SHALLOW raw scan ──
        // For every model-bearing record we only need two raw subrecords (the
        // model path and its paired hash); the full authoring-schema decode the
        // old path ran on every master record was pure waste and the dominant
        // cost. Candidate signatures and slot pairs come from the FO4 schema;
        // the per-record scan is read-only, so each signature sweep is parallel.
        let mut index: FxHashMap<String, SmallVec<[u8; 32]>> = FxHashMap::default();
        let mut model_signatures = fo4_model_slots().keys().cloned().collect::<Vec<_>>();
        model_signatures.sort_unstable();
        for sig_str in &model_signatures {
            let sig = match SigCode::from_str(sig_str) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for &handle in &config.target_master_handle_ids {
                let scan = match session.handle_raw_scan(handle) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let raw_ids = scan.raw_form_ids_of_sig(sig);
                let per_record: Vec<SmallVec<[SlotHit; 4]>> = raw_ids
                    .par_iter()
                    .map(|&raw_id| {
                        scan.with_record_subrecords(raw_id, |subs| {
                            scan_record_slots(sig_str, subs, target_schema)
                        })
                        .unwrap_or_default()
                    })
                    .collect();
                for hits in per_record {
                    for (key, bytes) in hits {
                        index.entry(key).or_insert(bytes);
                    }
                }
            }
        }

        let mut report = FixupReport::empty();
        // ── 2. Apply to every converted record signature ───────────────────
        // Decode against the original target, then write the changed records in a
        // single batch: deferring the structural writes keeps the target's
        // core/locator sections cached across the whole sweep (a per-record
        // `replace_record` invalidates and rebuilds them, which is quadratic on
        // large outputs). No record's decode depends on another's post-write
        // state, so the batched result is identical to the sequential one.
        let mut changed: Vec<Record> = Vec::new();
        let mut target_signatures = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        target_signatures.sort_unstable_by_key(|sig| sig.0);
        for sig in target_signatures {
            if !fo4_model_slots().contains_key(sig.as_str()) {
                continue;
            }
            let fks = session
                .form_keys_of_sig(sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                let mut record = match session.record_decoded(&fk, target_schema, interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if apply_harvested_modt(&mut record, &index, interner) > 0 {
                    changed.push(record);
                }
            }
        }

        report.records_changed = changed.len() as u32;
        session
            .replace_records(changed, target_schema, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modt_compute::encode_modt;
    use crate::modt_manifest::MeshModtEntry;

    #[test]
    fn normalize_lowercases_backslashes_and_strips_meshes_root() {
        assert_eq!(
            normalize_model_path("Meshes\\SetDressing\\MetalBarrel\\MetalBarrel01Fire.NIF"),
            "setdressing/metalbarrel/metalbarrel01fire.nif"
        );
        // No meshes\ prefix — just normalized.
        assert_eq!(
            normalize_model_path("Ammo\\Syringer\\SyringerAmmo.nif"),
            "ammo/syringer/syringerammo.nif"
        );
        // Trailing NUL from raw subrecord strings is trimmed.
        assert_eq!(normalize_model_path("Foo\\Bar.nif\0"), "foo/bar.nif");
    }

    #[test]
    fn schema_derived_slots_cover_every_fo4_model_info_context() {
        let slots = fo4_model_slots();
        assert_eq!(slots.len(), 45);
        for signature in [
            "ANIO", "ARTO", "CAMS", "CLMT", "IPCT", "MATO", "WRLD", "WTHR",
        ] {
            assert!(slots.contains_key(signature), "missing {signature}");
        }
        assert!(slots["ARMA"].contains(&ModelSlot {
            path_sig: "MOD5".to_string(),
            hash_sig: "MO5T".to_string(),
        }));
        assert!(slots["MATT"].contains(&ModelSlot {
            path_sig: "ANAM".to_string(),
            hash_sig: "MODT".to_string(),
        }));
        assert!(slots["RACE"].contains(&ModelSlot {
            path_sig: "MODL".to_string(),
            hash_sig: "MODT".to_string(),
        }));
    }

    use crate::ids::SigCode;
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;

    fn rec(sig: &str, fields: Vec<(&str, FieldValue)>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: crate::ids::FormKey {
                local: 1,
                plugin: interner.intern("M.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields
                .into_iter()
                .map(|(s, v)| FieldEntry {
                    sig: SubrecordSig::from_str(s).unwrap(),
                    value: v,
                })
                .collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }
    fn s(interner: &StringInterner, path: &str) -> FieldValue {
        FieldValue::String(interner.intern(path))
    }
    fn b(bytes: &[u8]) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_slice(bytes))
    }
    fn valid_modt(marker: u32) -> SmallVec<[u8; 32]> {
        SmallVec::from_vec(encode_modt(&MeshModtEntry {
            addon_nodes: vec![marker],
            ..Default::default()
        }))
    }
    fn field_value<'a>(record: &'a Record, sig: &str) -> Option<&'a FieldValue> {
        record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == sig)
            .map(|field| &field.value)
    }

    #[test]
    fn index_collects_paired_path_and_hash_per_slot() {
        let interner = StringInterner::new();
        let stat_modt = valid_modt(1);
        let armo_modt = valid_modt(2);
        let records = vec![
            rec(
                "STAT",
                vec![
                    ("MODL", s(&interner, "A\\B.nif")),
                    ("MODT", FieldValue::Bytes(stat_modt.clone())),
                ],
                &interner,
            ),
            // ARMO carries an alt slot (MOD2/MO2T).
            rec(
                "ARMO",
                vec![
                    ("MOD2", s(&interner, "C\\D.nif")),
                    ("MO2T", FieldValue::Bytes(armo_modt.clone())),
                ],
                &interner,
            ),
            // Path with no hash — not indexed.
            rec("MISC", vec![("MODL", s(&interner, "E\\F.nif"))], &interner),
        ];
        let mut index = FxHashMap::default();
        harvest_modt_index(&records, &interner, &mut index);
        assert_eq!(
            index.get("a/b.nif").map(|v| v.as_slice()),
            Some(stat_modt.as_slice())
        );
        assert_eq!(
            index.get("c/d.nif").map(|v| v.as_slice()),
            Some(armo_modt.as_slice())
        );
        assert!(!index.contains_key("e/f.nif"));
    }

    #[test]
    fn index_is_first_wins_on_duplicate_paths() {
        let interner = StringInterner::new();
        let first = valid_modt(1);
        let second = valid_modt(9);
        let records = vec![
            rec(
                "STAT",
                vec![
                    ("MODL", s(&interner, "A\\B.nif")),
                    ("MODT", FieldValue::Bytes(first.clone())),
                ],
                &interner,
            ),
            rec(
                "STAT",
                vec![
                    ("MODL", s(&interner, "a\\b.NIF")),
                    ("MODT", FieldValue::Bytes(second)),
                ],
                &interner,
            ),
        ];
        let mut index = FxHashMap::default();
        harvest_modt_index(&records, &interner, &mut index);
        assert_eq!(
            index.get("a/b.nif").map(|v| v.as_slice()),
            Some(first.as_slice())
        );
    }

    #[test]
    fn apply_inserts_hash_after_path_when_absent() {
        let interner = StringInterner::new();
        let mut index = FxHashMap::default();
        index.insert("a/b.nif".to_string(), SmallVec::from_slice(&[7u8, 7, 7]));
        // MODL present, MODT absent; OBND before, MODC after MODL.
        let mut r = rec(
            "STAT",
            vec![
                ("OBND", b(&[0; 12])),
                ("MODL", s(&interner, "A\\B.nif")),
                ("MODC", b(&[0; 4])),
            ],
            &interner,
        );
        assert_eq!(apply_harvested_modt(&mut r, &index, &interner), 1);
        let sigs: Vec<String> = r
            .fields
            .iter()
            .map(|e| e.sig.as_str().to_string())
            .collect();
        assert_eq!(sigs, vec!["OBND", "MODL", "MODT", "MODC"]);
        assert_eq!(field_value(&r, "MODT"), Some(&b(&[7, 7, 7])));
    }

    #[test]
    fn apply_overwrites_stale_hash_in_place() {
        let interner = StringInterner::new();
        let mut index = FxHashMap::default();
        index.insert("a/b.nif".to_string(), SmallVec::from_slice(&[7u8]));
        let mut r = rec(
            "STAT",
            vec![
                ("MODL", s(&interner, "A\\B.nif")),
                ("MODT", b(&[1, 2, 3, 4])),
            ],
            &interner,
        );
        assert_eq!(apply_harvested_modt(&mut r, &index, &interner), 1);
        assert_eq!(field_value(&r, "MODT"), Some(&b(&[7])));
        // No duplicate MODT.
        assert_eq!(
            r.fields.iter().filter(|e| e.sig.as_str() == "MODT").count(),
            1
        );
    }

    #[test]
    fn apply_drops_stale_hash_when_path_unknown() {
        let interner = StringInterner::new();
        let index = FxHashMap::default();
        let mut r = rec(
            "STAT",
            vec![
                ("MODL", s(&interner, "Novel\\Mesh.nif")),
                ("MODT", b(&[1, 2])),
            ],
            &interner,
        );
        assert_eq!(apply_harvested_modt(&mut r, &index, &interner), 1);
        assert_eq!(field_value(&r, "MODT"), None);
    }

    #[test]
    fn apply_noop_when_no_hash_and_path_unknown() {
        let interner = StringInterner::new();
        let index = FxHashMap::default();
        let mut r = rec(
            "STAT",
            vec![("MODL", s(&interner, "Novel\\Mesh.nif"))],
            &interner,
        );
        assert_eq!(apply_harvested_modt(&mut r, &index, &interner), 0);
        assert_eq!(field_value(&r, "MODT"), None);
    }

    #[test]
    fn apply_processes_repeated_race_rows_without_cross_pairing() {
        let interner = StringInterner::new();
        let first = valid_modt(1);
        let second = valid_modt(2);
        let mut index = FxHashMap::default();
        index.insert("actors/racea.nif".to_string(), first.clone());
        index.insert("actors/bodya.nif".to_string(), second.clone());
        let mut race = rec(
            "RACE",
            vec![
                ("ANAM", s(&interner, "Actors\\RaceA.nif")),
                ("MODT", b(b"LEGACY-A")),
                ("MODL", s(&interner, "Actors\\BodyA.nif")),
                ("MODT", b(b"LEGACY-B")),
                ("MODL", s(&interner, "Actors\\Missing.nif")),
                ("MODT", b(b"LEGACY-C")),
            ],
            &interner,
        );

        assert_eq!(apply_harvested_modt(&mut race, &index, &interner), 3);
        assert_eq!(
            race.fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["ANAM", "MODT", "MODL", "MODT", "MODL"]
        );
        let model_info = race
            .fields
            .iter()
            .filter_map(|field| match &field.value {
                FieldValue::Bytes(bytes) if field.sig.as_str() == "MODT" => Some(bytes),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(model_info, vec![&first, &second]);
    }

    #[test]
    fn decoded_debr_uses_authoritative_model_filename_field() {
        let interner = StringInterner::new();
        let valid = valid_modt(7);
        let data = FieldValue::Struct(vec![
            (interner.intern("percentage"), FieldValue::Uint(100)),
            (
                interner.intern("model_filename"),
                s(&interner, "Effects\\DebrisA.nif"),
            ),
            (interner.intern("has_collision"), FieldValue::Uint(1)),
        ]);
        let mut debris = rec(
            "DEBR",
            vec![
                ("DATA", data.clone()),
                ("MODT", FieldValue::Bytes(valid.clone())),
            ],
            &interner,
        );

        let mut harvested = FxHashMap::default();
        harvest_modt_index(std::slice::from_ref(&debris), &interner, &mut harvested);
        assert_eq!(
            harvested
                .get("effects/debrisa.nif")
                .map(|bytes| bytes.as_slice()),
            Some(valid.as_slice())
        );

        debris.fields[1].value = b(b"SKYRIM-LEGACY");
        assert_eq!(apply_harvested_modt(&mut debris, &harvested, &interner), 1);
        assert_eq!(debris.fields[1].value, FieldValue::Bytes(valid));
    }

    /// Proof the optimization is behavior-preserving: build the harvest index
    /// via the new SHALLOW raw-subrecord scan AND via the OLD full-decode path on
    /// the same fixture (multiple slots, a multi-slot record, first-wins on a
    /// duplicate path, path-without-hash, hash-without-path) and assert the two
    /// indexes are byte-identical.
    #[test]
    fn shallow_scan_index_matches_full_decode_index() {
        use crate::session::open_session;
        use bytes::Bytes;
        use esp_authoring_core::plugin_runtime::{
            ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_new_native,
            plugin_handle_store_ref,
        };
        use smol_str::SmolStr;

        fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
            ParsedSubrecord {
                signature: SmolStr::new(sig),
                data: Bytes::from(data),
                semantic_type: None,
            }
        }
        fn zbytes(path: &str) -> Vec<u8> {
            let mut v = path.as_bytes().to_vec();
            v.push(0);
            v
        }
        fn record(sig: &str, form_id: u32, subs: Vec<ParsedSubrecord>) -> ParsedItem {
            ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new(sig),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: None,
                version2: None,
                subrecords: subs,
                raw_payload: None,
                parse_error: None,
            })
        }
        fn group(label: &[u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
            ParsedItem::Group(ParsedGroup {
                label: *label,
                group_type: 0,
                tail: Bytes::new(),
                children,
            })
        }

        let Ok(handle) = plugin_handle_new_native("HarvestParity.esm", Some("fo4")) else {
            return;
        };
        let barrel_modt = valid_modt(0xaa).to_vec();
        let duplicate_modt = valid_modt(0x11).to_vec();
        let armor_modt = valid_modt(0x22).to_vec();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.root_items = vec![
                group(
                    b"STAT",
                    vec![
                        // First-wins winner for setdressing/barrel01.nif.
                        record(
                            "STAT",
                            0x0000_0801,
                            vec![
                                sub("MODL", zbytes("SetDressing\\Barrel01.nif")),
                                sub("MODT", barrel_modt.clone()),
                            ],
                        ),
                        // Same normalized path, different hash — must lose.
                        record(
                            "STAT",
                            0x0000_0802,
                            vec![
                                sub("MODL", zbytes("setdressing\\barrel01.NIF")),
                                sub("MODT", duplicate_modt),
                            ],
                        ),
                        // MODT with no MODL — skipped.
                        record("STAT", 0x0000_0803, vec![sub("MODT", vec![0x99])]),
                    ],
                ),
                group(
                    b"ARMO",
                    vec![record(
                        "ARMO",
                        0x0000_0810,
                        vec![
                            sub("MODL", zbytes("Armor\\LeatherBody.nif")),
                            sub("MODT", vec![0x03]),
                            sub("MOD2", zbytes("Armor\\Leather.nif")),
                            sub("MO2T", armor_modt.clone()),
                        ],
                    )],
                ),
                group(
                    b"WEAP",
                    vec![record(
                        "WEAP",
                        0x0000_0820,
                        vec![
                            sub("NAM1", zbytes("Weapons\\Pipe\\Casing.nif")),
                            sub("NAM2", vec![0x44, 0x55]),
                        ],
                    )],
                ),
                group(
                    b"MISC",
                    vec![record(
                        "MISC",
                        0x0000_0830,
                        // MODL with no MODT — skipped.
                        vec![sub("MODL", zbytes("Misc\\Thing.nif"))],
                    )],
                ),
            ];
            slot.invalidate_sections();
        }

        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let mut session = open_session(handle, None).unwrap();

        // OLD path: full authoring-schema decode of every model-bearing record.
        let mut old_index: FxHashMap<String, SmallVec<[u8; 32]>> = FxHashMap::default();
        for sig_str in fo4_model_slots().keys() {
            let Ok(sig) = SigCode::from_str(sig_str) else {
                continue;
            };
            let Ok(fks) = session.form_keys_of_sig_in_handle(handle, sig, &interner) else {
                continue;
            };
            let mut records: Vec<Record> = Vec::new();
            for fk in fks {
                if let Ok(rec) = session.record_decoded_in_handle(handle, &fk, &schema, &interner) {
                    records.push(rec);
                }
            }
            harvest_modt_index(&records, &interner, &mut old_index);
        }

        // NEW path: shallow raw-subrecord scan.
        let mut new_index: FxHashMap<String, SmallVec<[u8; 32]>> = FxHashMap::default();
        for sig_str in fo4_model_slots().keys() {
            let Ok(sig) = SigCode::from_str(sig_str) else {
                continue;
            };
            let scan = session.handle_raw_scan(handle).unwrap();
            for raw_id in scan.raw_form_ids_of_sig(sig) {
                let hits = scan
                    .with_record_subrecords(raw_id, |subs| {
                        scan_record_slots(sig_str, subs, &schema)
                    })
                    .unwrap_or_default();
                for (key, bytes) in hits {
                    new_index.entry(key).or_insert(bytes);
                }
            }
        }

        assert!(
            !old_index.is_empty(),
            "fixture should produce a non-empty index"
        );
        assert_eq!(
            old_index, new_index,
            "shallow scan diverged from full decode"
        );

        // Positive slots (schema-defined model paths, harvested by both paths).
        assert_eq!(
            new_index
                .get("setdressing/barrel01.nif")
                .map(|v| v.as_slice()),
            Some(barrel_modt.as_slice()),
            "STAT MODL/MODT, first-wins on the duplicate normalized path"
        );
        assert_eq!(
            new_index.get("armor/leather.nif").map(|v| v.as_slice()),
            Some(armor_modt.as_slice()),
            "ARMO MOD2/MO2T alt slot"
        );
        // Path present in the raw data but NOT a schema-defined model path for
        // its record type — the full decode yields Bytes (not String) for it and
        // skips it, so the shallow scan's schema gate must skip it too.
        assert!(
            !new_index.contains_key("armor/leatherbody.nif"),
            "ARMO has no MODL model slot — must be gated out"
        );
        assert!(
            !new_index.contains_key("weapons/pipe/casing.nif"),
            "WEAP NAM1/NAM2 is not a model slot — must be gated out"
        );
        // MISC.MODL is a real model slot, but this record has no paired hash.
        assert!(
            !new_index.contains_key("misc/thing.nif"),
            "path without hash is not indexed"
        );
    }
}
