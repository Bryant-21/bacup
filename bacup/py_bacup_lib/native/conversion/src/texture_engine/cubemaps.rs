//! FO4 environment-cubemap normalization.
//!
//! FO4's 72 vanilla cubemaps are all genuine six-face cubes; 45 of them are
//! 128x128 uncompressed with a full mip chain, which is the convention this
//! module targets.  Source cubes from Skyrim and FNV arrive at assorted sizes
//! and block-compressed formats.
//!
//! Flat 2D images in the env-map slot (29 in FNV, 4 in Skyrim) have no cube
//! structure to recover and return `Ok(None)` so the caller can substitute a
//! vanilla FO4 cubemap instead.

use directxtex_native::{
    DDS_FLAGS_NONE, DXGI_FORMAT_R8G8B8A8_UNORM, ScratchImage, TEX_FILTER_FLAGS,
    TEX_FILTER_FORCE_NON_WIC, TEX_FILTER_TRIANGLE, TexMetadata,
};

pub(crate) const FO4_CUBEMAP_EDGE: usize = 128;

/// DirectXTex routes `Resize`/`GenerateMipMaps` through WIC by default, which
/// needs COM initialized on the calling thread. Conversion phases run on rayon
/// workers that never call `CoInitialize`, so WIC returns `E_NOINTERFACE`.
///
/// The filter must also be one DirectXTex implements itself — POINT, LINEAR,
/// CUBIC or TRIANGLE. BOX/FANT is WIC-only, so pairing it with
/// `FORCE_NON_WIC` fails with `E_FAIL`. TRIANGLE is the best-quality custom
/// filter and handles both the 32->128 upscales and 1024->128 downscales in
/// the source corpus.
fn resample_filter() -> TEX_FILTER_FLAGS {
    TEX_FILTER_TRIANGLE | TEX_FILTER_FORCE_NON_WIC
}

/// Normalize a source environment map to FO4's cubemap convention.
///
/// Returns `Ok(None)` when the source is not a cubemap.
pub(crate) fn normalize_cubemap_bytes(source: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let mut metadata = TexMetadata::default();
    let image = ScratchImage::load_dds(source, DDS_FLAGS_NONE, Some(&mut metadata), None)
        .map_err(|error| format!("load cubemap: {error}"))?;

    if !metadata.is_cubemap() {
        return Ok(None);
    }

    let decoded = if metadata.format.is_compressed() {
        image
            .decompress(DXGI_FORMAT_R8G8B8A8_UNORM)
            .map_err(|error| format!("decompress cubemap: {error}"))?
    } else if metadata.format != DXGI_FORMAT_R8G8B8A8_UNORM {
        image
            .convert(DXGI_FORMAT_R8G8B8A8_UNORM, resample_filter(), 0.0)
            .map_err(|error| format!("convert cubemap: {error}"))?
    } else {
        image
    };

    // Resize collapses the chain to a single level, so mips are regenerated after.
    let resized = decoded
        .resize(FO4_CUBEMAP_EDGE, FO4_CUBEMAP_EDGE, resample_filter())
        .map_err(|error| format!("resize cubemap: {error}"))?;
    let mipped = resized
        .generate_mip_maps(resample_filter(), 0)
        .map_err(|error| format!("mip cubemap: {error}"))?;

    let blob = mipped
        .save_dds(DDS_FLAGS_NONE)
        .map_err(|error| format!("save cubemap: {error}"))?;
    Ok(Some(blob.buffer().to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use directxtex_native::CP_FLAGS_NONE;

    const DDSCAPS2_CUBEMAP: u32 = 0x200;
    const CAPS2_ALL_FACES: u32 = 0xfe00;

    fn caps2_of(bytes: &[u8]) -> u32 {
        u32::from_le_bytes(bytes[112..116].try_into().unwrap())
    }

    fn dims_of(bytes: &[u8]) -> (u32, u32, u32) {
        let height = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let width = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let mips = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        (width, height, mips)
    }

    fn synthetic_cube_dds(edge: usize) -> Vec<u8> {
        let mut image = ScratchImage::default();
        image
            .initialize_cube(DXGI_FORMAT_R8G8B8A8_UNORM, edge, edge, 1, 1, CP_FLAGS_NONE)
            .unwrap();
        for byte in image.pixels_mut() {
            *byte = 200;
        }
        image.save_dds(DDS_FLAGS_NONE).unwrap().buffer().to_vec()
    }

    #[test]
    fn normalizes_a_cube_to_the_fo4_convention() {
        let source = synthetic_cube_dds(32);
        let out = normalize_cubemap_bytes(&source).unwrap().expect("cube");

        let (width, height, mips) = dims_of(&out);
        assert_eq!(width, 128);
        assert_eq!(height, 128);
        assert_eq!(mips, 8, "128x128 has a full chain of 8");
        assert_eq!(
            caps2_of(&out) & DDSCAPS2_CUBEMAP,
            DDSCAPS2_CUBEMAP,
            "cubemap bit must survive"
        );
        assert_eq!(caps2_of(&out), CAPS2_ALL_FACES, "all six face bits set");
    }

    #[test]
    fn normalized_payload_is_exactly_six_faces() {
        let source = synthetic_cube_dds(32);
        let out = normalize_cubemap_bytes(&source).unwrap().expect("cube");
        // 128x128 RGBA8 full chain = 87380 bytes per face.
        let one_face: usize = (0..8).map(|i| (128usize >> i) * (128usize >> i) * 4).sum();
        assert_eq!(one_face, 87380);
        let body = out.len() - 128;
        assert_eq!(body, one_face * 6, "payload must be exactly 6.00 faces");
    }

    #[test]
    fn returns_none_for_a_flat_2d_source() {
        // FNV ships 29 of these: architecture/novac/motel_window_e.dds and friends.
        let mut image = ScratchImage::default();
        image
            .initialize_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 64, 64, 1, 1, CP_FLAGS_NONE)
            .unwrap();
        let bytes = image.save_dds(DDS_FLAGS_NONE).unwrap().buffer().to_vec();
        assert!(
            normalize_cubemap_bytes(&bytes).unwrap().is_none(),
            "a 2D source must report 'not a cube' rather than emit one face"
        );
    }
}
