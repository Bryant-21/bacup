//! FO76 native assets, texture side (spec section 7.3): the bytes BACUP ships for an FO76 texture
//! that FO4CS samples as authored. Everything is a byte copy except BC5_SNORM normals (endpoint
//! shift to UNORM) and 2D textures with no mip chain (a box-filtered chain in the source format).

use std::path::Path;

use directxtex_native::SharedBc7Encoder;
use materials_native::texture_convert::{TexturePathOutput, TextureSetPathRequest};

use super::bc5_endpoint_shift::shift_bc5_snorm_dds;

const DDSD_MIPMAPCOUNT: u32 = 0x2_0000;

fn header_u32(dds: &[u8], offset: usize) -> Option<u32> {
    dds.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn lacks_mip_chain(dds: &[u8]) -> bool {
    let (Some(flags), Some(height), Some(width), Some(count)) = (
        header_u32(dds, 8),
        header_u32(dds, 12),
        header_u32(dds, 16),
        header_u32(dds, 28),
    ) else {
        return false;
    };
    let levels = if flags & DDSD_MIPMAPCOUNT != 0 && count > 0 { count } else { 1 };
    dds.starts_with(b"DDS ") && levels == 1 && (width > 1 || height > 1)
}

pub fn can_copy_as_authored(source: &Path) -> bool {
    use std::io::Read;

    let mut header = [0u8; 148];
    if std::fs::File::open(source)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_err()
    {
        return false;
    }
    directxtex_native::read_dds_probe_bytes(&header)
        .is_ok_and(|probe| probe.dxgi_format != 84 && !lacks_mip_chain(&header))
}

/// Encoder name for a decoded source format; `None` keeps the file as authored.
fn chain_format_name(dxgi_format: u32) -> Option<&'static str> {
    Some(match dxgi_format {
        28 => "R8G8B8A8_UNORM",
        29 => "R8G8B8A8_UNORM_SRGB",
        71 => "BC1_UNORM",
        72 => "BC1_UNORM_SRGB",
        74 => "BC2_UNORM",
        75 => "BC2_UNORM_SRGB",
        77 => "BC3_UNORM",
        78 => "BC3_UNORM_SRGB",
        80 => "BC4_UNORM",
        // Decoded through DirectXTex to UNORM texels, so the result is the shifted form.
        83 | 84 => "BC5_UNORM",
        98 => "BC7_UNORM",
        99 => "BC7_UNORM_SRGB",
        _ => return None,
    })
}

fn with_generated_mips(source: &Path, bc7: Option<&SharedBc7Encoder<'_>>) -> Option<Vec<u8>> {
    let decoded = directxtex_native::read_dds_mips_rgba8(source).ok()?;
    let format = chain_format_name(decoded.dxgi_format)?;
    let (width, height, base) = decoded.mips.into_iter().next()?;
    let chain = directxtex_native::rgba8_box_mip_chain(width, height, &base).ok()?;
    directxtex_native::encode_dds_from_owned_rgba8_chain(chain, format, false, bc7).ok()
}

pub fn same_file_name(left: &Path, right: &Path) -> bool {
    left.file_name()
        .zip(right.file_name())
        .is_some_and(|(left, right)| left.to_string_lossy().eq_ignore_ascii_case(&right.to_string_lossy()))
}

pub fn native_outputs(request: &TextureSetPathRequest) -> Vec<TexturePathOutput> {
    let Some(output_dir) = request.outputs.first().and_then(|output| output.path.parent()) else {
        return Vec::new();
    };
    request
        .inputs
        .iter()
        .filter_map(|input| {
            Some(TexturePathOutput {
                role: input.role.clone(),
                path: output_dir.join(input.path.file_name()?),
                format: String::new(),
            })
        })
        .collect()
}

/// The source's global PBR probes FO4CS reflects BGSM v22+ materials off. FO4CS reads them only
/// from its own folder: two share names with FO4 cubemaps every vanilla metal samples.
pub const ADVANCED_LIGHTING_GLOBAL_CUBES: [&str; 4] = [
    "mipblur_defaultoutside1.dds",
    "mipblur_defaultoutside1_dielectric.dds",
    "mipblur_instinterior.dds",
    "mipblur_instinterior_dialectric.dds",
];

pub fn advanced_lighting_global_cube_output(data_root: &Path, name: &str) -> std::path::PathBuf {
    data_root
        .join("Textures")
        .join("FO4CS")
        .join("Cubemaps")
        .join("AdvancedLighting")
        .join(name)
}

/// Copies each global probe the source has from `Textures/Shared/Cubemaps` into FO4CS's folder,
/// as authored. Returns how many were written.
pub fn copy_advanced_lighting_global_cubes(
    source_extracted: &Path,
    data_root: &Path,
) -> Result<usize, String> {
    let source_dir = source_extracted.join("Textures").join("Shared").join("Cubemaps");
    let mut written = 0;
    for name in ADVANCED_LIGHTING_GLOBAL_CUBES {
        let source = source_dir.join(name);
        if !source.is_file() {
            continue;
        }
        let output = advanced_lighting_global_cube_output(data_root, name);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::copy(&source, &output)
            .map_err(|error| format!("copy {} -> {}: {error}", source.display(), output.display()))?;
        written += 1;
    }
    Ok(written)
}

