"""Catalog editor tool shell: action handlers, dirty guard and a headless frame, with a fake model."""

from __future__ import annotations

import copy
import sys
import types
from dataclasses import dataclass, field
from pathlib import Path

import pytest

from ui.tools.catalog_editor.tool import SETTINGS_TAB, CatalogEditorTool


@dataclass
class Row:
    key: str
    entry: dict
    source: str = "Base.esm"
    status: str = "inherited"
    inherited: bool = True


@dataclass
class Problem:
    feature: str
    section: str
    key: str
    message: str


class FakeDoc:
    schema = {"type": "object"}

    def __init__(self, name, kinds):
        self.name = name
        self.kinds = kinds
        self.data = {section: {} for section in kinds}
        self.edit_count = 0
        self.problems: list[Problem] = []
        self.saved = 0
        self.setting_values = {"enabled": True}
        self.author_settings: set[str] = set()

    @property
    def dirty(self):
        return self.edit_count > 0

    def sections(self):
        return list(self.kinds)

    def section_kind(self, section):
        return self.kinds[section]

    def entry_schema(self, section):
        return {"type": "object"}

    def section_keys(self, section):
        return list(self.data[section])

    def rows(self, section):
        return list(self.data[section].values())

    def add(self, section, key, entry, status):
        self.data[section][key] = Row(key, entry, "Base.esm" if status == "inherited" else "Mod.esp", status,
                                      status == "inherited")

    def set_entry(self, section, value, map_key=None):
        key = map_key or value.get("form", "new")
        status = "overridden" if key in self.data[section] and self.data[section][key].status != "added" else "added"
        self.add(section, key, value, status)
        self.edit_count += 1
        return key

    def remove(self, section, key):
        row = self.data[section][key]
        if row.status == "added":
            del self.data[section][key]
        else:
            self.data[section][key] = Row(key, row.entry, row.source, "removed", False)
        self.edit_count += 1

    def revert(self, section, key):
        self.data[section][key].status = "inherited"
        self.edit_count = max(0, self.edit_count - 1)

    def settings(self):
        return [(name, value, "Mod.esp" if name in self.author_settings else "Base.esm", name in self.author_settings)
                for name, value in self.setting_values.items()]

    def settings_schema(self):
        return {"properties": {"enabled": {"type": "boolean"}}}

    def set_setting(self, name, value):
        self.setting_values[name] = value
        self.author_settings.add(name)
        self.edit_count += 1

    def revert_setting(self, name):
        self.author_settings.discard(name)

    def validate(self, forms):
        return list(self.problems)

    def save(self):
        self.saved += 1
        self.edit_count = 0
        return Path(f"{self.name}/Mod.esp.json")


class FakeSession:
    def __init__(self):
        self.plugin = types.SimpleNamespace(name="Mod.esp")
        self.masters = [types.SimpleNamespace(name="Base.esm", path="C:/Data/Base.esm"),
                        types.SimpleNamespace(name="Gone.esm", path=None)]
        self.forms = object()
        self.docs = {"Perks": FakeDoc("Perks", {"perks": "array", "named": "map", "raw": "opaque"}),
                     "Emotes": FakeDoc("Emotes", {"emotes": "array"})}
        self.docs["Perks"].add("perks", "A", {"form": "A"}, "inherited")
        self.folders: list[str] = []

    @property
    def missing_masters(self):
        return [m.name for m in self.masters if m.path is None]

    def features(self):
        return list(self.docs)

    def feature(self, name):
        return self.docs[name]

    def dirty_features(self):
        return [name for name, doc in self.docs.items() if doc.dirty]

    def add_master_folder(self, path):
        self.folders.append(path)


@pytest.fixture
def fake_modules(monkeypatch):
    render = types.ModuleType("ui.tools.catalog_editor.render")
    render.default_value = lambda node, schema: {"form": "New"}
    render.summary = lambda node, entry, schema: str(entry.get("form", ""))
    render.RenderContext = lambda *args, **kwargs: types.SimpleNamespace(args=args, kwargs=kwargs)
    render.draw_value = lambda label, node, value, ctx, id_path: (False, value)
    render.humanize = lambda name: name
    picker = types.ModuleType("ui.tools.catalog_editor.picker")
    picker.Picker = lambda forms: types.SimpleNamespace(forms=forms, draw=lambda: None)
    monkeypatch.setitem(sys.modules, "ui.tools.catalog_editor.render", render)
    monkeypatch.setitem(sys.modules, "ui.tools.catalog_editor.picker", picker)
    # Once the real submodules are imported, `from ui.tools.catalog_editor import render` reads the package attribute.
    package = sys.modules["ui.tools.catalog_editor"]
    monkeypatch.setattr(package, "render", render, raising=False)
    monkeypatch.setattr(package, "picker", picker, raising=False)
    return render


