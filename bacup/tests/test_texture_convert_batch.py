import pytest
from pathlib import Path


def _write_test_texture(path: Path, r=128, g=64, b=32, a=255, size=4):
    """Write a small test DDS."""
    from creation_lib.dds import native_runtime as dds_native

    path.parent.mkdir(parents=True, exist_ok=True)
    rgba = bytes([r, g, b, a] * (size * size))
    dds_native.write_dds_rgba(
        str(path),
        size,
        size,
        rgba,
        format="R8G8B8A8_UNORM",
    )


class TestGroupTextures:
    def test_group_by_base_name(self):
        """Group textures by their base name (before suffix)."""
        from bacup_lib.texture.batch import group_textures_by_base
        from creation_lib.core.game_profiles import FO4_PROFILE, FO76_PROFILE

        files = [
            Path("armor_d.dds"),
            Path("armor_n.dds"),
            Path("armor_r.dds"),
            Path("armor_l.dds"),
            Path("weapon_d.dds"),
        ]
        groups = group_textures_by_base(files, FO76_PROFILE)
        assert len(groups["armor"]) == 4
        assert len(groups["weapon"]) == 1
        assert "custom_texture" in group_textures_by_base([Path("custom_texture.dds")], FO4_PROFILE)
        assert group_textures_by_base([], FO4_PROFILE) == {}


class TestBatchConvert:
    def test_converts_texture_set_and_merges_metallic_lighting(self, tmp_path: Path):
        """FO76->FO4 batch: all textures converted, _r + _l merged into _s, non-textures skipped."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output" / "nested"

        _write_test_texture(src_dir / "armor_d.dds")
        _write_test_texture(src_dir / "armor_n.dds", r=128, g=64, b=200)
        _write_test_texture(src_dir / "armor_r.dds", r=200, g=200, b=200)
        _write_test_texture(src_dir / "armor_l.dds", r=180, g=180, b=180)
        (src_dir / "readme.txt").write_text("not a texture")

        report = batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)

        assert report.converted_files > 0
        assert report.errors == 0
        assert report.skipped_files == 1
        assert report.details[0].source_file is not None
        output_names = {f.stem for f in dst_dir.rglob("*") if f.is_file()}
        assert {"armor_d", "armor_n", "armor_s"} <= output_names

    def test_fo76_reflectivity_lighting_merge_without_diffuse(self, tmp_path: Path):
        """FO76->FO4 batch: _r + _l files should merge even when _d is absent."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE
        from creation_lib.dds import native_runtime as dds_native

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output"
        _write_test_texture(src_dir / "armor_r.dds", r=255, g=255, b=255)
        _write_test_texture(src_dir / "armor_l.dds", r=128, g=32, b=0)

        report = batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)

        output_files = [f.name for f in dst_dir.rglob("*") if f.is_file()]
        assert output_files.count("armor_s.dds") == 1
        assert dds_native.read_dds_rgba(str(dst_dir / "armor_s.dds"))["rgba"][:4] == bytes(
            [255, 128, 0, 255]
        )
        assert report.errors == 0

    def test_routes_dds_group_through_materials_native(self, tmp_path: Path, monkeypatch):
        """Batch convert should send DDS texture groups to materials native."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE
        from creation_lib.material_tools import native_runtime

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output"
        _write_test_texture(src_dir / "armor_d.dds")
        _write_test_texture(src_dir / "armor_r.dds", r=200, g=200, b=200)
        _write_test_texture(src_dir / "armor_l.dds", r=180, g=180, b=180)
        payloads = []

        def fake_convert_texture_set_paths(payload):
            payloads.append(payload)
            for output in payload["outputs"]:
                Path(output["path"]).parent.mkdir(parents=True, exist_ok=True)
                Path(output["path"]).write_bytes(b"dds")
            return {
                "converted": [
                    {"role": output["role"], "path": output["path"]}
                    for output in payload["outputs"]
                ],
                "skipped": [],
            }

        monkeypatch.setattr(
            native_runtime,
            "convert_texture_set_paths",
            fake_convert_texture_set_paths,
        )

        report = batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)

        assert report.errors == 0
        assert len(payloads) == 1
        payload = payloads[0]
        assert payload["source_game"] == "fo76"
        assert payload["target_game"] == "fo4"
        assert {item["role"] for item in payload["inputs"]} == {
            "diffuse",
            "reflectivity",
            "lighting",
        }
        assert all(Path(item["path"]).suffix == ".dds" for item in payload["inputs"])
        assert {item["role"] for item in payload["outputs"]} == {
            "diffuse",
            "specular",
            "glow",
        }
        assert report.converted_files == 3
