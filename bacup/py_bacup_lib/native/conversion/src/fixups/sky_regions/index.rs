//! Pure index over the post-copy TARGET tree for interior-sky region synthesis.
//!
//! Collects, in one raw walk (no schema decode):
//!   - `broken_cells`: interior CELLs flagged **Show Sky** (`DATA` bit 0x80) that
//!     carry no `XCCM` (Sky/Weather from Region). These are the cells FO4 renders
//!     with no sky source because FO76's FO76-only `XISR` "Interior Sky Override"
//!     was dropped in translation (not in the FO4 whitelist).
//!   - `weather_to_regn`: WTHR object-id → REGN object-ids that list it in an
//!     `RDWT` (Weather Types) entry — the lookup for tier 1 (reuse the region a
//!     dropped XISR weather already belongs to).
//!   - `xccm_counts`: histogram of `XCCM` targets over cells that DO have one —
//!     its mode is the data-derived default region for tier 4.
//!
//! Interior vs exterior is decided exactly as `encounter_zones::cell_index`:
//! a CELL not under a WRLD world-children group (`group_type == 1`) is interior.

use esp_authoring_core::plugin_runtime::ParsedItem;
use rustc_hash::FxHashMap;

const WORLD_CHILD_GROUP: i32 = 1;
const FLAG_INTERIOR: u16 = 0x0001;
const FLAG_SHOW_SKY: u16 = 0x0080;
const RDWT_ROW_LEN: usize = 12;

#[derive(Default)]
pub struct SkyIndex {
    /// Object-ids of interior CELLs flagged Show-Sky with no `XCCM`.
    pub broken_cells: Vec<u32>,
    /// WTHR object-id → REGN object-ids that carry it in an `RDWT` entry.
    pub weather_to_regn: FxHashMap<u32, Vec<u32>>,
    /// `XCCM` target object-id → count, over cells that already have an `XCCM`.
    pub xccm_counts: FxHashMap<u32, u32>,
}

impl SkyIndex {
    /// The data-derived default region (tier 4): the `XCCM` target most cells
    /// already point at. Deterministic tie-break: higher count, then lower
    /// object-id. `None` when no cell in the output carries an `XCCM`.
    pub fn default_region(&self) -> Option<u32> {
        self.xccm_counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            .map(|(objid, _)| *objid)
    }
}

fn flags_u16(data: &[u8]) -> Option<u16> {
    (data.len() >= 2).then(|| u16::from_le_bytes([data[0], data[1]]))
}

fn formid_from_first4(d: &[u8]) -> Option<u32> {
    (d.len() >= 4).then(|| u32::from_le_bytes([d[0], d[1], d[2], d[3]]) & 0x00FF_FFFF)
}

