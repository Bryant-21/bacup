use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bsarchive_native::python::ArchiveReader;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct GameData {
    pub directories: Vec<String>,
    #[serde(default)]
    pub archive_directories: Vec<String>,
}

pub struct Alpha {
    pub width: usize,
    pub height: usize,
    pub values: Vec<u8>,
}

pub struct Assets {
    roots: Vec<PathBuf>,
    archives: Vec<Mutex<ArchiveReader>>,
    index: HashMap<String, (usize, String)>,
    alpha: Mutex<VecDeque<(String, Option<Arc<Alpha>>)>>,
}

pub fn data_name(folder: &str, name: &str) -> String {
    let name = name
        .replace('\0', "")
        .replace('\\', "/")
        .trim()
        .trim_matches('/')
        .to_lowercase();
    if name.is_empty() || name.starts_with(&format!("{folder}/")) {
        name
    } else {
        format!("{folder}/{name}")
    }
}

impl Assets {
    pub fn new(data: &GameData) -> Result<Self, String> {
        let mut archives = Vec::new();
        let mut index = HashMap::new();
        let mut seen = HashSet::new();
        for directory in &data.archive_directories {
            for root in [PathBuf::from(directory), Path::new(directory).join("Data")] {
                let Ok(entries) = std::fs::read_dir(root) else {
                    continue;
                };
                let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
                paths.sort_by_key(|path| path.to_string_lossy().to_lowercase());
                for path in paths {
                    if !path.is_file()
                        || !path.extension().is_some_and(|suffix| {
                            suffix.eq_ignore_ascii_case("ba2") || suffix.eq_ignore_ascii_case("bsa")
                        })
                    {
                        continue;
                    }
                    if !seen.insert(path.to_string_lossy().to_lowercase()) {
                        continue;
                    }
                    let reader = match ArchiveReader::open(&path) {
                        Ok(reader) => reader,
                        Err(error) => {
                            eprintln!(
                                "[item_icons] Failed to open archive {}: {error}",
                                path.display()
                            );
                            continue;
                        }
                    };
                    for name in reader.list_files() {
                        index.insert(
                            name.replace('\\', "/").to_lowercase(),
                            (archives.len(), name),
                        );
                    }
                    archives.push(Mutex::new(reader));
                }
            }
        }
        Ok(Self {
            roots: data.directories.iter().map(PathBuf::from).collect(),
            archives,
            index,
            alpha: Mutex::new(VecDeque::new()),
        })
    }

    pub fn loose(&self, name: &str) -> Option<PathBuf> {
        self.roots
            .iter()
            .map(|root| root.join(name.replace('\\', "/")))
            .find(|path| path.is_file())
    }

    pub fn exists(&self, name: &str) -> bool {
        self.loose(name).is_some()
            || self
                .index
                .contains_key(&name.replace('\\', "/").to_lowercase())
    }

    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(path) = self.loose(name) {
            return std::fs::read(path).map(Some).map_err(|e| e.to_string());
        }
        let Some((archive, member)) = self.index.get(&name.replace('\\', "/").to_lowercase())
        else {
            return Ok(None);
        };
        let result = self.archives[*archive]
            .lock()
            .map_err(|e| e.to_string())?
            .read_file(member);
        match result {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) => {
                eprintln!("[item_icons] Failed to extract {member}: {error}");
                Ok(None)
            }
        }
    }

    pub fn model(&self, model: &str) -> Option<String> {
        [
            format!("meshes/{model}"),
            format!(
                "meshes/{}",
                model.strip_prefix("bacup_static/").unwrap_or(model)
            ),
        ]
        .into_iter()
        .find(|name| self.exists(name))
    }

    pub fn alpha(&self, name: &str) -> Result<Option<Arc<Alpha>>, String> {
        let mut cache = self.alpha.lock().map_err(|e| e.to_string())?;
        if let Some(index) = cache.iter().position(|(key, _)| key == name) {
            let entry = cache.remove(index).unwrap();
            let result = entry.1.clone();
            cache.push_back(entry);
            return Ok(result);
        }
        let alpha = match self.read(name)? {
            Some(bytes) => decode_alpha(&bytes)?.map(Arc::new),
            None => None,
        };
        if cache.len() == 4 {
            cache.pop_front();
        }
        cache.push_back((name.into(), alpha.clone()));
        Ok(alpha)
    }
}

