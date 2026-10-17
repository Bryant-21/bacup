import hashlib
from pathlib import Path

import pytest

from bacup_lib.photo_mode_catalog import CATALOG, convert_catalog, overlay_geometry
from bacup_lib.translations import read_table

ROOT = Path(__file__).resolve().parents[5]
SOURCE = ROOT / "extracted/fo76"
TARGET = ROOT / "extracted/fo4"
PLUGIN = Path("N:/Steam Games/steamapps/common/Fallout76/Data/SeventySix.esm")


def test_pmft_geometry_rejects_invalid_image_contract():
    with pytest.raises(ValueError, match="Unexpected PMFT"):
        overlay_geometry({"FNAM": {"raw_hex": "02" + "00" * 31}})


@pytest.fixture(scope="module")
def converted(tmp_path_factory):
    if not PLUGIN.is_file() or not SOURCE.is_dir() or not TARGET.is_dir():
        pytest.skip("installed FO76 and extracted game assets unavailable")
    output = tmp_path_factory.mktemp("photo-mode-catalog")
    return output, convert_catalog(SOURCE, output / "data", PLUGIN, TARGET)


def test_full_source_catalog_is_loose_and_complete(converted):
    output, catalog = converted
    assert (output / CATALOG).is_file() and not (output / "data" / CATALOG).exists()
    assert len(catalog["poses"]) >= 70 and len(catalog["frames"]) > 200 and len(catalog["textures"]) >= 6
    assert len(catalog["lights"]) == 5 and all(len(style["lights"]) == 4 for style in catalog["lights"])
    assert catalog["playback_route"]["event"] == "dyn_ActivationLoop"
    assert catalog["playback_route"]["power_armor"]["swap_generators"]
    labels = dict(line.split("\t", 1) for line in read_table(output / "F4SE/Plugins/B21_TalesFromAppalachia_en.txt") if "\t" in line)
    assert {"Salute", "Air Guitar", "Ready", "Pip-Boy"}.issubset({labels[row["name"]] for row in catalog["poses"]})
    assert any(row.get("animation_unarmed") for row in catalog["poses"])
    assert sum(bool(row["animation_power_armor"]) for row in catalog["poses"]) >= 70
    for kind in ("poses", "frames", "textures"):
        for row in catalog[kind]:
            assert row["name"] in labels and row["category"] in labels
    for row in catalog["files"]:
        assert hashlib.sha256((output / "data" / row["path"]).read_bytes()).hexdigest() == row["sha256"]


def test_pose_cues_and_overlay_assets_have_source_dependencies(converted):
    _, catalog = converted
    assert any(row.get("animation_sounds") for row in catalog["poses"])
    assert not any("sound" in row for row in catalog["skipped"])
    assert all(row["path"].startswith("textures/") for row in catalog["frames"] + catalog["textures"])
