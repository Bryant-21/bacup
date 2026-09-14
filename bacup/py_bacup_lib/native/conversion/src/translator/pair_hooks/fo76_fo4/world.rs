use super::*;

pub(super) const EMPTY_SCOL_STAT_FIELD_SIGS: &[[u8; 4]] = &[
    *b"EDID", *b"VMAD", *b"OBND", *b"PTRN", *b"MODL", *b"MODT", *b"MODC", *b"MODS", *b"MODF",
    *b"FULL",
];
pub(super) const FO4_DEFAULT_FLORA_INGREDIENT_PRODUCTION: [u8; 4] = [100; 4];
pub(super) const CELL_INTERIOR_FLAG: u32 = 0x0001;
pub(super) const FO4_CELL_FOG_INHERIT_MASK: u32 = 0x031C;
/// `CELL.XCLL.inherits` bit 0, Ambient Color — the bit that also pulls the
/// template's `DALC` directional-ambient colours.
pub(super) const FO4_CELL_AMBIENT_COLOR_INHERIT_BIT: u32 = 0x0001;
/// Bits 0-1: Ambient Color | Directional Color.
pub(super) const FO4_CELL_AMBIENT_INHERIT_MASK: u32 = 0x0003;

// `CELL.XCLL` byte offsets. FO76 and FO4 share this whole prefix — FO76 only
// appends 24 bytes of skybox bounds — so these hold both before and after the
// source→FO4 struct relayout.
pub(super) const XCLL_INHERITS_OFFSET: usize = 88;
pub(super) const XCLL_MIN_LEN: usize = XCLL_INHERITS_OFFSET + 4;
/// Ambient RGB (+0) and Directional RGB (+4); byte 3 of each quad is padding.
pub(super) const XCLL_LIGHT_COLOR_OFFSETS: [usize; 6] = [0, 1, 2, 4, 5, 6];
/// The six directional-ambient colours (X±/Y±/Z±), one padded RGB quad each.
pub(super) const XCLL_DIRECTIONAL_AMBIENT_RANGE: std::ops::Range<usize> = 40..64;
/// A cell whose brightest own light byte is below this is unlit for FO4's
/// purposes. Chosen to cover the FO76 cells that render only via Enlighten GI
/// (brightest byte 0-47) while leaving normally-authored dim interiors alone.
pub(super) const FO76_UNLIT_CELL_MAX_LIGHT_BYTE: u8 = 48;

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

