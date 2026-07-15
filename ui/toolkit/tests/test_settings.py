"""Tests for the unified settings manager."""

import json

import pytest

from ui.toolkit.settings import ToolkitSettings


@pytest.fixture
def tmp_settings(tmp_path):
    # Pass a nonexistent editor_settings_path so migration is a no-op in isolated tests
    return ToolkitSettings(
        path=tmp_path / "toolkit_settings.json",
        editor_settings_path=tmp_path / "editor_settings.json",
    )


def test_defaults_when_no_file(tmp_settings):
    """Settings returns defaults when no file exists."""
    s = tmp_settings
    assert s.active_workspace == "nif"
    assert s.theme == "falloutnv"
    assert s.get_workspace_settings("nif") == {}
    assert s.get_workspace_settings("behavior") == {}


def test_save_and_load(tmp_path):
    """Settings round-trip through JSON."""
    path = tmp_path / "toolkit_settings.json"
    s1 = ToolkitSettings(path=path)
    s1.active_workspace = "behavior"
    s1.set_workspace_settings("nif", {"fov": 60.0})
    s1.set_workspace_settings("behavior", {"recent_files": ["a.json"]})
    s1.save()

    s2 = ToolkitSettings(path=path)
    assert s2.active_workspace == "behavior"
    assert s2.get_workspace_settings("nif") == {"fov": 60.0}
    assert s2.get_workspace_settings("behavior") == {"recent_files": ["a.json"]}


def test_merge_workspace_settings(tmp_settings):
    """set_workspace_settings merges, does not replace."""
    s = tmp_settings
    s.set_workspace_settings("nif", {"fov": 45.0})
    s.set_workspace_settings("nif", {"nav_style": "blender"})
    ws = s.get_workspace_settings("nif")
    assert ws["fov"] == 45.0
    assert ws["nav_style"] == "blender"


def test_apply_defaults(tmp_settings):
    """apply_defaults fills missing keys without overwriting existing."""
    s = tmp_settings
    s.set_workspace_settings("nif", {"fov": 60.0})
    s.apply_defaults("nif", {"fov": 45.0, "nav_style": "3dsmax"})
    ws = s.get_workspace_settings("nif")
    assert ws["fov"] == 60.0  # kept existing
    assert ws["nav_style"] == "3dsmax"  # filled default


def test_paths_defaults(tmp_settings):
    """paths returns correct defaults."""
    s = tmp_settings
    assert s.get_fo4_paths() == {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "scripts_user_dir": "",
        "scripts_base_dir": "",
        "installs": [],
    }
    assert s.get_fo76_paths() == {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "scripts_user_dir": "",
        "scripts_base_dir": "",
    }


def test_paths_roundtrip(tmp_path):
    """paths persists and loads correctly."""
    path = tmp_path / "toolkit_settings.json"
    s1 = ToolkitSettings(path=path)
    s1._paths["fo4"]["root_dir"] = "C:/games/fo4"
    s1._paths["fo4"]["additional_paths"] = ["C:/mods"]
    s1.save()

    s2 = ToolkitSettings(path=path)
    assert s2.get_fo4_paths()["root_dir"] == "C:/games/fo4"
    assert s2.get_fo4_paths()["additional_paths"] == ["C:/mods"]


def test_paths_migrate_from_editor_settings(tmp_path):
    """When paths key absent, migrates extra_paths from editor_settings.json."""
    settings_path = tmp_path / "toolkit_settings.json"
    editor_path = tmp_path / "editor_settings.json"
    editor_path.write_text(json.dumps({"extra_paths": ["C:/mod1", "C:/mod2"]}))
    s = ToolkitSettings(path=settings_path, editor_settings_path=editor_path)
    assert s.get_fo4_paths()["additional_paths"] == ["C:/mod1", "C:/mod2"]


def test_paths_no_migration_when_key_present(tmp_path):
    """Migration is skipped when paths key already exists in JSON."""
    settings_path = tmp_path / "toolkit_settings.json"
    editor_path = tmp_path / "editor_settings.json"
    editor_path.write_text(json.dumps({"extra_paths": ["C:/mod1"]}))
    # Write settings with paths key already present
    settings_path.write_text(
        json.dumps(
            {
                "paths": {
                    "fallout4": {
                        "root_dir": "",
                        "extracted_dir": "",
                        "additional_paths": [],
                    },
                    "fallout76": {
                        "root_dir": "",
                        "extracted_dir": "",
                        "additional_paths": [],
                    },
                }  # legacy keys — migration tested
            }
        )
    )
    s = ToolkitSettings(path=settings_path, editor_settings_path=editor_path)
    assert s.get_fo4_paths()["additional_paths"] == []  # migration was skipped


def test_script_source_paths_defaults(tmp_settings):
    """script_source_paths defaults to empty list."""
    assert tmp_settings.get_script_source_paths() == []


def test_script_source_paths_roundtrip(tmp_path):
    """script_source_paths persists and loads correctly."""
    path = tmp_path / "toolkit_settings.json"
    s1 = ToolkitSettings(path=path)
    s1.set_script_source_paths(["C:/fo4/scripts", "C:/mods/scripts"])
    s1.save()

    s2 = ToolkitSettings(path=path)
    assert s2.get_script_source_paths() == ["C:/fo4/scripts", "C:/mods/scripts"]


def test_script_source_paths_missing_key_loads_empty(tmp_path):
    """Old settings files without script_source_paths load as empty list."""
    path = tmp_path / "toolkit_settings.json"
    path.write_text(
        json.dumps(
            {
                "paths": {
                    "fallout4": {
                        "root_dir": "",
                        "extracted_dir": "",
                        "additional_paths": [],
                    },
                    "fallout76": {
                        "root_dir": "",
                        "extracted_dir": "",
                        "additional_paths": [],
                    },  # legacy keys
                }
            }
        )
    )
    s = ToolkitSettings(path=path)
    assert s.get_script_source_paths() == []
