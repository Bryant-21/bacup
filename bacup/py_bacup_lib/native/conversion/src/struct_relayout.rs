//! Source→FO4 `struct:` codec subrecord byte-relayout.
//!
//! # Root cause
//! `source_read::decode_subrecord` emits every `struct:`/`array_struct:` codec
//! subrecord as a raw `FieldValue::Bytes` blob (see that fn's "emit raw bytes"
//! branch). The FO76 source bytes are then written verbatim into the FO4 output
//! (only the version-gated TAIL is trimmed by the target normalizer). But the
//! FO76 and FO4 struct layouts for the SAME subrecord diverge MID-struct — FO76
//! carries extra/reordered fields before the shared tail — so FO4/CK reads every
//! field after the first divergence from the wrong byte offset.
//!
//! Ground-truthed on RACE.DATA: FO4 places `severable_explosion` at offset 146,
//! FO76 at 158 (a 12-byte shift from 4 extra FO76 mid-struct fields). The gore
//! FormIDs land on FO76 float bytes → CK "could not find dismember explosion /
//! impact data set / OnCripple ...". 33 struct subrecords diverge this way
//! (RACE.DATA, MGEF.DATA, `*.DSTD`, `*.COED`, WEAP.DNAM, BPTD.BPND, ...).
//!
//! # The fix (surgical BYTE relayout — keeps the `Bytes` data model)
//! For a divergent `struct:` subrecord, build a `field_id → (offset, width)` map
//! for the SOURCE layout (computed at the record's REAL source form_version) and
//! the TARGET layout (FO4 form_version). Copy each field that exists in BOTH
//! layouts (matched by `field_id` or a known cross-schema alias) from its source
//! offset to its target offset into a fresh, target-sized buffer. Target-only
//! fields stay zero; source-only fields are dropped. The result is FO4-laid-out
//! bytes that the FO4 game/CK and the downstream FK remap
//! (`remap_struct_fk_fields`) read at the correct offset.
//!
//! Runs at decode time (we have the raw source form_version + source schema in
//! hand) BEFORE the FK remap fixup. Once the bytes are FO4-laid-out, the #31
//! divergence guard in `ref_index::source_struct_layout_diverges` sees
//! source==target layout and stops skipping, so the gore FKs get their master
//! byte remapped (00→07) via their existing `formlink_targets`.

use esp_authoring_core::plugin_runtime::StructFieldInfo;

use crate::schema::AuthoringSchema;

/// Context carried through the decode path enabling source→FO4 struct relayout.
pub struct StructRelayoutCtx<'a> {
    /// FO4 target schema — supplies the destination field layout.
    pub target_schema: &'a AuthoringSchema,
    /// FO4 target record form_version (131) — selects the target layout's
    /// version-gated fields, matching what the FO4 game/CK will parse.
    pub target_form_version: u16,
    /// Legacy Fallout layouts are enabled only for BPTD.BPND. Other legacy
    /// structs can have semantic type changes that byte relayout cannot infer.
    pub legacy_bptd_only: bool,
}

/// FK-offset signature of a layout: the ordered (offset, width) of width-4
/// formlink fields. Used to decide divergence cheaply and identically to the
/// #31 guard (`ref_index::source_struct_layout_diverges`).
fn fk_offset_signature(layout: &[StructFieldInfo<'_>]) -> Vec<(usize, usize)> {
    layout
        .iter()
        .filter(|f| f.width == 4 && !f.formlink_targets.is_empty())
        .map(|f| (f.offset, f.width))
        .collect()
}

/// Total byte span of a layout (max offset+width across fields).
fn layout_span(layout: &[StructFieldInfo<'_>]) -> usize {
    layout.iter().map(|f| f.offset + f.width).max().unwrap_or(0)
}

fn source_field_id_for_target<'a>(
    record_sig: &str,
    sub_sig: &str,
    target_field_id: &'a str,
) -> &'a str {
    match (record_sig, sub_sig, target_field_id) {
        ("EXPL", "DATA", "flags") => "flags1",
        _ => target_field_id,
    }
}

