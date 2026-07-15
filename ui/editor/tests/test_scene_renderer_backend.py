from unittest.mock import sentinel

from creation_lib.renderer.scene_renderer import Material, SceneRenderer
from creation_lib.renderer.shader_pipeline import load_composed_shader
from ui.editor.panels.toolbar_tbr import GAME_ID_TO_PRESET, LIGHTING_PRESETS


def _make_renderer():
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer.backend = None
    renderer._backend_game_id = None
    renderer.programs = {}
    renderer.ctx = object()
    renderer._ensure_game_ibl = lambda game_id: None
    return renderer


def test_ensure_backend_keeps_loading_when_shader_compile_fails(monkeypatch):
    renderer = _make_renderer()

    def fail_compile(ctx, role, game_id):
        raise FileNotFoundError(f"missing shader for {game_id}/{role}")

    monkeypatch.setattr(
        "creation_lib.renderer.shader_pipeline.load_composed_shader",
        fail_compile,
    )
    monkeypatch.setattr(
        "creation_lib.renderer.backends.make_backend",
        lambda game_id, renderer: sentinel.backend,
    )

    renderer._ensure_backend("fnv")

    assert renderer.backend is sentinel.backend
    assert renderer._backend_game_id == "fnv"
    assert renderer.programs == {}


def test_ensure_backend_uses_shared_legacy_shader_family(monkeypatch):
    renderer = _make_renderer()
    shader_calls = []

    def load_shader(ctx, role, game_id):
        shader_calls.append((role, game_id))
        return f"{game_id}:{role}"

    monkeypatch.setattr(
        "creation_lib.renderer.shader_pipeline.load_composed_shader",
        load_shader,
    )
    monkeypatch.setattr(
        "creation_lib.renderer.backends.make_backend",
        lambda game_id, renderer: sentinel.backend,
    )

    renderer._ensure_backend("fnv")

    assert shader_calls == [("default", "gamebryo"), ("effect", "gamebryo")]
    assert renderer.programs["fnv"] == "gamebryo:default"
    assert renderer.programs["fnv_effect"] == "gamebryo:effect"


def test_gamebryo_shader_matches_shared_viewport_contract():
    class _CapturingContext:
        def program(self, *, vertex_shader, fragment_shader):
            assert "out vec2  vTexCoord;" in vertex_shader
            assert "out vec3  vNormalWorld;" in vertex_shader
            assert "out vec3  vPosWorld;" in vertex_shader
            assert "out vec3  vTangentWorld;" in vertex_shader
            assert "out vec3  vBitangentWorld;" in vertex_shader
            assert "out vec4  vVertexColor;" in vertex_shader
            assert "out vec3  vNormalView;" in vertex_shader

            assert "uniform sampler2D diffuseMap;" in fragment_shader
            assert "uniform sampler2D normalMap;" in fragment_shader
            assert "uniform sampler2D specMap;" in fragment_shader
            assert "uniform sampler2D envMap;" in fragment_shader
            assert "uniform sampler2D envMaskMap;" in fragment_shader
            assert "uniform sampler2D glowMap;" in fragment_shader
            assert "uniform float alphaThreshold;" in fragment_shader
            assert "uniform vec3 cameraPos;" in fragment_shader
            assert "in vec2 vTexCoord;" in fragment_shader
            assert "in vec3 vNormalWorld;" in fragment_shader
            assert "in vec3 vPosWorld;" in fragment_shader
            assert "in vec3 vTangentWorld;" in fragment_shader
            assert "in vec3 vBitangentWorld;" in fragment_shader
            assert "in vec4 vVertexColor;" in fragment_shader
            assert "in vec3 vNormalView;" in fragment_shader
            assert "layout(location = 0) out vec4 fragColor;" in fragment_shader
            assert "layout(location = 1) out vec4 fragNormal;" in fragment_shader

            assert "uniform sampler2D u_diffuse_tex;" not in fragment_shader
            assert "uniform sampler2D u_normal_tex;" not in fragment_shader
            assert "uniform sampler2D u_env_tex;" not in fragment_shader
            assert "uniform sampler2D u_env_mask_tex;" not in fragment_shader
            assert "uniform vec3 u_camera_pos;" not in fragment_shader
            assert "uniform float u_alpha_threshold;" not in fragment_shader
            assert "in vec3 v_world_pos;" not in fragment_shader
            assert "in vec3 v_world_normal;" not in fragment_shader
            assert "in vec2 v_uv;" not in fragment_shader
            assert "in vec4 v_color;" not in fragment_shader

            return sentinel.program

    assert load_composed_shader(_CapturingContext(), "default", "gamebryo") is sentinel.program


