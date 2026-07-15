"""Tests for NIF background loading (prepare/upload split)."""

import math
import numpy as np
import glm
import pytest
from dataclasses import fields
from creation_lib.renderer.nif_loader import (
    PreparedNifData,
    PreparedRenderBatch,
    PreparedShape,
    _extract_legacy_shape_data,
    _prepare_lod_render_batches,
    _should_prepare_lod_batches,
)
from creation_lib.nif.nif_file import NifBlock, NifFile


def _make_shape(block_id=1) -> PreparedShape:
    verts = np.zeros((3, 3), dtype=np.float32)
    normals = np.zeros((3, 3), dtype=np.float32)
    uvs = np.zeros((3, 2), dtype=np.float32)
    tris = np.array([[0, 1, 2]], dtype=np.uint32)
    return PreparedShape(
        block_id=block_id,
        name=f"Shape_{block_id}",
        verts=verts,
        normals=normals,
        uvs=uvs,
        tris=tris,
        colors=None,
        tangents=None,
        bitangents=None,
        transform=glm.mat4(1.0),
        material_inputs={},
    )


class TestPreparedNifData:
    def test_instantiation(self):
        shape = _make_shape(1)
        data = PreparedNifData(
            nif=None,
            nif_id="main",
            filepath="/path/to/test.nif",
            shapes={1: shape},
            decoded_textures={"tex.dds": None},
            texture_dirs=[],
            ba2_mgr=None,
        )
        assert data.nif_id == "main"
        assert data.filepath == "/path/to/test.nif"
        assert 1 in data.shapes
        assert "tex.dds" in data.decoded_textures

    def test_shapes_dict_keyed_by_block_id(self):
        shapes = {i: _make_shape(i) for i in range(3)}
        data = PreparedNifData(
            nif=None,
            nif_id="main",
            filepath="x.nif",
            shapes=shapes,
            decoded_textures={},
            texture_dirs=[],
            ba2_mgr=None,
        )
        assert set(data.shapes.keys()) == {0, 1, 2}


class TestPreparedShape:
    def test_fields_present(self):
        s = _make_shape()
        assert s.verts.shape == (3, 3)
        assert s.normals.shape == (3, 3)
        assert s.uvs.shape == (3, 2)
        assert s.tris.shape == (1, 3)
        assert s.colors is None
        assert s.tangents is None
        assert isinstance(s.transform, glm.mat4)
        assert s.material_inputs == {}


class TestBtoRenderBatching:
    def test_large_bto_files_use_lod_render_batches(self):
        shapes = {index: _make_shape(index) for index in range(512)}

        assert _should_prepare_lod_batches("tile.bto", shapes)
        assert not _should_prepare_lod_batches("tile.nif", shapes)
        assert not _should_prepare_lod_batches("tile.btr", shapes)
        assert not _should_prepare_lod_batches("small.bto", {1: _make_shape(1)})

    def test_batches_same_material_shapes_and_applies_parent_transforms(self):
        nif = _make_bto_batch_nif()
        shapes = {2: _make_triangle_shape(2), 7: _make_triangle_shape(7)}

        batches = _prepare_lod_render_batches(nif, shapes)

        assert len(batches) == 1
        batch = batches[0]
        assert batch.source_block_ids == (2, 7)
        assert batch.tris.tolist() == [[0, 1, 2], [3, 4, 5]]
        assert batch.verts[:, 0].tolist() == [10.0, 11.0, 10.0, 20.0, 21.0, 20.0]

    def test_keeps_different_texture_sets_in_separate_batches(self):
        nif = _make_bto_batch_nif(second_texture_set=10)
        shapes = {2: _make_triangle_shape(2), 7: _make_triangle_shape(7)}

        batches = _prepare_lod_render_batches(nif, shapes)

        assert len(batches) == 2
        assert [batch.source_block_ids for batch in batches] == [(2,), (7,)]


class TestCreateBa2Manager:
    def test_returns_new_manager_without_closing_old(self):
        """_create_ba2_manager must not call close_all() on any existing manager."""
        from unittest.mock import MagicMock, patch

        with (
            patch("ui.editor.app.NifFileWatcher"),
            patch("ui.editor.app.ConnectPointDisplay"),
            patch("ui.editor.app.LightDisplay"),
        ):
            from ui.editor.app import NifEditorApp

            app = NifEditorApp.__new__(NifEditorApp)
            old_mgr = MagicMock()
            app.ba2_manager = old_mgr

        mock_new_mgr = MagicMock()
        with patch(
            "creation_lib.textures.texture_dirs.create_ba2_manager",
            return_value=mock_new_mgr,
        ) as mock_create:
            result = app._create_ba2_manager([], [])

        old_mgr.close_all.assert_not_called()
        assert result is mock_new_mgr
        mock_create.assert_called_once_with([], [])


from unittest.mock import MagicMock, patch
from creation_lib.renderer.nif_loader import prepare_nif_data
from creation_lib.renderer.material_pipeline import _decode_cache