@pytest.fixture
def tool(fake_modules):
    tool = CatalogEditorTool()
    tool._session = FakeSession()
    tool._picker = fake_modules and sys.modules["ui.tools.catalog_editor.picker"].Picker(tool._session.forms)
    tool._feature = "Perks"
    tool._section = "perks"
    yield tool
    tool.cleanup()


def row_of(tool, key):
    return tool._session.docs["Perks"].data["perks"][key]


def test_override_copies_the_inherited_entry(tool):
    tool.override_row("perks", row_of(tool, "A"))
    assert row_of(tool, "A").status == "overridden"
    assert tool._selected_key == "A"


def test_remove_inherited_marks_removed_and_revert_restores(tool):
    tool.remove_row("perks", row_of(tool, "A"))
    assert row_of(tool, "A").status == "removed"
    tool.revert_row("perks", row_of(tool, "A"))
    assert row_of(tool, "A").status == "inherited"


def test_new_entry_in_array_and_map_sections(tool):
    assert tool.new_entry("perks") == "New"
    assert row_of(tool, "New").status == "added"
    assert tool.new_entry("named", "") is None and "name" in tool._error_msg
    assert tool.new_entry("named", " Fire ") == "Fire"


def test_edit_that_changes_the_key_replaces_an_added_row(tool):
    tool.new_entry("perks")
    tool.edit_entry("perks", row_of(tool, "New"), {"form": "Renamed"})
    assert "New" not in tool._session.docs["Perks"].data["perks"]
    assert tool._selected_key == "Renamed"


def test_validate_checks_dirty_features_and_reports_problems(tool):
    doc = tool._session.docs["Perks"]
    doc.problems = [Problem("Perks", "perks", "A", "FormKey does not resolve")]
    assert len(tool.validate()) == 1
    assert "1 problem" in tool._error_msg
    doc.problems = []
    assert tool.validate() == []
    assert tool._result_msg == "No problems found."


def test_save_refuses_with_problems_and_writes_nothing(tool):
    doc = tool._session.docs["Perks"]
    tool.new_entry("perks")
    doc.problems = [Problem("Perks", "perks", "New", "bad")]
    assert tool.save(everything=False) == []
    assert doc.saved == 0 and tool._problems


def test_save_all_writes_only_changed_features(tool):
    tool.new_entry("perks")
    saved = tool.save(everything=True)
    assert [path.parts[0] for path in saved] == ["Perks"]
    assert tool._session.docs["Emotes"].saved == 0
    assert not tool.has_unsaved_changes()
    assert tool.save(everything=True) == [] and tool._result_msg == "Nothing to save."


def test_dirty_guard_defers_the_action_until_decided(tool):
    ran = []
    tool._guarded(lambda: ran.append(1))
    assert ran == [1] and not tool._open_guard
    tool.new_entry("perks")
    tool._guarded(lambda: ran.append(2))
    assert ran == [1] and tool._open_guard and tool._guarded_action is not None


def test_goto_problem_selects_feature_section_and_key(tool):
    tool.goto_problem(Problem("Emotes", "emotes", "Wave", "dup"))
    assert (tool._feature, tool._section, tool._selected_key) == ("Emotes", "emotes", "Wave")


def test_add_master_folder_is_remembered_in_settings(tool):
    tool.add_master_folder("D:/Masters")
    assert tool._session.folders == ["D:/Masters"]
    assert tool.collect_settings()["extra_master_dirs"] == ["D:/Masters"]
    fresh = CatalogEditorTool()
    fresh.apply_settings(tool.collect_settings())
    assert fresh._extra_master_dirs == ["D:/Masters"]
    fresh.cleanup()


