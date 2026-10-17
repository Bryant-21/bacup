use std::collections::BTreeMap;

use serde::Serialize;

use super::{AfHeader, af_header};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SuffixNode {
    pub hash: u32,
    pub hash_name: String,
    pub node_name: String,
}

#[derive(Debug, PartialEq, Eq)]
struct RawBlock {
    counts: [u16; 6],
    fields: [Vec<u8>; 10],
}

pub(super) struct SuffixView {
    header: [u8; 64],
    pub source_header: AfHeader,
    pub hashes: Vec<u32>,
    blocks: Vec<RawBlock>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl Cursor<'_> {
    fn take(&mut self, alignment: usize, count: usize) -> Result<&[u8], String> {
        let offset = self
            .offset
            .checked_add(alignment - 1)
            .ok_or("Starfield AF offset overflow")?
            & !(alignment - 1);
        let end = offset
            .checked_add(count)
            .ok_or("Starfield AF size overflow")?;
        let value = self
            .bytes
            .get(offset..end)
            .ok_or("Starfield AF blocks are truncated")?;
        self.offset = end;
        Ok(value)
    }

    fn block(&mut self, flags: u8) -> Result<RawBlock, String> {
        let counter_width = if flags & 2 != 0 { 2 } else { 1 };
        let key_width = if flags & 4 != 0 { 2 } else { 1 };
        let mut counts = [0; 6];
        for count in &mut counts {
            let value = self.take(counter_width, counter_width)?;
            *count = if counter_width == 2 {
                u16::from_le_bytes(value.try_into().unwrap())
            } else {
                u16::from(value[0])
            };
        }
        let sizes = [key_width, key_width, key_width, key_width, 3, 4, 3, 8, 2, 1];
        let alignments = [key_width, key_width, key_width, key_width, 1, 1, 1, 2, 2, 1];
        let lengths = [
            counts[0], counts[2], counts[4], counts[5], counts[0], counts[1], counts[2], counts[3],
            counts[4], counts[5],
        ];
        let mut fields: [Vec<u8>; 10] = std::array::from_fn(|_| Vec::new());
        for index in 0..fields.len() {
            // Empty arrays do not align CALUMI's stream cursor.
            if lengths[index] != 0 {
                fields[index] = self
                    .take(
                        alignments[index],
                        usize::from(lengths[index]) * sizes[index],
                    )?
                    .to_vec();
            }
        }
        // CALUMI unfolds these runs, then indexes one prefix per transform key.
        let rotations: usize = fields[5]
            .chunks_exact(4)
            .map(|prefix| usize::from(prefix[3] & 0x3F) + 1)
            .sum();
        let translations: usize = fields[7]
            .chunks_exact(8)
            .map(|prefix| usize::from(u16::from_le_bytes(prefix[6..8].try_into().unwrap())))
            .sum();
        if rotations != usize::from(counts[0]) || translations != usize::from(counts[2]) {
            return Err("Starfield AF transform prefix runs differ from their key counts".into());
        }
        Ok(RawBlock { counts, fields })
    }
}

pub(super) fn inspect_suffix(bytes: &[u8]) -> Result<SuffixView, String> {
    let source_header = af_header(bytes)?;
    let offset = 64 + usize::from(source_header.preamble_offset);
    if offset % 4 != 0 {
        return Err("Starfield AF preamble does not end on its declared word boundary".into());
    }
    let mut cursor = Cursor { bytes, offset };
    let mut hashes = Vec::with_capacity(usize::from(source_header.fill_count));
    for _ in 0..source_header.fill_count {
        hashes.push(u32::from_le_bytes(cursor.take(4, 4)?.try_into().unwrap()));
    }
    cursor.take(4, 0)?;
    let atlas = cursor
        .take(1, usize::from(source_header.index_atlas_count))?
        .to_vec();
    let total: usize = atlas.iter().map(|value| usize::from(*value)).sum();
    if total != usize::from(source_header.bone_count) {
        return Err("Starfield AF atlas differs from the declared actor bone count".into());
    }
    for (index, count) in atlas.into_iter().enumerate() {
        if index % 2 != 0 {
            for _ in 0..count {
                cursor.block(source_header.flags[0])?;
            }
        }
    }
    let blocks = (0..source_header.fill_count)
        .map(|_| cursor.block(source_header.flags[0]))
        .collect::<Result<Vec<_>, _>>()?;
    if cursor.offset != bytes.len() {
        return Err(format!(
            "Starfield AF has {} unparsed bytes after its suffix blocks",
            bytes.len().saturating_sub(cursor.offset)
        ));
    }
    Ok(SuffixView {
        header: bytes[..64].try_into().unwrap(),
        source_header,
        hashes,
        blocks,
    })
}