pub(super) fn form_id_field_value_is_usable(value: &FieldValue) -> bool {
    match value {
        FieldValue::FormKey(form_key) => form_key.local & 0x00FF_FFFF != 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(
                bytes[..4]
                    .try_into()
                    .expect("four-byte FormID field prefix"),
            ) & 0x00FF_FFFF
                != 0
        }
        FieldValue::Uint(value) => *value & 0x00FF_FFFF != 0,
        FieldValue::Int(value) => *value > 0 && (*value as u64) & 0x00FF_FFFF != 0,
        FieldValue::List(values) => values.iter().any(form_id_field_value_is_usable),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| form_id_field_value_is_usable(value)),
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
    /// `CELL.XCLL` reaches a pair hook as raw `FieldValue::Bytes`: the source
    /// decoder (`source_read::decode_subrecord`) emits bytes for every `struct:`
    /// codec, never a typed `FieldValue::Struct`. `record::write_u32_field` is
    /// unusable on these bytes because it writes offset 0, which is the ambient
    /// colour, not `inherits`.
    fn cell_xcll_bytes_mut(record: &mut Record) -> Option<&mut [u8]> {
        record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"XCLL")
            .and_then(|entry| match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() >= XCLL_MIN_LEN => Some(&mut bytes[..]),
                _ => None,
            })
    }

    fn is_interior_cell_with_lighting_template(record: &Record) -> bool {
        record.sig.0 == *b"CELL"
            && record.fields.iter().any(|entry| {
                entry.sig.0 == *b"DATA"
                    && field_value_to_u32(&entry.value)
                        .is_some_and(|flags| flags & CELL_INTERIOR_FLAG != 0)
            })
            && record
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"LTMP" && form_id_field_value_is_usable(&entry.value))
    }

    fn xcll_inherits(xcll: &[u8]) -> u32 {
        u32::from_le_bytes(
            xcll[XCLL_INHERITS_OFFSET..XCLL_INHERITS_OFFSET + 4]
                .try_into()
                .expect("XCLL_MIN_LEN guarantees four bytes"),
        )
    }

    fn or_xcll_inherits(xcll: &mut [u8], mask: u32) {
        let updated = (Self::xcll_inherits(xcll) | mask).to_le_bytes();
        xcll[XCLL_INHERITS_OFFSET..XCLL_INHERITS_OFFSET + 4].copy_from_slice(&updated);
    }

    /// Brightest byte across the cell's own ambient, directional, and
    /// directional-ambient colours — how lit the cell is without its template.
    fn xcll_max_light_byte(xcll: &[u8]) -> u8 {
        XCLL_LIGHT_COLOR_OFFSETS
            .iter()
            .copied()
            .chain(XCLL_DIRECTIONAL_AMBIENT_RANGE.filter(|offset| offset % 4 != 3))
            .filter_map(|offset| xcll.get(offset).copied())
            .max()
            .unwrap_or(0)
    }

    pub(super) fn inherit_storm_cell_fog_from_fo4_template(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        let is_storm_cell = record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .is_some_and(|editor_id| {
                editor_id
                    .get(..5)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("storm"))
            });
        if !is_storm_cell || !Self::is_interior_cell_with_lighting_template(record) {
            return;
        }

        if let Some(xcll) = Self::cell_xcll_bytes_mut(record) {
            Self::or_xcll_inherits(xcll, FO4_CELL_FOG_INHERIT_MASK);
        }
    }

    /// FO76 lights many interiors with Enlighten global illumination and leaves
    /// the cell's own ambient/directional colours at or near black — Fort Atlas
    /// is fully black and lit by 350 placed lights alone. FO4 has no GI, so
    /// carrying those bytes faithfully yields an unlit cell; vanilla
    /// `Fallout4.esm` never ships one (0 of its 1,195 lit cells). Where a
    /// lighting template is attached, inherit its ambient — which also supplies
    /// the template's `DALC` directional-ambient colours — exactly as vanilla's
    /// one such cell, `SlocumsJoeHQOffice`, does.
    ///
    /// Only the two colour bits are set: fog and light-fade stay on the cell's
    /// own authored values.
    pub(super) fn inherit_ambient_light_for_unlit_fo76_cells(record: &mut Record) {
        if !Self::is_interior_cell_with_lighting_template(record) {
            return;
        }
        let Some(xcll) = Self::cell_xcll_bytes_mut(record) else {
            return;
        };
        if Self::xcll_inherits(xcll) & FO4_CELL_AMBIENT_COLOR_INHERIT_BIT != 0
            || Self::xcll_max_light_byte(xcll) >= FO76_UNLIT_CELL_MAX_LIGHT_BYTE
        {
            return;
        }

        Self::or_xcll_inherits(xcll, FO4_CELL_AMBIENT_INHERIT_MASK);
    }

    pub(super) fn convert_nif_backed_empty_scol_to_stat(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"SCOL"
            || record
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"ONAM" && form_id_field_value_is_usable(&entry.value))
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

    pub(super) fn ensure_flora_ingredient_production(record: &mut Record) {
        if record.sig.0 != *b"FLOR"
            || record.fields.iter().any(|entry| entry.sig.0 == *b"PFPC")
            || !record
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"PFIG" && form_id_field_value_is_usable(&entry.value))
        {
            return;
        }

        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"PFPC"),
            value: bytes_value(&FO4_DEFAULT_FLORA_INGREDIENT_PRODUCTION),
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