pub fn native_texture_bytes(
    source: &Path,
    bc7: Option<&SharedBc7Encoder<'_>>,
) -> Result<Vec<u8>, String> {
    let mut bytes =
        std::fs::read(source).map_err(|error| format!("read {}: {error}", source.display()))?;
    // Cubemaps, arrays and formats the encoder cannot write ship as authored.
    if lacks_mip_chain(&bytes)
        && let Some(with_mips) = with_generated_mips(source, bc7)
    {
        return Ok(with_mips);
    }
    shift_bc5_snorm_dds(&mut bytes).map_err(|error| format!("{}: {error}", source.display()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BC5_SNORM: u32 = 84;
    const BC5_UNORM: u32 = 83;

    fn mip_count(dds: &[u8]) -> u32 {
        let flags = header_u32(dds, 8).unwrap();
        let count = header_u32(dds, 28).unwrap();
        if flags & DDSD_MIPMAPCOUNT != 0 && count > 0 { count } else { 1 }
    }

    fn dx10_format(dds: &[u8]) -> Option<u32> {
        (&dds[84..88] == b"DX10").then(|| header_u32(dds, 128).unwrap())
    }

    fn checker(width: u32, height: u32) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| if i % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })
            .collect()
    }

    /// 4x4 BC5_SNORM with its full chain (4x4, 2x2, 1x1): one 16-byte block per level.
    fn bc5_snorm_with_chain() -> Vec<u8> {
        let mut dds = vec![0u8; 148];
        dds[..4].copy_from_slice(b"DDS ");
        dds[4..8].copy_from_slice(&124u32.to_le_bytes());
        dds[8..12].copy_from_slice(&(0x1007 | DDSD_MIPMAPCOUNT).to_le_bytes());
        dds[12..16].copy_from_slice(&4u32.to_le_bytes());
        dds[16..20].copy_from_slice(&4u32.to_le_bytes());
        dds[28..32].copy_from_slice(&3u32.to_le_bytes());
        dds[76..80].copy_from_slice(&32u32.to_le_bytes());
        dds[80..84].copy_from_slice(&4u32.to_le_bytes());
        dds[84..88].copy_from_slice(b"DX10");
        dds[128..132].copy_from_slice(&BC5_SNORM.to_le_bytes());
        dds[132..136].copy_from_slice(&3u32.to_le_bytes());
        dds[140..144].copy_from_slice(&1u32.to_le_bytes());
        for _ in 0..3 {
            dds.extend_from_slice(&[0x81, 0x7F, 0, 0, 0, 0, 0, 0, 0x00, 0xFF, 0, 0, 0, 0, 0, 0]);
        }
        dds
    }

    fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_bc5_snorm_normal_is_shifted_to_unorm_and_keeps_its_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "rock_n.dds", &bc5_snorm_with_chain());
        let out = native_texture_bytes(&path, None).unwrap();
        assert_eq!(dx10_format(&out), Some(BC5_UNORM));
        assert_eq!(mip_count(&out), 3);
        assert_eq!(&out[148..152], &[0x01, 0xFF, 0, 0]);
    }

    #[test]
    fn a_mipless_texture_gets_a_chain_in_its_own_format() {
        let dir = tempfile::tempdir().unwrap();
        let source =
            directxtex_native::encode_dds_from_rgba8_chain(&[(8, 8, checker(8, 8))], "BC1_UNORM", false, None)
                .unwrap();
        assert_eq!(mip_count(&source), 1);
        let path = write(&dir, "sign_d.dds", &source);
        let out = native_texture_bytes(&path, None).unwrap();
        assert_eq!(mip_count(&out), 4);
        let decoded = directxtex_native::read_dds_mips_rgba8(&write(&dir, "out.dds", &out)).unwrap();
        assert_eq!(decoded.dxgi_format, 71);
    }

    #[test]
    fn a_texture_with_its_chain_is_a_byte_copy() {
        let dir = tempfile::tempdir().unwrap();
        let chain = directxtex_native::rgba8_box_mip_chain(8, 8, &checker(8, 8)).unwrap();
        let source = directxtex_native::encode_dds_from_rgba8_chain(&chain, "BC7_UNORM", false, None).unwrap();
        let path = write(&dir, "rock_r.dds", &source);
        assert_eq!(native_texture_bytes(&path, None).unwrap(), source);
    }

    #[test]
    fn a_missing_source_is_an_error() {
        assert!(native_texture_bytes(Path::new("does/not/exist.dds"), None).is_err());
    }

    #[test]
    fn global_cubes_ship_only_in_the_fo4cs_folder() {
        let source = tempfile::tempdir().unwrap();
        let cubes = source.path().join("Textures").join("Shared").join("Cubemaps");
        std::fs::create_dir_all(&cubes).unwrap();
        std::fs::write(cubes.join("mipblur_defaultoutside1.dds"), b"metal").unwrap();
        std::fs::write(cubes.join("mipblur_instinterior_dialectric.dds"), b"dielectric").unwrap();
        let output = tempfile::tempdir().unwrap();
        let data_root = output.path().join("data");

        let written = copy_advanced_lighting_global_cubes(source.path(), &data_root).unwrap();

        assert_eq!(written, 2);
        let fo4cs = data_root.join("Textures").join("FO4CS").join("Cubemaps").join("AdvancedLighting");
        assert_eq!(std::fs::read(fo4cs.join("mipblur_defaultoutside1.dds")).unwrap(), b"metal");
        assert_eq!(
            std::fs::read(fo4cs.join("mipblur_instinterior_dialectric.dds")).unwrap(),
            b"dielectric"
        );
        assert!(!data_root.join("Textures").join("Shared").exists());
        for name in ADVANCED_LIGHTING_GLOBAL_CUBES {
            let rel = advanced_lighting_global_cube_output(Path::new("data"), name);
            let rel = rel.strip_prefix("data").unwrap().to_string_lossy().into_owned();
            assert!(!crate::relocation::is_target_cubemap_path(&rel), "{rel}");
        }
    }
}
