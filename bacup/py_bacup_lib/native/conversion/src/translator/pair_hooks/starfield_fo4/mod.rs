//! StarfieldFo4Hook: starfield→fo4 pair-level record hook (world records only).

pub(crate) mod book_and_art;
pub(crate) mod consumable;
pub(crate) mod explosion;
pub(crate) mod grass;
pub(crate) mod hazard;
pub(crate) mod leveled_and_faction;
pub(crate) mod light;
pub(crate) mod magic_effect;
pub(crate) mod object_properties;
pub(crate) mod placed_reference;
pub(crate) mod sound_and_image_space;

use super::fo4_layouts::{self, SourceFamily};
use super::model_paths;
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::sym::StringInterner;
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};

/// FO4 game units per Starfield NIF-space unit. Starfield's `havok_scale` is
/// 1.0 (its NIF/Havok space is real-world meters), while FO4's is 69.99125
/// (`MODERN_HAVOK_SCALE` in `game_profiles.py`) — so a raw meter value must be
/// scaled before it means anything as an FO4 int16 bound. Ground-truthed
/// against a real shipped record: `Starfield.esm` STAT `023C1D2`
/// (`AK_Ext_Bld_Door_Static_01`) decodes an `OBND` of X2=0.81884765625m,
/// Z2=2.400390625m — a ~1.6m × 2.4m door — which only becomes a
/// plausible FO4 door bound (57 × 168 units) at this factor.
const STARFIELD_METERS_TO_FO4_UNITS: f32 = 69.99125;
/// FO4's condition-function table ends here: vanilla `Fallout4.esm` carries
/// 101,882 CTDA rows across 287 function ids and none exceed 817. Starfield
/// shares the 32-byte CTDA codec but numbers its own functions past that (820,
/// 823, 825, 828, 837, 867 and 896 all ship), and FO4 indexes the table by that
/// id unchecked — an out-of-range row reads past the end, takes garbage as the
/// function's parameter types and access-violates in `TESParameters::InitItem`
/// while the owning form initialises its components.
const FO4_MAX_CONDITION_FUNCTION_ID: u16 = 817;
const CELL_STARFIELD_ONLY_HEADER_FLAGS: u32 = (1 << 2) | (1 << 22);
const FO4_CELL_XCLL_SIZE: usize = 136;
const FO4_ACTI_FLAGS: u32 = 0x6E93_BFF4;
const FO4_CELL_FLAGS: u32 = 0x000E_54A0;
const FO4_CONT_FLAGS: u32 = 0x4E01_9020;
const FO4_DOOR_FLAGS: u32 = 0x0081_9030;
const FO4_FURN_FLAGS: u32 = 0x3281_B0B4;
const FO4_GRAS_FLAGS: u32 = 0x0000_1020;
const FO4_KYWD_FLAGS: u32 = 0x0000_9020;
const FO4_LAND_FLAGS: u32 = 0x0004_1020;
const FO4_LIGH_FLAGS: u32 = 0x1203_1020;
/// FO4 leveled lists define no header flags; Starfield sets `0x8000` for Use All,
/// which FO4 reads from LVLF instead.
const FO4_LEVELED_LIST_FLAGS: u32 = 0x0000_1020;
const FO4_LTEX_FLAGS: u32 = 0x0000_1020;
const FO4_SCOL_FLAGS: u32 = 0x4E00_9E30;
const FO4_STAT_FLAGS: u32 = 0x5E8A_BEF4;
const FO4_TXST_FLAGS: u32 = 0x0000_1020;
const FO4_WRLD_FLAGS: u32 = 0x0008_5020;
const STARFIELD_FURN_MARKER_ROW_SIZE: usize = 28;
const FO4_FURN_MARKER_ROW_SIZE: usize = 24;

pub struct StarfieldFo4Hook;

pub(crate) fn is_mvp_topology_signature(signature: &str) -> bool {
    matches!(signature, "CELL" | "LAND" | "LTEX" | "REFR" | "WRLD")
}

impl PairHook for StarfieldFo4Hook {
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::sanitize_record_header_flags(record);
        Self::sanitize_scalar_flags(record, ctx.interner);
        Self::sanitize_destruction_flags(record);
        Self::normalize_refr_fixed_width_fields(record);
        Self::objectbounds_float_to_int(record);
        Self::refr_position_meters_to_units(record);
        Self::strip_components_extract_keywords(record, ctx.interner);
        Self::normalize_furniture_marker_parameters(record);
        Self::convert_embedded_navmesh(record, ctx.interner);
        Self::null_wwise_sounds(record);
        Self::strip_vmad(record);
        Self::drop_transforms_default_preview(record);
        Self::strip_incompatible_fixed_width_fields(record);
        Self::drop_out_of_range_conditions(record);
        Self::model_lightlayer_to_modt(record);
        placed_reference::normalize(record, ctx.interner);
        leveled_and_faction::normalize(record, ctx.interner);
        light::normalize(record, ctx.interner);
        grass::normalize(record, ctx.interner);
        sound_and_image_space::normalize(record, ctx.interner);
        // Shared FO4 target contracts, wired the same way as the fnv_fo4 and
        // skyrimse_fo4 hooks. REFR.XLOC must end up 16 bytes (FO4's schema
        // codec is 12 plus a separate `bytes_9` tail field), and EFSH must
        // carry the full ICON/NAM7/NAM8/DATA/DNAM contract with a 157-byte
        // DNAM. Both run last so they observe the final field set.
        match record.sig.0 {
            sig if sig == *b"REFR" => fo4_layouts::normalize_refr_xloc(record, ctx.interner),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::Starfield, ctx.interner)
            }
            _ => {}
        }
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::sanitize_cell_metadata(record);
        model_paths::normalize_model_paths(ctx.interner, record);
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

impl StarfieldFo4Hook {
    fn sanitize_record_header_flags(record: &mut Record) {
        let mask = match record.sig.as_str() {
            "ACTI" => FO4_ACTI_FLAGS,
            "CELL" => FO4_CELL_FLAGS,
            "CONT" => FO4_CONT_FLAGS,
            "DOOR" => FO4_DOOR_FLAGS,
            "FURN" => FO4_FURN_FLAGS,
            "GRAS" => FO4_GRAS_FLAGS,
            "KYWD" => FO4_KYWD_FLAGS,
            "LAND" => FO4_LAND_FLAGS,
            "LIGH" => FO4_LIGH_FLAGS,
            "LVLI" | "LVLN" => FO4_LEVELED_LIST_FLAGS,
            "LTEX" => FO4_LTEX_FLAGS,
            "SCOL" => FO4_SCOL_FLAGS,
            "STAT" => FO4_STAT_FLAGS,
            "TXST" => FO4_TXST_FLAGS,
            "WRLD" => FO4_WRLD_FLAGS,
            _ => return,
        };
        record.flags = RecordFlags::from_bits_retain(record.flags.bits() & mask);
    }

