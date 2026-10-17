"""LeveledLists in the catalog editor: Tales' fallback rules, plugin reading, import, and the condition builder.

Ports the tests of the retired ``ui/tools/leveled_conditions`` tool against ``features/leveled_lists.py``.
"""
from __future__ import annotations

import json
import re
import struct
from pathlib import Path

import pytest

from creation_lib.catalog.plugin_catalog import catalog_path, form_key, parse_form_key
from creation_lib.esp.editor import EditorSession
from creation_lib.esp.native_runtime import (
    plugin_handle_add_record_raw, plugin_handle_call, plugin_handle_close, plugin_handle_new,
)
from ui.tools.catalog_editor.features import extension_for, leveled_lists as leveled
from ui.tools.catalog_editor.features.leveled_lists import PluginEntry, PluginList
from ui.tools.catalog_editor.forms import PluginFormSource
from ui.tools.catalog_editor.model import ADDED, CatalogSession
from ui.tools.catalog_editor.widgets import conditions as condition_widget
from ui.tools.catalog_editor.widgets import templates
from ui.tools.catalog_editor.widgets.functions import BY_ID, FUNCTIONS

TALES = Path(__file__).resolve().parents[2] / "mods" / "B21_TalesFromAppalachia"
new_condition = condition_widget.new_condition


def example_document() -> dict:
    row = leveled.list_from_plugin(PluginList(
        "MyMod.esp:000800", "MyMod_LL_Loot", "LVLI", 0x01, 0, 0,
        entries=[PluginEntry("Fallout4.esm:023736", 1, 1, 0), PluginEntry("MyMod.esp:000820", 1, 1, 0)]))
    condition = new_condition(448)
    condition["run_on"] = "target"
    condition["params"] = [{"source": "MyMod.esp:000810", "form": "MyMod.esp:000810"}, None]
    row["entries"][1]["conditions"].append(condition)
    return {"version": 1, "settings": {"source_plugin": "MyMod.esp"}, "lists": [row]}


def test_keys_are_plugin_then_one_to_eight_hex_digits():
    assert form_key("Fallout4.esm", 0x023736) == "Fallout4.esm:023736"
    assert parse_form_key("MyMod.esp:00a8F0") == ("mymod.esp", 0xA8F0)
    # The old tool accepted exactly six digits; the shared contract takes one to eight.
    assert parse_form_key("MyMod.esp:A8F0") == ("mymod.esp", 0xA8F0)
    assert parse_form_key("MyMod.esp:123456789") is None
    assert parse_form_key("023736:Fallout4.esm") is None


@pytest.mark.skipif(not (TALES / "docs" / "LEVELED_LISTS.md").is_file(), reason="Tales tree not present")
def test_the_tool_writes_the_example_the_tales_docs_and_tests_use():
    doc = (TALES / "docs" / "LEVELED_LISTS.md").read_text(encoding="utf-8")
    section = doc.split("## Catalogs from other mods", 1)[1]
    block = re.search(r"```json\n(.*?)```", section, re.S).group(1)
    assert example_document() == json.loads(block)
    assert leveled.fallback_reasons(example_document()["lists"][0], example_document()) == []


@pytest.mark.skipif(not (TALES / "src" / "Fo76Conditions.h").is_file(), reason="Tales tree not present")
def test_the_function_table_is_exactly_what_tales_answers():
    source = (TALES / "src" / "Fo76Conditions.h").read_text(encoding="utf-8")
    body = source.split("inline FunctionRule ClassifyFunction", 1)[1].split("\n}\n", 1)[0]
    tales = {int(n) for n in re.findall(r"case (\d+):", body)}
    assert {function.id for function in FUNCTIONS} == tales
    assert len(BY_ID) == len(FUNCTIONS)