def test_fo76_shader_unpacks_lighting_map_as_material_channels():
    class _CapturingContext:
        def program(self, *, vertex_shader, fragment_shader):
            assert "uniform sampler2D   lightingMap;" in fragment_shader
            assert "float diffuseIntensity = dot(diffTex.rgb" in fragment_shader
            assert "float vertexIntensity = dot(vVertexColor.rgb" in fragment_shader
            assert "vec2 palUV = vec2(diffuseIntensity, paletteScale * vertexIntensity);" in fragment_shader
            assert "albedo = textureLod(greyscaleMap, palUV, 0.0).rgb;" in fragment_shader
            assert "vec4 lightingSample = vec4(0.0, 1.0, 0.0, 0.0);" in fragment_shader
            assert "vec3 lightingPacked = lightingSample.rgb;" in fragment_shader
            assert "roughness = clamp(1.0 - lightingPacked.r" in fragment_shader
            assert "float ambientOcclusion = clamp(lightingPacked.g" in fragment_shader
            assert "subsurfaceEnabled > 0.5" in fragment_shader
            assert "lightingPacked.b * subsurfaceScale" in fragment_shader
            assert "hasLightingEmissive > 0.5" in fragment_shader
            assert "emissive = lightingSample.a * tint" in fragment_shader
            assert "vec3 dielectricF0 = vec3(0.04);" in fragment_shader
            assert "texture(reflectivityMap, vTexCoord).rgb" in fragment_shader
            assert "vec3 F0 = mix(dielectricF0, albedo, metallic);" in fragment_shader
            assert "envMask = texture(reflectivityMap" not in fragment_shader
            assert "lightingSample.rgb * lightingMask" not in fragment_shader
            assert "lightingSpec * specStrength" not in fragment_shader
            assert "F0 = max(F0" not in fragment_shader
            return sentinel.program

    assert load_composed_shader(_CapturingContext(), "default", "fo76") is sentinel.program


def test_gamebryo_preset_covers_legacy_game_ids():
    assert "Gamebryo" in LIGHTING_PRESETS
    assert GAME_ID_TO_PRESET["oblivion"] == "Gamebryo"
    assert GAME_ID_TO_PRESET["fo3"] == "Gamebryo"
    assert GAME_ID_TO_PRESET["fnv"] == "Gamebryo"


def test_set_material_uniforms_assigns_env_mask_sampler_unit():
    class _Uniform:
        def __init__(self):
            self.value = None

    class _Texture:
        def __init__(self):
            self.used_units = []

        def use(self, unit):
            self.used_units.append(unit)

    renderer = _make_renderer()
    renderer.default_diffuse = None
    renderer.default_normal = None
    renderer.default_spec = None
    renderer.default_env = None
    renderer._logged_auto_env = False
    renderer._has_real_default_env = False

    env_mask_tex = _Texture()
    mat = Material()
    mat.env_mask_tex = env_mask_tex
    program = {"envMaskMap": _Uniform()}

    renderer._set_material_uniforms(program, mat)

    assert env_mask_tex.used_units == [4]
    assert program["envMaskMap"].value == 4


def test_set_material_uniforms_binds_fo76_lighting_and_reflectivity_maps():
    class _Uniform:
        def __init__(self):
            self.value = None

    class _Texture:
        def __init__(self):
            self.used_units = []

        def use(self, unit):
            self.used_units.append(unit)

    renderer = _make_renderer()
    renderer.default_diffuse = None
    renderer.default_normal = None
    renderer.default_spec = None
    renderer.default_env = None
    renderer._logged_auto_env = False
    renderer._has_real_default_env = False

    lighting_tex = _Texture()
    reflectivity_tex = _Texture()
    mat = Material()
    mat.textures["lighting"] = lighting_tex
    mat.textures["reflectivity"] = reflectivity_tex
    mat.lighting_has_emissive_alpha = True
    mat.subsurface_enabled = True
    mat.subsurface_scale = 0.5
    program = {
        "lightingMap": _Uniform(),
        "reflectivityMap": _Uniform(),
        "hasLightingMap": _Uniform(),
        "hasLightingEmissive": _Uniform(),
        "hasReflectivityMap": _Uniform(),
        "subsurfaceEnabled": _Uniform(),
        "subsurfaceScale": _Uniform(),
    }

    renderer._set_material_uniforms(program, mat)

    assert lighting_tex.used_units == [10]
    assert reflectivity_tex.used_units == [11]
    assert program["lightingMap"].value == 10
    assert program["reflectivityMap"].value == 11
    assert program["hasLightingMap"].value == 1.0
    assert program["hasLightingEmissive"].value == 1.0
    assert program["hasReflectivityMap"].value == 1.0
    assert program["subsurfaceEnabled"].value == 1.0
    assert program["subsurfaceScale"].value == 0.5


def test_set_material_uniforms_ignores_invalid_material_texture():
    class _Uniform:
        def __init__(self):
            self.value = None

    class _Texture:
        def __init__(self):
            self.used_units = []

        def use(self, unit):
            self.used_units.append(unit)

    class _InvalidTexture:
        mglo = object()

        def use(self, _unit):
            raise AssertionError("invalid texture was bound")

    renderer = _make_renderer()
    renderer.default_diffuse = _Texture()
    renderer.default_normal = None
    renderer.default_spec = None
    renderer.default_env = None
    renderer._logged_auto_env = False
    renderer._has_real_default_env = False

    mat = Material()
    mat.diffuse_tex = _InvalidTexture()
    program = {"diffuseMap": _Uniform()}

    renderer._set_material_uniforms(program, mat)

    assert renderer.default_diffuse.used_units == [0]
    assert program["diffuseMap"].value == 0
