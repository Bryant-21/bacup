from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from creation_lib.renderer.fo4_material import FO4MaterialBackend
from creation_lib.renderer.material_pipeline import (
    _get_texture_paths,
    _parse_bgsm,
    _resolve_texture_path,
    collect_nif_material_paths,
    collect_nif_texture_paths,
    invalidate_material_cache,
)


def test_resolve_texture_path_strips_data_prefix_for_loose_files(tmp_path):
    texture_file = (
        tmp_path
        / "textures"
        / "terrain"
        / "nukaworld"
        / "nukaworld.4.0.0.dds"
    )
    texture_file.parent.mkdir(parents=True)
    texture_file.write_bytes(b"dds")

    resolved = _resolve_texture_path(
        r"Data\Textures\Terrain\NukaWorld\NukaWorld.4.0.0.DDS",
        [tmp_path],
        None,
    )

    assert resolved == texture_file


def test_collect_nif_texture_paths_includes_legacy_nitrishape(monkeypatch):
    nif = MagicMock()
    shape = MagicMock()
    shader = MagicMock()
    tex_set = MagicMock()
    shape.type_name = "NiTriShape"
    shader.type_name = "BSShaderPPLightingProperty"
    nif.blocks = [shape]
    nif.schema.is_subtype_of.side_effect = lambda t, base: t == base
    shape.get_field.side_effect = lambda k: {"Shader Property": 1}.get(k)
    shader.get_field.side_effect = lambda k: {"Texture Set": 2}.get(k)
    tex_set.get_field.side_effect = lambda k: {
        "Textures": [
            r"textures\armor\legacy_d.dds",
            r"textures\armor\legacy_n.dds",
            "",
            "",
            "",
            "",
        ]
    }.get(k)
    nif.get_block.side_effect = lambda block_id: {1: shader, 2: tex_set}.get(block_id)

    monkeypatch.setattr(
        "creation_lib.renderer.material_pipeline._resolve_texture_path",
        lambda path_str, texture_dirs, ba2_mgr: Path("C:/fake") / Path(path_str).name,
    )

    result = collect_nif_texture_paths(nif, [], None)

    assert str(Path("C:/fake") / "legacy_d.dds") in result
    assert str(Path("C:/fake") / "legacy_n.dds") in result


def test_collect_nif_texture_paths_uses_legacy_properties_array(monkeypatch):
    nif = MagicMock()
    shape = MagicMock()
    shader = MagicMock()
    tex_set = MagicMock()
    shape.type_name = "NiTriShape"
    shader.type_name = "BSShaderPPLightingProperty"
    nif.blocks = [shape]
    nif.schema.is_subtype_of.side_effect = (
        lambda type_name, base: type_name == base
        or (type_name == "BSShaderPPLightingProperty" and base == "BSShaderProperty")
    )
    shape.get_field.side_effect = lambda key: {"Properties": [1]}.get(key)
    shader.get_field.side_effect = lambda key: {"Texture Set": 2}.get(key)
    tex_set.get_field.side_effect = lambda key: {
        "Textures": [
            r"textures\armor\legacy_props_d.dds",
            r"textures\armor\legacy_props_n.dds",
        ]
    }.get(key)
    nif.get_block.side_effect = lambda block_id: {1: shader, 2: tex_set}.get(block_id)

    monkeypatch.setattr(
        "creation_lib.renderer.material_pipeline._resolve_texture_path",
        lambda path_str, texture_dirs, ba2_mgr: Path("C:/fake") / Path(path_str).name,
    )

    result = collect_nif_texture_paths(nif, [], None)

    assert str(Path("C:/fake") / "legacy_props_d.dds") in result
    assert str(Path("C:/fake") / "legacy_props_n.dds") in result


