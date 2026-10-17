use materials_native::bgsm::{self, BgsmData};

use std::collections::BTreeSet;

use terrain_native::authoring_emit::RequiredTextureUsage;

use super::ltex_walk::normalize_esp_form_key;
use super::manifest::TextureManifest;

pub(super) fn prepare_manifest(manifest: &mut TextureManifest) -> Result<(), String> {
    for bundle in &mut manifest.textures {
        let bytes = std::fs::read(&bundle.source_material_file)
            .map_err(|error| format!("read raw terrain material {}: {error}", bundle.source_material_file))?;
        let material = bgsm::parse(&bytes)
            .map_err(|error| format!("parse raw terrain material {}: {error}", bundle.source_material_file))?;
        bundle.raw_material_rows = Some(material_rows(&material)
            .map_err(|error| format!("raw terrain material {}: {error}", bundle.source_material_file))?);
        bundle.raw_source_prefix = Some(format!("{}_source", bundle.output_prefix));
    }
    Ok(())
}

/// BTD4 must never change the ESM: bundles the BTD4-off scan does not need stay in the
/// manifest for the raw source bank and its texture copies, but emit no records.
///
/// `build_manifest` makes one bundle per LTEX with a normalized `Plugin:ObjectId` key and
/// no GCVR, while usages carry `ObjectId:Plugin` keys and their ground cover, so match on
/// the normalized LTEX alone.
/// A record LTEX with no bundle is an error: shipping it would leave BTD-off users and
/// terrain LOD without that landscape texture.
pub(super) fn mark_raw_bank_only(
    manifest: &mut TextureManifest,
    record_usages: &[RequiredTextureUsage],
) -> Result<(), String> {
    let mut unmatched: BTreeSet<String> = record_usages
        .iter()
        .map(|usage| normalize_esp_form_key(&usage.ltex_form_key).into_owned())
        .collect();
    let record_ltexes = unmatched.clone();
    for bundle in &mut manifest.textures {
        let ltex = normalize_esp_form_key(&bundle.source_ltex_form_key);
        bundle.raw_bank_only = !record_ltexes.contains(ltex.as_ref());
        unmatched.remove(ltex.as_ref());
    }
    if unmatched.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "BTD4 raw source bank: {} LTEX(es) needed as records have no manifest bundle: {}",
            unmatched.len(),
            unmatched.into_iter().collect::<Vec<_>>().join(", ")
        ))
    }
}

