from pathlib import Path
from types import SimpleNamespace

from bacup_lib.upgrade_manifest import UpgradeManifest, UpgradeVersion
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


def test_upgrade_on_build_options_sets_fields_with_auto_from(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, snam="alpha1")
    panel.upgrade = True

    options = panel.build_options()

    assert options.upgrade is True
    assert options.mod_version == "alpha2"  # manifest.current, no target override
    assert options.upgrade_from is None  # auto-detect, no override set
    from bacup_lib.upgrade_manifest import bundled_upgrade_manifest_path

    assert options.upgrade_manifest_path == bundled_upgrade_manifest_path()


def test_upgrade_off_leaves_full_build_path_unchanged(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, snam="alpha1")
    panel.upgrade = False

    options = panel.build_options()

    assert options.upgrade is False
    assert options.mod_version is None
    assert options.upgrade_from is None
    assert options.upgrade_manifest_path is None


def test_non_fo76_pair_uses_pair_scoped_upgrade_manifest(monkeypatch, tmp_path):
    manifest = UpgradeManifest(
        current="alpha2",
        versions=(
            _version("alpha1", ("ALL",), pair_id="skyrimse:fo4"),
            _version("alpha2", ("NONE",), pair_id="skyrimse:fo4"),
        ),
    )
    panel = _panel(
        monkeypatch,
        tmp_path,
        manifest=manifest,
        snam="alpha1",
        pair_id="skyrimse:fo4",
    )
    panel.upgrade = True

    options = panel.build_options()

    assert options.upgrade is True
    assert panel.upgrade_plan_preview() == (
        "No changes for Fables of the North in this upgrade."
    )


def test_upgrade_plan_preview_reflects_resolved_families_and_swap_labels(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, snam="alpha1")
    panel.upgrade = True

    preview = panel.upgrade_plan_preview()

    assert preview == (
        "Will regenerate: Materials, Meshes -> "
        "swap Materials, Meshes, MeshesExtra; reuse rest"
    )


def test_upgrade_plan_preview_repeats_target_scripts_when_current(monkeypatch, tmp_path):
    manifest = UpgradeManifest(
        current="alpha2",
        versions=(
            _version("alpha1", ("ALL",)),
            _version("alpha2", ("Scripts",)),
        ),
    )
    panel = _panel(monkeypatch, tmp_path, manifest=manifest, snam="alpha2")
    panel.upgrade = True

    assert panel.upgrade_plan_preview() == (
        "Will regenerate: Scripts -> swap Misc; reuse rest"
    )


def test_upgrade_preview_reports_forced_clean_build(monkeypatch, tmp_path):
    manifest = UpgradeManifest(
        current="alpha2",
        versions=(
            _version("alpha1", ("ALL",)),
            _version("alpha2", ("Meshes",), force_regen=True),
        ),
    )
    panel = _panel(monkeypatch, tmp_path, manifest=manifest, snam="alpha1")

    assert panel.upgrade_plan_preview() == (
        "Full clean build required by this upgrade (local output will be cleared)."
    )


def test_missing_manifest_degrades_to_full_build_without_crashing(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, manifest=None, snam=None)
    panel.upgrade = True

    options = panel.build_options()

    assert options.upgrade is False
    assert panel.upgrade_plan_preview() == "No upgrade manifest found - full build only."


def test_detected_installed_version_no_stamp_maps_to_alpha1(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, snam=None)

    assert panel._detected_installed_version() == "alpha1"


def test_detected_installed_version_returns_actual_stamp(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, snam="alpha2")

    assert panel._detected_installed_version() == "alpha2"


def test_draw_header_reports_installed_and_game_version_without_crashing(monkeypatch, tmp_path):
    panel = _panel(monkeypatch, tmp_path, snam="alpha1")
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._detect_ba2_target",
        lambda self: ("nextgen", "1.10.984"),
    )

    panel._draw_header()
