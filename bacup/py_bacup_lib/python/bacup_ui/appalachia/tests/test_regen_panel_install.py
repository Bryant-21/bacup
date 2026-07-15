from pathlib import Path
from types import SimpleNamespace

from bacup_ui.appalachia.tests.test_regen_panel_options import _panel, _ws
from bacup_ui.conversion.panels.regen_panel import _INSTALL_LOCATION_KEY, RegenPanel

_DOCS = Path.home() / "Documents" / "My Games" / "Fallout4"


def test_mo2_install_resolves_deploy_and_profile_ini(tmp_path, monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    instance = tmp_path
    install_path = instance / "mods" / "SeventySix"
    install_path.mkdir(parents=True)
    (instance / "ModOrganizer.ini").write_text(
        "[General]\nselected_profile=MyProfile\n", encoding="utf-8"
    )
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "mo2"
    panel.install_path = str(install_path)
    panel.mo2_use_profile_ini = True

    paths = panel.build_paths()

    assert paths.deploy_data_dir == install_path
    assert paths.runtime_ini_path == instance / "profiles" / "MyProfile" / "fallout4custom.ini"


def test_mo2_global_ini_when_profile_ini_disabled(tmp_path, monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    install_path = tmp_path / "mods" / "SeventySix"
    install_path.mkdir(parents=True)
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "mo2"
    panel.install_path = str(install_path)
    panel.mo2_use_profile_ini = False

    paths = panel.build_paths()

    assert paths.deploy_data_dir == install_path
    assert paths.runtime_ini_path == _DOCS / "Fallout4Custom.ini"


def test_vortex_install_deploys_and_uses_docs_ini(tmp_path, monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "vortex"
    panel.install_path = str(tmp_path)

    paths = panel.build_paths()

    assert paths.deploy_data_dir == tmp_path
    assert paths.runtime_ini_path == _DOCS / "Fallout4Custom.ini"


def test_none_install_disables_deploy_in_options():
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "none"

    assert panel.build_options().deploy is False


def test_install_location_change_persists_to_workspace_settings():
    ws = _ws("C:/FO4", "C:/FO76", "C:/x/fo76", {})
    panel = RegenPanel(ws)

    # Simulate the combo-changed branch in _draw_settings_column.
    panel.install_location = "mo2"
    panel.deploy = panel.install_location != "none"
    panel._set_workspace_settings({_INSTALL_LOCATION_KEY: panel.install_location})

    assert ws._workspace_settings[_INSTALL_LOCATION_KEY] == "mo2"


def test_run_install_audit_calls_audit_with_resolved_target(tmp_path, monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    instance = tmp_path
    install_path = instance / "mods" / "SeventySix"
    install_path.mkdir(parents=True)
    (instance / "ModOrganizer.ini").write_text(
        "[General]\nselected_profile=MyProfile\n", encoding="utf-8"
    )
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "mo2"
    panel.install_path = str(install_path)

    recorded = {}

    def fake_audit(*, deploy_dir, ini_path, mod_name, plugin_name):
        recorded.update(
            deploy_dir=deploy_dir,
            ini_path=ini_path,
            mod_name=mod_name,
            plugin_name=plugin_name,
        )
        return SimpleNamespace(note=None, rows=[], missing_registration=[], ini_path=ini_path)

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.audit_archive_ini", fake_audit)

    panel._run_install_audit()

    assert panel._install_audit_error is None
    assert recorded["deploy_dir"] == install_path
    assert recorded["ini_path"] == instance / "profiles" / "MyProfile" / "fallout4custom.ini"
    assert recorded["mod_name"] == "SeventySix"
    assert recorded["plugin_name"] == "SeventySix.esm"


def test_run_install_audit_none_mode_uses_output_root(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "none"

    recorded = {}

    def fake_audit(*, deploy_dir, ini_path, mod_name, plugin_name):
        recorded.update(deploy_dir=deploy_dir, ini_path=ini_path)
        return SimpleNamespace(note=None, rows=[], missing_registration=[], ini_path=ini_path)

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.audit_archive_ini", fake_audit)

    panel._run_install_audit()

    assert recorded["deploy_dir"] == Path("X:/app/mods/SeventySix")
    assert recorded["ini_path"] is None


def test_repair_install_ini_passes_deployed_archives(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "game"

    ini_path = Path("C:/docs/Fallout4Custom.ini")
    panel._install_audit = SimpleNamespace(
        ini_path=ini_path,
        missing_registration=["SeventySix - Main.ba2", "SeventySix - Textures.ba2"],
        rows=[
            SimpleNamespace(
                name="SeventySix - Main.ba2", kind="ba2", deployed=True
            ),
            SimpleNamespace(
                name="SeventySix - Textures.ba2", kind="ba2", deployed=True
            ),
        ],
    )

    repaired = {}

    def fake_repair(*, ini_path, base_ini_path, archive_names, plugin_name):
        repaired.update(
            ini_path=ini_path,
            base_ini_path=base_ini_path,
            archive_names=archive_names,
            plugin_name=plugin_name,
        )
        return archive_names

    audited = {"count": 0}

    def fake_audit(*, deploy_dir, ini_path, mod_name, plugin_name):
        audited["count"] += 1
        return SimpleNamespace(note=None, rows=[], missing_registration=[], ini_path=ini_path)

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.repair_archive_ini", fake_repair)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.audit_archive_ini", fake_audit)

    panel._repair_install_ini()

    assert repaired["ini_path"] == ini_path
    assert repaired["base_ini_path"] == _DOCS / "Fallout4.ini"
    assert repaired["archive_names"] == [
        "SeventySix - Main.ba2",
        "SeventySix - Textures.ba2",
    ]
    assert repaired["plugin_name"] == "SeventySix.esm"
    assert audited["count"] == 1  # re-audits after repair
