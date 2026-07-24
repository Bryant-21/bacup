use super::*;

pub(super) const EMPTY_SCOL_STAT_FIELD_SIGS: &[[u8; 4]] = &[
    *b"EDID", *b"VMAD", *b"OBND", *b"PTRN", *b"MODL", *b"MODT", *b"MODC", *b"MODS", *b"MODF",
    *b"FULL",
];

// Stripped because their values are keyed to the FO76 source-file layout.
// MHDT is carried back by the worldspace-header carry; FO4-native OFST/CLSZ are
// regenerated against the final output layout by the `rebuild_cell_offsets`
// phase; RNAM stays dropped.
pub(super) const WRLD_RUNTIME_TABLE_SIGS: &[[u8; 4]] = &[
    *b"RNAM", // large-reference table
    *b"MHDT", // max-height table
    *b"OFST", // offset table
    *b"CLSZ", // cell-size table
];

pub(super) fn fo76_map_marker_type_to_fo4(source_type: u16) -> u8 {
    match source_type {
        // Shared icon names shift at the FO4-only Diamond City, Bunker Hill,
        // Faneuil Hall, Synth Head, and Prydwen enum slots.
        0..=1 => source_type as u8,
        2..=15 => (source_type + 1) as u8,
        16..=22 => (source_type + 2) as u8,
        23..=44 => (source_type + 3) as u8,
        45..=54 => (source_type + 4) as u8,
        55..=63 => (source_type + 5) as u8,
        64 => 6,
        65 => 4,
        66 => 8,
        67..=70 => 15,
        71 => 74,
        72 => 22,
        73..=74 => 4,
        75 => 21,
        76 => 56,
        77 => 77,
        78 => 26,
        79..=80 => 18,
        81 => 37,
        82 => 8,
        83..=84 => 26,
        85 => 18,
        86 => 61,
        87 => 5,
        88 => 9,
        89 => 8,
        90 => 40,
        91 => 62,
        92 => 22,
        93 => 54,
        94 => 4,
        95 => 73,
        96 => 21,
        97 => 69,
        98 => 8,
        99 => 13,
        109 => 13,
        111 => 8,
        112 => 41,
        _ => 77,
    }
}

pub(super) fn scol_onam_is_usable(value: &FieldValue) -> bool {
    match value {
        FieldValue::FormKey(form_key) => form_key.local & 0x00FF_FFFF != 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[..4].try_into().expect("four-byte ONAM prefix")) & 0x00FF_FFFF
                != 0
        }
        FieldValue::Uint(value) => *value & 0x00FF_FFFF != 0,
        FieldValue::Int(value) => *value > 0 && (*value as u64) & 0x00FF_FFFF != 0,
        FieldValue::List(values) => values.iter().any(scol_onam_is_usable),
        FieldValue::Struct(fields) => fields.iter().any(|(_, value)| scol_onam_is_usable(value)),
        _ => false,
    }
}

pub(super) const FO76_XCRI_MESH_ROW_SIZE: usize = 8;
pub(super) const FO76_XCRI_REFERENCE_ROW_SIZE: usize = 16;
pub(super) const FO4_XCRI_MESH_ROW_SIZE: usize = 4;
pub(super) const FO4_XCRI_REFERENCE_ROW_SIZE: usize = 8;

pub(super) fn convert_cell_xcri_to_fo4(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        FieldValue::Bytes(bytes) => convert_cell_xcri_raw_to_fo4(bytes.as_slice())
            .map(|bytes| FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes))),
        FieldValue::Struct(fields) => convert_cell_xcri_struct_to_fo4(fields, interner),
        _ => None,
    }
}

// FO76's XCRI `reference_count` header field counts u32 *words* (2x the
// logical reference-row count), same as FO4's — see `esp_authoring_core::xcri`
// for the grounded byte layout and ground-truth size proofs. Delegating to
// the shared codec keeps this one parser in sync with `previs_merge`'s.
pub(super) fn convert_cell_xcri_raw_to_fo4(bytes: &[u8]) -> Option<Vec<u8>> {
    let table = decode_fo76(bytes)?;
    encode_fo4(&table)
}

