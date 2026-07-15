from __future__ import annotations
from pathlib import Path

from creation_lib.material_tools.base import BaseHeader
from creation_lib.material_tools.bgsm_bin import BGSMData


def _write_bgsm(path: Path, *, cast_shadows: bool) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    header = BaseHeader(
        signature=0x4D534742,
        version=2,
        tile_u=False,
        tile_v=False,
        u_offset=0.0,
        v_offset=0.0,
        u_scale=1.0,
        v_scale=1.0,
        alpha=1.0,
        alpha_blend_mode0=0,
        alpha_blend_mode1=6,
        alpha_blend_mode2=7,
        alpha_test_ref=128,
        alpha_test=False,
        zbuffer_write=True,
        zbuffer_test=True,
        ssr=False,
        wet_ssr=False,
        decal=False,
        two_sided=False,
        decal_nofade=False,
        non_occluder=False,
        refraction=False,
        refraction_falloff=False,
        refraction_power=0.0,
        env_mapping=None,
        env_mapping_mask_scale=None,
        depth_bias=False,
        grayscale_to_palette_color=False,
        mask_writes=0,
    )
    bgsm = BGSMData(
        header=header,
        DiffuseTexture="",
        NormalTexture="",
        SmoothSpecTexture="",
        GreyscaleTexture="",
        EnvmapTexture=None,
        GlowTexture="",
        InnerLayerTexture=None,
        WrinklesTexture="",
        DisplacementTexture=None,
        SpecularTexture="",
        LightingTexture="",
        FlowTexture="",
        DistanceFieldAlphaTexture=None,
        EnableEditorAlphaRef=False,
        RimLighting=None,
        RimPower=None,
        BackLightPower=None,
        SubsurfaceLighting=None,
        SubsurfaceLightingRolloff=None,
        Translucency=False,
        TranslucencyThickObject=False,
        TranslucencyMixAlbedoWithSubsurfaceColor=False,
        TranslucencySubsurfaceColor=(1.0, 1.0, 1.0),
        TranslucencyTransmissiveScale=0.0,
        TranslucencyTurbulence=0.0,
        SpecularEnabled=True,
        SpecularColor=(1.0, 1.0, 1.0),
        SpecularMult=1.0,
        Smoothness=0.5,
        FresnelPower=5.0,
        WetnessControlSpecScale=1.0,
        WetnessControlSpecPowerScale=1.0,
        WetnessControlSpecMinvar=0.0,
        WetnessControlEnvMapScale=None,
        WetnessControlFresnelPower=1.0,
        WetnessControlMetalness=0.0,
        PBR=None,
        CustomPorosity=None,
        PorosityValue=None,
        RootMaterialPath="",
        AnisoLighting=False,
        EmitEnabled=False,
        EmittanceColor=None,
        EmittanceMult=1.0,
        ModelSpaceNormals=False,
        ExternalEmittance=False,
        LumEmittance=None,
        UseAdaptativeEmissive=None,
        AdaptativeEmissive_ExposureOffset=None,
        AdaptativeEmissive_FinalExposureMin=None,
        AdaptativeEmissive_FinalExposureMax=None,
        BackLighting=None,
        ReceiveShadows=True,
        HideSecret=False,
        CastShadows=cast_shadows,
        DissolveFade=False,
        AssumeShadowmask=False,
        Glowmap=False,
        EnvironmentMappingWindow=None,
        EnvironmentMappingEye=None,
        Hair=False,
        HairTintColor=(0.0, 0.0, 0.0),
        Tree=False,
        Facegen=False,
        SkinTint=False,
        Tessellate=False,
        DisplacementTextureBias=None,
        DisplacementTextureScale=None,
        TessellationPnScale=None,
        TessellationBaseFactor=None,
        TessellationFadeDistance=None,
        GrayscaleToPaletteScale=1.0,
        SkewSpecularAlpha=None,
        Terrain=None,
        UnkInt1=None,
        TerrainThresholdFalloff=None,
        TerrainTilingDistance=None,
        TerrainRotationAngle=None,
    )
    with path.open("wb") as handle:
        bgsm.write(handle)


def test_invariants_pass_for_clean_output(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "FalloutNV.esm").write_bytes(b"esm")
    (tmp_path / "data" / "Meshes" / "foo.nif").parent.mkdir(parents=True)
    (tmp_path / "data" / "Meshes" / "foo.nif").write_bytes(b"")
    (tmp_path / "data" / "Textures" / "foo.dds").parent.mkdir(parents=True)
    (tmp_path / "data" / "Textures" / "foo.dds").write_bytes(b"")
    _write_bgsm(tmp_path / "data" / "Materials" / "bar.bgsm", cast_shadows=True)
    (tmp_path / "data" / "Sound" / "music.wav").parent.mkdir(parents=True)
    (tmp_path / "data" / "Sound" / "music.wav").write_bytes(b"RIFF")

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["FalloutNV.esm"],
        source_prefix="fnv",
    )

    assert result.ok, result.failures
    assert result.failures == []


