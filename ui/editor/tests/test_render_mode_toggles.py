from types import SimpleNamespace

import glm
import numpy as np

from creation_lib.renderer.render_modes import RenderMode, RenderModeManager
from creation_lib.renderer.render_toggles import RenderToggles
from creation_lib.renderer.scene_renderer import SceneRenderer
from ui.editor.particles.runtime import ParticleDrawBatch


class _Uniform:
    value = None


class _SetterOnlyDepthContext:
    def __init__(self):
        self.depth_func_values = []
        self.wireframe_values = []

    @property
    def depth_func(self):
        raise NotImplementedError

    @depth_func.setter
    def depth_func(self, value):
        self.depth_func_values.append(value)

    @property
    def wireframe(self):
        return self.wireframe_values[-1] if self.wireframe_values else False

    @wireframe.setter
    def wireframe(self, value):
        self.wireframe_values.append(value)


class _WireframeBackend:
    def __init__(self):
        self.draw_calls = []

    def _setup_fo4_uniforms(self, prog, lighting, rm):
        pass

    def _draw_node(self, node, prog, pass_type, use_alt_vao=False):
        self.draw_calls.append((node, pass_type, use_alt_vao))


class _RenderContext:
    def __init__(self):
        self.screen = SimpleNamespace(use=lambda: None)

    def clear(self, *args):
        pass

    def enable(self, *args):
        pass

    def disable(self, *args):
        pass


class _Framebuffer:
    def use(self):
        pass


class _Camera:
    def get_view_matrix(self):
        return glm.mat4(1.0)

    def get_projection_matrix(self, aspect):
        return glm.mat4(1.0)

    def get_eye_position(self):
        return glm.vec3(0.0)


class _ParticleRenderer:
    def __init__(self, order_log=None):
        self.updated_batches = None
        self.render_calls = []
        self.clear_calls = 0
        self.order_log = order_log

    def update_batches(self, draw_batches):
        self.updated_batches = draw_batches

    def render(self, *args):
        if self.order_log is not None:
            self.order_log.append("particles")
        self.render_calls.append(args)

    def clear(self):
        self.clear_calls += 1


def _particle_render_scene_renderer(runtime, session_transform=None, backend=None):
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer.fbo = _Framebuffer()
    renderer.ctx = _RenderContext()
    renderer.toggles = RenderToggles()
    renderer.programs = {"fo4": object(), "effect": object()}
    renderer._fbo_size = (100, 100)
    renderer.render_mode_mgr = None
    renderer._ensure_backend = lambda game_id: None
    renderer.backend = backend or SimpleNamespace(supports_shadows=False)
    renderer.scene_root = None
    renderer.active_nif_session = None
    session_root = SimpleNamespace(
        world_transform=session_transform or glm.mat4(1.0)
    )
    session = SimpleNamespace(
        particle_runtime=runtime,
        scene_root=session_root,
        game_profile=None,
    )
    renderer.active_nif_registry = SimpleNamespace(all_sessions=lambda: [session])
    renderer.grid = None
    renderer.grid_visible = False
    renderer.particle_renderer = _ParticleRenderer()
    renderer.uv_checker_texture = None
    renderer.connect_points = None
    renderer.light_display = None
    renderer._show_collision = False
    renderer.selection_mgr = None
    renderer._ssao_enabled = False
    renderer._shadow_enabled = False
    renderer._current_effect_prog = None
    return renderer


def _particle_batch(positions, emitter_object_block_id=None):
    return ParticleDrawBatch(
        nif_id="child",
        system_block_id=7,
        emitter_object_block_id=emitter_object_block_id,
        shader_property_block_id=None,
        alpha_property_block_id=None,
        positions=np.array(positions, dtype=np.float32),
        velocities=np.zeros((len(positions), 3), dtype=np.float32),
        colors=np.ones((len(positions), 4), dtype=np.float32),
        sizes=np.ones((len(positions),), dtype=np.float32),
        rotations=np.zeros((len(positions),), dtype=np.float32),
        atlas_indices=np.zeros((len(positions),), dtype=np.int32),
        atlas_offsets=np.array([(0.0, 1.0, 0.0, 1.0)], dtype=np.float32),
    )


def test_wireframe_pass_does_not_read_depth_func():
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer.ctx = _SetterOnlyDepthContext()
    renderer.backend = _WireframeBackend()
    program = {"toggle_lighting": _Uniform(), "toggle_diffuse": _Uniform()}

    renderer._draw_wireframe_pass("root", program, "opaque", None, None)

    assert renderer.backend.draw_calls == [("root", "opaque", False)]
    assert renderer.ctx.depth_func_values == ["<=", "<"]
    assert renderer.ctx.wireframe_values == [True, False]


