//! Minimal raw FO4 plugin reader: record index, WRLD/CELL topology, subrecords.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use memmap2::Mmap;
use rustc_hash::FxHashMap;

use crate::error::{PrevisError, Result};

pub const HEADER_SIZE: usize = 24;
pub const COMPRESSED_FLAG: u32 = 0x0004_0000;

const GROUP_WORLD_CHILDREN: i32 = 1;
const GROUP_EXTERIOR_BLOCK: i32 = 4;
const GROUP_EXTERIOR_SUB_BLOCK: i32 = 5;
const GROUP_CELL_CHILDREN: i32 = 6;
const GROUP_CELL_PERSISTENT: i32 = 8;
const GROUP_CELL_TEMPORARY: i32 = 9;

#[derive(Clone, Copy, Debug)]
pub struct RecordRef {
    pub signature: [u8; 4],
    pub offset: usize,
    pub form_id: u32,
    pub flags: u32,
}

#[derive(Clone, Debug, Default)]
pub struct CellTopology {
    pub world: Option<u32>,
    pub temporary: Vec<RecordRef>,
    pub persistent: Vec<RecordRef>,
}

pub struct Plugin {
    pub name: String,
    pub path: PathBuf,
    data: Mmap,
    pub masters: Vec<String>,
    records: FxHashMap<u32, RecordRef>,
    record_order: Vec<u32>,
    pub cells: FxHashMap<u32, CellTopology>,
    /// World FormID to its exterior-block CELL FormIDs in file order; the
    /// world's persistent CELL is excluded.
    pub world_cells: FxHashMap<u32, Vec<u32>>,
    grid_index: OnceLock<FxHashMap<u32, FxHashMap<(i32, i32), Vec<u32>>>>,
}

pub struct Subrecords<'a> {
    data: std::borrow::Cow<'a, [u8]>,
}

impl<'a> Subrecords<'a> {
    pub fn iter(&self) -> SubrecordIter<'_> {
        SubrecordIter {
            data: &self.data,
            offset: 0,
            pending_size: None,
        }
    }

    pub fn first(&self, signature: &[u8; 4]) -> Option<&[u8]> {
        self.iter().find(|(s, _)| s == signature).map(|(_, d)| d)
    }

    pub fn raw(&self) -> &[u8] {
        &self.data
    }
}

pub struct SubrecordIter<'a> {
    data: &'a [u8],
    offset: usize,
    pending_size: Option<usize>,
}

impl<'a> SubrecordIter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            offset: 0,
            pending_size: None,
        }
    }
}

impl<'a> Iterator for SubrecordIter<'a> {
    type Item = ([u8; 4], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.offset + 6 > self.data.len() {
                return None;
            }
            let signature: [u8; 4] = self.data[self.offset..self.offset + 4].try_into().unwrap();
            let short_size = u16::from_le_bytes([self.data[self.offset + 4], self.data[self.offset + 5]]) as usize;
            let start = self.offset + 6;
            if &signature == b"XXXX" {
                self.pending_size = Some(u32::from_le_bytes(self.data[start..start + 4].try_into().ok()?) as usize);
                self.offset = start + short_size;
                continue;
            }
            let size = self.pending_size.take().unwrap_or(short_size);
            let end = (start + size).min(self.data.len());
            self.offset = end;
            return Some((signature, &self.data[start..end]));
        }
    }
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