def test_invariants_accept_expected_esm_esp_and_esl_plugins(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "FalloutNV.esm").write_bytes(b"esm")
    (tmp_path / "DeadMoney.esp").write_bytes(b"esp")
    (tmp_path / "TinyPatch.esl").write_bytes(b"esl")

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["FalloutNV.esm", "DeadMoney.esp", "TinyPatch.esl"],
        source_prefix="fnv",
    )

    assert result.ok, result.failures


def test_invariants_ignore_dot_prefixed_internal_plugins(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "SeventySix.esm").write_bytes(b"esm")
    (tmp_path / ".regen_land_cache.esm").write_bytes(b"cache")

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["SeventySix.esm"],
        source_prefix="fo76",
    )

    assert result.ok, result.failures


def test_invariants_fail_when_bgsm_cast_shadows_is_false(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "FalloutNV.esm").write_bytes(b"esm")
    _write_bgsm(tmp_path / "data" / "Materials" / "bar.bgsm", cast_shadows=False)

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["FalloutNV.esm"],
        source_prefix="fnv",
    )

    assert not result.ok
    assert any("bCastShadows=False" in failure for failure in result.failures)


def test_invariants_fail_when_bgsm_is_not_binary(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "FalloutNV.esm").write_bytes(b"esm")
    bgsm_path = tmp_path / "data" / "Materials" / "json-disguised.bgsm"
    bgsm_path.parent.mkdir(parents=True)
    bgsm_path.write_text('{"bCastShadows": true}', encoding="utf-8")

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["FalloutNV.esm"],
        source_prefix="fnv",
    )

    assert not result.ok
    assert any("invalid BGSM" in failure for failure in result.failures)
    assert any("json-disguised.bgsm" in failure for failure in result.failures)


def test_invariants_threaded_bgsm_scan_reports_failures(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "FalloutNV.esm").write_bytes(b"esm")
    _write_bgsm(tmp_path / "data" / "Materials" / "ok.bgsm", cast_shadows=True)
    _write_bgsm(tmp_path / "data" / "Materials" / "bad.bgsm", cast_shadows=False)

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["FalloutNV.esm"],
        source_prefix="fnv",
        max_workers=2,
    )

    assert not result.ok
    assert result.failures == [
        f"BGSM with bCastShadows=False: {tmp_path / 'data' / 'Materials' / 'bad.bgsm'}"
    ]


def test_invariants_fail_when_assets_use_source_prefix(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "FalloutNV.esm").write_bytes(b"esm")
    (tmp_path / "data" / "Sound" / "fnv" / "music.wav").parent.mkdir(parents=True)
    (tmp_path / "data" / "Sound" / "fnv" / "music.wav").write_bytes(b"RIFF")

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["FalloutNV.esm"],
        source_prefix="fnv",
    )

    assert not result.ok
    assert any("asset inside source prefix" in failure for failure in result.failures)


def test_invariants_allow_explicit_asset_namespace_prefix(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    allowed_root = tmp_path / "allowed"
    (allowed_root / "SeventySix.esm").parent.mkdir(parents=True)
    (allowed_root / "SeventySix.esm").write_bytes(b"esm")
    (allowed_root / "data" / "Meshes" / "FO76" / "foo.nif").parent.mkdir(parents=True)
    (allowed_root / "data" / "Meshes" / "FO76" / "foo.nif").write_bytes(b"")

    allowed_result = check_run_invariants(
        allowed_root,
        expected_plugins=["SeventySix.esm"],
        source_prefix="fo76",
        allowed_asset_prefixes=("FO76",),
    )

    assert allowed_result.ok, allowed_result.failures

    blocked_root = tmp_path / "blocked"
    (blocked_root / "SeventySix.esm").parent.mkdir(parents=True)
    (blocked_root / "SeventySix.esm").write_bytes(b"esm")
    (blocked_root / "data" / "Meshes" / "fo76" / "bar.nif").parent.mkdir(parents=True)
    (blocked_root / "data" / "Meshes" / "fo76" / "bar.nif").write_bytes(b"")

    blocked_result = check_run_invariants(
        blocked_root,
        expected_plugins=["SeventySix.esm"],
        source_prefix="fo76",
        allowed_asset_prefixes=("FO76",),
    )

    assert not blocked_result.ok
    assert blocked_result.failures == [
        f"asset inside source prefix: {blocked_root / 'data' / 'Meshes' / 'fo76' / 'bar.nif'}"
    ]


def test_invariants_fail_when_expected_plugin_set_differs(tmp_path: Path) -> None:
    from bacup_lib.invariants import check_run_invariants

    (tmp_path / "DeadMoney.esm").write_bytes(b"esm")

    result = check_run_invariants(
        tmp_path,
        expected_plugins=["FalloutNV.esm"],
        source_prefix="fnv",
    )

    assert not result.ok
    assert any("plugin set mismatch" in failure for failure in result.failures)