pub(super) fn convert_cell_xcri_struct_to_fo4(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    if named_value(fields, "meshes_count", interner).is_none()
        && named_value(fields, "references_count", interner).is_none()
        && named_value(fields, "meshes", interner).is_none()
        && named_value(fields, "references", interner).is_none()
    {
        return None;
    }

    let meshes = match named_value(fields, "meshes", interner) {
        Some(FieldValue::List(items)) => items
            .iter()
            .map(|item| project_cell_xcri_mesh(item, interner))
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
        None => Vec::new(),
    };
    let references = match named_value(fields, "references", interner) {
        Some(FieldValue::List(items)) => items
            .iter()
            .map(|item| project_cell_xcri_reference(item, interner))
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
        None => Vec::new(),
    };

    Some(FieldValue::Struct(vec![
        (
            interner.intern("meshes_count"),
            FieldValue::Uint(meshes.len() as u64),
        ),
        (
            interner.intern("references_count"),
            // FO4's XCRI reference_count header field is 2x the logical row
            // count (u32-word count) — see `esp_authoring_core::xcri`.
            FieldValue::Uint(references.len() as u64 * 2),
        ),
        (interner.intern("meshes"), FieldValue::List(meshes)),
        (interner.intern("references"), FieldValue::List(references)),
    ]))
}

pub(super) fn project_cell_xcri_mesh(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        FieldValue::Struct(fields) => {
            project_u32_value(named_value(fields, "combined_mesh", interner)?)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= FO76_XCRI_MESH_ROW_SIZE => {
            Some(bytes_value(bytes.get(0..4)?))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= FO4_XCRI_MESH_ROW_SIZE => {
            Some(bytes_value(bytes.get(0..4)?))
        }
        FieldValue::Uint(_) | FieldValue::Int(_) => project_u32_value(value),
        _ => None,
    }
}

pub(super) fn project_cell_xcri_reference(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        FieldValue::Struct(fields) => Some(FieldValue::Struct(vec![
            (
                interner.intern("reference"),
                project_formid_value(named_value(fields, "reference", interner)?)?,
            ),
            (
                interner.intern("combined_mesh"),
                project_u32_value(named_value(fields, "combined_mesh", interner)?)?,
            ),
        ])),
        FieldValue::Bytes(bytes) if bytes.len() >= FO76_XCRI_REFERENCE_ROW_SIZE => {
            Some(FieldValue::Struct(vec![
                (interner.intern("reference"), bytes_value(bytes.get(0..4)?)),
                (
                    interner.intern("combined_mesh"),
                    bytes_value(bytes.get(8..12)?),
                ),
            ]))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= FO4_XCRI_REFERENCE_ROW_SIZE => {
            Some(FieldValue::Struct(vec![
                (interner.intern("reference"), bytes_value(bytes.get(0..4)?)),
                (
                    interner.intern("combined_mesh"),
                    bytes_value(bytes.get(4..8)?),
                ),
            ]))
        }
        _ => None,
    }
}
impl Fo76Fo4Hook {
    pub(super) fn convert_nif_backed_empty_scol_to_stat(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"SCOL"
            || record
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"ONAM" && scol_onam_is_usable(&entry.value))
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"MODL" && field_value_has_non_empty_text(&entry.value, interner)
            })
        {
            return;
        }

        record.sig = SigCode(*b"STAT");
        record.fields.retain(|entry| {
            EMPTY_SCOL_STAT_FIELD_SIGS
                .iter()
                .any(|sig| entry.sig.0 == *sig)
        });
    }

    pub(super) fn strip_wrld_runtime_tables(record: &mut Record) {
        if record.sig.0 != *b"WRLD" {
            return;
        }
        record.fields.retain(|entry| {
            !WRLD_RUNTIME_TABLE_SIGS
                .iter()
                .any(|sig| entry.sig.0 == *sig)
        });
    }

    pub(super) fn convert_or_drop_cell_combined_reference_index(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"CELL" {
            return;
        }

        let mut retained = smallvec::SmallVec::new();
        for mut entry in record.fields.drain(..) {
            if entry.sig.0 == *b"XCRI" {
                if let Some(converted) = convert_cell_xcri_to_fo4(&entry.value, interner) {
                    entry.value = converted;
                    retained.push(entry);
                }
                continue;
            }
            retained.push(entry);
        }
        record.fields = retained;
    }

    pub(super) fn convert_or_drop_region_objects(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"REGN" {
            return;
        }

        let mut retained = smallvec::SmallVec::new();
        for mut entry in record.fields.drain(..) {
            if entry.sig.0 == *b"RDOT" {
                if let Some(converted) =
                    crate::fo76_rdot::convert_fo76_regn_rdot_to_fo4(&entry.value, interner)
                {
                    entry.value = converted;
                    retained.push(entry);
                }
                continue;
            }
            retained.push(entry);
        }
        record.fields = retained;
    }

    pub(super) fn normalize_refr_map_marker_tnam(record: &mut Record) {
        if record.sig.0 != *b"REFR" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"TNAM"
                && let Some(source_type) = field_value_to_u16(&entry.value)
            {
                let target_type = fo76_map_marker_type_to_fo4(source_type);
                entry.value = bytes_value(&[target_type, 0]);
            }
        }
    }
}
