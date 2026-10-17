"""The shared entry workbench: browser filters, the per-entry diff, the layer strip, banner, inspector tabs,
the whole-list copy confirmation and the roll preview's unavailable path."""
from __future__ import annotations

import json
from dataclasses import dataclass

import pytest
from test_catalog_editor_leveled import example_document, headless  # noqa: F401  (headless is a fixture)

from creation_lib.catalog.plugin_catalog import catalog_path
from ui.tools.catalog_editor import simulate, workbench as wb
from ui.tools.catalog_editor.features import FeatureExtension, extension_for
from ui.tools.catalog_editor.model import ADDED, INHERITED, OVERRIDDEN, REMOVED
from ui.tools.catalog_editor.widgets import conditions as condition_widget
from ui.tools.catalog_editor.widgets.functions import answered_by


@dataclass
class Row:
    key: str
    entry: object
    status: str = INHERITED
    inherited: object = None
    source: str = "Base.esm"
    origin: str = "Base.esm"


def item(form, level=1, count=1, **extra):
    return {"form": form, "level": {"value": level}, "count": {"value": count}, "chance_none": {"value": 0},
            "conditions": [], **extra}


# --- browser -----------------------------------------------------------------------------------------


def test_browser_filters_mine_changed_problems_and_search_by_name_or_key():
    rows = [Row("A.esm:000001", {}, INHERITED), Row("A.esm:000002", {}, OVERRIDDEN), Row("Mod.esp:000003", {}, ADDED),
            Row("A.esm:000004", {}, REMOVED)]
    names = {"A.esm:000001": "LLI_Raider_Ammo", "A.esm:000002": "LLI_Raider_Weapons", "Mod.esp:000003": "B21_Cache",
             "A.esm:000004": "LLI_Chems"}

    def keys(mode, needle="", problems=()):
        return [row.key for row in wb.filter_rows(rows, mode, needle, lambda row: names[row.key], set(problems))]

    assert keys(wb.ALL) == [row.key for row in rows]
    assert keys(wb.MINE) == ["A.esm:000002", "Mod.esp:000003", "A.esm:000004"]
    assert keys(wb.CHANGED) == ["A.esm:000002", "A.esm:000004"]
    assert keys(wb.PROBLEMS, problems={"Mod.esp:000003"}) == ["Mod.esp:000003"]
    assert keys(wb.ALL, "raider") == ["A.esm:000001", "A.esm:000002"]
    assert keys(wb.ALL, "000003") == ["Mod.esp:000003"]


def test_entry_names_prefer_the_entrys_editor_id_then_the_keys_record():
    describe = {"A.esm:000010": "LL_Thing [LVLI] A.esm:000010"}.get
    assert wb.default_entry_name(Row("A.esm:000010", {"editor_id": "Mine"}), describe) == "Mine"
    assert wb.default_entry_name(Row("A.esm:000010", {}), describe) == "LL_Thing"
    assert wb.default_entry_name(Row("A.esm:000011", {}), describe) == "A.esm:000011"
    assert wb.default_entry_name(Row("bullet_storm", {}), describe) == "bullet_storm"


# --- diff --------------------------------------------------------------------------------------------


def test_list_items_match_by_form_then_position():
    master = [item("A.esm:01"), item("A.esm:02"), item("A.esm:03", 30)]
    mine = [item("A.esm:02"), item("A.esm:01", 5), item("Mod.esp:09")]
    pairs = wb.match_items(master, mine)
    assert (0, 1) in pairs and (1, 0) in pairs  # reordered items still pair by form
    assert (2, 2) not in pairs  # a different form at the same spot isn't "changed": one removed, one added
    assert (2, None) in pairs and (None, 2) in pairs
    curve = wb.match_items([[0, 1], [10, 5]], [[0, 1], [10, 6], [20, 9]])
    assert curve == [(0, 0), (1, 1), (None, 2)]  # no forms: by position


