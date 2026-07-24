//! Semantic WRLD normalization shared by the FNV and FO3 FO4 converters.

use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const LEGACY_SMALL_WORLD: u8 = 1 << 0;
const LEGACY_CANT_FAST_TRAVEL: u8 = 1 << 1;
const LEGACY_NO_LOD_WATER: u8 = 1 << 4;
const FO4_NO_LOD_WATER: u8 = 1 << 3;
const SHARED_FLAGS: u8 = LEGACY_SMALL_WORLD | LEGACY_CANT_FAST_TRAVEL;
const LEGACY_PRESERVED_FLAGS: u8 = SHARED_FLAGS | LEGACY_NO_LOD_WATER;

const REFERENCE_SIGS: [[u8; 4]; 4] = [*b"CNAM", *b"NAM2", *b"NAM3", *b"ZNAM"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrldSourceFamily {
    Fnv,
    Fo3,
    Fo76,
    Fo4,
}

impl WrldSourceFamily {
    fn is_legacy(self) -> bool {
        matches!(self, Self::Fnv | Self::Fo3)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrldDataChange {
    pub field_index: usize,
    pub source_flags: u8,
    pub target_flags: u8,
    pub dropped_source_flags: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrldReferenceState {
    Missing,
    PreservedValid,
    PreservedInvalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrldReferenceEvidence {
    pub sig: [u8; 4],
    pub state: WrldReferenceState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrldNormalizationReport {
    pub source: WrldSourceFamily,
    pub applied: bool,
    pub data_changes: Vec<WrldDataChange>,
    pub synthesized_data_default: bool,
    pub dropped_inam_fields: usize,
    pub references: Vec<WrldReferenceEvidence>,
}

impl WrldNormalizationReport {
    fn not_applied(source: WrldSourceFamily) -> Self {
        Self {
            source,
            applied: false,
            data_changes: Vec::new(),
            synthesized_data_default: false,
            dropped_inam_fields: 0,
            references: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrldNormalizationError {
    UnsupportedDataValue {
        field_index: usize,
    },
    DuplicateDataFields {
        first_index: usize,
        duplicate_index: usize,
    },
}

impl std::fmt::Display for WrldNormalizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedDataValue { field_index } => {
                write!(
                    formatter,
                    "unsupported WRLD DATA value at field {field_index}"
                )
            }
            Self::DuplicateDataFields {
                first_index,
                duplicate_index,
            } => write!(
                formatter,
                "duplicate WRLD DATA fields at indexes {first_index} and {duplicate_index}"
            ),
        }
    }
}

impl std::error::Error for WrldNormalizationError {}

/// Map FNV/FO3 WRLD DATA bits by meaning instead of reusing their bit positions.
///
/// Bits 0 and 1 are shared. Legacy `No LOD Water` moves from bit 4 to FO4 bit 3.
/// Every other source bit is intentionally discarded: FO4 assigns bits 4-7 to
/// unrelated `No Landscape`, `No Sky`, `Fixed Dimensions`, and `No Grass` flags.
pub const fn map_legacy_wrld_data_flags(source: u8) -> u8 {
    (source & SHARED_FLAGS)
        | if source & LEGACY_NO_LOD_WATER != 0 {
            FO4_NO_LOD_WATER
        } else {
            0
        }
}

/// Normalize one source-shaped WRLD for FO4 and return an auditable change report.
///
/// FNV/FO3 DATA is rewritten atomically. A missing DATA receives the zero default
/// specified by the reference conversion; an undecodable present DATA returns an
/// error and leaves the record unchanged. Reference fields are evidence only:
/// valid CNAM/NAM2/NAM3/ZNAM values are preserved and missing values stay missing.
pub fn normalize_wrld_for_fo4(
    record: &mut Record,
    source: WrldSourceFamily,
    interner: &StringInterner,
) -> Result<WrldNormalizationReport, WrldNormalizationError> {
    if record.sig.0 != *b"WRLD" || !source.is_legacy() {
        return Ok(WrldNormalizationReport::not_applied(source));
    }

    let source_data: Vec<_> = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.sig.0 == *b"DATA")
        .map(|(field_index, entry)| {
            legacy_data_flags(&entry.value, interner)
                .map(|flags| (field_index, flags))
                .ok_or(WrldNormalizationError::UnsupportedDataValue { field_index })
        })
        .collect::<Result<_, _>>()?;
    if let [first, duplicate, ..] = source_data.as_slice() {
        return Err(WrldNormalizationError::DuplicateDataFields {
            first_index: first.0,
            duplicate_index: duplicate.0,
        });
    }

    let data_changes = source_data
        .iter()
        .map(|(field_index, source_flags)| WrldDataChange {
            field_index: *field_index,
            source_flags: *source_flags,
            target_flags: map_legacy_wrld_data_flags(*source_flags),
            dropped_source_flags: *source_flags & !LEGACY_PRESERVED_FLAGS,
        })
        .collect::<Vec<_>>();

    for change in &data_changes {
        record.fields[change.field_index].value = FieldValue::Uint(u64::from(change.target_flags));
    }

    let synthesized_data_default = data_changes.is_empty();
    if synthesized_data_default {
        let insert_at = record
            .fields
            .iter()
            .position(|entry| {
                matches!(
                    &entry.sig.0,
                    b"NAM0" | b"NAM9" | b"ZNAM" | b"NNAM" | b"XNAM" | b"OFST"
                )
            })
            .unwrap_or(record.fields.len());
        record.fields.insert(
            insert_at,
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Uint(0),
            },
        );
    }

    let before_drop = record.fields.len();
    record.fields.retain(|entry| entry.sig.0 != *b"INAM");
    let dropped_inam_fields = before_drop - record.fields.len();
    let references = reference_evidence(record);

    Ok(WrldNormalizationReport {
        source,
        applied: true,
        data_changes,
        synthesized_data_default,
        dropped_inam_fields,
        references,
    })
}

fn legacy_data_flags(value: &FieldValue, interner: &StringInterner) -> Option<u8> {
    match value {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if matches!(bytes.len(), 1 | 4) => Some(bytes[0]),
        FieldValue::String(value) => legacy_flag_name(interner.resolve(*value)?),
        FieldValue::List(values) => values.iter().try_fold(0_u8, |flags, value| {
            Some(flags | legacy_data_flags(value, interner)?)
        }),
        _ => None,
    }
}

fn legacy_flag_name(name: &str) -> Option<u8> {
    let normalized = name
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    match normalized.as_slice() {
        b"smallworld" => Some(LEGACY_SMALL_WORLD),
        b"cantfasttravel" => Some(LEGACY_CANT_FAST_TRAVEL),
        b"nolodwater" => Some(LEGACY_NO_LOD_WATER),
        b"nolodnoise" => Some(1 << 5),
        b"dontallownpcfalldamage" => Some(1 << 6),
        b"needswateradjustment" => Some(1 << 7),
        _ => None,
    }
}

fn reference_evidence(record: &Record) -> Vec<WrldReferenceEvidence> {
    REFERENCE_SIGS
        .into_iter()
        .map(|sig| {
            let matching = record.fields.iter().filter(|entry| entry.sig.0 == sig);
            let mut found = false;
            let mut valid = false;
            for entry in matching {
                found = true;
                valid |= reference_value_is_valid(&entry.value);
            }
            let state = match (found, valid) {
                (false, _) => WrldReferenceState::Missing,
                (true, true) => WrldReferenceState::PreservedValid,
                (true, false) => WrldReferenceState::PreservedInvalid,
            };
            WrldReferenceEvidence { sig, state }
        })
        .collect()
}

fn reference_value_is_valid(value: &FieldValue) -> bool {
    match value {
        FieldValue::FormKey(form_key) => form_key.local != 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[..4].try_into().expect("four-byte prefix")) != 0
        }
        FieldValue::Uint(value) => *value != 0,
        FieldValue::Int(value) => *value > 0,
        FieldValue::List(values) => values.iter().any(reference_value_is_valid),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| reference_value_is_valid(value)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use smallvec::SmallVec;
    use std::collections::HashSet;

    fn record(interner: &StringInterner, local: u32, plugin: &str) -> Record {
        Record::new(
            SigCode(*b"WRLD"),
            FormKey {
                local,
                plugin: interner.intern(plugin),
            },
        )
    }

    fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    fn form_key(interner: &StringInterner, local: u32, plugin: &str) -> FieldValue {
        FieldValue::FormKey(FormKey {
            local,
            plugin: interner.intern(plugin),
        })
    }

    fn data_value(record: &Record) -> &FieldValue {
        &record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DATA")
            .expect("DATA")
            .value
    }

    fn reference_state(report: &WrldNormalizationReport, sig: &[u8; 4]) -> WrldReferenceState {
        report
            .references
            .iter()
            .find(|evidence| evidence.sig == *sig)
            .expect("reference evidence")
            .state
    }

    #[test]
    fn fnv_wrld_wastelandnv_golden_uses_source_bit_meaning() {
        let interner = StringInterner::new();
        let mut wasteland = record(&interner, 0x0D_A726, "FalloutNV.esm");
        let climate = form_key(&interner, 0x08_809B, "FalloutNV.esm");
        let water = form_key(&interner, 0x03_0009, "FalloutNV.esm");
        let image_space = form_key(&interner, 0x08_809D, "FalloutNV.esm");
        wasteland.fields.extend([
            field(b"CNAM", climate.clone()),
            field(b"NAM2", water.clone()),
            field(b"NAM3", water.clone()),
            field(b"INAM", image_space),
            field(b"DATA", FieldValue::Bytes(SmallVec::from_slice(&[0x80]))),
        ]);

        let report = normalize_wrld_for_fo4(&mut wasteland, WrldSourceFamily::Fnv, &interner)
            .expect("WastelandNV normalization");

        assert_eq!(data_value(&wasteland), &FieldValue::Uint(0));
        assert!(!wasteland.fields.iter().any(|entry| entry.sig.0 == *b"INAM"));
        assert_eq!(report.dropped_inam_fields, 1);
        assert_eq!(report.data_changes[0].source_flags, 0x80);
        assert_eq!(report.data_changes[0].target_flags, 0);
        assert_eq!(report.data_changes[0].dropped_source_flags, 0x80);
        assert_eq!(
            reference_state(&report, b"CNAM"),
            WrldReferenceState::PreservedValid
        );
        assert_eq!(
            reference_state(&report, b"NAM2"),
            WrldReferenceState::PreservedValid
        );
        assert_eq!(
            reference_state(&report, b"NAM3"),
            WrldReferenceState::PreservedValid
        );
        assert_eq!(
            reference_state(&report, b"ZNAM"),
            WrldReferenceState::Missing
        );
        assert_eq!(
            wasteland
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"CNAM")
                .map(|entry| &entry.value),
            Some(&climate)
        );
        assert_eq!(
            wasteland
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"NAM2")
                .map(|entry| &entry.value),
            Some(&water)
        );
    }

    #[test]
    fn fnv_wrld_all_source_bits_cannot_become_unrelated_fo4_flags() {
        let interner = StringInterner::new();
        let mut world = record(&interner, 1, "FalloutNV.esm");
        world.fields.push(field(b"DATA", FieldValue::Uint(0xFF)));

        let report = normalize_wrld_for_fo4(&mut world, WrldSourceFamily::Fnv, &interner)
            .expect("all-bit normalization");

        assert_eq!(data_value(&world), &FieldValue::Uint(0x0B));
        assert_eq!(report.data_changes[0].dropped_source_flags, 0xEC);
        assert_eq!(map_legacy_wrld_data_flags(0xFF) & 0xF0, 0);
    }

    #[test]
    fn fnv_wrld_fo3_and_fnv_are_legacy_but_fo76_and_fo4_are_noops() {
        let interner = StringInterner::new();
        for source in [WrldSourceFamily::Fnv, WrldSourceFamily::Fo3] {
            let mut world = record(&interner, 1, "legacy.esm");
            world.fields.push(field(b"DATA", FieldValue::Uint(0x13)));
            let report = normalize_wrld_for_fo4(&mut world, source, &interner).unwrap();
            assert!(report.applied);
            assert_eq!(report.source, source);
            assert_eq!(data_value(&world), &FieldValue::Uint(0x0B));
        }

        for source in [WrldSourceFamily::Fo76, WrldSourceFamily::Fo4] {
            let mut world = record(&interner, 1, "target.esm");
            world.fields.push(field(b"INAM", FieldValue::Uint(1)));
            world.fields.push(field(b"DATA", FieldValue::Uint(0xF8)));
            let before = world.fields.clone();
            let report = normalize_wrld_for_fo4(&mut world, source, &interner).unwrap();
            assert!(!report.applied);
            assert_eq!(world.fields, before);
        }
    }

    #[test]
    fn fnv_wrld_missing_data_gets_only_the_proto_default_and_no_references() {
        let interner = StringInterner::new();
        let mut world = record(&interner, 1, "Fallout3.esm");
        world.fields.extend([
            field(b"EDID", FieldValue::None),
            field(b"ZNAM", FieldValue::Uint(0x1234)),
        ]);

        let report = normalize_wrld_for_fo4(&mut world, WrldSourceFamily::Fo3, &interner)
            .expect("missing DATA normalization");

        assert!(report.synthesized_data_default);
        assert_eq!(data_value(&world), &FieldValue::Uint(0));
        assert_eq!(
            world
                .fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "DATA", "ZNAM"]
        );
        for sig in REFERENCE_SIGS {
            let expected = if sig == *b"ZNAM" {
                WrldReferenceState::PreservedValid
            } else {
                WrldReferenceState::Missing
            };
            assert_eq!(reference_state(&report, &sig), expected);
        }
    }

    #[test]
    fn fnv_wrld_four_byte_data_uses_only_the_engine_visible_first_byte() {
        let interner = StringInterner::new();
        let mut world = record(&interner, 1, "FalloutNV.esm");
        world.fields.push(field(
            b"DATA",
            FieldValue::Bytes(SmallVec::from_slice(&[0x13, 0xFF, 0xFF, 0xFF])),
        ));

        let report = normalize_wrld_for_fo4(&mut world, WrldSourceFamily::Fnv, &interner)
            .expect("four-byte DATA normalization");

        assert_eq!(data_value(&world), &FieldValue::Uint(0x0B));
        assert_eq!(report.data_changes[0].source_flags, 0x13);
    }

    #[test]
    fn fnv_wrld_duplicate_data_is_atomic() {
        let interner = StringInterner::new();
        let mut world = record(&interner, 1, "Fallout3.esm");
        world.fields.extend([
            field(b"INAM", FieldValue::Uint(1)),
            field(b"DATA", FieldValue::Uint(0x10)),
            field(b"DATA", FieldValue::Uint(0x20)),
        ]);
        let before = world.fields.clone();

        let error = normalize_wrld_for_fo4(&mut world, WrldSourceFamily::Fo3, &interner)
            .expect_err("duplicate DATA must fail");

        assert_eq!(
            error,
            WrldNormalizationError::DuplicateDataFields {
                first_index: 1,
                duplicate_index: 2,
            }
        );
        assert_eq!(world.fields, before);
    }

    #[test]
    fn fnv_wrld_unsupported_present_data_is_atomic_and_not_treated_as_missing() {
        let interner = StringInterner::new();
        let mut world = record(&interner, 1, "FalloutNV.esm");
        world.fields.push(field(b"INAM", FieldValue::Uint(1)));
        world.fields.push(field(
            b"DATA",
            FieldValue::Bytes(SmallVec::from_slice(&[0x80, 0x01])),
        ));
        let before = world.fields.clone();

        let error = normalize_wrld_for_fo4(&mut world, WrldSourceFamily::Fnv, &interner)
            .expect_err("malformed DATA must fail");

        assert_eq!(
            error,
            WrldNormalizationError::UnsupportedDataValue { field_index: 1 }
        );
        assert_eq!(world.fields, before);
    }

    #[test]
    fn fnv_wrld_affected_merged_corpus_has_the_expected_25_semantic_changes() {
        let cases: [(&str, WrldSourceFamily, u8, u8); 25] = [
            ("031E12", WrldSourceFamily::Fnv, 0x11, 0x09),
            ("0DA726", WrldSourceFamily::Fnv, 0x80, 0x00),
            ("148C05", WrldSourceFamily::Fnv, 0x11, 0x09),
            ("16D714", WrldSourceFamily::Fnv, 0x13, 0x0B),
            ("00400F", WrldSourceFamily::Fnv, 0x43, 0x03),
            ("006EDB", WrldSourceFamily::Fnv, 0x43, 0x03),
            ("004011", WrldSourceFamily::Fnv, 0x11, 0x09),
            ("18782E", WrldSourceFamily::Fnv, 0x13, 0x0B),
            ("1A1AB3", WrldSourceFamily::Fo3, 0x51, 0x09),
            ("1A1CCE", WrldSourceFamily::Fo3, 0x11, 0x09),
            ("1A1DA3", WrldSourceFamily::Fo3, 0x11, 0x09),
            ("0244A7", WrldSourceFamily::Fo3, 0x13, 0x0B),
            ("0271C0", WrldSourceFamily::Fo3, 0x11, 0x09),
            ("02F222", WrldSourceFamily::Fo3, 0x13, 0x0B),
            ("0C617E", WrldSourceFamily::Fo3, 0x11, 0x09),
            ("1A1E7A", WrldSourceFamily::Fo3, 0x21, 0x01),
            ("4EC113", WrldSourceFamily::Fo3, 0x23, 0x03),
            ("4ECFF3", WrldSourceFamily::Fo3, 0x23, 0x03),
            ("4EDF9F", WrldSourceFamily::Fo3, 0x23, 0x03),
            ("4F0D70", WrldSourceFamily::Fo3, 0x23, 0x03),
            ("4F1681", WrldSourceFamily::Fo3, 0x11, 0x09),
            ("4F168C", WrldSourceFamily::Fo3, 0x11, 0x09),
            ("4F5DF6", WrldSourceFamily::Fo3, 0x80, 0x00),
            ("502AA2", WrldSourceFamily::Fo3, 0x11, 0x09),
            ("5088A7", WrldSourceFamily::Fo3, 0x81, 0x01),
        ];

        assert_eq!(
            cases
                .iter()
                .map(|case| case.0)
                .collect::<HashSet<_>>()
                .len(),
            25
        );
        assert_eq!(
            cases
                .iter()
                .filter(|case| case.1 == WrldSourceFamily::Fnv)
                .count(),
            8
        );
        assert_eq!(
            cases
                .iter()
                .filter(|case| case.1 == WrldSourceFamily::Fo3)
                .count(),
            17
        );
        assert_eq!(cases.iter().filter(|case| case.2 & 0x10 != 0).count(), 15);
        assert_eq!(cases.iter().filter(|case| case.2 & 0x20 != 0).count(), 5);
        assert_eq!(cases.iter().filter(|case| case.2 & 0x40 != 0).count(), 3);
        assert_eq!(cases.iter().filter(|case| case.2 & 0x80 != 0).count(), 3);
        assert_eq!(
            cases
                .iter()
                .filter(|case| (case.2 & 0xF0).count_ones() > 1)
                .count(),
            1
        );
        for (id, _, source, expected) in cases {
            let actual = map_legacy_wrld_data_flags(source);
            assert_eq!(actual, expected, "merged WRLD {id}");
            assert_eq!(actual & 0xF0, 0, "unrelated FO4 flag on {id}");
        }
    }
}