def test_render_vertices_ignores_missing_scene_root():
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer.programs = {"vertex_points": object()}

    renderer.render_vertices(None, None, None)


def test_iter_particle_runtimes_skips_empty_state():
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer.active_nif_registry = None
    renderer.active_nif_session = None

    assert list(renderer._iter_particle_runtimes()) == []


def test_iter_particle_runtimes_uses_all_registry_sessions():
    first_runtime = SimpleNamespace(has_particles=True)
    second_runtime = SimpleNamespace(has_particles=True)
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer.active_nif_session = SimpleNamespace(particle_runtime=first_runtime)
    renderer.active_nif_registry = SimpleNamespace(
        all_sessions=lambda: [
            SimpleNamespace(particle_runtime=first_runtime),
            SimpleNamespace(particle_runtime=None),
            SimpleNamespace(particle_runtime=second_runtime),
        ]
    )

    assert [runtime for _, runtime in renderer._iter_particle_runtimes()] == [
        first_runtime,
        second_runtime,
    ]


def test_render_transforms_registry_child_particle_positions():
    batch = _particle_batch([[1.0, 2.0, 3.0]])
    runtime = SimpleNamespace(
        has_particles=True,
        build_draw_batches=lambda: [batch],
    )
    transform = glm.translate(glm.mat4(1.0), glm.vec3(10.0, 20.0, 30.0))
    renderer = _particle_render_scene_renderer(runtime, transform)

    renderer.render(_Camera(), lighting=None)

    updated_batch = renderer.particle_renderer.updated_batches[0]
    np.testing.assert_allclose(
        updated_batch.positions,
        np.array([[11.0, 22.0, 33.0]], dtype=np.float32),
    )
    np.testing.assert_allclose(batch.positions, np.array([[1.0, 2.0, 3.0]], dtype=np.float32))


def test_render_prefers_particle_system_node_world_transform():
    batch = _particle_batch([[1.0, 2.0, 3.0]])
    runtime = SimpleNamespace(
        has_particles=True,
        build_draw_batches=lambda: [batch],
    )
    root_transform = glm.translate(glm.mat4(1.0), glm.vec3(10.0, 20.0, 30.0))
    system_transform = glm.translate(glm.mat4(1.0), glm.vec3(100.0, 200.0, 300.0))
    renderer = _particle_render_scene_renderer(runtime, root_transform)
    session = renderer.active_nif_registry.all_sessions()[0]
    session.scene_root.children = [
        SimpleNamespace(block_id=7, world_transform=system_transform, children=[])
    ]

    renderer.render(_Camera(), lighting=None)

    updated_batch = renderer.particle_renderer.updated_batches[0]
    np.testing.assert_allclose(
        updated_batch.positions,
        np.array([[101.0, 202.0, 303.0]], dtype=np.float32),
    )


def test_render_prefers_particle_emitter_object_world_transform():
    batch = _particle_batch([[1.0, 2.0, 3.0]], emitter_object_block_id=178)
    runtime = SimpleNamespace(
        has_particles=True,
        build_draw_batches=lambda: [batch],
    )
    root_transform = glm.translate(glm.mat4(1.0), glm.vec3(10.0, 20.0, 30.0))
    system_transform = glm.translate(glm.mat4(1.0), glm.vec3(100.0, 200.0, 300.0))
    emitter_transform = glm.translate(glm.mat4(1.0), glm.vec3(0.0, 12.0, 4.75))
    renderer = _particle_render_scene_renderer(runtime, root_transform)
    session = renderer.active_nif_registry.all_sessions()[0]
    session.scene_root.children = [
        SimpleNamespace(
            block_id=7,
            world_transform=system_transform,
            children=[
                SimpleNamespace(block_id=178, world_transform=emitter_transform, children=[]),
            ],
        )
    ]

    renderer.render(_Camera(), lighting=None)

    updated_batch = renderer.particle_renderer.updated_batches[0]
    np.testing.assert_allclose(
        updated_batch.positions,
        np.array([[1.0, 14.0, 7.75]], dtype=np.float32),
    )


def test_render_clears_stale_particle_batches_when_runtime_returns_empty_batches():
    runtime = SimpleNamespace(
        has_particles=True,
        build_draw_batches=lambda: [],
    )
    renderer = _particle_render_scene_renderer(runtime)

    renderer.render(_Camera(), lighting=None)

    assert renderer.particle_renderer.updated_batches is None
    assert renderer.particle_renderer.render_calls == []
    assert renderer.particle_renderer.clear_calls == 1


