"""Tests for material_readers package."""
from __future__ import annotations
import json
import struct

import pytest

from creation_lib.renderer.material_readers.base import MaterialData
from creation_lib.renderer.material_readers.bgsm_reader import read_bgsm, read_bgem
from creation_lib.renderer.material_readers.starfield_mat_reader import read_mat_json


class TestMaterialData:
    def test_material_data_has_required_fields(self):
        md = MaterialData(
            texture_paths={"diffuse": "test.dds"},
            params={},
            material_model="spec-gloss",
        )
        assert md.texture_paths["diffuse"] == "test.dds"
        assert md.material_model == "spec-gloss"

    def test_material_data_defaults(self):
        md = MaterialData()
        assert md.texture_paths == {}
        assert md.params == {}
        assert md.material_model == "spec-gloss"

    def test_material_data_metallic_roughness(self):
        md = MaterialData(material_model="metallic-roughness")
        assert md.material_model == "metallic-roughness"


class TestBGSMReader:
    def test_read_bgsm_from_bytes_invalid_raises(self):
        """Test with truncated/invalid data raises an exception."""
        magic = struct.pack("<I", 0x4D534742)
        version = struct.pack("<I", 2)
        with pytest.raises(Exception):
            read_bgsm(magic + version + b"\x00" * 10)

    def test_read_bgsm_empty_bytes_raises(self):
        """Empty bytes should raise."""
        with pytest.raises(Exception):
            read_bgsm(b"")


class TestBGEMReader:
    def test_read_bgem_from_bytes_invalid_raises(self):
        """Test with truncated/invalid data raises an exception."""
        with pytest.raises(Exception):
            read_bgem(b"\x00" * 20)


class TestStarfieldMatReader:
    def test_read_mat_extracts_layer1_textures(self, tmp_path):
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\test_color.DDS", "UseReplacement": False},
                        "Normal": {"File": "Textures\\test_normal.DDS", "UseReplacement": False},
                        "Roughness": {"File": "Textures\\test_rough.DDS", "UseReplacement": False},
                    }
                }
            },
            "Version": 1,
        }
        mat_file = tmp_path / "test.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert isinstance(result, MaterialData)
        assert result.material_model == "metallic-roughness"
        assert "diffuse" in result.texture_paths  # Albedo -> diffuse
        assert "normal" in result.texture_paths
        assert "roughness" in result.texture_paths

    def test_skips_replacement_textures(self, tmp_path):
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "tex.DDS", "UseReplacement": True},
                    }
                }
            }
        }
        mat_file = tmp_path / "test.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert "diffuse" not in result.texture_paths

    def test_missing_summary_returns_empty(self, tmp_path):
        mat_file = tmp_path / "test.mat"
        mat_file.write_text("{}")
        result = read_mat_json(mat_file)
        assert len(result.texture_paths) == 0

    def test_import_chain_merges_parent(self, tmp_path):
        parent = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Normal": {"File": "parent_n.DDS", "UseReplacement": False},
                    }
                }
            }
        }
        child = {
            "Import": [str(tmp_path / "parent.mat")],
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "child_d.DDS", "UseReplacement": False},
                    }
                }
            },
        }
        (tmp_path / "parent.mat").write_text(json.dumps(parent))
        child_file = tmp_path / "child.mat"
        child_file.write_text(json.dumps(child))
        result = read_mat_json(child_file)
        assert "diffuse" in result.texture_paths
        assert "normal" in result.texture_paths

    def test_normalizes_backslashes(self, tmp_path):
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\Sub\\color.DDS", "UseReplacement": False},
                    }
                }
            }
        }
        mat_file = tmp_path / "test.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert result.texture_paths["diffuse"] == "Textures/Sub/color.DDS"

    def test_invalid_json_returns_empty(self, tmp_path):
        mat_file = tmp_path / "bad.mat"
        mat_file.write_text("not json at all")
        result = read_mat_json(mat_file)
        assert len(result.texture_paths) == 0
        assert result.material_model == "metallic-roughness"


