//! FO76→FO4 interior sky-region assignment (post-copy).
//!
//! FO4 renders a "Show Sky" interior's weather **only** from `XCCM` "Sky/Weather
//! from Region" → REGN. FO76 also uses the FO76-only `XISR` "Interior Sky
//! Override" (→ a WTHR weather directly), which has no FO4 equivalent and is not
//! in the FO4 CELL whitelist, so it is dropped in translation. Any FO76 show-sky
//! interior without an `XCCM` therefore lands in FO4 with no sky source.
//!
//! This pass stamps `XCCM` on every such cell, layered by fidelity:
//!   - **T1** — the cell's dropped `XISR` weather is read from the still-open
//!     SOURCE handle; if some converted REGN already lists that weather in an
//!     `RDWT` entry, point `XCCM` at that region (faithful, zero new records).
//!   - **T4** — otherwise fall back to the data-derived default region (the
//!     `XCCM` target most of the already-correct cells point at).
//!
//! Tiers T2 (location→marker→point-in-polygon) and T3 (synthesize a weather-only
//! REGN) layer on later for cells with no weather signal / no existing region.
//!
//! Runs as a free function (not a registry `Fixup`) because interior CELLs do not
//! exist at fixup time — they are inserted post-fixups by `emit_interior_cells`.
//! Invoked by `ConversionRun::synthesize_sky_regions` AFTER interior-cell emit
//! and encounter-zone synthesis, with the source handle still open.

mod index;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue};
use crate::session::PluginSession;
use rustc_hash::FxHashMap;

use index::build_sky_index;

/// Pull the WTHR object-id out of a decoded source CELL's `XISR` (Interior Sky
/// Override, `struct:I,f` = weather FormID + float). The conversion decoder
/// rawifies `struct:` codecs, so `XISR` arrives as `Bytes` (weather FormID in the
/// first 4 bytes, LE) rather than a decoded `Struct`; both shapes are handled.
/// `None` when the cell has no `XISR` or its weather is null.
fn xisr_weather_objid(
    cell: &crate::record::Record,
    interner: &crate::sym::StringInterner,
) -> Option<u32> {
    let field = cell.fields.iter().find(|f| f.sig.as_str() == "XISR")?;
    let objid = match &field.value {
        FieldValue::Bytes(b) if b.len() >= 4 => {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]) & 0x00FF_FFFF
        }
        FieldValue::Struct(members) => {
            let (_, value) = members
                .iter()
                .find(|(sym, _)| interner.resolve(*sym) == Some("weather"))?;
            match value {
                FieldValue::FormKey(fk) => fk.local & 0x00FF_FFFF,
                _ => return None,
            }
        }
        _ => return None,
    };
    (objid != 0).then_some(objid)
}