def test_render_updates_and_draws_available_particle_batches():
    batch = _particle_batch([[1.0, 2.0, 3.0]])
    runtime = SimpleNamespace(
        has_particles=True,
        build_draw_batches=lambda: [batch],
    )
    renderer = _particle_render_scene_renderer(runtime)

    renderer.render(_Camera(), lighting=None)

    assert renderer.particle_renderer.updated_batches is not None
    assert len(renderer.particle_renderer.updated_batches) == 1
    assert len(renderer.particle_renderer.render_calls) == 1


def test_render_clears_particle_renderer_before_starfield_early_return():
    from creation_lib.renderer.backends.sf_backend import SfBackend

    backend = SfBackend.__new__(SfBackend)
    backend.has_scene = lambda: True
    backend.render_full_calls = []
    backend.render_full = lambda *args: backend.render_full_calls.append(args)
    runtime = SimpleNamespace(
        has_particles=True,
        build_draw_batches=lambda: [_particle_batch([[1.0, 2.0, 3.0]])],
    )
    renderer = _particle_render_scene_renderer(runtime, backend=backend)

    renderer.render(_Camera(), lighting=None)

    assert renderer.particle_renderer.clear_calls == 1
    assert renderer.particle_renderer.updated_batches is None
    assert renderer.particle_renderer.render_calls == []
    assert len(backend.render_full_calls) == 1


def test_render_draws_grid_before_particles_before_transparent_pass():
    from creation_lib.renderer.backends.fo4_backend import Fo4Backend

    order_log = []
    backend = Fo4Backend.__new__(Fo4Backend)
    backend._setup_fo4_uniforms = lambda *args: None

    def draw_node(node, prog, pass_type, use_alt_vao=False):
        if pass_type == "transparent":
            order_log.append("transparent")

    backend._draw_node = draw_node
    batch = _particle_batch([[1.0, 2.0, 3.0]])
    runtime = SimpleNamespace(
        has_particles=True,
        build_draw_batches=lambda: [batch],
    )
    renderer = _particle_render_scene_renderer(runtime, backend=backend)
    renderer.scene_root = SimpleNamespace()
    renderer.grid = SimpleNamespace(render=lambda vp: order_log.append("grid"))
    renderer.grid_visible = True
    renderer.particle_renderer = _ParticleRenderer(order_log)

    renderer.render(_Camera(), lighting=None)

    assert order_log == ["grid", "particles", "transparent"]


def test_render_mode_toggles_are_independent():
    manager = RenderModeManager(renderer=None)

    manager.toggle_mode(RenderMode.WIREFRAME)
    assert manager.is_enabled(RenderMode.TEXTURED)
    assert manager.is_enabled(RenderMode.WIREFRAME)

    manager.toggle_mode(RenderMode.TEXTURED)
    assert not manager.is_enabled(RenderMode.TEXTURED)
    assert manager.is_enabled(RenderMode.WIREFRAME)


def test_set_mode_keeps_legacy_exclusive_semantics():
    manager = RenderModeManager(renderer=None)

    manager.set_mode(RenderMode.WIREFRAME)

    assert not manager.is_enabled(RenderMode.TEXTURED)
    assert manager.is_enabled(RenderMode.WIREFRAME)


def test_unlit_can_combine_with_textured():
    manager = RenderModeManager(renderer=None)

    manager.toggle_mode(RenderMode.UNLIT)

    assert manager.is_enabled(RenderMode.TEXTURED)
    assert manager.is_enabled(RenderMode.UNLIT)
    assert manager.get_label() == "Textured, Unlit"


def test_unlit_does_not_enable_textured_base():
    manager = RenderModeManager(renderer=None)

    manager.toggle_mode(RenderMode.TEXTURED)
    manager.toggle_mode(RenderMode.UNLIT)

    assert not manager.is_enabled(RenderMode.TEXTURED)
    assert manager.is_enabled(RenderMode.UNLIT)
    assert not manager.should_draw_textured_base()


def test_uv_checker_wins_over_textured_base():
    manager = RenderModeManager(renderer=None)

    manager.toggle_mode(RenderMode.UV_CHECKER)

    assert manager.is_enabled(RenderMode.TEXTURED)
    assert manager.is_enabled(RenderMode.UV_CHECKER)
    assert not manager.should_draw_textured_base()
