import numpy as np

from creation_lib.renderer.particle_renderer import ParticleRenderer, pack_particle_vertices
from ui.editor.particles.runtime import ParticleDrawBatch


def test_pack_particle_vertices_expands_each_particle_to_four_vertices():
    batch = ParticleDrawBatch(
        nif_id="main",
        system_block_id=1,
        shader_property_block_id=-1,
        alpha_property_block_id=-1,
        positions=np.array([[1.0, 2.0, 3.0]], dtype=np.float32),
        velocities=np.array([[0.0, 0.0, 1.0]], dtype=np.float32),
        colors=np.array([[1.0, 0.5, 0.25, 0.75]], dtype=np.float32),
        sizes=np.array([2.0], dtype=np.float32),
        rotations=np.array([0.0], dtype=np.float32),
        atlas_indices=np.array([0], dtype=np.int32),
        atlas_offsets=((0.0, 1.0, 0.0, 1.0),),
    )

    vertices, indices = pack_particle_vertices(batch)

    assert vertices.shape == (4, 13)
    assert indices.tolist() == [0, 1, 2, 0, 2, 3]
    assert vertices[0, 0:3].tolist() == [1.0, 2.0, 3.0]
    assert vertices[0, 7:11].tolist() == [1.0, 0.5, 0.25, 0.75]


def test_pack_particle_vertices_offsets_indices_and_selects_atlas_uvs():
    batch = ParticleDrawBatch(
        nif_id="main",
        system_block_id=1,
        shader_property_block_id=-1,
        alpha_property_block_id=-1,
        positions=np.array(
            [
                [1.0, 2.0, 3.0],
                [4.0, 5.0, 6.0],
            ],
            dtype=np.float32,
        ),
        velocities=np.array(
            [
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0],
            ],
            dtype=np.float32,
        ),
        colors=np.array(
            [
                [1.0, 0.5, 0.25, 0.75],
                [0.25, 0.5, 1.0, 0.5],
            ],
            dtype=np.float32,
        ),
        sizes=np.array([2.0, 3.0], dtype=np.float32),
        rotations=np.array([0.0, 0.5], dtype=np.float32),
        atlas_indices=np.array([1, 0], dtype=np.int32),
        atlas_offsets=(
            (0.0, 0.25, 0.0, 0.5),
            (0.5, 0.75, 0.25, 0.75),
        ),
    )

    vertices, indices = pack_particle_vertices(batch)

    assert vertices.shape == (8, 13)
    assert indices.tolist() == [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]
    assert vertices[0:4, 5:7].tolist() == [
        [0.5, 0.75],
        [0.75, 0.75],
        [0.75, 0.25],
        [0.5, 0.25],
    ]
    assert vertices[4:8, 5:7].tolist() == [
        [0.0, 0.5],
        [0.25, 0.5],
        [0.25, 0.0],
        [0.0, 0.0],
    ]
    assert vertices[4, 0:3].tolist() == [4.0, 5.0, 6.0]


def test_particle_renderer_uses_supplied_program():
    program = {}
    renderer = ParticleRenderer(object(), program)

    assert renderer._program is program


def test_particle_renderer_does_not_bind_optimized_atlas_index_attribute():
    class _Resource:
        def release(self):
            pass

    class _Context:
        def buffer(self, _data):
            return _Resource()

        def vertex_array(self, _program, bindings, _ibo):
            attrs = bindings[0][2:]
            assert "in_atlas_index" not in attrs
            return _Resource()

    batch = ParticleDrawBatch(
        nif_id="main",
        system_block_id=1,
        shader_property_block_id=-1,
        alpha_property_block_id=-1,
        positions=np.array([[1.0, 2.0, 3.0]], dtype=np.float32),
        velocities=np.array([[0.0, 0.0, 1.0]], dtype=np.float32),
        colors=np.array([[1.0, 0.5, 0.25, 0.75]], dtype=np.float32),
        sizes=np.array([2.0], dtype=np.float32),
        rotations=np.array([0.0], dtype=np.float32),
        atlas_indices=np.array([0], dtype=np.int32),
        atlas_offsets=((0.0, 1.0, 0.0, 1.0),),
    )
    renderer = ParticleRenderer(_Context(), {})

    renderer.update_batches([batch])


def test_pack_particle_vertices_wraps_out_of_range_atlas_index():
    batch = ParticleDrawBatch(
        nif_id="main",
        system_block_id=1,
        shader_property_block_id=-1,
        alpha_property_block_id=-1,
        positions=np.array([[1.0, 2.0, 3.0]], dtype=np.float32),
        velocities=np.array([[0.0, 0.0, 1.0]], dtype=np.float32),
        colors=np.array([[1.0, 0.5, 0.25, 0.75]], dtype=np.float32),
        sizes=np.array([2.0], dtype=np.float32),
        rotations=np.array([0.0], dtype=np.float32),
        atlas_indices=np.array([3], dtype=np.int32),
        atlas_offsets=(
            (0.0, 0.25, 0.0, 0.5),
            (0.5, 0.75, 0.25, 0.75),
        ),
    )

    vertices, _indices = pack_particle_vertices(batch)

    assert vertices[0:4, 5:7].tolist() == [
        [0.5, 0.75],
        [0.75, 0.75],
        [0.75, 0.25],
        [0.5, 0.25],
    ]