fn legacy_bptd_actor_value_form_id(value: u8) -> u32 {
    // Legacy condition values 25..31 are the same contiguous AVIF run at
    // Fallout4.esm:00036C..000372; -1/255 means no actor value.
    match value {
        25..=31 => 0x0000_036C + u32::from(value - 25),
        _ => 0,
    }
}

fn legacy_bptd_part_type(value: u8) -> u8 {
    // FO4 inserted Eye, LookAt, and Fly Grab before legacy Head2.
    match value {
        0..=1 => value,
        2..=14 => value + 3,
        _ => 0,
    }
}

fn legacy_bptd_flags(value: u8) -> u8 {
    // Other legacy bits are IK/to-hit flags that collide with FO4 meanings.
    value & 0x09
}

/// Relayout `src_bytes` (laid out per the SOURCE/FO76 struct layout for
/// `record_sig.sub_sig` at `source_form_version`) into the TARGET/FO4 layout.
///
/// Returns:
/// - `Some(new_bytes)` when the subrecord is a divergent `struct:` whose bytes
///   were successfully remapped field-by-field into the target layout.
/// - `None` when no relayout is needed or possible (layouts identical, either
///   layout unavailable, or the source bytes don't match the source layout's
///   expected span) — the caller keeps the original bytes.
pub fn relayout_struct_bytes(
    record_sig: &str,
    sub_sig: &str,
    src_bytes: &[u8],
    source_schema: &AuthoringSchema,
    source_form_version: Option<u16>,
    ctx: &StructRelayoutCtx<'_>,
) -> Option<Vec<u8>> {
    let legacy_bptd = ctx.legacy_bptd_only && record_sig == "BPTD" && sub_sig == "BPND";
    if ctx.legacy_bptd_only && !legacy_bptd {
        return None;
    }

    let source_layout =
        source_schema.struct_field_layout_versioned(record_sig, sub_sig, source_form_version);
    if source_layout.is_empty() {
        return None;
    }
    let target_layout = ctx.target_schema.struct_field_layout_versioned(
        record_sig,
        sub_sig,
        Some(ctx.target_form_version),
    );
    if target_layout.is_empty() {
        return None;
    }

    // Cheap divergence check: identical FK-offset signature ⇒ layouts agree on
    // the load-bearing fields ⇒ verbatim bytes are already correct. (Matches the
    // #31 guard's notion of "divergent", so we only act when it would skip.)
    if fk_offset_signature(&source_layout) == fk_offset_signature(&target_layout) {
        return None;
    }

    let source_span = layout_span(&source_layout);
    let target_span = layout_span(&target_layout);
    if source_span == 0 || target_span == 0 {
        return None;
    }

    // Only relayout a single, whole struct row. `array_struct:`/repeating-row
    // subrecords (src len a multiple of span but > 1 row) are out of scope here:
    // those are handled by the List/Struct decode special-cases, not this byte
    // path. If the source bytes aren't exactly one source-layout row, bail and
    // keep the original bytes rather than risk corrupting a multi-row blob.
    if src_bytes.len() != source_span {
        return None;
    }

    // field_id → source (offset, width).
    let mut source_by_id: rustc_hash::FxHashMap<&str, (usize, usize)> =
        rustc_hash::FxHashMap::default();
    for f in &source_layout {
        source_by_id
            .entry(f.field_id)
            .or_insert((f.offset, f.width));
    }

    let mut out = vec![0u8; target_span];
    for tf in &target_layout {
        let source_field_id = if legacy_bptd && tf.field_id == "explodable_limb_replacement_scale" {
            "limb_replacement_scale"
        } else {
            source_field_id_for_target(record_sig, sub_sig, tf.field_id)
        };
        let Some(&(src_off, src_width)) = source_by_id.get(source_field_id) else {
            // Target-only field (e.g. an FO4 field FO76 lacks): leave zero.
            continue;
        };
        if legacy_bptd {
            match tf.field_id {
                "actor_value" if src_width == 1 && tf.width == 4 => {
                    let raw = legacy_bptd_actor_value_form_id(src_bytes[src_off]);
                    out[tf.offset..tf.offset + 4].copy_from_slice(&raw.to_le_bytes());
                    continue;
                }
                "flags" if src_width == 1 && tf.width == 1 => {
                    out[tf.offset] = legacy_bptd_flags(src_bytes[src_off]);
                    continue;
                }
                "part_type" if src_width == 1 && tf.width == 1 => {
                    out[tf.offset] = legacy_bptd_part_type(src_bytes[src_off]);
                    continue;
                }
                _ => {}
            }
        }
        // Widths should match for a same-named field across games; if they don't
        // (schema disagreement) copy the min to stay in-bounds rather than panic.
        let width = src_width.min(tf.width);
        let src_end = src_off + width;
        let tgt_end = tf.offset + width;
        if src_end > src_bytes.len() || tgt_end > out.len() {
            continue;
        }
        out[tf.offset..tgt_end].copy_from_slice(&src_bytes[src_off..src_end]);
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AuthoringSchema;

    fn fo4() -> std::sync::Arc<AuthoringSchema> {
        AuthoringSchema::for_game("fo4").expect("fo4 schema")
    }
    fn fo76() -> std::sync::Arc<AuthoringSchema> {
        AuthoringSchema::for_game("fo76").expect("fo76 schema")
    }

    /// RACE.DATA is the canonical divergent struct. A 216-byte FO76 row
    /// (form_version 208) must relayout to a 200-byte FO4 row, and the gore
    /// FormIDs must move from their FO76 offsets to their FO4 offsets.
    #[test]
    fn relayouts_race_data_gore_formids_to_fo4_offsets() {
        let fo4 = fo4();
        let fo76 = fo76();

        // Source RACE.DATA at FV 208 is 216 bytes. Put a recognizable sentinel
        // in the FO76 severable_explosion slot (offset 158) and confirm it lands
        // at the FO4 severable_explosion offset (146).
        let src_layout = fo76.struct_field_layout_versioned("RACE", "DATA", Some(208));
        let tgt_layout = fo4.struct_field_layout_versioned("RACE", "DATA", Some(131));
        assert!(!src_layout.is_empty() && !tgt_layout.is_empty());

        let src_span = super::layout_span(&src_layout);
        let tgt_span = super::layout_span(&tgt_layout);
        assert_eq!(src_span, 216, "FO76 RACE.DATA FV208 span");
        assert_eq!(tgt_span, 200, "FO4 RACE.DATA FV131 span");

        let src_sev_off = src_layout
            .iter()
            .find(|f| f.field_id == "severable_explosion")
            .map(|f| f.offset)
            .expect("fo76 severable_explosion");
        let tgt_sev_off = tgt_layout
            .iter()
            .find(|f| f.field_id == "severable_explosion")
            .map(|f| f.offset)
            .expect("fo4 severable_explosion");
        assert_ne!(
            src_sev_off, tgt_sev_off,
            "offsets must diverge for this test"
        );

        let mut src = vec![0u8; src_span];
        let sentinel = 0xAABBCCDDu32.to_le_bytes();
        src[src_sev_off..src_sev_off + 4].copy_from_slice(&sentinel);

        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: false,
        };
        let out = relayout_struct_bytes("RACE", "DATA", &src, &fo76, Some(208), &ctx)
            .expect("RACE.DATA must relayout");
        assert_eq!(out.len(), tgt_span, "output is FO4-sized");
        assert_eq!(
            &out[tgt_sev_off..tgt_sev_off + 4],
            &sentinel,
            "severable_explosion moved to FO4 offset"
        );
    }

    #[test]
    fn relayouts_expl_flags1_to_fo4_flags() {
        let fo4 = fo4();
        let fo76 = fo76();

        let src_layout = fo76.struct_field_layout_versioned("EXPL", "DATA", Some(208));
        let tgt_layout = fo4.struct_field_layout_versioned("EXPL", "DATA", Some(131));
        let src_span = super::layout_span(&src_layout);
        let tgt_span = super::layout_span(&tgt_layout);

        let src_flags = src_layout
            .iter()
            .find(|field| field.field_id == "flags1")
            .expect("FO76 EXPL.DATA flags1");
        let tgt_flags = tgt_layout
            .iter()
            .find(|field| field.field_id == "flags")
            .expect("FO4 EXPL.DATA flags");
        assert_eq!(src_flags.width, tgt_flags.width);

        let expected = 0xA5A5_5A5A_u32.to_le_bytes();
        let mut src = vec![0u8; src_span];
        src[src_flags.offset..src_flags.offset + src_flags.width].copy_from_slice(&expected);

        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: false,
        };
        let out = relayout_struct_bytes("EXPL", "DATA", &src, &fo76, Some(208), &ctx)
            .expect("EXPL.DATA must relayout");
        assert_eq!(out.len(), tgt_span);
        assert_eq!(
            &out[tgt_flags.offset..tgt_flags.offset + tgt_flags.width],
            &expected,
            "FO76 flags1 must populate the FO4 flags field"
        );
    }

    /// A non-divergent struct (or one absent from a schema) must return None so
    /// the caller keeps the original bytes untouched.
    #[test]
    fn non_divergent_returns_none() {
        let fo4 = fo4();
        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: false,
        };
        // FO4→FO4 layout for any struct is identical ⇒ None.
        let layout = fo4.struct_field_layout_versioned("RACE", "DATA", Some(131));
        let span = super::layout_span(&layout);
        let src = vec![0u8; span];
        assert!(
            relayout_struct_bytes("RACE", "DATA", &src, &fo4, Some(131), &ctx).is_none(),
            "identical source/target layout must not relayout"
        );
    }

    #[test]
    fn relayouts_fnv_bptd_node_data_to_fo4_semantics() {
        let fo4 = fo4();
        let fnv = AuthoringSchema::for_game("fnv").expect("fnv schema");
        let fo3 = AuthoringSchema::for_game("fo3").expect("fo3 schema");
        let src_layout = fnv.struct_field_layout_versioned("BPTD", "BPND", Some(15));
        let fo3_layout = fo3.struct_field_layout_versioned("BPTD", "BPND", Some(15));
        let tgt_layout = fo4.struct_field_layout_versioned("BPTD", "BPND", Some(131));
        assert_eq!(
            src_layout
                .iter()
                .map(|field| (field.field_id, field.offset, field.width))
                .collect::<Vec<_>>(),
            fo3_layout
                .iter()
                .map(|field| (field.field_id, field.offset, field.width))
                .collect::<Vec<_>>(),
            "FNV and FO3 BPND layouts must stay identical"
        );
        assert_eq!(super::layout_span(&src_layout), 84);
        assert_eq!(super::layout_span(&tgt_layout), 101);

        let source_field = |id: &str| {
            src_layout
                .iter()
                .find(|field| field.field_id == id)
                .unwrap_or_else(|| panic!("missing FNV BPND field {id}"))
        };
        let target_field = |id: &str| {
            tgt_layout
                .iter()
                .find(|field| field.field_id == id)
                .unwrap_or_else(|| panic!("missing FO4 BPND field {id}"))
        };
        let mut src = vec![0u8; 84];
        let mut set_source = |id: &str, bytes: &[u8]| {
            let field = source_field(id);
            assert_eq!(field.width, bytes.len(), "source width for {id}");
            src[field.offset..field.offset + field.width].copy_from_slice(bytes);
        };
        set_source("damage_mult", &1.0f32.to_le_bytes());
        set_source("flags", &[0x09]);
        set_source("part_type", &[0x05]);
        set_source("health_percent", &[40]);
        set_source("actor_value", &[25]);
        set_source("to_hit_chance", &[20]);
        set_source("explodable_explosion_chance", &[30]);
        set_source("explodable_debris_count", &1u16.to_le_bytes());
        set_source("explodable_debris", &0x000B_8FF4u32.to_le_bytes());
        set_source("explodable_explosion", &0x000B_B636u32.to_le_bytes());
        set_source("tracking_max_angle", &9.0f32.to_le_bytes());
        set_source("explodable_debris_scale", &1.0f32.to_le_bytes());
        set_source("severable_debris_count", &7i32.to_le_bytes());
        set_source("severable_debris", &0x000A_BCDEu32.to_le_bytes());
        set_source("severable_explosion", &0x0003_1CF9u32.to_le_bytes());
        set_source("severable_debris_scale", &1.0f32.to_le_bytes());
        for (id, value) in [
            ("gore_effects_positioning_position_x", 11.0f32),
            ("gore_effects_positioning_position_y", 12.0f32),
            ("gore_effects_positioning_position_z", 13.0f32),
            ("gore_effects_positioning_rotation_x", 14.0f32),
            ("gore_effects_positioning_rotation_y", 15.0f32),
            ("gore_effects_positioning_rotation_z", 16.0f32),
        ] {
            set_source(id, &value.to_le_bytes());
        }
        set_source("severable_impact_dataset", &0x000B_B7B8u32.to_le_bytes());
        set_source("explodable_impact_dataset", &0x000B_B7B8u32.to_le_bytes());
        set_source("severable_decal_count", &[2]);
        set_source("explodable_decal_count", &[3]);
        set_source("unknown_u8_26", &[0xAA]);
        set_source("unknown_u8_27", &[0xBB]);
        set_source("limb_replacement_scale", &2.0f32.to_le_bytes());

        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: true,
        };
        let out = relayout_struct_bytes("BPTD", "BPND", &src, &fnv, Some(15), &ctx)
            .expect("legacy BPTD.BPND must relayout");
        assert_eq!(out.len(), 101);

        let target_bytes = |id: &str| {
            let field = target_field(id);
            &out[field.offset..field.offset + field.width]
        };
        assert_eq!(target_bytes("damage_mult"), 1.0f32.to_le_bytes());
        assert_eq!(
            target_bytes("explodable_debris"),
            0x000B_8FF4u32.to_le_bytes()
        );
        assert_eq!(
            target_bytes("explodable_explosion"),
            0x000B_B636u32.to_le_bytes()
        );
        assert_eq!(
            target_bytes("severable_explosion"),
            0x0003_1CF9u32.to_le_bytes()
        );
        assert_eq!(
            target_bytes("severable_debris"),
            0x000A_BCDEu32.to_le_bytes()
        );
        assert_eq!(
            target_bytes("severable_impact_dataset"),
            0x000B_B7B8u32.to_le_bytes()
        );
        assert_eq!(
            target_bytes("explodable_impact_dataset"),
            0x000B_B7B8u32.to_le_bytes()
        );
        assert_eq!(
            target_bytes("explodable_debris_scale"),
            1.0f32.to_le_bytes()
        );
        assert_eq!(target_bytes("severable_debris_scale"), 1.0f32.to_le_bytes());
        assert_eq!(target_bytes("actor_value"), 0x0000_036Cu32.to_le_bytes());
        assert_eq!(target_bytes("flags"), [0x09]);
        assert_eq!(target_bytes("part_type"), [0x08]);
        assert_eq!(target_bytes("health_percent"), [40]);
        assert_eq!(target_bytes("to_hit_chance"), [20]);
        assert_eq!(target_bytes("explodable_explosion_chance"), [30]);
        assert_eq!(target_bytes("severable_debris_count"), [7]);
        assert_eq!(target_bytes("explodable_debris_count"), [1]);
        assert_eq!(target_bytes("severable_decal_count"), [2]);
        assert_eq!(target_bytes("explodable_decal_count"), [3]);
        assert_eq!(
            target_bytes("explodable_limb_replacement_scale"),
            2.0f32.to_le_bytes()
        );
        for id in [
            "cut_min",
            "cut_max",
            "cut_radius",
            "gore_effects_local_rotate_x",
            "gore_effects_local_rotate_y",
            "cut_tesselation",
            "on_cripple_debris_scale",
        ] {
            assert_eq!(target_bytes(id), 0.0f32.to_le_bytes(), "target-only {id}");
        }
        for id in [
            "on_cripple_art_object",
            "on_cripple_debris",
            "on_cripple_explosion",
            "on_cripple_impact_dataset",
        ] {
            assert_eq!(target_bytes(id), 0u32.to_le_bytes(), "target-only {id}");
        }
        for id in [
            "non_lethal_dismemberment_chance",
            "geometry_segment_index",
            "on_cripple_debris_count",
            "on_cripple_decal_count",
        ] {
            assert_eq!(target_bytes(id), [0], "target-only {id}");
        }

        let race_layout = fnv.struct_field_layout_versioned("RACE", "DATA", Some(15));
        let race = vec![0u8; super::layout_span(&race_layout)];
        assert!(
            relayout_struct_bytes("RACE", "DATA", &race, &fnv, Some(15), &ctx).is_none(),
            "legacy relayout scope must not touch non-BPTD structs"
        );
        assert!(
            relayout_struct_bytes("BPTD", "BPND", &[0xA5; 101], &fnv, Some(15), &ctx).is_none(),
            "already-target-sized rows must be preserved"
        );
    }

    #[test]
    fn covers_legacy_bptd_enum_census() {
        // FalloutNV.esm (304 rows) plus Fallout3.esm (207 rows).
        const ACTOR_VALUE_CENSUS: &[(u8, usize)] = &[
            (25, 86),
            (26, 77),
            (27, 45),
            (28, 72),
            (29, 105),
            (30, 97),
            (31, 26),
            (255, 3),
        ];
        const PART_TYPE_CENSUS: &[(u8, usize)] = &[
            (0, 80),
            (1, 69),
            (2, 3),
            (3, 52),
            (4, 6),
            (5, 55),
            (6, 6),
            (7, 57),
            (8, 31),
            (9, 13),
            (10, 62),
            (11, 29),
            (12, 19),
            (13, 28),
            (14, 1),
        ];
        const FLAGS_CENSUS: &[(u8, usize)] = &[
            (0, 73),
            (1, 48),
            (2, 6),
            (3, 16),
            (6, 5),
            (7, 4),
            (8, 38),
            (9, 181),
            (11, 33),
            (14, 4),
            (15, 57),
            (27, 5),
            (47, 2),
            (50, 4),
            (58, 2),
            (59, 16),
            (72, 6),
            (73, 9),
            (78, 1),
            (90, 1),
        ];
        assert_eq!(
            ACTOR_VALUE_CENSUS
                .iter()
                .map(|(_, count)| count)
                .sum::<usize>(),
            511
        );
        assert_eq!(
            PART_TYPE_CENSUS
                .iter()
                .map(|(_, count)| count)
                .sum::<usize>(),
            511
        );
        assert_eq!(
            FLAGS_CENSUS.iter().map(|(_, count)| count).sum::<usize>(),
            511
        );

        let fo4 = fo4();
        let fnv = AuthoringSchema::for_game("fnv").expect("fnv schema");
        let src_layout = fnv.struct_field_layout_versioned("BPTD", "BPND", Some(15));
        let tgt_layout = fo4.struct_field_layout_versioned("BPTD", "BPND", Some(131));
        let source_offset = |id: &str| {
            src_layout
                .iter()
                .find(|field| field.field_id == id)
                .map(|field| field.offset)
                .unwrap_or_else(|| panic!("missing FNV BPND field {id}"))
        };
        let target_field = |id: &str| {
            tgt_layout
                .iter()
                .find(|field| field.field_id == id)
                .unwrap_or_else(|| panic!("missing FO4 BPND field {id}"))
        };
        let actor_source_offset = source_offset("actor_value");
        let part_source_offset = source_offset("part_type");
        let flags_source_offset = source_offset("flags");
        let actor_target = target_field("actor_value");
        let part_target = target_field("part_type");
        let flags_target = target_field("flags");
        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: true,
        };
        let relayout = |src: &[u8]| {
            relayout_struct_bytes("BPTD", "BPND", src, &fnv, Some(15), &ctx)
                .expect("legacy BPTD.BPND relayout")
        };

        for &(value, count) in ACTOR_VALUE_CENSUS {
            for _ in 0..count {
                let mut src = vec![0u8; 84];
                src[actor_source_offset] = value;
                let out = relayout(&src);
                let actual = u32::from_le_bytes(
                    out[actor_target.offset..actor_target.offset + 4]
                        .try_into()
                        .unwrap(),
                );
                assert_eq!(actual, legacy_bptd_actor_value_form_id(value));
            }
        }
        for &(value, count) in PART_TYPE_CENSUS {
            for _ in 0..count {
                let mut src = vec![0u8; 84];
                src[part_source_offset] = value;
                let out = relayout(&src);
                assert_eq!(out[part_target.offset], legacy_bptd_part_type(value));
            }
        }
        for &(value, count) in FLAGS_CENSUS {
            for _ in 0..count {
                let mut src = vec![0u8; 84];
                src[flags_source_offset] = value;
                let out = relayout(&src);
                assert_eq!(out[flags_target.offset], legacy_bptd_flags(value));
            }
        }
    }
}