def test_collect_nif_material_paths_includes_loose_bgsm(monkeypatch):
    nif = MagicMock()
    shape = MagicMock()
    shader = MagicMock()
    shape.type_name = "BSTriShape"
    shader.type_name = "BSLightingShaderProperty"
    nif.blocks = [shape]
    nif.schema.is_subtype_of.side_effect = lambda type_name, base: type_name == base
    shape.get_field.side_effect = lambda key: {"Shader Property": 1}.get(key)
    shader.get_field.side_effect = lambda key: {
        "Name": "Weapons/Test/TestMaterial.bgsm",
    }.get(key)
    nif.get_block.side_effect = lambda block_id: {1: shader}.get(block_id)
    material_path = Path("C:/fake/Materials/Weapons/Test/TestMaterial.bgsm")

    def resolve(path_str, texture_dirs, ba2_mgr):
        if path_str == "Materials/Weapons/Test/TestMaterial.bgsm":
            return material_path
        return None

    monkeypatch.setattr(
        "creation_lib.renderer.material_pipeline._resolve_texture_path",
        resolve,
    )

    result = collect_nif_material_paths(nif, [], None)

    assert result == {
        str(material_path): "weapons/test/testmaterial.bgsm",
    }


def test_invalidate_material_cache_removes_entries_for_changed_file():
    from creation_lib.renderer import material_pipeline as mp

    material_path = Path("C:/fake/Materials/Weapons/Test/TestMaterial.bgsm")
    material_key = "weapons/test/testmaterial.bgsm"

    with (
        patch.dict(
            mp._material_cache,
            {material_key: ({"diffuse": "a.dds"}, {})},
            clear=True,
        ),
        patch.dict(
            mp._material_cache_sources,
            {material_key: str(material_path)},
            clear=True,
        ),
    ):
        removed = invalidate_material_cache(str(material_path))

        assert removed == [material_key]
        assert material_key not in mp._material_cache
        assert material_key not in mp._material_cache_sources


def test_parse_bgsm_exposes_fo76_texture_slots(monkeypatch):
    header = SimpleNamespace(grayscale_to_palette_color=False, version=22)
    data = SimpleNamespace(
        header=header,
        DiffuseTexture="Landscape/Test_d.dds\x00",
        NormalTexture="Landscape/Test_n.dds\x00",
        SmoothSpecTexture="Landscape/Test_s.dds\x00",
        GreyscaleTexture="Landscape/Test_gs.dds\x00",
        EnvmapTexture="Shared/Cubemaps/Test.dds\x00",
        GlowTexture="Landscape/Test_g.dds\x00",
        InnerLayerTexture="Landscape/Test_inner.dds\x00",
        WrinklesTexture="Landscape/Test_wrinkles.dds\x00",
        DisplacementTexture="Landscape/Test_disp.dds\x00",
        SpecularTexture="Landscape/Test_r.dds\x00",
        LightingTexture="Landscape/Test_l.dds\x00",
        FlowTexture="Landscape/Test_flow.dds\x00",
        DistanceFieldAlphaTexture="Landscape/Test_df.dds\x00",
        GrayscaleToPaletteScale=1.0,
        SpecularColor=(1.0, 1.0, 1.0),
        SpecularMult=1.0,
        Smoothness=1.0,
        FresnelPower=5.0,
        EmitEnabled=False,
        EmittanceColor=(0.0, 0.0, 0.0),
        EmittanceMult=1.0,
        LumEmittance=100.0,
        Glowmap=False,
        PBR=True,
        Translucency=True,
        TranslucencySubsurfaceColor=(0.2, 0.4, 0.6),
        TranslucencyTransmissiveScale=0.75,
    )
    monkeypatch.setattr(
        "creation_lib.material_tools.bgsm_bin.read_bgsm",
        lambda _stream: data,
    )

    parsed = _parse_bgsm(b"fake-bgsm")

    assert parsed["reflectivity"] == "Landscape/Test_r.dds"
    assert parsed["lighting"] == "Landscape/Test_l.dds"
    assert parsed["inner_layer"] == "Landscape/Test_inner.dds"
    assert parsed["wrinkles"] == "Landscape/Test_wrinkles.dds"
    assert parsed["displacement"] == "Landscape/Test_disp.dds"
    assert parsed["flow"] == "Landscape/Test_flow.dds"
    assert parsed["distance_field_alpha"] == "Landscape/Test_df.dds"
    assert parsed["lum_emittance"] == 100.0
    assert parsed["subsurface_enabled"] is True
    assert parsed["subsurface_color"] == (0.2, 0.4, 0.6)
    assert parsed["subsurface_scale"] == 0.75
    assert parsed["pbr"] is True


