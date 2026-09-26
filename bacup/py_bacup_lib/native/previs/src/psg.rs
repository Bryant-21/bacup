//! CK shared-geometry (`.psg`) container: retained source shape buffers.

use rustc_hash::FxHashMap;

use crate::error::{PrevisError, Result};

pub const MAGIC: &[u8; 4] = b"bpsg";
pub const ROW_SIZE: usize = 20;

pub struct PsgGeometry<'a> {
    pub vertex_desc: u64,
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub offset: u32,
    pub vertex_desc: u64,
    pub vertex_count: u32,
    pub index_count: u32,
}

impl Row {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.vertex_desc.to_le_bytes());
        out.extend_from_slice(&self.vertex_count.to_le_bytes());
        out.extend_from_slice(&self.index_count.to_le_bytes());
    }

    fn decode(bytes: &[u8]) -> Row {
        Row {
            offset: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            vertex_desc: u64::from_le_bytes(bytes[4..12].try_into().unwrap()),
            vertex_count: u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            index_count: u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        }
    }

    pub fn data_size(&self) -> usize {
        self.vertex_count as usize * ((self.vertex_desc & 0xF) * 4) as usize + self.index_count as usize * 2
    }
}

/// Unique geometry rows plus the data stream their offsets index; shared by
/// the uncompressed `.psg` and the shipped `.csg`.
pub struct Layout {
    pub rows: Vec<Row>,
    pub data: Vec<u8>,
}

impl Layout {
    pub fn slice(&self, row: &Row) -> Result<&[u8]> {
        let start = row.offset as usize;
        self.data
            .get(start..start + row.data_size())
            .ok_or_else(|| PrevisError::invalid("shared geometry row exceeds its data stream"))
    }

    pub(crate) fn encode_rows(&self, out: &mut Vec<u8>) {
        for row in &self.rows {
            row.encode(out);
        }
    }

    pub(crate) fn decode_rows(bytes: &[u8], count: usize) -> Result<Vec<Row>> {
        let table = bytes
            .get(..count * ROW_SIZE)
            .ok_or_else(|| PrevisError::invalid("shared geometry row table is truncated"))?;
        Ok(table.chunks_exact(ROW_SIZE).map(Row::decode).collect())
    }
}

/// Identical buffers share one data offset; offsets follow first appearance.
pub fn deduplicated_offsets(buffers: &[&[u8]]) -> Vec<u32> {
    let mut seen: FxHashMap<&[u8], u32> = FxHashMap::default();
    let mut next = 0u32;
    buffers
        .iter()
        .map(|&buffer| {
            *seen.entry(buffer).or_insert_with(|| {
                let offset = next;
                next += buffer.len() as u32;
                offset
            })
        })
        .collect()
}

pub fn layout(entries: &[PsgGeometry]) -> Layout {
    let buffers: Vec<&[u8]> = entries.iter().map(|e| e.data).collect();
    let offsets = deduplicated_offsets(&buffers);
    let mut layout = Layout {
        rows: Vec::new(),
        data: Vec::new(),
    };
    for (entry, &offset) in entries.iter().zip(&offsets) {
        // Offsets follow first appearance, so an earlier one is already laid out.
        if (offset as usize) < layout.data.len() {
            continue;
        }
        layout.rows.push(Row {
            offset,
            vertex_desc: entry.vertex_desc,
            vertex_count: entry.vertex_count,
            index_count: entry.triangle_count * 3,
        });
        layout.data.extend_from_slice(entry.data);
    }
    layout
}

pub fn build(entries: &[PsgGeometry]) -> Vec<u8> {
    encode(&layout(entries))
}

pub fn encode(layout: &Layout) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + layout.rows.len() * ROW_SIZE + layout.data.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(layout.rows.len() as u32).to_le_bytes());
    layout.encode_rows(&mut out);
    out.extend_from_slice(&layout.data);
    out
}

pub fn parse(bytes: &[u8]) -> Result<Layout> {
    if bytes.get(..4) != Some(MAGIC.as_slice()) {
        return Err(PrevisError::invalid("PSG magic is missing"));
    }
    let count = u32::from_le_bytes(
        bytes
            .get(4..8)
            .ok_or_else(|| PrevisError::invalid("PSG is truncated"))?
            .try_into()
            .unwrap(),
    ) as usize;
    let rows = Layout::decode_rows(&bytes[8..], count)?;
    Ok(Layout {
        rows,
        data: bytes[8 + count * ROW_SIZE..].to_vec(),
    })
}
