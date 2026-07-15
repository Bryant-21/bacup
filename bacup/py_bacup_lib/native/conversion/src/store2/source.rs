//! Mmap-backed source plugin store.
//!
//! One forward walk at open() builds `index` (file order). Record payload
//! bytes stay in the mmap (file-backed pages — near-zero private RSS);
//! compressed payloads are inflated transiently per `to_parsed_record()`
//! call and dropped by the caller after decode.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use bytes::Bytes;
use memmap2::Mmap;
use smol_str::SmolStr;

use esp_authoring_core::plugin_runtime::{ParsedRecord, decode_compressed_subrecords_from_payload};

pub const COMPRESSED_RECORD_FLAG: u32 = 0x0004_0000;
const RECORD_HEADER_LEN: usize = 24;

#[derive(Debug)]
pub enum SourceOpenError {
    Io(String),
    Malformed(String),
}

impl std::fmt::Display for SourceOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "source esm io: {m}"),
            Self::Malformed(m) => write!(f, "source esm malformed: {m}"),
        }
    }
}

impl std::error::Error for SourceOpenError {}

/// One top-level-walk record entry, in file order.
#[derive(Debug, Clone, Copy)]
pub struct RecordIndexEntry2 {
    pub sig: [u8; 4],
    pub form_id: u32,
    pub flags: u32,
    /// Offset of the 24-byte record header in the file.
    pub header_offset: usize,
    /// Length of the on-disk payload (compressed length when compressed).
    pub data_len: usize,
}

pub struct SourceEsm {
    mmap: Mmap,
    /// Every record in the plugin, file order (groups flattened).
    pub(crate) index: Vec<RecordIndexEntry2>,
    /// form_id -> position in `index`.
    by_form_id: HashMap<u32, usize>,
    /// sig -> positions in `index`, file order.
    by_sig: HashMap<[u8; 4], Vec<usize>>,
    pub plugin_name: String,
}

pub struct RecordView<'a> {
    esm: &'a SourceEsm,
    entry: &'a RecordIndexEntry2,
}

impl SourceEsm {
    pub fn open(path: &Path) -> Result<Self, SourceOpenError> {
        let file = File::open(path).map_err(|e| SourceOpenError::Io(e.to_string()))?;
        let mmap = unsafe { Mmap::map(&file) }.map_err(|e| SourceOpenError::Io(e.to_string()))?;
        let plugin_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut esm = SourceEsm {
            mmap,
            index: Vec::new(),
            by_form_id: HashMap::new(),
            by_sig: HashMap::new(),
            plugin_name,
        };
        esm.walk()?;
        Ok(esm)
    }

    fn walk(&mut self) -> Result<(), SourceOpenError> {
        let data: &[u8] = &self.mmap;
        if data.len() < RECORD_HEADER_LEN || &data[0..4] != b"TES4" {
            return Err(SourceOpenError::Malformed("no TES4 header".into()));
        }
        let tes4_len = read_u32(data, 4)? as usize;
        let mut offset = RECORD_HEADER_LEN + tes4_len;
        while offset + RECORD_HEADER_LEN <= data.len() {
            let sig: [u8; 4] = data[offset..offset + 4].try_into().unwrap();
            let size = read_u32(data, offset + 4)? as usize;
            if &sig == b"GRUP" {
                // GRUP size includes its own 24-byte header; descend by simply
                // continuing the linear walk (children follow the header).
                if size < RECORD_HEADER_LEN {
                    return Err(SourceOpenError::Malformed(format!(
                        "GRUP size {size} < header at {offset}"
                    )));
                }
                offset += RECORD_HEADER_LEN;
                continue;
            }
            let flags = read_u32(data, offset + 8)?;
            let form_id = read_u32(data, offset + 12)?;
            let pos = self.index.len();
            self.index.push(RecordIndexEntry2 {
                sig,
                form_id,
                flags,
                header_offset: offset,
                data_len: size,
            });
            self.by_form_id.entry(form_id).or_insert(pos);
            self.by_sig.entry(sig).or_default().push(pos);
            offset += RECORD_HEADER_LEN + size;
        }
        Ok(())
    }

    pub fn record_count(&self) -> usize {
        self.index.len()
    }