fn walk(items: &[ParsedItem], world: Option<u32>, out: &mut SkyIndex) {
    for item in items {
        match item {
            ParsedItem::Group(g) => {
                let w = if g.group_type == WORLD_CHILD_GROUP {
                    Some(u32::from_le_bytes(g.label) & 0x00FF_FFFF)
                } else {
                    world
                };
                walk(&g.children, w, out);
            }
            ParsedItem::Record(r) => {
                let objid = r.form_id & 0x00FF_FFFF;
                match r.signature.as_str() {
                    "REGN" => {
                        for sub in r
                            .subrecords
                            .iter()
                            .filter(|s| s.signature.as_str() == "RDWT")
                        {
                            for row in sub.data.chunks_exact(RDWT_ROW_LEN) {
                                if let Some(w) = formid_from_first4(row) {
                                    if w != 0 {
                                        out.weather_to_regn.entry(w).or_default().push(objid);
                                    }
                                }
                            }
                        }
                    }
                    // A CELL under a WRLD world-children group is exterior; one
                    // that is not (top interior Block/Sub-Block topology) is interior.
                    "CELL" if world.is_none() => {
                        let xccm = r
                            .subrecords
                            .iter()
                            .find(|s| s.signature.as_str() == "XCCM")
                            .and_then(|s| formid_from_first4(&s.data));
                        if let Some(region) = xccm {
                            if region != 0 {
                                *out.xccm_counts.entry(region).or_default() += 1;
                            }
                        }
                        let flags = r
                            .subrecords
                            .iter()
                            .find(|s| s.signature.as_str() == "DATA")
                            .and_then(|s| flags_u16(&s.data))
                            .unwrap_or(0);
                        let show_sky = flags & FLAG_INTERIOR != 0 && flags & FLAG_SHOW_SKY != 0;
                        if show_sky && xccm.is_none() {
                            out.broken_cells.push(objid);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

pub fn build_sky_index(root_items: &[ParsedItem]) -> SkyIndex {
    let mut out = SkyIndex::default();
    walk(root_items, None, &mut out);
    // Deterministic region pick per weather.
    for regns in out.weather_to_regn.values_mut() {
        regns.sort_unstable();
        regns.dedup();
    }
    out.broken_cells.sort_unstable();
    out.broken_cells.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{ParsedGroup, ParsedRecord, ParsedSubrecord};
    use smol_str::SmolStr;

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn cell(form_id: u32, flags: u16, xccm: Option<u32>) -> ParsedItem {
        let mut subs = vec![sub("DATA", flags.to_le_bytes().to_vec())];
        if let Some(r) = xccm {
            subs.push(sub("XCCM", r.to_le_bytes().to_vec()));
        }
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: subs,
            raw_payload: None,
            parse_error: None,
        })
    }

    fn regn(form_id: u32, weathers: &[u32]) -> ParsedItem {
        let mut rdwt = Vec::new();
        for w in weathers {
            rdwt.extend_from_slice(&w.to_le_bytes()); // weather
            rdwt.extend_from_slice(&50u32.to_le_bytes()); // chance
            rdwt.extend_from_slice(&0u32.to_le_bytes()); // global
        }
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("REGN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("RDWT", rdwt)],
            raw_payload: None,
            parse_error: None,
        })
    }

    fn group(group_type: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    const INTERIOR: u16 = 0x0001;
    const SHOW_SKY: u16 = 0x0080;

    #[test]
    fn show_sky_interior_without_xccm_is_broken() {
        let tree = vec![group(
            0,
            *b"CELL",
            vec![group(
                2,
                0i32.to_le_bytes(),
                vec![group(
                    3,
                    9i32.to_le_bytes(),
                    vec![
                        // broken: interior + show sky, no XCCM
                        cell(0x01000010, INTERIOR | SHOW_SKY, None),
                        // fine: has XCCM
                        cell(0x01000011, INTERIOR | SHOW_SKY, Some(0x0020CFF8)),
                        // not show-sky: ignored
                        cell(0x01000012, INTERIOR, None),
                    ],
                )],
            )],
        )];
        let idx = build_sky_index(&tree);
        assert_eq!(idx.broken_cells, vec![0x000010]);
        // The XCCM-bearing cell feeds the default histogram.
        assert_eq!(idx.xccm_counts.get(&0x20CFF8).copied(), Some(1));
    }

    #[test]
    fn exterior_show_sky_cell_is_not_broken() {
        let world: u32 = 0x25DA15;
        let tree = vec![group(
            0,
            *b"WRLD",
            vec![group(
                WORLD_CHILD_GROUP,
                world.to_le_bytes(),
                // Exterior cell under a world-children group is never "interior".
                vec![cell(0x0100ABCD, INTERIOR | SHOW_SKY, None)],
            )],
        )];
        let idx = build_sky_index(&tree);
        assert!(idx.broken_cells.is_empty());
    }

    #[test]
    fn weather_to_regn_maps_each_weather() {
        let tree = vec![group(
            0,
            *b"REGN",
            vec![
                regn(0x0020CFF8, &[0x3A1A9F, 0x2BA027]),
                regn(0x0020D000, &[0x3A1A9F]),
            ],
        )];
        let idx = build_sky_index(&tree);
        assert_eq!(
            idx.weather_to_regn.get(&0x3A1A9F).cloned(),
            Some(vec![0x20CFF8, 0x20D000])
        );
        assert_eq!(
            idx.weather_to_regn.get(&0x2BA027).cloned(),
            Some(vec![0x20CFF8])
        );
    }

    #[test]
    fn default_region_is_mode_of_existing_xccm() {
        let tree = vec![group(
            0,
            *b"CELL",
            vec![group(
                2,
                0i32.to_le_bytes(),
                vec![group(
                    3,
                    9i32.to_le_bytes(),
                    vec![
                        cell(0x01000001, INTERIOR | SHOW_SKY, Some(0x00AAAA)),
                        cell(0x01000002, INTERIOR | SHOW_SKY, Some(0x00AAAA)),
                        cell(0x01000003, INTERIOR | SHOW_SKY, Some(0x00BBBB)),
                    ],
                )],
            )],
        )];
        let idx = build_sky_index(&tree);
        assert_eq!(idx.default_region(), Some(0xAAAA));
    }

    #[test]
    fn default_region_none_when_no_xccm() {
        let idx = build_sky_index(&[]);
        assert_eq!(idx.default_region(), None);
    }
}