def test_particle_renderer_binds_particle_texture_per_batch():
    class _Uniform:
        def __init__(self):
            self.value = None

    class _Resource:
        def release(self):
            pass

    class _Vao(_Resource):
        def __init__(self):
            self.render_calls = 0

        def render(self, _mode):
            self.render_calls += 1

    class _Texture:
        def __init__(self):
            self.use_calls = []

        def use(self, unit):
            self.use_calls.append(unit)

    class _Context:
        def __init__(self):
            self.depth_mask = True
            self.blend_func = None
            self.vao = _Vao()

        def buffer(self, _data):
            return _Resource()

        def vertex_array(self, _program, _bindings, _ibo):
            return self.vao

        def enable(self, _mode):
            pass

        def disable(self, _mode):
            pass

    program = {
        "u_vp": _Uniform(),
        "u_camera_right": _Uniform(),
        "u_camera_up": _Uniform(),
        "ParticleTexture": _Uniform(),
        "hasParticleTexture": _Uniform(),
    }
    texture = _Texture()
    batch = ParticleDrawBatch(
        nif_id="main",
        system_block_id=1,
        shader_property_block_id=-1,
        alpha_property_block_id=-1,
        positions=np.array([[1.0, 2.0, 3.0]], dtype=np.float32),
        velocities=np.array([[0.0, 0.0, 1.0]], dtype=np.float32),
        colors=np.array([[1.0, 1.0, 1.0, 1.0]], dtype=np.float32),
        sizes=np.array([2.0], dtype=np.float32),
        rotations=np.array([0.0], dtype=np.float32),
        atlas_indices=np.array([0], dtype=np.int32),
        atlas_offsets=((0.0, 1.0, 0.0, 1.0),),
        texture=texture,
    )
    ctx = _Context()
    renderer = ParticleRenderer(ctx, program)

    renderer.update_batches([batch])
    renderer.render(
        tuple(np.eye(4, dtype=np.float32).reshape(-1)),
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
    )

    assert texture.use_calls == [0]
    assert program["ParticleTexture"].value == 0
    assert program["hasParticleTexture"].value is True
    assert ctx.vao.render_calls == 1


def test_particle_renderer_binds_greyscale_palette_texture_per_batch():
    class _Uniform:
        def __init__(self):
            self.value = None

    class _Resource:
        def release(self):
            pass

    class _Vao(_Resource):
        def render(self, _mode):
            pass

    class _Texture:
        def __init__(self):
            self.use_calls = []

        def use(self, unit):
            self.use_calls.append(unit)

    class _Context:
        def __init__(self):
            self.depth_mask = True
            self.blend_func = None

        def buffer(self, _data):
            return _Resource()

        def vertex_array(self, _program, _bindings, _ibo):
            return _Vao()

        def enable(self, _mode):
            pass

        def disable(self, _mode):
            pass

    source_texture = _Texture()
    greyscale_texture = _Texture()
    program = {
        "u_vp": _Uniform(),
        "u_camera_right": _Uniform(),
        "u_camera_up": _Uniform(),
        "ParticleTexture": _Uniform(),
        "GreyscaleTexture": _Uniform(),
        "hasParticleTexture": _Uniform(),
        "hasGreyscaleTexture": _Uniform(),
        "greyscaleColor": _Uniform(),
        "greyscaleAlpha": _Uniform(),
    }
    batch = ParticleDrawBatch(
        nif_id="main",
        system_block_id=1,
        shader_property_block_id=-1,
        alpha_property_block_id=-1,
        positions=np.array([[1.0, 2.0, 3.0]], dtype=np.float32),
        velocities=np.array([[0.0, 0.0, 1.0]], dtype=np.float32),
        colors=np.array([[1.0, 1.0, 1.0, 1.0]], dtype=np.float32),
        sizes=np.array([2.0], dtype=np.float32),
        rotations=np.array([0.0], dtype=np.float32),
        atlas_indices=np.array([0], dtype=np.int32),
        atlas_offsets=((0.0, 1.0, 0.0, 1.0),),
        texture=source_texture,
        greyscale_texture=greyscale_texture,
        greyscale_color=True,
        greyscale_alpha=True,
    )
    renderer = ParticleRenderer(_Context(), program)

    renderer.update_batches([batch])
    renderer.render(
        tuple(np.eye(4, dtype=np.float32).reshape(-1)),
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
    )

    assert source_texture.use_calls == [0]
    assert greyscale_texture.use_calls == [1]
    assert program["ParticleTexture"].value == 0
    assert program["GreyscaleTexture"].value == 1
    assert program["hasParticleTexture"].value is True
    assert program["hasGreyscaleTexture"].value is True
    assert program["greyscaleColor"].value is True
    assert program["greyscaleAlpha"].value is True