def test_a_plugin_entry_keeps_owner_rank_global_and_health():
    row = leveled.list_from_plugin(PluginList("A.esp:000800", "L", "LVLN", 0x07 | 0x80, 25, 3, "A.esp:000900", [
        PluginEntry("A.esp:000801", 5, 2, 10, owner="A.esp:000950", owner_is_faction=True, owner_data=2, health=0.5),
        PluginEntry("A.esp:000802", 1, 1, 0, owner="A.esp:000951", owner_global="A.esp:000952", health=1.0),
    ]))
    assert row["flags"] == 0x07
    assert row["chance_none"] == {"value": 25, "global": "A.esp:000900"}
    assert row["max_count"] == {"value": 3}
    assert row["entries"][0]["extra"] == {"health": 0.5, "owner": "A.esp:000950", "rank": 2}
    assert row["entries"][1]["extra"] == {"health": 1.0, "owner": "A.esp:000951", "global": "A.esp:000952"}


def test_entry_differences_name_what_moved():
    document = example_document()
    plugin = PluginList("MyMod.esp:000800", "MyMod_LL_Loot", "LVLI", 1, 0, 0,
                        entries=[PluginEntry("Fallout4.esm:023736", 1, 1, 0), PluginEntry("MyMod.esp:000821", 1, 2, 0)])
    assert leveled.entry_differences(document["lists"][0], plugin) == [
        "entry 2: form MyMod.esp:000820 vs MyMod.esp:000821, count 1 vs 2"]
    plugin.entries.pop()
    assert leveled.entry_differences(document["lists"][0], plugin) == ["2 catalog entries, 1 in the plugin"]


def reasons(mutate) -> list[str]:
    document = example_document()
    mutate(document, document["lists"][0])
    return leveled.fallback_reasons(document["lists"][0], document)


def test_fallback_reasons_follow_tales_rules():
    def condition(row):
        return row["entries"][1]["conditions"][0]

    assert reasons(lambda d, row: condition(row).update(function=105)) == [
        "item 2 (MyMod.esp:000820), check 1 (function 105): Tales doesn't answer this function"]
    assert reasons(lambda d, row: condition(row).update(op="=~"))[0].startswith("item 2 (MyMod.esp:000820), check 1 (HasPerk, 448): operator")
    assert reasons(lambda d, row: condition(row).update(run_on="linked_ref")) == [
        "item 2 (MyMod.esp:000820), check 1 (HasPerk, 448): run on 'linked_ref' is not supported"]
    assert reasons(lambda d, row: row.update(flags=0x400)) == ["unknown flags 0x400"]
    assert reasons(lambda d, row: row.update(flags=0x10 | 0x20 | 0x100 | 0x200)) == []
    assert reasons(lambda d, row: row["entries"][0]["level"].update(curve="MyMod.esp:curve")) == [
        "item 1 (Fallout4.esm:023736), level: curve MyMod.esp:curve is not in the file"]

    def player_set(d, row):
        condition(row)["run_on"] = "active_players"
        row["quest"] = "MyMod.esp:000990"
    assert reasons(player_set) == [
        "item 2 (MyMod.esp:000820), check 1 (HasPerk, 448): a player-set run-on with a list quest is not supported"]
    # An empty form parameter is answered as FO76's handler does.
    assert reasons(lambda d, row: condition(row).update(function=74, params=[None, None])) == []


def test_fallbacks_name_the_item_and_check_they_come_from():
    def describe(key):
        return {"MyMod.esp:000820": "Rifle", "MyMod.esp:000810": "Scrounger"}.get(key, key)

    document = example_document()
    row = document["lists"][0]
    perk = templates.build(templates.BY_TEMPLATE["perk"])
    templates.set_form(perk, "MyMod.esp:000810")
    row["entries"][1]["conditions"] = [perk, {**new_condition(18), "run_on": "subject"}]
    (finding,) = leveled.fallback_findings(row, document, None, describe)
    assert (finding.item, finding.check) == (1, 1)
    assert finding.text == "item 2 (Rifle), check 2 (function 18): Tales doesn't answer this function"
    perk["unresolved"] = True
    first = leveled.fallback_findings(row, document, None, describe)[0]
    assert first.where == 'item 2 (Rifle), check 1 "The player has perk Scrounger" (HasPerk, 448)'