    fn sanitize_scalar_flags(record: &mut Record, interner: &StringInterner) {
        let sig = record.sig.as_str();
        for field in &mut record.fields {
            let mask = match (sig, field.sig.as_str()) {
                ("ACTI", "FNAM") => 0x1F,
                ("DOOR", "FNAM") => 0x7F,
                ("REFR", "FNAM") => 0x0F,
                _ => continue,
            };
            mask_flag_value(&mut field.value, mask);
        }

        if sig != "FURN" {
            return;
        }
        for field in &mut record.fields {
            match field.sig.as_str() {
                "FNAM" => {
                    let flags = flag_bits(&field.value, interner) & 0x03;
                    field.value = FieldValue::Bytes(smallvec::smallvec![flags as u8, 0]);
                }
                "WBDT" => clamp_furniture_bench_type(&mut field.value),
                _ => {}
            }
        }
    }

    fn normalize_refr_fixed_width_fields(record: &mut Record) {
        if record.sig.as_str() != "REFR" {
            return;
        }
        for field in &mut record.fields {
            // XLOC is deliberately absent: FO4's deployed lock-data contract is
            // 16 bytes, not the 12-byte schema codec, so it is owned by
            // fo4_layouts::normalize_refr_xloc rather than narrowed here.
            let width = match field.sig.as_str() {
                "XPLK" => 4,
                "TNAM" => 2,
                _ => continue,
            };
            if let FieldValue::Bytes(bytes) = &mut field.value {
                bytes.truncate(width);
            }
        }
    }

    // TERM is furniture too — FO4 loads both through
    // TESFurniture::LoadFurnitureData, which reads SNAM as marker rows of
    // exactly FO4_FURN_MARKER_ROW_SIZE and hard-crashes on any other payload.
    fn normalize_furniture_marker_parameters(record: &mut Record) {
        if !matches!(record.sig.as_str(), "FURN" | "TERM") {
            return;
        }

        let mut after_marker_model = false;
        record.fields.retain_mut(|entry| match entry.sig.as_str() {
            "XMRK" => {
                after_marker_model = true;
                true
            }
            "SNAM" if !after_marker_model => false,
            "SNAM" => {
                let FieldValue::Bytes(bytes) = &mut entry.value else {
                    return false;
                };
                if bytes.is_empty() || bytes.len() % STARFIELD_FURN_MARKER_ROW_SIZE != 0 {
                    return false;
                }

                let mut projected = smallvec::SmallVec::<[u8; 32]>::with_capacity(
                    bytes.len() / STARFIELD_FURN_MARKER_ROW_SIZE * FO4_FURN_MARKER_ROW_SIZE,
                );
                for row in bytes.chunks_exact(STARFIELD_FURN_MARKER_ROW_SIZE) {
                    projected.extend_from_slice(&row[..FO4_FURN_MARKER_ROW_SIZE]);
                }
                *bytes = projected;
                true
            }
            _ => true,
        });
    }

    fn sanitize_destruction_flags(record: &mut Record) {
        if !matches!(record.sig.as_str(), "ACTI" | "DOOR" | "FURN") {
            return;
        }
        for field in &mut record.fields {
            if field.sig.as_str() != "DEST" {
                continue;
            }
            if let FieldValue::Bytes(bytes) = &mut field.value {
                if let Some(flags) = bytes.get_mut(4) {
                    *flags &= 0x03;
                }
            }
        }
    }

    fn sanitize_cell_metadata(record: &mut Record) {
        if record.sig.as_str() != "CELL" {
            return;
        }

        record.flags =
            RecordFlags::from_bits_retain(record.flags.bits() & !CELL_STARFIELD_ONLY_HEADER_FLAGS);
        record.fields.retain(|entry| match entry.sig.as_str() {
            "XCLL" => match &entry.value {
                FieldValue::Bytes(bytes) => bytes.len() == FO4_CELL_XCLL_SIZE,
                FieldValue::None => false,
                _ => true,
            },
            "XCIM" | "XLCN" | "XCAS" | "XCMO" | "XCCM" => !is_null_cell_reference(&entry.value),
            _ => true,
        });
    }

    fn convert_embedded_navmesh(record: &mut Record, interner: &StringInterner) {
        record.fields.retain_mut(|entry| {
            if entry.sig.0 != *b"NVNM" {
                return true;
            }
            let FieldValue::Bytes(raw) = &entry.value else {
                return true;
            };
            match crate::starfield_navmesh::convert_nvnm_to_fo4(raw, STARFIELD_METERS_TO_FO4_UNITS)
            {
                Ok(converted) => {
                    entry.value = FieldValue::Bytes(smallvec::SmallVec::from_vec(converted));
                    true
                }
                Err(error) => {
                    record
                        .warnings
                        .push(interner.intern(&format!("starfield_nvnm_dropped:{error}")));
                    false
                }
            }
        });
    }

    /// `OBND` reaches this hook as raw `FieldValue::Bytes` (struct-codec
    /// subrecords are never decoded to `FieldValue::Struct` — see
    /// `source_read.rs`). Starfield's real layout is 6 little-endian `f32`
    /// meters (24 bytes); FO4 expects 6 little-endian `i16` units (12 bytes).
    /// Runs in `pre_translate` so the generic translate step, which copies a
    /// struct-codec field's bytes through verbatim, copies the already-FO4-
    /// shaped payload.
    fn objectbounds_float_to_int(record: &mut Record) {
        for entry in &mut record.fields {
            if entry.sig.0 != *b"OBND" {
                continue;
            }
            let FieldValue::Bytes(raw) = &entry.value else {
                continue;
            };
            if raw.len() != 24 {
                continue;
            }
            let mut out = smallvec::SmallVec::<[u8; 32]>::new();
            for chunk in raw.chunks_exact(4) {
                let meters = f32::from_le_bytes(chunk.try_into().unwrap());
                let units = (meters * STARFIELD_METERS_TO_FO4_UNITS).round();
                let clamped = units.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                out.extend_from_slice(&clamped.to_le_bytes());
            }
            entry.value = FieldValue::Bytes(out);
        }
    }