class TestLayerData:
    def test_layer_data_defaults(self):
        from creation_lib.renderer.material_readers.base import LayerData
        layer = LayerData()
        assert layer.texture_paths == {}
        assert layer.tint_color is None
        assert layer.opacity == 1.0

    def test_layer_data_with_textures(self):
        from creation_lib.renderer.material_readers.base import LayerData
        layer = LayerData(
            texture_paths={"diffuse": "tex_color.dds", "normal": "tex_normal.dds"},
            tint_color=(1.0, 0.5, 0.0),
            opacity=0.8,
        )
        assert layer.texture_paths["diffuse"] == "tex_color.dds"
        assert layer.tint_color == (1.0, 0.5, 0.0)
        assert layer.opacity == 0.8


class TestBlenderData:
    def test_blender_data_defaults(self):
        from creation_lib.renderer.material_readers.base import BlenderData
        blender = BlenderData()
        assert blender.mode == "linear"
        assert blender.mask_texture is None
        assert blender.height_blend_threshold == 0.5
        assert blender.height_blend_factor == 1.0
        assert blender.vertex_color_channel is None

    def test_blender_data_custom(self):
        from creation_lib.renderer.material_readers.base import BlenderData
        blender = BlenderData(
            mode="position_contrast",
            mask_texture="blend_mask.dds",
            vertex_color_channel="r",
            height_blend_threshold=0.3,
            height_blend_factor=2.0,
        )
        assert blender.mode == "position_contrast"
        assert blender.mask_texture == "blend_mask.dds"
        assert blender.vertex_color_channel == "r"


class TestMaterialDataLayers:
    def test_material_data_with_layers(self):
        from creation_lib.renderer.material_readers.base import MaterialData, LayerData, BlenderData
        md = MaterialData(
            material_model="metallic-roughness",
            layers=[
                LayerData(texture_paths={"diffuse": "layer1.dds"}),
                LayerData(texture_paths={"diffuse": "layer2.dds"}),
            ],
            blenders=[
                BlenderData(mode="linear"),
            ],
        )
        assert len(md.layers) == 2
        assert len(md.blenders) == 1
        assert md.layers[0].texture_paths["diffuse"] == "layer1.dds"

    def test_material_data_backward_compat(self):
        """Existing code that doesn't set layers should still work."""
        from creation_lib.renderer.material_readers.base import MaterialData
        md = MaterialData()
        assert md.layers == []
        assert md.blenders == []
        # texture_paths still works (Layer1 shortcut)
        assert md.texture_paths == {}


class TestStarfieldMultiLayer:
    def test_parses_two_layers(self, tmp_path):
        """Two-layer .mat should produce 2 LayerData entries."""
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\layer1_color.DDS", "UseReplacement": False},
                        "Normal": {"File": "Textures\\layer1_normal.DDS", "UseReplacement": False},
                    }
                },
                "Layer2": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\layer2_color.DDS", "UseReplacement": False},
                        "Roughness": {"File": "Textures\\layer2_rough.DDS", "UseReplacement": False},
                    }
                },
            }
        }
        mat_file = tmp_path / "two_layer.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert len(result.layers) == 2
        assert "diffuse" in result.layers[0].texture_paths
        assert "diffuse" in result.layers[1].texture_paths
        # Backward compat: texture_paths == Layer1 textures
        assert result.texture_paths == result.layers[0].texture_paths

    def test_parses_six_layers(self, tmp_path):
        """Six-layer .mat should produce 6 LayerData entries."""
        summary = {}
        for i in range(1, 7):
            summary[f"Layer{i}"] = {
                "Textures": {
                    "Albedo": {"File": f"Textures\\l{i}_color.DDS", "UseReplacement": False},
                }
            }
        mat = {"Summary": summary}
        mat_file = tmp_path / "six_layer.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert len(result.layers) == 6

    def test_single_layer_backward_compat(self, tmp_path):
        """Single-layer .mat should still produce layers[0] + texture_paths."""
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "Textures\\color.DDS", "UseReplacement": False},
                    }
                }
            }
        }
        mat_file = tmp_path / "single.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert len(result.layers) == 1
        assert result.texture_paths["diffuse"] == "Textures/color.DDS"

    def test_layer_tint_color(self, tmp_path):
        """Layer with TintColor should be parsed."""
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "tex.DDS", "UseReplacement": False},
                    },
                    "TintColor": {"R": 1.0, "G": 0.5, "B": 0.0},
                }
            }
        }
        mat_file = tmp_path / "tinted.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert result.layers[0].tint_color == (1.0, 0.5, 0.0)

    def test_layer_opacity(self, tmp_path):
        """Layer with Opacity field should be parsed."""
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {
                        "Albedo": {"File": "tex.DDS", "UseReplacement": False},
                    },
                    "Opacity": 0.7,
                }
            }
        }
        mat_file = tmp_path / "opacity.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert result.layers[0].opacity == pytest.approx(0.7)