def test_condition_forms_resolve_and_cycles_fall_back():
    def use_form(d, row):
        d["condition_forms"] = {"MyMod.esp:F00001": [new_condition(77)]}
        row["conditions"] = [{**new_condition(875), "params": [{"source": "MyMod.esp:F00001"}, None]}]
    assert reasons(use_form) == []

    def spelled_differently(d, row):
        d["condition_forms"] = {"mymod.esp:00f00001": [new_condition(77)]}
        row["conditions"] = [{**new_condition(875), "params": [{"source": "MyMod.esp:F00001"}, None]}]
    assert reasons(spelled_differently) == []

    def missing(d, row):
        row["conditions"] = [{**new_condition(875), "params": [{"source": "MyMod.esp:F00009"}, None]}]
    assert reasons(missing) == ["the list, check 1 (IsTrueForConditionForm, 875): its parameters don't fit what Tales needs"]

    def cycle(d, row):
        loop = {**new_condition(875), "params": [{"source": "MyMod.esp:F00001"}, None]}
        d["condition_forms"] = {"MyMod.esp:F00001": [loop]}
        row["conditions"] = [loop]
    assert reasons(cycle) == ["the list, check 1 (IsTrueForConditionForm, 875), condition form MyMod.esp:F00001, check 1 "
                              "(IsTrueForConditionForm, 875): condition form MyMod.esp:F00001 refers back to itself"]


def test_missing_forms_are_reported_by_role():
    document = example_document()
    row = document["lists"][0]
    row["chance_none"]["global"] = "MyMod.esp:000900"
    present = {"MyMod.esp:000800": "LVLI", "Fallout4.esm:023736": "ALCH", "MyMod.esp:000820": "MISC",
               "MyMod.esp:000810": "PERK", "MyMod.esp:000900": "MISC"}

    def loaded(key, role):
        return key in present and leveled.role_accepts(role, present[key])
    assert leveled.fallback_reasons(row, document, loaded) == []
    del present["MyMod.esp:000820"]
    present["MyMod.esp:000800"] = "MISC"
    assert leveled.fallback_reasons(row, document, loaded) == [
        "form MyMod.esp:000800 not loaded or not a list",
        "item 2 (MyMod.esp:000820): form MyMod.esp:000820 not loaded or not a entry"]


# --- plugins ---------------------------------------------------------------------------------


def plugin_file(directory: Path, name: str, masters=(), records=()) -> Path:
    handle = plugin_handle_new(name, "fo4")
    try:
        if name.casefold().endswith(".esm"):
            plugin_handle_call(handle, "set_header_flags", 1)
        for master in masters:
            plugin_handle_call(handle, "add_master", master)
        for signature, form_id, subrecords in records:
            plugin_handle_add_record_raw(handle, signature, form_id, 0, 0, 131, 0, subrecords)
        path = directory / name
        plugin_handle_call(handle, "save", str(path))
        return path
    finally:
        plugin_handle_close(handle)


def write_base_and_mod(directory: Path) -> Path:
    plugin_file(directory, "Base.esm", records=[
        ("MISC", 0x800, [("EDID", b"B21_Item\0", None)]),
        ("FACT", 0x801, [("EDID", b"B21_Faction\0", None)]),
        ("GLOB", 0x802, [("EDID", b"B21_Chance\0", None), ("FNAM", b"f", None), ("FLTV", struct.pack("<f", 0), None)]),
    ])
    return plugin_file(directory, "Mod.esp", masters=("Base.esm",), records=[
        ("LVLI", 0x01000900, [
            ("EDID", b"B21_LL\0", None),
            ("LVLD", bytes([30]), None),
            ("LVLM", bytes([2]), None),
            ("LVLF", bytes([0x05]), None),
            ("LVLG", struct.pack("<I", 0x00000802), None),
            ("LLCT", bytes([2]), None),
            ("LVLO", struct.pack("<HBBIHBB", 4, 0, 0, 0x00000800, 3, 15, 0), None),
            ("COED", struct.pack("<IIf", 0x00000801, 2, 0.75), None),
            ("LVLO", struct.pack("<HBBIHBB", 1, 0, 0, 0x01000901, 1, 0, 0), None),
        ]),
        ("MISC", 0x01000901, [("EDID", b"B21_Own\0", None)]),
    ])