fn decode_alpha(bytes: &[u8]) -> Result<Option<Alpha>, String> {
    if bytes.len() < 128 || &bytes[..4] != b"DDS " {
        return Err("Invalid DDS header".into());
    }
    let integer = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let flags = integer(80);
    let has_alpha = if flags & 0x40 != 0 {
        flags & 1 != 0
    } else if flags & 0x20000 != 0 {
        integer(88) == 16 && flags & 1 != 0
    } else if flags & 0x20 != 0 {
        false
    } else if flags & 4 != 0 {
        match &bytes[84..88] {
            b"DXT1" | b"DXT3" | b"DXT5" => true,
            b"ATI1" | b"ATI2" | b"BC4U" | b"BC5U" | b"BC5S" => false,
            b"DX10" if bytes.len() >= 148 => match integer(128) {
                27..=29 | 70..=78 | 97..=99 => true,
                79 | 80 | 82..=84 | 95 | 96 => false,
                format => return Err(format!("Unsupported icon DDS format {format}")),
            },
            _ => return Err("Unsupported icon DDS pixel format".into()),
        }
    } else {
        return Err("Unsupported icon DDS pixel flags".into());
    };
    if !has_alpha {
        return Ok(None);
    }
    let block_offset = match &bytes[84..88] {
        b"DXT5" => Some(128),
        b"DX10" if bytes.len() >= 148 && matches!(integer(128), 76..=78) => Some(148),
        _ => None,
    };
    if let Some(offset) = block_offset {
        return bc3_alpha(&bytes[offset..], integer(16) as usize, integer(12) as usize).map(Some);
    }
    let image = directxtex_native::read_dds_rgba_image_bytes(bytes)?;
    Ok(Some(Alpha {
        width: image.width as usize,
        height: image.height as usize,
        values: image.rgba.chunks_exact(4).map(|pixel| pixel[3]).collect(),
    }))
}

fn bc3_alpha(data: &[u8], width: usize, height: usize) -> Result<Alpha, String> {
    let columns = width.div_ceil(4);
    let rows = height.div_ceil(4);
    let size = columns
        .checked_mul(rows)
        .and_then(|n| n.checked_mul(16))
        .ok_or("DDS dimensions overflow")?;
    if width == 0 || height == 0 || data.len() < size {
        return Err("Truncated BC3 texture".into());
    }
    let mut values = vec![0; width * height];
    for (index, block) in data[..size].chunks_exact(16).enumerate() {
        let (a, b) = (u16::from(block[0]), u16::from(block[1]));
        let mut alpha = [0u8; 8];
        alpha[..2].copy_from_slice(&block[..2]);
        // Pillow's BC3 alpha interpolation truncates; DirectXTex rounds instead.
        if a > b {
            for i in 2..8 {
                alpha[i] = (((8 - i) as u16 * a + (i - 1) as u16 * b) / 7) as u8;
            }
        } else {
            for i in 2..6 {
                alpha[i] = (((6 - i) as u16 * a + (i - 1) as u16 * b) / 5) as u8;
            }
            alpha[7] = 255;
        }
        let bits = block[2..8]
            .iter()
            .enumerate()
            .fold(0u64, |bits, (i, &byte)| bits | (u64::from(byte) << (8 * i)));
        let (left, top) = ((index % columns) * 4, (index / columns) * 4);
        for y in 0..4 {
            for x in 0..4 {
                if left + x < width && top + y < height {
                    values[(top + y) * width + left + x] =
                        alpha[((bits >> (3 * (4 * y + x))) & 7) as usize];
                }
            }
        }
    }
    Ok(Alpha {
        width,
        height,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc3_alpha_preserves_truncation_and_short_endpoint_mode() {
        for (a, b, expected) in [
            (255, 0, [255, 0, 218, 182, 145, 109, 72, 36]),
            (0, 254, [0, 254, 50, 101, 152, 203, 0, 255]),
        ] {
            let mut block = [0; 16];
            block[0] = a;
            block[1] = b;
            let bits = (0..16).fold(0u64, |bits, i| bits | ((i % 8) << (3 * i)));
            block[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
            assert_eq!(bc3_alpha(&block, 4, 4).unwrap().values, expected.repeat(2));
        }
        assert!(bc3_alpha(&[0; 15], 4, 4).is_err());
    }
}