def test_diff_lines_and_undo_restore_the_masters_values():
    master = {"target": "A.esm:0100", "chance_none": {"value": 10},
              "entries": [item("A.esm:01"), item("A.esm:02"), item("A.esm:03")]}
    mine = {"target": "A.esm:0100", "chance_none": {"value": 25}, "scope": "seventysix",
            "entries": [item("A.esm:01"), item("A.esm:02", 15), item("Mod.esp:09")]}
    lines = wb.diff_entry(master, mine)
    kinds = {(line.field, line.master_index, line.mine_index): line.kind for line in lines}
    assert kinds[("target", None, None)] == wb.SAME
    assert kinds[("chance_none", None, None)] == wb.CHANGE
    assert kinds[("scope", None, None)] == wb.ADD
    assert kinds[("entries", 1, 1)] == wb.CHANGE
    assert kinds[("entries", 2, None)] == wb.REMOVE
    assert kinds[("entries", None, 2)] == wb.ADD
    assert wb.diff_counts(lines) == (2, 1, 2)
    undone = mine
    for line in [line for line in lines if line.kind != wb.SAME][::-1]:
        undone = wb.revert_line(undone, master, line)
    assert wb.diff_entry(master, undone) == [line for line in wb.diff_entry(master, master)]
    assert sorted(i["form"] for i in undone["entries"]) == ["A.esm:01", "A.esm:02", "A.esm:03"]


def test_layer_strip_counts_what_the_plugin_does():
    master = {"entries": [item("A.esm:01"), item("A.esm:02"), item("A.esm:03")]}
    mine = {"entries": [item("A.esm:01", 4), item("A.esm:02"), item("Mod.esp:09")]}
    main = ("entries", "items")
    row = Row("A.esm:0100", mine, OVERRIDDEN, master, "Mod.esp")
    assert wb.layer_strip(row, "Mod.esp", "Base.esm", main) == (
        "Base.esm · 3 items -> Mod.esp · +1 -1 ~1 -> Result · 3 items")
    assert wb.layer_strip(Row("k", master, INHERITED, master), "Mod.esp", "Base.esm", main).endswith("(unchanged)")
    assert wb.layer_strip(Row("k", mine, ADDED), "Mod.esp", "", main) == "Mod.esp adds this -> Result · 3 items"
    assert "Fallout 4" in wb.layer_strip(Row("k", master, REMOVED, master), "Mod.esp", "Base.esm", main, "Fallout 4 rolls it")


def test_value_text_reads_values_forms_and_items():
    names = {"A.esm:000001": "Pipe Pistol"}.get
    assert wb.value_text({"value": 25, "global": "A.esm:000002"}, lambda k: names(k) or k) == "25 (from a global)"
    assert wb.value_text("A.esm:000001", lambda k: names(k) or k) == "Pipe Pistol"
    assert wb.value_text(item("A.esm:000001"), lambda k: names(k) or k).startswith("Pipe Pistol · level 1")
    assert wb.value_text([1, 2]) == "1, 2" and wb.value_text([{}, {}, {}]) == "3 items"


# --- the tool, on a master's catalog -----------------------------------------------------------------


def inherited_list(tool, tmp_path, entries_extra=None):
    """Base.esm's catalog has a three-item list; returns its row (inherited by Mod.esp)."""
    row = example_document()["lists"][0]
    row["entries"].append({**row["entries"][0], "form": "Base.esm:000800"})
    if entries_extra:
        entries_extra(row)
    base = tmp_path / catalog_path("LeveledLists", "Base.esm")
    base.parent.mkdir(parents=True, exist_ok=True)
    base.write_text(json.dumps({"version": 1, "lists": [row]}), encoding="utf-8")
    tool._session._features.clear()
    tool._documents.clear()
    doc = tool._current_document()
    (found,) = doc.rows("lists")
    tool.select_row(found.key)
    return doc, found