pub(super) fn resolve_nodes(hashes: &[u32], names: &[String]) -> Result<Vec<SuffixNode>, String> {
    let mut candidates: BTreeMap<u32, Vec<(String, String)>> = BTreeMap::new();
    for name in names {
        let mut spellings = vec![name.clone()];
        // Shipped Eon hashes "Trigger" while its receiver node is "TRIGGER".
        if name.eq_ignore_ascii_case("Trigger") {
            spellings.push("Trigger".into());
        }
        spellings.sort();
        spellings.dedup();
        for spelling in spellings {
            let hash = materials_native::cdb::bethesda_crc32(spelling.as_bytes());
            candidates
                .entry(hash)
                .or_default()
                .push((spelling, name.clone()));
        }
    }
    hashes
        .iter()
        .map(|hash| {
            let entries = candidates.get(hash).ok_or_else(|| {
                format!("Starfield weapon suffix node {hash:08X} is absent from its model closure")
            })?;
            let [(hash_name, node_name)] = entries.as_slice() else {
                return Err(format!(
                    "Starfield weapon suffix node {hash:08X} has ambiguous model names"
                ));
            };
            Ok(SuffixNode {
                hash: *hash,
                hash_name: hash_name.clone(),
                node_name: node_name.clone(),
            })
        })
        .collect()
}

fn append_aligned(output: &mut Vec<u8>, alignment: usize, value: &[u8]) {
    if value.is_empty() {
        return;
    }
    output.resize((output.len() + alignment - 1) & !(alignment - 1), 0);
    output.extend_from_slice(value);
}

impl SuffixView {
    pub fn animation_view(&self) -> Result<Vec<u8>, String> {
        let count = u8::try_from(self.blocks.len())
            .map_err(|_| "Starfield suffix view exceeds the byte atlas limit")?;
        let mut output = self.header.to_vec();
        output[42..44].copy_from_slice(&u16::from(count).to_le_bytes());
        output[46..48].copy_from_slice(&2u16.to_le_bytes());
        output[48..52].fill(0);
        output.extend_from_slice(&[0, count]);
        let counter_width = if self.source_header.flags[0] & 2 != 0 {
            2
        } else {
            1
        };
        let key_width = if self.source_header.flags[0] & 4 != 0 {
            2
        } else {
            1
        };
        let alignments = [key_width, key_width, key_width, key_width, 1, 1, 1, 2, 2, 1];
        for block in &self.blocks {
            for count in block.counts {
                append_aligned(
                    &mut output,
                    counter_width,
                    &count.to_le_bytes()[..counter_width],
                );
            }
            for (index, field) in block.fields.iter().enumerate() {
                append_aligned(&mut output, alignments[index], field);
            }
        }
        Ok(output)
    }
}