fn material_rows(material: &BgsmData) -> Result<[[f32; 4]; 3], String> {
    if material.PBR != Some(true) || material.Terrain != Some(true) {
        return Err("source BGSM is not a PBR terrain material".into());
    }
    let required = |value: Option<f32>, name: &str| {
        value.filter(|v| v.is_finite()).ok_or_else(|| format!("missing or non-finite {name}"))
    };
    let softness = required(material.TerrainThresholdFalloff, "blend softness")?.max(0.0001);
    let tiling = required(material.TerrainTilingDistance, "tiling distance")?;
    let rotation = required(material.TerrainRotationAngle, "rotation angle")?.to_radians();
    let color = if material.EmitEnabled {
        material.EmittanceColor.ok_or("missing emittance override color")?
    } else {
        [0.0; 3]
    };
    let rows = [
        [softness, rotation.sin(), rotation.cos(), if tiling == 0.0 { f32::from_bits(0x3B40_0000) } else { tiling.recip() }],
        [color[0].powf(2.2), color[1].powf(2.2), color[2].powf(2.2), if material.EmitEnabled { 1.0 } else { 0.0 }],
        [material.EmittanceMult * required(material.LumEmittance, "luminous emittance")?,
         required(material.AdaptativeEmissive_ExposureOffset, "stable emittance exposure offset")?,
         required(material.AdaptativeEmissive_FinalExposureMin, "stable emittance min exposure")?,
         required(material.AdaptativeEmissive_FinalExposureMax, "stable emittance max exposure")?],
    ];
    if !rows.iter().flatten().all(|v| v.is_finite()) {
        return Err("native material rows contain non-finite values".into());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn material() -> BgsmData {
        BgsmData {
            PBR: Some(true), Terrain: Some(true),
            TerrainThresholdFalloff: Some(0.25), TerrainTilingDistance: Some(128.0),
            TerrainRotationAngle: Some(90.0), EmitEnabled: true,
            EmittanceColor: Some([0.5, 0.25, 1.0]), EmittanceMult: 3.0,
            LumEmittance: Some(4.0), UseAdaptativeEmissive: Some(false),
            AdaptativeEmissive_ExposureOffset: Some(-2.0),
            AdaptativeEmissive_FinalExposureMin: Some(0.125),
            AdaptativeEmissive_FinalExposureMax: Some(8.0),
            ..Default::default()
        }
    }

    #[test]
    fn raw_terrain_rows_match_native_softness_rotation_scale_and_emittance() {
        let rows = material_rows(&material()).unwrap();
        assert_eq!(rows[0][0], 0.25);
        assert!((rows[0][1] - 1.0).abs() < 1e-6);
        assert!(rows[0][2].abs() < 1e-6);
        assert_eq!(rows[0][3], 1.0 / 128.0);
        assert_eq!(rows[1], [0.5f32.powf(2.2), 0.25f32.powf(2.2), 1.0, 1.0]);
        assert_eq!(rows[2], [12.0, -2.0, 0.125, 8.0]);
        let mut disabled = material();
        disabled.EmitEnabled = false;
        disabled.EmittanceColor = None;
        disabled.UseAdaptativeEmissive = Some(true);
        disabled.TerrainThresholdFalloff = Some(-1.0);
        disabled.TerrainTilingDistance = Some(0.0);
        let rows = material_rows(&disabled).unwrap();
        assert_eq!(rows[0][0], 0.0001);
        assert_eq!(rows[0][3].to_bits(), 0x3B40_0000);
        assert_eq!(rows[1], [0.0; 4]);
        assert_eq!(rows[2], [12.0, -2.0, 0.125, 8.0]);
    }

    #[test]
    fn raw_terrain_rows_reject_missing_native_contract() {
        let mut source = material();
        source.PBR = Some(false);
        assert!(material_rows(&source).is_err());
        source.PBR = Some(true);
        source.TerrainRotationAngle = Some(f32::NAN);
        assert!(material_rows(&source).is_err());
        source.TerrainRotationAngle = Some(0.0);
        source.LumEmittance = None;
        assert!(material_rows(&source).is_err());
    }

    #[test]
    fn only_the_btd4_off_usages_become_records() {
        use super::super::manifest::TextureBundle;
        // Shaped like build_manifest output: one bundle per LTEX, normalized key, no GCVR.
        let bundle = |ltex: &str| TextureBundle {
            source_ltex_form_key: ltex.into(),
            ..Default::default()
        };
        let mut manifest = TextureManifest {
            textures: vec![
                bundle("SeventySix.esm:000100"),
                bundle("SeventySix.esm:000200"),
                bundle("SeventySix.esm:000300"),
            ],
            ..TextureManifest::default()
        };
        let usage = |ltex: &str, gcvr: Option<&str>| RequiredTextureUsage {
            ltex_form_key: ltex.into(),
            ground_cover_form_key: gcvr.map(Into::into),
        };
        mark_raw_bank_only(&mut manifest, &[
            usage("000100:SeventySix.esm", None),
            usage("000200:SeventySix.esm", Some("000900:SeventySix.esm")),
        ])
        .unwrap();
        let flags: Vec<_> = manifest.textures.iter().map(|bundle| bundle.raw_bank_only).collect();
        assert_eq!(flags, [false, false, true]);

        let error = mark_raw_bank_only(&mut manifest, &[usage("000400:SeventySix.esm", None)])
            .unwrap_err();
        assert!(error.contains("SeventySix.esm:000400"), "{error}");
    }
}