def form_source(directory: Path, path: Path, data=None) -> PluginFormSource:
    session = EditorSession(auto_scan_conflicts=False, master_search_paths=[data or directory], lazy_masters=True)
    return PluginFormSource(session, str(path), data)


def test_the_reader_turns_lvli_records_into_keys_values_and_extras(tmp_path):
    forms = form_source(tmp_path, write_base_and_mod(tmp_path))
    assert forms.name == "Mod.esp"
    (row,) = leveled.read_plugin_lists(forms)
    assert (row.target, row.editor_id, row.signature) == ("Mod.esp:000900", "B21_LL", "LVLI")
    assert (row.chance_none, row.max_count, row.flags, row.chance_global) == (30, 2, 0x05, "Base.esm:000802")
    first, second = row.entries
    assert (first.form, first.level, first.count, first.chance_none) == ("Base.esm:000800", 4, 3, 15)
    assert (first.owner, first.owner_is_faction, first.owner_data, first.health) == ("Base.esm:000801", True, 2, 0.75)
    assert (second.form, second.owner) == ("Mod.esp:000901", "")
    assert leveled.forms_loaded(forms, "Base.esm:000802", "value_global")
    assert leveled.forms_loaded(forms, "Mod.esp:000900", "list")
    assert not leveled.forms_loaded(forms, "Base.esm:000800", "list")
    assert not leveled.forms_loaded(forms, "Base.esm:000999", "entry")
    assert forms.describe("Base.esm:000800") == "B21_Item [MISC] Base.esm:000800"
    assert ("Base.esm:000801", "B21_Faction [FACT] Base.esm:000801") in forms.search("Faction", ("FACT",))


def test_fallout4_and_dlc_records_are_pickable_even_when_not_mastered(tmp_path):
    data = tmp_path / "Data"
    data.mkdir()
    plugin_file(data, "Fallout4.esm", records=[("PERK", 0x800, [("EDID", b"B21_Fo4Perk\0", None)])])
    plugin_file(data, "DLCCoast.esm", masters=("Fallout4.esm",), records=[
        ("PERK", 0x01000800, [("EDID", b"B21_CoastPerk\0", None)])])
    plugin_file(tmp_path, "Other.esp", records=[("PERK", 0x800, [("EDID", b"B21_OtherPerk\0", None)])])
    mod = plugin_file(tmp_path, "Mod.esp", masters=("Fallout4.esm", "Missing.esm"), records=[
        ("LVLI", 0x02000900, [("EDID", b"B21_LL\0", None)])])
    forms = form_source(tmp_path, mod, data)
    assert forms.loaded_names == ["Fallout4.esm", "Mod.esp", "DLCCoast.esm"]
    assert forms.missing_masters == ["Missing.esm"]
    perks = [label for _, label in forms.search("", ("PERK",))]
    assert perks == ["B21_CoastPerk [PERK] DLCCoast.esm:000800", "B21_Fo4Perk [PERK] Fallout4.esm:000800"]
    assert leveled.forms_loaded(forms, "DLCCoast.esm:000800", "any")
    assert not leveled.forms_loaded(forms, "Other.esp:000800", "any")
    assert forms.load_more(tmp_path / "Other.esp") == "Other.esp"
    assert leveled.forms_loaded(forms, "Other.esp:000800", "any")
    assert leveled.read_plugin_lists(forms)[0].target == "Mod.esp:000900"


# --- the model: layers and save --------------------------------------------------------------


def author_file(directory: Path, plugin: str = "Mod.esp") -> Path:
    return directory / catalog_path("LeveledLists", plugin)


