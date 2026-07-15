"""Realtime material sphere preview backed by the shared editor renderer."""

from __future__ import annotations

import math
import time
from pathlib import Path
from types import SimpleNamespace

import glm
import moderngl
import numpy as np

from creation_lib.renderer.camera import OrbitCamera
from creation_lib.renderer.lighting import LightingSetup
from creation_lib.renderer.scene_renderer import Material, Mesh, SceneNode, SceneRenderer
from creation_lib.renderer.simple_renderer import compute_tangents
from creation_lib.renderer import material_pipeline
from creation_lib.textures.texture_dirs import build_texture_dirs, create_ba2_manager


_FO76_BGSM_VERSION = 20


def _preview_game_id(active_game: str, file_type: str, version: int) -> tuple[str, str]:
    if file_type == "bgsm" and version >= _FO76_BGSM_VERSION:
        return "fo76", f"BGSM version {version} selects FO76 shading"
    if active_game == "skyrimse":
        return "skyrimse", "Active game selects Skyrim SE shading"
    if active_game == "fo76":
        return "fo76", "Active game selects FO76 shading"
    return "fo4", "Active game selects FO4 shading"


def _to_vec3(value, default: tuple[float, float, float] = (1.0, 1.0, 1.0)) -> glm.vec3:
    if isinstance(value, (tuple, list)) and len(value) >= 3:
        return glm.vec3(float(value[0]), float(value[1]), float(value[2]))
    return glm.vec3(*default)


def _bool(fields_dict: dict[str, object], key: str, default: bool = False) -> bool:
    return bool(fields_dict.get(key, default))


def _float(fields_dict: dict[str, object], key: str, default: float = 0.0) -> float:
    try:
        return float(fields_dict.get(key, default) or 0.0)
    except (TypeError, ValueError):
        return default


def _texture(
    ctx: moderngl.Context, texture_dirs: list[Path], rel_path: str, ba2_mgr=None
) -> tuple[moderngl.Texture | None, str | None]:
    if not rel_path:
        return None, None
    resolved = material_pipeline._resolve_texture_path(
        rel_path, texture_dirs, ba2_mgr=ba2_mgr
    )
    if resolved is None:
        return None, rel_path
    cache_key = str(resolved) if not isinstance(resolved, bytes) else rel_path
    return material_pipeline._cached_load(ctx, resolved, cache_key=cache_key), None


def _build_sphere_mesh(
    lat_segments: int = 40, lon_segments: int = 64
) -> tuple[np.ndarray, np.ndarray]:
    positions: list[list[float]] = []
    normals: list[list[float]] = []
    uvs: list[list[float]] = []
    indices: list[int] = []

    for y in range(lat_segments + 1):
        v = y / lat_segments
        theta = v * math.pi
        sin_theta = math.sin(theta)
        cos_theta = math.cos(theta)

        for x in range(lon_segments + 1):
            u = x / lon_segments
            phi = u * math.tau
            sin_phi = math.sin(phi)
            cos_phi = math.cos(phi)

            px = sin_theta * cos_phi
            py = sin_theta * sin_phi
            pz = cos_theta
            positions.append([px, py, pz])
            normals.append([px, py, pz])
            uvs.append([u, 1.0 - v])

    row = lon_segments + 1
    for y in range(lat_segments):
        for x in range(lon_segments):
            i0 = y * row + x
            i1 = i0 + 1
            i2 = i0 + row
            i3 = i2 + 1
            indices.extend((i0, i2, i1, i1, i2, i3))

    pos_arr = np.asarray(positions, dtype=np.float32)
    norm_arr = np.asarray(normals, dtype=np.float32)
    uv_arr = np.asarray(uvs, dtype=np.float32)
    face_arr = np.asarray(indices, dtype=np.int32).reshape(-1, 3)
    tangents = compute_tangents(pos_arr, norm_arr, uv_arr, face_arr)
    bitangents = np.cross(norm_arr, tangents).astype(np.float32)
    texcoord2 = np.zeros((len(pos_arr), 2), dtype=np.float32)
    colors = np.ones((len(pos_arr), 4), dtype=np.float32)

    vertex_data = np.hstack(
        [pos_arr, norm_arr, uv_arr, texcoord2, tangents, bitangents, colors]
    ).astype(np.float32)
    index_data = np.asarray(indices, dtype=np.uint32)
    return vertex_data, index_data