def test_create_particle_runtime_assigns_models_and_runtime():
    from ui.editor.app import NifEditorApp
    from ui.editor.particles.model import ParticleSystemModel
    from ui.editor.particles.runtime import ParticleRuntime

    app = NifEditorApp.__new__(NifEditorApp)
    fake_nif = MagicMock()
    fake_nif.blocks = []
    model = ParticleSystemModel(
        nif_id="main",
        system_block_id=1,
        name="ParticleSystem",
    )

    with patch("ui.editor.app.build_particle_models", return_value=[model]):
        models, runtime = app._create_particle_runtime(fake_nif, "main")

    assert models == [model]
    assert isinstance(runtime, ParticleRuntime)


def test_create_particle_runtime_loads_particle_source_textures():
    from ui.editor.app import NifEditorApp
    from ui.editor.particles.model import ParticleSystemModel

    app = NifEditorApp.__new__(NifEditorApp)
    app.ctx = object()
    fake_nif = MagicMock()
    fake_nif.blocks = []
    texture_dirs = [object()]
    ba2_mgr = object()
    texture = object()
    greyscale_texture = object()
    model = ParticleSystemModel(
        nif_id="main",
        system_block_id=1,
        name="ParticleSystem",
        source_texture=r"textures\effects\particle.dds",
        greyscale_texture=r"textures\effects\gradients\particlegrad.dds",
    )

    with (
        patch("ui.editor.app.build_particle_models", return_value=[model]),
        patch(
            "creation_lib.renderer.material_pipeline.load_texture_path",
            side_effect=[texture, greyscale_texture],
        ) as mock_load_texture,
    ):
        _models, runtime = app._create_particle_runtime(
            fake_nif,
            "main",
            texture_dirs=texture_dirs,
            ba2_mgr=ba2_mgr,
        )

    assert mock_load_texture.call_args_list[0].args == (
        app.ctx,
        r"textures\effects\particle.dds",
        texture_dirs,
        ba2_mgr,
    )
    assert mock_load_texture.call_args_list[1].args == (
        app.ctx,
        r"textures\effects\gradients\particlegrad.dds",
        texture_dirs,
        ba2_mgr,
    )
    assert runtime._texture_by_system == {1: texture}
    assert runtime._greyscale_texture_by_system == {1: greyscale_texture}


def test_create_particle_runtime_fails_closed_when_model_build_raises():
    from ui.editor.app import NifEditorApp

    app = NifEditorApp.__new__(NifEditorApp)
    fake_nif = MagicMock()
    fake_nif.blocks = []

    with (
        patch("ui.editor.app.build_particle_models", side_effect=ValueError("bad graph")),
        patch("ui.editor.app._log") as mock_log,
    ):
        models, runtime = app._create_particle_runtime(fake_nif, "main")

    assert models == []
    assert runtime is None
    mock_log.exception.assert_called_once()


def test_rebuild_scene_restores_particle_runtime_playback_state():
    from types import SimpleNamespace

    from ui.editor.app import NifEditorApp

    app = NifEditorApp.__new__(NifEditorApp)
    old_runtime = MagicMock()
    new_runtime = MagicMock()
    app.registry = MagicMock()
    session = SimpleNamespace(
        nif=MagicMock(),
        file_path="fx.nif",
        game_profile=None,
        attachment_node=None,
        scene_root=None,
        anim_manager=MagicMock(),
        particle_models=["old_model"],
        particle_runtime=old_runtime,
    )


def _identity_rotation():
    return {
        "m11": 1.0,
        "m21": 0.0,
        "m31": 0.0,
        "m12": 0.0,
        "m22": 1.0,
        "m32": 0.0,
        "m13": 0.0,
        "m23": 0.0,
        "m33": 1.0,
    }


def _transform_fields(x=0.0, y=0.0, z=0.0):
    return [
        ("Translation", {"x": x, "y": y, "z": z}),
        ("Rotation", _identity_rotation()),
        ("Scale", 1.0),
    ]


def _make_bto_batch_nif(second_texture_set=5):
    nif = NifFile()
    nif.blocks = [
        NifBlock(0, "NiNode", [("Name", "obj"), *_transform_fields(), ("Children", [1, 6])]),
        NifBlock(1, "BSMultiBoundNode", [("Name", "a"), *_transform_fields(10.0), ("Children", [2])]),
        NifBlock(2, "BSSubIndexTriShape", [("Name", "shape_a"), *_transform_fields(), ("Shader Property", 3), ("Alpha Property", 4)]),
        NifBlock(3, "BSLightingShaderProperty", [("Texture Set", 5), ("Shader Flags 1", []), ("Shader Flags 2", [])]),
        NifBlock(4, "NiAlphaProperty", [("Flags", 4844), ("Threshold", 127)]),
        NifBlock(5, "BSShaderTextureSet", [("Textures", ["textures/a.dds", "textures/a_n.dds"])]),
        NifBlock(6, "BSMultiBoundNode", [("Name", "b"), *_transform_fields(20.0), ("Children", [7])]),
        NifBlock(7, "BSSubIndexTriShape", [("Name", "shape_b"), *_transform_fields(), ("Shader Property", 8), ("Alpha Property", 9)]),
        NifBlock(8, "BSLightingShaderProperty", [("Texture Set", second_texture_set), ("Shader Flags 1", []), ("Shader Flags 2", [])]),
        NifBlock(9, "NiAlphaProperty", [("Flags", 4844), ("Threshold", 127)]),
        NifBlock(10, "BSShaderTextureSet", [("Textures", ["textures/b.dds", "textures/b_n.dds"])]),
    ]
    return nif


