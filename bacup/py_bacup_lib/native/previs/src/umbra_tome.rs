//! Typed model of the Umbra 3 tome shipped as FO4 `Vis/<plugin>/<cell>.uvd`.
//!
//! Written from `tools/re/projects/ck_previs_precombine/UMBRA_TOME_FORMAT.md`.
//! Every offset in the file is a `u32` relative to the tome start (tile-local
//! blocks are relative to their tile), every block starts 16-byte aligned, and
//! the writer re-derives all offsets, counts, sizes and the CRC32C from the
//! model, so `write(parse(x)) == x` proves the layout rules, not a byte copy.

use crate::error::{PrevisError, Result};
use crate::tome::{TOME_MAGIC, crc32c};

pub const HEADER_SIZE: usize = 0x14C;
pub const TILE_HEADER_SIZE: usize = 0x50;
pub const BUILD_INFO_LEN: usize = 0x80;
const BUILD_INFO_OFFSET: usize = 0xBC;

/// Kd-tree node count to the length in words of its serialized node stream:
/// two bits per node plus a rank lookup table.
pub fn tree_data_words(node_count: u32) -> usize {
    let n = node_count as usize;
    let lut = (((n >> 4) - (n >> 8) + 3) >> 2) + (n >> 16) + (((n >> 8) - (n >> 16) + 1) >> 1);
    ((2 * n + 31) >> 5) + lut
}

/// Leaf payload stream: `map_width` bits for each of the `(nodes + 1) / 2` leaves.
pub fn tree_map_words(node_count: u32, map_width: u32) -> usize {
    bit_words(node_count.div_ceil(2) as usize * map_width as usize)
}

fn bit_words(bits: usize) -> usize {
    bits.div_ceil(32)
}

/// A serialized kd-tree: 20-byte descriptor plus up to three referenced streams.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tree {
    pub node_count: u32,
    pub map_width: u32,
    pub data: Option<Vec<u32>>,
    pub map: Option<Vec<u32>>,
    pub split_count: u32,
    pub splits: Option<Vec<f32>>,
}

/// Six quantized `u16` coordinates relative to the owning tile/tome bounds,
/// stored in file order `(min_y, min_x, max_x, min_z, max_z, max_y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackedAabb(pub [u16; 6]);