    /// `DATA` (position+rotation), `XTEL` (door teleport destination), and
    /// `XRDS` (light/sound radius override) carry Starfield metres that must
    /// become FO4 game units before the generic translate step copies their
    /// bytes/value through verbatim. Scoped to placed-child signatures only
    /// (mirrors `esp_authoring_core::plugin_runtime::is_placed_child_signature`);
    /// never runs against base records. `XSCL` is a dimensionless per-ref
    /// scale and must never be multiplied here: that would compound into a
    /// 70x giant.
    ///
    /// Not scaled here (verified against the schema): `XPRD` is `float32`
    /// "Idle Time", a patrol wait duration, not a radius. `LIGH`/`ACTI` radius
    /// fields live on base object records, never on a placed signature. `WRLD`
    /// `NAM0`/`NAM9`/`DNAM` are world distances on the worldspace record and
    /// must be derived from the synthesized FO4 cell lattice, not scaled in
    /// place from the old Starfield bounds.
    fn refr_position_meters_to_units(record: &mut Record) {
        if !matches!(
            record.sig.as_str(),
            "REFR" | "ACHR" | "PHZD" | "PGRE" | "PGRD"
        ) {
            return;
        }
        for entry in &mut record.fields {
            match entry.sig.as_str() {
                "DATA" => {
                    if let FieldValue::Bytes(raw) = &mut entry.value {
                        if raw.len() == 24 {
                            Self::scale_f32_range(raw, 0, 3);
                        }
                    }
                }
                "XTEL" => {
                    if let FieldValue::Bytes(raw) = &mut entry.value {
                        if raw.len() == 36 {
                            Self::scale_f32_range(raw, 4, 3);
                        }
                    }
                }
                "XRDS" => {
                    if let FieldValue::Float(radius) = &mut entry.value {
                        *radius *= STARFIELD_METERS_TO_FO4_UNITS;
                    }
                }
                _ => {}
            }
        }
    }

    /// Multiplies `count` little-endian f32s starting at `byte_offset` by
    /// `STARFIELD_METERS_TO_FO4_UNITS`, in place.
    fn scale_f32_range(raw: &mut smallvec::SmallVec<[u8; 32]>, byte_offset: usize, count: usize) {
        for i in 0..count {
            let start = byte_offset + i * 4;
            let meters = f32::from_le_bytes(raw[start..start + 4].try_into().unwrap());
            let units = meters * STARFIELD_METERS_TO_FO4_UNITS;
            raw[start..start + 4].copy_from_slice(&units.to_le_bytes());
        }
    }

    /// Starfield wraps object data in `BFCB`(component type)...`BFCE` runs
    /// (the "Components" system). FO4 has no such wrapper: every run is
    /// dropped. `BGSKeywordForm_Component` runs carry the same `KSIZ`/`KWDA`
    /// pair FO4 itself uses natively for top-level Keywords, so those two
    /// subrecords are hoisted out (merged into an existing top-level `KWDA`
    /// if the record already has one, e.g. DOOR) before the run is dropped.
    fn strip_components_extract_keywords(record: &mut Record, interner: &StringInterner) {
        let mut kept: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        let mut extracted_keywords: Vec<FieldValue> = Vec::new();

        let mut i = 0;
        while i < record.fields.len() {
            if record.fields[i].sig.0 == *b"BFCB" {
                let is_keyword_component = matches!(
                    &record.fields[i].value,
                    FieldValue::String(sym)
                        if interner.resolve(*sym) == Some("BGSKeywordForm_Component")
                );
                i += 1;
                while i < record.fields.len() && record.fields[i].sig.0 != *b"BFCE" {
                    if is_keyword_component && record.fields[i].sig.0 == *b"KWDA" {
                        if let FieldValue::List(items) = &record.fields[i].value {
                            extracted_keywords.extend(items.iter().cloned());
                        }
                    }
                    i += 1;
                }
                i += 1; // consume BFCE (harmless if the run was unterminated)
                continue;
            }
            kept.push(record.fields[i].clone());
            i += 1;
        }

        if !extracted_keywords.is_empty() {
            match kept.iter().position(|entry| entry.sig.0 == *b"KWDA") {
                Some(index) => {
                    if let FieldValue::List(items) = &mut kept[index].value {
                        items.extend(extracted_keywords);
                    }
                    let merged_count = match &kept[index].value {
                        FieldValue::List(items) => items.len() as u64,
                        _ => 0,
                    };
                    if let Some(ksiz) = kept.iter_mut().find(|entry| entry.sig.0 == *b"KSIZ") {
                        ksiz.value = FieldValue::Uint(merged_count);
                    }
                }
                None => {
                    let count = extracted_keywords.len() as u64;
                    kept.push(FieldEntry {
                        sig: crate::ids::SubrecordSig::from_str("KSIZ").unwrap(),
                        value: FieldValue::Uint(count),
                    });
                    kept.push(FieldEntry {
                        sig: crate::ids::SubrecordSig::from_str("KWDA").unwrap(),
                        value: FieldValue::List(extracted_keywords),
                    });
                }
            }
        }
        record.fields = kept;
    }

    /// `VWWD` ("Vehicle WWise Data") is a WWise event-GUID struct present on
    /// every world-only record type in the Starfield schema. FO4 has no
    /// equivalent subrecord anywhere, so it is dropped rather than mapped —
    /// never emit a WWise GUID blob into an FO4 FormKey slot.
    fn null_wwise_sounds(record: &mut Record) {
        record.fields.retain(|entry| entry.sig.0 != *b"VWWD");
    }

    /// FO4 cannot execute Starfield Papyrus bytecode; strip VMAD from every
    /// world-only record before translation (mirrors
    /// `skyrimse_fo4::strip_raw_skyrim_vmad`).
    fn strip_vmad(record: &mut Record) {
        record.fields.retain(|entry| entry.sig.0 != *b"VMAD");
    }

    /// `PTT2` ("Transforms") is a struct of 8 FormKey slots — inventory /
    /// workshop / ship-builder / preview icon transforms — none of which FO4
    /// has a matching field for. Drop it; FO4's (nonexistent) PreviewTransform
    /// stays unset.
    fn drop_transforms_default_preview(record: &mut Record) {
        record.fields.retain(|entry| entry.sig.0 != *b"PTT2");
    }

