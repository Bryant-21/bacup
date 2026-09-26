from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.upgrade_manifest import (
    UpgradeManifest,
    UpgradeVersion,
    bundled_upgrade_manifest_path,
)
from bacup_ui.conversion.panels.regen_panel import RegenPanel


def _version(version_id, families, *, pair_id="fo76:fo4", force_regen=False):
    return UpgradeVersion(
        version_id,
        families_by_conversion=((pair_id, tuple(families)),),
        force_regen_by_conversion=((pair_id, force_regen),),
    )


_MANIFEST = UpgradeManifest(
    current="alpha2",
    versions=(
        _version("alpha1", ("ALL",)),
        _version("alpha2", ("Meshes", "Materials")),
    ),
)


def _ws(fo4_root="C:/FO4", fo76_root="C:/FO76", fo76_ext="C:/x/fo76"):
    paths = {
        "fo4": {"root_dir": fo4_root, "extracted_dir": fo4_root + "/Data"},
        "fo76": {"root_dir": fo76_root, "extracted_dir": fo76_ext},
    }
    return SimpleNamespace(
        _toolkit_settings=SimpleNamespace(
            get_game_paths=lambda g: dict(paths.get(g, {})),
            get_workspace_settings=lambda _w: {},
            set_workspace_settings=lambda _w, values: None,
        ),
        _runner=None,
    )


def _panel(
    monkeypatch,
    tmp_path,
    *,
    manifest=_MANIFEST,
    snam="alpha1",
    pair_id=None,
):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    if manifest is None:
        def _missing(_path):
            raise FileNotFoundError(_path)

        monkeypatch.setattr(
            "bacup_ui.conversion.panels.regen_panel.load_upgrade_manifest", _missing
        )
    else:
        monkeypatch.setattr(
            "bacup_ui.conversion.panels.regen_panel.load_upgrade_manifest",
            lambda _path: manifest,
        )
    # _detected_installed_version now requires the ESM to exist on disk (it
    # returns "(not deployed)" otherwise), so back it with a real temp file and
    # patch the fast header parser to return the fixture stamp.
    esm = tmp_path / "SeventySix.esm"
    esm.write_bytes(b"TES4")
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._deployed_esm_path",
        lambda self: esm,
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.read_plugin_snam_header", lambda _path: snam
    )
    return RegenPanel(_ws(), fixed_pair_id=pair_id)


def test_upgrade_build_options_use_manifest_and_auto_detected_from(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, snam="alpha1")
    panel.upgrade = False
    full = panel.build_options()
    assert full.upgrade is False
    assert full.mod_version == "alpha2"
    assert full.upgrade_manifest_path is None

    panel.upgrade = True
    options = panel.build_options()
    assert options.upgrade is True
    assert options.hydrate_upgrade_from_deployed is True
    assert options.mod_version == "alpha2"
    assert options.upgrade_from is None
    assert options.upgrade_manifest_path == bundled_upgrade_manifest_path()


@pytest.mark.parametrize("manifest, expect_upgrade", [
    (_MANIFEST, True),
    (UpgradeManifest(current="alpha2", versions=(
        _version("alpha1", ("ALL",)), _version("alpha2", ("ALL",)),
    )), False),
    (UpgradeManifest(current="alpha3", versions=(
        _version("alpha1", ("ALL",)), _version("alpha2", ("ALL",)), _version("alpha3", ("Scripts",)),
    )), False),
    (UpgradeManifest(current="alpha2", versions=(
        _version("alpha1", ("ALL",)), _version("alpha2", ("Meshes",), force_regen=True),
    )), False),
    (None, False),
])
def test_upgrade_falls_back_to_full_build_when_required(monkeypatch, tmp_path, manifest, expect_upgrade):
    panel = _panel(monkeypatch, tmp_path, manifest=manifest, snam="alpha1")
    panel.upgrade = True

    options = panel.build_options()

    assert options.upgrade is expect_upgrade
    assert options.hydrate_upgrade_from_deployed is expect_upgrade