def test_material_cache_is_scoped_to_resolved_material_source(monkeypatch):
    from creation_lib.renderer import material_pipeline as mp

    shader = MagicMock()
    shader.get_field.side_effect = lambda key: {
        "Name": "Materials/Landscape/Fissure/FissureRockSlab01.bgsm",
    }.get(key)
    fo4_source = Path("C:/fo4/materials/landscape/fissure/fissurerockslab01.bgsm")
    fo76_source = Path("C:/fo76/materials/landscape/fissure/fissurerockslab01.bgsm")
    calls = []

    def resolve(_mat_name, texture_dirs, _ba2_mgr):
        return fo76_source if str(texture_dirs[0]).endswith("fo76") else fo4_source

    def parse(resolved):
        calls.append(resolved)
        if resolved == fo76_source:
            return {
                "type": "bgsm",
                "pbr": True,
                "diffuse": "fo76_d.dds",
                "normal": "fo76_n.dds",
                "smooth_spec": "",
                "reflectivity": "fo76_r.dds",
                "lighting": "fo76_l.dds",
            }
        return {
            "type": "bgsm",
            "pbr": False,
            "diffuse": "fo4_d.dds",
            "normal": "fo4_n.dds",
            "smooth_spec": "",
            "reflectivity": "fo4_s.dds",
            "glow": "fo4_g.dds",
        }

    with (
        patch.dict(mp._material_cache, {}, clear=True),
        patch.dict(mp._material_cache_sources, {}, clear=True),
    ):
        monkeypatch.setattr(mp, "_resolve_material_file", resolve)
        monkeypatch.setattr(mp, "_parse_bgsm", parse)

        fo4_paths = _get_texture_paths(None, shader, "BSLightingShaderProperty", [Path("C:/fo4")])
        fo76_paths = _get_texture_paths(None, shader, "BSLightingShaderProperty", [Path("C:/fo76")])

    assert calls == [fo4_source, fo76_source]
    assert fo4_paths["glow"] == "fo4_g.dds"
    assert fo4_paths["specular"] == "fo4_s.dds"
    assert fo76_paths["lighting"] == "fo76_l.dds"
    assert fo76_paths["reflectivity"] == "fo76_r.dds"
    assert fo76_paths["specular"] == ""
    assert fo76_paths["glow"] == ""


def test_fo76_bgsm_infers_optional_rgb_emissive_sibling(monkeypatch):
    from creation_lib.renderer import material_pipeline as mp

    shader = MagicMock()
    shader.get_field.side_effect = lambda key: {
        "Name": "Materials/Landscape/Fissure/FissureRockSlab01.bgsm",
    }.get(key)
    material_source = Path("C:/fo76/materials/landscape/fissure/fissurerockslab01.bgsm")

    def resolve_texture(path_str, _texture_dirs, _ba2_mgr):
        if path_str == "Landscape/Fissure/FissureRockSlab01_g.dds":
            return Path("C:/fo76/textures/landscape/fissure/fissurerockslab01_g.dds")
        return None

    with (
        patch.dict(mp._material_cache, {}, clear=True),
        patch.dict(mp._material_cache_sources, {}, clear=True),
    ):
        monkeypatch.setattr(mp, "_resolve_material_file", lambda *_args: material_source)
        monkeypatch.setattr(
            mp,
            "_parse_bgsm",
            lambda _resolved: {
                "type": "bgsm",
                "pbr": True,
                "diffuse": "Landscape/Fissure/FissureRockSlab01_d.dds",
                "normal": "Landscape/Fissure/FissureRockSlab01_n.dds",
                "smooth_spec": "",
                "glow": "",
            },
        )
        monkeypatch.setattr(mp, "_resolve_texture_path", resolve_texture)

        paths = _get_texture_paths(
            None, shader, "BSLightingShaderProperty", [Path("C:/fo76")]
        )

    assert paths["glow"] == "Landscape/Fissure/FissureRockSlab01_g.dds"