    /// These same-4CC source fields have different FO4 meanings and widths.
    /// The Starfield decoder has already lost bytes for the zstring-shaped
    /// variants, so padding would manufacture target data rather than convert
    /// it. They are intentionally absent from this pair's translated fields.
    fn strip_incompatible_fixed_width_fields(record: &mut Record) {
        let record_sig = record.sig.0;
        record.fields.retain(|entry| {
            !((record_sig == *b"STAT" && entry.sig.0 == *b"DNAM")
                || (record_sig == *b"DOOR" && entry.sig.0 == *b"ANAM")
                || (record_sig == *b"KYWD" && entry.sig.0 == *b"CNAM")
                || (record_sig == *b"TXST" && entry.sig.0 == *b"DNAM"))
        });
    }

    /// A dropped condition reads as unconditionally true, which is the trade the
    /// `fnv` pair already makes for its own unsupported functions. The CIS1/CIS2
    /// parameter strings a condition owns follow it out.
    fn drop_out_of_range_conditions(record: &mut Record) {
        let mut dropping_condition_strings = false;
        record.fields.retain(|entry| match &entry.sig.0 {
            b"CTDA" | b"CTDT" => {
                let drop = matches!(&entry.value, FieldValue::Bytes(bytes)
                if bytes.get(8..10).is_some_and(|raw| {
                    u16::from_le_bytes([raw[0], raw[1]]) > FO4_MAX_CONDITION_FUNCTION_ID
                }));
                dropping_condition_strings = drop;
                !drop
            }
            b"CIS1" | b"CIS2" => !dropping_condition_strings,
            _ => {
                dropping_condition_strings = false;
                true
            }
        });
        record.sync_condition_count();
    }

    /// `FLLD` ("Light Layer") has no FO4 analogue and is dropped. FO4's MODT
    /// (model texture-hash manifest) is populated later by the shared
    /// `regenerate_modt` phase from the record's Model path alone (Starfield
    /// source records never carry a MODT subrecord), so nothing is computed
    /// here — this only guards the CTD-known invariant that a surviving MODT
    /// is never empty. An empty/invalid MODT is stripped so the record is
    /// left MODT-less (i.e. flagged for the regen pass) instead of shipping a
    /// zero-length hash.
    fn model_lightlayer_to_modt(record: &mut Record) {
        record.fields.retain(|entry| entry.sig.0 != *b"FLLD");
        record.fields.retain(|entry| {
            entry.sig.0 != *b"MODT"
                || match &entry.value {
                    FieldValue::None => false,
                    FieldValue::Bytes(bytes) => !bytes.is_empty(),
                    _ => true,
                }
        });
    }
}

fn mask_flag_value(value: &mut FieldValue, mask: u64) {
    match value {
        FieldValue::Uint(raw) => *raw &= mask,
        FieldValue::Int(raw) => *raw &= mask as i64,
        FieldValue::Bytes(bytes) => {
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte &= (mask >> (index * 8)) as u8;
            }
        }
        FieldValue::List(values) => {
            values.retain_mut(|value| match value {
                FieldValue::Uint(raw) => {
                    *raw &= mask;
                    *raw != 0
                }
                FieldValue::Int(raw) => {
                    *raw &= mask as i64;
                    *raw != 0
                }
                _ => true,
            });
        }
        _ => {}
    }
}

fn flag_bits(value: &FieldValue, interner: &StringInterner) -> u64 {
    match value {
        FieldValue::Uint(raw) => *raw,
        FieldValue::Int(raw) => *raw as u64,
        FieldValue::Bytes(bytes) => bytes
            .iter()
            .take(8)
            .enumerate()
            .fold(0, |bits, (index, byte)| {
                bits | ((*byte as u64) << (index * 8))
            }),
        FieldValue::List(values) => values.iter().fold(0, |bits, value| {
            bits | match value {
                FieldValue::Uint(raw) => *raw,
                FieldValue::Int(raw) => *raw as u64,
                FieldValue::String(sym) => match interner.resolve(*sym) {
                    Some("Unknown0" | "Unknown 0") => 1,
                    Some("IgnoredBySandbox" | "Ignored By Sandbox") => 2,
                    _ => 0,
                },
                _ => 0,
            }
        }),
        _ => 0,
    }
}

fn clamp_furniture_bench_type(value: &mut FieldValue) {
    match value {
        FieldValue::Uint(raw) if *raw > 9 => *raw = 0,
        FieldValue::Int(raw) if *raw > 9 => *raw = 0,
        FieldValue::Bytes(bytes) if bytes.first().is_some_and(|raw| *raw > 9) => bytes[0] = 0,
        _ => {}
    }
}