def test_save_and_load_keep_fields_the_editor_does_not_show(tmp_path):
    session = CatalogSession.open(write_base_and_mod(tmp_path), data_dir=tmp_path)
    doc = session.feature("LeveledLists")
    row = example_document()["lists"][0]
    row["future_field"] = {"kept": True}
    doc.set_entry("lists", row)
    doc.set_entry("regions", {"world": "SeventySix.esm:000970", "polygons": [], "nuke": False}, map_key="SeventySix.esm:000890")
    doc.set_entry("condition_forms", [new_condition(77)], map_key="Mod.esp:F00001")
    path = doc.save()
    assert path == author_file(tmp_path)
    written = json.loads(path.read_text(encoding="utf-8"))
    assert written["lists"] == [row]
    assert written["condition_forms"] == {"Mod.esp:F00001": [new_condition(77)]}

    again = CatalogSession.open(tmp_path / "Mod.esp", data_dir=tmp_path).feature("LeveledLists")
    assert not again.dirty
    assert again.author_document() == written
    assert [r.key for r in again.rows("lists")] == ["MyMod.esp:000800"]

    path.write_text(json.dumps({"version": 2, "lists": []}), encoding="utf-8")
    broken = CatalogSession.open(tmp_path / "Mod.esp", data_dir=tmp_path).feature("LeveledLists")
    assert any("unknown version" in problem.message and problem.blocking for problem in broken.validate(None))


def test_a_masters_catalog_shows_its_lists_inherited(tmp_path):
    path = write_base_and_mod(tmp_path)
    base = tmp_path / catalog_path("LeveledLists", "Base.esm")
    base.parent.mkdir(parents=True)
    base.write_text(json.dumps({"version": 1, "lists": [example_document()["lists"][0]],
                                "condition_forms": {"Base.esm:F00001": [new_condition(77)]}}), encoding="utf-8")
    doc = CatalogSession.open(path, data_dir=tmp_path).feature("LeveledLists")
    (row,) = doc.rows("lists")
    assert (row.key, row.status, row.source) == ("MyMod.esp:000800", "inherited", "Base.esm")
    assert leveled.merged_view(doc)["condition_forms"] == {"Base.esm:F00001": [new_condition(77)]}


# --- the tool --------------------------------------------------------------------------------


@pytest.fixture
def headless(tmp_path):
    from imgui_bundle import imgui

    from ui.tools.catalog_editor.picker import Picker
    from ui.tools.catalog_editor.tool import CatalogEditorTool

    path = write_base_and_mod(tmp_path)
    forms = form_source(tmp_path, path)
    context = imgui.create_context()
    tool = CatalogEditorTool()
    io = imgui.get_io()
    io.display_size = imgui.ImVec2(1600, 1000)
    io.delta_time = 1 / 60
    io.backend_flags |= imgui.BackendFlags_.renderer_has_textures
    tool._session = CatalogSession.open(path, data_dir=tmp_path, forms=forms)
    tool._picker = Picker(forms)
    tool._feature, tool._section = "LeveledLists", "lists"

    def frames(count=2, extra=None):
        for frame in range(count):
            imgui.new_frame()
            imgui.begin("test")
            tool.draw_content()
            if extra:
                extra(frame)
            imgui.end()
            imgui.end_frame()

    try:
        yield tool, frames
    finally:
        tool.cleanup()
        imgui.destroy_context(context)


def import_action(tool):
    (action,) = extension_for("LeveledLists").section_actions(tool, tool._current_document(), "lists")
    return action


def test_import_from_plugin_adds_the_list_as_the_plugin_rolls_it(headless):
    tool, frames = headless
    doc = tool._current_document()
    target = "Mod.esp:000900"
    action = import_action(tool)
    assert action.label == "Import from plugin..."
    frames()
    assert doc.rows("lists") == [] and not doc.dirty

    action.run()
    assert tool._picker._results == [(target, f"B21_LL [LVLI] {target}  2 items")]
    tool._picker._apply(target)  # what picking the row does
    (row,) = doc.rows("lists")
    assert (row.key, row.status) == (target, ADDED)
    assert row.entry == leveled.list_from_plugin(leveled.read_plugin_lists(tool.forms)[0])
    assert tool._selected_key == target and doc.dirty
    frames()  # the detail panel draws

    tool._picker._apply(target)  # importing again selects instead of duplicating
    assert len(doc.rows("lists")) == 1
    assert "already in the catalog" in tool._result_msg
    action.run()
    assert tool._picker._results[0][1].startswith("* ")

    tool.edit_entry("lists", doc.rows("lists")[0], {**row.entry, "flags": row.entry["flags"] | 0x40})
    assert doc.author_document()["lists"][0]["flags"] & 0x40