def test_fo4_material_backend_reads_legacy_properties_shader_and_alpha():
    nif = MagicMock()
    shape = MagicMock()
    shader = MagicMock()
    tex_set = MagicMock()
    alpha = MagicMock()
    shape.type_name = "NiTriShape"
    shader.type_name = "BSShaderPPLightingProperty"
    alpha.type_name = "NiAlphaProperty"
    nif.schema.is_subtype_of.side_effect = (
        lambda type_name, base: type_name == base
        or (type_name == "BSShaderPPLightingProperty" and base == "BSShaderProperty")
    )
    shape.get_field.side_effect = lambda key: {"Properties": [1, 2]}.get(key)
    shader.get_field.side_effect = lambda key: {
        "Specular Color": {"r": 0.5, "g": 0.6, "b": 0.7},
        "Specular Strength": 2.5,
        "Glossiness": 40.0,
        "Fresnel Power": 3.0,
        "UV Scale": {"U": 1.0, "V": 1.0},
        "UV Offset": {"U": 0.0, "V": 0.0},
        "Shader Flags 1": [],
        "Shader Flags 2": [],
        "Texture Set": 3,
    }.get(key)
    tex_set.get_field.side_effect = lambda key: {"Textures": []}.get(key)
    alpha.get_field.side_effect = lambda key: {
        "Flags": 3821,
        "Threshold": 128,
    }.get(key)
    nif.get_block.side_effect = lambda block_id: {
        1: shader,
        2: alpha,
        3: tex_set,
    }.get(block_id)

    backend = FO4MaterialBackend()

    material = backend.build_material(MagicMock(), nif, shape, [], None)

    assert material.spec_strength == 2.5
    assert material.alpha_flags == 11
    assert material.alpha_threshold == 128 / 255.0
    assert material.blend_src == 6
    assert material.blend_dst == 7


def test_texture_cache_eviction_does_not_release_live_texture(monkeypatch):
    from creation_lib.renderer import material_pipeline as mp

    class _Texture:
        def __init__(self, name):
            self.name = name
            self.released = False

        def use(self, _unit):
            pass

        def release(self):
            self.released = True

    textures = {}

    def load_texture(_ctx, path):
        tex = _Texture(path)
        textures[path] = tex
        return tex

    with (
        patch.dict(mp._tex_cache, {}, clear=True),
        patch.dict(mp._decode_cache, {}, clear=True),
    ):
        monkeypatch.setattr(mp, "_MAX_TEX_CACHE", 1)
        monkeypatch.setattr(mp, "load_texture", load_texture)

        first = mp._cached_load(object(), Path("first.dds"))
        second = mp._cached_load(object(), Path("second.dds"))

    assert first.released is False
    assert second.released is False
    assert first is textures[str(Path("first.dds"))]
    assert second is textures[str(Path("second.dds"))]


def test_texture_cache_reloads_invalid_texture(monkeypatch):
    from creation_lib.renderer import material_pipeline as mp

    class _InvalidTexture:
        mglo = object()

        def use(self, _unit):
            raise AssertionError("invalid texture was reused")

    class _Texture:
        def use(self, _unit):
            pass

    replacement = _Texture()
    key = str(Path("invalid.dds"))

    with (
        patch.dict(mp._tex_cache, {key: _InvalidTexture()}, clear=True),
        patch.dict(mp._decode_cache, {}, clear=True),
    ):
        monkeypatch.setattr(mp, "load_texture", lambda _ctx, _path: replacement)

        loaded = mp._cached_load(object(), Path("invalid.dds"))

        assert loaded is replacement
        assert mp._tex_cache[key] is replacement
