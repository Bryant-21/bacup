import sys, os, json, tempfile, pathlib
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from ui.toolkit.settings import ToolkitSettings


def _make_settings(data: dict) -> ToolkitSettings:
    tmpdir = pathlib.Path(tempfile.mkdtemp())
    shared_path = tmpdir / "shared_settings.json"
    variant_path = tmpdir / "full.json"
    shared_path.write_text(json.dumps(data), encoding="utf-8")
    return ToolkitSettings(
        shared_path=shared_path,
        variant_path=variant_path,
        editor_settings_path="/nonexistent",
    )


def test_indexes_defaults_all_true_when_key_missing():
    s = _make_settings({})
    assert s.indexes == {
        "fo4_data": True,
        "scripts": True,
        "wiki": True,
        "nifs": True,
        "behaviors": True,
        "swf": True,
        "voice_reference": True,
    }


def test_indexes_loaded_from_file():
    s = _make_settings(
        {"indexes": {"fo4_data": False, "scripts": False, "nifs": True, "behaviors": False}}
    )
    assert s.indexes["fo4_data"] is False
    assert s.indexes["scripts"] is False
    assert s.indexes["wiki"] is True
    assert s.indexes["behaviors"] is False
    assert s.indexes["nifs"] is True


def test_indexes_partial_missing_key_defaults_true():
    s = _make_settings({"indexes": {"fo4_data": False}})
    assert s.indexes["scripts"] is True
    assert s.indexes["wiki"] is True
    assert s.indexes["nifs"] is True
    assert s.indexes["behaviors"] is True


def test_indexes_saved_and_reloaded():
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
        path = f.name
    s = ToolkitSettings(path=path, editor_settings_path="/nonexistent")
    s.indexes = {"fo4_data": False, "scripts": False, "nifs": False, "behaviors": True}
    s.save()
    s2 = ToolkitSettings(path=path, editor_settings_path="/nonexistent")
    pathlib.Path(path).unlink(missing_ok=True)
    assert s2.indexes["fo4_data"] is False
    assert s2.indexes["scripts"] is False
    assert s2.indexes["wiki"] is True
    assert s2.indexes["nifs"] is False
    assert s2.indexes["behaviors"] is True


def test_indexes_setter_replaces_dict():
    s = _make_settings({})
    s.indexes = {"fo4_data": False, "scripts": False, "nifs": False, "behaviors": False}
    assert s.indexes["fo4_data"] is False
    assert s.indexes["scripts"] is False
    assert s.indexes["wiki"] is True


# ---------------------------------------------------------------------------
# SettingsWindow extract status fields
# ---------------------------------------------------------------------------

import json as _json
from unittest.mock import MagicMock

# Patch imgui_bundle before importing settings_window (no display needed)
sys.modules.setdefault("imgui_bundle", MagicMock())
sys.modules.setdefault("imgui_bundle.imgui", MagicMock())

from creation_lib.ui.shell import SettingsWindow


def _make_settings_window(tmp_path, extracted_dir=""):
    cfg = tmp_path / "toolkit_settings.json"
    s = ToolkitSettings(path=cfg, editor_settings_path=tmp_path / "old.json")
    if extracted_dir:
        s._paths["fo4"]["extracted_dir"] = extracted_dir
    sw = SettingsWindow(s)
    return sw


def test_extract_status_fields_exist_in_init(tmp_path):
    sw = _make_settings_window(tmp_path)
    assert hasattr(sw, "_extract_status")
    assert hasattr(sw, "_extract_up_to_date")
    assert hasattr(sw, "_extract_last_date")
    assert sw._extract_status == ""
    assert sw._extract_up_to_date is False
    assert sw._extract_last_date == ""