def test_import_rejects_a_key_the_plugin_does_not_define(headless):
    tool, _ = headless
    import_action(tool).run()
    tool._picker._apply("Base.esm:000800")
    assert tool._current_document().rows("lists") == []
    assert "not a list" in tool._error_msg


def test_actions_exist_only_for_the_plugin_lists_section(headless):
    tool, _ = headless
    extension = extension_for("LeveledLists")
    doc = tool._current_document()
    assert extension.section_actions(tool, doc, "curves") == []
    tool._session.forms = None
    assert extension.section_actions(tool, doc, "lists") == []
    assert extension_for("Challenges") is None


def test_detail_panel_reports_differences_and_resets_the_items(headless):
    tool, frames = headless
    doc = tool._current_document()
    import_action(tool).run()
    tool._picker._apply("Mod.esp:000900")
    row = doc.rows("lists")[0]
    changed = {**row.entry, "entries": row.entry["entries"][:1]}
    tool.edit_entry("lists", row, changed)
    plugin_list = leveled.read_plugin_lists(tool.forms)[0]
    assert leveled.entry_differences(changed, plugin_list) == ["1 catalog entries, 2 in the plugin"]
    frames()
    tool.edit_entry("lists", doc.rows("lists")[0], {**changed, "entries": leveled.list_from_plugin(plugin_list)["entries"]})
    assert leveled.entry_differences(doc.rows("lists")[0].entry, plugin_list) == []


def test_fallback_reasons_save_as_warnings_not_blockers(headless):
    tool, _ = headless
    doc = tool._current_document()
    row = leveled.list_from_plugin(leveled.read_plugin_lists(tool.forms)[0])
    row["entries"][0]["conditions"].append(new_condition(105))
    doc.set_entry("lists", row)
    (problem,) = extension_for("LeveledLists").problems(doc, tool.forms)
    assert not problem.blocking and (problem.section, problem.key) == ("lists", "Mod.esp:000900")
    assert "Tales doesn't answer this function" in problem.message
    assert tool.validate() == [problem] and tool._error_msg == ""
    saved = tool.save(everything=False)
    assert saved == [author_file(tool._session.plugin_path.parent)] and saved[0].is_file()
    assert tool._problems == [problem]


def test_inherited_lists_are_not_warned_about(headless, tmp_path):
    tool, _ = headless
    base = tmp_path / catalog_path("LeveledLists", "Base.esm")
    base.parent.mkdir(parents=True)
    broken = example_document()["lists"][0]
    broken["flags"] = 0x400
    base.write_text(json.dumps({"version": 1, "lists": [broken]}), encoding="utf-8")
    tool._session._features.clear()
    doc = tool._current_document()
    assert doc.rows("lists")[0].status == "inherited"
    assert extension_for("LeveledLists").problems(doc, tool.forms) == []


