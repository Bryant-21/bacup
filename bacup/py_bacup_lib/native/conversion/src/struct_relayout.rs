//! FO76→FO4 `struct:` codec subrecord byte-relayout.
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
//! layouts (matched by `field_id`) from its source offset to its target offset
//! into a fresh, target-sized buffer. Target-only fields stay zero; source-only
//! fields are dropped. The result is FO4-laid-out bytes that the FO4 game/CK and
//! the downstream FK remap (`remap_struct_fk_fields`) read at the correct offset.
//!
//! Runs at decode time (we have the raw source form_version + source schema in
//! hand) BEFORE the FK remap fixup. Once the bytes are FO4-laid-out, the #31
//! divergence guard in `ref_index::source_struct_layout_diverges` sees
//! source==target layout and stops skipping, so the gore FKs get their master
//! byte remapped (00→07) via their existing `formlink_targets`.

use esp_authoring_core::plugin_runtime::StructFieldInfo;

use crate::schema::AuthoringSchema;

/// Context carried through the decode path enabling FO76→FO4 struct relayout.
/// Only `Some` on the whole-plugin / asset translate path for an FO76→FO4 run;
/// `None` everywhere else preserves the prior verbatim-bytes behaviour.
pub struct StructRelayoutCtx<'a> {
    /// FO4 target schema — supplies the destination field layout.
    pub target_schema: &'a AuthoringSchema,
    /// FO4 target record form_version (131) — selects the target layout's
    /// version-gated fields, matching what the FO4 game/CK will parse.
    pub target_form_version: u16,
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
        let Some(&(src_off, src_width)) = source_by_id.get(tf.field_id) else {
            // Target-only field (e.g. an FO4 field FO76 lacks): leave zero.
            continue;
        };
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

    /// A non-divergent struct (or one absent from a schema) must return None so
    /// the caller keeps the original bytes untouched.
    #[test]
    fn non_divergent_returns_none() {
        let fo4 = fo4();
        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
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
}