def test_changing_a_masters_list_asks_first_and_says_the_whole_list_is_copied(headless, tmp_path):  # noqa: F811
    tool, frames = headless
    doc, row = inherited_list(tool, tmp_path)
    assert row.status == INHERITED and row.origin == "Base.esm"
    view = tool._workbench.view(doc, "lists", row)
    removed = {**row.entry, "entries": row.entry["entries"][:2]}
    tool._workbench.request_edit(view, removed)
    pending = tool._workbench.pending
    assert pending is not None and doc.rows("lists")[0].status == INHERITED  # nothing changed yet
    assert "whole list (3 items)" in pending.notice and "won't reach you" in pending.notice
    assert "B21_Item left out" in pending.notice and "Send list to Fallout 4's roll" in pending.notice
    frames(2)  # the confirmation draws
    tool._workbench.cancel_copy()
    assert doc.rows("lists")[0].status == INHERITED
    tool._workbench.request_edit(view, removed)
    tool._workbench.confirm_copy()
    (row,) = doc.rows("lists")
    assert row.status == OVERRIDDEN and len(row.entry["entries"]) == 2
    frames(2)  # header shows the persistent "owns a copy" note, the diff tabs and the struck-through row
    lines = wb.diff_entry(row.inherited, row.entry)
    (gone,) = [line for line in lines if line.kind == wb.REMOVE]
    tool._workbench.undo_line(tool._workbench.view(doc, "lists", row), gone)
    assert doc.rows("lists")[0].status == INHERITED  # undoing the only change drops the copy


def test_send_to_fallout4_wording_and_the_removed_state(headless, tmp_path):  # noqa: F811
    tool, frames = headless
    doc, row = inherited_list(tool, tmp_path)
    wording = tool._workbench.remove_wording(doc, "lists")
    assert wording.label == "Send list to Fallout 4's roll" and "Doesn't delete any loot" in wording.tooltip
    assert tool._workbench.remove_wording(doc, "curves").label == "Remove from the catalog"
    tool.remove_row("lists", row)
    (row,) = doc.rows("lists")
    assert row.status == REMOVED
    extension = extension_for("LeveledLists")
    assert [badge.text for badge in extension.browser_badges(tool, doc, "lists", row)] == ["FO4 roll"]
    view = tool._workbench.view(doc, "lists", row)
    assert view.read_only and extension.status_badge(view).text == "Fallout 4 rolls it"
    frames(2)


def test_the_banner_names_the_item_and_check_and_links_to_them(headless, tmp_path):  # noqa: F811
    tool, frames = headless

    def unanswerable(row):
        row["entries"][2]["conditions"] = [{**condition_widget.new_condition(18), "run_on": "subject"}]

    doc, row = inherited_list(tool, tmp_path, unanswerable)
    view = tool._workbench.view(doc, "lists", row)
    (problem,) = [problem for problem in tool._workbench.entry_problems(view) if problem.check is not None]
    assert problem.whole and (problem.item, problem.check) == (2, 0)
    assert "item 3 (B21_Item), check 1 (function 18)" in problem.message and "entry 3" not in problem.message
    assert extension_for("LeveledLists").status_badge(view).role == "error"
    frames(2)
    view.select_item(problem.item)
    view.focus_tab("Checks")
    frames(2)
    assert tool._workbench.items[("LeveledLists", "lists", row.key)] == 2


def test_restoring_an_item_warns_when_it_would_make_the_list_fall_back(headless, tmp_path):  # noqa: F811
    tool, _ = headless

    def unanswerable(row):
        row["entries"][2]["conditions"] = [{**condition_widget.new_condition(18), "run_on": "subject"}]

    doc, row = inherited_list(tool, tmp_path, unanswerable)
    master_entries = row.entry["entries"]
    doc.set_entry("lists", {**row.entry, "entries": master_entries[:2]})
    (row,) = doc.rows("lists")
    view = tool._workbench.view(doc, "lists", row)
    extension = extension_for("LeveledLists")
    warning = extension.restore_warning(view, master_entries, row.entry["entries"], 2)
    assert warning.startswith("Restoring this item would hand the whole list to Fallout 4's roll")
    assert extension.restore_warning(view, master_entries, row.entry["entries"], 1) == ""