impl PackedAabb {
    pub fn min(&self) -> [u16; 3] {
        [self.0[1], self.0[0], self.0[3]]
    }
    pub fn max(&self) -> [u16; 3] {
        [self.0[2], self.0[5], self.0[4]]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClusterNode {
    pub portal_index: u32,
    pub portal_count: u32,
    pub bounds: PackedAabb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellNode {
    pub portal_index: u32,
    pub portal_count: u32,
    pub object_index: u32,
    pub object_count: u32,
    pub cluster_index: u32,
    pub cluster_count: u32,
    pub bounds: PackedAabb,
}

/// 16-byte portal record shared by tile portals and cluster portals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Portal {
    pub link: u32,
    pub z: u16,
    pub target_index: u16,
    pub rect_a: u32,
    pub rect_b: u32,
}

impl Portal {
    pub const NO_TARGET: u32 = 0x03FF_FFFF;

    pub fn target(&self) -> u32 {
        self.link & Self::NO_TARGET
    }
    pub fn is_hierarchy(&self) -> bool {
        self.link >> 26 & 1 != 0
    }
    pub fn is_user(&self) -> bool {
        self.link >> 27 & 1 != 0
    }
    pub fn is_outside(&self) -> bool {
        self.link >> 28 & 1 != 0
    }
    pub fn face(&self) -> u32 {
        self.link >> 29
    }
    pub fn user_object_offset(&self) -> u32 {
        self.rect_a >> 12
    }
    pub fn user_object_count(&self) -> u32 {
        self.rect_a & 0xFFF
    }
    pub fn gate_vertex_offset(&self) -> u32 {
        self.rect_b >> 12
    }
    pub fn gate_vertex_count(&self) -> u32 {
        self.rect_b & 0xFFF
    }
}

/// Bit-packed `(element, count)` list stream; `count` entries of `width` bits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BitList {
    pub count: u32,
    pub words: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tile {
    pub tree_min: [f32; 3],
    pub tree_max: [f32; 3],
    pub tree: Tree,
    /// Low byte of the size word; bit 0 marks a leaf tile.
    pub flags: u8,
    pub portal_expand: f32,
    pub num_clusters: u16,
    pub cell_nodes: Option<Vec<CellNode>>,
    pub portals: Option<Vec<Portal>>,
    pub planes: Option<Vec<[f32; 4]>>,
    pub num_bsp_nodes: u32,
    pub bsp_triangles: Option<Vec<[u32; 2]>>,
}

impl Tile {
    pub fn is_leaf(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn num_cells(&self) -> usize {
        self.cell_nodes.as_ref().map_or(0, Vec::len)
    }
}

/// Per-leaf-tile border matching entry used when tomes are stitched together.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LeafMatch {
    /// `first_matching_tree << 3 | face_mask`.
    pub packed: u32,
    pub bits_a: u32,
    pub bits_b: u32,
    /// `num_cells * bits_a * bits_b` bits for the leaf tile.
    pub cell_map: Option<Vec<u32>>,
}

impl LeafMatch {
    pub fn first_tree(&self) -> u32 {
        self.packed >> 3
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tome {
    pub lod_base_distance: f32,
    pub flags: u32,
    pub tree_min: [f32; 3],
    pub tree_max: [f32; 3],
    pub tree: Tree,
    pub num_objects: u32,
    pub object_bounds: Option<Vec<[f32; 6]>>,
    pub object_distances: Option<Vec<[u32; 8]>>,
    pub user_ids: Option<Vec<u32>>,
    /// Packed bit widths: object element, object count, cluster element,
    /// cluster count, five bits each from bit 0.
    pub list_widths: u32,
    pub object_lists: Option<BitList>,
    pub cluster_lists: Option<BitList>,
    pub num_gates: u32,
    pub gate_ids: Option<Vec<u32>>,
    pub gate_vertices: Option<Vec<[f32; 3]>>,
    pub gate_indices: Option<Vec<u32>>,
    pub cluster_nodes: Option<Vec<ClusterNode>>,
    pub cluster_portals: Option<Vec<Portal>>,
    pub cell_starts: Option<Vec<u32>>,
    pub bits_per_slot_path: u32,
    pub tile_paths: Option<Vec<u32>>,
    pub tile_lod_levels: Option<Vec<u32>>,
    pub tiles: Vec<Option<Tile>>,
    pub leaf_matches: Vec<LeafMatch>,
    pub matching_trees: Vec<Tree>,
    /// The 128-byte zero-padded optimizer banner field.
    pub build_info: Vec<u8>,
}

impl Tome {
    pub fn num_clusters(&self) -> usize {
        self.cluster_nodes.as_ref().map_or(0, Vec::len)
    }

    /// The optimizer banner, e.g. `T 512.0 SO 128.0 SH 16.000 BF 100 ... - 3.3.17 ...`.
    pub fn build_info_text(&self) -> String {
        let end = self
            .build_info
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.build_info.len());
        String::from_utf8_lossy(&self.build_info[..end]).into_owned()
    }

    fn object_list_width(&self) -> usize {
        ((self.list_widths & 31) + (self.list_widths >> 5 & 31)) as usize
    }

    fn cluster_list_width(&self) -> usize {
        ((self.list_widths >> 10 & 31) + (self.list_widths >> 15 & 31)) as usize
    }
}

fn user_portal_extent(portals: &[Portal]) -> usize {
    portals
        .iter()
        .filter(|p| p.is_user())
        .map(|p| (p.user_object_offset() + p.user_object_count()) as usize)
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------- reading

struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    fn bytes(&self, offset: usize, len: usize) -> Result<&'a [u8]> {
        offset
            .checked_add(len)
            .and_then(|end| self.data.get(offset..end))
            .ok_or_else(|| {
                PrevisError::invalid(format!("tome block 0x{offset:X}+{len} is out of range"))
            })
    }
    fn u16(&self, offset: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.bytes(offset, 2)?.try_into().unwrap(),
        ))
    }
    fn u32(&self, offset: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.bytes(offset, 4)?.try_into().unwrap(),
        ))
    }
    fn f32(&self, offset: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32(offset)?))
    }
    fn vec3(&self, offset: usize) -> Result<[f32; 3]> {
        Ok([
            self.f32(offset)?,
            self.f32(offset + 4)?,
            self.f32(offset + 8)?,
        ])
    }
    fn words(&self, offset: usize, count: usize) -> Result<Vec<u32>> {
        Ok(self
            .bytes(offset, count * 4)?
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect())
    }
    fn array<T>(
        &self,
        base: usize,
        offset: u32,
        count: usize,
        stride: usize,
        read: impl Fn(&Self, usize) -> Result<T>,
    ) -> Result<Option<Vec<T>>> {
        if offset == 0 {
            return Ok(None);
        }
        let start = base + offset as usize;
        self.bytes(start, count * stride)?;
        (0..count)
            .map(|i| read(self, start + i * stride))
            .collect::<Result<_>>()
            .map(Some)
    }
    fn packed_aabb(&self, offset: usize) -> Result<PackedAabb> {
        let mut v = [0u16; 6];
        for (i, slot) in v.iter_mut().enumerate() {
            *slot = self.u16(offset + i * 2)?;
        }
        Ok(PackedAabb(v))
    }
    fn portal(&self, offset: usize) -> Result<Portal> {
        Ok(Portal {
            link: self.u32(offset)?,
            z: self.u16(offset + 4)?,
            target_index: self.u16(offset + 6)?,
            rect_a: self.u32(offset + 8)?,
            rect_b: self.u32(offset + 12)?,
        })
    }
    fn tree(&self, base: usize, descriptor: usize) -> Result<Tree> {
        let packed = self.u32(descriptor)?;
        let (node_count, map_width) = (packed >> 5, packed & 31);
        let split_count = self.u32(descriptor + 12)?;
        let opt_words = |offset: u32, count: usize| -> Result<Option<Vec<u32>>> {
            if offset == 0 {
                Ok(None)
            } else {
                self.words(base + offset as usize, count).map(Some)
            }
        };
        let data = opt_words(self.u32(descriptor + 4)?, tree_data_words(node_count))?;
        let map = opt_words(
            self.u32(descriptor + 8)?,
            tree_map_words(node_count, map_width),
        )?;
        let splits = opt_words(self.u32(descriptor + 16)?, split_count as usize)?
            .map(|w| w.into_iter().map(f32::from_bits).collect());
        Ok(Tree {
            node_count,
            map_width,
            data,
            map,
            split_count,
            splits,
        })
    }
}

