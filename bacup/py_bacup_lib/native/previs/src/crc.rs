//! Bethesda's raw reflected CRC-32 (no final XOR) used across CK generation.

const POLYNOMIAL: u32 = 0xEDB8_8320;

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = (value >> 1) ^ if value & 1 != 0 { POLYNOMIAL } else { 0 };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

pub fn update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc = TABLE[((byte as u32 ^ crc) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

pub fn bs_crc32(data: &[u8]) -> u32 {
    update(0, data)
}

pub fn update_upper(crc: u32, text: &str) -> u32 {
    update(crc, text.to_ascii_uppercase().as_bytes())
}

/// PSG/CSG filename hash: CRC of the lowercase stem.
pub fn geometry_filename_hash(file_name: &str) -> u32 {
    let stem = match file_name.rfind('.') {
        Some(dot) => &file_name[..dot],
        None => file_name,
    };
    bs_crc32(stem.to_ascii_lowercase().as_bytes())
}

/// CK floors each coordinate, masks it to a 512-unit cell as an *unsigned*
/// 32-bit value (so negative cells wrap), and hashes the resulting floats.
fn quantized_coordinate(value: f32) -> f32 {
    let floored = (value as f64).floor() as i64;
    let masked = (floored as u32) & 0xFFFF_FE00;
    masked as f32
}

pub fn spatial_group_key(center: [f32; 3]) -> u32 {
    let mut bytes = [0u8; 12];
    for (axis, &value) in center.iter().enumerate() {
        bytes[axis * 4..axis * 4 + 4].copy_from_slice(&quantized_coordinate(value).to_le_bytes());
    }
    bs_crc32(&bytes) >> 2
}

pub const EXTERIOR_CELL_SIZE: f32 = 4096.0;
const EXTERIOR_COLUMNS: i32 = 2;
const EXTERIOR_COLUMN_SIZE: f32 = EXTERIOR_CELL_SIZE / EXTERIOR_COLUMNS as f32;
const EXTERIOR_LEVELS: i32 = 4;
const EXTERIOR_LEVEL_SIZE: f32 = 1024.0;
/// Word `level` of the mask, bit `x * 3 + y`: recovered from the 67 group
/// keys CK wrote for 25 Appalachia CELLs, all of which decode to such a box.
const EXTERIOR_ROW_STRIDE: i32 = 3;

/// The subdivisions `[low, high]` along one axis that the bound's extent
/// overlaps, clipped to `0..count`; `None` when it lies wholly outside.
fn overlapped(low: f32, high: f32, size: f32, count: i32) -> Option<(i32, i32)> {
    let first = ((low / size).floor() as i32).max(0);
    let last = ((high / size).floor() as i32).min(count - 1);
    (first <= last).then_some((first, last))
}

/// CK's exterior group key: the CRC of a 20-byte mask of the subdivisions
/// the reference's bounding-sphere box overlaps. The CELL is split into 2x2
/// columns of 2048 units and four 1024-unit levels starting at its lowest
/// LAND vertex. Nothing is marked outside those, or at all without LAND, so
/// such references share the empty-mask key 0.
///
/// Recovered from CK output rather than a trace: it reproduces 4,572 of the
/// 4,573 references CK combined in 25 Appalachia CELLs and 1,300 of 1,301 in
/// seven retail Commonwealth CELLs; each miss is a runtime bound that differs
/// from ours, not the key.
pub fn exterior_mask_key(center: [f32; 3], radius: f32, grid: (i32, i32), land_minimum: Option<f32>) -> u32 {
    let mut words = [0u32; 5];
    if let Some(floor) = land_minimum {
        let origin = [grid.0 as f32 * EXTERIOR_CELL_SIZE, grid.1 as f32 * EXTERIOR_CELL_SIZE, floor];
        let span = |axis: usize, size: f32, count: i32| {
            overlapped(center[axis] - radius - origin[axis], center[axis] + radius - origin[axis], size, count)
        };
        let columns = (span(0, EXTERIOR_COLUMN_SIZE, EXTERIOR_COLUMNS), span(1, EXTERIOR_COLUMN_SIZE, EXTERIOR_COLUMNS));
        if let ((Some(xs), Some(ys)), Some(levels)) = (columns, span(2, EXTERIOR_LEVEL_SIZE, EXTERIOR_LEVELS)) {
            for level in levels.0..=levels.1 {
                for x in xs.0..=xs.1 {
                    for y in ys.0..=ys.1 {
                        words[level as usize] |= 1 << (x * EXTERIOR_ROW_STRIDE + y);
                    }
                }
            }
        }
    }
    let mut bytes = [0u8; 20];
    for (index, word) in words.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    bs_crc32(&bytes) >> 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_bound_centres_map_to_the_traced_group_keys() {
        assert_eq!(spatial_group_key([113.69702911376953, -8.496938789903652e-06, 211.31422424316406]), 0x16000F31);
        assert_eq!(spatial_group_key([369.6970520019531, 255.99998474121094, 211.31422424316406]), 0x00000000);
        assert_eq!(spatial_group_key([1226.981689453125, 995.9204711914062, 283.0429992675781]), 0x32022960);
    }

    /// References and keys from CK's precombine of Appalachia (-26,22) and
    /// (-24,24), LAND minima 13112 and 14080, and retail Fallout4.esm.
    #[test]
    fn exterior_keys_match_ck_output() {
        let cases: [([f32; 3], f32, (i32, i32), f32, u32); 7] = [
            ([-104289.453125, 93444.265625, 14893.13671875], 154.13095092773438, (-26, 22), 13112.0, 0x3989_CFA7),
            // The box reaches the next level up.
            ([-103614.6953125, 93444.1640625, 15035.5859375], 127.76936340332031, (-26, 22), 13112.0, 0x3A8F_F4D8),
            ([-103540.390625, 93424.6640625, 15080.69140625], 46.969966888427734, (-26, 22), 13112.0, 0x3989_CFA7),
            // Both columns on both axes and three levels.
            ([-96346.8828125, 101422.09375, 15521.3759765625], 1364.5830078125, (-24, 24), 14080.0, 0x04F0_9B23),
            // Wholly above the fourth level (retail CELL DD99) and wholly below
            // the LAND (retail CELL DA20): an empty mask.
            ([-41133.953125, 79176.6953125, 4599.947265625], 152.21282958984375, (-11, 19), 160.0, 0),
            ([70041.671875, 69369.9140625, 536.447265625], 345.967041015625, (17, 16), 1392.0, 0),
            ([-43399.15234375, 79914.6171875, 4499.93701171875], 37.659423828125, (-11, 19), 160.0, 0),
        ];
        for (center, radius, grid, land, key) in cases {
            assert_eq!(exterior_mask_key(center, radius, grid, Some(land)), key, "{center:?}");
        }
        assert_eq!(exterior_mask_key([0.0, 0.0, 0.0], 10.0, (0, 0), None), 0);
    }

    #[test]
    fn filename_hash_uses_the_lowercase_stem() {
        assert_eq!(geometry_filename_hash("B21_CKRE_Interior - Geometry.psg"), 3959362665);
    }
}