def _make_triangle_shape(block_id):
    return PreparedShape(
        block_id=block_id,
        name=f"Shape_{block_id}",
        verts=np.array([[0, 0, 0], [1, 0, 0], [0, 1, 0]], dtype=np.float32),
        normals=np.array([[0, 0, 1], [0, 0, 1], [0, 0, 1]], dtype=np.float32),
        uvs=np.array([[0, 0], [1, 0], [0, 1]], dtype=np.float32),
        tris=np.array([[0, 1, 2]], dtype=np.uint32),
        colors=None,
        tangents=None,
        bitangents=None,
        transform=glm.mat4(1.0),
        material_inputs={},
    )


def _rotation_z_180():
    return {
        "m11": -1.0,
        "m12": 0.0,
        "m13": 0.0,
        "m21": 0.0,
        "m22": -1.0,
        "m23": 0.0,
        "m31": 0.0,
        "m32": 0.0,
        "m33": 1.0,
    }
    session.anim_manager._node_cache = MagicMock()
    session.anim_manager._rest_transforms = MagicMock()
    app.registry.sessions = {"main": session}
    app.registry.active_id = "main"
    app.registry.get_session.return_value = session
    app.renderer = MagicMock()
    app.renderer.programs = {"default": object()}
    app.ctx = object()
    app.ba2_manager = object()
    app.light_display = MagicMock()
    app._build_texture_dirs = MagicMock(return_value=([],))
    app._sync_visibility = MagicMock()
    app._rebuild_selection_bounds = MagicMock()
    app._create_particle_runtime = MagicMock(return_value=(["new_model"], new_runtime))

    with patch(
        "creation_lib.renderer.nif_loader.rebuild_scene_from_nif",
        return_value=object(),
    ):
        app.rebuild_scene_from_nif("main")

    assert session.particle_models == ["new_model"]
    assert session.particle_runtime is new_runtime
    new_runtime.restore_playback_from.assert_called_once_with(old_runtime)