fn is_null_cell_reference(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => true,
        FieldValue::FormKey(form_key) => form_key.local == 0,
        FieldValue::Bytes(bytes) => bytes.len() == 4 && bytes.iter().all(|byte| *byte == 0),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::sym::StringInterner;
    use crate::translator::pair_hook::PairCtx;
    use crate::translator::{Game, TranslateResult, Translator};

    fn run_pre_translate(interner: &StringInterner, record: &mut Record) {
        StarfieldFo4Hook
            .pre_translate(&mut PairCtx::new(interner), record)
            .unwrap();
    }

    fn translate_record(interner: &StringInterner, mut record: Record) -> Record {
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        translator
            .pre_translate(&mut PairCtx::new(interner), &mut record)
            .unwrap();
        match translator.translate(&record, interner) {
            TranslateResult::Translated(record) => record,
            result => panic!("record should translate, got {result:?}"),
        }
    }

    #[test]
    fn mvp_topology_signatures_cover_unmapped_world_records_only() {
        for signature in ["CELL", "LAND", "LTEX", "REFR", "WRLD"] {
            assert!(is_mvp_topology_signature(signature), "{signature}");
        }
        for signature in [
            "ACTI", "EFSH", "MSTT", "NAVM", "NAVI", "PKIN", "RFGP", "TERM",
        ] {
            assert!(!is_mvp_topology_signature(signature), "{signature}");
        }
    }

    #[test]
    fn drops_starfield_fields_that_collide_with_fo4_fixed_width_fields() {
        let interner = StringInterner::new();
        for (record_sig, incompatible_sig, compatible_sig) in [
            ("STAT", "DNAM", "MODL"),
            ("DOOR", "ANAM", "MODL"),
            ("KYWD", "CNAM", "DNAM"),
            ("TXST", "DNAM", "TX00"),
        ] {
            let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
            let mut record = Record::new(SigCode::from_str(record_sig).unwrap(), form_key);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(incompatible_sig).unwrap(),
                value: FieldValue::Bytes(smallvec::smallvec![0]),
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(compatible_sig).unwrap(),
                value: FieldValue::Bytes(smallvec::smallvec![1]),
            });

            run_pre_translate(&interner, &mut record);

            assert!(
                record
                    .fields
                    .iter()
                    .all(|entry| entry.sig.as_str() != incompatible_sig),
                "{record_sig}.{incompatible_sig} must not reach the FO4 writer"
            );
            assert!(
                record
                    .fields
                    .iter()
                    .any(|entry| entry.sig.as_str() == compatible_sig),
                "{record_sig}.{compatible_sig} must remain"
            );
        }
    }

    #[test]
    fn kywd_tnam_defaults_starfield_only_types_and_preserves_shared_types() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();

        for (source_type, expected_type) in [(52, 0), (13, 13)] {
            let mut record = Record::new(SigCode::from_str("KYWD").unwrap(), form_key);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::String(interner.intern("TestKeyword")),
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("TNAM").unwrap(),
                value: FieldValue::Uint(source_type),
            });

            let record = translate_record(&interner, record);
            let type_value = &record
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "TNAM")
                .expect("TNAM remains required by FO4 KYWD")
                .value;

            assert_eq!(type_value, &FieldValue::Int(expected_type));
            assert!(
                record
                    .fields
                    .iter()
                    .any(|entry| entry.sig.as_str() == "EDID")
            );
        }
    }

    #[test]
    fn furniture_emits_required_uint16_flags_and_preserves_marker_color() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("001E9A@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("FURN").unwrap(), form_key);
        record.flags = RecordFlags::from_bits_retain((1 << 27) | (1 << 16));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![0xCC, 0x4C, 0x33, 0]),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("FNAM").unwrap(),
            value: FieldValue::List(vec![FieldValue::String(
                interner.intern("IgnoredBySandbox"),
            )]),
        });

        let record = translate_record(&interner, record);

        assert_eq!(record.flags.bits(), 1 << 16);
        assert_eq!(find_bytes(&record, "PNAM"), &[0xCC, 0x4C, 0x33, 0]);
        let flags = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "FNAM")
            .expect("FO4 FURN requires FNAM");
        assert_eq!(flags.value, FieldValue::Bytes(smallvec::smallvec![2, 0]));
    }

    #[test]
    fn lights_drop_starfield_only_header_flags() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("LIGH").unwrap(), form_key);
        // Adaptive Lighting 0x8000 and bit 7 alongside FO4's Unknown 17 and Obstacle.
        record.flags = RecordFlags::from_bits_retain(0x8000 | 0x80 | 0x0002_0000 | 0x0200_0000);

        run_pre_translate(&interner, &mut record);

        assert_eq!(record.flags.bits(), 0x0202_0000);
    }

    #[test]
    fn leveled_lists_drop_the_starfield_use_all_header_flag() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
        for signature in ["LVLI", "LVLN"] {
            let mut record = Record::new(SigCode::from_str(signature).unwrap(), form_key);
            record.flags = RecordFlags::from_bits_retain(0x8000 | 0x20);

            run_pre_translate(&interner, &mut record);

            assert_eq!(record.flags.bits(), 0x20, "{signature}");
        }
    }

    #[test]
    fn starfield_furn_drops_early_snam_and_projects_each_marker_row() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("001E9A@Starfield.esm", &interner).unwrap();
        let marker_parameters = (0_u8..56).collect::<Vec<_>>();
        let mut expected = marker_parameters[0..24].to_vec();
        expected.extend_from_slice(&marker_parameters[28..52]);
        let mut record = Record::new(SigCode::from_str("FURN").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("FNAM").unwrap(),
            value: FieldValue::Uint(0),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Int(7),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XMRK").unwrap(),
            value: FieldValue::String(
                interner.intern(
                    "Markers\\FurnitureMarkers\\SitTable_ExecutiveHandClasp_Idle_Marker.nif",
                ),
            ),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(marker_parameters)),
        });

        let record = translate_record(&interner, record);
        let snam = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "SNAM")
            .collect::<Vec<_>>();

        assert_eq!(snam.len(), 1);
        let FieldValue::Bytes(bytes) = &snam[0].value else {
            panic!("FO4 FURN SNAM must remain raw marker bytes");
        };
        assert_eq!(bytes.as_slice(), expected.as_slice());
    }

    // A 28-byte Starfield row left unprojected reaches FO4 as a 4-byte SNAM,
    // which hard-crashes the loader in TESFurniture::LoadFurnitureData.
    #[test]
    fn starfield_term_drops_early_snam_and_projects_each_marker_row() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("002974@Starfield.esm", &interner).unwrap();
        let marker_parameters = (0_u8..56).collect::<Vec<_>>();
        let mut expected = marker_parameters[0..24].to_vec();
        expected.extend_from_slice(&marker_parameters[28..52]);
        let mut record = Record::new(SigCode::from_str("TERM").unwrap(), form_key);
        // Pre-XMRK SNAM is Starfield's sound slot; FO4 has no such field.
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Int(7),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XMRK").unwrap(),
            value: FieldValue::String(
                interner.intern("markers\\furnituremarkers\\furn_human_standingterminal.nif"),
            ),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(marker_parameters)),
        });

        let record = translate_record(&interner, record);
        let snam = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "SNAM")
            .collect::<Vec<_>>();

        assert_eq!(snam.len(), 1);
        let FieldValue::Bytes(bytes) = &snam[0].value else {
            panic!("FO4 TERM SNAM must remain raw marker bytes");
        };
        assert_eq!(bytes.len() % FO4_FURN_MARKER_ROW_SIZE, 0);
        assert_eq!(bytes.as_slice(), expected.as_slice());
    }

    // FO4's REFR.XLOC schema codec is twelve bytes, but the deployed loader
    // reads sixteen; re-encoding through the short codec drops the tail.
    #[test]
    fn starfield_refr_xloc_is_padded_to_the_fo4_sixteen_byte_contract() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("1F31EC@Starfield.esm", &interner).unwrap();
        let source = (0_u8..16).collect::<Vec<_>>();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XLOC").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(source.clone())),
        });

        let record = translate_record(&interner, record);
        let xloc = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "XLOC")
            .expect("XLOC survives translation");
        let FieldValue::Bytes(bytes) = &xloc.value else {
            panic!("FO4 XLOC must be raw bytes: {:?}", xloc.value);
        };
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[..12], &source[..12]);
        assert_eq!(&bytes[12..], &[0_u8; 4]);
    }

    #[test]
    fn starfield_efsh_gets_the_full_fo4_contract_with_safe_default_dnam() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("01914F@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("EFSH").unwrap(), form_key);
        // Starfield's EFSH DATA is not FO4-shaped and its layout is unmapped,
        // so the contract must be completed from safe defaults, not carried.
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(vec![0_u8; 7])),
        });

        let record = translate_record(&interner, record);
        for sig in ["ICON", "NAM7", "NAM8", "DATA", "DNAM"] {
            assert!(
                record.fields.iter().any(|field| field.sig.as_str() == sig),
                "FO4 EFSH contract is missing {sig}"
            );
        }
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .expect("DNAM");
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("FO4 EFSH DNAM must be raw bytes: {:?}", dnam.value);
        };
        assert_eq!(bytes.len(), 157);
    }

    #[test]
    fn scalar_flag_domains_drop_starfield_only_bits() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
        for (signature, raw, expected) in [("ACTI", 0x64, 0x04), ("DOOR", 0x90, 0x10)] {
            let mut record = Record::new(SigCode::from_str(signature).unwrap(), form_key);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("FNAM").unwrap(),
                value: FieldValue::Uint(raw),
            });

            run_pre_translate(&interner, &mut record);

            assert_eq!(record.fields[0].value, FieldValue::Uint(expected));
        }
    }

    #[test]
    fn destruction_header_drops_starfield_only_flag_bits() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("04A6C8@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ACTI").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DEST").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![0, 0, 0, 0, 0x07, 0, 0, 0]),
        });

        run_pre_translate(&interner, &mut record);

        assert_eq!(find_bytes(&record, "DEST")[4], 0x03);
    }

    #[test]
    fn refr_structs_are_projected_to_fo4_widths() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("22920F@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        // XLOC is covered by starfield_refr_xloc_is_padded_to_the_fo4_sixteen_
        // byte_contract instead — it widens rather than narrows.
        for (signature, source_width) in [("XPLK", 8), ("TNAM", 4)] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(signature).unwrap(),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(
                    (0..source_width).map(|byte| byte as u8).collect(),
                )),
            });
        }

        run_pre_translate(&interner, &mut record);

        for (signature, target_width) in [("XPLK", 4), ("TNAM", 2)] {
            let bytes = record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == signature)
                .and_then(|field| match &field.value {
                    FieldValue::Bytes(bytes) => Some(bytes),
                    _ => None,
                })
                .unwrap();
            assert_eq!(bytes.len(), target_width, "{signature}");
        }
    }

    #[test]
    fn furniture_bench_type_defaults_starfield_only_values() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("01A21B@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("FURN").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("WBDT").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![11]),
        });

        run_pre_translate(&interner, &mut record);

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(smallvec::smallvec![0])
        );
    }

    #[test]
    fn drops_raw_prps_from_acti_and_cont_while_preserving_models() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();

        for (record_sig, raw_len) in [("ACTI", 59), ("CONT", 36)] {
            let mut record = Record::new(SigCode::from_str(record_sig).unwrap(), form_key);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("PRPS").unwrap(),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(vec![0xA5; raw_len])),
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("MODL").unwrap(),
                value: FieldValue::String(interner.intern("SetDressing\\Compatible.nif")),
            });

            let record = translate_record(&interner, record);

            assert!(
                record
                    .fields
                    .iter()
                    .all(|entry| entry.sig.as_str() != "PRPS")
            );
            assert!(
                record
                    .fields
                    .iter()
                    .any(|entry| entry.sig.as_str() == "MODL")
            );
        }
    }

    #[test]
    fn sanitizes_starfield_cell_metadata_without_dropping_valid_refs() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
        let valid_ref = FormKey::parse("000123@Fallout4.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("CELL").unwrap(), form_key);
        record.flags = RecordFlags::from_bits_retain(CELL_STARFIELD_ONLY_HEADER_FLAGS | 0x400);
        for (sig, value) in [
            (
                "XCLL",
                FieldValue::Bytes(smallvec::SmallVec::from_vec(vec![0xA5; 108])),
            ),
            ("XCIM", FieldValue::None),
            ("XLCN", FieldValue::Bytes(smallvec::smallvec![0, 0, 0, 0])),
            ("XCAS", FieldValue::FormKey(valid_ref)),
            ("XCMO", FieldValue::Bytes(smallvec::smallvec![0, 0, 0, 0])),
            ("XCCM", FieldValue::None),
            (
                "XCLL",
                FieldValue::Bytes(smallvec::SmallVec::from_vec(vec![0x5A; FO4_CELL_XCLL_SIZE])),
            ),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value,
            });
        }

        StarfieldFo4Hook
            .post_translate(&mut PairCtx::new(&interner), &mut record)
            .unwrap();

        assert_eq!(record.flags.bits(), 0x400);
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.as_str() == "XCLL")
                .count(),
            1
        );
        assert!(
            record
                .fields
                .iter()
                .all(|entry| !matches!(entry.sig.as_str(), "XCIM" | "XLCN" | "XCCM"))
        );
        assert!(record.fields.iter().any(|entry| {
            entry.sig.as_str() == "XCAS" && entry.value == FieldValue::FormKey(valid_ref)
        }));
    }

    #[test]
    fn drops_starfield_native_terminals_from_mvp_base_objects() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
        let terminal_menu = FormKey::parse("123456@Starfield.esm", &interner).unwrap();

        for record_sig in ["ACTI", "CONT", "DOOR"] {
            let mut record = Record::new(SigCode::from_str(record_sig).unwrap(), form_key);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("NTRM").unwrap(),
                value: FieldValue::FormKey(terminal_menu),
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("MODL").unwrap(),
                value: FieldValue::String(interner.intern("SetDressing\\Compatible.nif")),
            });

            let record = translate_record(&interner, record);

            assert!(
                record
                    .fields
                    .iter()
                    .all(|entry| entry.sig.as_str() != "NTRM")
            );
            assert!(
                record
                    .fields
                    .iter()
                    .any(|entry| entry.sig.as_str() == "MODL")
            );
        }
    }

    #[test]
    fn drops_unemitted_starfield_container_inventory_while_preserving_model() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("001559@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("CONT").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("COCT").unwrap(),
            value: FieldValue::Uint(1),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CNTO").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![
                0xBE, 0xFC, 0x1F, 0x00, 0x01, 0x00, 0x00, 0x00
            ]),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("COED").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![0; 12]),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern("SetDressing\\Compatible.nif")),
        });

        let record = translate_record(&interner, record);

        assert!(
            record
                .fields
                .iter()
                .all(|entry| !matches!(&entry.sig.0, b"COCT" | b"CNTO" | b"COED"))
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn drops_starfield_comment_class_condition_and_reconciles_citc() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("29A5F9@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ACTI").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CITC").unwrap(),
            value: FieldValue::Uint(1),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(
                hex::decode("00C474690000803F4503F9448A4D2100000000000000000000000000FFFFFFFF")
                    .unwrap(),
            )),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern("SetDressing\\Compatible.nif")),
        });

        let record = translate_record(&interner, record);

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "CTDA")
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "CITC")
                .expect("condition count remains")
                .value,
            FieldValue::Uint(0)
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn drops_out_of_range_conditions_on_non_acti_records() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("016A7C@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("MGEF").unwrap(), form_key);
        // abVFX_SpaceShipNPCGravJump exactly as it shipped: functions 820 and
        // 825. FO4 access-violated initialising this record's conditions.
        for row in [
            "000000000000803F34033B4100000000000000000000000000000000FFFFFFFF",
            "200000000000803F39033B4100000000000000000000000000000000FFFFFFFF",
            // Function 74, FO4's most common, must survive.
            "000000000000803F4A003B4100000000000000000000000000000000FFFFFFFF",
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("CTDA").unwrap(),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(hex::decode(row).unwrap())),
            });
        }

        run_pre_translate(&interner, &mut record);

        let surviving: Vec<_> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "CTDA")
            .collect();
        assert_eq!(surviving.len(), 1, "only the in-range condition remains");
        let FieldValue::Bytes(bytes) = &surviving[0].value else {
            panic!("condition should stay raw bytes");
        };
        assert_eq!(u16::from_le_bytes([bytes[8], bytes[9]]), 74);
    }

    #[test]
    fn objectbounds_float_to_int_scales_starfield_meters_to_fo4_units() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);

        // Real shipped OBND from Starfield.esm STAT 023C1D2 (AK_Ext_Bld_Door_Static_01):
        // X1,Y1,Z1,X2,Y2,Z2 in meters, a ~1.6m x 0.18m x 2.4m building door.
        let floats: [f32; 6] = [
            -0.818_847_66,
            -0.393_189_28,
            -0.000_560_298_56,
            0.818_847_66,
            -0.210_083_01,
            2.400_390_6,
        ];
        let mut obnd_bytes = smallvec::SmallVec::<[u8; 32]>::new();
        for value in floats {
            obnd_bytes.extend_from_slice(&value.to_le_bytes());
        }
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("OBND").unwrap(),
            value: FieldValue::Bytes(obnd_bytes),
        });

        run_pre_translate(&interner, &mut record);

        let obnd = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "OBND")
            .unwrap();
        let FieldValue::Bytes(bytes) = &obnd.value else {
            panic!("OBND should stay a raw Bytes struct-codec field");
        };
        assert_eq!(bytes.len(), 12, "FO4 OBND is 6 x int16 = 12 bytes");
        let values: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        assert_eq!(values, vec![-57, -28, 0, 57, -15, 168]);
    }

    fn push_bytes_field(record: &mut Record, sig: &str, floats: &[f32]) {
        let mut raw = smallvec::SmallVec::<[u8; 32]>::new();
        for value in floats {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(raw),
        });
    }

    fn find_bytes<'a>(record: &'a Record, sig: &str) -> &'a [u8] {
        let FieldValue::Bytes(raw) = &record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == sig)
            .unwrap()
            .value
        else {
            panic!("{sig} should stay a raw Bytes struct-codec field");
        };
        raw
    }

    fn floats_of(raw: &[u8]) -> Vec<f32> {
        raw.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    #[test]
    fn refr_position_scales_meters_to_fo4_units() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        push_bytes_field(&mut record, "DATA", &[100.0, 200.0, 50.0, 0.1, 0.2, 0.3]);

        run_pre_translate(&interner, &mut record);

        let values = floats_of(find_bytes(&record, "DATA"));
        assert_eq!(values, vec![6999.125, 13998.25, 3499.5625, 0.1, 0.2, 0.3]);
    }

    #[test]
    fn refr_rotation_floats_are_untouched() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ACHR").unwrap(), form_key);
        push_bytes_field(&mut record, "DATA", &[1.0, 2.0, 3.0, 0.5, -0.5, 1.5]);

        run_pre_translate(&interner, &mut record);

        let values = floats_of(find_bytes(&record, "DATA"));
        assert_eq!(&values[3..], &[0.5, -0.5, 1.5]);
    }

    #[test]
    fn xscl_is_never_scaled() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XSCL").unwrap(),
            value: FieldValue::Float(2.0),
        });

        run_pre_translate(&interner, &mut record);

        let xscl = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "XSCL")
            .unwrap();
        assert_eq!(xscl.value, FieldValue::Float(2.0));
    }

    #[test]
    fn non_placed_records_keep_their_data() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);
        push_bytes_field(&mut record, "DATA", &[100.0, 200.0, 50.0, 0.1, 0.2, 0.3]);

        run_pre_translate(&interner, &mut record);

        let values = floats_of(find_bytes(&record, "DATA"));
        assert_eq!(values, vec![100.0, 200.0, 50.0, 0.1, 0.2, 0.3]);
    }

    #[test]
    fn obnd_is_not_double_scaled() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);
        let obnd_floats: [f32; 6] = [
            -0.818_847_66,
            -0.393_189_28,
            -0.000_560_298_56,
            0.818_847_66,
            -0.210_083_01,
            2.400_390_6,
        ];
        push_bytes_field(&mut record, "OBND", &obnd_floats);
        push_bytes_field(&mut record, "DATA", &[100.0, 200.0, 50.0, 0.1, 0.2, 0.3]);

        run_pre_translate(&interner, &mut record);

        let obnd_values: Vec<i16> = find_bytes(&record, "OBND")
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        assert_eq!(obnd_values, vec![-57, -28, 0, 57, -15, 168]);
        let data_values = floats_of(find_bytes(&record, "DATA"));
        assert_eq!(data_values, vec![100.0, 200.0, 50.0, 0.1, 0.2, 0.3]);
    }

    #[test]
    fn xrds_radius_scales_meters_to_fo4_units() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XRDS").unwrap(),
            value: FieldValue::Float(1.5),
        });

        run_pre_translate(&interner, &mut record);

        let xrds = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "XRDS")
            .unwrap();
        assert_eq!(
            xrds.value,
            FieldValue::Float(1.5 * STARFIELD_METERS_TO_FO4_UNITS)
        );
    }

    #[test]
    fn xtel_destination_position_scales_but_rotation_and_formids_do_not() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        let mut raw = smallvec::SmallVec::<[u8; 32]>::new();
        raw.extend_from_slice(&0x11223344u32.to_le_bytes()); // door FormID
        for value in [10.0f32, 20.0, 30.0, 0.1, 0.2, 0.3] {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        raw.extend_from_slice(&7u32.to_le_bytes()); // flags
        raw.extend_from_slice(&0x55667788u32.to_le_bytes()); // transition_interior
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XTEL").unwrap(),
            value: FieldValue::Bytes(raw),
        });

        run_pre_translate(&interner, &mut record);

        let raw = find_bytes(&record, "XTEL");
        assert_eq!(raw.len(), 36);
        assert_eq!(
            u32::from_le_bytes(raw[0..4].try_into().unwrap()),
            0x11223344
        );
        assert_eq!(
            floats_of(&raw[4..16]),
            vec![
                10.0 * STARFIELD_METERS_TO_FO4_UNITS,
                20.0 * STARFIELD_METERS_TO_FO4_UNITS,
                30.0 * STARFIELD_METERS_TO_FO4_UNITS
            ]
        );
        assert_eq!(floats_of(&raw[16..28]), vec![0.1, 0.2, 0.3]);
        assert_eq!(u32::from_le_bytes(raw[28..32].try_into().unwrap()), 7);
        assert_eq!(
            u32::from_le_bytes(raw[32..36].try_into().unwrap()),
            0x55667788
        );
    }

    #[test]
    fn hoists_keywordformcomponent_keywords_and_drops_component_wrapper() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("161610@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);
        let keyword = FormKey::parse("081884@Starfield.esm", &interner).unwrap();

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("Ak_Wall_Light_Desktop")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("BFCB").unwrap(),
            value: FieldValue::String(interner.intern("BGSKeywordForm_Component")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KSIZ").unwrap(),
            value: FieldValue::Uint(1),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KWDA").unwrap(),
            value: FieldValue::List(vec![FieldValue::FormKey(keyword)]),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("BFCE").unwrap(),
            value: FieldValue::None,
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern("SetDressing\\Office\\light.nif")),
        });

        run_pre_translate(&interner, &mut record);

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "BFCB" && entry.sig.as_str() != "BFCE")
        );
        let ksiz = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "KSIZ")
            .expect("KSIZ hoisted to top level");
        assert_eq!(ksiz.value, FieldValue::Uint(1));
        let kwda = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "KWDA")
            .expect("KWDA hoisted to top level");
        assert_eq!(
            kwda.value,
            FieldValue::List(vec![FieldValue::FormKey(keyword)])
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn drops_non_keyword_components_without_hoisting_anything() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("0115EA@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ACTI").unwrap(), form_key);

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("BFCB").unwrap(),
            value: FieldValue::String(interner.intern("BGSAnimationGraph_Component")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ANAM").unwrap(),
            value: FieldValue::String(interner.intern("graph")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("BFCE").unwrap(),
            value: FieldValue::None,
        });

        run_pre_translate(&interner, &mut record);

        assert!(record.fields.is_empty());
    }

    #[test]
    fn drops_vehicle_wwise_data_field() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("161610@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("Test")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("VWWD").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![0u8; 8]),
        });

        run_pre_translate(&interner, &mut record);

        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "EDID");
    }

    #[test]
    fn converts_starfield_embedded_navmesh_on_ship_ramp_activator() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("064DFF@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ACTI").unwrap(), form_key);

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("SMOD_Bay_Nova_NG-6_Ramp")),
        });
        let mut nvnm = Vec::new();
        nvnm.extend_from_slice(&17_u32.to_le_bytes());
        nvnm.extend_from_slice(&0xA5E9_A03C_u32.to_le_bytes());
        nvnm.extend_from_slice(&0_u32.to_le_bytes());
        nvnm.extend_from_slice(&0x25_u32.to_le_bytes());
        nvnm.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..7 {
            nvnm.extend_from_slice(&0_u32.to_le_bytes());
        }
        nvnm.extend_from_slice(&[0, 0, 0, 0]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(nvnm)),
        });

        run_pre_translate(&interner, &mut record);

        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let TranslateResult::Translated(record) = translator.translate(&record, &interner) else {
            panic!("ACTI should translate");
        };

        let navmesh = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "NVNM")
            .expect("converted NVNM retained");
        let FieldValue::Bytes(bytes) = &navmesh.value else {
            panic!("NVNM must remain bytes");
        };
        assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 15);
        esp_authoring_core::nvnm::parse_nvnm(bytes).expect("FO4 NVNM parses");
    }

    #[test]
    fn strips_vmad_from_world_only_records() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ACTI").unwrap(), form_key);

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("Test")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("VMAD").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![5, 0, 2, 0]),
        });

        run_pre_translate(&interner, &mut record);

        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "EDID");
    }

    #[test]
    fn drops_transforms_leaving_no_preview_transform() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("161610@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("Test")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PTT2").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![0u8; 32]),
        });

        run_pre_translate(&interner, &mut record);

        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "EDID");
    }

    #[test]
    fn drops_light_layer_and_empty_modt_leaving_model_flagged_for_regen() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern("SetDressing\\Office\\light.nif")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODT").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::new()),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("FLLD").unwrap(),
            value: FieldValue::Uint(12345),
        });

        run_pre_translate(&interner, &mut record);

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "FLLD")
        );
        let modt = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MODT");
        assert!(
            modt.is_none()
                || matches!(&modt.unwrap().value, FieldValue::Bytes(bytes) if !bytes.is_empty()),
            "converted STAT with a Model must have a non-empty MODT or be MODT-less (flagged for the regen pass)"
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn keeps_valid_modt_alongside_dropped_light_layer() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("023C1D2@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern("SetDressing\\Office\\light.nif")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODT").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("FLLD").unwrap(),
            value: FieldValue::Uint(1),
        });

        run_pre_translate(&interner, &mut record);

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "FLLD")
        );
        let modt = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MODT")
            .expect("structurally valid MODT is kept");
        assert_eq!(
            modt.value,
            FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4])
        );
    }
}