def test_the_drops_table_works_without_the_roll_preview(headless, monkeypatch):  # noqa: F811
    tool, frames = headless
    doc = tool._current_document()
    key = doc.set_entry("lists", example_document()["lists"][0])
    tool.select_row(key)
    extension = extension_for("LeveledLists")

    def missing(*args, **kwargs):
        raise NotImplementedError

    monkeypatch.setattr(simulate, "preview_list", missing)
    monkeypatch.setattr(simulate, "sample", missing)
    extension._preview.key = None
    view = tool._workbench.view(doc, "lists", doc.rows("lists")[0])
    preview, note = extension.preview(view)
    assert preview is None and note.startswith("odds unavailable")
    frames(3)
    extension.roll(view)
    assert extension.situation.histogram is None and "Can't roll" in tool._error_msg


def test_the_drops_table_shows_a_fake_preview_and_histogram(headless, monkeypatch):  # noqa: F811
    tool, frames = headless
    doc = tool._current_document()
    key = doc.set_entry("lists", example_document()["lists"][0])
    tool.select_row(key)
    extension = extension_for("LeveledLists")
    seen = {}

    def fake_preview(entry, situation, resolve_list=None):
        seen["situation"] = situation
        return simulate.ListPreview(True, False, [], 0.1, [
            simulate.EntryOutcome(0, 0.45, True, ["eligible"]),
            simulate.EntryOutcome(1, None, False, ["player lacks perk"], ["k1"])],
            [simulate.UnknownCheck("k1", "weather keyword", 1)], ["odds from a 4000-roll simulation"])

    monkeypatch.setattr(simulate, "preview_list", fake_preview)
    monkeypatch.setattr(simulate, "sample", lambda entry, situation, rolls, seed=0, resolve_list=None:
                        {"Fallout4.esm:023736": 900, "": 100})
    extension._preview.key = None
    extension.situation.level, extension.situation.assume = 30, 1
    view = tool._workbench.view(doc, "lists", doc.rows("lists")[0])
    preview, _ = extension.preview(view)
    assert preview.entries[0].odds == 0.45 and extension.situation.unknown_count == 1
    assert seen["situation"].level == 30 and seen["situation"].assume == {"k1": True}
    extension.roll(view)
    assert extension.situation.histogram[1] == {"Fallout4.esm:023736": 900, "": 100}
    frames(3)
    extension.situation.assume = 0


# --- inspector ---------------------------------------------------------------------------------------


def test_generic_features_get_fields_and_in_game_tabs(headless):  # noqa: F811
    tool, frames = headless
    doc = tool._session.feature("PerkCards")
    key = doc.set_entry("cards", {"form": "Mod.esp:000D01", "special": 1, "ranks": [{"cost": 1, "name": "A"}]})
    tool._feature, tool._section = "PerkCards", "cards"
    tool.select_row(key)
    view = tool._workbench.view(doc, "cards", doc.rows("cards")[0])
    assert [tab.title for tab in tool._workbench._generic_tabs(view, extension_for("PerkCards"))] == ["Fields"]
    assert [tab.title for tab in tool._workbench._generic_tabs(view, extension_for("Diseases"))] == ["Fields", "In game"]
    assert extension_for("PerkCards").result_view(doc, "cards").title == "Ranks"
    frames(2)


def test_every_feature_extension_hook_has_a_harmless_default():
    plain = FeatureExtension()
    assert plain.entry_name(None, None, "", None) is None and plain.browser_badges(None, None, "", None) == []
    assert plain.result_view(None, "") is None and plain.inspector_tabs(None) == []
    assert plain.entry_problems(None) is None and plain.sentence(None) == "" and not plain.has_situation(None, "")


@pytest.mark.parametrize("function, answer", [(80, "Fallout 4"), (765, "Tales"), (859, "Tales"), (18, "falls back")])
def test_checks_say_who_answers_them(function, answer):
    assert answered_by(function) == answer


def test_narrow_windows_fold_the_browser_into_a_combo(headless):  # noqa: F811
    from imgui_bundle import imgui

    tool, frames = headless
    doc = tool._current_document()
    tool.select_row(doc.set_entry("lists", example_document()["lists"][0]))
    io = imgui.get_io()
    io.display_size = imgui.ImVec2(1000, 800)
    frames(2)
    io.display_size = imgui.ImVec2(1600, 1000)
    frames(2)
