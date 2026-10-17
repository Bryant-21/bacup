"""Exercise actual Dear ImGui widgets/input without game data or an OS window."""
from collections import Counter
from dataclasses import dataclass
import os
import subprocess
import threading
from pathlib import Path
from types import SimpleNamespace

from imgui_bundle import imgui
import pytest

from ui.mod_manager.workspace import ModManagerWorkspace
from creation_lib.mod_manager.controller import PreparedInstall
from creation_lib.mod_manager.conflicts import FileEntry, FileIndex, Provider
from creation_lib.mod_manager.fomod import Environment, Fomod
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.package import Package
from ui.toolkit.app import ToolkitApp
from .fixtures import make_mo2, make_package
from .test_mo2 import junction


@dataclass
class Item:
    label: str
    id: int
    window: str
    rect: tuple


class NativeUI:
    def __init__(self, monkeypatch):
        self.context = imgui.create_context()
        self.io = imgui.get_io()
        self.io.set_ini_filename(None)
        self.io.display_size = (1800, 1100)
        self.io.delta_time = .1
        self.io.backend_flags |= imgui.BackendFlags_.renderer_has_textures
        self.io.config_flags |= imgui.ConfigFlags_.nav_enable_keyboard
        self.io.config_flags |= imgui.ConfigFlags_.docking_enable
        self.io.config_debug_highlight_id_conflicts = True
        self.items = []
        for name in ("menu_item", "selectable", "checkbox", "button", "input_text",
                     "input_text_with_hint", "input_text_multiline", "combo", "slider_int",
                     "collapsing_header", "begin_tab_item", "begin_combo", "begin_menu", "tree_node_ex", "table_header"):
            original = getattr(imgui, name)
            def record(label, *args, _original=original, **kwargs):
                result = _original(label, *args, **kwargs)
                last = self.context.last_item_data
                if last.id_ and imgui.is_item_visible():
                    rect = last.rect
                    self.items.append(Item(label, last.id_, imgui.internal.get_current_window().name,
                                           (rect.min.x, rect.min.y, rect.max.x, rect.max.y)))
                return result
            monkeypatch.setattr(imgui, name, record)

    def frame(self, draw, *, mouse=None, down=None, unique=True):
        self.items = []
        if mouse is not None:
            self.io.add_mouse_pos_event(*mouse)
        if down is not None:
            self.io.add_mouse_button_event(0, down)
        imgui.new_frame()
        draw()
        for window in self.context.windows:
            if window.name not in {"Library##mod_manager", "Settings##mod_manager"} or not window.dock_node:
                continue
            bar = window.dock_node.tab_bar
            if not bar:
                continue
            for tab in bar.tabs:
                if tab.window == window:
                    left = bar.bar_rect.min.x + tab.offset - bar.scrolling_anim
                    self.items.append(Item(window.name.split("##")[0], tab.id_, "Manager tabs",
                                           (left, bar.bar_rect.min.y, left + tab.width, bar.bar_rect.max.y)))
        imgui.render()
        if unique:
            counts = Counter(item.id for item in self.items)
            duplicates = [item for item in self.items if counts[item.id] > 1]
            assert not duplicates, duplicates
            assert self.context.hovered_id_previous_frame_item_count <= 1
            assert self.context.debug_draw_id_conflicts_count == 0
        return self.items

    def warm(self, draw):
        for _ in range(3):
            self.frame(draw)

    def find(self, label, *, window="", id=None):
        found = [item for item in self.items if item.label == label and window in item.window
                 and (id is None or item.id == id)]
        assert len(found) == 1, (label, window, found)
        return found[0]

    def click(self, item, draw):
        x1, y1, x2, y2 = item.rect
        pos = ((x1 + x2) / 2, (y1 + y2) / 2)
        self.frame(draw, mouse=pos)
        self.frame(draw, mouse=pos, down=True)
        self.frame(draw, mouse=pos, down=False)
        self.frame(draw, mouse=pos)

    def key(self, key, draw):
        self.io.add_key_event(key, True)
        self.frame(draw)
        self.io.add_key_event(key, False)
        self.frame(draw)

    def text(self, item, draw, value):
        self.click(item, draw)
        self.io.add_key_event(imgui.Key.mod_ctrl, True)
        self.key(imgui.Key.a, draw)
        self.io.add_key_event(imgui.Key.mod_ctrl, False)
        if value:
            self.io.add_input_characters_utf8(value)
            self.frame(draw)
        else:
            self.key(imgui.Key.backspace, draw)
        self.key(imgui.Key.enter, draw)

    def choose_next(self, label, draw, count=1):
        self.click(self.find(label), draw)
        for _ in range(count):
            self.key(imgui.Key.down_arrow, draw)
        self.key(imgui.Key.enter, draw)

    def scroll(self, item, draw, amount):
        x1, y1, x2, y2 = item.rect
        self.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2))
        self.io.add_mouse_wheel_event(0, amount)
        self.frame(draw)
        self.warm(draw)


@pytest.fixture
def native(monkeypatch):
    native = NativeUI(monkeypatch)
    yield native
    imgui.destroy_context(native.context)


def menu_host(names):
    workspaces = {key: SimpleNamespace(id=key, name=name) for key, name in names.items()}
    host = SimpleNamespace(_ws_map=workspaces, _active_ws=next(iter(workspaces.values())), selected="")
    host._switch_workspace = lambda key: setattr(host, "selected", key)
    def draw():
        imgui.set_next_window_pos((0, 0))
        imgui.set_next_window_size((600, 400))
        imgui.begin("Workspace choices", flags=imgui.WindowFlags_.no_saved_settings)
        ToolkitApp._ws_menu_items(host, list(workspaces))
        imgui.end()
    return host, draw


def test_workspace_menu_duplicate_names_have_unique_stable_ids_and_click_target(native):
    host, draw = menu_host({"mod_builder": "Mod Manager", "mod_manager": "Mod Manager"})
    native.warm(draw)
    items = list(native.items)
    assert len(items) == 2 and items[0].id != items[1].id
    native.click(items[1], draw)
    assert host.selected == "mod_manager"
    ids = [item.id for item in native.items]
    host._ws_map["mod_manager"].name = "Modlist Manager"
    native.warm(draw)
    assert [item.id for item in native.items] == ids


def test_new_workspace_display_name_preserves_existing_manager():
    from ui.toolkit.workspaces import create_workspaces
    old, new = create_workspaces(workspace_ids=("mod_builder", "mod_manager"))
    assert old.name == "Mod Builder" and old.id == "mod_builder"
    assert new.name == "Modlist Manager" and new.id == "mod_manager"
    assert isinstance(new, ModManagerWorkspace)


def test_manual_mapping_and_owned_merge_review_apply_use_actual_input(native, workspace, tmp_path):
    from .test_transactions import setup_owned
    manager, draw = workspace.manager, panels(workspace)
    library, profile, mod, incoming = setup_owned(tmp_path / "merge fixture")
    (library.root / "mods" / mod / "keep.ini").write_text("generated unmatched owned content")
    manager.library, manager.profile_id = library, profile
    manager.refresh()
    manager.prepare_install(incoming, replace_id=mod)
    finish_job(workspace, native, draw)
    workspace._tab, workspace._install_name = "Install", "Fixture"
    native.warm(draw)
    native.choose_next("Content behavior", draw)
    assert manager.install_mode == "merge"
    native.click(native.find("Replace owned content in all affected profiles; keep backup"), draw)
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Customize destination mapping"), draw)
    native.click(native.find("extra.txt → extra.txt###mapping_source"), draw)
    native.text(native.find("##mapping_destination"), draw, "nested/custom.txt")
    native.click(native.find("Set destination"), draw)
    assert manager.install_plan is None
    native.click(native.find("Customize destination mapping"), draw)
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    assert {target for target, _ in manager.install_pairs} == {"file.txt", "keep.ini", "nested/custom.txt"}
    native.click(native.find("Merge with backup"), draw)
    finish_job(workspace, native, draw)
    owned = library.root / "mods" / mod
    assert (owned / "file.txt").read_text() == "replacement"
    assert (owned / "keep.ini").read_text() == "generated unmatched owned content"
    assert (owned / "nested/custom.txt").read_text() == "extra"
    assert incoming.joinpath("extra.txt").read_text() == "extra"
    assert len(library.history()) == 1


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_plugin_keyboard_filter_range_letter_toggle_review_cancel_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from .test_standalone import game_fixture
    from creation_lib.mod_manager.instances import create
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    game = game_fixture(tmp_path)
    for name in ("Fallout4.esm", "Alpha.esp", "Amber.esp", "Beta.esp"):
        (game / "Data" / name).write_bytes(b"generated plugin fixture")
    m.library = Library(tmp_path / "keyboard plugin library", create=True)
    m.profile_id = create(m.library, "fo4", game)
    m.refresh()
    original = m.library.profile_snapshot(m.profile_id)[0].files
    monkeypatch.setattr("creation_lib.mod_manager.plugin_state.modkit_metadata", lambda path, game_id, cancel: ({"flags": {"esm": path.suffix == ".esm"}}, () if path.name == "Fallout4.esm" else ("Fallout4.esm",)))
    workspace._right_pane = "Plugins"
    native.warm(draw)
    native.click(native.find("Refresh plugins"), draw)
    finish_job(workspace, native, draw)
    panes = workspace.panes
    selections = [item for item in native.items if item.label == "##plugin_selection"]
    native.click(selections[1], draw)
    assert panes.selected == "Alpha.esp"
    native.key(imgui.Key.a, draw)
    assert panes.selected == "Amber.esp"
    native.key(imgui.Key.a, draw)
    assert panes.selected == "Alpha.esp"
    native.io.add_key_event(imgui.Key.mod_shift, True)
    native.key(imgui.Key.down_arrow, draw)
    native.io.add_key_event(imgui.Key.mod_shift, False)
    native.frame(draw)
    assert panes.selected_plugins == {"Alpha.esp", "Amber.esp"}
    native.key(imgui.Key.space, draw)
    assert {"alpha.esp", "amber.esp"} <= panes.active and "fallout4.esm" in panes.active
    chord(native, draw, imgui.Key.a)
    assert panes.selected_plugins == set(panes.order)
    native.key(imgui.Key.space, draw)
    assert panes.active == {"fallout4.esm"}
    chord(native, draw, imgui.Key.f)
    assert native.io.want_text_input
    assert native.context.active_id == native.find("##plugin_filter").id
    native.text(native.find("##plugin_filter"), draw, "Beta")
    assert panes.query == "Beta" and len([item for item in native.items if item.label == "##plugin_selection"]) == 1
    native.click(native.find("##plugin_selection"), draw)
    chord(native, draw, imgui.Key.a)
    assert panes.selected_plugins == {"Beta.esp"}
    native.key(imgui.Key.space, draw)
    assert panes.active == {"fallout4.esm", "beta.esp"}
    native.click(native.find("Revert"), draw)
    assert m.library.profile_snapshot(m.profile_id)[0].files == original
    assert (game / "Data" / "Beta.esp").read_bytes() == b"generated plugin fixture"


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_plugin_order_locked_guard_provider_folder_nexus_and_typing_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from .test_standalone import game_fixture
    from creation_lib.mod_manager.instances import create
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    game = game_fixture(tmp_path)
    (game / "Data" / "Fallout4.esm").write_bytes(b"generated core fixture")
    source = tmp_path / "generated mod plugin package"
    source.mkdir()
    for name in ("Alpha.esp", "Amber.esp", "Beta.esp"):
        (source / name).write_bytes(b"generated mod plugin fixture")
    m.library = Library(tmp_path / "owned plugin keyboard library", create=True)
    m.profile_id = create(m.library, "fo4", game)
    package = Package(source)
    try:
        mod = m.library.install(package, package.plain_plan(), "Managed provider", m.profile_id, nexus={"game": "fallout4", "mod_id": 123, "file_id": 456})
    finally:
        package.close()
    m.refresh()
    monkeypatch.setattr("creation_lib.mod_manager.plugin_state.modkit_metadata", lambda path, game_id, cancel: ({"flags": {"esm": path.suffix == ".esm"}}, () if path.name == "Fallout4.esm" else ("Fallout4.esm",)))
    folders, links = [], []
    popen = subprocess.Popen
    def launch(args, **kwargs):
        if args[0] == "explorer.exe":
            return folders.append((args, kwargs))
        return popen(args, **kwargs)
    monkeypatch.setattr(subprocess, "Popen", launch)
    monkeypatch.setattr("creation_lib.mod_manager.nexus_links.browse", lambda url: links.append(url))
    workspace._right_pane = "Plugins"
    native.warm(draw)
    native.click(native.find("Refresh plugins"), draw)
    finish_job(workspace, native, draw)
    panes = workspace.panes
    native.click([item for item in native.items if item.label == "##plugin_selection"][1], draw)
    chord(native, draw, imgui.Key.enter)
    assert folders == [(["explorer.exe", str(m.library.root / "mods" / mod)], {"shell": False})]
    selection = [item for item in native.items if item.label == "##plugin_selection"][1]
    native.io.add_key_event(imgui.Key.mod_shift, True)
    x1, y1, x2, y2 = selection.rect
    native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2), down=True)
    native.frame(draw, down=False)
    native.frame(draw, down=True)
    native.frame(draw, down=False)
    native.io.add_key_event(imgui.Key.mod_shift, False)
    native.frame(draw)
    assert links == ["https://www.nexusmods.com/fallout4/mods/123"]
    native.key(imgui.Key.b, draw)
    assert panes.selected == "Beta.esp"
    chord(native, draw, imgui.Key.up_arrow)
    assert panes.order == ["Fallout4.esm", "Alpha.esp", "Beta.esp", "Amber.esp"]
    native.click(native.find("Lock priority"), draw)
    native.click([item for item in native.items if item.label == "##plugin_selection"][3], draw)
    before = list(panes.order)
    chord(native, draw, imgui.Key.up_arrow)
    assert panes.order == ["Fallout4.esm", "Amber.esp", "Beta.esp", "Alpha.esp"] and not m.error
    before = list(panes.order)
    native.click(native.find("##plugin_filter"), draw)
    native.key(imgui.Key.space, draw)
    native.key(imgui.Key.a, draw)
    assert panes.order == before
    assert panes.selected == "Amber.esp"


