//! TES4 plugin-header normalization for FO4 conversion targets.
//!
//! Stamps the target's TES4 with an author tag (`header.author`, emitted as
//! `CNAM`) and `INTV` = 1 (in `header.extra_subrecords`), so the output reads
//! as a fresh mod rather than a copy of a vanilla ESM.
//!
//! A plugin loaded from disk (e.g. the seed plugin from
//! `build_authoring_dir_streaming_native`) keeps its TES4 bytes in
//! `header.raw_subrecords`, which `header_subrecords_from_parsed` emits instead
//! of the typed fields. Every header edit here therefore clears
//! `raw_subrecords`.
//!
//! `encode_record_for_target` compresses the CELL/LAND records it encodes, but
//! records already in the seed plugin (notably FO76→FO4 terrain CELLs) skip that
//! path, so this module stamps COMPRESSED across the target tree once at run
//! creation.
//!
//! Non-FO4 targets are no-ops.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    COMPRESSED_RECORD_FLAG, ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_store_ref,
};
use smol_str::SmolStr;

use crate::run::RunError;

/// Author tag written to TES4 CNAM on FO4 conversion output.
pub const FO4_CONVERSION_AUTHOR: &str = "modkit-fo76-to-fo4";

/// INTV (Info Version) data for FO4 conversion output. The body is a u32 LE
/// with value 1.
pub const FO4_CONVERSION_INTV_BYTES: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

/// Apply plugin-header normalization to the target plugin handle.
///
/// For FO4: sets `header.author`, upserts one `INTV` in `extra_subrecords`,
/// clears `raw_subrecords`, and stamps COMPRESSED (0x00040000) on every CELL
/// and LAND so `io::record_bytes_from_parsed` compresses them at emit. No-op
/// for other targets.
pub fn normalize_target_plugin_header(
    target_handle_id: u64,
    target_game: &str,
) -> Result<(), RunError> {
    // `Game::as_str()` yields lowercase `"fo4"`. A miscased target ("FO4",
    // "Fallout4") fails this in debug builds instead of silently skipping
    // normalization.
    debug_assert!(
        target_game == target_game.to_ascii_lowercase().trim(),
        "target_game must be canonical-lowercase (got {target_game:?}); upstream caller bug"
    );
    if target_game != "fo4" {
        return Ok(());
    }
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?;
    // Tolerate sentinel target_handle_ids (used by registry-only unit tests
    // that never load a plugin into the store). Real conversion runs always
    // pre-allocate a target handle via `plugin_handle_new`, so the lookup
    // succeeds in production.
    let Some(slot) = store.get_mut(&target_handle_id) else {
        return Ok(());
    };
    let header = &mut slot.parsed.header;
    header.author = FO4_CONVERSION_AUTHOR.to_string();

    let intv_data = Bytes::from_static(&FO4_CONVERSION_INTV_BYTES);
    if let Some(existing) = header
        .extra_subrecords
        .iter_mut()
        .find(|sr| sr.signature.as_str() == "INTV")
    {
        existing.data = intv_data;
    } else {
        header.extra_subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("INTV"),
            data: intv_data,
            semantic_type: None,
        });
    }
    // header_subrecords_from_parsed (esp/src/io.rs) emits raw_subrecords when
    // non-empty, which would drop the author + INTV edits above.
    header.raw_subrecords.clear();

    stamp_compressed_on_cell_and_land(&mut slot.parsed.root_items);

    slot.invalidate_sections();
    Ok(())
}

fn stamp_compressed_on_cell_and_land(items: &mut [ParsedItem]) {
    for item in items {
        match item {
            ParsedItem::Record(record) => stamp_compressed_if_cell_or_land(record),
            ParsedItem::Group(group) => stamp_compressed_on_cell_and_land(&mut group.children),
        }
    }
}

fn stamp_compressed_if_cell_or_land(record: &mut ParsedRecord) {
    let sig = record.signature.as_str();
    if sig == "CELL" || sig == "LAND" {
        record.flags |= COMPRESSED_RECORD_FLAG;
    }
}