    pub fn signatures_sorted(&self) -> Vec<[u8; 4]> {
        let mut sigs: Vec<[u8; 4]> = self.by_sig.keys().copied().collect();
        sigs.sort_unstable();
        sigs
    }

    /// Local object-ids (full raw form_ids) of `sig`, file order — mirrors
    /// the legacy `iter_form_keys_of_sig` ordering contract.
    pub fn form_ids_of_sig(&self, sig: [u8; 4]) -> impl Iterator<Item = u32> + '_ {
        self.by_sig
            .get(&sig)
            .into_iter()
            .flatten()
            .map(|&pos| self.index[pos].form_id)
    }

    pub fn positions_of_sig(&self, sig: [u8; 4]) -> impl Iterator<Item = usize> + '_ {
        self.by_sig.get(&sig).into_iter().flatten().copied()
    }

    pub fn view_by_form_id(&self, form_id: u32) -> Option<RecordView<'_>> {
        let pos = *self.by_form_id.get(&form_id)?;
        Some(RecordView {
            esm: self,
            entry: &self.index[pos],
        })
    }

    /// View by index position (file order). Used by translate_v2's chunked
    /// enumeration, which carries the position alongside the FormKey.
    pub fn view_at(&self, pos: usize) -> Option<RecordView<'_>> {
        self.index
            .get(pos)
            .map(|entry| RecordView { esm: self, entry })
    }

    /// Enumerate `(sig, index-position)` in the legacy translate order: sorted
    /// signatures (matches `source_read::source_signatures`), then each sig's
    /// records in file order (matches `iter_form_keys_of_sig`). `records_limit`
    /// applies the same per-sig-batch cap as `ConversionRun::translate_all`
    /// (the limit is checked before each sig batch, then the batch is truncated
    /// to the remaining budget). Returns positions so the caller can build the
    /// matching `FormKey`s (with the source master-index mapping) and fetch the
    /// `RecordView` for each.
    pub fn enumerate_positions_sorted_sig(
        &self,
        records_limit: Option<usize>,
    ) -> Vec<([u8; 4], usize)> {
        let mut out = Vec::new();
        for sig in self.signatures_sorted() {
            if let Some(limit) = records_limit {
                if out.len() >= limit {
                    break;
                }
            }
            let Some(positions) = self.by_sig.get(&sig) else {
                continue;
            };
            if let Some(limit) = records_limit {
                let remaining = limit.saturating_sub(out.len());
                for &pos in positions.iter().take(remaining) {
                    out.push((sig, pos));
                }
            } else {
                for &pos in positions {
                    out.push((sig, pos));
                }
            }
        }
        out
    }
}

impl<'a> RecordView<'a> {
    pub fn sig(&self) -> [u8; 4] {
        self.entry.sig
    }

    pub fn flags(&self) -> u32 {
        self.entry.flags
    }

    pub fn form_id(&self) -> u32 {
        self.entry.form_id
    }

