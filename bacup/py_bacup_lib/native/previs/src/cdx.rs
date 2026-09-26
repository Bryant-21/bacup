//! Precombine resource index (`<plugin>.cdx`) shipped beside the `.csg`.
//!
//! One 16-byte row `(kind, CELL, value, extra)` per resource a precombined
//! CELL needs, sorted by `(CELL, kind, extra, value)`. Named resources are
//! BSResource-style ids: the CRC of the lowercase stem (text before the last
//! dot) and of the lowercase directory. Interior rows are verified row for
//! row against every retail interior CDX.
//!
//! An exterior CELL's rows are keyed by its world instead: the CELL word holds
//! the WRLD FormID and the kind carries the grid, `x` in bits 20..32 and `y` in
//! bits 8..20, over the resource kind without its interior bit `0x80` (retail
//! Fallout4.cdx: CELL E0D9 at (1,-6) indexes its meshes as kind `0x001FFA06`).
//! Its visibility row names the cluster root's UVD, as the CELL's RVIS does.

use nif_core_native::model::{NifFile, NifValue};

use crate::crc::bs_crc32;
use crate::error::{PrevisError, Result};
use crate::shader;

pub const MAGIC: &[u8; 4] = b"bcdx";

const GEOMETRY: u32 = 0x80;
const LIGHTING_MATERIAL: u32 = 0x81;
const EFFECT_MATERIAL: u32 = 0x82;
const DATA_TEXTURE: u32 = 0x83;
const COLOR_TEXTURE: u32 = 0x84;
const MESH: u32 = 0x86;
const VISIBILITY: u32 = 0x87;
const NO_VISIBILITY: u32 = 0x88;
const EFFECT_TEXTURE_KINDS: [(&str, u32); 5] = [
    ("Source Texture", COLOR_TEXTURE),
    ("Greyscale Texture", COLOR_TEXTURE),
    ("Env Map Texture", COLOR_TEXTURE),
    ("Normal Texture", DATA_TEXTURE),
    ("Env Mask Texture", DATA_TEXTURE),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Row {
    pub cell: u32,
    pub kind: u32,
    pub extra: u32,
    pub value: u32,
}

/// Retail only shows slots 0–7 (displacement and slot 9 never occur), so
/// those two stay unsupported rather than guessed.
fn texture_kind(slot: usize) -> Result<u32> {
    match slot {
        0 | 3 | 4 => Ok(COLOR_TEXTURE),
        1 | 2 | 5 | 6 | 7 => Ok(DATA_TEXTURE),
        _ => Err(PrevisError::unsupported(format!("texture slot {slot} has no verified CDX kind"))),
    }
}

fn data_relative(path: &str, root: &str) -> String {
    let mut normalized = path.to_ascii_lowercase().replace('/', "\\");
    if let Some(at) = normalized.find("\\data\\") {
        normalized = normalized[at + 6..].to_string();
    } else if let Some(rest) = normalized.strip_prefix("data\\") {
        normalized = rest.to_string();
    }
    if normalized.starts_with(&format!("{root}\\")) {
        normalized
    } else {
        format!("{root}\\{normalized}")
    }
}

fn resource_id(relative: &str) -> (u32, u32) {
    let (directory, name) = relative.rsplit_once('\\').unwrap_or(("", relative));
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    (bs_crc32(stem.as_bytes()), bs_crc32(directory.as_bytes()))
}

fn text(value: Option<&NifValue>) -> Option<&str> {
    match value {
        Some(NifValue::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CellLocation {
    Interior,
    Exterior { world: u32, grid: (i32, i32) },
}

impl CellLocation {
    /// The row's CELL word and its kind for an interior resource kind.
    fn key(&self, cell_form_id: u32, kind: u32) -> (u32, u32) {
        match *self {
            CellLocation::Interior => (cell_form_id, kind),
            CellLocation::Exterior { world, grid: (x, y) } => {
                (world, ((x as u32 & 0xFFF) << 20) | ((y as u32 & 0xFFF) << 8) | (kind & 0x7F))
            }
        }
    }
}

/// Rows for one precombined CELL from its group NIFs, keyed by XCRI mesh ID.
/// `visibility` is the CELL whose UVD covers this one, when that UVD exists:
/// the CELL itself inside, its cluster root outside.
pub fn cell_rows(
    plugin_name: &str,
    cell_form_id: u32,
    location: CellLocation,
    groups: impl IntoIterator<Item = Result<(u32, NifFile)>>,
    visibility: Option<u32>,
) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let mut push = |kind, value, extra| {
        let (cell, kind) = location.key(cell_form_id, kind);
        rows.push(Row { cell, kind, extra, value })
    };
    for group in groups {
        let (mesh_id, nif) = group?;
        push(MESH, mesh_id, cell_form_id);
        for block in &nif.blocks {
            match block.type_name.as_str() {
                "BSPackedCombinedSharedGeomDataExtra" => {
                    let Some(NifValue::Array(objects)) = block.fields.get("Object") else {
                        return Err(PrevisError::invalid("PCD has no Object array"));
                    };
                    for object in objects {
                        let NifValue::Struct(fields) = object else {
                            return Err(PrevisError::invalid("PCD object is not a struct"));
                        };
                        let word = |name: &str| match fields.get(name) {
                            Some(NifValue::UInt(v)) => Ok(*v as u32),
                            _ => Err(PrevisError::invalid(format!("PCD object lacks {name}"))),
                        };
                        push(GEOMETRY, word("Data Offset")?, word("Filename Hash")?);
                    }
                }
                "BSLightingShaderProperty" => {
                    if let Some(name) = text(block.fields.get("Name")) {
                        let (value, extra) = resource_id(&data_relative(name, "materials"));
                        push(LIGHTING_MATERIAL, value, extra);
                    }
                    let Some(NifValue::Ref(set)) = block.fields.get("Texture Set") else {
                        continue;
                    };
                    let Some(set) = usize::try_from(*set).ok().and_then(|i| nif.blocks.get(i)) else {
                        continue;
                    };
                    if let Some(NifValue::Array(textures)) = set.fields.get("Textures") {
                        for (slot, texture) in textures.iter().enumerate() {
                            if let Some(path) = text(Some(texture)) {
                                let (value, extra) = resource_id(&data_relative(path, "textures"));
                                push(texture_kind(slot)?, value, extra);
                            }
                        }
                    }
                }
                // Retail indexes an unnamed effect shader's textures only when
                // another shader in the CELL uses them, so it adds no rows.
                "BSEffectShaderProperty" => {
                    let Some(name) = text(block.fields.get("Name")) else { continue };
                    let (value, extra) = resource_id(&data_relative(name, "materials"));
                    push(EFFECT_MATERIAL, value, extra);
                    for (field, kind) in EFFECT_TEXTURE_KINDS {
                        if let Some(path) = text(shader::get_bare(&block.fields, field)) {
                            let (value, extra) = resource_id(&data_relative(path, "textures"));
                            push(kind, value, extra);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // Retail has no exterior CELL without previs; its marker row is the
    // interior one moved to the exterior key like every other kind.
    match visibility {
        Some(root) => {
            let plugin = plugin_name.to_ascii_lowercase();
            push(
                VISIBILITY,
                bs_crc32(format!("{:08x}", root & 0x00FF_FFFF).as_bytes()),
                bs_crc32(format!("vis\\{plugin}").as_bytes()),
            );
        }
        None => push(NO_VISIBILITY, bs_crc32(b""), bs_crc32(b"vis")),
    }
    Ok(rows)
}

pub fn encode(mut rows: Vec<Row>) -> Vec<u8> {
    rows.sort_unstable();
    rows.dedup();
    let mut out = Vec::with_capacity(8 + rows.len() * 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for row in rows {
        for word in [row.kind, row.cell, row.value, row.extra] {
            out.extend_from_slice(&word.to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_ids_match_retail_dlcworkshop01() {
        assert_eq!(
            resource_id(&data_relative(
                "C:\\Projects\\Fallout4\\Build\\PC\\Data\\materials\\Interiors\\Building\\BldWoodWall01.BGSM",
                "materials"
            )),
            (0x4E57_8EE1, 0xA6E6_6AC9)
        );
        assert_eq!(
            resource_id(&data_relative("shared/cubemaps/roombuildingblurcube_e.dds", "textures")),
            (0x4081_55E9, 0xC588_DB1C)
        );
        assert_eq!(bs_crc32(b"0000091f"), 0x31E8_AC65);
        assert_eq!(bs_crc32(b"vis\\dlcworkshop01.esm"), 0x34D2_067C);
        assert_eq!(bs_crc32(b"vis"), 0x2D4F_E48A);
    }

    #[test]
    fn only_named_effect_shaders_add_rows() {
        let mut nif = NifFile::new("fo4");
        for name in ["Materials\\Effects\\Glow.BGEM", ""] {
            let id = nif.add_block("BSEffectShaderProperty", None);
            for (field, value) in [
                ("Name", name),
                ("Source Texture", "Textures\\Effects\\Glow_d.dds"),
                ("Greyscale Texture", "Textures\\Effects\\Grad_d.dds"),
                ("Env Map Texture", ""),
                ("Normal Texture", "Textures\\Effects\\Glow_n.dds"),
                ("Env Mask Texture", "Textures\\Effects\\Glow_m.dds"),
            ] {
                shader::set_bare(&mut nif.blocks[id].fields, field, NifValue::String(value.into()));
            }
        }
        let rows = cell_rows("Test.esm", 7, CellLocation::Interior, [Ok((1, nif))], None).unwrap();
        let kind_of = |path: &str, root: &str| {
            let (value, extra) = resource_id(&data_relative(path, root));
            rows.iter().filter(|r| r.value == value && r.extra == extra).map(|r| r.kind).collect::<Vec<_>>()
        };
        assert_eq!(kind_of("Materials\\Effects\\Glow.BGEM", "materials"), [EFFECT_MATERIAL]);
        assert_eq!(kind_of("Textures\\Effects\\Glow_d.dds", "textures"), [COLOR_TEXTURE]);
        assert_eq!(kind_of("Textures\\Effects\\Grad_d.dds", "textures"), [COLOR_TEXTURE]);
        assert_eq!(kind_of("Textures\\Effects\\Glow_n.dds", "textures"), [DATA_TEXTURE]);
        assert_eq!(kind_of("Textures\\Effects\\Glow_m.dds", "textures"), [DATA_TEXTURE]);
        // Mesh, no-visibility, one material and four texture rows; the unnamed shader adds none.
        assert_eq!(rows.len(), 7);
    }

    /// Retail Fallout4.cdx rows of Commonwealth (world 3C) CELLs E0D9 at
    /// (1,-6), whose RVIS root is E4C6, and DC65 at (0,28), root DC86.
    #[test]
    fn exterior_rows_match_retail_fallout4() {
        let rows = |cell, grid, root| {
            let location = CellLocation::Exterior { world: 0x3C, grid };
            cell_rows("Fallout4.esm", cell, location, [Ok((0x01B8_656E, NifFile::new("fo4")))], Some(root)).unwrap()
        };
        assert_eq!(
            rows(0xE0D9, (1, -6), 0xE4C6),
            [
                Row { cell: 0x3C, kind: 0x001F_FA06, extra: 0xE0D9, value: 0x01B8_656E },
                Row { cell: 0x3C, kind: 0x001F_FA07, extra: 0xD055_7C27, value: 0x2605_AE84 },
            ]
        );
        assert_eq!(rows(0xDC65, (0, 28), 0xDC86)[1], Row { cell: 0x3C, kind: 0x1C07, extra: 0xD055_7C27, value: 0xAE05_754B });
        let without = cell_rows("Fallout4.esm", 0xE0D9, CellLocation::Exterior { world: 0x3C, grid: (1, -6) }, [], None).unwrap();
        assert_eq!(without, [Row { cell: 0x3C, kind: 0x001F_FA08, extra: bs_crc32(b"vis"), value: bs_crc32(b"") }]);
    }

    #[test]
    fn encode_sorts_by_cell_kind_extra_value() {
        let rows = vec![
            Row { cell: 2, kind: GEOMETRY, extra: 1, value: 0 },
            Row { cell: 1, kind: MESH, extra: 1, value: 5 },
            Row { cell: 1, kind: GEOMETRY, extra: 9, value: 1 },
            Row { cell: 1, kind: GEOMETRY, extra: 2, value: 7 },
            Row { cell: 1, kind: GEOMETRY, extra: 2, value: 7 },
        ];
        let bytes = encode(rows);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 4);
        let word = |row: usize, i: usize| u32::from_le_bytes(bytes[8 + row * 16 + i * 4..][..4].try_into().unwrap());
        assert_eq!((word(0, 0), word(0, 2), word(0, 3)), (GEOMETRY, 7, 2));
        assert_eq!((word(1, 0), word(1, 3)), (GEOMETRY, 9));
        assert_eq!(word(2, 0), MESH);
        assert_eq!(word(3, 1), 2);
    }
}
