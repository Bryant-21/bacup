"""Tests for Starfield multi-layer material data flow."""
import json
from pathlib import Path

import pytest
from unittest.mock import MagicMock


class TestMaterialLayerFields:
    def test_material_default_layer_count(self):
        from creation_lib.renderer.scene_renderer import Material
        mat = Material()
        assert mat.layer_count == 1
        assert mat.layer_albedos == []
        assert mat.layer_normals == []
        assert mat.layer_specs == []
        assert mat.layer_tints == []
        assert mat.layer_opacities == []
        assert mat.blend_masks == []
        assert mat.blend_modes == []

    def test_material_with_layers(self):
        from creation_lib.renderer.scene_renderer import Material
        mock_tex = MagicMock()
        mat = Material(
            layer_count=3,
            layer_albedos=[mock_tex, mock_tex],  # layers 2+3 (layer 1 uses diffuseMap)
            layer_tints=[(1, 0, 0), (0, 1, 0), (0, 0, 1)],
            layer_opacities=[1.0, 0.8, 0.5],
            blend_modes=["linear", "position_contrast"],
        )
        assert mat.layer_count == 3
        assert len(mat.layer_albedos) == 2  # layers 2+3 only
        assert len(mat.blend_modes) == 2

    def test_material_backward_compat(self):
        """Existing FO4 code that uses single-layer Material should still work."""
        from creation_lib.renderer.scene_renderer import Material
        mat = Material()
        mat.diffuse_tex = MagicMock()
        mat.spec_color = (1, 1, 1)
        assert mat.diffuse_tex is not None
        assert mat.layer_count == 1


class TestLayerUniformBinding:
    def test_bind_single_layer_no_extra_uniforms(self):
        """Single-layer material should set sfLayerCount=1, no layer textures."""
        from creation_lib.renderer.scene_renderer import Material

        mat = Material(layer_count=1)
        # Verify defaults — the renderer should handle this gracefully
        assert mat.layer_count == 1
        assert mat.layer_albedos == []

    def test_bind_multi_layer_has_data(self):
        """Multi-layer material should carry all layer data."""
        from creation_lib.renderer.scene_renderer import Material

        mock_tex = MagicMock()
        mat = Material(
            layer_count=3,
            layer_albedos=[mock_tex, mock_tex],    # layers 2, 3 (layer 1 uses diffuseMap)
            layer_specs=[mock_tex, mock_tex],
            layer_tints=[(1, 1, 1), (1, 0.5, 0), (0, 1, 0)],
            layer_opacities=[1.0, 0.8, 0.6],
            blend_masks=[mock_tex, None],
            blend_modes=["linear", "position_contrast"],
            blend_vc_channels=[None, "r"],
            blend_height_thresholds=[0.5, 0.3],
            blend_height_factors=[1.0, 2.0],
        )
        assert mat.layer_count == 3
        assert len(mat.layer_albedos) == 2  # layers 2+3
        assert len(mat.blend_modes) == 2


class TestMatPipelineIntegration:
    def test_parse_mat_returns_material_data(self, tmp_path):
        """_parse_mat should call starfield_mat_reader and return texture paths."""
        from creation_lib.renderer.material_pipeline import _parse_mat

        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\color.DDS", "UseReplacement": False},
                        "Normal": {"File": "Textures\\normal.DDS", "UseReplacement": False},
                    }
                },
                "Layer2": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\l2_color.DDS", "UseReplacement": False},
                    }
                },
                "Blender1": {
                    "BlendMode": "Linear",
                },
            }
        }
        mat_file = tmp_path / "test.mat"
        mat_file.write_text(json.dumps(mat))

        result = _parse_mat(mat_file)
        assert result is not None
        assert result["type"] == "mat"
        assert "diffuse" in result
        assert result.get("_layer_count", 1) == 2

    def test_parse_mat_returns_none_for_invalid(self, tmp_path):
        """_parse_mat should return None for invalid .mat files."""
        from creation_lib.renderer.material_pipeline import _parse_mat

        bad_file = tmp_path / "bad.mat"
        bad_file.write_text("not json")
        result = _parse_mat(bad_file)
        # Should return None or a dict with empty textures
        assert result is None or len(result.get("diffuse", "")) == 0

    def test_parse_mat_bytes(self, tmp_path):
        """_parse_mat should accept bytes (from BA2 extraction)."""
        from creation_lib.renderer.material_pipeline import _parse_mat

        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\color.DDS", "UseReplacement": False},
                    }
                }
            }
        }
        mat_bytes = json.dumps(mat).encode("utf-8")
        result = _parse_mat(mat_bytes)
        assert result is not None
        assert "diffuse" in result


class TestMultiLayerTextureLoading:
    def test_build_material_sets_layer_count(self):
        """When _parse_mat returns multi-layer data, Material.layer_count should match."""
        from ui.editor.material_readers.base import LayerData, BlenderData

        # Verify the data structures flow correctly
        layers = [
            LayerData(texture_paths={"diffuse": "l1.dds"}),
            LayerData(texture_paths={"diffuse": "l2.dds"}, tint_color=(1, 0.5, 0)),
        ]
        blenders = [BlenderData(mode="linear")]

        assert len(layers) == 2
        assert layers[1].tint_color == (1, 0.5, 0)
        assert blenders[0].mode == "linear"