def test_the_condition_builder_adds_checks_to_an_item_in_plain_words(headless):
    from ui.tools.catalog_editor import render

    tool, _ = headless
    doc = tool._current_document()
    import_action(tool).run()
    tool._picker._apply("Mod.esp:000900")
    ctx = render.RenderContext(doc.schema, tool.forms, tool._picker, doc.section_keys)
    editor = condition_widget._Editor([], ctx, "second-item")
    editor.add_check(templates.build(templates.BY_TEMPLATE["perk"]))
    templates.set_form(editor.rows[0], "Base.esm:000801")
    editor.add_check(templates.build(templates.BY_TEMPLATE["level"]))
    assert [templates.sentence(c, editor.describe) for c in editor.rows] == [
        "The player has perk B21_Faction", "The player's level is at least 10"]
    templates.set_chain_mode(editor.rows, "any")
    editor.add_check(templates.build(templates.BY_TEMPLATE["night"]))
    assert templates.chain_mode(editor.rows) == "any"
    assert editor.state["editing"] == 2

    row = doc.rows("lists")[0]
    entries = [dict(entry) for entry in row.entry["entries"]]
    entries[1]["conditions"] = editor.rows
    tool.edit_entry("lists", row, {**row.entry, "entries": entries})
    saved = doc.author_document()["lists"][0]["entries"][1]["conditions"]
    assert [c["function"] for c in saved] == [c["function"] for c in editor.rows] and len(saved) == 3
    assert leveled.fallback_reasons(doc.rows("lists")[0].entry, leveled.merged_view(doc)) == []


def test_every_supported_condition_can_be_added_from_the_palette(headless):
    from ui.tools.catalog_editor import render
    from ui.tools.catalog_editor.widgets.functions import FUNCTIONS

    tool, _ = headless
    doc = tool._current_document()
    ctx = render.RenderContext(doc.schema, tool.forms, tool._picker, doc.section_keys)
    editor = condition_widget._Editor([], ctx, "palette")
    for function in FUNCTIONS:
        editor.add_function(function)
    assert [c["function"] for c in editor.rows] == [f.id for f in FUNCTIONS]

    race = next(f for f in FUNCTIONS if f.name == "GetIsRace")
    editor = condition_widget._Editor([], ctx, "race")
    editor.add_function(race)
    assert tool._picker._wants_open and tool._picker._signatures == ("RACE",)
    tool._picker._apply("Base.esm:000801")
    editor.apply_landed()
    assert editor.rows[0]["params"][0]["form"] == "Base.esm:000801"


def test_the_picker_tree_lists_loaded_plugins_and_unchecked_ones_drop_out_of_search(headless):
    tool, frames = headless
    picker = tool._picker
    groups = dict(picker.plugin_groups())
    assert groups["Your plugin"] == ["Mod.esp"] and groups["Masters"] == ["Base.esm"]

    picker.open(None, lambda key: None)
    plugins = {key.split(":")[0] for key, _ in picker._results}
    assert {"Base.esm", "Mod.esp"} <= plugins
    picker.set_plugin_enabled("base.ESM", False)
    assert picker.enabled_plugins() == ["Mod.esp"]
    assert {key.split(":")[0] for key, _ in picker._results} == {"Mod.esp"}
    frames(3, lambda _frame: picker.draw())  # the tree, the bigger modal and the results draw headless
    picker.set_plugin_enabled("Mod.esp", False)
    assert picker._results == [] and picker.enabled_plugins() == []
    picker.set_plugin_enabled("Base.esm", True)
    picker.set_plugin_enabled("Mod.esp", True)
    assert picker.enabled_plugins() is None