def test_open_plugin_runs_the_session_on_a_worker_and_polls_it_in(tool, monkeypatch):
    tool._session = None
    model = types.ModuleType("ui.tools.catalog_editor.model")
    seen = {}

    class CatalogSession:
        @staticmethod
        def open(path, **kwargs):
            seen["path"], seen["kwargs"] = path, kwargs
            return FakeSession()

    model.CatalogSession = CatalogSession
    monkeypatch.setitem(sys.modules, "ui.tools.catalog_editor.model", model)
    tool._extra_master_dirs = ["D:/Masters"]
    tool.bacup_output_dir_provider = lambda: "X:/bacup/mods/SeventySix"
    tool.open_plugin("C:/mods/Mod.esp")
    tool._loading.result(timeout=5)
    tool._poll()
    assert seen["path"] == "C:/mods/Mod.esp"
    assert seen["kwargs"]["bacup_output_dir"] == "X:/bacup/mods/SeventySix"
    assert seen["kwargs"]["extra_master_dirs"] == ("D:/Masters",)
    assert tool._feature == "Perks" and tool._last_plugin == "C:/mods/Mod.esp"
    assert "1 master(s) not found" in tool._result_msg


@pytest.fixture
def frames(tool):
    from imgui_bundle import imgui

    context = imgui.create_context()
    io = imgui.get_io()
    io.display_size = imgui.ImVec2(1600, 1000)
    io.delta_time = 1 / 60
    io.backend_flags |= imgui.BackendFlags_.renderer_has_textures

    def run(count=3):
        for _ in range(count):
            imgui.new_frame()
            imgui.begin("test")
            tool.draw_content()
            imgui.end()
            imgui.end_frame()

    try:
        yield run
    finally:
        imgui.destroy_context(context)


def test_headless_frames_draw_every_section_kind(tool, frames):
    tool._session.docs["Perks"].add("named", "Fire", {"x": 1}, "added")
    tool._session.docs["Perks"].add("raw", "r", {"y": 2}, "inherited")
    for section in ("perks", "named", "raw", SETTINGS_TAB):
        tool._select_section(section)
        tool._selected_key = "A" if section == "perks" else None
        frames()
    tool._problems = [Problem("Perks", "perks", "A", "bad")]
    frames()
    tool._guarded_action = lambda: None
    tool._open_guard = True
    frames()


def test_splitter_widths_persist_in_the_tool_settings(tool):
    defaults = tool.get_default_settings()["panes"]
    assert defaults["inspector"] >= 0.35
    tool.drag_splitter("inspector", -100, 1000)
    tool.drag_splitter("browser", 20, 1000)
    saved = tool.collect_settings()
    assert saved["panes"]["inspector"] == pytest.approx(defaults["inspector"] + 0.1)
    assert saved["panes"]["browser"] == pytest.approx(defaults["browser"] + 0.02)
    fresh = CatalogEditorTool()
    fresh.apply_settings(saved)
    assert fresh._panes == saved["panes"]
    fresh.cleanup()


def test_splitters_cannot_squeeze_the_table_away(tool):
    tool.drag_splitter("inspector", -10_000, 1000)
    tool.drag_splitter("browser", 10_000, 1000)
    assert tool._panes["browser"] + tool._panes["inspector"] <= 0.81
    assert tool._panes["inspector"] >= 0.2


def test_headless_frames_keep_the_widths_and_draw_the_popped_out_editor(tool, frames):
    from imgui_bundle import imgui

    tool._select_section("perks")
    tool._selected_key = "A"
    frames()
    tool._popped_out = True
    frames()
    assert imgui.internal.find_window_by_name("Inspector###catalog_editor_popout") is not None
    assert tool.collect_settings()["editor_popped_out"] is True
    tool.apply_settings({"editor_popped_out": False})
    frames()
    assert tool._popped_out is False


def test_view_json_shows_the_row_entry_exactly(tool, frames):
    import json

    tool._select_section("perks")
    tool._selected_key = "A"
    frames()
    row = row_of(tool, "A")
    tool._json.show("A", row.entry)
    frames()
    assert tool._json._text == json.dumps(row.entry, indent=2)


def test_about_lives_in_a_drawer_that_starts_closed(tool, frames):
    from imgui_bundle import imgui

    tool._session.docs["Perks"].schema = {"description": "What perks do.", "x-help": "More.\n\nEven more."}
    frames()
    assert not tool._about.is_open
    assert tool._about_paragraphs(tool._current_document()) == ["What perks do.", "More.", "Even more."]
    tool._about.open(tool._draw_about_text, "About Perks")
    frames(30)
    assert tool._about.is_open and tool._about._open_pct == 1.0

    io = imgui.get_io()
    io.add_mouse_pos_event(1500, 500)
    io.add_mouse_button_event(0, True)
    frames(1)
    io.add_mouse_button_event(0, False)
    frames(30)
    assert not tool._about.is_open and tool._about._open_pct == 0.0
