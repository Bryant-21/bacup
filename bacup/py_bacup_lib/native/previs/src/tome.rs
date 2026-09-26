//! Umbra 3 tome header: CRC32C and optimizer-version compatibility rewrite.
//!
//! CK 1.11.137 embeds Umbra 3.3.22 while shipped FO4 UVDs were built with
//! 3.3.17. The visibility payload is identical; only the two version digits
//! and the CRC32C over `[8, declared_size)` differ.

use crate::error::{PrevisError, Result};

pub const TOME_MAGIC: u32 = 0xD600_0012;
const CRC32C_POLYNOMIAL: u32 = 0x82F6_3B78;
const VERSION_SEARCH_LIMIT: usize = 512;

const fn crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut value = 0;
    while value < 256 {
        let mut checksum = value as u32;
        let mut bit = 0;
        while bit < 8 {
            checksum = if checksum & 1 != 0 {
                (checksum >> 1) ^ CRC32C_POLYNOMIAL
            } else {
                checksum >> 1
            };
            bit += 1;
        }
        table[value] = checksum;
        value += 1;
    }
    table
}

static CRC32C_TABLE: [u32; 256] = crc32c_table();

pub fn crc32c(data: &[u8]) -> u32 {
    let mut checksum = 0xFFFF_FFFFu32;
    for &byte in data {
        checksum = CRC32C_TABLE[((checksum ^ byte as u32) & 0xFF) as usize] ^ (checksum >> 8);
    }
    !checksum
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomeHeader {
    pub size: u32,
    pub stored_crc32c: u32,
    pub computed_crc32c: u32,
    pub optimizer_version: String,
    version_range: (usize, usize),
}

impl TomeHeader {
    pub fn crc32c_valid(&self) -> bool {
        self.stored_crc32c == self.computed_crc32c
    }
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// Finds the single ` - X.Y.Z F ` optimizer banner in the tome header.
fn find_optimizer_version(data: &[u8]) -> Result<(usize, usize)> {
    let window = &data[..data.len().min(VERSION_SEARCH_LIMIT)];
    let mut found = None;
    let mut start = 0;
    while let Some(position) = find(&window[start..], b" - ").map(|p| p + start) {
        let version_start = position + 3;
        let mut cursor = version_start;
        let mut dots = 0;
        let mut digits_in_part = 0;
        while cursor < window.len() {
            let byte = window[cursor];
            if byte.is_ascii_digit() {
                digits_in_part += 1;
            } else if byte == b'.' && digits_in_part > 0 && dots < 2 {
                dots += 1;
                digits_in_part = 0;
            } else {
                break;
            }
            cursor += 1;
        }
        if dots == 2 && digits_in_part > 0 && window[cursor..].starts_with(b" F ") {
            if found.is_some() {
                return Err(PrevisError::invalid(
                    "Umbra tome contains more than one optimizer version",
                ));
            }
            found = Some((version_start, cursor));
        }
        start = position + 1;
    }
    found.ok_or_else(|| PrevisError::invalid("Umbra tome does not contain an optimizer version"))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

pub fn inspect_tome(data: &[u8]) -> Result<TomeHeader> {
    if data.len() < 12 {
        return Err(PrevisError::invalid("Umbra tome is smaller than its header"));
    }
    let magic = u32_at(data, 0);
    if magic != TOME_MAGIC {
        return Err(PrevisError::invalid(format!(
            "unexpected Umbra tome magic: 0x{magic:08X}"
        )));
    }
    let declared_size = u32_at(data, 8);
    if declared_size as usize != data.len() {
        return Err(PrevisError::invalid(format!(
            "Umbra tome declares {declared_size} bytes but contains {}",
            data.len()
        )));
    }
    let version_range = find_optimizer_version(data)?;
    Ok(TomeHeader {
        size: declared_size,
        stored_crc32c: u32_at(data, 4),
        computed_crc32c: crc32c(&data[8..]),
        optimizer_version: String::from_utf8_lossy(&data[version_range.0..version_range.1])
            .into_owned(),
        version_range,
    })
}

pub fn rewrite_optimizer_version(data: &[u8], optimizer_version: &str) -> Result<Vec<u8>> {
    let header = inspect_tome(data)?;
    let (start, end) = header.version_range;
    if optimizer_version.len() != end - start {
        return Err(PrevisError::invalid(
            "replacement optimizer version must preserve the header length",
        ));
    }
    let mut rewritten = data.to_vec();
    rewritten[start..end].copy_from_slice(optimizer_version.as_bytes());
    let checksum = crc32c(&rewritten[8..]);
    rewritten[4..8].copy_from_slice(&checksum.to_le_bytes());
    Ok(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_matches_the_castagnoli_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    fn synthetic_tome(version: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&TOME_MAGIC.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(b"Umbra - ");
        data.extend_from_slice(version);
        data.extend_from_slice(b" F payload");
        let size = data.len() as u32;
        data[8..12].copy_from_slice(&size.to_le_bytes());
        let checksum = crc32c(&data[8..]);
        data[4..8].copy_from_slice(&checksum.to_le_bytes());
        data
    }

    #[test]
    fn rewrite_changes_only_version_and_crc_and_rejects_length_changes() {
        {
            let source = synthetic_tome(b"3.3.22");
            let rewritten = rewrite_optimizer_version(&source, "3.3.17").unwrap();
            let header = inspect_tome(&rewritten).unwrap();
            assert_eq!(header.optimizer_version, "3.3.17");
            assert!(header.crc32c_valid());
            let differing = source
                .iter()
                .zip(&rewritten)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(index, _)| index)
                .filter(|index| !(4..8).contains(index))
                .count();
            assert_eq!(differing, 2);
        }
        {
            let source = synthetic_tome(b"3.3.22");
            assert!(rewrite_optimizer_version(&source, "3.3.170").is_err());
        }
    }
}