class MaterialPreviewRenderer:
    def __init__(self, toolkit_settings=None):
        self._toolkit_settings = toolkit_settings
        self._ctx: moderngl.Context | None = None
        self._renderer: SceneRenderer | None = None
        self._camera = OrbitCamera()
        self._camera.distance = 3.1
        self._camera.azimuth = 35.0
        self._camera.elevation = 18.0
        self._lighting = LightingSetup()
        self._lighting.set_preset("studio")
        self._lighting.skylight = True
        self._session = SimpleNamespace(game_profile=SimpleNamespace(id="fo4"))
        self._node: SceneNode | None = None
        self._status_lines: list[str] = []
        self._ba2_mgr = None
        self._ba2_game_id: str | None = None

    @property
    def status_lines(self) -> list[str]:
        return list(self._status_lines)

    def _ensure_renderer(self) -> bool:
        if self._renderer is not None:
            return True
        try:
            self._ctx = moderngl.get_context()
            self._renderer = SceneRenderer(self._ctx)
            self._renderer.init_shaders()
            self._renderer.bg_color = (0.11, 0.12, 0.14)
            self._renderer.grid_visible = False
            self._renderer.toggles.vertex_colors = False
            self._renderer.active_nif_session = self._session
            self._node = self._build_scene_node(self._ctx, self._renderer)
            self._renderer.scene_root = self._node
            return True
        except Exception as exc:
            self._status_lines = [f"Preview renderer unavailable: {exc}"]
            return False

    def _build_scene_node(
        self, ctx: moderngl.Context, renderer: SceneRenderer
    ) -> SceneNode:
        vertices, indices = _build_sphere_mesh()
        vbo = ctx.buffer(vertices.tobytes())
        ibo = ctx.buffer(indices.tobytes())
        program = renderer.programs["fo4"]
        fmt_parts = ["3f", "3f", "2f", "2f", "3f", "3f", "4f"]
        attr_names = [
            "in_position", "in_normal", "in_texcoord", "in_texcoord2",
            "in_tangent", "in_bitangent", "in_color",
        ]
        offset_parts = []
        for f_part, attr in zip(fmt_parts, attr_names):
            if attr in program:
                offset_parts.append(f_part)
            else:
                offset_parts.append(f"{int(f_part[:-1]) * 4}x")
        active_attrs = [a for a in attr_names if a in program]
        vao = ctx.vertex_array(
            program,
            [(vbo, " ".join(offset_parts), *active_attrs)],
            index_buffer=ibo,
        )
        mesh = Mesh(
            vao=vao,
            vbo=vbo,
            ibo=ibo,
            num_indices=len(indices),
            material=Material(),
            vbo_format="3f 3f 2f 2f 3f 3f 4f",
            vbo_attrs=[
                "in_position",
                "in_normal",
                "in_texcoord",
                "in_texcoord2",
                "in_tangent",
                "in_bitangent",
                "in_color",
            ],
        )
        node = SceneNode(name="PreviewSphere", block_id=-1, mesh=mesh)
        node.transform = glm.mat4(1.0)
        node.world_transform = glm.mat4(1.0)
        node.bound_center = glm.vec3(0.0, 0.0, 0.0)
        node.bound_radius = 1.0
        return node

    def render(
        self,
        width: int,
        height: int,
        file_type: str,
        version: int,
        fields_dict: dict[str, object],
        file_path: str | None = None,
    ) -> int | None:
        if width <= 0 or height <= 0:
            return None
        if not self._ensure_renderer() or self._renderer is None or self._node is None:
            return None

        active_game = "fo4"
        if self._toolkit_settings is not None:
            try:
                active_game = self._toolkit_settings.get_active_game()
            except Exception:
                pass
        render_game, shader_reason = _preview_game_id(active_game, file_type, version)
        from app.paths import get_app_root
        texture_dirs, user_archive_dirs, base_archive_dirs = build_texture_dirs(
            self._toolkit_settings,
            game_id=active_game,
            nif_path=file_path,
            mods_root=get_app_root() / "mods",
        )
        if self._ba2_game_id != active_game:
            self._ba2_mgr = create_ba2_manager(user_archive_dirs, base_archive_dirs)
            self._ba2_game_id = active_game
        self._renderer.update_default_env(texture_dirs, game_id=render_game)
        self._session.game_profile.id = render_game
        self._node.mesh.material = self._build_material(
            file_type, version, fields_dict, texture_dirs, render_game, shader_reason,
            self._ba2_mgr,
        )

        spin = glm.rotate(
            glm.mat4(1.0), time.perf_counter() * 0.2, glm.vec3(0.0, 0.0, 1.0)
        )
        tilt = glm.rotate(glm.mat4(1.0), glm.radians(-18.0), glm.vec3(1.0, 0.0, 0.0))
        model = spin * tilt
        self._node.transform = model
        self._node.world_transform = model

        self._renderer.ensure_fbo(width, height)
        self._renderer.render(self._camera, self._lighting)
        return self._renderer.get_fbo_texture_id()

    def _build_material(
        self,
        file_type: str,
        version: int,
        fields_dict: dict[str, object],
        texture_dirs: list[Path],
        render_game: str,
        shader_reason: str,
        ba2_mgr=None,
    ) -> Material:
        assert self._ctx is not None
        def tex(path: str):
            return _texture(self._ctx, texture_dirs, path, ba2_mgr)
        mat = Material()
        mat.double_sided = _bool(fields_dict, "two_sided")
        mat.uv_scale_offset = glm.vec4(
            _float(fields_dict, "u_scale", 1.0),
            _float(fields_dict, "v_scale", 1.0),
            _float(fields_dict, "u_offset", 0.0),
            _float(fields_dict, "v_offset", 0.0),
        )
        mat.alpha_threshold = _float(fields_dict, "alpha_test_ref", 128.0) / 255.0
        if _bool(fields_dict, "alpha_test"):
            mat.alpha_flags |= 1
        if (
            _float(fields_dict, "alpha", 1.0) < 0.999
            or int(fields_dict.get("alpha_blend_mode0", 0) or 0) > 1
        ):
            mat.alpha_flags |= 8

        missing: list[str] = []
        status = [
            f"Preview shader: {render_game.upper()} (BGSM/BGEM v{version})",
            shader_reason,
        ]

        if file_type == "bgsm":
            diffuse_path = str(fields_dict.get("DiffuseTexture", "") or "")
            normal_path = str(fields_dict.get("NormalTexture", "") or "")
            spec_path = str(
                (
                    fields_dict.get("SpecularTexture")
                    if version > 2
                    else fields_dict.get("SmoothSpecTexture")
                )
                or ""
            )
            glow_path = str(fields_dict.get("GlowTexture", "") or "")
            env_path = str(fields_dict.get("EnvmapTexture", "") or "")
            greyscale_path = str(fields_dict.get("GreyscaleTexture", "") or "")

            mat.spec_color = _to_vec3(fields_dict.get("SpecularColor"), (1.0, 1.0, 1.0))
            mat.spec_strength = (
                _float(fields_dict, "SpecularMult", 1.0)
                if _bool(fields_dict, "SpecularEnabled", True)
                else 0.0
            )
            mat.glossiness = _float(fields_dict, "Smoothness", 0.5)
            mat.fresnel_power = _float(fields_dict, "FresnelPower", 5.0)
            mat.has_emit = _bool(fields_dict, "EmitEnabled")
            mat.glow_color = _to_vec3(
                fields_dict.get("EmittanceColor"), (1.0, 1.0, 1.0)
            )
            mat.glow_mult = _float(fields_dict, "EmittanceMult", 1.0)
            mat.has_env_map = bool(env_path) or _bool(fields_dict, "env_mapping")
            mat.env_map_scale = (
                _float(fields_dict, "env_mapping_mask_scale", 1.0) or 1.0
            )
            mat.has_palette = _bool(fields_dict, "grayscale_to_palette_color")
            mat.palette_scale = _float(fields_dict, "GrayscaleToPaletteScale", 1.0)

            mat.diffuse_tex, unresolved = tex(diffuse_path)
            if unresolved:
                missing.append(unresolved)
            mat.normal_tex, unresolved = tex(normal_path)
            if unresolved:
                missing.append(unresolved)
            mat.spec_tex, unresolved = tex(spec_path)
            if unresolved:
                missing.append(unresolved)
            mat.glow_tex, unresolved = tex(glow_path)
            if unresolved:
                missing.append(unresolved)
            mat.env_tex, unresolved = tex(env_path)
            if unresolved:
                missing.append(unresolved)
            mat.greyscale_tex, unresolved = tex(greyscale_path)
            if unresolved:
                missing.append(unresolved)

            mat.has_glow_map = mat.glow_tex is not None
            status.append(f"Base: {diffuse_path or '(none)'}")
        else:
            base_path = str(fields_dict.get("BaseTexture", "") or "")
            normal_path = str(fields_dict.get("NormalTexture", "") or "")
            env_path = str(fields_dict.get("EnvmapTexture", "") or "")
            env_mask_path = str(fields_dict.get("EnvmapMaskTexture", "") or "")
            glow_path = str(fields_dict.get("GlowTexture", "") or "")
            spec_path = str(fields_dict.get("SpecularTexture", "") or "")
            greyscale_path = str(fields_dict.get("GrayscaleTexture", "") or "")

            mat.is_effect_shader = True
            mat.has_source_texture = bool(base_path)
            mat.diffuse_tex, unresolved = tex(base_path)
            if unresolved:
                missing.append(unresolved)
            mat.normal_tex, unresolved = tex(normal_path)
            if unresolved:
                missing.append(unresolved)
            mat.env_tex, unresolved = tex(env_path)
            if unresolved:
                missing.append(unresolved)
            mat.env_mask_tex, unresolved = tex(env_mask_path)
            if unresolved:
                missing.append(unresolved)
            mat.spec_tex, unresolved = tex(spec_path)
            if unresolved:
                missing.append(unresolved)
            mat.glow_tex, unresolved = tex(glow_path)
            if unresolved:
                missing.append(unresolved)
            mat.greyscale_tex, unresolved = tex(greyscale_path)
            if unresolved:
                missing.append(unresolved)

            base_color = _to_vec3(fields_dict.get("BaseColor"), (1.0, 1.0, 1.0))
            base_scale = _float(fields_dict, "BaseColorScale", 1.0)
            mat.emissive_color = glm.vec4(
                base_color.x * base_scale,
                base_color.y * base_scale,
                base_color.z * base_scale,
                1.0,
            )
            if "EmittanceColor" in fields_dict:
                emit = _to_vec3(fields_dict.get("EmittanceColor"), (1.0, 1.0, 1.0))
                mat.emissive_color = glm.vec4(emit.x, emit.y, emit.z, 1.0)
            mat.emissive_mult = max(_float(fields_dict, "BaseColorScale", 1.0), 0.001)
            mat.lighting_influence = _float(fields_dict, "LightingInfluence", 1.0)
            mat.use_falloff = _bool(fields_dict, "FalloffEnabled")
            mat.has_rgb_falloff = _bool(fields_dict, "FalloffColorEnabled")
            mat.has_env_map = _bool(fields_dict, "EnvironmentMapping") or bool(env_path)
            mat.env_reflection = (
                _float(fields_dict, "EnvironmentMappingMaskScale", 1.0) or 1.0
            )
            mat.has_glow_map = mat.glow_tex is not None
            status.append(f"Base: {base_path or '(none)'}")

        if render_game == "fo76":
            mat.material_model = "spec-gloss"

        if texture_dirs:
            status.append(f"Assets: {texture_dirs[0]}")
        else:
            status.append("Assets: no configured game data path")
        if missing:
            status.append(f"Missing: {missing[0]}")
        self._status_lines = status
        return mat