class TestStarfieldBlenders:
    def test_parses_single_blender(self, tmp_path):
        """A two-layer mat should have one blender."""
        mat = {
            "Summary": {
                "Layer1": {
                    "Textures": {"Albedo": {"File": "l1.DDS", "UseReplacement": False}},
                },
                "Layer2": {
                    "Textures": {"Albedo": {"File": "l2.DDS", "UseReplacement": False}},
                    "Blender": {
                        "BlendMode": "Linear",
                    },
                },
            }
        }
        mat_file = tmp_path / "blended.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert len(result.blenders) == 1
        assert result.blenders[0].mode == "linear"

    def test_blender_with_mask_texture(self, tmp_path):
        mat = {
            "Summary": {
                "Layer1": {"Textures": {"Albedo": {"File": "l1.DDS", "UseReplacement": False}}},
                "Layer2": {
                    "Textures": {"Albedo": {"File": "l2.DDS", "UseReplacement": False}},
                    "Blender": {
                        "BlendMode": "PositionContrast",
                        "MaskTexture": {"File": "Textures\\blend_mask.DDS", "UseReplacement": False},
                    },
                },
            }
        }
        mat_file = tmp_path / "masked.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert result.blenders[0].mode == "position_contrast"
        assert result.blenders[0].mask_texture == "Textures/blend_mask.DDS"

    def test_blender_vertex_color_channel(self, tmp_path):
        mat = {
            "Summary": {
                "Layer1": {"Textures": {"Albedo": {"File": "l1.DDS", "UseReplacement": False}}},
                "Layer2": {
                    "Textures": {"Albedo": {"File": "l2.DDS", "UseReplacement": False}},
                    "Blender": {
                        "BlendMode": "Linear",
                        "VertexColorChannel": "R",
                    },
                },
            }
        }
        mat_file = tmp_path / "vc.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert result.blenders[0].vertex_color_channel == "r"

    def test_blender_height_blend(self, tmp_path):
        mat = {
            "Summary": {
                "Layer1": {"Textures": {"Albedo": {"File": "l1.DDS", "UseReplacement": False}}},
                "Layer2": {
                    "Textures": {"Albedo": {"File": "l2.DDS", "UseReplacement": False}},
                    "Blender": {
                        "BlendMode": "Linear",
                        "HeightBlendThreshold": 0.3,
                        "HeightBlendFactor": 2.5,
                    },
                },
            }
        }
        mat_file = tmp_path / "height.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert result.blenders[0].height_blend_threshold == pytest.approx(0.3)
        assert result.blenders[0].height_blend_factor == pytest.approx(2.5)

    def test_no_blenders_returns_empty(self, tmp_path):
        """Single-layer mat has no blenders."""
        mat = {
            "Summary": {
                "Layer1": {"Textures": {"Albedo": {"File": "l1.DDS", "UseReplacement": False}}},
            }
        }
        mat_file = tmp_path / "single.mat"
        mat_file.write_text(json.dumps(mat))
        result = read_mat_json(mat_file)
        assert result.blenders == []