def test_settings_window_can_hide_indexes_section(tmp_path):
    from creation_lib.ui.shell import SettingsSection

    def _noop_draw(ctx):
        pass

    sw = _make_settings_window(tmp_path)
    sw.register_section(SettingsSection(id="general", label="General", draw=_noop_draw))
    sw.register_section(SettingsSection(id="paths", label="Paths", draw=_noop_draw))
    sw.register_section(SettingsSection(id="indexes", label="Indexes", draw=_noop_draw))

    hidden = SettingsWindow(sw._settings, include_indexes=False)
    hidden.register_section(SettingsSection(id="general", label="General", draw=_noop_draw))
    hidden.register_section(SettingsSection(id="paths", label="Paths", draw=_noop_draw))
    # indexes not registered when include_indexes=False

    assert "indexes" in sw._fixed_and_workspace_sections()
    assert "indexes" not in hidden._fixed_and_workspace_sections()


def test_load_settings_no_extracted_dir(tmp_path):
    sw = _make_settings_window(tmp_path, extracted_dir="")
    sw._index_game = "fo4"
    sw.load_settings()
    assert "No extracted data" in sw._extract_status
    assert sw._extract_up_to_date is False


def test_load_settings_extracted_dir_missing(tmp_path):
    missing = str(tmp_path / "nonexistent")
    sw = _make_settings_window(tmp_path, extracted_dir=missing)
    sw._index_game = "fo4"
    sw.load_settings()
    assert "No extracted data" in sw._extract_status
    assert sw._extract_up_to_date is False


def test_load_settings_no_manifest_file(tmp_path):
    # extracted_dir exists but no .ba2_manifest.json
    extracted = tmp_path / "extracted" / "fo4"
    extracted.mkdir(parents=True)
    sw = _make_settings_window(tmp_path, extracted_dir=str(extracted))
    sw._index_game = "fo4"
    sw.load_settings()
    assert "No extracted data" in sw._extract_status
    assert sw._extract_up_to_date is False


def test_load_settings_up_to_date(tmp_path):
    extracted = tmp_path / "extracted" / "fo4"
    extracted.mkdir(parents=True)

    # Fake game Data/ dir with one BA2
    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    ba2 = data_dir / "Fallout4.ba2"
    ba2.write_bytes(b"x" * 50)
    st = ba2.stat()

    manifest = {
        "game": "fo4",
        "source_dir": str(data_dir),
        "extracted_at": "2026-03-21T10:00:00",
        "archives": {"Fallout4.ba2": {"size": st.st_size, "mtime": st.st_mtime}},
        "papyrus_source": {"exists": False},
    }
    (extracted / ".ba2_manifest.json").write_text(_json.dumps(manifest))

    # Point game root to tmp_path (Data/ is child)
    s_cfg = tmp_path / "tk.json"
    s = ToolkitSettings(path=s_cfg, editor_settings_path=tmp_path / "old.json")
    s._paths["fo4"]["extracted_dir"] = str(extracted)
    s._paths["fo4"]["root_dir"] = str(tmp_path)

    sw = SettingsWindow(s)
    sw._index_game = "fo4"
    sw.load_settings()
    assert sw._extract_up_to_date is True
    assert "Up to date" in sw._extract_status
    assert "2026-03-21" in sw._extract_last_date


def test_load_settings_updates_available(tmp_path):
    extracted = tmp_path / "extracted" / "fo4"
    extracted.mkdir(parents=True)

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    ba2 = data_dir / "Fallout4.ba2"
    ba2.write_bytes(b"x" * 50)

    # Manifest has stale mtime
    manifest = {
        "game": "fo4",
        "source_dir": str(data_dir),
        "extracted_at": "2026-03-20T10:00:00",
        "archives": {"Fallout4.ba2": {"size": 50, "mtime": 0.0}},  # wrong mtime
        "papyrus_source": {"exists": False},
    }
    (extracted / ".ba2_manifest.json").write_text(_json.dumps(manifest))

    s_cfg = tmp_path / "tk.json"
    s = ToolkitSettings(path=s_cfg, editor_settings_path=tmp_path / "old.json")
    s._paths["fo4"]["extracted_dir"] = str(extracted)
    s._paths["fo4"]["root_dir"] = str(tmp_path)

    sw = SettingsWindow(s)
    sw._index_game = "fo4"
    sw.load_settings()
    assert sw._extract_up_to_date is False
    assert "Updates available" in sw._extract_status