impl Plugin {
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| PrevisError::io(path.display().to_string(), e))?;
        // SAFETY: the plugin is treated as read-only input for this process.
        let data = unsafe { Mmap::map(&file) }.map_err(|e| PrevisError::io(path.display().to_string(), e))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| PrevisError::invalid(format!("plugin path has no filename: {}", path.display())))?;
        let mut plugin = Plugin {
            name,
            path: path.to_path_buf(),
            data,
            masters: Vec::new(),
            records: FxHashMap::default(),
            record_order: Vec::new(),
            cells: FxHashMap::default(),
            world_cells: FxHashMap::default(),
            grid_index: OnceLock::new(),
        };
        plugin.index()?;
        Ok(plugin)
    }

    fn index(&mut self) -> Result<()> {
        let data: &[u8] = &self.data;
        if data.len() < HEADER_SIZE || &data[..4] != b"TES4" {
            return Err(PrevisError::invalid(format!("{} is not a TES4 plugin", self.name)));
        }
        let header_size = HEADER_SIZE + u32_at(data, 4) as usize;
        let header = RecordRef {
            signature: *b"TES4",
            offset: 0,
            form_id: 0,
            flags: u32_at(data, 8),
        };
        let masters = self
            .subrecords_at(&header)?
            .iter()
            .filter(|(s, _)| s == b"MAST")
            .map(|(_, d)| String::from_utf8_lossy(d).trim_end_matches('\0').to_string())
            .collect();
        let mut walker = Walker::default();
        walker.walk(data, header_size, data.len(), &Context::default())?;
        self.masters = masters;
        self.records = walker.records;
        self.record_order = walker.record_order;
        self.cells = walker.cells;
        self.world_cells = walker.world_cells;
        Ok(())
    }

    /// Exterior CELLs of `world` at a grid coordinate, in plugin order.
    pub fn cells_at(&self, world: u32, x: i32, y: i32) -> &[u32] {
        let index = self.grid_index.get_or_init(|| {
            let mut index: FxHashMap<u32, FxHashMap<(i32, i32), Vec<u32>>> = FxHashMap::default();
            for (&world, cells) in &self.world_cells {
                let grid = index.entry(world).or_default();
                for &cell in cells {
                    let Some(record) = self.record(cell) else { continue };
                    let Ok(subrecords) = self.subrecords_at(&record) else { continue };
                    let Some(xclc) = subrecords.first(b"XCLC").filter(|d| d.len() >= 8) else {
                        continue;
                    };
                    let key = (read_i32(xclc, 0).unwrap(), read_i32(xclc, 4).unwrap());
                    grid.entry(key).or_default().push(cell);
                }
            }
            index
        });
        index
            .get(&world)
            .and_then(|grid| grid.get(&(x, y)))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn record(&self, form_id: u32) -> Option<RecordRef> {
        self.records.get(&form_id).copied()
    }

    pub fn records_of<'a>(&'a self, signature: &'a [u8; 4]) -> impl Iterator<Item = RecordRef> + 'a {
        self.record_order
            .iter()
            .filter_map(|id| self.records.get(id).copied())
            .filter(move |r| &r.signature == signature)
    }

    pub fn subrecords_at(&self, record: &RecordRef) -> Result<Subrecords<'_>> {
        let data: &[u8] = &self.data;
        let size = u32_at(data, record.offset + 4) as usize;
        let payload = &data[record.offset + HEADER_SIZE..record.offset + HEADER_SIZE + size];
        if record.flags & COMPRESSED_FLAG == 0 {
            return Ok(Subrecords {
                data: std::borrow::Cow::Borrowed(payload),
            });
        }
        if payload.len() < 4 {
            return Err(PrevisError::invalid(format!("compressed record {:08X} is truncated", record.form_id)));
        }
        let expected = u32_at(payload, 0) as usize;
        let mut decoded = Vec::with_capacity(expected);
        flate2::read::ZlibDecoder::new(&payload[4..])
            .read_to_end(&mut decoded)
            .map_err(|e| PrevisError::io(format!("decompress {:08X}", record.form_id), e))?;
        if decoded.len() != expected {
            return Err(PrevisError::invalid(format!(
                "record {:08X} decompressed to {} bytes, expected {expected}",
                record.form_id,
                decoded.len()
            )));
        }
        Ok(Subrecords {
            data: std::borrow::Cow::Owned(decoded),
        })
    }

    pub fn raw_record(&self, record: &RecordRef) -> &[u8] {
        let size = u32_at(&self.data, record.offset + 4) as usize;
        &self.data[record.offset..record.offset + HEADER_SIZE + size]
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Plugin-local master index byte for records defined by this file.
    pub fn self_index(&self) -> u32 {
        self.masters.len() as u32
    }

    /// Owner filename and object ID for a raw FormID read from this plugin.
    pub fn owner_of(&self, raw: u32) -> (&str, u32) {
        let index = (raw >> 24) as usize;
        let owner = self.masters.get(index).map_or(self.name.as_str(), String::as_str);
        (owner, raw & 0x00FF_FFFF)
    }

    /// Raw FormID this plugin would use for `(owner, object_id)`, if visible.
    pub fn raw_for(&self, owner: &str, object_id: u32) -> Option<u32> {
        if owner.eq_ignore_ascii_case(&self.name) {
            return Some((self.self_index() << 24) | object_id);
        }
        self.masters
            .iter()
            .position(|m| m.eq_ignore_ascii_case(owner))
            .map(|index| ((index as u32) << 24) | object_id)
    }
}

#[derive(Default, Clone)]
struct Context {
    world: Option<u32>,
    exterior_block: bool,
    cell: Option<u32>,
    cell_group: Option<i32>,
}