pub(super) fn rig_view(source: &[u8], nodes: &[SuffixNode]) -> Result<Vec<u8>, String> {
    if source.len() < 80 || source[..4] != 5u32.to_le_bytes() {
        return Err("Starfield suffix view requires a version5 source RIG".into());
    }
    let count = u16::try_from(nodes.len()).map_err(|_| "Starfield suffix rig count overflow")?;
    let map_offset = 80 + 96 * nodes.len();
    let mut output = source[..80].to_vec();
    output[16..20].copy_from_slice(&(map_offset as u32).to_le_bytes());
    output[56..58].copy_from_slice(&count.to_le_bytes());
    output[58..60].copy_from_slice(&count.to_le_bytes());
    let mut name_offset = map_offset + 314;
    for (index, node) in nodes.iter().enumerate() {
        if node.node_name.contains('\0') {
            return Err("Starfield suffix node contains a NUL".into());
        }
        let mut bone = [0u8; 96];
        bone[..4].copy_from_slice(&1f32.to_le_bytes());
        bone[16..20].copy_from_slice(&1f32.to_le_bytes());
        bone[44..48].copy_from_slice(&(-1i32).to_le_bytes());
        bone[48..56].copy_from_slice(&(name_offset as u64).to_le_bytes());
        // CALUMI requires one root even for this identity-pose decoding view.
        bone[56..60].copy_from_slice(&(if index == 0 { -1i32 } else { 0 }).to_le_bytes());
        for offset in [60, 64, 68] {
            bone[offset..offset + 4].copy_from_slice(&(-1i32).to_le_bytes());
        }
        bone[72..76].copy_from_slice(&(index as i32).to_le_bytes());
        output.extend_from_slice(&bone);
        name_offset += node.node_name.len() + 1;
    }
    output.extend_from_slice(&[0xFF; 314]);
    for node in nodes {
        output.extend_from_slice(node.node_name.as_bytes());
        output.push(0);
    }
    let size = u32::try_from(output.len()).map_err(|_| "Starfield suffix rig size overflow")?;
    output[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_crc_resolution_preserves_actual_trigger_node_and_rejects_missing_or_ambiguous() {
        let nodes = resolve_nodes(
            &[0xABD892DB, 0x48BAE9BF],
            &["Magazine".into(), "TRIGGER".into()],
        )
        .unwrap();
        assert_eq!(nodes[1].hash_name, "Trigger");
        assert_eq!(nodes[1].node_name, "TRIGGER");
        assert!(resolve_nodes(&[0xABD892DB], &[]).is_err());
        assert!(resolve_nodes(&[0x48BAE9BF], &["Trigger".into(), "TRIGGER".into()]).is_err());
    }

    #[test]
    fn suffix_view_relocates_compressed_fields_without_changing_them() {
        let mut header = [0u8; 64];
        header[40] = 5;
        header[44..46].copy_from_slice(&101u16.to_le_bytes());
        header[36] = 6;
        let mut block = RawBlock {
            counts: [1; 6],
            fields: std::array::from_fn(|index| {
                vec![index as u8; [2, 2, 2, 2, 3, 4, 3, 8, 2, 1][index]]
            }),
        };
        block.fields[5][3] = 0;
        block.fields[7][6..8].copy_from_slice(&1u16.to_le_bytes());
        let view = SuffixView {
            source_header: af_header(&header).unwrap(),
            header,
            hashes: vec![0xABD892DB],
            blocks: vec![block],
        };
        let repacked = view.animation_view().unwrap();
        let parsed = inspect_suffix(&repacked).unwrap();
        assert_eq!(parsed.source_header.frame_count, 101);
        assert_eq!(parsed.source_header.bone_count, 1);
        let mut cursor = Cursor {
            bytes: &repacked,
            offset: 66,
        };
        assert_eq!(cursor.block(6).unwrap(), view.blocks[0]);
        assert_eq!(cursor.offset, repacked.len());
        assert!(inspect_suffix(&repacked[..repacked.len() - 1]).is_err());
        let mut invalid = repacked.clone();
        invalid[65] = 2;
        assert!(inspect_suffix(&invalid).err().unwrap().contains("atlas"));
        let mut trailing = repacked;
        trailing.push(0);
        assert!(inspect_suffix(&trailing).is_err());
    }

    #[test]
    fn inconsistent_transform_prefix_runs_are_rejected_before_native_decode() {
        let mut header = [0u8; 64];
        header[40] = 5;
        let mut view = SuffixView {
            source_header: af_header(&header).unwrap(),
            header,
            hashes: vec![0xABD892DB],
            blocks: vec![RawBlock {
                counts: [1, 0, 0, 0, 0, 0],
                fields: std::array::from_fn(|index| match index {
                    0 => vec![0],
                    4 => vec![0; 3],
                    _ => vec![],
                }),
            }],
        };
        assert!(
            inspect_suffix(&view.animation_view().unwrap())
                .err()
                .unwrap()
                .contains("prefix")
        );
        view.blocks[0].counts = [0, 0, 1, 1, 0, 0];
        view.blocks[0].fields = std::array::from_fn(|index| match index {
            1 => vec![0],
            6 => vec![0; 3],
            7 => vec![0; 8],
            _ => vec![],
        });
        assert!(
            inspect_suffix(&view.animation_view().unwrap())
                .err()
                .unwrap()
                .contains("prefix")
        );
        view.blocks[0].fields[7][6..8].copy_from_slice(&2u16.to_le_bytes());
        assert!(
            inspect_suffix(&view.animation_view().unwrap())
                .err()
                .unwrap()
                .contains("prefix")
        );
    }

    #[test]
    #[ignore = "requires extracted source Eon AF files"]
    fn real_eon_suffix_views_retain_all_compressed_fields_and_authored_duration() {
        let root = std::path::PathBuf::from(std::env::var("B21_STARFIELD_MESHES").expect("B21_STARFIELD_MESHES"));
        for path in [
            "actors/human/_1stperson/animations/gun/grippistol/stinger/gun_firesingle.af",
            "actors/human/_1stperson/animations/gun/grippistol/stinger/firesingle.af",
            "actors/human/_1stperson/animations/gun/grippistol/stinger/reload.af",
            "actors/human/animations/gun/grippistol/stinger/player/reload.af",
        ] {
            let source = inspect_suffix(&std::fs::read(root.join(path)).unwrap()).unwrap();
            assert_eq!(source.blocks.len(), 8);
            let repacked = inspect_suffix(&source.animation_view().unwrap()).unwrap();
            assert_eq!(repacked.source_header.bone_count, 8);
            assert_eq!(
                repacked.source_header.frame_count,
                source.source_header.frame_count
            );
            assert!(repacked.hashes.is_empty());
        }
    }
}