/// Stamp a TES4 `SNAM` (plugin description) on a target plugin handle.
///
/// Carries the Appalachia upgrade version string (e.g. `"alpha2"`) so a later
/// run can read the installed version back from the deployed ESM
/// (`bacup_lib/version_stamp.py::read_plugin_snam`). FO4-only like
/// `normalize_target_plugin_header`, and likewise clears `raw_subrecords` so the
/// typed `header.description` is emitted as `SNAM`.
pub fn set_tes4_snam(target_handle_id: u64, target_game: &str, text: &str) -> Result<(), RunError> {
    if target_game != "fo4" {
        return Ok(());
    }
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?;
    let Some(slot) = store.get_mut(&target_handle_id) else {
        return Ok(());
    };
    let header = &mut slot.parsed.header;
    header.description = text.to_string();
    header.raw_subrecords.clear();
    slot.invalidate_sections();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use esp_authoring_core::plugin_runtime::{ParsedGroup, plugin_handle_new_native};

    #[test]
    fn fo4_target_plugin_header_has_author_and_intv() {
        // After normalization, the target plugin's
        // TES4 CNAM (author) is "modkit-fo76-to-fo4" and an INTV subrecord
        // with value 1 (little-endian u32 0x01000000 in bytes [1,0,0,0])
        // is present.
        let target = match plugin_handle_new_native("Phase8Hdr.esp", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return, // no Python runtime in unit tests
        };

        normalize_target_plugin_header(target, "fo4").expect("normalize fo4");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        assert_eq!(slot.parsed.header.author, FO4_CONVERSION_AUTHOR);
        let intv = slot
            .parsed
            .header
            .extra_subrecords
            .iter()
            .find(|sr| sr.signature.as_str() == "INTV")
            .expect("INTV subrecord present on FO4 target after normalization");
        assert_eq!(intv.data.as_ref(), &FO4_CONVERSION_INTV_BYTES);
    }

    #[test]
    fn non_fo4_target_plugin_header_is_left_untouched() {
        let target = match plugin_handle_new_native("Phase8Hdr.esp", Some("skyrimse")) {
            Ok(id) => id,
            Err(_) => return,
        };

        // Pre-set some other author to make sure we don't clobber it.
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target).unwrap();
            slot.parsed.header.author = "preexisting".to_string();
        }

        normalize_target_plugin_header(target, "skyrimse").expect("noop");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        assert_eq!(slot.parsed.header.author, "preexisting");
        assert!(
            !slot
                .parsed
                .header
                .extra_subrecords
                .iter()
                .any(|sr| sr.signature.as_str() == "INTV"),
            "non-FO4 targets must not gain an INTV subrecord"
        );
    }

    #[test]
    fn normalize_is_idempotent_on_intv() {
        // Calling normalize twice should not duplicate the INTV subrecord.
        let target = match plugin_handle_new_native("Phase8Hdr.esp", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return,
        };

        normalize_target_plugin_header(target, "fo4").unwrap();
        normalize_target_plugin_header(target, "fo4").unwrap();

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let intv_count = slot
            .parsed
            .header
            .extra_subrecords
            .iter()
            .filter(|sr| sr.signature.as_str() == "INTV")
            .count();
        assert_eq!(intv_count, 1, "INTV should appear exactly once");
    }

    #[test]
    fn normalize_clears_header_raw_subrecords_so_typed_fields_take_effect() {
        // A plugin loaded from disk keeps its TES4 bytes in
        // `header.raw_subrecords`, which `header_subrecords_from_parsed` emits
        // instead of the typed fields, so author/INTV edits need it cleared.
        let target = match plugin_handle_new_native("Phase8Hdr.esp", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return,
        };

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target).unwrap();
            slot.parsed.header.raw_subrecords.push(ParsedSubrecord {
                signature: SmolStr::new("CNAM"),
                data: Bytes::from_static(b"stale-author"),
                semantic_type: None,
            });
        }

        normalize_target_plugin_header(target, "fo4").unwrap();

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        assert!(
            slot.parsed.header.raw_subrecords.is_empty(),
            "raw_subrecords must be cleared so author/INTV typed fields are emitted"
        );
    }

    #[test]
    fn normalize_stamps_compressed_on_cell_and_land_records() {
        // Regression: terrain CELLs and LANDs originate from the seed plugin
        // (built from authoring YAML) and never go through
        // encode_record_for_target. Normalization must walk the target tree
        // and stamp the COMPRESSED flag so io::record_bytes_from_parsed
        // gzips them at write time.
        let target = match plugin_handle_new_native("Phase8Hdr.esp", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return,
        };

        // Seed the parsed tree with one CELL, one LAND, one STAT, both inside
        // a nested GRUP and at the top level — exercises the recursive walk.
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target).unwrap();
            let cell = ParsedRecord {
                signature: SmolStr::new("CELL"),
                form_id: 0x01_000001,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: Some(0),
                subrecords: Vec::new(),
                raw_payload: None,
                parse_error: None,
            };
            let land = ParsedRecord {
                signature: SmolStr::new("LAND"),
                form_id: 0x01_000002,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: Some(0),
                subrecords: Vec::new(),
                raw_payload: None,
                parse_error: None,
            };
            let stat = ParsedRecord {
                signature: SmolStr::new("STAT"),
                form_id: 0x01_000003,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: Some(0),
                subrecords: Vec::new(),
                raw_payload: None,
                parse_error: None,
            };
            let nested_group = ParsedGroup {
                label: *b"WRLD",
                group_type: 0,
                tail: Bytes::new(),
                children: vec![ParsedItem::Record(cell), ParsedItem::Record(land)],
            };
            slot.parsed.root_items.push(ParsedItem::Group(nested_group));
            slot.parsed.root_items.push(ParsedItem::Record(stat));
        }

        normalize_target_plugin_header(target, "fo4").unwrap();

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let mut found_cell = false;
        let mut found_land = false;
        let mut stat_flags = 0_u32;
        for item in &slot.parsed.root_items {
            match item {
                ParsedItem::Group(group) => {
                    for child in &group.children {
                        if let ParsedItem::Record(record) = child {
                            match record.signature.as_str() {
                                "CELL" => {
                                    found_cell = true;
                                    assert!(
                                        record.flags & COMPRESSED_RECORD_FLAG != 0,
                                        "nested CELL must be COMPRESSED-stamped"
                                    );
                                }
                                "LAND" => {
                                    found_land = true;
                                    assert!(
                                        record.flags & COMPRESSED_RECORD_FLAG != 0,
                                        "nested LAND must be COMPRESSED-stamped"
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ParsedItem::Record(record) if record.signature.as_str() == "STAT" => {
                    stat_flags = record.flags;
                }
                _ => {}
            }
        }
        assert!(found_cell, "test setup must include a CELL record");
        assert!(found_land, "test setup must include a LAND record");
        assert_eq!(
            stat_flags & COMPRESSED_RECORD_FLAG,
            0,
            "non-CELL/LAND records must not gain COMPRESSED"
        );
    }

    #[test]
    fn set_tes4_snam_round_trips_on_fo4_target() {
        let target = match plugin_handle_new_native("Phase8SnamTest.esp", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return, // no Python runtime in unit tests
        };

        set_tes4_snam(target, "fo4", "alpha2").expect("set snam");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        assert_eq!(slot.parsed.header.description, "alpha2");
    }

    #[test]
    fn set_tes4_snam_is_noop_for_non_fo4_targets() {
        let target = match plugin_handle_new_native("Phase8SnamTest.esp", Some("skyrimse")) {
            Ok(id) => id,
            Err(_) => return,
        };

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target).unwrap();
            slot.parsed.header.description = "preexisting".to_string();
        }

        set_tes4_snam(target, "skyrimse", "alpha2").expect("noop");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        assert_eq!(slot.parsed.header.description, "preexisting");
    }
}
