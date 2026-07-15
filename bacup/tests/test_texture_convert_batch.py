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
        from creation_lib.core.game_profiles import FO76_PROFILE

        files = [
            Path("armor_d.dds"),
            Path("armor_n.dds"),
            Path("armor_r.dds"),
            Path("armor_l.dds"),
            Path("weapon_d.dds"),
        ]
        groups = group_textures_by_base(files, FO76_PROFILE)
        assert "armor" in groups
        assert "weapon" in groups
        assert len(groups["armor"]) == 4
        assert len(groups["weapon"]) == 1

    def test_group_unrecognized_suffix(self):
        """Unrecognized suffixes use full stem as base name."""
        from bacup_lib.texture.batch import group_textures_by_base
        from creation_lib.core.game_profiles import FO4_PROFILE

        files = [Path("custom_texture.dds")]
        groups = group_textures_by_base(files, FO4_PROFILE)
        assert "custom_texture" in groups

    def test_group_empty_input(self):
        """Empty input returns empty groups."""
        from bacup_lib.texture.batch import group_textures_by_base
        from creation_lib.core.game_profiles import FO4_PROFILE

        groups = group_textures_by_base([], FO4_PROFILE)
        assert groups == {}


class TestBatchConvert:
    def test_converts_all_textures(self, tmp_path: Path):
        """Batch convert should process all recognized textures."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output"

        _write_test_texture(src_dir / "armor_d.dds")
        _write_test_texture(src_dir / "armor_n.dds", r=128, g=64, b=200)
        _write_test_texture(src_dir / "armor_r.dds", r=200, g=200, b=200)
        _write_test_texture(src_dir / "armor_l.dds", r=180, g=180, b=180)

        report = batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)

        assert dst_dir.exists()
        assert report.converted_files > 0
        assert report.errors == 0
        # Should have merged _r + _l into _s
        output_names = {f.stem for f in dst_dir.rglob("*") if f.is_file()}
        assert "armor_s" in output_names

    def test_creates_output_directory(self, tmp_path: Path):
        """Output dir should be created if it doesn't exist."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output" / "nested"
        _write_test_texture(src_dir / "tex_d.dds")

        batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)
        assert dst_dir.exists()

    def test_skips_non_texture_files(self, tmp_path: Path):
        """Non-texture files should be skipped."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output"
        (src_dir).mkdir(parents=True)
        (src_dir / "readme.txt").write_text("not a texture")

        report = batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)
        assert report.skipped_files == 1

    def test_report_contains_file_details(self, tmp_path: Path):
        """Report should list individual file conversions."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output"
        _write_test_texture(src_dir / "armor_d.dds")

        report = batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)
        assert len(report.details) > 0
        assert report.details[0].source_file is not None

    def test_fo76_metallic_lighting_merged(self, tmp_path: Path):
        """FO76->FO4 batch: _r + _l files should be merged into one _s file."""
        from bacup_lib.texture.batch import batch_convert
        from creation_lib.core.game_profiles import FO76_PROFILE, FO4_PROFILE

        src_dir = tmp_path / "source"
        dst_dir = tmp_path / "output"
        _write_test_texture(src_dir / "armor_r.dds", r=200, g=200, b=200)
        _write_test_texture(src_dir / "armor_l.dds", r=180, g=180, b=180)
        _write_test_texture(src_dir / "armor_d.dds")
        _write_test_texture(src_dir / "armor_n.dds", r=128, g=64, b=200)

        report = batch_convert(src_dir, dst_dir, FO76_PROFILE, FO4_PROFILE)

        output_files = {f.name for f in dst_dir.rglob("*") if f.is_file()}
        # Should have armor_d, armor_n, armor_s (merged from _r + _l)
        assert any("armor_s" in f for f in output_files)
        assert any("armor_d" in f for f in output_files)
        assert any("armor_n" in f for f in output_files)
        assert report.errors == 0

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