pub fn synthesize_sky_regions(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let output_plugin = mapper.output_plugin_sym();
    let cell_sig = SigCode::from_str("CELL").map_err(FixupError::SchemaError)?;
    let xccm_sig = SubrecordSig::from_str("XCCM").map_err(FixupError::SchemaError)?;

    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let source_schema = config
        .source_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing source schema".into()))?;

    let idx = build_sky_index(&session.target_slot().parsed.root_items);
    if idx.broken_cells.is_empty() {
        return Ok(report);
    }
    let default_region = idx.default_region();

    // Map every source CELL object-id → its FormKey, so a broken cell (target
    // object-id, preserved 1:1) can be read back from the source for its XISR.
    let source_cell_fks = session
        .source_form_keys_of_sig(cell_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let src_by_obj: FxHashMap<u32, FormKey> = source_cell_fks
        .into_iter()
        .map(|fk| (fk.local & 0x00FF_FFFF, fk))
        .collect();

    let mut cell_changes: Vec<crate::record::Record> = Vec::new();
    for &cell_obj in &idx.broken_cells {
        // T1: the cell's dropped XISR weather → a region that already lists it.
        let region_obj = src_by_obj
            .get(&cell_obj)
            .and_then(|src_fk| {
                session
                    .source_record_decoded(src_fk, source_schema.as_ref(), mapper.interner)
                    .ok()
            })
            .and_then(|rec| xisr_weather_objid(&rec, mapper.interner))
            .and_then(|weather| idx.weather_to_regn.get(&weather))
            .and_then(|regns| regns.first().copied())
            // T4: data-derived default region.
            .or(default_region);

        let Some(region_obj) = region_obj else {
            continue;
        };
        let cell_fk = FormKey {
            local: cell_obj,
            plugin: output_plugin,
        };
        let mut cell =
            match session.record_decoded(&cell_fk, target_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
        let region_fk = FormKey {
            local: region_obj,
            plugin: output_plugin,
        };
        if let Some(f) = cell.fields.iter_mut().find(|f| f.sig == xccm_sig) {
            f.value = FieldValue::FormKey(region_fk);
        } else {
            cell.fields.push(FieldEntry {
                sig: xccm_sig,
                value: FieldValue::FormKey(region_fk),
            });
        }
        cell_changes.push(cell);
    }

    report.records_changed += session
        .replace_records_contents(cell_changes, target_schema.as_ref(), mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState, ResolutionMode};
    use crate::record::Record;
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord,
        ensure_interior_cell_and_child_group, plugin_handle_new_native, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    const OUTPUT_PLUGIN: &str = "Converted.esm";

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    // ── pure helper: XISR decode ─────────────────────────────────────────────

    #[test]
    fn xisr_weather_objid_reads_struct_member() {
        let i = StringInterner::new();
        let out = i.intern(OUTPUT_PLUGIN);
        let mut rec = Record::new(
            SigCode::from_str("CELL").unwrap(),
            FormKey {
                local: 1,
                plugin: out,
            },
        );
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XISR").unwrap(),
            value: FieldValue::Struct(vec![
                (
                    i.intern("weather"),
                    FieldValue::FormKey(FormKey {
                        local: 0x013A1A9F,
                        plugin: out,
                    }),
                ),
                (i.intern("unknown_float"), FieldValue::Float(-1.0)),
            ]),
        });
        assert_eq!(xisr_weather_objid(&rec, &i), Some(0x3A1A9F));
    }

    #[test]
    fn xisr_weather_objid_reads_rawified_bytes() {
        // The conversion rawifies `struct:` codecs, so a decoded source XISR
        // arrives as Bytes: weather FormID (4) + float (4).
        let i = StringInterner::new();
        let out = i.intern(OUTPUT_PLUGIN);
        let mut rec = Record::new(
            SigCode::from_str("CELL").unwrap(),
            FormKey {
                local: 1,
                plugin: out,
            },
        );
        let mut bytes = 0x003A1A9Fu32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(-1.0_f32).to_le_bytes());
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XISR").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&bytes)),
        });
        assert_eq!(xisr_weather_objid(&rec, &i), Some(0x3A1A9F));
    }

    #[test]
    fn xisr_weather_objid_none_without_xisr() {
        let i = StringInterner::new();
        let out = i.intern(OUTPUT_PLUGIN);
        let rec = Record::new(
            SigCode::from_str("CELL").unwrap(),
            FormKey {
                local: 1,
                plugin: out,
            },
        );
        assert_eq!(xisr_weather_objid(&rec, &i), None);
    }

    // ── integration: full pass over handle store ─────────────────────────────

    fn interior_cell_record(
        form_id: u32,
        eid: &str,
        flags: u16,
        xisr_weather: Option<u32>,
    ) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        let mut subs = vec![
            sub("EDID", edid),
            sub("DATA", flags.to_le_bytes().to_vec()),
            sub("XCLW", 3.0_f32.to_le_bytes().to_vec()),
        ];
        if let Some(w) = xisr_weather {
            let mut isr = w.to_le_bytes().to_vec();
            isr.extend_from_slice(&(-1.0_f32).to_le_bytes());
            subs.push(sub("XISR", isr));
        }
        ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: subs,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn put_source_cell(source: u64, record: ParsedRecord) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&source).unwrap();
        if let Some(ParsedItem::Group(g)) = slot
            .parsed
            .root_items
            .iter_mut()
            .find(|item| matches!(item, ParsedItem::Group(g) if &g.label == b"CELL"))
        {
            g.children.push(ParsedItem::Record(record));
        } else {
            slot.parsed.root_items.push(ParsedItem::Group(ParsedGroup {
                label: *b"CELL",
                group_type: 0,
                tail: Bytes::new(),
                children: vec![ParsedItem::Record(record)],
            }));
        }
        slot.invalidate_sections();
    }

    fn put_target_regn(target: u64, form_id: u32, weathers: &[u32]) {
        let mut rdwt = Vec::new();
        for w in weathers {
            rdwt.extend_from_slice(&w.to_le_bytes());
            rdwt.extend_from_slice(&50u32.to_le_bytes());
            rdwt.extend_from_slice(&0u32.to_le_bytes());
        }
        let record = ParsedRecord {
            signature: SmolStr::new("REGN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("RDWT", rdwt)],
            raw_payload: None,
            parse_error: None,
        };
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&target).unwrap();
        if let Some(ParsedItem::Group(g)) = slot
            .parsed
            .root_items
            .iter_mut()
            .find(|item| matches!(item, ParsedItem::Group(g) if &g.label == b"REGN"))
        {
            g.children.push(ParsedItem::Record(record));
        } else {
            slot.parsed.root_items.push(ParsedItem::Group(ParsedGroup {
                label: *b"REGN",
                group_type: 0,
                tail: Bytes::new(),
                children: vec![ParsedItem::Record(record)],
            }));
        }
        slot.invalidate_sections();
    }

    fn find_cell<'a>(items: &'a [ParsedItem], objid: u32) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(r)
                    if r.signature.as_str() == "CELL" && r.form_id & 0x00FF_FFFF == objid =>
                {
                    return Some(r);
                }
                ParsedItem::Group(g) => {
                    if let Some(f) = find_cell(&g.children, objid) {
                        return Some(f);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn xccm_target(cell: &ParsedRecord) -> Option<u32> {
        cell.subrecords
            .iter()
            .find(|s| s.signature.as_str() == "XCCM")
            .map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]) & 0x00FF_FFFF)
    }

    fn config() -> FixupConfig {
        FixupConfig {
            preserve_source_ids: true,
            is_whole_plugin: true,
            target_schema: Some(AuthoringSchema::for_game("fo4").unwrap()),
            source_schema: Some(AuthoringSchema::for_game("fo76").unwrap()),
            ..FixupConfig::default()
        }
    }

    fn run_pass(source: u64, target: u64) -> FixupReport {
        let interner = StringInterner::new();
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: OUTPUT_PLUGIN.into(),
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..MapperOptions::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(target, Some(source)).unwrap();
        let r = synthesize_sky_regions(&mut session, &mut mapper, &config()).unwrap();
        session.flush_pending_effects();
        r
    }

    /// T1: a broken show-sky interior whose dropped XISR weather belongs to a
    /// converted REGN gets XCCM = that region.
    #[test]
    fn t1_xisr_weather_resolves_to_existing_region() {
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();
        let cell_local = 0x005B59C8;
        let weather = 0x003A1A9F;
        let region = 0x0020CFF8;

        // Source cell carries the XISR weather; target cell is the translated
        // show-sky interior with no XCCM; target REGN lists that weather.
        put_source_cell(
            source,
            interior_cell_record(cell_local, "MiresEye01", 0x0081, Some(weather)),
        );
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "MiresEye01", 0x0081, None),
        )
        .unwrap();
        put_target_regn(target, region, &[0x002BA027, weather]);

        let report = run_pass(source, target);
        assert_eq!(report.records_changed, 1);

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let cell = find_cell(&slot.parsed.root_items, cell_local & 0x00FF_FFFF).unwrap();
        assert_eq!(
            xccm_target(cell),
            Some(region & 0x00FF_FFFF),
            "XCCM points at the region that lists the cell's XISR weather"
        );
    }

    /// T4: a broken show-sky interior with no XISR weather falls back to the
    /// default region (the XCCM most existing cells already point at).
    #[test]
    fn t4_no_weather_falls_back_to_default_region() {
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();
        let default_region = 0x0020CFF8;

        // Two already-correct cells establish the default region by majority.
        ensure_interior_cell_and_child_group(
            target,
            with_xccm(
                interior_cell_record(0x00100001, "Ok1", 0x0081, None),
                default_region,
            ),
        )
        .unwrap();
        ensure_interior_cell_and_child_group(
            target,
            with_xccm(
                interior_cell_record(0x00100002, "Ok2", 0x0081, None),
                default_region,
            ),
        )
        .unwrap();
        // The broken cell has no XISR weather and no matching region.
        let broken = 0x00100003;
        put_source_cell(source, interior_cell_record(broken, "Broken", 0x0081, None));
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(broken, "Broken", 0x0081, None),
        )
        .unwrap();

        let report = run_pass(source, target);
        assert_eq!(report.records_changed, 1);

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let cell = find_cell(&slot.parsed.root_items, broken).unwrap();
        assert_eq!(xccm_target(cell), Some(default_region & 0x00FF_FFFF));
    }

    fn with_xccm(mut rec: ParsedRecord, region: u32) -> ParsedRecord {
        rec.subrecords
            .push(sub("XCCM", region.to_le_bytes().to_vec()));
        rec
    }
}