    fn raw_payload(&self) -> &'a [u8] {
        let start = self.entry.header_offset + RECORD_HEADER_LEN;
        &self.esm.mmap[start..start + self.entry.data_len]
    }

    fn header_field_u16(&self, byte: usize) -> u16 {
        let d: &[u8] = &self.esm.mmap;
        let o = self.entry.header_offset + byte;
        u16::from_le_bytes([d[o], d[o + 1]])
    }

    fn header_field_u32(&self, byte: usize) -> u32 {
        let d: &[u8] = &self.esm.mmap;
        let o = self.entry.header_offset + byte;
        u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
    }

    /// Materialize a transient `ParsedRecord` (decompress + split) with the
    /// same field semantics as the esp loader. Caller drops it after decode.
    pub fn to_parsed_record(&self) -> Result<ParsedRecord, String> {
        let raw = self.raw_payload();
        let (subrecords, raw_payload) = if self.entry.flags & COMPRESSED_RECORD_FLAG != 0 {
            let raw_payload = Bytes::copy_from_slice(raw);
            let decoded = decode_compressed_subrecords_from_payload(&raw_payload)
                .map_err(|error| format!("inflate {:08X}: {error}", self.entry.form_id))?;
            (
                decoded.subrecords,
                (!decoded.salvaged_bad_checksum).then_some(raw_payload),
            )
        } else {
            (
                esp_authoring_core::plugin_runtime::split_record_payload(&Bytes::copy_from_slice(
                    raw,
                ))?,
                None,
            )
        };
        Ok(ParsedRecord {
            signature: SmolStr::new(String::from_utf8_lossy(&self.entry.sig)),
            form_id: self.entry.form_id,
            flags: self.entry.flags,
            version_control: self.header_field_u32(16),
            form_version: Some(self.header_field_u16(20)),
            version2: Some(self.header_field_u16(22)),
            subrecords,
            raw_payload,
            parse_error: None,
        })
    }
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, SourceOpenError> {
    if offset + 4 > data.len() {
        return Err(SourceOpenError::Malformed(format!(
            "u32 read past end at {offset}"
        )));
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

#[cfg(test)]
pub(crate) mod test_fixture {
    use std::io::Write;

    pub fn record(sig: &[u8; 4], form_id: u32, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(sig);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&form_id.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // vc
        out.extend_from_slice(&187u16.to_le_bytes()); // form_version (FO76)
        out.extend_from_slice(&0u16.to_le_bytes()); // vc2
        out.extend_from_slice(payload);
        out
    }

    pub fn subrecord(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(sig);
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    pub fn compressed_payload(decompressed: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(decompressed).unwrap();
        let z = enc.finish().unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(&(decompressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&z);
        out
    }

    pub fn group(label: &[u8; 4], group_type: i32, children: &[Vec<u8>]) -> Vec<u8> {
        let children_len: usize = children.iter().map(|c| c.len()).sum();
        let mut out = Vec::new();
        out.extend_from_slice(b"GRUP");
        out.extend_from_slice(&((children_len + 24) as u32).to_le_bytes());
        out.extend_from_slice(label);
        out.extend_from_slice(&group_type.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]); // stamp + unknown
        for c in children {
            out.extend_from_slice(c);
        }
        out
    }

    /// TES4 header + the given top-level GRUPs, as one plugin byte blob.
    pub fn plugin(masters: &[&str], groups: &[Vec<u8>]) -> Vec<u8> {
        let mut hedr = Vec::new();
        hedr.extend_from_slice(&1.0f32.to_le_bytes());
        hedr.extend_from_slice(&0u32.to_le_bytes());
        hedr.extend_from_slice(&0x800u32.to_le_bytes());
        let mut tes4_payload = subrecord(b"HEDR", &hedr);
        for m in masters {
            let mut z = m.as_bytes().to_vec();
            z.push(0);
            tes4_payload.extend_from_slice(&subrecord(b"MAST", &z));
            tes4_payload.extend_from_slice(&subrecord(b"DATA", &0u64.to_le_bytes()));
        }
        let mut out = record(b"TES4", 0, 0x0000_0001 /* ESM */, &tes4_payload);
        for g in groups {
            out.extend_from_slice(g);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixture::*;
    use super::*;

    fn write_temp_plugin(bytes: &[u8]) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::Builder::new().suffix(".esm").tempfile().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        f
    }

    fn two_sig_plugin() -> Vec<u8> {
        let weap1 = record(b"WEAP", 0x0100_0801, 0, &subrecord(b"EDID", b"WeapOne\0"));
        let weap2 = record(b"WEAP", 0x0100_0802, 0, &subrecord(b"EDID", b"WeapTwo\0"));
        let armo_payload = subrecord(b"EDID", b"ArmoOne\0");
        let armo = record(
            b"ARMO",
            0x0100_0803,
            COMPRESSED_RECORD_FLAG,
            &compressed_payload(&armo_payload),
        );
        plugin(
            &["SeventySix.esm"],
            &[
                group(b"WEAP", 0, &[weap1, weap2]),
                group(b"ARMO", 0, &[armo]),
            ],
        )
    }

    fn bad_adler_land_plugin() -> Vec<u8> {
        let mut land_payload = Vec::new();
        land_payload.extend_from_slice(&subrecord(b"DATA", &[0; 4]));
        land_payload.extend_from_slice(&subrecord(b"VNML", &vec![0; 3267]));
        land_payload.extend_from_slice(&subrecord(b"VHGT", &vec![0; 1096]));
        assert_eq!(land_payload.len(), 4385);
        let mut compressed = compressed_payload(&land_payload);
        let last = compressed.len() - 1;
        compressed[last] ^= 0x01;
        let land = record(b"LAND", 0x0015_0FC0, COMPRESSED_RECORD_FLAG, &compressed);
        plugin(&[], &[group(b"LAND", 0, &[land])])
    }

    #[test]
    fn walk_indexes_records_in_file_order() {
        let f = write_temp_plugin(&two_sig_plugin());
        let esm = SourceEsm::open(f.path()).unwrap();
        assert_eq!(esm.record_count(), 3);
        let weaps: Vec<u32> = esm.form_ids_of_sig(*b"WEAP").collect();
        assert_eq!(weaps, vec![0x0100_0801, 0x0100_0802]);
        assert_eq!(esm.signatures_sorted(), vec![*b"ARMO", *b"WEAP"]);
    }

    #[test]
    fn view_materializes_plain_record() {
        let f = write_temp_plugin(&two_sig_plugin());
        let esm = SourceEsm::open(f.path()).unwrap();
        let pr = esm
            .view_by_form_id(0x0100_0801)
            .unwrap()
            .to_parsed_record()
            .unwrap();
        assert_eq!(pr.signature.as_str(), "WEAP");
        assert_eq!(pr.subrecords.len(), 1);
        assert_eq!(pr.subrecords[0].signature.as_str(), "EDID");
        assert_eq!(pr.subrecords[0].data.as_ref(), b"WeapOne\0");
        assert_eq!(pr.form_version, Some(187));
    }

    #[test]
    fn view_decompresses_compressed_record() {
        let f = write_temp_plugin(&two_sig_plugin());
        let esm = SourceEsm::open(f.path()).unwrap();
        let pr = esm
            .view_by_form_id(0x0100_0803)
            .unwrap()
            .to_parsed_record()
            .unwrap();
        assert_eq!(pr.subrecords[0].data.as_ref(), b"ArmoOne\0");
        assert!(pr.raw_payload.is_some());
    }

    #[test]
    fn view_salvages_bad_adler_land_and_saves_canonically() {
        use esp_authoring_core::plugin_runtime::compress_subrecords_payload;

        let source = write_temp_plugin(&bad_adler_land_plugin());
        let esm = SourceEsm::open(source.path()).unwrap();
        let parsed = esm
            .view_by_form_id(0x0015_0FC0)
            .unwrap()
            .to_parsed_record()
            .expect("bad-Adler LAND must not surface as ReadFailed");
        assert!(parsed.raw_payload.is_none());
        assert_eq!(
            parsed
                .subrecords
                .iter()
                .map(|subrecord| (subrecord.signature.as_str(), subrecord.data.len()))
                .collect::<Vec<_>>(),
            vec![("DATA", 4), ("VNML", 3267), ("VHGT", 1096)]
        );

        let canonical_payload = compress_subrecords_payload(&parsed.subrecords).unwrap();
        let canonical_record = record(
            b"LAND",
            0x0015_0FC0,
            COMPRESSED_RECORD_FLAG,
            &canonical_payload,
        );
        let canonical_file =
            write_temp_plugin(&plugin(&[], &[group(b"LAND", 0, &[canonical_record])]));
        let canonical = SourceEsm::open(canonical_file.path()).unwrap();
        let reloaded = canonical
            .view_by_form_id(0x0015_0FC0)
            .unwrap()
            .to_parsed_record()
            .expect("canonical LAND reload");
        assert!(reloaded.raw_payload.is_some());
        assert_eq!(reloaded.subrecords.len(), 3);
    }

    #[test]
    fn nested_groups_flatten_in_file_order() {
        let cell = record(b"CELL", 0x0100_0900, 0, &subrecord(b"EDID", b"CellA\0"));
        let refr = record(b"REFR", 0x0100_0901, 0, &[]);
        let inner = group(b"\x00\x09\x00\x01", 9, &[refr]);
        let outer = group(b"CELL", 0, &[[cell, inner].concat()]);
        let f = write_temp_plugin(&plugin(&[], &[outer]));
        let esm = SourceEsm::open(f.path()).unwrap();
        let sigs: Vec<[u8; 4]> = (0..esm.record_count()).map(|i| esm.index[i].sig).collect();
        assert_eq!(sigs, vec![*b"CELL", *b"REFR"]);
    }

    #[test]
    fn open_rejects_non_plugin() {
        let f = write_temp_plugin(b"not a plugin at all............");
        assert!(SourceEsm::open(f.path()).is_err());
    }

    /// Load fixture bytes into a real plugin handle WITHOUT a Python
    /// interpreter (cargo tests can't start one under `extension-module`):
    /// create an empty handle, then overwrite its `parsed` tree with a full
    /// `parse_plugin_file` of the same temp file. The handle then drives the
    /// legacy `source_read` enumeration; the same file drives `SourceEsm`.
    fn load_fixture_handle(path: &std::path::Path, game: &str) -> u64 {
        use esp_authoring_core::plugin_runtime::{
            parse_plugin_file, plugin_handle_new_native, plugin_handle_store_ref,
        };
        let parsed = parse_plugin_file(&path.to_string_lossy(), Some(game.to_string()), true)
            .expect("parse fixture plugin");
        let plugin_name = parsed.plugin_name.clone();
        let handle = plugin_handle_new_native(&plugin_name, Some(game)).expect("new handle");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).expect("slot present");
            slot.parsed = parsed;
            slot.invalidate_sections();
        }
        handle
    }

    /// Fixture with sigs interleaved across groups AND non-monotonic form_ids,
    /// so file order ≠ formid order. Every record uses the plugin's own index
    /// (no masters) so `object_id == form_id & 0xFFFFFF`.
    fn interleaved_fixture() -> Vec<u8> {
        // WEAP group: 0x803 then 0x801 (descending — non-monotonic).
        let weap_hi = record(b"WEAP", 0x0000_0803, 0, &subrecord(b"EDID", b"WeapHi\0"));
        let weap_lo = record(b"WEAP", 0x0000_0801, 0, &subrecord(b"EDID", b"WeapLo\0"));
        // MISC group between/around: 0x802.
        let misc = record(b"MISC", 0x0000_0802, 0, &subrecord(b"EDID", b"MiscMid\0"));
        // KEYW group: 0x900.
        let keyw = record(b"KEYW", 0x0000_0900, 0, &subrecord(b"EDID", b"KeywA\0"));
        plugin(
            &[],
            &[
                group(b"WEAP", 0, &[weap_hi, weap_lo]),
                group(b"MISC", 0, &[misc]),
                group(b"KEYW", 0, &[keyw]),
            ],
        )
    }

    #[test]
    fn enumeration_matches_legacy_handle_core_section() {
        use crate::ids::SigCode;
        use crate::source_read::{iter_form_keys_of_sig, source_signatures};
        use crate::sym::StringInterner;

        let bytes = interleaved_fixture();
        let f = write_temp_plugin(&bytes);
        let esm = SourceEsm::open(f.path()).unwrap();
        let handle = load_fixture_handle(f.path(), "fo76");

        // Legacy enumeration: sorted sigs, per-sig file-order FormKeys.
        let mut interner = StringInterner::new();
        let legacy: Vec<([u8; 4], u32)> = {
            let sigs = source_signatures(handle, &interner).unwrap();
            let mut out = Vec::new();
            for sig in sigs {
                for fk in iter_form_keys_of_sig(handle, sig, &mut interner).unwrap() {
                    out.push((sig.0, fk.local));
                }
            }
            out
        };

        // SourceEsm enumeration: object-ids via the same (own-index) mapping.
        let v2: Vec<([u8; 4], u32)> = esm
            .enumerate_positions_sorted_sig(None)
            .into_iter()
            .map(|(sig, pos)| {
                let entry = esm.index[pos];
                (sig, entry.form_id & 0x00FF_FFFF)
            })
            .collect();

        assert_eq!(
            v2, legacy,
            "store2 enumeration must match legacy handle CoreSection order"
        );
        // Sanity: the fixture really is interleaved (file order != formid order).
        let weap = SigCode::from_str("WEAP").unwrap();
        let weaps: Vec<u32> = esm.form_ids_of_sig(weap.0).collect();
        assert_eq!(
            weaps,
            vec![0x0000_0803, 0x0000_0801],
            "WEAP file order is descending by formid"
        );

        esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle);
    }
}