fn read_tile(r: &Reader, base: usize) -> Result<Tile> {
    let size_word = r.u32(base + 0x2C)?;
    let num_cells = r.u16(base + 0x34)? as usize;
    let cell_nodes = r.array(base, r.u32(base + 0x38)?, num_cells, 36, |r, o| {
        Ok(CellNode {
            portal_index: r.u32(o)?,
            portal_count: r.u32(o + 4)?,
            object_index: r.u32(o + 8)?,
            object_count: r.u32(o + 12)?,
            cluster_index: r.u32(o + 16)?,
            cluster_count: r.u32(o + 20)?,
            bounds: r.packed_aabb(o + 24)?,
        })
    })?;
    let portal_count = cell_nodes
        .iter()
        .flatten()
        .map(|c| (c.portal_index + c.portal_count) as usize)
        .max()
        .unwrap_or(0);
    let num_bsp_nodes = r.u32(base + 0x44)?;
    let num_planes = r.u32(base + 0x4C)? as usize;
    let tile = Tile {
        tree_min: r.vec3(base)?,
        tree_max: r.vec3(base + 0xC)?,
        tree: r.tree(base, base + 0x18)?,
        flags: size_word as u8,
        portal_expand: r.f32(base + 0x30)?,
        num_clusters: r.u16(base + 0x36)?,
        portals: r.array(base, r.u32(base + 0x3C)?, portal_count, 16, Reader::portal)?,
        bsp_triangles: r.array(
            base,
            r.u32(base + 0x40)?,
            num_bsp_nodes as usize,
            8,
            |r, o| Ok([r.u32(o)?, r.u32(o + 4)?]),
        )?,
        num_bsp_nodes,
        planes: r.array(base, r.u32(base + 0x48)?, num_planes, 16, |r, o| {
            Ok([r.f32(o)?, r.f32(o + 4)?, r.f32(o + 8)?, r.f32(o + 12)?])
        })?,
        cell_nodes,
    };
    if tile.planes.as_ref().map_or(0, Vec::len) != num_planes {
        return Err(PrevisError::unsupported(
            "tile plane count without plane data",
        ));
    }
    Ok(tile)
}