def test_every_editor_section_draws_headless(headless):
    from ui.tools.catalog_editor import render
    from ui.tools.catalog_editor.tool import SETTINGS_TAB

    tool, frames = headless
    doc = tool._current_document()
    row = leveled.list_from_plugin(leveled.read_plugin_lists(tool.forms)[0])
    row["flags"] |= 0x200
    row["quest"] = "Mod.esp:000990"
    row["chance_none"]["curve"] = "Mod.esp:C00001"
    row_conditions = row["entries"][0]["conditions"]
    for function in FUNCTIONS:
        row_conditions.append({**new_condition(function.id), "run_on": "reference", "reference": "Base.esm:000800"})
    row_conditions.append({**new_condition(1), "global": "Base.esm:000802"})
    for template in templates.TEMPLATES:
        row["conditions"].append(templates.build(template))
    doc.set_entry("lists", row)
    doc.set_entry("curves", {"points": [[0, 0], [10, 50]]}, map_key="Mod.esp:C00001")
    doc.set_entry("condition_forms", [new_condition(77)], map_key="Mod.esp:F00001")
    doc.set_entry("delegates", "Mod.esp:000900", map_key="Base.esm:000800")

    for section in (*doc.sections(), SETTINGS_TAB):
        tool._select_section(section)
        rows = doc.rows(section) if section != SETTINGS_TAB else []
        tool._selected_key = rows[0].key if rows else None
        frames(2)

    ctx = render.RenderContext(doc.schema, tool.forms, tool._picker, doc.section_keys)
    for rows, name in ((row["conditions"], "list"), (row_conditions, "entry")):
        for index in range(len(rows)):
            condition_widget._state[name] = {"editing": index, "filter": ""}
            frames(1, lambda frame: condition_widget.draw("", {}, rows, ctx, name))
    tool._picker.open(("FACT",), lambda key: None)
    frames(3)
    assert tool._picker._results == [("Base.esm:000801", "B21_Faction [FACT] Base.esm:000801")]
    assert tool._error_msg == ""
    assert all(isinstance(c["params"], list) for c in row_conditions)


# --- the condition builder -------------------------------------------------------------------


def test_every_builder_check_makes_a_condition_tales_rolls_and_reads_back():
    for template in templates.TEMPLATES:
        assert template.function in BY_ID, template.id
        for who in template.who:
            condition = templates.build(template, who)
            if who == "reference":
                condition["reference"] = "MyMod.esp:000830"
            if template.picks_form:
                templates.set_form(condition, "MyMod.esp:000840")
            assert templates.match(condition) is template, (template.id, who)
            document = example_document()
            document["lists"][0]["conditions"] = [condition]
            assert leveled.fallback_reasons(document["lists"][0], document) == [], (template.id, who)
            assert "(pick one)" not in templates.sentence(condition, lambda key: f"Thing [MISC] {key}")


def test_builder_sentences_read_as_plain_words():
    def describe(key):
        return {"MyMod.esp:000810": "Scrounger [PERK] MyMod.esp:000810"}.get(key, f"{key} (not loaded)")

    perk = templates.build(templates.BY_TEMPLATE["perk"])
    templates.set_form(perk, "MyMod.esp:000810")
    assert perk["run_on"] == "reference" and perk["reference"] == templates.PLAYER_REF
    assert templates.sentence(perk, describe) == "The player has perk Scrounger"
    templates.set_truth(perk, False)
    assert templates.sentence(perk, describe) == "NOT: The player has perk Scrounger"
    level = templates.build(templates.BY_TEMPLATE["level"])
    assert templates.sentence(level, describe) == "The player's level is at least 10"
    assert templates.sentence(templates.build(templates.BY_TEMPLATE["random"]), describe) == "50% chance"
    day = templates.build(templates.BY_TEMPLATE["day"])
    day["value"] = 6
    assert templates.sentence(day, describe) == "It is Saturday"
    sex = templates.build(templates.BY_TEMPLATE["sex"])
    templates.set_sex(sex, True)
    assert templates.sentence(sex, describe) == "This container/actor is female"
    templates.set_who(perk, "self")
    assert perk["run_on"] == "subject" and "reference" not in perk
    raw = {**new_condition(105), "run_on": "target"}
    assert templates.match(raw) is None
    assert templates.sentence(raw, describe) == "function 105() on target == 1"


def test_match_all_or_any_sets_fo76_or_flags():
    rows = [new_condition(77), new_condition(80), new_condition(5)]
    assert templates.chain_mode(rows) == "all"
    templates.set_chain_mode(rows, "any")
    assert [row.get("or", False) for row in rows] == [True, True, False]
    assert templates.chain_mode(rows) == "any"
    rows[1].pop("or")
    assert templates.chain_mode(rows) == "mixed"
    templates.set_chain_mode(rows, "all")
    assert not any(row.get("or") for row in rows)


def test_every_builder_check_has_a_palette_title():
    assert set(templates.TITLES) == set(templates.BY_TEMPLATE)