#[derive(Default)]
struct Walker {
    records: FxHashMap<u32, RecordRef>,
    record_order: Vec<u32>,
    cells: FxHashMap<u32, CellTopology>,
    world_cells: FxHashMap<u32, Vec<u32>>,
}

impl Walker {
    fn walk(&mut self, data: &[u8], start: usize, end: usize, context: &Context) -> Result<()> {
        let mut cursor = start;
        while cursor < end {
            if cursor + HEADER_SIZE > end {
                return Err(PrevisError::invalid(format!("truncated header at {cursor:#x}")));
            }
            let signature: [u8; 4] = data[cursor..cursor + 4].try_into().unwrap();
            let size = u32_at(data, cursor + 4) as usize;
            if &signature == b"GRUP" {
                if size < HEADER_SIZE || cursor + size > end {
                    return Err(PrevisError::invalid(format!("invalid GRUP size at {cursor:#x}")));
                }
                let label = u32_at(data, cursor + 8);
                let group_type = u32_at(data, cursor + 12) as i32;
                let mut child = context.clone();
                match group_type {
                    GROUP_WORLD_CHILDREN => {
                        child.world = Some(label);
                        child.cell = None;
                        child.exterior_block = false;
                    }
                    GROUP_EXTERIOR_BLOCK | GROUP_EXTERIOR_SUB_BLOCK => child.exterior_block = true,
                    GROUP_CELL_CHILDREN => {
                        child.cell = Some(label);
                        child.cell_group = None;
                    }
                    GROUP_CELL_PERSISTENT | GROUP_CELL_TEMPORARY => {
                        child.cell = Some(label);
                        child.cell_group = Some(group_type);
                    }
                    _ => {}
                }
                self.walk(data, cursor + HEADER_SIZE, cursor + size, &child)?;
                cursor += size;
                continue;
            }
            let total = HEADER_SIZE + size;
            if cursor + total > end {
                return Err(PrevisError::invalid(format!("record at {cursor:#x} exceeds its group")));
            }
            let record = RecordRef {
                signature,
                offset: cursor,
                form_id: u32_at(data, cursor + 12),
                flags: u32_at(data, cursor + 8),
            };
            if self.records.insert(record.form_id, record).is_none() {
                self.record_order.push(record.form_id);
            }
            if &signature == b"CELL" {
                let topology = self.cells.entry(record.form_id).or_default();
                topology.world = context.world;
                if let (Some(world), true) = (context.world, context.exterior_block) {
                    self.world_cells.entry(world).or_default().push(record.form_id);
                }
            } else if let (Some(cell), Some(group)) = (context.cell, context.cell_group) {
                let topology = self.cells.entry(cell).or_default();
                if group == GROUP_CELL_TEMPORARY {
                    topology.temporary.push(record);
                } else {
                    topology.persistent.push(record);
                }
            }
            cursor += total;
        }
        if cursor != end {
            return Err(PrevisError::invalid(format!("region ended at {cursor:#x}, expected {end:#x}")));
        }
        Ok(())
    }
}

/// A plugin and its masters, resolved in load order for override lookup.
pub struct LoadOrder {
    /// Masters first, the target plugin last.
    pub plugins: Vec<Plugin>,
}

impl LoadOrder {
    pub fn open(target: &Path, data_dirs: &[PathBuf]) -> Result<Self> {
        let plugin = Plugin::open(target)?;
        let mut plugins = Vec::with_capacity(plugin.masters.len() + 1);
        for master in &plugin.masters {
            let path = data_dirs
                .iter()
                .map(|dir| dir.join(master))
                .find(|p| p.is_file())
                .ok_or_else(|| PrevisError::invalid(format!("required master is missing: {master}")))?;
            plugins.push(Plugin::open(&path)?);
        }
        plugins.push(plugin);
        Ok(Self { plugins })
    }

    pub fn target(&self) -> &Plugin {
        self.plugins.last().unwrap()
    }

    pub fn plugin_named(&self, name: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Winning override for a FormID as referenced from `from`.
    pub fn resolve(&self, from: &Plugin, raw: u32) -> Option<(&Plugin, RecordRef)> {
        let (owner, object_id) = from.owner_of(raw);
        self.plugins.iter().rev().find_map(|plugin| {
            let local = plugin.raw_for(owner, object_id)?;
            plugin.record(local).map(|record| (plugin, record))
        })
    }
}

pub fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()))
}

pub fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

pub fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    data.get(offset..offset + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()))
}

pub fn zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}