class TestPrepareNifData:
    def test_returns_prepared_nif_data(self, tmp_path):
        """prepare_nif_data returns PreparedNifData without touching _decode_cache."""
        mock_nif = MagicMock()
        mock_nif.blocks = []  # empty NIF — no shapes to extract

        with (
            patch("creation_lib.renderer.nif_loader.NifFile") as mock_nif_cls,
            patch(
                "creation_lib.renderer.nif_loader.collect_nif_texture_paths",
                return_value={},
            ),
        ):
            mock_nif_cls.load.return_value = mock_nif
            _decode_cache.clear()

            result = prepare_nif_data(
                filepath="fake.nif",
                texture_dirs=[],
                ba2_mgr=None,
                nif_id="main",
            )

        assert result.nif_id == "main"
        assert result.filepath == "fake.nif"
        assert isinstance(result.shapes, dict)
        assert isinstance(result.decoded_textures, dict)
        # CRITICAL: must not write to the module-level _decode_cache
        assert len(_decode_cache) == 0

    def test_extracts_shapes_from_nif(self, tmp_path):
        """prepare_nif_data walks blocks and extracts PreparedShape per BSTriShape."""
        mock_nif = MagicMock()
        mock_block = MagicMock()
        mock_block.block_id = 5
        mock_block.type_name = "BSTriShape"
        mock_nif.blocks = [mock_block]

        verts_data = [{"Vertex": {"x": 0, "y": 0, "z": 0}}] * 3
        tris_data = [{"v1": 0, "v2": 1, "v3": 2}]
        mock_block.get_field.side_effect = lambda k: {
            "Vertex Data": verts_data,
            "Triangles": tris_data,
            "Name": "TestShape",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(k)

        with (
            patch("creation_lib.renderer.nif_loader.NifFile") as mock_nif_cls,
            patch(
                "creation_lib.renderer.nif_loader.collect_nif_texture_paths",
                return_value={},
            ),
            patch("creation_lib.renderer.nif_loader._get_string", return_value="TestShape"),
        ):
            mock_nif_cls.load.return_value = mock_nif

            result = prepare_nif_data("fake.nif", [], None, "main")

        assert 5 in result.shapes
        shape = result.shapes[5]
        assert shape.name == "TestShape"
        assert shape.verts.shape == (3, 3)

    def test_extracts_legacy_nitrishape_from_data_block(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 7
        shape.type_name = "NiTriShape"
        data.block_id = 8
        data.type_name = "NiTriShapeData"
        mock_nif.blocks = [shape]
        mock_nif.schema.is_subtype_of.side_effect = lambda t, base: (
            (t == "NiTriShape" and base == "NiTriShape")
            or (t == "NiTriShape" and base == "NiNode")
            or (t == "NiTriShape" and base == "BSTriShape")
        )
        mock_nif.get_block.side_effect = lambda block_id: {8: data}.get(block_id)

        shape.get_field.side_effect = lambda k: {
            "Data": 8,
            "Name": "LegacyShape",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(k)
        data.get_field.side_effect = lambda k: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 1,
            "UV Sets": [[{"u": 0, "v": 0}, {"u": 1, "v": 0}, {"u": 0, "v": 1}]],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(k)

        with (
            patch("creation_lib.renderer.nif_loader.NifFile") as mock_nif_cls,
            patch("creation_lib.renderer.nif_loader.collect_nif_texture_paths", return_value={}),
            patch("creation_lib.renderer.nif_loader._get_string", return_value="LegacyShape"),
        ):
            mock_nif_cls.load.return_value = mock_nif
            result = prepare_nif_data("legacy.nif", [], None, "main")

        assert 7 in result.shapes
        assert result.shapes[7].tris.tolist() == [[0, 1, 2]]

    def test_extracts_legacy_nitrishape_subtype_from_data_block(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 11
        shape.type_name = "BSLODTriShape"
        data.block_id = 12
        data.type_name = "NiTriShapeData"
        mock_nif.blocks = [shape]
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda t, base: t == base or (t == "BSLODTriShape" and base == "NiTriShape")
        )
        mock_nif.get_block.side_effect = lambda block_id: {12: data}.get(block_id)

        shape.get_field.side_effect = lambda k: {
            "Data": 12,
            "Name": "LegacyLODShape",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(k)
        data.get_field.side_effect = lambda k: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 1,
            "UV Sets": [[{"u": 0, "v": 0}, {"u": 1, "v": 0}, {"u": 0, "v": 1}]],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(k)

        with (
            patch("creation_lib.renderer.nif_loader.NifFile") as mock_nif_cls,
            patch("creation_lib.renderer.nif_loader.collect_nif_texture_paths", return_value={}),
            patch("creation_lib.renderer.nif_loader._get_string", return_value="LegacyLODShape"),
        ):
            mock_nif_cls.load.return_value = mock_nif
            result = prepare_nif_data("legacy_lod.nif", [], None, "main")

        assert 11 in result.shapes
        assert result.shapes[11].tris.tolist() == [[0, 1, 2]]

    def test_extracts_legacy_nitristrips_from_data_block(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 9
        shape.type_name = "NiTriStrips"
        data.block_id = 10
        data.type_name = "NiTriStripsData"
        mock_nif.blocks = [shape]
        mock_nif.schema.is_subtype_of.side_effect = lambda t, base: t == base
        mock_nif.get_block.side_effect = lambda block_id: {10: data}.get(block_id)

        shape.get_field.side_effect = lambda k: {
            "Data": 10,
            "Name": "StripShape",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(k)
        data.get_field.side_effect = lambda k: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 1, "y": 1, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 0,
            "Normals": [],
            "Has UV": 0,
            "UV Sets": [],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Strip Lengths": [4],
            "Points": [0, 1, 2, 3],
        }.get(k)

        with (
            patch("creation_lib.renderer.nif_loader.NifFile") as mock_nif_cls,
            patch("creation_lib.renderer.nif_loader.collect_nif_texture_paths", return_value={}),
            patch("creation_lib.renderer.nif_loader._get_string", return_value="StripShape"),
        ):
            mock_nif_cls.load.return_value = mock_nif
            result = prepare_nif_data("legacy_strip.nif", [], None, "main")

        assert 9 in result.shapes
        assert result.shapes[9].tris.tolist() == [[0, 1, 2], [2, 1, 3]]

    def test_extracts_legacy_nitristrips_from_grouped_points(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 17
        shape.type_name = "NiTriStrips"
        data.block_id = 18
        data.type_name = "NiTriStripsData"
        mock_nif.blocks = [shape]
        mock_nif.schema.is_subtype_of.side_effect = lambda t, base: t == base
        mock_nif.get_block.side_effect = lambda block_id: {18: data}.get(block_id)

        shape.get_field.side_effect = lambda k: {
            "Data": 18,
            "Name": "GroupedStripShape",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(k)
        data.get_field.side_effect = lambda k: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 1, "y": 1, "z": 0},
                {"x": 0, "y": 1, "z": 0},
                {"x": 2, "y": 0, "z": 0},
                {"x": 2, "y": 1, "z": 0},
                {"x": 3, "y": 1, "z": 0},
            ],
            "Has Normals": 0,
            "Normals": [],
            "Has UV": 0,
            "UV Sets": [],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Strip Lengths": [4, 3],
            "Points": [[0, 1, 2, 3], [4, 5, 6]],
        }.get(k)

        with (
            patch("creation_lib.renderer.nif_loader.NifFile") as mock_nif_cls,
            patch("creation_lib.renderer.nif_loader.collect_nif_texture_paths", return_value={}),
            patch("creation_lib.renderer.nif_loader._get_string", return_value="GroupedStripShape"),
        ):
            mock_nif_cls.load.return_value = mock_nif
            result = prepare_nif_data("legacy_grouped_strip.nif", [], None, "main")

        assert 17 in result.shapes
        assert result.shapes[17].tris.tolist() == [[0, 1, 2], [2, 1, 3], [4, 5, 6]]

    def test_legacy_shape_without_vertex_colors_gets_white_color_stream(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 13
        shape.type_name = "NiTriShape"
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda type_name, base: type_name == base
        )
        mock_nif.get_block.return_value = data

        shape.get_field.side_effect = lambda key: {
            "Data": 14,
            "Name": "LegacyWhite",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(key)
        data.get_field.side_effect = lambda key: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 0,
            "UV Sets": [],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Tangents": [],
            "Bitangents": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(key)

        prepared = _extract_legacy_shape_data(mock_nif, shape)

        assert prepared is not None
        assert prepared.colors.shape == (3, 4)
        np.testing.assert_array_equal(
            prepared.colors, np.ones((3, 4), dtype=np.float32)
        )

    def test_legacy_shape_preserves_uvs_when_uv_sets_exist_without_has_uv_flag(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 14
        shape.type_name = "NiTriShape"
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda type_name, base: type_name == base
        )
        mock_nif.get_block.return_value = data

        shape.get_field.side_effect = lambda key: {
            "Data": 15,
            "Name": "LegacyUvShape",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(key)
        data.get_field.side_effect = lambda key: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 0,
            "UV Sets": [[{"u": 0.0, "v": 0.0}, {"u": 1.0, "v": 0.0}, {"u": 0.0, "v": 1.0}]],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Tangents": [],
            "Bitangents": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(key)

        prepared = _extract_legacy_shape_data(mock_nif, shape)

        assert prepared is not None
        np.testing.assert_array_equal(
            prepared.uvs,
            np.array([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], dtype=np.float32),
        )

    def test_legacy_shape_preserves_vertex_colors_and_tangent_basis(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 15
        shape.type_name = "NiTriShape"
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda type_name, base: type_name == base
        )
        mock_nif.get_block.return_value = data

        shape.get_field.side_effect = lambda key: {
            "Data": 16,
            "Name": "LegacyTangentShape",
            "Translation": {},
            "Rotation": {},
            "Scale": 1.0,
        }.get(key)
        data.get_field.side_effect = lambda key: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 1,
            "UV Sets": [[{"u": 0, "v": 0}, {"u": 1, "v": 0}, {"u": 0, "v": 1}]],
            "Has Vertex Colors": 1,
            "Vertex Colors": [
                {"r": 0.25, "g": 0.5, "b": 0.75, "a": 1.0},
                {"r": 1.0, "g": 0.25, "b": 0.0, "a": 0.5},
                {"r": 0.0, "g": 1.0, "b": 0.5, "a": 0.25},
            ],
            "Tangents": [{"x": 1, "y": 0, "z": 0}] * 3,
            "Bitangents": [{"x": 0, "y": 1, "z": 0}] * 3,
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(key)

        prepared = _extract_legacy_shape_data(mock_nif, shape)

        assert prepared is not None
        np.testing.assert_allclose(
            prepared.colors,
            np.array(
                [
                    [0.25, 0.5, 0.75, 1.0],
                    [1.0, 0.25, 0.0, 0.5],
                    [0.0, 1.0, 0.5, 0.25],
                ],
                dtype=np.float32,
            ),
        )
        np.testing.assert_array_equal(
            prepared.tangents,
            np.array([[1, 0, 0], [1, 0, 0], [1, 0, 0]], dtype=np.float32),
        )
        np.testing.assert_array_equal(
            prepared.bitangents,
            np.array([[0, 1, 0], [0, 1, 0], [0, 1, 0]], dtype=np.float32),
        )

    def test_legacy_skinned_shape_applies_skin_translation_correction(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        skin_instance = MagicMock()
        skin_data = MagicMock()
        shape.block_id = 19
        shape.type_name = "NiTriShape"
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda type_name, base: type_name == base
        )
        mock_nif.get_block.side_effect = lambda block_id: {
            20: data,
            21: skin_instance,
            22: skin_data,
        }.get(block_id)

        shape.get_field.side_effect = lambda key: {
            "Data": 20,
            "Skin Instance": 21,
            "Name": "LegacySkinnedTranslation",
            "Translation": {"x": 10.0, "y": 0.0, "z": 0.0},
            "Rotation": {},
            "Scale": 1.0,
        }.get(key)
        data.get_field.side_effect = lambda key: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 0,
            "UV Sets": [],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Tangents": [],
            "Bitangents": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(key)
        skin_instance.get_field.side_effect = lambda key: {"Data": 22}.get(key)
        skin_data.get_field.side_effect = lambda key: {
            "Skin Transform": {
                "Translation": {"x": 12.0, "y": 0.0, "z": 0.0},
                "Rotation": {},
                "Scale": 1.0,
            }
        }.get(key)

        prepared = _extract_legacy_shape_data(mock_nif, shape)

        assert prepared is not None
        np.testing.assert_allclose(
            prepared.verts,
            np.array(
                [[2.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 1.0, 0.0]],
                dtype=np.float32,
            ),
        )

    def test_legacy_skinned_shape_skips_inverse_pair_correction(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        skin_instance = MagicMock()
        skin_data = MagicMock()
        shape.block_id = 31
        shape.type_name = "NiTriShape"
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda type_name, base: type_name == base
        )
        mock_nif.get_block.side_effect = lambda block_id: {
            32: data,
            33: skin_instance,
            34: skin_data,
        }.get(block_id)

        shape.get_field.side_effect = lambda key: {
            "Data": 32,
            "Skin Instance": 33,
            "Name": "LegacySkinnedInversePair",
            "Translation": {"x": 10.0, "y": 0.0, "z": 0.0},
            "Rotation": {},
            "Scale": 1.0,
        }.get(key)
        data.get_field.side_effect = lambda key: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 0,
            "UV Sets": [],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Tangents": [],
            "Bitangents": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(key)
        skin_instance.get_field.side_effect = lambda key: {"Data": 34}.get(key)
        skin_data.get_field.side_effect = lambda key: {
            "Skin Transform": {
                "Translation": {"x": -10.00005, "y": 0.0, "z": 0.0},
                "Rotation": {},
                "Scale": 1.0,
            }
        }.get(key)

        prepared = _extract_legacy_shape_data(mock_nif, shape)

        assert prepared is not None
        np.testing.assert_allclose(
            prepared.verts,
            np.array(
                [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                dtype=np.float32,
            ),
        )

    def test_legacy_skinned_shape_rotates_normals_into_shape_local_space(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        skin_instance = MagicMock()
        skin_data = MagicMock()
        shape.block_id = 23
        shape.type_name = "NiTriShape"
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda type_name, base: type_name == base
        )
        mock_nif.get_block.side_effect = lambda block_id: {
            24: data,
            25: skin_instance,
            26: skin_data,
        }.get(block_id)

        shape.get_field.side_effect = lambda key: {
            "Data": 24,
            "Skin Instance": 25,
            "Name": "LegacySkinnedNormals",
            "Translation": {},
            "Rotation": _rotation_z_180(),
            "Scale": 1.0,
        }.get(key)
        data.get_field.side_effect = lambda key: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 1, "y": 0, "z": 0}] * 3,
            "Has UV": 0,
            "UV Sets": [],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Tangents": [],
            "Bitangents": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(key)
        skin_instance.get_field.side_effect = lambda key: {"Data": 26}.get(key)
        skin_data.get_field.side_effect = lambda key: {
            "Skin Transform": {
                "Translation": {},
                "Rotation": {},
                "Scale": 1.0,
            }
        }.get(key)

        prepared = _extract_legacy_shape_data(mock_nif, shape)

        assert prepared is not None
        np.testing.assert_allclose(
            prepared.normals,
            np.array([[-1.0, 0.0, 0.0]] * 3, dtype=np.float32),
        )

    def test_legacy_unskinned_shape_keeps_original_vertices(self):
        mock_nif = MagicMock()
        shape = MagicMock()
        data = MagicMock()
        shape.block_id = 27
        shape.type_name = "NiTriShape"
        mock_nif.schema.is_subtype_of.side_effect = (
            lambda type_name, base: type_name == base
        )
        mock_nif.get_block.return_value = data

        shape.get_field.side_effect = lambda key: {
            "Data": 28,
            "Name": "LegacyUnskinned",
            "Translation": {"x": 5.0, "y": 0.0, "z": 0.0},
            "Rotation": _rotation_z_180(),
            "Scale": 1.0,
        }.get(key)
        data.get_field.side_effect = lambda key: {
            "Vertices": [
                {"x": 0, "y": 0, "z": 0},
                {"x": 1, "y": 0, "z": 0},
                {"x": 0, "y": 1, "z": 0},
            ],
            "Has Normals": 1,
            "Normals": [{"x": 0, "y": 0, "z": 1}] * 3,
            "Has UV": 0,
            "UV Sets": [],
            "Has Vertex Colors": 0,
            "Vertex Colors": [],
            "Tangents": [],
            "Bitangents": [],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
        }.get(key)

        prepared = _extract_legacy_shape_data(mock_nif, shape)

        assert prepared is not None
        np.testing.assert_allclose(
            prepared.verts,
            np.array(
                [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                dtype=np.float32,
            ),
        )

    def test_decoded_textures_populated_not_decode_cache(self):
        """Texture decode results go into PreparedNifData.decoded_textures, not _decode_cache."""
        from pathlib import Path

        mock_nif = MagicMock()
        mock_nif.blocks = []
        fake_path = Path("/fake/tex.png")  # non-DDS so it goes through loose_other path
        fake_decoded = MagicMock()

        collected = {str(fake_path): fake_path}
        with (
            patch("creation_lib.renderer.nif_loader.NifFile") as mock_nif_cls,
            patch(
                "creation_lib.renderer.nif_loader.collect_nif_texture_paths",
                return_value=collected,
            ),
            patch(
                "creation_lib.renderer.nif_loader.decode_texture",
                return_value=fake_decoded,
            ),
        ):
            mock_nif_cls.load.return_value = mock_nif
            _decode_cache.clear()

            result = prepare_nif_data("fake.nif", [], None, "main")

        assert str(fake_path) in result.decoded_textures
        assert result.decoded_textures[str(fake_path)] is fake_decoded
        # Module-level cache must remain untouched
        assert len(_decode_cache) == 0


from creation_lib.renderer.nif_loader import upload_nif_to_gpu
from creation_lib.renderer.scene_renderer import SceneNode


def _make_prepared(shapes=None) -> PreparedNifData:
    mock_nif = MagicMock()
    mock_nif.blocks = []
    return PreparedNifData(
        nif=mock_nif,
        nif_id="main",
        filepath="test.nif",
        shapes=shapes or {},
        decoded_textures={"tex.dds": MagicMock()},
        texture_dirs=[],
        ba2_mgr=None,
    )


class TestUploadNifToGpu:
    def test_merges_decoded_textures_into_decode_cache(self):
        """decoded_textures from PreparedNifData must be merged into _decode_cache via LRU."""
        fake_decoded = MagicMock()
        prepared = _make_prepared()
        prepared.decoded_textures = {"some/tex.dds": fake_decoded}
        mock_ctx = MagicMock()
        mock_program = MagicMock()
        mock_program.__contains__ = lambda self, key: False

        with patch.dict("creation_lib.renderer.material_pipeline._decode_cache", {}, clear=True):
            upload_nif_to_gpu(prepared, mock_ctx, mock_program)
            from creation_lib.renderer.material_pipeline import _decode_cache

            assert "some/tex.dds" in _decode_cache
            assert _decode_cache["some/tex.dds"] is fake_decoded

    def test_returns_scene_root_and_nif(self):
        """upload_nif_to_gpu returns (SceneNode, NifFile)."""
        prepared = _make_prepared()
        mock_ctx = MagicMock()
        mock_program = MagicMock()
        mock_program.__contains__ = lambda self, key: False

        with patch.dict("creation_lib.renderer.material_pipeline._decode_cache", {}, clear=True):
            scene_root, nif = upload_nif_to_gpu(prepared, mock_ctx, mock_program)

        assert isinstance(scene_root, SceneNode)
        assert nif is prepared.nif

    def test_upload_uses_prepared_render_batches(self):
        """Large BTO loads should upload combined render batches, not every source shape."""
        fake_shape = MagicMock()
        fake_nif = MagicMock()
        fake_nif.blocks = [MagicMock()]
        fake_nif.get_block.return_value = fake_shape
        batch = PreparedRenderBatch(
            name="BTO batch 1 (2 shapes)",
            block_id=2,
            material_shape_id=2,
            source_block_ids=(2, 7),
            verts=np.zeros((3, 3), dtype=np.float32),
            normals=np.zeros((3, 3), dtype=np.float32),
            uvs=np.zeros((3, 2), dtype=np.float32),
            tris=np.array([[0, 1, 2]], dtype=np.uint32),
            colors=None,
            tangents=None,
            bitangents=None,
        )
        prepared = PreparedNifData(
            nif=fake_nif,
            nif_id="main",
            filepath="tile.bto",
            shapes={},
            decoded_textures={},
            texture_dirs=[],
            ba2_mgr=None,
            render_batches=[batch],
        )
        mesh = MagicMock()
        mock_ctx = MagicMock()
        mock_program = MagicMock()
        mock_program.__contains__ = lambda self, key: False

        with (
            patch("creation_lib.renderer.nif_loader._build_mesh", return_value=mesh) as mock_build,
            patch("creation_lib.renderer.nif_loader.build_material", return_value=object()),
        ):
            scene_root, _nif = upload_nif_to_gpu(prepared, mock_ctx, mock_program)

        batch_root = scene_root.children[0]
        assert batch_root.name == "tile (batched)"
        assert len(batch_root.children) == 1
        assert batch_root.children[0].source_block_ids == (2, 7)
        mock_build.assert_called_once()


from concurrent.futures import Future


class TestLoadNifAsync:
    """Tests for the async load_nif path in NifEditorApp."""

    def _make_app(self):
        """Create a NifEditorApp with all heavy dependencies mocked out."""
        with (
            patch("ui.editor.app.NifFileWatcher"),
            patch("ui.editor.app.ConnectPointDisplay"),
            patch("ui.editor.app.LightDisplay"),
        ):
            from ui.editor.app import NifEditorApp

            app = NifEditorApp.__new__(NifEditorApp)
            # Minimal init
            app.registry = MagicMock()
            app.ba2_manager = None
            app._loading = False
            app._loading_future = None
            app._loading_nif_id = "main"
            app._loading_filename = ""
            app._loading_ba2_mgr = None
            app.status_text = ""
            app.renderer = MagicMock()
            app.renderer.programs = {"default": MagicMock(), "fo4": MagicMock()}
            app._load_executor = MagicMock()
            return app

    def test_load_nif_sets_loading_true(self):
        """load_nif() must set _loading=True and submit a future."""
        app = self._make_app()
        fake_future = MagicMock(spec=Future)
        app._load_executor.submit.return_value = fake_future

        with (
            patch("ui.editor.app.Path.exists", return_value=True),
            patch.object(app, "_detect_game_profile", return_value=None),
            patch.object(app, "_build_texture_dirs", return_value=([], [], [])),
            patch.object(app, "_create_ba2_manager", return_value=MagicMock()),
        ):
            app.load_nif("test.nif")

        assert app._loading is True
        assert app._loading_future is fake_future
        assert app._loading_filename == "test.nif"

    def test_load_nif_submits_prepare_nif_data(self):
        """load_nif() must submit prepare_nif_data (not load_nif_to_scene) to executor."""
        app = self._make_app()
        app._load_executor.submit.return_value = MagicMock(spec=Future)

        with (
            patch("ui.editor.app.Path.exists", return_value=True),
            patch.object(app, "_detect_game_profile", return_value=None),
            patch.object(app, "_build_texture_dirs", return_value=([], [], [])),
            patch.object(app, "_create_ba2_manager", return_value=None),
            patch("creation_lib.renderer.nif_loader.prepare_nif_data") as mock_prepare,
        ):
            app.load_nif("test.nif")
            # executor.submit should have been called with prepare_nif_data as first arg
            call_args = app._load_executor.submit.call_args
            assert call_args[0][0] is mock_prepare


class TestPollLoading:
    def _make_app_with_loading(self):
        with (
            patch("ui.editor.app.NifFileWatcher"),
            patch("ui.editor.app.ConnectPointDisplay"),
            patch("ui.editor.app.LightDisplay"),
        ):
            from ui.editor.app import NifEditorApp

            app = NifEditorApp.__new__(NifEditorApp)
            app._loading = True
            app._loading_future = MagicMock(spec=Future)
            app._loading_filename = "test.nif"
            app._loading_nif_id = "main"
            app.status_text = ""
            app.renderer = MagicMock()
            app.renderer.programs = {"default": MagicMock(), "fo4": MagicMock()}
            app.registry = MagicMock()
            app.registry.sessions = {}
            app.selection_mgr = MagicMock()
            app.camera = MagicMock()
            app.undo_manager = MagicMock()
            app.light_display = MagicMock()
            app.nif_watcher = MagicMock()
            app.ctx = MagicMock()
            return app

    def test_does_nothing_when_not_loading(self):
        app = self._make_app_with_loading()
        app._loading = False
        app._loading_future = MagicMock()
        app._poll_loading()
        app._loading_future.done.assert_not_called()

    def test_does_nothing_when_future_not_done(self):
        app = self._make_app_with_loading()
        app._loading_future.done.return_value = False
        app._poll_loading()
        assert app._loading is True  # still loading

    def test_error_path_clears_loading(self):
        app = self._make_app_with_loading()
        app._loading_future.done.return_value = True
        app._loading_future.result.side_effect = RuntimeError("disk error")
        app._poll_loading()
        assert app._loading is False
        assert "Error" in app.status_text
        assert app._loading_future is None

    def test_success_path_clears_loading_after_gpu_upload(self):
        app = self._make_app_with_loading()
        fake_prepared = MagicMock()
        fake_prepared.nif_id = "main"
        fake_prepared.filepath = "test.nif"
        fake_scene_root = MagicMock()
        fake_scene_root.bound_radius = 0
        fake_scene_root.children = []
        fake_nif = MagicMock()

        app._loading_future.done.return_value = True
        app._loading_future.result.return_value = fake_prepared

        with (
            patch(
                "creation_lib.renderer.nif_loader.upload_nif_to_gpu",
                return_value=(fake_scene_root, fake_nif),
            ),
            patch("ui.editor.app.AnimationManager") as MockAnim,
            patch("ui.editor.app.NifSession") as MockSession,
            patch("ui.editor.recent_files.add"),
        ):  # patch at source, not lazy import alias
            app._poll_loading()

        assert app._loading is False
        assert app._loading_future is None
        assert "Loaded" in app.status_text

    def test_main_load_clears_previous_file_watches(self):
        app = self._make_app_with_loading()
        fake_prepared = MagicMock()
        fake_prepared.nif_id = "main"
        fake_prepared.filepath = "new.nif"
        fake_scene_root = MagicMock()
        fake_scene_root.bound_radius = 0
        fake_scene_root.children = []
        fake_nif = MagicMock()

        app._loading_future.done.return_value = True
        app._loading_future.result.return_value = fake_prepared

        with (
            patch(
                "creation_lib.renderer.nif_loader.upload_nif_to_gpu",
                return_value=(fake_scene_root, fake_nif),
            ),
            patch("ui.editor.app.AnimationManager") as MockAnim,
            patch("ui.editor.app.NifSession"),
            patch("ui.editor.recent_files.add"),
        ):
            app._poll_loading()

        app.nif_watcher.stop_watching.assert_called_once()
        app.nif_watcher.start_watching.assert_called_once_with("new.nif")