pub fn parse(data: &[u8]) -> Result<Tome> {
    let r = Reader { data };
    if data.len() < HEADER_SIZE || r.u32(0)? != TOME_MAGIC {
        return Err(PrevisError::invalid("not an Umbra tome"));
    }
    if r.u32(8)? as usize != data.len() {
        return Err(PrevisError::invalid(
            "tome size field does not match the data",
        ));
    }
    if r.u32(4)? != crc32c(&data[8..]) {
        return Err(PrevisError::invalid("tome CRC32C mismatch"));
    }
    for (offset, what) in [
        (0x34, "top-level tree map"),
        (0x4C, "user id groups"),
        (0xB0, "tome collection"),
        (0xB4, "collection cluster starts"),
        (0xB8, "collection portal starts"),
        (0x13C, "object depthmaps"),
        (0x140, "depthmap faces"),
        (0x144, "depthmap palettes"),
        (0x148, "depthmap face count"),
    ] {
        if r.u32(offset)? != 0 {
            return Err(PrevisError::unsupported(format!("tome uses {what}")));
        }
    }

    let num_objects = r.u32(0x40)?;
    let list_widths = r.u32(0x54)?;
    let num_gates = r.u32(0x68)?;
    let num_clusters = r.u32(0x7C)? as usize;
    let num_leaf_tiles = r.u32(0x8C)? as usize;
    let num_tiles = r.u32(0x90)? as usize;
    let bits_per_slot_path = r.u32(0x94)?;
    let tree = r.tree(0, 0x2C)?;

    let bit_list = |offset: u32, count: u32, width: usize| -> Result<Option<BitList>> {
        if offset == 0 {
            return Ok(None);
        }
        let words = r.words(offset as usize, bit_words(width * count as usize))?;
        Ok(Some(BitList { count, words }))
    };
    let u32s = |offset: u32, count: usize| r.array(0, offset, count, 4, Reader::u32);

    let cluster_nodes = r.array(0, r.u32(0x80)?, num_clusters, 20, |r, o| {
        Ok(ClusterNode {
            portal_index: r.u32(o)?,
            portal_count: r.u32(o + 4)?,
            bounds: r.packed_aabb(o + 8)?,
        })
    })?;
    let cluster_portal_count = cluster_nodes
        .as_ref()
        .and_then(|nodes| nodes.last())
        .map_or(0, |n| (n.portal_index + n.portal_count) as usize);
    let cluster_portals = r.array(0, r.u32(0x84)?, cluster_portal_count, 16, Reader::portal)?;

    let tile_offsets = r.words(r.u32(0xA0)? as usize, num_tiles)?;
    let tiles = tile_offsets
        .iter()
        .map(|&offset| {
            (offset != 0)
                .then(|| read_tile(&r, offset as usize))
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?;

    let gate_extent = tiles
        .iter()
        .flatten()
        .filter_map(|t| t.portals.as_deref())
        .chain(cluster_portals.as_deref())
        .map(user_portal_extent)
        .max()
        .unwrap_or(0);

    let leaf_cells: Vec<usize> = tiles
        .iter()
        .flatten()
        .filter(|t| t.is_leaf())
        .map(Tile::num_cells)
        .collect();
    if leaf_cells.len() != num_leaf_tiles {
        return Err(PrevisError::unsupported(
            "leaf tile count disagrees with the tile flags",
        ));
    }
    let matching_base = r.u32(0xA4)? as usize;
    let leaf_matches = (0..num_leaf_tiles)
        .map(|i| {
            let o = matching_base + i * 16;
            let (bits_a, bits_b) = (r.u32(o + 8)?, r.u32(o + 12)?);
            let map_offset = r.u32(o + 4)?;
            let cell_map = if map_offset == 0 {
                None
            } else {
                let bits = leaf_cells[i] * bits_a as usize * bits_b as usize;
                Some(r.words(map_offset as usize, bit_words(bits))?)
            };
            Ok(LeafMatch {
                packed: r.u32(o)?,
                bits_a,
                bits_b,
                cell_map,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let trees_base = r.u32(0xA8)? as usize;
    let matching_trees = (0..r.u32(0xAC)? as usize)
        .map(|i| r.tree(0, trees_base + i * 20))
        .collect::<Result<Vec<_>>>()?;

    let tome = Tome {
        lod_base_distance: r.f32(0x0C)?,
        flags: r.u32(0x10)?,
        tree_min: r.vec3(0x14)?,
        tree_max: r.vec3(0x20)?,
        num_objects,
        object_bounds: r.array(0, r.u32(0x44)?, num_objects as usize, 24, |r, o| {
            Ok([
                r.f32(o)?,
                r.f32(o + 4)?,
                r.f32(o + 8)?,
                r.f32(o + 12)?,
                r.f32(o + 16)?,
                r.f32(o + 20)?,
            ])
        })?,
        object_distances: r.array(0, r.u32(0x48)?, num_objects as usize, 32, |r, o| {
            Ok(r.words(o, 8)?.try_into().unwrap())
        })?,
        user_ids: u32s(r.u32(0x50)?, num_objects as usize)?,
        list_widths,
        object_lists: None,
        cluster_lists: None,
        num_gates,
        gate_ids: u32s(r.u32(0x6C)?, num_gates as usize)?,
        gate_vertices: r.array(0, r.u32(0x70)?, r.u32(0x74)? as usize, 12, Reader::vec3)?,
        gate_indices: u32s(r.u32(0x78)?, gate_extent)?,
        cluster_nodes,
        cluster_portals,
        cell_starts: u32s(r.u32(0x88)?, num_tiles + 1)?,
        bits_per_slot_path,
        tile_paths: u32s(
            r.u32(0x98)?,
            bit_words(bits_per_slot_path as usize * num_tiles),
        )?,
        tile_lod_levels: u32s(r.u32(0x9C)?, tree.node_count as usize)?,
        tree,
        tiles,
        leaf_matches,
        matching_trees,
        build_info: r.bytes(BUILD_INFO_OFFSET, BUILD_INFO_LEN)?.to_vec(),
    };
    let object_lists = bit_list(r.u32(0x58)?, r.u32(0x5C)?, tome.object_list_width())?;
    let cluster_lists = bit_list(r.u32(0x60)?, r.u32(0x64)?, tome.cluster_list_width())?;
    if tome.gate_vertices.is_none() && r.u32(0x74)? != 0 {
        return Err(PrevisError::unsupported(
            "gate vertex count without gate vertices",
        ));
    }
    Ok(Tome {
        object_lists,
        cluster_lists,
        ..tome
    })
}

// ---------------------------------------------------------------- writing

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn align(&mut self) {
        self.out.resize(self.out.len().next_multiple_of(16), 0);
    }
    fn put_u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn put_f32(&mut self, v: f32) {
        self.put_u32(v.to_bits());
    }
    fn set_u32(&mut self, at: usize, v: u32) {
        self.out[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    /// Emits one aligned block and returns its offset relative to `base`, or 0 when absent.
    fn block<T>(&mut self, base: usize, items: Option<&[T]>, put: impl Fn(&mut Self, &T)) -> u32 {
        let Some(items) = items else { return 0 };
        self.align();
        let offset = (self.out.len() - base) as u32;
        for item in items {
            put(self, item);
        }
        offset
    }
    fn words(&mut self, base: usize, words: Option<&[u32]>) -> u32 {
        self.block(base, words, |w, &v| w.put_u32(v))
    }
    fn packed_aabb(&mut self, b: &PackedAabb) {
        for &v in &b.0 {
            self.put_u16(v);
        }
    }
    fn portal(&mut self, p: &Portal) {
        self.put_u32(p.link);
        self.put_u16(p.z);
        self.put_u16(p.target_index);
        self.put_u32(p.rect_a);
        self.put_u32(p.rect_b);
    }
    /// Streams in file order data, splits, map; returns (data, map, splits) offsets.
    fn tree_streams(&mut self, base: usize, tree: &Tree) -> [u32; 3] {
        let data = self.words(base, tree.data.as_deref());
        let splits = self.block(base, tree.splits.as_deref(), |w, &v| w.put_f32(v));
        let map = self.words(base, tree.map.as_deref());
        [data, map, splits]
    }
    fn tree_descriptor(&mut self, at: usize, tree: &Tree, [data, map, splits]: [u32; 3]) {
        self.set_u32(at, tree.node_count << 5 | tree.map_width);
        self.set_u32(at + 4, data);
        self.set_u32(at + 8, map);
        self.set_u32(at + 12, tree.split_count);
        self.set_u32(at + 16, splits);
    }
}

fn write_tile(w: &mut Writer, tile: &Tile) -> u32 {
    w.align();
    let base = w.out.len();
    for v in tile.tree_min.iter().chain(&tile.tree_max) {
        w.put_f32(*v);
    }
    w.out.resize(base + TILE_HEADER_SIZE, 0);
    w.out[base + 0x30..base + 0x34].copy_from_slice(&tile.portal_expand.to_bits().to_le_bytes());
    w.out[base + 0x34..base + 0x36].copy_from_slice(&(tile.num_cells() as u16).to_le_bytes());
    w.out[base + 0x36..base + 0x38].copy_from_slice(&tile.num_clusters.to_le_bytes());
    let cells = w.block(base, tile.cell_nodes.as_deref(), |w, c| {
        for v in [
            c.portal_index,
            c.portal_count,
            c.object_index,
            c.object_count,
            c.cluster_index,
            c.cluster_count,
        ] {
            w.put_u32(v);
        }
        w.packed_aabb(&c.bounds);
    });
    let portals = w.block(base, tile.portals.as_deref(), Writer::portal);
    let planes = w.block(base, tile.planes.as_deref(), |w, p| {
        p.iter().for_each(|&v| w.put_f32(v))
    });
    let tree = w.tree_streams(base, &tile.tree);
    let bsp = w.block(base, tile.bsp_triangles.as_deref(), |w, t| {
        t.iter().for_each(|&v| w.put_u32(v))
    });
    w.align();
    let size = (w.out.len() - base) as u32;
    w.tree_descriptor(base + 0x18, &tile.tree, tree);
    w.set_u32(base + 0x2C, size << 8 | tile.flags as u32);
    w.set_u32(base + 0x38, cells);
    w.set_u32(base + 0x3C, portals);
    w.set_u32(base + 0x40, bsp);
    w.set_u32(base + 0x44, tile.num_bsp_nodes);
    w.set_u32(base + 0x48, planes);
    w.set_u32(base + 0x4C, tile.planes.as_ref().map_or(0, Vec::len) as u32);
    base as u32
}

pub fn write(tome: &Tome) -> Vec<u8> {
    let mut w = Writer::default();
    w.out.resize(HEADER_SIZE, 0);
    let mut header = vec![(0x0C, tome.lod_base_distance.to_bits()), (0x10, tome.flags)];
    for (i, v) in tome.tree_min.iter().chain(&tome.tree_max).enumerate() {
        header.push((0x14 + i * 4, v.to_bits()));
    }
    let info = &tome.build_info[..tome.build_info.len().min(BUILD_INFO_LEN)];
    w.out[BUILD_INFO_OFFSET..BUILD_INFO_OFFSET + info.len()].copy_from_slice(info);

    let tree = w.tree_streams(0, &tome.tree);
    let tile_paths = w.words(0, tome.tile_paths.as_deref());
    let lod_levels = w.words(0, tome.tile_lod_levels.as_deref());
    let cell_starts = w.words(0, tome.cell_starts.as_deref());
    let object_lists = w.words(0, tome.object_lists.as_ref().map(|l| l.words.as_slice()));
    let cluster_lists = w.words(0, tome.cluster_lists.as_ref().map(|l| l.words.as_slice()));
    let cluster_nodes = w.block(0, tome.cluster_nodes.as_deref(), |w, c| {
        w.put_u32(c.portal_index);
        w.put_u32(c.portal_count);
        w.packed_aabb(&c.bounds);
    });
    let cluster_portals = w.block(0, tome.cluster_portals.as_deref(), Writer::portal);
    let gate_vertices = w.block(0, tome.gate_vertices.as_deref(), |w, v| {
        v.iter().for_each(|&f| w.put_f32(f))
    });
    let gate_indices = w.words(0, tome.gate_indices.as_deref());
    let object_bounds = w.block(0, tome.object_bounds.as_deref(), |w, b| {
        b.iter().for_each(|&f| w.put_f32(f))
    });
    let object_distances = w.block(0, tome.object_distances.as_deref(), |w, d| {
        d.iter().for_each(|&v| w.put_u32(v))
    });
    let user_ids = w.words(0, tome.user_ids.as_deref());

    w.align();
    let tile_offsets = w.out.len();
    w.out.resize(tile_offsets + tome.tiles.len() * 4, 0);
    for (i, tile) in tome.tiles.iter().enumerate() {
        if let Some(tile) = tile {
            let offset = write_tile(&mut w, tile);
            w.set_u32(tile_offsets + i * 4, offset);
        }
    }

    let gate_ids = w.words(0, tome.gate_ids.as_deref());
    w.align();
    let matching = w.out.len();
    w.out.resize(matching + tome.leaf_matches.len() * 16, 0);
    w.align();
    let trees = w.out.len();
    w.out.resize(trees + tome.matching_trees.len() * 20, 0);
    let mut next_tree = 0;
    for (i, leaf) in tome.leaf_matches.iter().enumerate() {
        // A leaf without matching trees stores 0 and does not bound the previous leaf's range.
        let end = tome.leaf_matches[i + 1..]
            .iter()
            .find(|n| n.packed != 0)
            .map_or(tome.matching_trees.len(), |n| n.first_tree() as usize);
        while next_tree < end.min(tome.matching_trees.len()) {
            let tree = &tome.matching_trees[next_tree];
            let streams = w.tree_streams(0, tree);
            w.tree_descriptor(trees + next_tree * 20, tree, streams);
            next_tree += 1;
        }
        let map = w.words(0, leaf.cell_map.as_deref());
        let o = matching + i * 16;
        for (k, v) in [leaf.packed, map, leaf.bits_a, leaf.bits_b]
            .into_iter()
            .enumerate()
        {
            w.set_u32(o + k * 4, v);
        }
    }
    for (i, tree) in tome.matching_trees.iter().enumerate().skip(next_tree) {
        let streams = w.tree_streams(0, tree);
        w.tree_descriptor(trees + i * 20, tree, streams);
    }
    w.align();

    let count = |v: Option<usize>| v.unwrap_or(0) as u32;
    header.extend([
        (0x40, tome.num_objects),
        (0x44, object_bounds),
        (0x48, object_distances),
        (0x50, user_ids),
        (0x54, tome.list_widths),
        (0x58, object_lists),
        (0x5C, tome.object_lists.as_ref().map_or(0, |l| l.count)),
        (0x60, cluster_lists),
        (0x64, tome.cluster_lists.as_ref().map_or(0, |l| l.count)),
        (0x68, tome.num_gates),
        (0x6C, gate_ids),
        (0x70, gate_vertices),
        (0x74, count(tome.gate_vertices.as_ref().map(Vec::len))),
        (0x78, gate_indices),
        (0x7C, tome.num_clusters() as u32),
        (0x80, cluster_nodes),
        (0x84, cluster_portals),
        (0x88, cell_starts),
        (0x8C, tome.leaf_matches.len() as u32),
        (0x90, tome.tiles.len() as u32),
        (0x94, tome.bits_per_slot_path),
        (0x98, tile_paths),
        (0x9C, lod_levels),
        (0xA0, tile_offsets as u32),
        (
            0xA4,
            if tome.leaf_matches.is_empty() {
                0
            } else {
                matching as u32
            },
        ),
        (
            0xA8,
            if tome.matching_trees.is_empty() {
                0
            } else {
                trees as u32
            },
        ),
        (0xAC, tome.matching_trees.len() as u32),
    ]);
    for (at, v) in header {
        w.set_u32(at, v);
    }
    w.tree_descriptor(0x2C, &tome.tree, tree);

    let size = w.out.len() as u32;
    w.set_u32(0, TOME_MAGIC);
    w.set_u32(8, size);
    let crc = crc32c(&w.out[8..]);
    w.set_u32(4, crc);
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> Tome {
        let mut build_info = vec![0u8; BUILD_INFO_LEN];
        build_info[..6].copy_from_slice(b"3.3.17");
        let outside = |face: u32, z: u16| Portal {
            link: face << 29 | 1 << 28 | Portal::NO_TARGET,
            z,
            target_index: 0,
            rect_a: 0x0000_FFFF << 16,
            rect_b: 0x0000_FFFF,
        };
        let full = PackedAabb([0, 0, 0xFFFF, 0, 0xFFFF, 0xFFFF]);
        let leaf_tree = Tree {
            node_count: 1,
            map_width: 1,
            data: Some(vec![3]),
            map: Some(vec![0]),
            ..Tree::default()
        };
        Tome {
            lod_base_distance: 512.0,
            tree_min: [0.0; 3],
            tree_max: [512.0; 3],
            tree: Tree {
                node_count: 1,
                data: Some(vec![3]),
                split_count: 1,
                splits: Some(vec![0.0]),
                ..Tree::default()
            },
            cluster_nodes: Some(vec![ClusterNode {
                bounds: full,
                ..ClusterNode::default()
            }]),
            cell_starts: Some(vec![0, 1]),
            bits_per_slot_path: 1,
            tile_paths: Some(vec![0]),
            tile_lod_levels: Some(vec![0]),
            tiles: vec![Some(Tile {
                tree_max: [512.0; 3],
                tree: leaf_tree.clone(),
                flags: 1,
                portal_expand: 256.0,
                num_clusters: 1,
                cell_nodes: Some(vec![CellNode {
                    portal_count: 6,
                    bounds: full,
                    ..CellNode::default()
                }]),
                portals: Some(
                    (0..6)
                        .map(|f| outside(f, if f % 2 == 0 { 0 } else { 0xFFFF }))
                        .collect(),
                ),
                ..Tile::default()
            })],
            leaf_matches: vec![LeafMatch {
                packed: 6 << 3,
                ..LeafMatch::default()
            }],
            matching_trees: vec![leaf_tree; 6],
            build_info,
            ..Tome::default()
        }
    }

    #[test]
    fn synthetic_tome_round_trips_with_valid_crc_and_alignment() {
        let bytes = write(&minimal());
        assert_eq!(bytes.len() % 16, 0);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            crc32c(&bytes[8..])
        );
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed, minimal());
        assert_eq!(write(&parsed), bytes);
    }

    #[test]
    #[ignore = "set UMBRA_TOME_CORPUS to a directory of retail .uvd files"]
    fn retail_uvds_round_trip_exactly() {
        let root = std::env::var_os("UMBRA_TOME_CORPUS").expect("UMBRA_TOME_CORPUS is not set");
        let mut files = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("uvd"))
                {
                    files.push(path);
                }
            }
        }
        files.sort();
        let mut failures = std::collections::BTreeMap::<String, Vec<String>>::new();
        for path in &files {
            let data = std::fs::read(path).unwrap();
            let category = match parse(&data) {
                Err(e) => Some(format!("parse: {e}")),
                Ok(tome) => {
                    let out = write(&tome);
                    if out == data {
                        None
                    } else {
                        let first = (0..out.len().min(data.len()))
                            .find(|&i| !(4..8).contains(&i) && out[i] != data[i])
                            .unwrap_or(out.len().min(data.len()));
                        let kind = if out.len() == data.len() {
                            "same length"
                        } else {
                            "length differs"
                        };
                        println!(
                            "    {}: first diff 0x{first:X}, len {} vs {}",
                            path.display(),
                            out.len(),
                            data.len()
                        );
                        Some(format!("mismatch, {kind}"))
                    }
                }
            };
            if let Some(category) = category {
                failures
                    .entry(category)
                    .or_default()
                    .push(path.display().to_string());
            }
        }
        let failed: usize = failures.values().map(Vec::len).sum();
        println!(
            "umbra tome round trip: {}/{} exact",
            files.len() - failed,
            files.len()
        );
        for (category, paths) in &failures {
            println!("  {} x {category}: e.g. {}", paths.len(), paths[0]);
        }
        assert!(!files.is_empty(), "no .uvd files found");
        assert_eq!(failed, 0);
    }
}