def test_owned_hide_unhide_buttons_apply_with_backup_using_actual_input(native, workspace, tmp_path):
    from .test_transactions import setup_owned
    manager, draw = workspace.manager, panels(workspace)
    library, profile, mod, _ = setup_owned(tmp_path / "hide fixture")
    manager.library, manager.profile_id = library, profile
    manager.refresh()
    manager.inspect_files()
    finish_job(workspace, native, draw)
    workspace._tab, workspace._file_selected = "Files", "file.txt"
    native.warm(draw)
    original = library.root.joinpath("mods", mod, "file.txt").read_bytes()
    native.click(native.find("Hide owned file###provider_hidden"), draw)
    finish_job(workspace, native, draw)
    assert library.root.joinpath("mods", mod, "file.txt.mohidden").read_bytes() == original
    native.click(native.find("Refresh file inspection"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Unhide owned file###provider_hidden"), draw)
    finish_job(workspace, native, draw)
    assert library.root.joinpath("mods", mod, "file.txt").read_bytes() == original
    assert len(library.history()) == 2


@pytest.mark.parametrize("outcome", ["success", "cancelled", "failed"])
def test_owned_save_refresh_header_filter_cancel_and_failure_use_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    from .test_standalone import game_fixture
    from .test_saves import save_fixture
    from creation_lib.mod_manager.instances import create
    manager, draw = workspace.manager, panels(workspace)
    manager.library = Library(tmp_path / "saves library", create=True)
    manager.profile_id = create(manager.library, "fo4", game_fixture(tmp_path))
    manager.refresh()
    root = manager.library.root / "runtime" / "profiles" / manager.profile_id / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    if outcome == "failed":
        path.write_bytes(b"generated malformed save")
    original = path.read_bytes()
    workspace._right_pane = "Saves"
    native.warm(draw)
    native.click(native.find("Refresh owned saves"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Fixture.fos###owned_save"), draw)
    started, release = threading.Event(), threading.Event()
    if outcome == "cancelled":
        from creation_lib.mod_manager.saves import fo4_header
        def blocked(save, cancel):
            started.set()
            assert release.wait(10)
            return fo4_header(save, cancel)
        monkeypatch.setattr("creation_lib.mod_manager.saves.fo4_header", blocked)
    try:
        native.click(native.find("Inspect save header"), draw)
        if outcome == "success":
            finish_job(workspace, native, draw)
            assert manager.save_details[str(path)]["player"] == "Fixture Character"
            native.text(native.find("##save_filter"), draw, "Character")
            assert native.find("Fixture.fos###owned_save")
        else:
            if outcome == "cancelled":
                assert started.wait(5)
                native.click(native.find("Cancel operation"), draw)
                release.set()
            manager._future.exception(timeout=10)
            manager.poll()
            native.warm(draw)
            assert manager.job_state == outcome and not manager.save_details
            if outcome == "failed":
                assert "header" in manager.error or "signature" in manager.error
        assert path.read_bytes() == original
    finally:
        release.set()


def test_owned_parent_folder_hide_unhide_use_actual_input(native, workspace, tmp_path):
    from .test_transactions import setup_owned
    manager, draw = workspace.manager, panels(workspace)
    library, profile, mod, _ = setup_owned(tmp_path / "parent fixture")
    root = library.root / "mods" / mod
    root.joinpath("textures", "empty").mkdir(parents=True)
    root.joinpath("textures", "fixture.txt").write_bytes(b"generated parent content")
    manager.library, manager.profile_id = library, profile
    manager.refresh()
    manager.inspect_files()
    finish_job(workspace, native, draw)
    workspace._tab, workspace._file_selected = "Files", "textures/fixture.txt"
    native.warm(draw)
    native.click(native.find("Hide parent folder###provider_parent_hidden"), draw)
    finish_job(workspace, native, draw)
    assert root.joinpath("textures.mohidden", "empty").is_dir()
    native.click(native.find("Refresh file inspection"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Unhide parent folder###provider_parent_hidden"), draw)
    finish_job(workspace, native, draw)
    assert root.joinpath("textures", "fixture.txt").read_bytes() == b"generated parent content"
    assert root.joinpath("textures", "empty").is_dir() and len(library.history()) == 2


def test_additional_fomod_source_tree_review_and_install_use_actual_input(native, workspace, tmp_path):
    manager, draw = workspace.manager, panels(workspace)
    workspace._prepare(str(make_package(tmp_path)))
    finish_job(workspace, native, draw)
    native.click(native.find("Dependency environment"), draw)
    unknown = next(item for item in native.items if item.label == "##file_state" and manager.prepared.environment.files.get("absent.esp", "Unknown") == "Unknown")
    native.click(unknown, draw)
    native.key(imgui.Key.down_arrow, draw)
    native.key(imgui.Key.enter, draw)
    assert manager.prepared.environment.files["absent.esp"] == "Missing"
    native.click(native.find("Dependency environment"), draw)
    native.click(native.find("Additional package sources"), draw)
    native.text(native.find("##package_source_filter"), draw, "high/extra")
    native.click(native.find("extra.txt###package_source_file"), draw)
    native.scroll(native.find("Additional package sources"), draw, -100000)
    native.text(native.find("##additional_destination"), draw, "extras/fixture.txt")
    native.click(native.find("Include package source"), draw)
    assert manager.install_additions == {"extras/fixture.txt": "high/extra.txt"}
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    assert manager.install_plan["extras/fixture.txt"] == "high/extra.txt"
    native.click(native.find("Install into library"), draw)
    finish_job(workspace, native, draw)
    owned = manager.library.root / "mods" / manager.selected_id
    assert (owned / "extras" / "fixture.txt").read_bytes() == b"extra"
    assert not manager.install_additions


def test_wizard_cache_does_not_sort_large_state_inventory_and_tracks_overrides(native, workspace, tmp_path, monkeypatch):
    manager, draw = workspace.manager, panels(workspace)
    workspace._prepare(str(make_package(tmp_path)))
    finish_job(workspace, native, draw)
    states = manager.prepared.environment.files
    states.update({f"textures/generated-{index}.txt": "Active" for index in range(50_000)})
    states["absent.esp"] = "Missing"
    monkeypatch.setattr(states, "items", lambda: pytest.fail("The wizard must not sort the full observed inventory each frame"))
    native.warm(draw)
    first = workspace._evaluation
    assert first is not None
    native.warm(draw)
    assert workspace._evaluation is first
    states["Fixture.esp"] = "Inactive"
    native.warm(draw)
    assert workspace._evaluation is not first and states["fixture.esp"] == "Inactive"


def test_category_settings_and_shared_assignment_use_actual_input(native, workspace):
    manager, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(native.find("Settings"), draw)
    native.click(native.find("Manage categories"), draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##category_name"), draw, "Appearance/Textures")
    native.click(native.find("Save category"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Appearance/Textures | 0 mods###library_category"), draw)
    native.text(native.find("##category_name"), draw, "Visual/Textures")
    native.click(native.find("Rename selected category"), draw)
    finish_job(workspace, native, draw)
    assert any(item.name == "Visual/Textures" for item in manager.categories)
    native.click(native.find("Back to Library"), draw)
    native.click(native.find("Library"), draw)
    row = next(item for item in manager.rows if item.name == "Base")
    manager.selected_id = row.id
    workspace._right_pane = "Information"
    native.warm(draw)
    native.text(native.find("##mod_category"), draw, "Visual/Textures")
    native.click(native.find("Assign category"), draw)
    finish_job(workspace, native, draw)
    assert manager.rows_by_id[row.id].category == "Visual/Textures"
    assert all(next(item for item in manager.library.rows(profile.id) if item.id == row.id).category == "Visual/Textures" for profile in manager.profiles)
    if not workspace._show_filters:
        native.click(native.find("Filters"), draw)
    native.click(native.find("Filter options"), draw)
    # All -> uncategorized -> Visual -> Visual/Textures (parent paths are included).
    native.choose_next("##category_filter", draw)
    assert workspace._category_filter == "" and row.id not in visible_rows(native, manager, workspace)
    native.choose_next("##category_filter", draw)
    assert workspace._category_filter == "Visual" and set(visible_rows(native, manager, workspace)) == {row.id}
    assert workspace.collect_settings()["category_filter"] == "Visual"
    native.click(native.find("Clear filters"), draw)
    assert workspace._category_filter is None and row.id in visible_rows(native, manager, workspace)
    assert manager.selected_id == row.id, "Clearing filters changed the selected information row"
    native.click(native.find("Additional categories"), draw)
    assert manager.selected_id == row.id, "Opening additional categories changed selection"
    native.text(native.find("##mod_categories"), draw, "Gameplay/Quests")
    assert manager.selected_id == row.id, "Typing categories changed the selected mod"
    assert "Gameplay/Quests" in workspace._mod_categories, (native.find("##mod_categories").rect, native.context.nav_window.name, native.io.want_text_input)
    native.click(native.find("Assign category"), draw)
    finish_job(workspace, native, draw)
    assert manager.rows_by_id[row.id].categories == ("Visual/Textures", "Gameplay/Quests")
    native.text(native.find("##search_mods"), draw, "Gameplay/Quests")
    assert set(visible_rows(native, manager, workspace)) == {row.id}


@pytest.mark.parametrize("outcome", ["backup", "profile", "review-cancel", "cancelled", "failed"])
def test_owned_save_copy_review_confirm_and_cancel_use_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    from .test_standalone import game_fixture
    from .test_saves import save_fixture
    from creation_lib.mod_manager.instances import create
    manager, draw = workspace.manager, panels(workspace)
    manager.library = Library(tmp_path / "copy library", create=True)
    manager.profile_id = create(manager.library, "fo4", game_fixture(tmp_path))
    other = manager.library.create_profile("Other", clone=manager.profile_id)
    manager.refresh()
    root = manager.library.root / "runtime" / "profiles" / manager.profile_id / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    path.with_suffix(".f4se").write_bytes(b"generated companion")
    original = path.read_bytes()
    workspace._right_pane = "Saves"
    native.warm(draw)
    native.click(native.find("Refresh owned saves"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Fixture.fos###owned_save"), draw)
    native.click(native.find("Copy / back up save"), draw)
    if outcome == "profile":
        native.choose_next("Destination", draw)
        assert workspace._save_copy_target == other
    native.click(native.find("Review save copy"), draw)
    finish_job(workspace, native, draw)
    assert manager.save_transfer_plan and not list(manager.library.root.joinpath(".staging").iterdir())
    if outcome == "review-cancel":
        native.click(native.find("Cancel save copy"), draw)
        assert manager.save_transfer_plan is None
        return
    started, release = threading.Event(), threading.Event()
    if outcome == "cancelled":
        from creation_lib.mod_manager.save_transfer import apply
        def blocked(library, plan, cancel, progress):
            started.set()
            assert release.wait(10)
            return apply(library, plan, cancel, progress)
        monkeypatch.setattr("creation_lib.mod_manager.save_transfer.apply", blocked)
    elif outcome == "failed":
        path.with_suffix(".f4se").write_bytes(b"changed after review")
    try:
        native.click(native.find("Create these reviewed owned copies"), draw)
        native.click(native.find("Copy reviewed save pair"), draw)
        if outcome == "cancelled":
            assert started.wait(5)
            native.click(native.find("Cancel operation", window="manager_workflow"), draw)
            release.set()
        if outcome in {"cancelled", "failed"}:
            manager._future.exception(timeout=10)
            manager.poll()
            assert manager.job_state == outcome
            assert not manager.library.root.joinpath(".save-history").exists()
        else:
            finish_job(workspace, native, draw)
            if outcome == "backup":
                backups = list(manager.library.root.joinpath(".save-history", manager.profile_id).iterdir())
                assert len(backups) == 1 and backups[0].joinpath("Fixture.f4se").read_bytes() == b"generated companion"
            else:
                assert manager.library.root.joinpath("runtime", "profiles", other, "saves", "Fixture.fos").read_bytes() == original
        assert path.read_bytes() == original
    finally:
        release.set()


@pytest.fixture
def workspace(tmp_path, monkeypatch):
    monkeypatch.setattr("app.paths.get_db_dir", lambda: tmp_path / "app data")
    monkeypatch.setattr("ui.toolkit.app_paths.get_ini_dir", lambda: tmp_path / "app ini")
    workspace = ModManagerWorkspace()
    workspace._show_filters = True
    workspace._record_field_rects = True
    workspace._right_pane = "Information"
    manager = workspace.manager
    manager.library = Library(tmp_path / "library", create=True)
    manager.import_preview = inspect_instance(make_mo2(tmp_path))
    manager.library.import_mo2(manager.import_preview)
    manager.refresh()
    with manager.library._db() as db:
        for index, name in enumerate(("Same name", "Same name", "##enabled")):
            identifier = f"duplicate-{index}"
            provider = tmp_path / identifier
            provider.mkdir()
            db.execute("INSERT INTO mods(id,name,source,kind) VALUES (?,?,?,'external')", (identifier, name, str(provider)))
            db.execute("INSERT INTO entries VALUES (?,?,?,1,?)", (manager.profile_id, identifier, index + 4, "Visual/Textures"))
    manager.refresh()
    yield workspace
    workspace.cleanup()


def test_mcp_disable_checkbox_persists_actual_input(native, workspace):
    draw = panels(workspace)
    native.warm(draw)
    assert not workspace.launcher.disable_mcp
    native.click(native.find("Launch options"), draw)
    native.click(native.find("Keep MCP disabled"), draw)
    assert workspace.launcher.disable_mcp
    assert workspace.collect_settings()["disable_mcp"] is True


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_archive_member_preview_uses_selected_provider_actual_input(native, workspace, tmp_path, size):
    from creation_lib.ba2.native_runtime import pack_archive
    from creation_lib.mod_manager.archive_index import ArchiveIndex, Member
    from .test_archive_preview import provider
    native.io.display_size = size
    source = tmp_path / "archive-preview"
    source.mkdir()
    (source / "readme.txt").write_text("First provider")
    first = tmp_path / "first.ba2"
    pack_archive(str(source), str(first), "fo4", compress=True)
    (source / "readme.txt").write_text("Second provider")
    second = tmp_path / "second.ba2"
    pack_archive(str(source), str(second), "fo4", compress=True)
    archives = (provider(first), provider(second))
    m = workspace.manager
    m.archive_index = ArchiveIndex(m.profile_id, archives, (Member("readme.txt", archives),), (), ())
    workspace._right_pane = "Archives"
    draw = panels(workspace)
    native.warm(draw)
    native.click(native.find("readme.txt | 2 providers###archive_member"), draw)
    native.click(native.find("Preview member"), draw)
    finish_job(workspace, native, draw)
    assert workspace.panes.archive_preview.result.text == "First provider"
    native.choose_next("Preview provider", draw)
    assert workspace.panes.archive_preview.result is None
    native.click(native.find("Preview member"), draw)
    finish_job(workspace, native, draw)
    assert workspace.panes.archive_preview.result.text == "Second provider"


def panels(workspace, responsive=False):
    dock_id = None
    def draw():
        nonlocal dock_id
        workspace.draw()
        width, height = imgui.get_io().display_size
        imgui.set_next_window_pos((0, 0))
        imgui.set_next_window_size((width, 32))
        imgui.begin("Manager menu", flags=imgui.WindowFlags_.menu_bar | imgui.WindowFlags_.no_title_bar | imgui.WindowFlags_.no_saved_settings)
        if imgui.begin_menu_bar():
            workspace.draw_menu()
            imgui.end_menu_bar()
        imgui.end()
        imgui.set_next_window_pos((0, 35))
        imgui.set_next_window_size((width, height - 35))
        imgui.push_style_var(imgui.StyleVar_.window_padding, (0, 0))
        imgui.begin("Manager panels", flags=imgui.WindowFlags_.no_title_bar | imgui.WindowFlags_.no_saved_settings | imgui.WindowFlags_.no_docking)
        if dock_id is None:
            dock_id = imgui.get_id("manager_test_dock")
            imgui.internal.dock_builder_add_node(dock_id)
            imgui.internal.dock_builder_set_node_size(dock_id, (width, height - 35))
            for label in ("Library", "Settings"):
                imgui.internal.dock_builder_dock_window(label + "##mod_manager", dock_id)
            imgui.internal.dock_builder_finish(dock_id)
        imgui.dock_space(dock_id)
        imgui.end()
        imgui.pop_style_var()
        workspace._draw_main()
        workspace._draw_settings()
    return draw


def chord(native, draw, key):
    native.io.add_key_event(imgui.Key.mod_ctrl, True)
    native.key(key, draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, False)
    native.frame(draw)


def trust_fixture_bundle(workspace, monkeypatch):
    # Fixture PE bytes are inert. Only the read-only version metadata seam is mocked.
    from creation_lib.mod_manager import pe_metadata
    original_version = pe_metadata.file_version
    monkeypatch.setattr(pe_metadata, "file_version", lambda path: (0, 5, 7, 2) if Path(path).name.startswith("usvfs_") else original_version(path))
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: Path(workspace.launcher.bundle_root))
    workspace.launcher.bundle_trusted = True
    workspace.launcher._bundle_trust_path = workspace.launcher.bundle_root


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_toolbar_destinations_and_all_commands_fit_actual_input(native, workspace, size):
    native.io.display_size = size
    draw = panels(workspace)
    native.warm(draw)
    commands = [native.find(label) for label in ("Filters", "Install", "Add mod", "Refresh files", "Undo", "Actions")]
    assert all(0 <= item.rect[0] < item.rect[2] <= size[0] and 0 <= item.rect[1] < item.rect[3] <= size[1] for item in commands)
    assert native.find("Settings").rect[3] < min(item.rect[1] for item in commands)
    assert max(item.rect[1] for item in commands) - min(item.rect[1] for item in commands) < 10
    native.click(native.find("Install"), draw)
    assert workspace._dialog == "Install"
    native.find("Manual layout (skip FOMOD rules)")
    native.click(native.find("Back to Library"), draw)
    native.click(native.find("Add mod"), draw)
    native.click(native.find("Import MO2 instance..."), draw)
    assert workspace._dialog == "Import MO2"
    native.click(native.find("Back to Library"), draw)
    native.click(native.find("Actions"), draw)
    native.click(native.find("Executables", window="##Popup"), draw)
    assert workspace._dialog == "Executables"
    native.click(native.find("Add"), draw)
    native.find("##exe_title")
    native.click(native.find("Cancel"), draw)
    native.click(native.find("Actions"), draw)
    native.click(native.find("Profile backups"), draw)
    native.find("Back up current profile")
    native.click(native.find("Back to Library"), draw)
    native.click(native.find("Actions"), draw)
    native.click(native.find("Manage profiles"), draw)
    native.find("New profile")
    native.key(imgui.Key.escape, draw)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_navigation_shortcuts_focus_picker_and_escape_actual_input(native, workspace, tmp_path, monkeypatch, size):
    native.io.display_size = size
    draw = panels(workspace)
    picked = []
    package = make_package(tmp_path)
    monkeypatch.setattr("ui.mod_manager.workspace.pick_file", lambda *args: picked.append(args) or str(package))
    native.warm(draw)
    native.click(native.find("Library"), draw)
    chord(native, draw, imgui.Key.s)
    assert workspace._page == "Settings"
    native.click(native.find("Library"), draw)
    chord(native, draw, imgui.Key.p)
    native.find("New profile")
    native.key(imgui.Key.escape, draw)
    native.click(native.find("Library"), draw)
    chord(native, draw, imgui.Key.e)
    assert workspace._dialog == "Executables"
    native.click(native.find("Add"), draw)
    native.click(native.find("##exe_title"), draw)
    native.io.add_input_characters_utf8("Draft")
    native.frame(draw)
    native.key(imgui.Key.escape, draw)
    assert workspace._dialog == "Executables"  # The field handles its first Escape.
    native.key(imgui.Key.escape, draw)
    assert not workspace._dialog
    native.click(native.find("Filters"), draw)
    assert not workspace._show_filters
    chord(native, draw, imgui.Key.f)
    native.io.add_input_characters_utf8("Override")
    native.frame(draw)
    assert workspace._query == "Override"
    # Other command shortcuts leave active text fields alone.
    chord(native, draw, imgui.Key.s)
    assert workspace._page == "Library"
    native.key(imgui.Key.escape, draw)
    native.click(native.find("Library"), draw)
    chord(native, draw, imgui.Key.m)
    finish_job(workspace, native, draw)
    assert len(picked) == 1 and workspace._dialog == "Install" and workspace.manager.prepared
    native.key(imgui.Key.escape, draw)
    assert not workspace._dialog and workspace.manager.prepared


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_file_refresh_shortcut_toolbar_busy_and_escape_cancel_actual_input(native, workspace, monkeypatch, size):
    from creation_lib.mod_manager.paths import check_cancel
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(native.find("Library"), draw)
    native.key(imgui.Key.f5, draw)
    from .test_local_status import wait_scan
    wait_scan(m)
    native.warm(draw)
    assert m.file_index and not m.busy and m.conflict_scan_state in {"ready", "partial"}
    entered = threading.Event()
    def wait_for_cancel(library, profile, cancel, progress, **kwargs):
        entered.set()
        cancel.wait(5)
        check_cancel(cancel)
        raise AssertionError("The reviewed UI operation should have been cancelled")
    monkeypatch.setattr("creation_lib.mod_manager.conflicts.build_index", wait_for_cancel)
    m.inspect_files()  # Explicit file-inspection review still has main-worker cancellation.
    assert entered.wait(2) and m.busy
    native.click(native.find("Install"), draw)
    assert not workspace._dialog
    chord(native, draw, imgui.Key.e)
    assert not workspace._dialog and m.busy
    native.click(native.find("Cancel operation"), draw)
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == "cancelled" and not m.file_index
    workspace._tab = "Install"
    native.warm(draw)
    m.inspect_files()
    assert entered.wait(2) and m.busy
    native.click(native.find("Back to Library"), draw)  # Retains the running worker.
    workspace._tab = "Install"
    native.warm(draw)
    native.click(native.find("Install", window="Library"), draw)  # Disabled; focus stays in library.
    # Escape works from the manager or workflow even if a background click raised Library.
    native.key(imgui.Key.escape, draw)
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert not workspace._dialog and m.job_state == "cancelled"


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_tree_priority_undo_prefix_cycle_and_text_guard_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    selected = next(item for item in native.items if item.label == "##select_mod")
    native.click(selected, draw)
    mod = m.selected_id
    before = [(row.id, row.priority) for row in m.rows]
    key = imgui.Key.up_arrow if m.rows_by_id[mod].priority else imgui.Key.down_arrow
    chord(native, draw, key)
    finish_job(workspace, native, draw)
    assert m.selected_id == mod and [(row.id, row.priority) for row in m.rows] != before
    native.click(native.find("##select_mod", id=selected.id), draw)
    chord(native, draw, imgui.Key.z)
    finish_job(workspace, native, draw)
    assert [(row.id, row.priority) for row in m.rows] == before
    native.click(native.find("##select_mod", id=selected.id), draw)
    native.key(imgui.Key.s, draw)
    first = m.selected_id
    assert m.rows_by_id[first].name == "Same name"
    native.key(imgui.Key.s, draw)
    assert m.selected_id != first and m.rows_by_id[m.selected_id].name == "Same name"
    native.text(native.find("##search_mods"), draw, "Same")
    before = [(row.id, row.priority) for row in m.rows]
    chord(native, draw, imgui.Key.down_arrow)
    assert not m.busy and [(row.id, row.priority) for row in m.rows] == before


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_folder_commands_and_double_click_information_use_actual_input(native, workspace, monkeypatch, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    calls = []
    popen = subprocess.Popen
    def launch(argv, **kw):
        if argv[0] == "explorer.exe":
            return calls.append((argv, kw))
        return popen(argv, **kw)
    monkeypatch.setattr(subprocess, "Popen", launch)
    native.warm(draw)
    selected = next(item for item in native.items if item.label == "##select_mod")
    native.click(selected, draw)
    source = m.rows_by_id[m.selected_id].source
    chord(native, draw, imgui.Key.enter)
    assert calls == [(["explorer.exe", source], {"shell": False})]
    native.click(native.find("Actions"), draw)
    native.click(native.find("Open selected mod folder"), draw)
    assert calls[-1] == (["explorer.exe", source], {"shell": False}) and len(calls) == 2
    native.click(native.find("Actions"), draw)
    native.click(native.find("Open library folder"), draw)
    assert calls[-1] == (["explorer.exe", str(m.library.root)], {"shell": False})
    def double_click(ctrl=False):
        native.warm(draw)
        item = native.find("##select_mod", id=selected.id)
        x1, y1, x2, y2 = item.rect
        native.io.add_key_event(imgui.Key.mod_ctrl, ctrl)
        native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2), down=True)
        native.frame(draw, down=False)
        native.frame(draw, down=True)
        native.frame(draw, down=False)
        native.io.add_key_event(imgui.Key.mod_ctrl, False)
        native.frame(draw)
    workspace._right_pane = "Plugins"
    native.warm(draw)
    if m.busy:
        finish_job(workspace, native, draw)
    double_click()
    assert workspace._right_pane == "Information"
    double_click(ctrl=True)
    assert calls[-1] == (["explorer.exe", source], {"shell": False}) and len(calls) == 4


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_plugin_table_scrolls_to_distant_selection(native, workspace, size):
    from creation_lib.mod_manager.plugin_state import Catalog, Plugin
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    plugins = tuple(Plugin(f"Fixture{i:03d}.esp", "fixture.esp", "Fixture", "", False, i,
                           header={"flags": 0}, masters=()) for i in range(200))
    m.plugin_catalog = Catalog(m.profile_id, "fixture", "fo4", plugins)
    workspace._right_pane = "Plugins"
    native.warm(draw)
    workspace.panes._scroll_plugin = plugins[180].name
    native.warm(draw)
    rows = [item for item in native.items if item.label.startswith("##plugin_priority_")]
    assert 0 < len(rows) < 30
    assert any(item.label == "##plugin_priority_fixture180.esp" for item in rows)
    workspace.panes._scroll_plugin = plugins[0].name
    native.warm(draw)
    assert native.find("##plugin_priority_fixture000.esp")


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_top_level_settings_and_plugin_column_drag(native, workspace, size):
    from creation_lib.mod_manager.plugin_state import Catalog, Plugin
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    workspace._right_pane = "Plugins"
    m.plugin_catalog = Catalog(m.profile_id, "fixture", "fo4", (
        Plugin("Fixture.esp", "fixture.esp", "Fixture", "", False, 0, header={"flags": 0}, masters=()),))
    native.warm(draw)
    assert [window.label for window in workspace.get_dockable_windows()] == ["Library##mod_manager", "Settings##mod_manager"]
    assert native.find("Settings").window == "Manager tabs"
    source, target = native.find("Type"), native.find("Plugin")
    native.frame(draw, mouse=((source.rect[0] + source.rect[2]) / 2, (source.rect[1] + source.rect[3]) / 2))
    drag_plugin(native, draw, source, target)
    assert native.find("Type").rect[0] < native.find("Plugin").rect[0]
    native.click(native.find("Settings"), draw)
    assert native.find("Row height")
    assert not any(item.label == "Install" for item in native.items)
    native.click(native.find("Library"), draw)
    assert native.find("Type").rect[0] < native.find("Plugin").rect[0]
    settings = imgui.save_ini_settings_to_memory()
    assert "[Table]" in settings and "Order=2" in settings


def open_recipe_form(native, workspace, draw):
    native.click(native.find("Actions"), draw)
    native.click(native.find("Profile recipes"), draw)
    native.click(native.find("Export current profile recipe"), draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##recipe_path"), draw, workspace.manager.recipe_export)
    native.click(native.find("Read recipe"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Review profile transfer"), draw)
    finish_job(workspace, native, draw)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_profile_recipe_review_cancel_import_and_repeat_preserve_local_edits_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    original = m.profile_id
    before = m.library.profile_snapshot(original)
    native.warm(draw)
    open_recipe_form(native, workspace, draw)
    for label in ("Review profile transfer", "Import reviewed profile", "Cancel transfer"):
        item = native.find(label)
        assert 0 <= item.rect[1] < item.rect[3] <= size[1]
    native.click(native.find("Import reviewed profile"), draw)
    assert not m.busy and len(m.profiles) == 2  # Explicit acknowledgement is required.
    native.click(native.find("Cancel transfer"), draw)
    assert m.recipe_plan is None and m.library.profile_snapshot(original) == before
    native.click(native.find("Review profile transfer"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Keep original profiles and import this reviewed mapping"), draw)
    native.click(native.find("Import reviewed profile"), draw)
    finish_job(workspace, native, draw)
    assert len(m.profiles) == 3 and m.profile_id != original
    transferred = m.profile_id
    assert m.library.profile_snapshot(original) == before
    native.click(native.find("Back to Library"), draw)
    native.click(next(item for item in native.items if item.label == "##enabled"), draw)
    finish_job(workspace, native, draw)
    edited = m.library.profile_snapshot(transferred)
    native.click(native.find("Actions"), draw)
    native.click(native.find("Profile recipes"), draw)
    native.click(native.find("Read recipe"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Review profile transfer"), draw)
    finish_job(workspace, native, draw)
    assert m.recipe_plan.existing
    native.click(native.find("Keep original profiles and import this reviewed mapping"), draw)
    native.click(native.find("Use existing transferred profile"), draw)
    finish_job(workspace, native, draw)
    assert len(m.profiles) == 3 and m.library.profile_snapshot(transferred) == edited


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_profile_recipe_import_cancel_failure_rolls_back_actual_input(native, workspace, monkeypatch, size, outcome):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    before = m.profiles
    native.warm(draw)
    open_recipe_form(native, workspace, draw)
    started, release = threading.Event(), threading.Event()
    def stop(stage):
        if stage == "import-inserted":
            if outcome == "failed":
                raise OSError("disposable profile recipe failure")
            started.set()
            assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.profile_recipes._checkpoint", stop)
    native.click(native.find("Keep original profiles and import this reviewed mapping"), draw)
    native.click(native.find("Import reviewed profile"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == outcome and m.library.profiles() == before
    assert Library(m.library.root).profiles() == before


def test_recipe_mapping_and_configuration_changes_require_new_review_actual_input(native, workspace):
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    open_recipe_form(native, workspace, draw)
    native.click(native.find("Keep original profiles and import this reviewed mapping"), draw)
    native.text(native.find("New profile name"), draw, "A new requested name")
    native.click(native.find("Import reviewed profile"), draw)
    assert not m.busy and len(m.profiles) == 2
    native.click(native.find("Configuration preview"), draw)
    preview = native.find("##recipe_config_preview")
    native.text(preview, draw, "Never replace read-only recipe text")
    assert workspace.transfers.seen is m.recipe and dict(m.recipe.files)[workspace.transfers.config_name] != "Never replace read-only recipe text"


def test_large_recipe_mapping_inventory_is_not_rebuilt_every_frame(native, workspace):
    from dataclasses import replace
    from creation_lib.mod_manager.profile_recipes import Entry, read, export
    m, draw = workspace.manager, panels(workspace)
    recipe = read(export(m.library, m.profile_id))
    class Metered(tuple):
        calls = 0
        def __iter__(self):
            self.calls += 1
            return super().__iter__()
    entries = Metered(Entry(f"missing-{i}", f"Fixture {i}", "external", True, "") for i in range(50_000))
    m.recipe = replace(recipe, entries=entries)
    m.recipe_suggestions = {}
    workspace._tab = "Profile transfers"
    native.warm(draw)
    calls = entries.calls
    native.warm(draw)
    assert entries.calls == calls and workspace.transfers._missing == 50_000
    assert sum(item.label == "##recipe_mapping" for item in native.items) < 20


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_cross_library_recipe_requires_real_mapping_choices_actual_input(native, workspace, tmp_path, size):
    from creation_lib.mod_manager.profile_recipes import export
    native.io.display_size = size
    directory = tmp_path / "separate recipe origin"
    directory.mkdir()
    library = Library(directory / "owned library", create=True)
    library.import_mo2(inspect_instance(make_mo2(directory)))
    original = library.profiles()[0].id
    source = library.profile_snapshot(original)
    path = export(library, original)
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(native.find("Actions"), draw)
    native.click(native.find("Profile recipes"), draw)
    native.text(native.find("##recipe_path"), draw, str(path))
    native.click(native.find("Read recipe"), draw)
    finish_job(workspace, native, draw)
    assert not workspace.transfers.mapping and workspace.transfers._missing == 3
    native.click(native.find("Review profile transfer"), draw)
    assert not m.busy and not m.recipe_plan
    native.scroll(native.find("Map recipe mods"), draw, -8)
    for index, entry in enumerate(workspace.transfers.payloads):
        choices = [item for item in native.items if item.label == "##recipe_mapping"]
        assert len(choices) == 3
        native.click(choices[index], draw)
        available = [item for item in native.items if item.label == "##map_local_mod"]
        candidate = next(i for i, row in enumerate(workspace.transfers._candidates) if row.name == entry.name)
        native.click(available[candidate], draw)
    assert workspace.transfers._missing == 0
    native.click(native.find("Review profile transfer"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Keep original profiles and import this reviewed mapping"), draw)
    native.click(native.find("Import reviewed profile"), draw)
    finish_job(workspace, native, draw)
    assert len(m.profiles) == 3 and library.profile_snapshot(original) == source


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_nexus_keyboard_and_shift_double_click_open_known_page_actual_input(native, workspace, monkeypatch, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    opened = []
    monkeypatch.setattr("creation_lib.mod_manager.nexus_links.open_browser", lambda url, **kwargs: opened.append(url) or True)
    with m.library._db() as db:
        db.execute("UPDATE mods SET nexus_mod_id=123 WHERE kind='external'")
    m.refresh()
    native.warm(draw)
    native.click(native.find("Library"), draw)
    chord(native, draw, imgui.Key.n)
    assert opened == ["https://www.nexusmods.com/fallout4"]
    native.click(native.find("Actions"), draw)
    native.click(native.find("Game on Nexus"), draw)
    assert opened[-1] == "https://www.nexusmods.com/fallout4" and len(opened) == 2
    item = next(item for item in native.items if item.label == "##select_mod")
    x1, y1, x2, y2 = item.rect
    native.io.add_key_event(imgui.Key.mod_shift, True)
    native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2), down=True)
    native.frame(draw, down=False)
    native.frame(draw, down=True)
    native.frame(draw, down=False)
    native.io.add_key_event(imgui.Key.mod_shift, False)
    native.frame(draw)
    assert opened[-1] == "https://www.nexusmods.com/fallout4/mods/123" and len(opened) == 3


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_alt_group_descendants_shift_collapse_and_information_page_navigation_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    groups = [item for item in native.items if item.label == "##group_node"]
    assert groups
    native.io.add_key_event(imgui.Key.mod_alt, True)
    native.click(groups[0], draw)
    native.io.add_key_event(imgui.Key.mod_alt, False)
    native.frame(draw)
    assert workspace._selected_mods == {row.id for row in m.rows if row.kind != "separator" and row.group.startswith("Visual")}
    assert m.selected_id in workspace._selected_mods
    native.io.add_key_event(imgui.Key.mod_shift, True)
    native.key(imgui.Key.enter, draw)
    native.io.add_key_event(imgui.Key.mod_shift, False)
    native.frame(draw)
    assert "group:Visual" in workspace._closed_groups[m.profile_id], (m.rows_by_id[m.selected_id].group, native.context.nav_window.name if native.context.nav_window else None, native.io.want_text_input)
    native.click(next(item for item in native.items if item.label == "##group_node"), draw)
    native.key(imgui.Key.right_arrow, draw)
    native.click(next(item for item in native.items if item.label == "##select_mod"), draw)
    # Information navigation uses the filtered display order, independent of collapsed groups.
    workspace._right_pane = "Information"
    native.warm(draw)
    native.click(native.find("Mod info"), draw)
    first = m.selected_id
    native.key(imgui.Key.page_down, draw)
    assert m.selected_id != first
    native.key(imgui.Key.page_up, draw)
    assert m.selected_id == first
    native.click(native.find("##notes"), draw)
    native.io.add_input_characters_utf8("Draft notes")
    native.frame(draw)
    native.key(imgui.Key.page_down, draw)
    assert m.selected_id == first


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_f2_display_title_cancel_save_repeat_and_external_delete_guard_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    item = next(item for item in native.items if item.label == "##select_mod")
    native.click(item, draw)
    mod = m.selected_id
    row = m.rows_by_id[mod]
    original = m.library.profile_snapshot(m.profile_id)
    native.key(imgui.Key.f2, draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##mod_display_title"), draw, "Cancelled display alias")
    native.click(native.find("Cancel title edit"), draw)
    assert m.rows_by_id[mod] == row and not workspace._dialog
    native.click(native.find("##select_mod", id=item.id), draw)
    native.key(imgui.Key.f2, draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##mod_display_title"), draw, "My ### display alias")
    native.click(native.find("Save display title"), draw)
    finish_job(workspace, native, draw)
    assert not workspace._dialog and m.rows_by_id[mod].name == "My ### display alias" and m.rows_by_id[mod].source == row.source
    m.library.import_mo2(m.import_preview)
    m.refresh()
    native.warm(draw)
    assert m.rows_by_id[mod].name == "My ### display alias"
    assert m.library.profile_snapshot(m.profile_id)[0] == original[0]
    native.click(native.find("##select_mod", id=item.id), draw)
    native.key(imgui.Key.delete, draw)
    assert not m.busy and not workspace._dialog and "owned mods only" in m.error
    assert m.rows_by_id[mod].source == row.source


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_delete_opens_frozen_owned_review_cancel_and_recoverable_removal_actual_input(native, workspace, tmp_path, size):
    from .test_transactions import setup_owned
    native.io.display_size = size
    library, profile, mod, package = setup_owned(tmp_path / "owned delete fixture")
    m, draw = workspace.manager, panels(workspace)
    m.library, m.profile_id = library, profile
    m.refresh()
    workspace._show_filters = False
    native.warm(draw)
    native.click(next(item for item in native.items if item.label == "##select_mod"), draw)
    assert m.selected_id == mod
    native.key(imgui.Key.delete, draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Remove reviewed owned mod"), draw)
    assert not m.busy and mod in m.rows_by_id
    native.click(native.find("Cancel mod removal"), draw)
    assert not workspace._dialog and (library.root / "mods" / mod).is_dir()
    native.click(next(item for item in native.items if item.label == "##select_mod"), draw)
    native.key(imgui.Key.delete, draw)
    finish_job(workspace, native, draw)
    remove = native.find("Remove reviewed owned mod")
    assert 0 <= remove.rect[1] < remove.rect[3] <= size[1]
    native.click(native.find("Keep complete recovery content and remove this owned mod"), draw)
    native.click(native.find("Remove reviewed owned mod"), draw)
    finish_job(workspace, native, draw)
    assert not workspace._dialog and mod not in m.rows_by_id and not (library.root / "mods" / mod).exists()
    history = library.history()
    assert len(history) == 1 and history[0]["kind"] == "uninstall"
    m.review_restore(history[0]["id"])
    finish_job(workspace, native, draw)
    m.restore()
    finish_job(workspace, native, draw)
    assert mod in m.rows_by_id and (library.root / "mods" / mod).is_dir()


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_display_title_cancel_failure_after_write_preserves_alias_actual_input(native, workspace, monkeypatch, size, outcome):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(next(item for item in native.items if item.label == "##select_mod"), draw)
    row = m.rows_by_id[m.selected_id]
    native.key(imgui.Key.f2, draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##mod_display_title"), draw, "Never committed title")
    started, release = threading.Event(), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("disposable display title failure")
        started.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.mod_names._checkpoint", stop)
    native.click(native.find("Save display title"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == outcome and m.rows_by_id[row.id] == row
    assert next(item for item in Library(m.library.root).rows(m.profile_id) if item.id == row.id) == row


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_all_right_pane_controls_fit_and_overwrite_copy_clear_restore_use_actual_input(native, workspace, size):
    from creation_lib.mod_manager.overwrite import root
    native.io.display_size = size
    manager, draw = workspace.manager, panels(workspace)
    folder = root(manager.library, manager.profile_id)
    (folder / "generated" / "empty").mkdir(parents=True)
    (folder / "generated" / "fixture.txt").write_bytes(b"generated output")
    native.warm(draw)
    tabs = [native.find(name) for name in ("Plugins", "Archives", "Data", "Saves", "Downloads", "Mod info")]
    assert all(0 <= item.rect[0] < item.rect[2] <= size[0] and 0 <= item.rect[1] < item.rect[3] <= size[1] for item in tabs)
    menu_action(native, draw, "Manage owned overwrite...")
    native.click(native.find("Review owned output"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Copy output to owned mod"), draw)
    finish_job(workspace, native, draw)
    row = manager.rows_by_id[manager.selected_id]
    assert row.kind == "owned" and (Path(row.source) / "generated/fixture.txt").read_bytes() == b"generated output"
    assert (folder / "generated/fixture.txt").read_bytes() == b"generated output"
    native.click(native.find("Review owned output"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Clear output with backup"), draw)
    assert not manager.busy  # Disabled until the explicit reviewed checkbox.
    native.click(native.find("Keep a complete backup and clear this owned output"), draw)
    native.click(native.find("Clear output with backup"), draw)
    finish_job(workspace, native, draw)
    assert not list(folder.iterdir())
    native.click(native.find("Review owned output"), draw)
    finish_job(workspace, native, draw)
    assert len(manager.overwrite_history) == 1
    native.click(native.find("Review output backup restore"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Cancel output restore"), draw)
    assert manager.overwrite_restore is None and not list(folder.iterdir())
    native.click(native.find("Review output backup restore"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Restore this backup into empty owned output"), draw)
    native.click(native.find("Restore reviewed output backup"), draw)
    finish_job(workspace, native, draw)
    assert (folder / "generated/fixture.txt").read_bytes() == b"generated output"
    assert (folder / "generated/empty").is_dir()


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_overwrite_cancel_and_failure_preserve_output_and_recover_journal(native, workspace, monkeypatch, outcome):
    from creation_lib.mod_manager.overwrite import root
    manager, draw = workspace.manager, panels(workspace)
    folder = root(manager.library, manager.profile_id)
    folder.mkdir(parents=True)
    (folder / "fixture.txt").write_bytes(b"owned bytes must survive")
    native.warm(draw)
    menu_action(native, draw, "Manage owned overwrite...")
    native.click(native.find("Review owned output"), draw)
    finish_job(workspace, native, draw)
    started, release = threading.Event(), threading.Event()
    def stop(stage):
        if stage == "journal":
            if outcome == "failed":
                raise OSError("disposable output failure")
            started.set()
            assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.overwrite._checkpoint", stop)
    native.click(native.find("Keep a complete backup and clear this owned output"), draw)
    native.click(native.find("Clear output with backup"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    manager._future.exception(timeout=10)
    manager.poll()
    native.warm(draw)
    assert manager.job_state == outcome and (folder / "fixture.txt").read_bytes() == b"owned bytes must survive"
    reopened = Library(manager.library.root)
    assert not list((reopened.root / ".overwrite-transactions").glob("*.json"))


def finish_job(workspace, native, draw):
    manager = workspace.manager
    if manager._future:
        manager._future.result(timeout=10)
        manager.poll()
    assert not manager.error, manager.error
    native.warm(draw)


def visible_rows(native, manager, workspace):
    nodes = workspace._visible_tree
    included = {row.id for row in manager.filtered(workspace._query, workspace._group, workspace._enabled_only,
                                                  sort_by=workspace._sort_by, descending=workspace._descending, origin=workspace._origin, category=workspace._category_filter, statuses=workspace._status_filters)}
    items = [item for item in native.items if item.label in {"##select_mod", "##group_node"}]
    assert len(items) == len(nodes)
    return {node.row.id: item for node, item in zip(nodes, items) if node.row and node.row.id in included}


def menu_action(native, draw, label):
    native.click(native.find("Mods"), draw)
    native.click(native.find(label), draw)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900), (1920, 1080)])
def test_standalone_first_run_detect_create_install_and_reopen_without_mo2(native, workspace, tmp_path, monkeypatch, size):
    from .test_standalone import game_fixture
    native.io.display_size = size
    manager = workspace.manager
    manager.library, manager.import_preview, manager.instance_migration = None, None, None
    manager.refresh()
    workspace._library_path = str(tmp_path / "standalone library")
    workspace._show_filters = False
    game = game_fixture(tmp_path)
    original = {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()}
    monkeypatch.setattr("creation_lib.core.path_detector.detect_game_path", lambda game_id: str(game))
    # MO2 is deliberately unavailable for the entire standalone path.
    monkeypatch.setattr("creation_lib.mod_manager.mo2.inspect_instance", lambda *args, **kwargs: pytest.fail("Standalone flow attempted MO2 inspection"))
    draw = panels(workspace)
    native.warm(draw)
    native.click(native.find("Detect game"), draw)
    finish_job(workspace, native, draw)
    assert workspace._setup_root == str(game)
    native.click(native.find("Create instance"), draw)
    finish_job(workspace, native, draw)
    assert len(manager.profiles) == 1 and len(manager.executables) == 2 and manager.import_preview is None
    profile_id = manager.profile_id
    package = tmp_path / "plain package"
    package.mkdir()
    (package / "generated.txt").write_bytes(b"generated payload")
    monkeypatch.setattr("ui.mod_manager.workspace.pick_file", lambda *args: str(package))
    menu_action(native, draw, "Install package...")
    native.click(native.find("Open archive"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Install into library"), draw)
    finish_job(workspace, native, draw)
    assert manager.rows[0].kind == "owned" and workspace._dialog == ""
    reopened = Library(manager.library.root)
    assert reopened.profiles()[0].id == profile_id and len(reopened.rows(profile_id)) == 1
    assert {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()} == original


def test_manual_layout_retry_after_invalid_fomod_uses_actual_controls(native, workspace, tmp_path, monkeypatch):
    package = tmp_path / "manual package"
    (package / "wrapper" / "Data").mkdir(parents=True)
    (package / "wrapper" / "fomod").mkdir()
    (package / "wrapper" / "Data" / "fixture.txt").write_bytes(b"manual fixture")
    (package / "wrapper" / "fomod" / "ModuleConfig.xml").write_text("<invalid")
    (package / "wrapper" / "fomod" / "script.py").write_text("raise RuntimeError('Never execute')")
    original = {str(path): path.read_bytes() for path in package.rglob("*") if path.is_file()}
    manager, draw = workspace.manager, panels(workspace)
    monkeypatch.setattr("ui.mod_manager.workspace.pick_file", lambda *args: str(package))
    native.warm(draw)
    menu_action(native, draw, "Install package...")
    native.click(native.find("Open archive"), draw)
    if manager._future is not None:
        assert manager._future.exception(timeout=10)
    manager.poll()
    native.warm(draw)
    assert manager.job_state == "failed" and manager.prepared is None
    native.click(native.find("Manual layout (skip FOMOD rules)"), draw)
    native.click(native.find("Open archive"), draw)
    finish_job(workspace, native, draw)
    assert manager.prepared.manual
    native.text(native.find("Content root"), draw, "wrapper/Data")
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    assert manager.install_plan == {"fixture.txt": "wrapper/Data/fixture.txt"}
    native.click(native.find("Install into library"), draw)
    finish_job(workspace, native, draw)
    row = next(row for row in manager.rows if row.kind == "owned")
    assert (Path(row.source) / "fixture.txt").read_bytes() == b"manual fixture"
    assert {str(path): path.read_bytes() for path in package.rglob("*") if path.is_file()} == original


def test_tree_multiselect_drag_priority_and_keyboard_undo_use_actual_input(native, workspace):
    manager, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.text(native.find("##search_mods"), draw, "Same name")
    native.click(visible_rows(native, manager, workspace)["duplicate-0"], draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, True)
    native.click(visible_rows(native, manager, workspace)["duplicate-1"], draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, False)
    native.frame(draw)
    assert workspace._selected_mods == {"duplicate-0", "duplicate-1"}
    native.io.add_key_event(imgui.Key.mod_shift, True)
    native.click(visible_rows(native, manager, workspace)["duplicate-1"], draw)
    native.io.add_key_event(imgui.Key.mod_shift, False)
    native.frame(draw)
    assert workspace._selected_mods == {"duplicate-0", "duplicate-1"}
    # A single selected mod is dragged using actual held mouse movement/release.
    native.click(visible_rows(native, manager, workspace)["duplicate-0"], draw)
    before = manager.library.rows(manager.profile_id)
    source = visible_rows(native, manager, workspace)["duplicate-0"]
    target = visible_rows(native, manager, workspace)["duplicate-1"]
    def center(item):
        x1, y1, x2, y2 = item.rect
        return ((x1 + x2) / 2, (y1 + y2) / 2)
    # Explicit lower-half drop means "after"; the exact midpoint is a rounding boundary.
    after_target = (center(target)[0], target.rect[3] - 2)
    native.frame(draw, mouse=center(source), down=True)
    native.frame(draw, mouse=after_target, down=True)
    native.frame(draw, mouse=after_target, down=True)
    native.frame(draw, mouse=after_target, down=False)
    finish_job(workspace, native, draw)
    assert manager.library.rows(manager.profile_id) != before
    native.click(visible_rows(native, manager, workspace)["duplicate-0"], draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, True)
    native.key(imgui.Key.z, draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, False)
    finish_job(workspace, native, draw)
    assert manager.library.rows(manager.profile_id) == before


def test_profile_backup_review_cancel_restore_and_ini_recovery_use_actual_input(native, workspace, tmp_path):
    from .test_standalone import game_fixture
    from creation_lib.mod_manager.instances import create
    manager, draw = workspace.manager, panels(workspace)
    manager.library = Library(tmp_path / "standalone library", create=True)
    manager.profile_id = create(manager.library, "fo4", game_fixture(tmp_path))
    manager.refresh()
    native.warm(draw)
    menu_action(native, draw, "Profile configuration")
    workspace._profile_filename = "Fallout4Custom.ini"
    native.warm(draw)
    initial = manager.profiles[0].files[workspace._profile_filename]
    native.text(native.find("##profile_ini_editor"), draw, "[Display]\nFixture=1\n")
    native.click(native.find("Save local INI"), draw)
    finish_job(workspace, native, draw)
    assert "Fixture=1" in manager.profiles[0].files[workspace._profile_filename]
    native.click(native.find("Restore previous INI"), draw)
    finish_job(workspace, native, draw)
    assert manager.profiles[0].files[workspace._profile_filename] == initial
    native.click(native.find("Back to Library"), draw)
    menu_action(native, draw, "Profile backups...")
    native.click(native.find("Back up current profile"), draw)
    finish_job(workspace, native, draw)
    assert len(manager.profile_backups) == 1
    identifier = manager.profile_backups[0]["id"]
    original_name = manager.profiles[0].name
    from creation_lib.mod_manager.profile_state import rename
    rename(manager.library, manager.profile_id, "Changed profile")
    manager.refresh()
    native.warm(draw)
    native.click(native.find("Review backup restore"), draw)
    finish_job(workspace, native, draw)
    assert manager.profile_restore.backup_id == identifier
    native.click(native.find("Cancel backup restore"), draw)
    assert manager.profiles[0].name == "Changed profile"
    native.click(native.find("Review backup restore"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Replace this local profile's configuration and organization"), draw)
    native.click(native.find("Restore reviewed profile backup"), draw)
    finish_job(workspace, native, draw)
    assert manager.profiles[0].name == original_name and len(manager.profile_backups) == 2


def test_saved_ini_is_not_replaced_by_stale_runtime_text_in_editor(native, workspace, tmp_path):
    import hashlib
    import json
    from .test_standalone import game_fixture
    from creation_lib.mod_manager.instances import create
    from creation_lib.mod_manager.profile_state import fo4_runtime_ini
    manager, draw = workspace.manager, panels(workspace)
    manager.library = Library(tmp_path / "local library", create=True)
    manager.profile_id = create(manager.library, "fo4", game_fixture(tmp_path))
    manager.refresh()
    name = "Fallout4Custom.ini"
    root = manager.library.root / "runtime" / "profiles" / manager.profile_id
    (root / "ini").mkdir(parents=True)
    (root / "ini" / name).write_text("[General]\nFixture=runtime\n", encoding="utf-8")
    digest = hashlib.sha256(fo4_runtime_ini(name, manager.profiles[0].files[name], manager.profile_id).encode()).hexdigest()
    (root / "ini_baseline.json").write_text(json.dumps({name.casefold(): digest}), encoding="utf-8")
    workspace._tab, workspace._profile_filename = "Profile config", name
    native.warm(draw)
    assert "Fixture=runtime" in workspace._ini_text
    native.text(native.find("##profile_ini_editor"), draw, "[General]\nFixture=saved\n")
    native.click(native.find("Save local INI"), draw)
    finish_job(workspace, native, draw)
    assert "Fixture=saved" in workspace._ini_text
    native.click(native.find("Save local INI"), draw)
    finish_job(workspace, native, draw)
    assert "Fixture=saved" in manager.profiles[0].files[name]


def test_backup_delete_acknowledgement_resets_when_profile_changes(native, workspace):
    from creation_lib.mod_manager.profile_backup import capture
    manager, draw = workspace.manager, panels(workspace)
    first = manager.profile_id
    second = manager.library.create_profile("Second", first)
    first_backup = capture(manager.library, first)
    second_backup = capture(manager.library, second)
    manager.refresh()
    workspace._tab = "Profile backups"
    native.warm(draw)
    assert workspace._backup_selected == first_backup
    native.click(native.find("Remove the selected metadata backup"), draw)
    assert workspace._backup_delete_ack
    manager.select_profile(second)
    native.warm(draw)
    assert workspace._backup_selected == second_backup and not workspace._backup_delete_ack


@pytest.mark.parametrize("size", [(1280, 720), (1600, 900)])
def test_main_log_contract_and_populated_data_pane_use_actual_input(native, workspace, size):
    manager, draw = workspace.manager, panels(workspace)
    workspace._show_filters = False
    native.io.display_size = size
    native.warm(draw)
    assert not any(item.label in {"Log", "Clear local log"} for item in native.items)
    assert not hasattr(workspace, "_log_entries")
    manager.inspect_files()
    finish_job(workspace, native, draw)
    native.click(native.find("Data"), draw)
    native.warm(draw)
    assert native.find("Refresh Data")
    native.click(native.find("textures###data_folder"), draw)
    native.warm(draw)
    assert native.find("shared.txt###data_file")
    native.text(native.find("##data_filter"), draw, "shared")
    native.click(native.find("shared.txt###data_file"), draw)
    assert workspace._file_selected == "textures/shared.txt" and workspace._dialog == "Files"


def test_persistent_download_resume_and_remove_history_use_actual_widgets(native, workspace):
    from .test_download_queue import CDN
    from .test_nexus import LINK
    from creation_lib.mod_manager.download_queue import DownloadPaused, DownloadQueue
    from creation_lib.mod_manager.nexus import parse_nxm
    manager, draw = workspace.manager, panels(workspace)
    cdn = CDN()
    manager.nexus_client = cdn.client()
    queue = DownloadQueue(manager.library)
    identifier = queue.enqueue(parse_nxm(LINK))
    cancel = threading.Event()
    with pytest.raises(DownloadPaused):
        queue.run(identifier, manager.nexus_client, parse_nxm(LINK), cancel, lambda *args: cancel.set())
    manager.refresh()
    native.warm(draw)
    native.click(native.find("Downloads"), draw)
    native.warm(draw)
    native.click(native.find("Resume queued download"), draw)
    finish_job(workspace, native, draw)
    assert len(manager.download_records) == 1 and manager.download_jobs[0]["state"] == "complete"
    path = Path(manager.download_records[0]["path"])
    native.click(native.find("Remove completed history"), draw)
    assert not manager.download_jobs and path.read_bytes() == cdn.payload


def test_nexus_file_review_and_account_actions_use_actual_widgets(native, workspace):
    from .test_nexus_state import Transport
    from creation_lib.mod_manager.nexus import NexusClient
    manager = workspace.manager
    row = next(item for item in manager.rows if item.name == "Base")
    with manager.library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1,version='1.2' WHERE id=?", (row.id,))
    manager.refresh()
    manager.selected_id = row.id
    transport = Transport()
    transport.endorsements, transport.tracked = [], []
    client = NexusClient("fixture-secret", transport=transport)
    client.user = {"user_id": 42, "name": "Fixture", "premium": True}
    manager.nexus_client = client
    workspace._show_filters = False
    def draw():
        workspace.draw()
        imgui.set_next_window_pos((0, 0))
        imgui.set_next_window_size((1000, 1000))
        imgui.begin("Information fixture", flags=imgui.WindowFlags_.no_saved_settings)
        workspace._draw_details(embedded=True)
        imgui.end()
        if workspace._dialog:
            workspace._draw_workflow()
    native.warm(draw)
    native.click(native.find("Refresh Nexus files and account"), draw)
    finish_job(workspace, native, draw)
    assert manager.nexus_states[("fallout4", 123)].endorsement == "Undecided"
    assert len([item for item in native.items if item.label == "Queue replacement download"]) == 2
    native.click(native.find("Endorse on Nexus"), draw)
    assert not manager.busy
    native.click(native.find("Send this request to the shown Nexus account"), draw)
    native.click(native.find("Send reviewed account request"), draw)
    finish_job(workspace, native, draw)
    assert transport.requests[-1].full_url.endswith("/endorse.json")
    assert "Refresh account state" in manager.status and not manager.nexus_states
    native.click(native.find("Track on Nexus"), draw)
    native.click(native.find("Send this request to the shown Nexus account"), draw)
    native.click(native.find("Send reviewed account request"), draw)
    finish_job(workspace, native, draw)
    assert transport.requests[-1].get_method() == "POST" and transport.requests[-1].full_url.endswith("/tracked_mods.json")




def test_plugin_pane_review_cancel_apply_and_archive_member_filter_use_actual_input(native, workspace, tmp_path, monkeypatch):
    from .test_standalone import game_fixture
    from .test_archive_index import ba2_fixture
    from creation_lib.mod_manager.instances import create
    manager, draw = workspace.manager, panels(workspace)
    game = game_fixture(tmp_path)
    for name in ("Fallout4.esm", "Addon.esp"):
        (game / "Data" / name).write_bytes(b"generated placeholder")
    ba2_fixture(game / "Data" / "Addon - Main.ba2", ["textures/fixture.txt", "meshes/other.txt"])
    manager.library = Library(tmp_path / "standalone library", create=True)
    manager.profile_id = create(manager.library, "fo4", game)
    manager.refresh()
    workspace._right_pane = "Plugins"
    monkeypatch.setattr("creation_lib.mod_manager.plugin_state.modkit_metadata", lambda path, game_id, cancel: ({"author": "Fixture", "flags": {"esm": path.suffix == ".esm"}}, () if path.name == "Fallout4.esm" else ("Fallout4.esm",)))
    native.warm(draw)
    native.click(native.find("Refresh plugins"), draw)
    finish_job(workspace, native, draw)
    toggles = [item for item in native.items if item.label == "##plugin_enabled"]
    assert len(toggles) == 2
    native.click(toggles[1], draw)
    native.click([item for item in native.items if item.label == "##plugin_selection"][1], draw)
    native.click(native.find("Lock priority"), draw)
    native.click(native.find("Revert"), draw)
    assert manager.profiles[0].files["plugins.txt"] == ""
    toggles = [item for item in native.items if item.label == "##plugin_enabled"]
    native.click(toggles[1], draw)
    native.click(native.find("Lock priority"), draw)
    native.click(native.find("Save load order"), draw)
    finish_job(workspace, native, draw)
    assert manager.profiles[0].files["plugins.txt"] == "*Fallout4.esm\n*Addon.esp\n"
    assert manager.profiles[0].files["lockedorder.txt"] == "Addon.esp|1\n"
    assert workspace.panes.locked == {"addon.esp": ("Addon.esp", 1)}
    native.click(native.find("Archives"), draw)
    native.warm(draw)
    native.click(native.find("Refresh archives"), draw)
    finish_job(workspace, native, draw)
    assert len(manager.archive_index.members) == 2
    native.text(native.find("##archive_filter"), draw, "textures")
    assert [member.path for member in workspace.panes._archive_members] == ["textures/fixture.txt"]
    native.click(native.find("Overlapping archive candidates"), draw)
    assert workspace.panes._archive_members == []
    assert game.joinpath("Data", "Addon.esp").read_bytes() == b"generated placeholder"


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900), (1920, 1080)])
def test_executable_editor_review_cancel_run_remove_and_settings_use_real_input(native, workspace, tmp_path, monkeypatch, size):
    native.io.display_size = size
    draw, manager = panels(workspace, responsive=True), workspace.manager
    binary = tmp_path / "harmless fixture.exe"
    binary.write_bytes(b"generated fixture; not executed")
    calls = []
    popen = subprocess.Popen
    def launch(*args, **kwargs):
        if str(args[0][0]) == str(binary):
            calls.append((args, kwargs))
            return SimpleNamespace(pid=123)
        return popen(*args, **kwargs)
    monkeypatch.setattr(subprocess, "Popen", launch)
    native.warm(draw)
    native.click(native.find("Launch options"), draw)
    native.click(native.find("Edit executables..."), draw)
    native.click(native.find("Add"), draw)
    native.text(native.find("##exe_title"), draw, "Fixture tool")
    native.text(native.find("##exe_binary"), draw, str(binary))
    native.text(native.find("##exe_arguments"), draw, '"two words" &')
    native.choose_next("##exe_mode", draw)
    assert workspace.launcher.mode == "direct"
    native.click(native.find("Save and close"), draw)
    assert len(manager.executables) == 1
    saved_id = manager.executables[0].id
    run = native.find("Run")
    assert run.rect[2] < size[0] and run.rect[3] < size[1]
    native.click(run, draw)
    finish_job(workspace, native, draw)
    assert manager.launch_review.runtime == "direct"
    native.click(native.find("Cancel launch"), draw)
    assert not calls and manager.launch_review is None
    native.click(native.find("Run"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Start reviewed executable"), draw)
    finish_job(workspace, native, draw)
    assert len(calls) == 1 and calls[0][1]["shell"] is False
    assert "not virtualized" in manager.status
    native.click(native.find("Launch options"), draw)
    native.click(native.find("Edit executables..."), draw)
    native.click(native.find("Remove"), draw)
    native.click(native.find("Save and close"), draw)
    assert manager.executables == [] and saved_id
    native.click(native.find("Settings"), draw)
    settings = workspace.collect_settings()
    assert "runtime_game" in settings and "runtime_bundle" in settings


def test_project_review_requires_minimal_ini_choice_cancel_resets_and_sources_protected(native, workspace, tmp_path, monkeypatch):
    from creation_lib.mod_manager.launcher import Executable
    from creation_lib.mod_manager import runtime
    from .test_runtime import pe

    native.io.display_size = (1600, 900)
    manager, draw = workspace.manager, panels(workspace, responsive=True)
    for row in manager.rows:
        if row.id.startswith("duplicate-"):
            manager.library.customize(manager.profile_id, row.id, enabled=False)
    game, bundle, user = (tmp_path / name for name in ("game", "bundle", "user"))
    for path in (game / "Data", bundle, user / "plugins", user / "ini"):
        path.mkdir(parents=True)
    from .test_launch_resolution import installation
    from .test_pe_metadata import fixture_version
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", fixture_version)
    installation(game)
    binary = game / "f4se_loader.exe"
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c),
                          ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    item = manager.save_executable(Executable("", "F4SE", str(binary)))
    workspace.launcher.game_root, workspace.launcher.bundle_root = str(game), str(bundle)
    trust_fixture_bundle(workspace, monkeypatch)
    workspace.launcher.selected_id = item.id
    prepare = runtime.prepare
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: bundle)
    monkeypatch.setattr(runtime, "prepare", lambda *args, **kwargs: prepare(*args, **kwargs, user_targets=(user / "plugins", user / "ini")))
    calls = []
    monkeypatch.setattr(runtime, "start", lambda *args, **kwargs: calls.append((args, kwargs)) or (None, None, {"state": "completed"}))
    native.warm(draw)
    native.click(native.find("Run"), draw)
    finish_job(workspace, native, draw)
    assert manager.launch_review.minimal_ini
    native.click(native.find("Start reviewed executable"), draw)
    assert not manager.busy and not calls
    assert not any(item.label == "I understand programs may modify existing source files" for item in native.items)
    native.click(native.find("Cancel launch"), draw)
    assert manager.launch_review is None and not calls
    native.click(native.find("Run"), draw)
    finish_job(workspace, native, draw)
    assert not workspace.launcher.source_writes_ack and not workspace.launcher.minimal_ini_ack
    native.click(native.find("Use minimal local settings for missing INIs"), draw)
    native.click(native.find("Start reviewed executable"), draw)
    finish_job(workspace, native, draw)
    assert len(calls) == 1
    assert calls[0][1]["accept_source_writes"] is False and calls[0][1]["accept_minimal_ini"] is True
    assert "Project filespace runtime completed" in manager.status


def test_profile_delete_cancel_confirm_restore_and_restart_use_real_input(native, workspace, tmp_path):
    manager, draw = workspace.manager, panels(workspace)
    target = manager.profile_id
    before = manager.library.rows(target)
    source = Path(manager.import_preview.root)
    source_files = {str(path): path.read_bytes() for path in source.glob("profiles/*/*") if path.is_file()}
    native.warm(draw)
    native.click(native.find("Profiles"), draw)
    native.click(native.find("Delete profile..."), draw)
    native.click(native.find("Cancel"), draw)
    assert manager.profile_id == target and len(manager.profiles) == 2
    native.click(native.find("Profiles"), draw)
    native.click(native.find("Delete profile..."), draw)
    native.click(native.find("Delete local profile"), draw)  # Disabled until explicit confirmation.
    assert len(manager.profiles) == 2
    native.click(native.find("Remove only this local profile"), draw)
    native.click(native.find("Delete local profile"), draw)
    finish_job(workspace, native, draw)
    assert len(manager.profiles) == 1 and manager.profile_id != target
    reopened = Library(manager.library.root)
    assert reopened.deleted_profiles()[0]["id"] == target
    native.click(native.find("Profiles"), draw)
    native.click(native.find("Removed profiles"), draw)
    native.click(native.find("Restore profile"), draw)
    finish_job(workspace, native, draw)
    assert manager.profile_id == target and manager.rows == before
    assert {str(path): path.read_bytes() for path in source.glob("profiles/*/*") if path.is_file()} == source_files


def test_group_tree_collapse_search_reveal_selection_and_saved_expansion(native, workspace):
    manager, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    initial = visible_rows(native, manager, workspace)
    native.click(initial["duplicate-1"], draw)
    root = next(item for node, item in zip(workspace._visible_tree, [item for item in native.items if item.label in {"##select_mod", "##group_node"}]) if node.key == "group:Visual")
    x1, y1, _, y2 = root.rect
    native.click(Item(root.label, root.id, root.window, (x1, y1, x1 + 16, y2)), draw)
    assert "group:Visual" in workspace._closed_groups[manager.profile_id]
    assert not any(item.label == "##select_mod" for item in native.items)
    assert manager.selected_id == "duplicate-1"
    native.text(native.find("##search_mods"), draw, "Same name")
    filtered = visible_rows(native, manager, workspace)
    assert set(filtered) == {"duplicate-0", "duplicate-1"}
    assert filtered["duplicate-1"].id == initial["duplicate-1"].id
    native.text(native.find("##search_mods"), draw, "")
    assert len(workspace._visible_tree) == 1
    saved = workspace.collect_settings()
    assert saved["closed_groups"][manager.profile_id] == ["group:Visual"]
    restored = ModManagerWorkspace()
    try:
        restored.apply_settings(saved)
        assert restored._closed_groups[manager.profile_id] == {"group:Visual"}
    finally:
        restored.cleanup()


def test_tree_keyboard_selection_and_space_toggle_use_actual_input(native, workspace):
    manager, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.text(native.find("##search_mods"), draw, "Same name")
    rows = visible_rows(native, manager, workspace)
    native.click(rows["duplicate-0"], draw)
    native.key(imgui.Key.down_arrow, draw)
    assert manager.selected_id == "duplicate-1"
    native.key(imgui.Key.up_arrow, draw)
    assert manager.selected_id == "duplicate-0"
    native.click(visible_rows(native, manager, workspace)["duplicate-0"], draw)
    native.key(imgui.Key.space, draw)
    finish_job(workspace, native, draw)
    assert not manager.rows_by_id["duplicate-0"].enabled


def test_duplicate_library_rows_keep_ids_when_sorted_filtered_and_edited(native, workspace):
    draw, manager = panels(workspace), workspace.manager
    native.warm(draw)
    native.click(native.find("Filter options"), draw)
    initial = visible_rows(native, manager, workspace)
    assert len(initial) == 7
    native.click(initial["duplicate-1"], draw)
    assert manager.selected_id == "duplicate-1"
    native.choose_next("##display_sort", draw)
    assert workspace._sort_by == "name"
    sorted_rows = visible_rows(native, manager, workspace)
    assert {key: item.id for key, item in sorted_rows.items()} == {key: item.id for key, item in initial.items()}
    native.text(native.find("##search_mods"), draw, "Same name")
    assert workspace._query == "Same name"
    filtered = visible_rows(native, manager, workspace)
    assert set(filtered) == {"duplicate-0", "duplicate-1"}
    assert filtered["duplicate-1"].id == initial["duplicate-1"].id
    native.click(filtered["duplicate-0"], draw)
    assert manager.selected_id == "duplicate-0"
    native.text(native.find("##group"), draw, "Visual/Custom")
    assert workspace._detail_group == "Visual/Custom"
    native.click(native.find("Save organization"), draw)
    finish_job(workspace, native, draw)
    assert manager.rows_by_id["duplicate-0"].group == "Visual/Custom"
    native.text(native.find("##search_mods"), draw, "")
    native.choose_next("##group_filter", draw)
    assert workspace._group == "Visual"
    assert len(visible_rows(native, manager, workspace)) == 7


def test_duplicate_profile_names_switch_correct_profile_and_scope_detail_fields(native, workspace):
    manager, draw = workspace.manager, panels(workspace)
    manager.selected_id = manager.rows[1].id
    original = manager.profile_id
    native.warm(draw)
    detail_id = native.find("##group").id
    native.click(native.find("Profiles"), draw)
    native.click(native.find("New profile"), draw)
    native.text(native.find("##profile_name"), draw, "Default")
    native.click(native.find("Clone current profile"), draw)
    assert not workspace._new_profile_clone
    native.click(native.find("Clone current profile"), draw)
    assert workspace._new_profile_clone
    native.click(native.find("Create profile"), draw)
    finish_job(workspace, native, draw)
    clone = manager.profile_id
    assert clone != original
    native.click(native.find("##profile"), draw)
    profile_items = [item for item in native.items if item.label == "##choose_profile"]
    assert len(profile_items) == len(manager.profiles) == 3
    original_item = profile_items[next(index for index, profile in enumerate(manager.profiles) if profile.id == original)]
    native.click(original_item, draw)
    assert manager.profile_id == original and native.find("##group").id == detail_id
    native.click(native.find("##profile"), draw)
    profile_items = [item for item in native.items if item.label == "##choose_profile"]
    target = profile_items[next(index for index, profile in enumerate(manager.profiles) if profile.id == clone)]
    native.click(target, draw)
    assert manager.profile_id == clone
    assert native.find("##group").id != detail_id
    native.text(native.find("##group"), draw, "Clone edits")
    native.click(native.find("Save organization"), draw)
    finish_job(workspace, native, draw)
    assert manager.rows_by_id[manager.selected_id].group == "Clone edits"
    original_row = next(row for row in manager.library.rows(original) if row.id == manager.selected_id)
    assert original_row.group == "Visual"


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["success", "cancelled", "failed"])
def test_source_save_browser_and_reviewed_owned_import_use_actual_input(native, workspace, tmp_path, monkeypatch, size, outcome):
    from creation_lib.mod_manager.instances import create
    from .test_saves import save_fixture
    from .test_standalone import game_fixture
    native.io.display_size = size
    manager, draw = workspace.manager, panels(workspace)
    manager.library = Library(tmp_path / "source copy library", create=True)
    manager.profile_id = create(manager.library, "fo4", game_fixture(tmp_path))
    manager.refresh()
    source = tmp_path / "selected source folder"
    source.mkdir()
    path = save_fixture(source / "Source.fos")
    path.with_suffix(".f4se").write_bytes(b"generated source companion")
    before = {item.name: item.read_bytes() for item in source.iterdir()}
    workspace._right_pane = "Saves"
    native.warm(draw)
    native.click(native.find("Save locations"), draw)
    native.click(native.find("Browse source/global folder..."), draw)
    native.text(native.find("##save_source_folder"), draw, str(source))
    native.click(native.find("Read selected save folder"), draw)
    finish_job(workspace, native, draw)
    assert manager.save_index.external
    native.click(native.find("Show source saves"), draw)
    native.click(native.find("Source.fos###owned_save"), draw)
    native.click(native.find("Inspect save header"), draw)
    finish_job(workspace, native, draw)
    assert manager.save_details[str(path)]["player"] == "Fixture Character"
    native.click(native.find("Import / back up source save"), draw)
    assert workspace._save_copy_target == manager.profile_id
    native.click(native.find("Review save copy"), draw)
    finish_job(workspace, native, draw)
    assert manager.save_transfer_plan.source_folder == str(source)
    native.click(native.find("Cancel save copy"), draw)
    assert not manager.save_transfer_plan
    native.click(native.find("Review save copy"), draw)
    finish_job(workspace, native, draw)
    started, release = threading.Event(), threading.Event()
    def checkpoint(stage):
        if stage == "staged":
            if outcome == "failed":
                raise OSError("disposable failed source save copy")
            if outcome == "cancelled":
                started.set()
                assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", checkpoint)
    native.click(native.find("Create these reviewed owned copies"), draw)
    native.click(native.find("Copy reviewed save pair"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    owned = manager.library.root / "runtime/profiles" / manager.profile_id / "saves"
    if outcome == "success":
        finish_job(workspace, native, draw)
        assert owned.joinpath("Source.fos").read_bytes() == before["Source.fos"]
        assert owned.joinpath("Source.f4se").read_bytes() == before["Source.f4se"]
        native.click(native.find("Back to Library"), draw)
        native.click(native.find("Save locations"), draw)
        native.click(native.find("Owned profile saves"), draw)
        finish_job(workspace, native, draw)
        assert not manager.save_index.external and native.find("Source.fos###owned_save")
    else:
        manager._future.exception(timeout=10)
        manager.poll()
        assert manager.job_state == outcome and not owned.exists()
    assert {item.name: item.read_bytes() for item in source.iterdir()} == before


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_mo2_profile_source_save_location_is_explicit_and_read_only(native, workspace, size):
    from .test_saves import save_fixture
    native.io.display_size = size
    manager, draw = workspace.manager, panels(workspace)
    profile = next(item for item in manager.profiles if item.id == manager.profile_id)
    source = Path(profile.source) / "saves"
    source.mkdir()
    path = save_fixture(source / "MO2.fos")
    before = path.read_bytes()
    workspace._right_pane = "Saves"
    native.warm(draw)
    assert manager.save_index is None
    native.click(native.find("Save locations"), draw)
    native.click(native.find("MO2 profile source saves"), draw)
    finish_job(workspace, native, draw)
    assert manager.save_index.external and manager.save_index.root == str(source)
    native.click(native.find("MO2.fos###owned_save"), draw)
    native.click(native.find("Inspect save header"), draw)
    finish_job(workspace, native, draw)
    assert path.read_bytes() == before


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_installer_actions_stay_visible_while_long_options_and_review_scroll(native, workspace, tmp_path, size):
    native.io.display_size = size
    manager, draw = workspace.manager, panels(workspace)
    package = Package(make_package(tmp_path))
    options = "".join(f'<plugin name="Choice {index}"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>' for index in range(100))
    xml = f'''<config><moduleName>Long options</moduleName><requiredInstallFiles><file source="base.txt"/></requiredInstallFiles>
      <installSteps><installStep name="Many choices"><optionalFileGroups><group name="Options" type="SelectAny"><plugins>{options}</plugins></group></optionalFileGroups></installStep></installSteps></config>'''
    manager.prepared = PreparedInstall(package, Fomod(xml.encode()), Environment())
    workspace._install_name, workspace._tab = "Long options fixture", "Install"
    native.warm(draw)
    review = native.find("Review destination files")
    native.click(review, draw)
    finish_job(workspace, native, draw)
    install = native.find("Install into library")
    assert review.window == install.window and "installer_details" not in install.window
    native.scroll(native.find("Choice 0###choose_option"), draw, -100000)
    assert native.find("Review destination files").rect == review.rect
    assert native.find("Install into library").rect == install.rect
    native.click(native.find("Install into library"), draw)
    finish_job(workspace, native, draw)
    assert any(row.name == "Long options fixture" for row in manager.rows)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["success", "cancelled", "failed"])
def test_owned_save_backup_restore_uses_review_ack_cancel_and_failure_widgets(native, workspace, tmp_path, monkeypatch, size, outcome):
    from creation_lib.mod_manager.instances import create
    from creation_lib.mod_manager.save_transfer import apply, review
    from creation_lib.mod_manager.saves import inventory
    from .test_saves import save_fixture
    from .test_standalone import game_fixture
    native.io.display_size = size
    manager, draw = workspace.manager, panels(workspace)
    manager.library = Library(tmp_path / "restore library", create=True)
    manager.profile_id = create(manager.library, "fo4", game_fixture(tmp_path))
    manager.refresh()
    root = manager.library.root / "runtime/profiles" / manager.profile_id / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    path.with_suffix(".f4se").write_bytes(b"generated companion")
    original = path.read_bytes()
    backup = Path(apply(manager.library, review(manager.library, inventory(manager.library, manager.profile_id), str(path))))
    path.unlink()
    path.with_suffix(".f4se").unlink()
    workspace._right_pane = "Saves"
    native.warm(draw)
    native.click(native.find("Save locations"), draw)
    native.click(native.find("Save backups"), draw)
    native.click(native.find("Refresh save backups"), draw)
    finish_job(workspace, native, draw)
    native.click(next(item for item in native.items if item.label.endswith("###save_backup")), draw)
    native.click(native.find("Review save backup restore"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Restore reviewed save pair"), draw)
    assert not manager.busy and not path.exists()
    native.click(native.find("Cancel save restore"), draw)
    assert manager.save_transfer_plan is None and not path.exists()
    native.click(native.find("Review save backup restore"), draw)
    finish_job(workspace, native, draw)
    started, release = threading.Event(), threading.Event()
    def checkpoint(stage):
        if stage == "staged":
            if outcome == "failed":
                raise OSError("disposable failed backup restore")
            if outcome == "cancelled":
                started.set()
                assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", checkpoint)
    native.click(native.find("Restore these reviewed copies into this profile"), draw)
    native.click(native.find("Restore reviewed save pair"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if outcome == "success":
        finish_job(workspace, native, draw)
        assert path.read_bytes() == original and path.with_suffix(".f4se").read_bytes() == b"generated companion"
    else:
        manager._future.exception(timeout=10)
        manager.poll()
        assert manager.job_state == outcome and not list(root.iterdir())
    assert backup.joinpath("Fixture.fos").read_bytes() == original


def test_fomod_duplicate_groups_options_and_canonical_dependencies_install_via_widgets(native, workspace, tmp_path):
    manager, draw = workspace.manager, panels(workspace)
    package = Package(make_package(tmp_path))
    xml = b'''<config><moduleName>Repeated labels</moduleName>
      <requiredInstallFiles><file source="base.txt" destination="base.txt"/></requiredInstallFiles>
      <installSteps><installStep name="Options"><optionalFileGroups>
       <group name="Same group" type="SelectAny"><plugins>
        <plugin name="Same option"><files><file source="detail.txt"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
        <plugin name="Same option"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
       </plugins></group>
       <group name="Same group" type="SelectAny"><plugins>
        <plugin name="Same option"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
       </plugins></group></optionalFileGroups></installStep></installSteps>
      <conditionalFileInstalls><patterns>
       <pattern><dependencies><fileDependency file="Data\\Fixture.esp" state="Missing"/><fileDependency file="data/fixture.esp" state="Missing"/></dependencies><files><file source="patch.txt"/></files></pattern>
       <pattern><dependencies><fileDependency file="one###same.esp" state="Missing"/><fileDependency file="two###same.esp" state="Missing"/></dependencies><files><file source="readme.txt"/></files></pattern>
      </patterns></conditionalFileInstalls></config>'''
    manager.prepared = PreparedInstall(package, Fomod(xml), Environment({"one###same.esp": "Missing", "two###same.esp": "Missing"}))
    workspace._install_name, workspace._tab = "Repeated-label fixture", "Install"
    native.warm(draw)
    native.click(native.find("Dependency environment"), draw)
    assert len([item for item in native.items if item.label == "##file_state"]) == 3
    states = [item for item in native.items if item.label == "##file_state"]
    native.click(states[0], draw)
    native.key(imgui.Key.down_arrow, draw)
    native.key(imgui.Key.enter, draw)
    assert manager.prepared.environment.files["data/fixture.esp"] == "Missing"
    native.click(native.find("Dependency environment"), draw)
    options = [item for item in native.items if item.label == "Same option###choose_option"]
    assert len(options) == 3 and len({item.id for item in options}) == 3
    native.click(options[0], draw)
    native.click([item for item in native.items if item.label == "Same option###choose_option"][2], draw)
    assert manager.choices == {"0:0": {"0:0:0"}, "0:1": {"0:1:0"}}
    native.click(native.find("Next"), draw)
    native.click(native.find("Back"), draw)
    assert [item.id for item in native.items if item.label == "Same option###choose_option"] == [item.id for item in options]
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    assert manager.install_plan
    native.click(native.find("Install into library"), draw)
    finish_job(workspace, native, draw)
    assert any(row.name == "Repeated-label fixture" for row in manager.rows)


def test_first_use_import_creates_automatic_destination_and_navigates_profiles_config(native, workspace, tmp_path):
    manager, draw = workspace.manager, panels(workspace)
    source = manager.import_preview.root
    manager.library, manager.import_preview = None, None
    manager.refresh()
    workspace._library_path, workspace._tab = "", "Import MO2"
    native.warm(draw)
    native.text(native.find("##mo2_path"), draw, source)
    native.click(native.find("Advanced import options"), draw)
    native.click(native.find("Preview import"), draw)
    finish_job(workspace, native, draw)
    assert manager.import_preview and not manager.library
    preview = manager.import_preview
    preview.configuration["game"]["selected_profile"] = "Default"
    paths = [Path(source) / "ModOrganizer.ini", Path(source) / "profiles" / "Default" / "modlist.txt"]
    before = [path.read_bytes() for path in paths]
    automatic = tmp_path / "app data" / "mod_manager" / "library"
    assert not automatic.exists() and workspace._library_path == ""
    assert not any(item.label == "Create library" and "import_destination" in item.window for item in native.items)
    native.click(native.find("Import selected profiles"), draw)
    finish_job(workspace, native, draw)
    assert manager.library.root == automatic
    assert workspace.collect_settings()["library_path"] == str(automatic)
    assert len(manager.profiles) == 2 and len(manager.rows) == 4
    assert next(profile.name for profile in manager.profiles if profile.id == manager.profile_id) == "Default"
    assert manager.instance_configuration["paths"]["instance"] == source
    assert "Imported 2 new profiles" in manager.status
    assert native.find("##search_mods")  # Successful commit returned to the navigable library.
    menu_action(native, draw, "Profile configuration")
    snapshot = native.find("##profile_ini_editor")
    original = next(profile for profile in manager.profiles if profile.id == manager.profile_id).files[workspace._profile_filename]
    native.text(snapshot, draw, "Unsaved edits stay local to the editor")
    assert next(profile for profile in manager.profiles if profile.id == manager.profile_id).files[workspace._profile_filename] == original
    native.click(native.find("Back to Library"), draw)
    menu_action(native, draw, "Import MO2 instance...")
    native.click(native.find("Import selected profiles"), draw)
    finish_job(workspace, native, draw)
    assert "Imported 0 new profiles" in manager.status and len(manager.profiles) == 2
    assert [path.read_bytes() for path in paths] == before
    from ui.toolkit.settings import ToolkitSettings
    settings_path = tmp_path / "isolated settings.json"
    settings = ToolkitSettings(path=settings_path, editor_settings_path=tmp_path / "absent editor.json")
    settings.set_workspace_settings("mod_manager", workspace.collect_settings())
    settings.save()
    loaded = ToolkitSettings(path=settings_path, editor_settings_path=tmp_path / "absent editor.json")
    restored = ModManagerWorkspace()
    try:
        restored.apply_settings(loaded.get_workspace_settings("mod_manager"))
        restored_draw = panels(restored)
        native.warm(restored_draw)
        finish_job(restored, native, restored_draw)
        assert restored.manager.library.root == automatic and len(restored.manager.profiles) == 2
        assert restored.manager.library.library_id == manager.library.library_id
        assert native.find("##search_mods")
    finally:
        restored.cleanup()


def test_custom_import_destination_failure_retry_and_default_selection_use_real_widgets(native, workspace, tmp_path):
    manager, draw = workspace.manager, panels(workspace)
    source = manager.import_preview.root
    manager.library = None
    manager.refresh()
    workspace._library_path, workspace._tab = "", "Import MO2"
    native.warm(draw)
    native.click(native.find("Advanced import options"), draw)
    forbidden = Path(source) / "new destination"
    native.text(native.find("##library_path", window="import_destination"), draw, str(forbidden))
    native.click(native.find("Import selected profiles"), draw)
    assert not forbidden.exists() and "outside the MO2" in manager.error
    occupied = tmp_path / "unexpected files"
    occupied.mkdir()
    sentinel = occupied / "keep.txt"
    sentinel.write_bytes(b"do not adopt")
    native.text(native.find("##library_path", window="import_destination"), draw, str(occupied))
    native.click(native.find("Import selected profiles"), draw)
    if manager._future:
        try:
            manager._future.result(timeout=10)
        except Exception:
            pass
    manager.poll()
    native.warm(draw)
    assert manager.job_state == "failed" and manager.library is None
    assert "empty folder" in manager.error and str(occupied) in manager.error
    assert native.find("Import selected profiles") and sentinel.read_bytes() == b"do not adopt"
    custom = tmp_path / "custom" / "missing parents" / "library"
    native.text(native.find("##library_path", window="import_destination"), draw, str(custom))
    native.click(native.find("Import selected profiles"), draw)
    finish_job(workspace, native, draw)
    assert manager.library.root == custom and len(manager.profiles) == 2
    menu_action(native, draw, "Import MO2 instance...")
    native.click(native.find("Use default location"), draw)
    automatic = tmp_path / "app data" / "mod_manager" / "library"
    assert workspace._library_path == str(automatic) and not automatic.exists()
    native.click(native.find("Import selected profiles"), draw)
    finish_job(workspace, native, draw)
    assert manager.library.root == automatic and len(manager.profiles) == 2


def test_link_source_approval_preserves_selection_imports_and_stages_through_widgets(native, workspace, tmp_path):
    manager, draw = workspace.manager, panels(workspace)
    source = Path(manager.import_preview.root)
    alias, target = source / "mods" / "Linked content", tmp_path / "approved target"
    target.mkdir()
    (target / "linked.txt").write_bytes(b"synthetic source mapping")
    (target / "meta.ini").write_text("[General]\nversion=1\n")
    (source / "profiles" / "Default" / "modlist.txt").write_text("+Linked content\n+Base\n")
    junction(alias, target)
    try:
        manager.library = Library(tmp_path / "linked library", create=True)
        manager.inspect_mo2(source)
        finish_job(workspace, native, draw)
        workspace._mo2_path, workspace._tab = str(source), "Import MO2"
        native.warm(draw)
        native.click(native.find("Advanced import options"), draw)
        alternate = next(profile for profile in manager.import_preview.profiles if profile.name == "Alternate")
        native.click(native.find("Alternate (4 entries)###include_profile"), draw)
        assert alternate.id not in workspace._import_selected
        native.click(native.find("Linked mod sources (1)###linked_sources"), draw)
        native.warm(draw)
        native.scroll(native.find("Advanced import options"), draw, -100)
        native.click(native.find("Use target read-only"), draw)
        finish_job(workspace, native, draw)
        assert manager.import_preview.linked_sources[0].source == str(target.resolve())
        assert alternate.id not in workspace._import_selected
        assert workspace.collect_settings()["source_mappings"][str(alias)] == str(target)
        native.click(native.find("Import selected profiles"), draw)
        finish_job(workspace, native, draw)
        assert len(manager.profiles) == 1 and len(manager.rows) == 2
        linked = next(row for row in manager.rows if row.name == "Linked content")
        assert linked.source == str(target.resolve())
        menu_action(native, draw, "Stage profile export...")
        native.click(native.find("Preview files and conflicts"), draw)
        finish_job(workspace, native, draw)
        assert "linked.txt" in manager.export_preview.files
        native.click(native.find("Create staged export"), draw)
        finish_job(workspace, native, draw)
        assert "staged successfully" in manager.status
        assert (target / "linked.txt").read_bytes() == b"synthetic source mapping"
    finally:
        alias.rmdir() if os.name == "nt" else alias.unlink()


def test_one_click_cancellation_returns_to_library_and_resume_uses_real_input(native, workspace, tmp_path, monkeypatch):
    from .test_instance_migration import full_instance
    root, _, alias, _ = full_instance(tmp_path / "cancel fixture")
    manager, draw = workspace.manager, panels(workspace)
    manager.library = manager.import_preview = None
    manager.refresh()
    workspace._library_path, workspace._mo2_path, workspace._tab = "", str(root), "Import MO2"
    started, release = threading.Event(), threading.Event()
    original = Library.import_mo2
    def pause(library, *args, **kwargs):
        def checkpoint(name):
            started.set()
            assert release.wait(10)
            from creation_lib.mod_manager.paths import check_cancel
            check_cancel(manager._cancel)
        kwargs["checkpoint"] = checkpoint
        return original(library, *args, **kwargs)
    monkeypatch.setattr(Library, "import_mo2", pause)
    try:
        native.warm(draw)
        native.click(native.find("Import instance"), draw)
        assert started.wait(5)
        native.warm(draw)
        native.click(native.find("Cancel operation"), draw)
        release.set()
        if manager._future:
            manager._future.exception(timeout=10)
            manager.poll()
        native.warm(draw)
        assert manager.job_state == "cancelled" and workspace._dialog == ""
        assert not manager.profiles and not tuple((manager.library.root / "mods").iterdir())
        native.click(native.find("Resume migration"), draw)
        finish_job(workspace, native, draw)
        assert manager.instance_migration.state == "complete" and len(manager.instance_migration.referenced) == 5
        assert not tuple((manager.library.root / "mods").iterdir())
    finally:
        release.set()
        alias.rmdir() if os.name == "nt" else alias.unlink()


def test_owned_copy_and_file_conflict_filters_navigation_use_real_widgets(native, workspace, monkeypatch):
    manager, draw = workspace.manager, panels(workspace)
    row = next(item for item in manager.rows if item.name == "Override")
    manager.selected_id = row.id
    original = (Path(row.source) / "textures/shared.txt").read_bytes()
    native.warm(draw)
    assert not any(item.label == "Copy into library" for item in native.items)
    workspace._migration_selected, workspace._tab = {row.id}, "Copy sources"
    manager.review_migration([row.id])
    finish_job(workspace, native, draw)
    assert len(manager.migration_review.sources) == 1
    native.click(native.find("Copy these sources and preserve every profile reference"), draw)
    native.click(native.find("Copy into owned library"), draw)
    finish_job(workspace, native, draw)
    assert manager.rows_by_id[row.id].kind == "owned" and manager.origins[row.id]["source"] == row.source
    assert (Path(row.source) / "textures/shared.txt").read_bytes() == original
    native.click(native.find("Inspect mod files"), draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##search_files"), draw, "shared")
    native.choose_next("File view", draw, 2)  # Winning files from the selected Override.
    assert len(manager.file_index.filtered("shared", "winning", row.id)) == 1
    native.click(native.find("##select_file"), draw)
    providers = [item for item in native.items if item.label == "Select mod###provider_select"]
    assert len(providers) == 3 and len({item.id for item in providers}) == 3
    # Provider order is descending priority: Disabled, Override, Base.
    native.click(providers[-1], draw)
    base = next(item for item in manager.rows if item.name == "Base")
    assert manager.selected_id == base.id
    native.click(native.find("Inspect mod files"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Provider mod"), draw)
    native.text(native.find("##provider_search"), draw, "Base")
    native.click(native.find("##choose_provider"), draw)
    assert workspace._file_mod == base.id
    native.choose_next("File view", draw)  # Winning -> Losing for Base.
    assert workspace._file_mode == "losing"
    assert len(manager.file_index.filtered("shared", "losing", base.id)) == 1
    native.click(native.find("##select_file"), draw)
    clipboard, explorer = [], []
    monkeypatch.setattr(imgui, "set_clipboard_text", clipboard.append)
    monkeypatch.setattr("subprocess.Popen", lambda args: explorer.append(args))
    native.click([item for item in native.items if item.label == "Copy path###provider_copy"][-1], draw)
    native.click([item for item in native.items if item.label == "Reveal###provider_reveal"][-1], draw)
    assert clipboard == [str(Path(base.source) / "textures/shared.txt")]
    assert explorer == [["explorer.exe", f"/select,{Path(clipboard[0]).absolute()}"]]


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_owned_copy_cancel_and_capacity_failure_surface_without_source_changes(native, workspace, monkeypatch, outcome):
    manager, draw = workspace.manager, panels(workspace)
    row = next(item for item in manager.rows if item.name == "Override")
    manager.selected_id = row.id
    native.warm(draw)
    assert not any(item.label == "Copy into library" for item in native.items)
    workspace._migration_selected, workspace._tab = {row.id}, "Copy sources"
    manager.review_migration([row.id])
    finish_job(workspace, native, draw)
    original = (Path(row.source) / "textures/shared.txt").read_bytes()
    started, release = threading.Event(), threading.Event()
    if outcome == "cancelled":
        copy = manager.library._copy_plan
        def wait_before_copy(*args, **kwargs):
            started.set()
            assert release.wait(10)
            return copy(*args, **kwargs)
        monkeypatch.setattr(manager.library, "_copy_plan", wait_before_copy)
    else:
        monkeypatch.setattr("creation_lib.mod_manager.migration.shutil.disk_usage", lambda root: SimpleNamespace(free=0))
    native.click(native.find("Copy these sources and preserve every profile reference"), draw)
    native.click(native.find("Copy into owned library"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if manager._future:
        manager._future.exception(timeout=10)
        manager.poll()
    native.warm(draw)
    assert manager.job_state == outcome
    if outcome == "failed":
        assert "Insufficient free space" in manager.error
    assert manager.rows_by_id[row.id].kind == "external" and not manager.origins
    assert (Path(row.source) / "textures/shared.txt").read_bytes() == original
    assert not (manager.library.root / "mods" / row.id).exists()


def test_twenty_thousand_providers_are_clipped_and_last_provider_scrolls_into_view(native, workspace, monkeypatch):
    manager = workspace.manager
    # This test owns a synthetic index; discard pending real-fixture scan results
    # so their completion timing cannot replace it during the scroll gesture.
    manager._conflict_cancel.set()
    manager._conflict_generation += 1
    manager._conflict_due = float("inf")
    providers = tuple(Provider(f"provider-{i:05d}", f"Provider {i:05d}", i, True, False,
                               f"C:/disposable-fixture/provider-{i:05d}/file.txt", 1, 1) for i in range(20_000))
    entry = FileEntry("textures/many-providers.txt", providers)
    manager.file_index = FileIndex(manager.profile_id, "Synthetic fan-out", (entry,), (), ())
    workspace._tab, workspace._file_selected = "Files", entry.path
    seen, render = [], workspace._draw_file_provider
    def record(provider, winner):
        seen.append(provider.mod_id)
        render(provider, winner)
    monkeypatch.setattr(workspace, "_draw_file_provider", record)
    base_draw = panels(workspace)
    def draw():
        seen.clear()
        base_draw()
    native.warm(draw)
    assert 0 < len(seen) < 30 and "provider-19999" in seen
    native.scroll(next(item for item in native.items if item.label == "##provider_path"), draw, -100000)
    assert "provider-00000" in seen and len(seen) < 30


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900), (1920, 1080)])
def test_novice_one_click_migration_library_conflicts_and_settings_at_realistic_sizes(native, workspace, tmp_path, monkeypatch, size):
    from .test_instance_migration import full_instance
    root, _, alias, _ = full_instance(tmp_path / "full fixture")
    manager = workspace.manager
    manager.library = manager.import_preview = None
    manager.refresh()
    workspace._library_path, workspace._mo2_path, workspace._tab = "", "", "Library"
    native.io.display_size = size
    draw = panels(workspace, responsive=True)
    tabs = []
    begin_tabs = imgui.begin_tab_bar
    monkeypatch.setattr(imgui, "begin_tab_bar", lambda *args, **kwargs: tabs.append(args) or begin_tabs(*args, **kwargs))
    try:
        native.warm(draw)
        menu_action(native, draw, "Import MO2 instance...")
        native.text(native.find("##mo2_path"), draw, str(root))
        assert not any(item.label in {"Migrate from MO2###open_migration", "Install package", "Files", "More actions"} for item in native.items)
        assert not any(item.label in {"Preview import", "Import selected profiles", "Create library"} for item in native.items)
        native.click(native.find("Import instance"), draw)
        finish_job(workspace, native, draw)
        assert manager.instance_migration.state == "complete"
        assert len(manager.profiles) == 3 and len(manager.instance_migration.referenced) == 5
        assert workspace._dialog == "" and native.find("##search_mods")
        menu_action(native, draw, "Inspect files")
        native.click(native.find("Refresh file inspection"), draw)
        finish_job(workspace, native, draw)
        native.text(native.find("##search_files"), draw, "shared")
        native.click(native.find("##select_file"), draw)
        assert len([item for item in native.items if item.label == "Select mod###provider_select"]) >= 1
        native.click(native.find("Settings"), draw)
        native.warm(draw)
        assert workspace._page == "Settings" and native.find("Row height")
        native.click(native.find("Enable mods on install"), draw)
        assert not manager.enable_on_install
        native.click(native.find("Library"), draw)
        native.text(native.find("##search_mods"), draw, "Base")
        assert [row.name for row in manager.filtered("Base")] == ["Base"]
        assert {args[0] for args in tabs} == {"manager_panes", "manager_settings_sections"}
        right = size[0] - max(220, size[0] * .25) - 5
        for item in native.items:
            if item.window.startswith("Library") and item.label in {"Migrate from MO2###open_migration", "Install package", "Files", "Profile config", "Stage profile", "Recovery", "##search_mods"}:
                assert item.rect[2] <= right, (size, item)
    finally:
        alias.rmdir() if os.name == "nt" else alias.unlink()


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_recovery_backup_delete_review_cancel_ack_and_permanent_cleanup_actual_input(native, workspace, tmp_path, size):
    from .test_history_prune import fixture as owned_fixture
    native.io.display_size = size
    library, profile, mod, entry = owned_fixture(tmp_path / "prune input fixture")
    m, draw = workspace.manager, panels(workspace)
    m.library, m.profile_id = library, profile
    m.refresh()
    workspace._tab = "Recovery"
    native.warm(draw)
    native.click(native.find("Review permanent backup deletion"), draw)
    finish_job(workspace, native, draw)
    delete = native.find("Delete reviewed recovery backup")
    assert 0 <= delete.rect[1] < delete.rect[3] <= size[1]
    cancel = native.find("Cancel backup deletion")
    assert 0 <= cancel.rect[1] < cancel.rect[3] <= size[1]
    native.click(delete, draw)
    assert not m.busy and Path(entry["path"]).is_dir()
    native.click(native.find("Cancel backup deletion"), draw)
    assert m.prune_review is None and len(library.history()) == 1
    native.click(native.find("Review permanent backup deletion"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Permanently delete this reviewed recovery backup"), draw)
    native.click(native.find("Delete reviewed recovery backup"), draw)
    finish_job(workspace, native, draw)
    assert m.prune_review is None and not m.recovery_history and not Path(entry["path"]).exists()
    assert (library.root / "mods" / mod / "file.txt").read_text() == "replacement"
    assert m.rows_by_id[mod].name == "Fixture"


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_installer_compatibility_switch_resets_review_and_installs_selected_policy_actual_input(native, workspace, tmp_path, size):
    from .test_fomod_policy_controller import make_policy_package
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    m.prepare_install(make_policy_package(tmp_path / "policy input package"))
    finish_job(workspace, native, draw)
    workspace._tab, workspace._install_name = "Install", "Policy input result"
    native.warm(draw)
    assert m.prepared.fomod.compatibility == "mo2"
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    assert m.install_plan["chosen.txt"] == "first.txt"
    native.choose_next("Installer compatibility", draw)
    finish_job(workspace, native, draw)
    assert m.prepared.fomod.compatibility == "schema" and m.install_plan is None
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    assert m.install_plan["chosen.txt"] == "second.txt"
    install = native.find("Install into library")
    assert 0 <= install.rect[1] < install.rect[3] <= size[1]
    native.click(install, draw)
    finish_job(workspace, native, draw)
    assert not workspace._dialog
    assert (m.library.root / "mods" / m.selected_id / "chosen.txt").read_bytes() == b"second"


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_installer_policy_cancel_failure_keeps_prior_review_actual_input(native, workspace, tmp_path, monkeypatch, size, outcome):
    from .test_fomod_policy_controller import make_policy_package
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    m.prepare_install(make_policy_package(tmp_path / "policy cancellation package"))
    finish_job(workspace, native, draw)
    workspace._tab, workspace._install_name = "Install", "Policy cancelled result"
    native.warm(draw)
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    before, plan = m.prepared, dict(m.install_plan)
    started, release = threading.Event(), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("disposable policy input failure")
        started.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.fomod_compatibility._checkpoint", stop)
    native.choose_next("Installer compatibility", draw)
    try:
        if outcome == "cancelled":
            assert started.wait(10)
            native.warm(draw)
            native.click(native.find("Install into library"), draw)
            assert m.busy and m.prepared is before
            native.click(native.find("Cancel operation"), draw)
            assert m._cancel.is_set()
    finally:
        release.set()
    m._future.exception(timeout=10)
    m.poll()
    native.warm(draw)
    assert m.job_state == outcome and m.prepared is before and m.install_plan == plan
    assert m.prepared.fomod.compatibility == "mo2"


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_recovery_backup_delete_cancel_failure_after_move_actual_input(native, workspace, tmp_path, monkeypatch, size, outcome):
    from .test_history_prune import fixture as owned_fixture
    native.io.display_size = size
    library, profile, mod, entry = owned_fixture(tmp_path / "prune rollback input fixture")
    m, draw = workspace.manager, panels(workspace)
    m.library, m.profile_id = library, profile
    m.refresh()
    workspace._tab = "Recovery"
    native.warm(draw)
    native.click(native.find("Review permanent backup deletion"), draw)
    finish_job(workspace, native, draw)
    started, release = threading.Event(), threading.Event()
    def stop(name):
        if name == "moved":
            if outcome == "failed":
                raise OSError("disposable recovery deletion failure")
            started.set()
            assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.history_prune._checkpoint", stop)
    native.click(native.find("Permanently delete this reviewed recovery backup"), draw)
    native.click(native.find("Delete reviewed recovery backup"), draw)
    if outcome == "cancelled":
        assert started.wait(10)
        native.warm(draw)
        native.click(native.find("Cancel operation"), draw)
        assert m._cancel.is_set()
        release.set()
    m._future.exception(timeout=10)
    m.poll()
    native.warm(draw)
    assert m.job_state == outcome and m.prune_review is not None
    assert (Path(entry["path"]) / "file.txt").read_text() == "original"
    assert (library.root / "mods" / mod / "file.txt").read_text() == "replacement"
    assert len(library.history()) == 1


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_fo4_plugin_tables_and_two_comparisons_use_actual_input(native, workspace, tmp_path, monkeypatch, size):
    import json
    from creation_lib.mod_manager.instances import create
    from .test_save_inspection import fixture
    from .test_standalone import game_fixture
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    game = game_fixture(tmp_path)
    for name in ("Fallout4.esm", "New.esp"):
        (game / "Data" / name).write_bytes(b"generated header fixture")
    m.library = Library(tmp_path / "plugin comparison library", create=True)
    m.profile_id = create(m.library, "fo4", game)
    with m.library._db() as db:
        files = m.library.profiles()[0].files
        files["plugins.txt"] = "*New.esp\n"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), m.profile_id))
    m.refresh()
    root = m.library.root / "runtime" / "profiles" / m.profile_id / "saves"
    root.mkdir(parents=True)
    left = fixture(root / "Left.fos", regular=("Fallout4.esm", "Old.esp"))
    right = fixture(root / "Right.fos", regular=("Fallout4.esm", "New.esp"), light=())
    before = {item.name: item.read_bytes() for item in root.iterdir()}
    monkeypatch.setattr("creation_lib.mod_manager.plugin_state.modkit_metadata", lambda path, game_id, cancel: ({"flags": {}}, ()))
    workspace._right_pane = "Saves"
    native.warm(draw)
    native.click(native.find("Refresh owned saves"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Left.fos###owned_save"), draw)
    native.click(native.find("Inspect saved plugins"), draw)
    finish_job(workspace, native, draw)
    assert m.save_inspections[left.path].light == ("Small.esl",)
    native.find("Compare plugins")
    native.find("Close")
    native.click(native.find("Compare plugins"), draw)
    finish_job(workspace, native, draw)
    assert m.save_comparisons[left.path].missing == ("Old.esp", "Small.esl")
    assert m.save_comparisons[left.path].added == ("New.esp",)
    native.choose_next("##save_compare_target", draw)
    native.click(native.find("Compare plugins"), draw)
    finish_job(workspace, native, draw)
    assert m.save_comparisons[left.path].target == Path(right.path).name
    native.click(native.find("Close"), draw)
    assert workspace._tab == "" and before == {item.name: item.read_bytes() for item in root.iterdir()}


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_plugin_inspection_failure_or_cancel_preserves_previous_read_actual_input(native, workspace, tmp_path, size, outcome):
    from dataclasses import replace
    from creation_lib.mod_manager.extensions import Registry
    from creation_lib.mod_manager.instances import create
    from .test_save_inspection import fixture
    from .test_standalone import game_fixture
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    m.library = Library(tmp_path / "read failure library", create=True)
    m.profile_id = create(m.library, "fo4", game_fixture(tmp_path))
    m.refresh()
    root = m.library.root / "runtime" / "profiles" / m.profile_id / "saves"
    root.mkdir(parents=True)
    source = fixture(root / "Generated.fos")
    m.inspect_saves()
    finish_job(workspace, native, draw)
    m.inspect_save_plugins(source.path)
    finish_job(workspace, native, draw)
    previous, before = m.save_inspections[source.path], Path(source.path).read_bytes()
    started, release = threading.Event(), threading.Event()
    original = m.extensions.adapter("save", "fo4")
    def callback(save, cancel):
        if outcome == "failed":
            raise ValueError("generated adapter failure")
        started.set()
        assert release.wait(10)
        return original.callback(save, cancel)
    registry = Registry()
    registry.register(replace(original, callback=callback))
    m.extensions = registry
    workspace._save_inspect_path, workspace._tab = source.path, "Save plugins"
    native.warm(draw)
    native.click(native.find("Inspect again"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
            assert m._cancel.is_set()
            release.set()
        if m._future is not None:
            m._future.exception(timeout=10)
        m.poll()
        native.warm(draw)
        assert m.job_state == outcome and m.save_inspections[source.path] is previous
        assert Path(source.path).read_bytes() == before
        native.find("Close")
    finally:
        release.set()


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_combined_data_archive_provenance_and_unresolved_override_actual_input(native, workspace, tmp_path, size):
    import json
    from creation_lib.mod_manager.instances import create
    from .test_archive_index import ba2_fixture
    from .test_standalone import game_fixture
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    game = game_fixture(tmp_path)
    ba2_fixture(game / "Data" / "Base.ba2", ["textures/shared.txt", "meshes/archive-only.txt"])
    (game / "Data" / "textures").mkdir()
    (game / "Data" / "textures" / "shared.txt").write_bytes(b"generated loose")
    m.library = Library(tmp_path / "combined input library", create=True)
    m.profile_id = create(m.library, "fo4", game)
    with m.library._db() as db:
        files = m.library.profiles()[0].files
        files["Fallout4.ini"] = "[Archive]\nsResourceArchiveList=Base.ba2\n"
        files["Fallout4Custom.ini"] = "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), m.profile_id))
    m.refresh()
    workspace._right_pane = "Data"
    native.warm(draw)
    native.click(native.find("Read loose + archives"), draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##data_filter"), draw, "shared")
    native.click(native.find("shared.txt###data_file"), draw)
    assert next(entry for entry in m.combined_index.files if entry.path == "textures/shared.txt").winner.kind == "loose"
    native.click(native.find("Show archive"), draw)
    native.warm(draw)
    assert workspace._right_pane == "Archives"
    assert native.find("textures/shared.txt | 1 providers###archive_member")
    workspace._right_pane = "Data"
    native.warm(draw)
    with m.library._db() as db:
        files["Fallout4Custom.ini"] = "[Archive]\nbInvalidateOlderFiles=0\n"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), m.profile_id))
    native.click(native.find("Read loose + archives"), draw)
    finish_job(workspace, native, draw)
    entry = next(entry for entry in m.combined_index.files if entry.path == "textures/shared.txt")
    assert entry.winner is None and "unresolved" in entry.reason


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_local_reference_remove_cancel_repeat_and_undo_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(next(item for item in native.items if item.label == "##select_mod"), draw)
    identifier, before = m.selected_id, m.library.rows(m.profile_id)
    root = Path(m.rows_by_id[identifier].source)
    sources = {str(path): path.read_bytes() for path in root.rglob("*") if path.is_file()}
    native.click(native.find("Actions"), draw)
    native.click(native.find("Remove selected reference / separator"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Cancel reference removal"), draw)
    assert not m.reference_review and m.library.rows(m.profile_id) == before
    native.click(native.find("Actions"), draw)
    native.click(native.find("Remove selected reference / separator"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Remove these reviewed profile entries; retain source content"), draw)
    native.click(native.find("Remove reviewed references"), draw)
    finish_job(workspace, native, draw)
    assert identifier not in m.rows_by_id and not workspace._dialog
    m.library.import_mo2(m.import_preview, reconcile=True)
    m.refresh()
    native.warm(draw)
    assert identifier not in m.rows_by_id
    native.click(native.find("Undo"), draw)
    finish_job(workspace, native, draw)
    assert m.library.rows(m.profile_id) == before
    assert {str(path): path.read_bytes() for path in root.rglob("*") if path.is_file()} == sources


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_local_reference_removal_cancel_or_failure_uses_actual_input(native, workspace, monkeypatch, size, outcome):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(next(item for item in native.items if item.label == "##select_mod"), draw)
    before = m.library.rows(m.profile_id)
    native.click(native.find("Actions"), draw)
    native.click(native.find("Remove selected reference / separator"), draw)
    finish_job(workspace, native, draw)
    started, release = threading.Event(), threading.Event()
    def checkpoint():
        if outcome == "failed":
            raise OSError("generated reference removal failure")
        started.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.reference_removal._checkpoint", checkpoint)
    native.click(native.find("Remove these reviewed profile entries; retain source content"), draw)
    native.click(native.find("Remove reviewed references"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
            assert m._cancel.is_set()
            release.set()
        if m._future is not None:
            m._future.exception(timeout=10)
        m.poll()
        native.warm(draw)
        assert m.job_state == outcome and m.reference_review is not None
        assert m.library.rows(m.profile_id) == before
        native.find("Cancel reference removal")
    finally:
        release.set()


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_save_screenshot_preview_hide_uses_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from imgui_bundle import immvision
    from creation_lib.mod_manager.instances import create
    from .test_save_inspection import fixture
    from .test_standalone import game_fixture
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    m.library = Library(tmp_path / "preview library", create=True)
    m.profile_id = create(m.library, "fo4", game_fixture(tmp_path))
    m.refresh()
    root = m.library.root / "runtime" / "profiles" / m.profile_id / "saves"
    root.mkdir(parents=True)
    source = fixture(root / "colors.fos", screenshot=(2, 1, bytes((255, 0, 0, 255, 0, 0, 255, 255))))
    before = Path(source.path).read_bytes()
    m.inspect_saves()
    finish_job(workspace, native, draw)
    m.inspect_save_plugins(source.path)
    finish_job(workspace, native, draw)
    workspace._save_inspect_path, workspace._tab = source.path, "Save plugins"
    displayed = []
    monkeypatch.setattr(immvision, "image_display", lambda label, image, size, **kwargs: displayed.append((label, image.copy(), size)))
    native.warm(draw)
    native.click(native.find("Preview screenshot"), draw)
    finish_job(workspace, native, draw)
    assert displayed and displayed[-1][0] == "##save_screenshot" and displayed[-1][1][0, 0].tolist() == [255, 0, 0]
    native.find("Compare plugins")
    native.find("Close")
    native.click(native.find("Hide screenshot"), draw)
    assert not m.save_images and Path(source.path).read_bytes() == before


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_save_screenshot_failure_cancel_preserves_previous_image_actual_input(native, workspace, tmp_path, monkeypatch, size, outcome):
    from imgui_bundle import immvision
    from creation_lib.mod_manager.instances import create
    from .test_save_inspection import fixture
    from .test_standalone import game_fixture
    from creation_lib.mod_manager.save_inspection import screenshot
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    m.library = Library(tmp_path / "preview failure library", create=True)
    m.profile_id = create(m.library, "fo4", game_fixture(tmp_path))
    m.refresh()
    root = m.library.root / "runtime" / "profiles" / m.profile_id / "saves"
    root.mkdir(parents=True)
    source = fixture(root / "generated.fos")
    m.inspect_saves()
    finish_job(workspace, native, draw)
    m.inspect_save_plugins(source.path)
    finish_job(workspace, native, draw)
    m.preview_save_screenshot(source.path)
    m._future.result(timeout=10)
    m.poll()
    previous, before = m.save_images[source.path], Path(source.path).read_bytes()
    monkeypatch.setattr(immvision, "image_display", lambda *args, **kwargs: None)
    started, release = threading.Event(), threading.Event()
    def blocked(saved, cancel):
        if outcome == "failed":
            raise ValueError("generated screenshot failure")
        started.set()
        assert release.wait(10)
        return screenshot(saved, cancel)
    monkeypatch.setattr("creation_lib.mod_manager.save_inspection.screenshot", blocked)
    workspace._save_inspect_path, workspace._tab = source.path, "Save plugins"
    native.warm(draw)
    native.click(native.find("Preview screenshot"), draw)
    try:
        if outcome == "cancelled":
            assert started.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
            assert m._cancel.is_set()
            release.set()
        if m._future is not None:
            m._future.exception(timeout=10)
        m.poll()
        native.warm(draw)
        assert m.job_state == outcome and m.save_images[source.path] is previous
        assert Path(source.path).read_bytes() == before
        native.find("Hide screenshot")
    finally:
        release.set()


def prepare_plugin_drag(native, workspace, tmp_path, monkeypatch):
    from .test_standalone import game_fixture
    from creation_lib.mod_manager.instances import create
    m, draw = workspace.manager, panels(workspace)
    game = game_fixture(tmp_path)
    for name in ("Fallout4.esm", "Alpha.esp", "Beta.esp", "Gamma.esp", "Hidden.esp", "Omega.esp"):
        (game / "Data" / name).write_bytes(b"generated drag plugin fixture")
    m.library = Library(tmp_path / "plugin drag library", create=True)
    m.profile_id = create(m.library, "fo4", game)
    m.refresh()
    monkeypatch.setattr("creation_lib.mod_manager.plugin_state.modkit_metadata", lambda path, game_id, cancel: ({"flags": {"esm": path.suffix == ".esm"}}, () if path.name == "Fallout4.esm" else ("Fallout4.esm",)))
    workspace._right_pane = "Plugins"
    native.warm(draw)
    native.click(native.find("Refresh plugins"), draw)
    finish_job(workspace, native, draw)
    return m, draw, game


def plugin_item(native, panes, name):
    by_name = {p.name: p for p in panes.catalog.plugins}
    visible = [n for n in panes.order if not panes.query or panes.query.casefold() in (n + " " + by_name[n].provider).casefold()]
    return [item for item in native.items if item.label == "##plugin_selection"][visible.index(name)]


def drag_plugin(native, draw, source, target, *, held=None):
    def center(item):
        x1, y1, x2, y2 = item.rect
        return ((x1 + x2) / 2, (y1 + y2) / 2)
    native.frame(draw, mouse=center(source), down=True)
    native.frame(draw, mouse=center(target), down=True)
    native.frame(draw, mouse=center(target), down=True)
    if held:
        held()
    native.frame(draw, mouse=center(target), down=False)
    native.warm(draw)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_plugin_selected_block_drag_filtered_review_cancel_actual_input(native, workspace, tmp_path, monkeypatch, size):
    native.io.display_size = size
    m, draw, game = prepare_plugin_drag(native, workspace, tmp_path, monkeypatch)
    panes = workspace.panes
    source_files = {path.name: path.read_bytes() for path in (game / "Data").iterdir() if path.is_file()}
    original = m.library.profile_snapshot(m.profile_id)[0].files
    native.click(plugin_item(native, panes, "Alpha.esp"), draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, True)
    native.click(plugin_item(native, panes, "Gamma.esp"), draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, False)
    native.frame(draw)
    assert panes.selected_plugins == {"Alpha.esp", "Gamma.esp"}
    native.text(native.find("##plugin_filter"), draw, "a")
    drag_plugin(native, draw, plugin_item(native, panes, "Gamma.esp"), plugin_item(native, panes, "Omega.esp"))
    assert panes.order == ["Fallout4.esm", "Beta.esp", "Hidden.esp", "Alpha.esp", "Gamma.esp", "Omega.esp"]
    assert panes.selected_plugins == {"Alpha.esp", "Gamma.esp"}
    drag_plugin(native, draw, plugin_item(native, panes, "Alpha.esp"), plugin_item(native, panes, "Beta.esp"))
    assert panes.order == ["Fallout4.esm", "Alpha.esp", "Gamma.esp", "Beta.esp", "Hidden.esp", "Omega.esp"]
    assert m.library.profile_snapshot(m.profile_id)[0].files == original
    native.click(native.find("Revert"), draw)
    assert panes.order == ["Fallout4.esm", "Alpha.esp", "Beta.esp", "Gamma.esp", "Hidden.esp", "Omega.esp"]
    assert m.library.profile_snapshot(m.profile_id)[0].files == original
    assert {path.name: path.read_bytes() for path in (game / "Data").iterdir() if path.is_file()} == source_files


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_plugin_drag_core_lock_and_outside_release_guards_actual_input(native, workspace, tmp_path, monkeypatch, size):
    native.io.display_size = size
    m, draw, _ = prepare_plugin_drag(native, workspace, tmp_path, monkeypatch)
    panes = workspace.panes
    original = list(panes.order)
    drag_plugin(native, draw, plugin_item(native, panes, "Alpha.esp"), plugin_item(native, panes, "Fallout4.esm"))
    assert panes.order == original and "masters must remain first" in m.error
    m.error = ""
    native.warm(draw)
    native.click(plugin_item(native, panes, "Beta.esp"), draw)
    native.click(native.find("Lock priority"), draw)
    drag_plugin(native, draw, plugin_item(native, panes, "Alpha.esp"), plugin_item(native, panes, "Omega.esp"))
    assert panes.order.index("Beta.esp") == original.index("Beta.esp") and not m.error
    assert panes.order != original
    original = list(panes.order)
    source = plugin_item(native, panes, "Alpha.esp")
    x1, y1, x2, y2 = source.rect
    native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2), down=True)
    native.frame(draw, mouse=(0, 0), down=True)
    native.frame(draw, mouse=(0, 0), down=False)
    native.warm(draw)
    assert panes.order == original and panes._drag is None


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_plugin_drag_rejects_mid_gesture_draft_change_actual_input(native, workspace, tmp_path, monkeypatch, size):
    native.io.display_size = size
    m, draw, _ = prepare_plugin_drag(native, workspace, tmp_path, monkeypatch)
    panes = workspace.panes
    original = list(panes.order)
    drag_plugin(native, draw, plugin_item(native, panes, "Alpha.esp"), plugin_item(native, panes, "Omega.esp"),
                held=lambda: panes.active.add("beta.esp"))
    assert panes.order == original and "draft changed during this drag" in m.error


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("kind", ["save", "overwrite"])
@pytest.mark.parametrize("outcome", ["success", "cancelled", "failed"])
def test_retained_backup_review_decline_delete_cancel_failure_actual_input(native, workspace, tmp_path, monkeypatch, size, kind, outcome):
    from .test_backup_prune import fixture, files
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    library, profile, source, first, second = fixture(tmp_path, kind)
    m.library, m.profile_id = library, profile
    m.refresh()
    source_before, first_before, second_before = files(source), files(first), files(second)
    native.warm(draw)
    if kind == "save":
        workspace._right_pane = "Saves"
        native.warm(draw)
        native.click(native.find("Save locations"), draw)
        native.click(native.find("Save backups"), draw)
        native.click(native.find("Refresh save backups"), draw)
        finish_job(workspace, native, draw)
        selected = next(item for item in native.items if item.label.endswith("###save_backup") and first.name[:8] in item.label)
        native.click(selected, draw)
        label = "Review save backup deletion"
    else:
        native.click(native.find("Actions"), draw)
        native.click(native.find("Owned overwrite"), draw)
        native.click(native.find("Review owned output"), draw)
        finish_job(workspace, native, draw)
        # Rows use stable IDs. Selecting each displayed row through input locates
        # the requested fixture without depending on its timestamp ordering.
        for item in [item for item in native.items if item.label.endswith("###overwrite_backup")]:
            native.click(item, draw)
            if workspace._overwrite_selected == first.name:
                break
        assert workspace._overwrite_selected == first.name
        label = "Review output backup deletion"
    native.click(native.find(label), draw)
    finish_job(workspace, native, draw)
    assert workspace._dialog == "Backup deletion" and m.retained_review.backup_id == first.name
    remove = native.find("Delete reviewed retained backup")
    native.click(remove, draw)  # Acknowledgement is still absent.
    assert not m.busy and first.is_dir()
    native.click(native.find("Cancel retained backup deletion"), draw)
    assert m.retained_review is None and first.is_dir()
    native.click(native.find(label), draw)
    finish_job(workspace, native, draw)
    entered, release = threading.Event(), threading.Event()
    def checkpoint(stage):
        if stage == "moved":
            if outcome == "failed":
                raise OSError("disposable failed retained-bundle deletion")
            if outcome == "cancelled":
                entered.set()
                assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.backup_prune._checkpoint", checkpoint)
    native.click(native.find("Permanently delete this reviewed retained backup"), draw)
    native.click(native.find("Delete reviewed retained backup"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if outcome == "success":
        finish_job(workspace, native, draw)
        assert not first.exists() and m.retained_review is None
    else:
        m._future.exception(timeout=10)
        m.poll()
        native.warm(draw)
        assert m.job_state == outcome and files(first) == first_before
        assert m.retained_review.backup_id == first.name
    assert files(source) == source_before and files(second) == second_before


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["success", "cancelled", "failed"])
def test_default_global_saves_read_cancel_failure_preserve_previous_inventory_actual_input(native, workspace, tmp_path, monkeypatch, size, outcome):
    from .test_global_saves import fixture
    from creation_lib.mod_manager import global_saves
    from creation_lib.mod_manager.saves import inventory
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    library, profile, _, source, seen = fixture(tmp_path, monkeypatch)
    m.library, m.profile_id = library, profile
    m.refresh()
    previous = inventory(library, profile)
    m.save_index = previous
    original = {path.name: path.read_bytes() for path in source.iterdir()}
    workspace._right_pane = "Saves"
    native.warm(draw)
    assert not seen  # Merely opening Saves does not resolve Windows folders.
    entered, release = threading.Event(), threading.Event()
    original_inventory = global_saves.inventory
    def read(*args, **kwargs):
        if outcome == "failed":
            raise ManagerError("disposable global discovery failure")
        if outcome == "cancelled":
            entered.set()
            assert release.wait(10)
        return original_inventory(*args, **kwargs)
    from creation_lib.mod_manager.paths import ManagerError
    monkeypatch.setattr(global_saves, "inventory", read)
    native.click(native.find("Save locations"), draw)
    native.click(native.find("Default global FO4 saves"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if outcome == "success":
        finish_job(workspace, native, draw)
        assert m.save_index.external and m.save_index.root == str(source)
        native.click(native.find("Global.fos###owned_save"), draw)
        native.click(native.find("Inspect save header"), draw)
        finish_job(workspace, native, draw)
        assert m.save_details[str(source / "Global.fos")]["player"] == "Fixture Character"
        assert seen == [global_saves.DOCUMENTS]
    else:
        if m._future is not None:
            m._future.exception(timeout=10)
            m.poll()
        native.warm(draw)
        assert m.job_state == outcome and m.save_index is previous
    assert {path.name: path.read_bytes() for path in source.iterdir()} == original


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_real_registry_menu_selects_new_table_free_manager_actual_input(native, tmp_path, monkeypatch, size):
    from ui.toolkit.workspaces import create_workspaces
    from ui.toolkit.variants import get_variant
    from imgui_bundle import hello_imgui
    monkeypatch.setattr(hello_imgui, "get_runner_params", lambda: SimpleNamespace(fps_idling=SimpleNamespace(fps_idle=60)))
    native.io.display_size = size
    old, manager_workspace = create_workspaces(workspace_ids=("mod_builder", "mod_manager"))
    settings = SimpleNamespace(active_workspace="mod_builder", save=lambda: None)
    host = SimpleNamespace(_ws_map={old.id: old, manager_workspace.id: manager_workspace},
                           _active_ws=old, _settings=settings, _ensure_initialized=lambda _: True,
                           _app_variant=get_variant("full"))
    host._switch_workspace = lambda key: ToolkitApp._switch_workspace(host, key)
    host._ws_menu_items = lambda keys: ToolkitApp._ws_menu_items(host, keys)
    def draw_menu():
        imgui.set_next_window_pos((0, 0))
        imgui.set_next_window_size((500, 250))
        imgui.begin("Registered workspaces", flags=imgui.WindowFlags_.no_saved_settings | imgui.WindowFlags_.menu_bar)
        if imgui.begin_menu_bar():
            ToolkitApp._show_menus(host)
            imgui.end_menu_bar()
        imgui.end()
    try:
        native.warm(draw_menu)
        native.click(native.find("Workspace"), draw_menu)
        assert native.find("Mod Builder###workspace")
        native.click(native.find("Modlist Manager###workspace"), draw_menu)
        assert host._active_ws is manager_workspace and settings.active_workspace == "mod_manager"
        assert manager_workspace.active and not old.active
        manager_workspace.manager.library = Library(tmp_path / "routed library", create=True)
        manager_workspace.manager.library.import_mo2(inspect_instance(make_mo2(tmp_path)))
        manager_workspace.manager.refresh()
        draw = panels(manager_workspace)
        monkeypatch.setattr(imgui, "begin_table", lambda *a, **k: pytest.fail("Routed modlist used a table"))
        native.warm(draw)
        assert [item for item in native.items if item.label == "##select_mod"]
        assert manager_workspace._incoming_package == ""
    finally:
        manager_workspace.cleanup()
        old.cleanup()


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_startup_archive_drop_review_cancel_and_corrupt_input_actual_ui(native, workspace, tmp_path, size):
    from zipfile import ZipFile
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    path = tmp_path / "Startup.zip"
    with ZipFile(path, "w") as package:
        package.writestr("textures/generated.txt", b"generated routing payload")
    before = {str(p): p.read_bytes() for p in path.parent.iterdir() if p.is_file()}
    old_rows = m.library.rows(m.profile_id)
    host = SimpleNamespace(_launch_open_done=False, _launch_path=str(path), _ws_map={workspace.id: workspace},
                           _active_ws=None, _switch_workspace=lambda *a, **k: True)
    host._resolve_launch_target = ToolkitApp._resolve_launch_target
    ToolkitApp._handle_launch_open(host)
    assert host._launch_open_done and host._launch_path is None and workspace._incoming_package == str(path)
    native.warm(draw)
    finish_job(workspace, native, draw)
    assert m.prepared and workspace._dialog == "Install"
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)
    assert m.install_plan is not None and m.library.rows(m.profile_id) == old_rows
    native.click(native.find("Close installer"), draw)
    assert m.prepared is None and m.library.rows(m.profile_id) == old_rows
    corrupt = tmp_path / "Corrupt.zip"
    corrupt.write_bytes(b"generated corrupt archive")
    assert ToolkitApp._handle_dropped_files(SimpleNamespace(_active_ws=workspace), [str(corrupt)])
    native.warm(draw)
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == "failed" and m.prepared is None and m.error
    assert m.library.rows(m.profile_id) == old_rows and corrupt.read_bytes() == b"generated corrupt archive"
    assert all(Path(p).read_bytes() == data for p, data in before.items())


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_mod_selected_block_group_drop_and_guarded_undo_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(visible_rows(native, m, workspace)["duplicate-0"], draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, True)
    native.click(visible_rows(native, m, workspace)["duplicate-1"], draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, False)
    native.frame(draw)
    assert workspace._selected_mods == {"duplicate-0", "duplicate-1"}
    before = m.library.rows(m.profile_id)
    groups = [item for node, item in zip(workspace._visible_tree, [i for i in native.items if i.label in {"##select_mod", "##group_node"}]) if node.key == "group:Visual"]
    assert len(groups) == 1
    drag_plugin(native, draw, visible_rows(native, m, workspace)["duplicate-0"], groups[0])
    finish_job(workspace, native, draw)
    assert all(m.rows_by_id[key].group == "Visual" for key in ("duplicate-0", "duplicate-1"))
    assert workspace._selected_mods == {"duplicate-0", "duplicate-1"}
    chord(native, draw, imgui.Key.z)
    finish_job(workspace, native, draw)
    assert m.library.rows(m.profile_id) == before


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_mod_drag_rejects_db_change_during_gesture_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    source = visible_rows(native, m, workspace)["duplicate-0"]
    target = visible_rows(native, m, workspace)["duplicate-2"]
    def later():
        with m.library._db() as db:
            db.execute("UPDATE entries SET group_name='Later' WHERE profile_id=? AND mod_id='duplicate-0'", (m.profile_id,))
    drag_plugin(native, draw, source, target, held=later)
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == "failed" and "changed during" in m.error
    rows = m.library.rows(m.profile_id)
    assert next(row for row in rows if row.id == "duplicate-0").group == "Later"
    assert [row.id for row in rows] == [row.id for row in m.rows]


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("field,value", [("version", "2.4-local"), ("notes", "local note"), ("category", "Visual/Custom"), ("nexus", "789"), ("priority", "0")])
def test_hovered_field_f2_review_cancel_save_actual_input(native, workspace, size, field, value):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    workspace._metadata_fields = {field}
    with m.library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=456 WHERE id='duplicate-1'")
    m.refresh()
    native.warm(draw)
    native.click(visible_rows(native, m, workspace)["duplicate-0"], draw)
    before = m.rows_by_id["duplicate-1"]
    def edit():
        workspace._record_field_rects = True
        native.warm(draw)
        first, last = workspace._field_rects[("duplicate-1", field)]
        x1, y1 = first
        x2, y2 = last
        workspace._record_field_rects = False  # Exercise the production hover path.
        native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2))
        assert not workspace._field_rects
        assert workspace._hovered_field == ("duplicate-1", field)
        native.key(imgui.Key.f2, draw)
        finish_job(workspace, native, draw)
        assert workspace._dialog == "Edit mod field" and m.field_review.mod_id == "duplicate-1"
    edit()
    native.click(native.find("Cancel field edit"), draw)
    assert m.field_review is None and m.rows_by_id["duplicate-1"] == before
    edit()
    native.text(native.find("##mod_field_value"), draw, value)
    native.click(native.find("Save local field"), draw)
    finish_job(workspace, native, draw)
    assert workspace._dialog == ""
    row = m.rows_by_id["duplicate-1"]
    if field == "category":
        assert row.categories == (value,)
    elif field == "nexus":
        assert row.nexus_mod_id == int(value) and row.nexus_file_id == 0
    else:
        assert str(getattr(row, field)).strip() == value


@pytest.mark.parametrize("outcome", ["cancelled", "failed", "stale"])
def test_hovered_field_write_cancel_failure_stale_actual_input(native, workspace, monkeypatch, outcome):
    m, draw = workspace.manager, panels(workspace)
    workspace._metadata_fields = {"version"}
    native.warm(draw)
    native.click(visible_rows(native, m, workspace)["duplicate-0"], draw)
    first, last = workspace._field_rects[("duplicate-0", "version")]
    native.frame(draw, mouse=((first[0] + last[0]) / 2, (first[1] + last[1]) / 2))
    native.key(imgui.Key.f2, draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##mod_field_value"), draw, "unexpected")
    before = m.rows_by_id["duplicate-0"]
    entered, release = threading.Event(), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("generated field write failure")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.row_edit._checkpoint", stop)
    if outcome == "stale":
        with m.library._db() as db:
            db.execute("UPDATE mods SET version='later' WHERE id='duplicate-0'")
    native.click(native.find("Save local field"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    row = next(row for row in m.library.rows(m.profile_id) if row.id == before.id)
    assert row.version == ("later" if outcome == "stale" else before.version)
    assert m.job_state == ("cancelled" if outcome == "cancelled" else "failed")


def group_item(native, workspace, path):
    items = [item for item in native.items if item.label in {"##select_mod", "##group_node"}]
    return next(item for node, item in zip(workspace._visible_tree, items, strict=True) if node.key == "group:" + path)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("path,context", [("Visual", False), ("Visual/Textures", True)])
def test_local_group_f2_and_virtual_context_review_cancel_rename_undo_actual_input(native, workspace, size, path, context):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    before = m.library.rows(m.profile_id)
    other = m.library.profiles()[1].id
    other_rows = m.library.rows(other)
    def edit():
        item = group_item(native, workspace, path)
        native.click(item, draw)
        x1, y1, x2, y2 = item.rect
        if context:
            native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2))
            native.io.add_mouse_button_event(1, True)
            native.frame(draw)
            native.io.add_mouse_button_event(1, False)
            native.frame(draw)
            native.warm(draw)
            native.click(native.find("Rename local group"), draw)
        else:
            native.key(imgui.Key.f2, draw)
        finish_job(workspace, native, draw)
        assert workspace._dialog == "Rename group" and m.group_review.original == path
    edit()
    native.click(native.find("Cancel group rename"), draw)
    assert m.library.rows(m.profile_id) == before and m.group_review is None
    edit()
    native.text(native.find("##local_group_path"), draw, "Custom/Renamed")
    native.click(native.find("Rename reviewed group"), draw)
    finish_job(workspace, native, draw)
    assert workspace._dialog == "" and any(row.group.startswith("Custom/Renamed") for row in m.rows)
    assert m.library.rows(other) == other_rows
    native.click(visible_rows(native, m, workspace)["duplicate-0"], draw)
    chord(native, draw, imgui.Key.z)
    finish_job(workspace, native, draw)
    assert m.library.rows(m.profile_id) == before


@pytest.mark.parametrize("outcome", ["cancelled", "failed", "stale"])
def test_local_group_rename_commit_cancel_failure_stale_actual_input(native, workspace, monkeypatch, outcome):
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    native.click(group_item(native, workspace, "Visual"), draw)
    native.key(imgui.Key.f2, draw)
    finish_job(workspace, native, draw)
    native.text(native.find("##local_group_path"), draw, "Changed")
    entered, release = threading.Event(), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("generated group rename failure")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.group_names._checkpoint", stop)
    if outcome == "stale":
        with m.library._db() as db:
            db.execute("UPDATE entries SET group_name='Later' WHERE profile_id=? AND mod_id='duplicate-0'", (m.profile_id,))
    before = m.library.rows(m.profile_id)
    native.click(native.find("Rename reviewed group"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.library.rows(m.profile_id) == before
    assert m.job_state == ("cancelled" if outcome == "cancelled" else "failed")


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_mod_context_information_title_and_source_removal_review_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    before = m.library.rows(m.profile_id)
    def context():
        item = visible_rows(native, m, workspace)["duplicate-0"]
        x1, y1, x2, y2 = item.rect
        native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2))
        native.io.add_mouse_button_event(1, True)
        native.frame(draw)
        native.io.add_mouse_button_event(1, False)
        native.frame(draw)
        native.warm(draw)
    context()
    native.click(native.find("Information", window="##Popup_"), draw)
    assert workspace._right_pane == "Information" and m.selected_id == "duplicate-0"
    context()
    native.click(native.find("Rename display title"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Cancel title edit"), draw)
    context()
    native.click(native.find("Remove references / separators from profile"), draw)
    finish_job(workspace, native, draw)
    assert m.reference_review.mod_ids == ("duplicate-0",)
    native.click(native.find("Cancel reference removal"), draw)
    assert m.library.rows(m.profile_id) == before


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["paused", "failed"])
def test_concurrent_download_slots_requests_pause_failure_resume_actual_input(native, workspace, size, outcome):
    from .test_download_scheduler import Wire, link
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    wire = Wire(fail=outcome == "failed")
    m.nexus_client = wire.client()
    try:
        native.warm(draw)
        native.click(native.find("Downloads"), draw)
        native.click(native.find("Simultaneous downloads"), draw)
        assert 2 <= m.download_slots <= 4
        assert workspace.collect_settings()["download_slots"] == m.download_slots
        m.accept_nxm(link(0))
        m.accept_nxm(link(1))
        def requests():
            workspace.draw()
            imgui.set_next_window_pos((0, 0))
            imgui.set_next_window_size(size)
            imgui.begin("Nexus request fixture", flags=imgui.WindowFlags_.no_saved_settings)
            workspace._draw_nexus()
            imgui.end()
        native.warm(requests)
        buttons = [item for item in native.items if item.label == "Download"]
        native.click(buttons[0], requests)
        buttons = [item for item in native.items if item.label == "Download"]
        native.click(buttons[1], requests)
        assert wire.entered[0].wait(5) and wire.entered[1].wait(5)
        assert m.downloads_active and not m.busy and wire.maximum == 2
        native.warm(requests)
        if outcome == "paused":
            native.click([item for item in native.items if item.label == "Pause request"][0], requests)
        wire.release.set()
        for transfer in list(m._download_scheduler.transfers.values()):
            transfer.future.exception(timeout=15)
        m.poll()
        native.warm(draw)
        assert not m.downloads_active and len(m.download_records) == 1
        assert sorted(job["state"] for job in m.download_jobs) == ["complete", outcome]
        if outcome == "failed":
            assert m.error and "private-fixture" not in m.error
        native.click(native.find("Resume queued download"), draw)
        for transfer in list(m._download_scheduler.transfers.values()):
            transfer.future.result(timeout=15)
        m.poll()
        native.warm(draw)
        assert len(m.download_records) == 2 and all(job["state"] == "complete" for job in m.download_jobs)
        assert all(Path(record["path"]).read_bytes() == wire.payload for record in m.download_records)
        assert not any(row.kind == "owned" for row in m.library.rows(m.profile_id))
    finally:
        wire.release.set()


def mod_context(native, workspace, draw, mod_id):
    item = visible_rows(native, workspace.manager, workspace)[mod_id]
    x1, y1, x2, y2 = item.rect
    native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2))
    native.io.add_mouse_button_event(1, True)
    native.frame(draw)
    native.io.add_mouse_button_event(1, False)
    native.frame(draw)
    native.warm(draw)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_create_empty_mod_below_context_and_reopen_actual_input(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    anchor = next(row for row in m.rows if row.name == "Base")
    mod_context(native, workspace, draw, anchor.id)
    native.click(native.find("Create empty mod below..."), draw)
    native.text(native.find("##empty_mod_name"), draw, "New loose files")
    native.click(native.find("Create mod"), draw)
    finish_job(workspace, native, draw)
    rows = m.library.rows(m.profile_id)
    created = next(row for row in rows if row.name == "New loose files")
    assert rows.index(created) == next(i for i, row in enumerate(rows) if row.id == anchor.id) + 1
    assert created.group == anchor.group and Path(created.source).is_dir()
    assert not list(Path(created.source).iterdir())
    assert workspace._dialog == "" and m.selected_id == created.id
    native.click(native.find("Add mod"), draw)
    native.click(native.find("Create empty mod..."), draw)
    native.text(native.find("##empty_mod_name"), draw, "Cancelled folder")
    native.click(native.find("Cancel"), draw)
    assert m.library.rows(m.profile_id) == rows


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_empty_library_space_context_menu(native, workspace, size):
    native.io.display_size = size
    draw = panels(workspace)
    native.warm(draw)
    native.text(native.find("##search_mods"), draw, "no matching fixture")
    header = native.find("Priority###sort_priority")
    native.frame(draw, mouse=((header.rect[0] + header.rect[2]) / 2, header.rect[3] + 55))
    native.io.add_mouse_button_event(1, True)
    native.frame(draw)
    native.io.add_mouse_button_event(1, False)
    native.frame(draw)
    native.warm(draw)
    native.click(native.find("Create empty mod..."), draw)
    assert workspace._dialog == "Create empty mod"
    assert workspace.metadata.empty_after == ""
    native.click(native.find("Cancel"), draw)
    assert workspace._dialog == ""


def source_relink_review(native, workspace, draw, tmp_path):
    import shutil
    m = workspace.manager
    row = next(row for row in m.rows if row.name == "Base")
    target = tmp_path / "Relocated source"
    if not target.exists():
        shutil.copytree(row.source, target)
    mod_context(native, workspace, draw, row.id)
    native.click(native.find("Relink source folder..."), draw)
    native.text(native.find("##reference_source_path"), draw, str(target))
    native.click(native.find("Review referenced source"), draw)
    finish_job(workspace, native, draw)
    assert m.relink_review.source == str(target)
    return row, target


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_referenced_source_context_review_decline_apply_undo_actual_input(native, workspace, tmp_path, size):
    from .test_reference_sources import files
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    before = m.library.rows(m.profile_id)
    row, target = source_relink_review(native, workspace, draw, tmp_path)
    originals, replacement = files(Path(row.source)), files(target)
    native.click(native.find("Apply reviewed source relink"), draw)
    assert not m.busy and m.library.rows(m.profile_id) == before
    native.click(native.find("Cancel source relink"), draw)
    assert m.relink_review is None and m.library.rows(m.profile_id) == before
    source_relink_review(native, workspace, draw, tmp_path)
    native.click(native.find("This folder is the same referenced mod"), draw)
    native.click(native.find("Apply reviewed source relink"), draw)
    finish_job(workspace, native, draw)
    assert workspace._dialog == "" and m.rows_by_id[row.id].source == str(target)
    assert all(next(item for item in m.library.rows(profile.id) if item.id == row.id).source == str(target) for profile in m.profiles)
    mod_context(native, workspace, draw, row.id)
    native.click(native.find("Undo source relink"), draw)
    finish_job(workspace, native, draw)
    assert m.library.rows(m.profile_id) == before and not m.reference_undo
    assert files(Path(row.source)) == originals and files(target) == replacement


def metadata_refresh_review(native, workspace, draw, tmp_path):
    from .test_source_metadata import METADATA
    m = workspace.manager
    row = next(item for item in m.rows if Path(item.source).name == "Base")
    metadata = Path(row.source) / "meta.ini"
    metadata.write_text(METADATA, encoding="utf-8")
    mapping = tmp_path / "categories.dat"
    mapping.write_text("1|Visual/Textures|0|0\n2|Gameplay/Fixture|0|0\n", encoding="utf-8")
    mod_context(native, workspace, draw, row.id)
    native.click(native.find("Refresh source metadata..."), draw)
    finish_job(workspace, native, draw)
    assert workspace._dialog == "Refresh source metadata" and not workspace.source_metadata.selected
    native.text(native.find("##source_category_mapping"), draw, str(mapping))
    native.click(native.find("Review source metadata"), draw)
    finish_job(workspace, native, draw)
    assert not any(field.issue for field in m.source_metadata_review.fields)
    return row, metadata


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_selective_source_metadata_context_review_decline_scroll_apply_undo_actual_input(native, workspace, tmp_path, size):
    from .test_reference_sources import files
    m, draw = workspace.manager, panels(workspace)
    native.io.display_size = size
    native.warm(draw)
    before = m.library.rows(m.profile_id)
    row, _ = metadata_refresh_review(native, workspace, draw, tmp_path)
    original = files(Path(row.source))
    native.click(native.find("Apply selected source fields"), draw)
    assert not m.busy and m.library.rows(m.profile_id) == before
    native.click(native.find("Version###source_field"), draw)
    native.click(native.find("Cancel metadata refresh"), draw)
    assert m.source_metadata_review is None and m.library.rows(m.profile_id) == before
    metadata_refresh_review(native, workspace, draw, tmp_path)
    native.click(native.find("Version###source_field"), draw)
    native.scroll(native.find("Version###source_field"), draw, -10000)
    native.click(native.find("Categories###source_field"), draw)
    native.click(native.find("Nexus association###source_field"), draw)
    assert workspace.source_metadata.selected == {"version", "categories", "nexus"}
    footer = native.find("Apply selected source fields")
    assert 0 <= footer.rect[1] < footer.rect[3] <= size[1]
    native.click(footer, draw)
    finish_job(workspace, native, draw)
    assert workspace._dialog == "" and row.id in m.source_metadata_undo
    for profile in m.profiles:
        current = next(item for item in m.library.rows(profile.id) if item.id == row.id)
        assert current.version == "3.4" and current.notes == row.notes and current.name == row.name
        assert current.categories == ("Visual/Textures", "Gameplay/Fixture")
        assert (current.nexus_game, current.nexus_mod_id, current.nexus_file_id) == ("fallout4", 987, 654)
    mod_context(native, workspace, draw, row.id)
    native.click(native.find("Undo metadata refresh"), draw)
    finish_job(workspace, native, draw)
    assert m.library.rows(m.profile_id) == before and not m.source_metadata_undo
    assert files(Path(row.source)) == original


def hover_plugin_priority_f2(native, workspace, draw, name):
    item = plugin_item(native, workspace.panes, name)
    native.click(item, draw)
    priority = native.find("##plugin_priority_" + name.casefold())
    x1, y1, x2, y2 = priority.rect
    native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2))
    native.key(imgui.Key.f2, draw)
    native.warm(draw)


def type_slider(native, draw, label, value):
    native.io.add_key_event(imgui.Key.mod_ctrl, True)
    native.click(native.find(label), draw)
    native.key(imgui.Key.a, draw)
    native.io.add_key_event(imgui.Key.mod_ctrl, False)
    native.io.add_input_characters_utf8(str(value))
    native.frame(draw)
    native.key(imgui.Key.enter, draw)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_hover_plugin_priority_f2_cancel_filtered_draft_review_apply_actual_input(native, workspace, tmp_path, monkeypatch, size):
    native.io.display_size = size
    m, draw, game = prepare_plugin_drag(native, workspace, tmp_path, monkeypatch)
    panes = workspace.panes
    before, original = list(panes.order), m.library.profile_snapshot(m.profile_id)[0].files
    payloads = {path.name: path.read_bytes() for path in (game / "Data").iterdir() if path.is_file()}
    hover_plugin_priority_f2(native, workspace, draw, "Omega.esp")
    assert workspace._dialog == "Edit plugin priority" and workspace.plugin_priority.plan.name == "Omega.esp"
    type_slider(native, draw, "##plugin_priority_value", 1)
    native.click(native.find("Cancel plugin priority edit"), draw)
    assert panes.order == before and m.library.profile_snapshot(m.profile_id)[0].files == original
    native.text(native.find("##plugin_filter"), draw, "Omega")
    hover_plugin_priority_f2(native, workspace, draw, "Omega.esp")
    type_slider(native, draw, "##plugin_priority_value", 1)
    assert workspace.plugin_priority.value == 1
    native.click(native.find("Use reviewed draft priority"), draw)
    assert workspace._dialog == "" and panes.order[1] == "Omega.esp" and panes.plan is None
    assert m.library.profile_snapshot(m.profile_id)[0].files == original
    native.click(native.find("Save load order"), draw)
    finish_job(workspace, native, draw)
    files = m.library.profile_snapshot(m.profile_id)[0].files
    assert files["loadorder.txt"].splitlines()[1] == "Omega.esp"
    assert {path.name: path.read_bytes() for path in (game / "Data").iterdir() if path.is_file()} == payloads


@pytest.mark.parametrize("outcome", ["stale", "failed", "locked", "core"])
def test_plugin_priority_context_f2_stale_failure_lock_core_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    m, draw, _ = prepare_plugin_drag(native, workspace, tmp_path, monkeypatch)
    panes, original = workspace.panes, m.library.profile_snapshot(m.profile_id)[0].files
    name = "Fallout4.esm" if outcome == "core" else "Omega.esp"
    if outcome == "locked":
        panes.locked[name.casefold()] = (name, panes.order.index(name))
    before = list(panes.order)
    if outcome in {"locked", "core"}:
        hover_plugin_priority_f2(native, workspace, draw, name)
        assert workspace._dialog != "Edit plugin priority" and m.error
    else:
        item = plugin_item(native, panes, name)
        x1, y1, x2, y2 = item.rect
        native.frame(draw, mouse=((x1 + x2) / 2, (y1 + y2) / 2))
        native.io.add_mouse_button_event(1, True)
        native.frame(draw)
        native.io.add_mouse_button_event(1, False)
        native.frame(draw)
        native.warm(draw)
        native.click(native.find("Edit draft priority"), draw)
        type_slider(native, draw, "##plugin_priority_value", 1)
        if outcome == "stale":
            panes.active.add("omega.esp")
        else:
            def stop():
                raise OSError("generated plugin draft error")
            monkeypatch.setattr("creation_lib.mod_manager.plugin_priority._checkpoint", stop)
        native.click(native.find("Use reviewed draft priority"), draw)
        assert workspace._dialog == "Edit plugin priority" and m.error
    assert panes.order == before and m.library.profile_snapshot(m.profile_id)[0].files == original


@pytest.mark.parametrize("outcome", ["cancelled", "failed", "stale-input", "stale-local"])
def test_source_metadata_refresh_cancel_failure_stale_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    from .test_reference_sources import files
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    row, metadata = metadata_refresh_review(native, workspace, draw, tmp_path)
    entered, release = threading.Event(), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("generated metadata refresh failure")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.source_metadata._checkpoint", stop)
    if outcome == "stale-input":
        metadata.write_text("[General]\nversion=later\n", encoding="utf-8")
    if outcome == "stale-local":
        with m.library._db() as db:
            db.execute("UPDATE mods SET notes='later' WHERE id=?", (row.id,))
    before, original = m.library.rows(m.profile_id), files(Path(row.source))
    native.click(native.find("Version###source_field"), draw)
    native.click(native.find("Apply selected source fields"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == ("cancelled" if outcome == "cancelled" else "failed")
    assert m.library.rows(m.profile_id) == before and not m.source_metadata_undo
    assert files(Path(row.source)) == original


@pytest.mark.parametrize("outcome", ["cancelled", "failed", "stale-files", "stale-metadata"])
def test_source_relink_cancel_failure_stale_preserves_folders_and_metadata_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    from .test_reference_sources import files
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    row, target = source_relink_review(native, workspace, draw, tmp_path)
    entered, release = threading.Event(), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("generated source metadata failure")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.reference_sources._checkpoint", stop)
    if outcome == "stale-files":
        (target / "later.txt").write_bytes(b"later fixture content")
    if outcome == "stale-metadata":
        with m.library._db() as db:
            db.execute("UPDATE mods SET source='later' WHERE id=?", (row.id,))
    before, originals, replacement = m.library.rows(m.profile_id), files(Path(row.source)), files(target)
    native.click(native.find("This folder is the same referenced mod"), draw)
    native.click(native.find("Apply reviewed source relink"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == ("cancelled" if outcome == "cancelled" else "failed")
    assert m.library.rows(m.profile_id) == before and not m.reference_undo
    assert files(Path(row.source)) == originals and files(target) == replacement


def target_folder_fixture(tmp_path, monkeypatch, build=(1, 10, 163, 0), name="game"):
    from .test_pe_metadata import fixture_version, image
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", fixture_version)
    game = tmp_path / name
    (game / "Data").mkdir(parents=True)
    image(game / "Fallout4.exe", version=build)
    return game


def target_folder_review(native, workspace, draw, game, name="Target"):
    native.click(native.find("Launch options"), draw)
    native.click(native.find("FO4 folders"), draw)
    native.text(native.find("##target_name"), draw, name)
    native.text(native.find("##target_folder"), draw, str(game))
    native.click(native.find("Review FO4 folder"), draw)
    finish_job(workspace, native, draw)
    assert workspace.manager.target_review.target.game_root == str(game)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_folder_first_review_decline_apply_staging_switch_and_settings_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from .test_f4se_setup import package
    from creation_lib.mod_manager import launch_targets
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    game = target_folder_fixture(tmp_path, monkeypatch)
    before = m.library.rows(m.profile_id)
    archive = package(tmp_path)
    native.warm(draw)
    target_folder_review(native, workspace, draw, game)
    native.click(native.find("Cancel folder review"), draw)
    assert m.target_review is None and not m.targets
    native.click(native.find("Review FO4 folder"), draw)
    finish_job(workspace, native, draw)
    workspace.targets_ui.archive = str(archive)
    native.click(native.find("Use FO4 folder"), draw)
    # Save and automatic preparation are distinct cancellable jobs.
    finish_job(workspace, native, draw)
    finish_job(workspace, native, draw)
    target = m.context_library.target()
    assert target.f4se["managed"] and target.f4se["state"] == "ready"
    assert m.rows == before and not (game / "f4se_loader.exe").exists()
    assert workspace.collect_settings()["target_id"] == target.id
    native.click(native.find("Back to Library"), draw)
    native.click(native.find("Launch options"), draw)
    native.click(native.find("##launch_target"), draw)
    native.click(native.find("Original executables##target_original"), draw)
    assert not m.target_id and m.context_library is m.library
    native.click(native.find("##launch_target"), draw)
    native.click(native.find("Target##target_choice"), draw)
    assert m.target_id == target.id
    repeated = launch_targets.review(m.library, target.name, game)
    assert launch_targets.apply(m.library, repeated) == target.id
    assert m.library.rows(m.profile_id) == before


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_folder_first_f4se_cancel_or_failure_preserves_source_and_unready_target_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    from .test_f4se_setup import package
    from creation_lib.mod_manager import launch_targets
    m, draw = workspace.manager, panels(workspace)
    game = target_folder_fixture(tmp_path, monkeypatch)
    archive = package(tmp_path)
    identifier = launch_targets.apply(m.library, launch_targets.review(m.library, "Target", game))
    m.refresh()
    m.select_target(identifier)
    workspace.targets_ui.archive = str(archive)
    native.warm(draw)
    native.click(native.find("Launch options"), draw)
    native.click(native.find("FO4 folders"), draw)
    entered, release = threading.Event(), threading.Event()
    def checkpoint():
        if outcome == "failed":
            raise OSError("generated acquisition fault")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.f4se_setup._checkpoint", checkpoint)
    native.click(native.find("Prepare matching F4SE"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == outcome
    assert m.context_library.target().f4se["state"] == "missing"
    assert not (game / "f4se_loader.exe").exists()
    assert not list((m.context_library.root / "f4se").iterdir())


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_target_managed_launch_review_has_explicit_experimental_gate_and_cancel_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from .test_f4se_setup import package
    from .test_runtime import pe
    from creation_lib.mod_manager import launch_targets, f4se_setup
    m, draw = workspace.manager, panels(workspace)
    native.io.display_size = size
    game = target_folder_fixture(tmp_path, monkeypatch)
    identifier = launch_targets.apply(m.library, launch_targets.review(m.library, "Target", game))
    f4se_setup.prepare(m.library, identifier, archive=package(tmp_path))
    m.refresh()
    m.select_target(identifier)
    bundle = tmp_path / "fixture bundle"
    bundle.mkdir()
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    workspace.launcher.bundle_root = str(bundle)
    trust_fixture_bundle(workspace, monkeypatch)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.fo4_user_targets", lambda: (tmp_path / "global plugins", tmp_path / "global ini"))
    native.warm(draw)
    native.click(native.find("Run"), draw)
    finish_job(workspace, native, draw)
    assert m.launch_review.experimental
    from dataclasses import replace
    m.launch_review = replace(m.launch_review, compatibility=tuple("Generated unknown plugin finding " + str(index) for index in range(200)))
    native.warm(draw)
    native.find("Try the experimental managed F4SE launch route")
    native.find("Start reviewed executable")
    assert not m.runtime_session
    native.click(native.find("Cancel launch"), draw)
    assert m.launch_review is None and not m.runtime_session


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_game_root_setup_decline_ack_apply_and_guarded_restore_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from .test_f4se_setup import package
    from .test_reference_sources import files
    from creation_lib.mod_manager import launch_targets, f4se_setup
    m, draw = workspace.manager, panels(workspace)
    native.io.display_size = size
    monkeypatch.setattr("creation_lib.mod_manager.runtime.process_names", lambda: {"steam.exe"})
    game = target_folder_fixture(tmp_path, monkeypatch)
    identifier = launch_targets.apply(m.library, launch_targets.review(m.library, "Target", game))
    f4se_setup.prepare(m.library, identifier, archive=package(tmp_path))
    m.refresh()
    m.select_target(identifier)
    before = [(row.source, files(Path(row.source))) for row in m.rows if row.kind != "separator"]
    native.warm(draw)
    native.click(native.find("Launch options"), draw)
    native.click(native.find("FO4 folders"), draw)
    native.click(native.find("Review game-root F4SE setup"), draw)
    finish_job(workspace, native, draw)
    native.find("Apply root setup")
    native.click(native.find("Cancel root review"), draw)
    assert not m.root_setup_review and not (game / "f4se_loader.exe").exists()
    native.click(native.find("Review game-root F4SE setup"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Apply root setup"), draw)  # Disabled until separate acknowledgement.
    assert not m.busy and not (game / "f4se_loader.exe").exists()
    native.click(native.find("Allow the listed root-file writes with recoverable backups"), draw)
    native.click(native.find("Apply root setup"), draw)
    finish_job(workspace, native, draw)
    assert m.context_library.target().f4se["route"] == "game-root"
    assert (game / "f4se_loader.exe").is_file() and not (game / "Data/Scripts").exists()
    from .test_runtime import pe
    bundle = tmp_path / "root review bundle"
    bundle.mkdir()
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    workspace.launcher.bundle_root = str(bundle)
    trust_fixture_bundle(workspace, monkeypatch)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.fo4_user_targets", lambda: (tmp_path / "global plugins", tmp_path / "global ini"))
    native.click(native.find("Back to Library"), draw)
    native.click(native.find("Run"), draw)
    finish_job(workspace, native, draw)
    assert not m.launch_review.experimental and not m.launch_review.root_files
    assert m.launch_review.launch.argv[0] == str(game / "f4se_loader.exe")
    assert "-altexe" not in m.launch_review.launch.argv and "-altdll" not in m.launch_review.launch.argv
    assert m.launch_review.runtime == "modbox-vfs-fo4-x64"
    assert not any(item.label == "I understand programs may modify existing source files" for item in native.items)
    native.click(native.find("Cancel launch"), draw)
    assert not m.runtime_session
    native.click(native.find("Launch options"), draw)
    native.click(native.find("FO4 folders"), draw)
    native.click(native.find("Review root restore/recovery"), draw)
    finish_job(workspace, native, draw)
    assert not workspace.targets_ui.root_setup.ack
    native.click(native.find("Allow the listed root-file writes with recoverable backups"), draw)
    native.click(native.find("Apply root restore"), draw)
    finish_job(workspace, native, draw)
    assert not (game / "f4se_loader.exe").exists() and not m.context_library.target().f4se.get("root_setup")
    assert [(row.source, files(Path(row.source))) for row in m.rows if row.kind != "separator"] == before


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_game_root_setup_cancellation_failure_rolls_back_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    from .test_f4se_setup import package
    from creation_lib.mod_manager import launch_targets, f4se_setup, f4se_root_setup
    m, draw = workspace.manager, panels(workspace)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.process_names", lambda: {"steam.exe"})
    game = target_folder_fixture(tmp_path, monkeypatch)
    identifier = launch_targets.apply(m.library, launch_targets.review(m.library, "Target", game))
    f4se_setup.prepare(m.library, identifier, archive=package(tmp_path))
    m.refresh()
    m.select_target(identifier)
    native.warm(draw)
    native.click(native.find("Launch options"), draw)
    native.click(native.find("FO4 folders"), draw)
    native.click(native.find("Review game-root F4SE setup"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Allow the listed root-file writes with recoverable backups"), draw)
    entered, release = threading.Event(), threading.Event()
    def checkpoint(phase):
        if phase != "installed-file":
            return
        if outcome == "failed":
            raise OSError("generated root placement fault")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr(f4se_root_setup, "_checkpoint", checkpoint)
    native.click(native.find("Apply root setup"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future:
        m._future.exception(timeout=10)
        m.poll()
    native.warm(draw)
    assert m.job_state == outcome and not (game / "f4se_loader.exe").exists()
    assert not m.context_library.target().f4se.get("root_setup")


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_reported_headers_toggle_visual_sort_preserve_groups_and_selection(native, workspace, size):
    from creation_lib.mod_manager.mod_drag import begin
    from creation_lib.mod_manager.paths import ManagerError
    native.io.display_size = size
    workspace._show_filters = False
    manager, draw = workspace.manager, panels(workspace)
    before = [(row.id, row.priority, row.enabled, row.group) for row in manager.rows]
    native.warm(draw)
    selected = visible_rows(native, manager, workspace)["duplicate-1"]
    native.click(selected, draw)
    native.click(native.find("Mod name###sort_name"), draw)
    native.warm(draw)
    assert workspace._sort_by == "name" and not workspace._descending
    assert manager.selected_id == "duplicate-1"
    groups = [node.key for node in workspace._visible_tree if node.children]
    children = [node.row for node in workspace._visible_tree if node.row and node.row.group == "Visual/Textures"]
    assert [row.id for row in children if row.name == "Same name"] == ["duplicate-0", "duplicate-1"]
    with pytest.raises(ManagerError, match="ascending Priority"):
        begin(workspace, "duplicate-1")
    native.click(native.find("Mod name###sort_name"), draw)
    native.warm(draw)
    assert workspace._descending and groups == [node.key for node in workspace._visible_tree if node.children]
    children = [node.row for node in workspace._visible_tree if node.row and node.row.group == "Visual/Textures"]
    assert [row.id for row in children if row.name == "Same name"] == ["duplicate-1", "duplicate-0"]
    native.click(native.find("Version###sort_version"), draw)
    assert workspace._sort_by == "version" and not workspace._descending
    native.click(native.find("On###sort_enabled"), draw)
    assert workspace._sort_by == "enabled" and not workspace._descending
    native.click(native.find("Priority###sort_priority"), draw)
    native.warm(draw)
    assert workspace._sort_by == "priority" and not workspace._descending
    assert begin(workspace, "duplicate-1").mod_ids == ("duplicate-1",)
    assert [(row.id, row.priority, row.enabled, row.group) for row in manager.rows] == before
    assert not workspace._list_font_size and workspace._row_height >= 24


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_project_default_ignores_saved_upstream_bundle_and_cancel_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from creation_lib.mod_manager.launcher import Executable
    from creation_lib.mod_manager import runtime_setup
    from .test_runtime_setup import make_bundle
    from .test_runtime import pe
    native.io.display_size = size
    workspace._show_filters = False
    manager, draw = workspace.manager, panels(workspace)
    game = tmp_path / "game"
    game.mkdir()
    exe = manager.save_executable(Executable("", "FO4 fixture", str(pe(game / "Fallout4.exe"))))
    workspace.launcher.selected_id = exe.id
    bundle = make_bundle(tmp_path / "resource/usvfs")
    original = runtime_setup.supported_bundle
    monkeypatch.setattr(runtime_setup, "supported_bundle", lambda value: original(value, tmp_path))
    from creation_lib.mod_manager import pe_metadata
    original_version = pe_metadata.file_version
    monkeypatch.setattr(pe_metadata, "file_version", lambda path: (0, 5, 7, 2) if Path(path).name.startswith("usvfs_") else original_version(path))
    calls = []
    monkeypatch.setattr(manager, "prepare_launch", lambda *args: calls.append(args))
    native.warm(draw)
    native.click(native.find("Run"), draw)
    native.warm(draw)
    assert workspace._dialog == "Launch" and calls[0][2] == "" and not manager.error
    assert not any(item.label in {"##runtime_bundle", "I trust this existing bundle's origin"} for item in native.items)
    native.click(native.find("Cancel launch"), draw)
    calls.clear()
    workspace.launcher.bundle_root = "resource/usvfs"
    native.click(native.find("Run"), draw)
    native.warm(draw)
    assert workspace._dialog == "Launch" and not manager.error and calls[0][2] == ""
    assert not workspace.launcher.bundle_trusted
    assert not any(item.label in {"##runtime_bundle", "I trust this existing bundle's origin"} for item in native.items)
    native.warm(draw)
    native.click(native.find("Cancel launch"), draw)
    assert not manager.runtime_session and not workspace._dialog
    assert sorted(path.name for path in bundle.iterdir()) == ["usvfs_proxy_x64.exe", "usvfs_proxy_x86.exe", "usvfs_x64.dll", "usvfs_x86.dll"]


@pytest.mark.parametrize("variant", ["full", "mod_manager"])
@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_reported_one_host_log_has_severity_filter_clear_and_clean_handlers(native, workspace, tmp_path, variant, size):
    import logging
    from dataclasses import replace
    from ui.toolkit.settings import ToolkitSettings
    from ui.toolkit.variants import get_variant
    native.io.display_size = size
    settings = ToolkitSettings(path=tmp_path / "settings.json", editor_settings_path=tmp_path / "absent-editor.json")
    settings.setup_complete = True
    settings.active_workspace = "mod_manager"
    app = ToolkitApp([workspace], settings, app_variant=replace(get_variant(variant), include_ai_panel=False))
    dock = app._get_docking_params()
    assert len([window for window in dock.dockable_windows if window.label == "Log"]) == 1
    assert not any("manager_log" in window.label for window in dock.dockable_windows)
    panel = app._log_panel
    original = tuple(logging.getLogger().handlers)
    panel.install()
    panel.install()
    def draw():
        imgui.set_next_window_pos((0, 0))
        imgui.set_next_window_size(size)
        panel.draw()
    try:
        workspace._action(lambda: setattr(workspace.manager, "error", "Fixture single log error"))
        workspace.manager.poll()
        workspace.manager.poll()
        assert len([line for _, _, line in panel._lines if "Fixture single log error" in line]) == 1
        native.warm(draw)
        native.click(native.find("Error"), draw)
        assert logging.ERROR not in panel._level_filter
        native.click(native.find("Error"), draw)
        native.text(native.find("##filter"), draw, "single log")
        assert panel._filter_text == "single log"
        native.click(native.find("Clear"), draw)
        assert not panel._lines
        assert logging.getLogger().handlers.count(panel._handler) == 1
    finally:
        panel.uninstall()
        panel.uninstall()
    assert tuple(logging.getLogger().handlers) == original


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_target_customization_args_source_preferences_remove_restore_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from creation_lib.mod_manager import launch_targets, target_options, target_preferences, target_removal
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    game = target_folder_fixture(tmp_path, monkeypatch, name="customization game")
    identifier = launch_targets.apply(m.library, launch_targets.review(m.library, "Customization target", game))
    m.refresh()
    m.select_target(identifier)
    row = next(row for row in m.rows if row.name == "Base")
    m.selected_id = row.id
    shared = m.library.rows(m.profile_id)
    variant = tmp_path / "mod version for target"
    variant.mkdir()
    (variant / "file.txt").write_bytes(b"target variant")
    retained = m.context_library.root / "runtime/profiles" / m.profile_id / "saves/owned.fos"
    retained.parent.mkdir(parents=True)
    retained.write_bytes(b"retained owned save")
    native.warm(draw)
    native.click(native.find("Launch options"), draw)
    native.click(native.find("FO4 folders"), draw)
    native.click(native.find("Edit launch arguments"), draw)
    native.text(native.find("##target_arguments"), draw, "-v")
    native.click(native.find("Review target customization"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Cancel target customization"), draw)
    assert target_options.value(m.library, identifier)["arguments"] == ""
    workspace.target_customization.open("Arguments")
    native.warm(draw)
    native.text(native.find("##target_arguments"), draw, "-v")
    native.click(native.find("Review target customization"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Apply target customization"), draw)
    finish_job(workspace, native, draw)
    assert workspace.launcher.selected().arguments == "-v"
    workspace.target_customization.open("Source")
    native.warm(draw)
    native.text(native.find("##target_source"), draw, str(variant))
    native.text(native.find("##target_version"), draw, "older tested version")
    native.click(native.find("Review target customization"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Apply target customization"), draw)
    assert not m.busy and m.rows_by_id[row.id].source == row.source
    native.click(native.find("This folder is the intended version of the same mod"), draw)
    native.click(native.find("Apply target customization"), draw)
    finish_job(workspace, native, draw)
    assert m.rows_by_id[row.id].source == str(variant) and m.rows_by_id[row.id].version == "older tested version"
    assert m.library.rows(m.profile_id) == shared
    m.customize(row.id, enabled=False)
    finish_job(workspace, native, draw)
    assert not m.rows_by_id[row.id].enabled and m.library.rows(m.profile_id) == shared
    workspace.target_customization.open("Preferences")
    native.warm(draw)
    native.click(native.find("Edit local display INI"), draw)
    assert workspace._dialog == "Profile config" and workspace._profile_filename == "Fallout4Prefs.ini"
    workspace.target_customization.open("Preferences")
    native.warm(draw)
    native.choose_next("INI scope##target_ini_scope", draw)
    native.choose_next("Save scope##target_save_scope", draw)
    native.click(native.find("Review target customization"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Apply target customization"), draw)
    finish_job(workspace, native, draw)
    assert target_preferences.value(m.library, identifier) == {"ini": "shared", "saves": "shared"}
    assert m.save_library.root == m.library.root and retained.read_bytes() == b"retained owned save"
    workspace.target_customization.open("Remove")
    native.warm(draw)
    native.click(native.find("Review target customization"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Archive or restore this target metadata; retain all files"), draw)
    native.click(native.find("Apply target customization"), draw)
    finish_job(workspace, native, draw)
    assert not m.target_id and not m.targets and target_removal.removed(m.library)[0].id == identifier
    assert retained.read_bytes() == b"retained owned save" and (variant / "file.txt").read_bytes() == b"target variant"
    workspace.target_customization.open("Remove", identifier)
    native.warm(draw)
    native.click(native.find("Review target customization"), draw)
    finish_job(workspace, native, draw)
    native.click(native.find("Archive or restore this target metadata; retain all files"), draw)
    native.click(native.find("Apply target customization"), draw)
    finish_job(workspace, native, draw)
    m.select_target(identifier)
    assert m.rows_by_id[row.id].source == str(variant) and workspace.launcher.selected().arguments == "-v"


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_target_preference_apply_cancel_failure_keeps_owned_state_actual_input(native, workspace, tmp_path, monkeypatch, outcome):
    from creation_lib.mod_manager import launch_targets, target_preferences
    game = target_folder_fixture(tmp_path, monkeypatch, name="preference fault game")
    m, draw = workspace.manager, panels(workspace)
    identifier = launch_targets.apply(m.library, launch_targets.review(m.library, "Fault target", game))
    m.refresh()
    m.select_target(identifier)
    native.warm(draw)
    workspace.target_customization.open("Preferences")
    native.warm(draw)
    native.choose_next("Save scope##target_save_scope", draw)
    native.click(native.find("Review target customization"), draw)
    finish_job(workspace, native, draw)
    entered, release = threading.Event(), threading.Event()
    def fault():
        if outcome == "failed":
            raise OSError("Fixture preference commit fault")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr(target_preferences, "_checkpoint", fault)
    native.click(native.find("Apply target customization"), draw)
    try:
        if outcome == "cancelled":
            assert entered.wait(5)
            native.warm(draw)
            native.click(native.find("Cancel operation"), draw)
    finally:
        release.set()
    if m._future is not None:
        m._future.exception(timeout=10)
    m.poll()
    native.warm(draw)
    assert m.job_state == outcome and target_preferences.value(m.library, identifier)["saves"] == "target"
    assert (game / "Fallout4.exe").is_file() and workspace.target_customization.plan is not None


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_fetched_nexus_badges_reach_modlist_and_disconnect_resets_actual_input(native, workspace, size, monkeypatch):
    from creation_lib.mod_manager.nexus_state import File, State
    from ui.mod_manager import mod_list_ui
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    row = next(row for row in m.rows if row.name == "Base")
    with m.library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1 WHERE id=?", (row.id,))
    m.refresh()
    m.nexus_client = SimpleNamespace(user={"name": "Fixture", "premium": True}, forget=lambda: None)
    m.nexus_states[("fallout4", 123)] = State("fallout4", 123, (File(2, "new.zip", "2", "Main"),), ((1, 2),), "Endorsed", True)
    workspace._show_filters, workspace._metadata_fields = False, ["status"]
    rendered = []
    original = mod_list_ui.draw_badge
    def record(draw, badge, *args):
        rendered.append(badge.text)
        original(draw, badge, *args)
    monkeypatch.setattr(mod_list_ui, "draw_badge", record)
    native.warm(draw)
    assert "update" in rendered and "endorsed" in rendered
    workspace._tab = "Nexus"
    native.warm(draw)
    native.click(native.find("Disconnect and forget key"), draw)
    assert not m.nexus_states and not m.nexus_client
    native.click(native.find("Library"), draw)
    rendered.clear()
    native.warm(draw)
    assert "update" not in rendered and "endorsed" not in rendered


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_selected_target_profile_clone_retains_ini_and_plugin_settings_actual_input(native, workspace, tmp_path, monkeypatch, size):
    from creation_lib.mod_manager import launch_targets
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    game = target_folder_fixture(tmp_path, monkeypatch, name="clone target game")
    identifier = launch_targets.apply(m.library, launch_targets.review(m.library, "Clone target", game))
    m.refresh()
    m.select_target(identifier)
    source = m.profile_id
    view = m.context_library
    files = view.profile_snapshot(source)[0].files
    view.save_ini(source, "Fallout4Prefs.ini", "[Display]\nTargetClone=retained\n", files["Fallout4Prefs.ini"])
    m.refresh()
    original = view.profile_snapshot(source)[0].files
    native.warm(draw)
    native.click(native.find("Profiles"), draw)
    native.click(native.find("New profile"), draw)
    native.text(native.find("##profile_name"), draw, "Cloned target UI")
    assert workspace._new_profile_clone
    native.click(native.find("Create profile"), draw)
    finish_job(workspace, native, draw)
    copied = m.context_library.profile_snapshot(m.profile_id)[0].files
    assert m.profile_id != source and m.target_id == identifier
    assert all(copied.get(name) == original.get(name) for name in launch_targets.PROFILE_FIELDS)
    assert "TargetClone=retained" in copied["Fallout4Prefs.ini"]
    assert not (view.root / "runtime/profiles" / m.profile_id).exists()
