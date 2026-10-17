"""Search/order/identity/analysis regressions use synthetic native data."""
from concurrent.futures import Future
from dataclasses import replace
from threading import Event

import pytest

from creation_lib.esp.editor.fields import Field, FieldKind
from creation_lib.esp.editor.conflicts import ConflictScan
from creation_lib.esp.editor.session import LoadedPlugin
from creation_lib.esp.native_runtime import RecordSummary
from ui.esp_editor import app as app_module
from ui.esp_editor.navigation import matches, record_identity, session_slots, sort_records
from ui.tests.test_esp_editor_workflow import plugin_file


@pytest.fixture
def editor():
    app = app_module.EspEditorApp()
    yield app
    app.cleanup()


def loaded(index, name=None, game="fo4"):
    return LoadedPlugin(index + 1, f"/fixture/{name or str(index) + '.esp'}", game, False, index, name or f"{index}.esp")


def test_position_nine_and_file_index_eight_can_identify_same_owner():
    plugins = [loaded(i) for i in range(9)]
    slots = session_slots(plugins, {p.handle: 0 for p in plugins})
    identity = record_identity(plugins[8], 0x089399CE, tuple(p.plugin_name for p in plugins[:8]), plugins, slots)
    assert plugins[8].load_order_index + 1 == 9
    assert slots[plugins[8].handle].label == "08"
    assert identity.file_id == identity.session_id == "089399CE"
    assert identity.owner == plugins[8].plugin_name


def test_file_master_count_and_owner_differ_from_containing_slot():
    plugins = [loaded(i) for i in range(10)]
    slots = session_slots(plugins, {p.handle: 0 for p in plugins})
    # Containing file is slot 09; own file-local slot is 02.
    own = record_identity(plugins[9], 0x029399CE, (plugins[3].plugin_name, plugins[5].plugin_name), plugins, slots)
    assert own.file_id == "029399CE" and own.session_id == "099399CE"
    # An override uses master index 01 and retains the original owner's slot 05.
    override = record_identity(plugins[9], 0x019399CE, (plugins[3].plugin_name, plugins[5].plugin_name), plugins, slots)
    assert override.owner == plugins[5].plugin_name
    assert override.session_id == "059399CE"


def test_esl_header_flags_assign_light_slots_and_do_not_consume_full_slots():
    plugins = [loaded(0, "Base.esm"), loaded(1, "Small.esp"), loaded(2, "Normal.esl"), loaded(3, "Second.esp")]
    slots = session_slots(plugins, {plugins[0].handle: 1, plugins[1].handle: 0x200, plugins[2].handle: 0, plugins[3].handle: 0x200})
    assert [slots[p.handle].label for p in plugins] == ["00", "FE:000", "01", "FE:001"]
    own = record_identity(plugins[1], 0x01000800, ("Base.esm",), plugins, slots)
    assert own.session_id == "FE000800" and own.owner == "Small.esp"
    override = record_identity(plugins[3], 0x01000800, ("Base.esm", "Small.esp"), plugins, slots)
    assert override.session_id == "FE000800" and override.owner == "Small.esp"
    invalid = record_identity(plugins[1], 0x019399CE, ("Base.esm",), plugins, slots)
    assert invalid.session_id is None  # never silently truncate an oversized ESL object ID


def test_starfield_medium_and_unloaded_owner_resolution():
    plugins = [loaded(0, "Base.esm", "starfield"), loaded(1, "Medium.esm", "starfield")]
    slots = session_slots(plugins, {plugins[0].handle: 0, plugins[1].handle: 0x400})
    assert slots[plugins[1].handle].label == "FD:00"
    assert record_identity(plugins[1], 0x01001234, ("Base.esm",), plugins, slots).session_id == "FD001234"
    assert record_identity(plugins[1], 0x00001234, ("Missing.esm",), plugins, slots).session_id is None
    assert record_identity(plugins[1], 0x07001234, (), plugins, slots).owner is None
    slots = session_slots(plugins, {plugins[0].handle: 0, plugins[1].handle: 0x100})
    assert slots[plugins[1].handle].label == "FE:000"
    assert record_identity(plugins[1], 0x01000800, ("Base.esm",), plugins, slots).session_id == "FE000800"
    assert dict((mask, name) for mask, name, _ in app_module._HEADER_FLAG_TABLES["starfield"])[0x400] == "Medium master"


def test_unknown_metadata_never_guesses_original_owner_or_light_slot(editor):
    plugin = loaded(0)
    editor.session._plugins = [plugin]
    try:
        assert editor._record_identity(plugin.handle, 0x800).owner is None
        editor._plugin_masters[plugin.handle] = ()
        identity = editor._record_identity(plugin.handle, 0x800)
        assert identity.owner == plugin.plugin_name and identity.session_id is None
        editor._plugin_flags[plugin.handle] = 0x200
        assert editor._record_identity(plugin.handle, 0x800).session_id == "FE000800"
    finally:
        editor.session._plugins.clear()


def test_search_fields_formid_prefix_type_labels_and_stable_display_sort():
    records = [RecordSummary(0x800, "MISC", "B21_Zebra"), RecordSummary(0x802, "WEAP", "B21_Alpha"), RecordSummary(0x801, "MISC", "B21_Alpha")]
    assert matches(records[0], "0x00000800", "", "FormID")
    assert not matches(records[0], "800", "", "Editor ID")
    assert matches(records[1], "weapon", "", "Record type", {"WEAP": "Weapon"})
    assert not matches(records[1], "alpha", "MISC", "Editor ID")
    assert [r.form_id for r in sort_records(records, "Editor ID", False)] == [0x801, 0x802, 0x800]
    assert [r.form_id for r in sort_records(records, "FormID", True)] == [0x802, 0x801, 0x800]


def test_name_search_uses_full_name_exclusively_on_worker(editor, tmp_path):
    plugin = editor.session.load(plugin_file(tmp_path, "Names.esp"), game="fo4")
    records = [RecordSummary(0x800, "MISC", "B21_Fixture")]
    key = (plugin.handle, "MISC")
    editor._cached_group_records[key] = records
    editor._search_field, editor._filter_text = "Name", "fixture value"
    assert editor._filter_group_records(key, records) == []
    editor._filter_futures[key][1].result(timeout=5)
    editor.poll()
    assert editor._filter_group_records(key, records) == records
    editor._filter_text = "b21_fixture"  # Editor ID contains this, FULL does not
    editor._filter_group_records(key, records)
    editor._filter_futures[key][1].result(timeout=5)
    editor.poll()
    assert editor._filter_group_records(key, records) == []


def test_native_incremental_analysis_overview_navigation_and_sort_preserve_priority(editor, tmp_path):
    plugin_file(tmp_path, "Base.esm", value=1)
    patch = editor.session.load(plugin_file(tmp_path, "Patch.esp", masters=("Base.esm",), value=2), game="fo4")
    before = [(p.handle, p.load_order_index) for p in editor.session.plugins]
    editor.start_override_analysis()
    for _ in range(8):
        if editor._analysis_future is None:
            break
        editor._analysis_future.result(timeout=5)
        editor.poll()
    assert editor._scan_complete and editor._analysis_completed == editor._analysis_total
    report = editor._overview_reports()[0]
    assert report.winner.plugin_handle == patch.handle
    assert editor._record_role(patch.handle, 0x800) == "winner"
    editor._record_sort, editor._sort_descending = "Editor ID", True
    assert editor._overview_reports() == [report]
    assert [(p.handle, p.load_order_index) for p in editor.session.plugins] == before
    editor._show_overview = True
    editor._navigate_report(report, report.master_entry)
    assert not editor._show_overview
    assert editor.selection.plugin_handle == report.master_entry.plugin_handle
    editor._overview_query = "no_such_record"
    assert editor._overview_reports() == []


def test_cancelled_and_stale_analysis_never_applies_background_result(editor):
    editor.session._plugins = [loaded(0)]
    editor._analysis_stamp = editor._view_stamp()
    future = Future()
    future.set_running_or_notify_cancel()
    editor._analysis_future = future
    editor.cancel_override_analysis()
    future.set_result({"MISC": 1})
    editor.poll()
    assert editor._conflict_scan is None and not editor._analysis_queue
    editor._analysis_cancelled = False
    editor._analysis_stamp = editor._view_stamp()
    future = Future()
    editor._analysis_future = future
    editor._content_revision += 1
    future.set_result({"MISC": 1})
    editor.poll()
    assert editor._conflict_scan is None and "changed" in editor._analysis_label
    editor.session._plugins.clear()


def test_large_overview_filters_on_worker_and_rejects_stale_query(editor, tmp_path, monkeypatch):
    plugin_file(tmp_path, "Base.esm", value=1)
    editor.session.load(plugin_file(tmp_path, "Patch.esp", masters=("Base.esm",), value=2), game="fo4")
    report = editor.session.run_conflict_scan().reports[0]
    editor._conflict_scan = ConflictScan(by_record_key={
        (report.winner.plugin_handle, i): replace(report, form_id=i, editor_id=f"B21_{i}")
        for i in range(4096)
    })
    release = Event()
    original = editor._build_overview
    def delayed(*args):
        assert release.wait(timeout=5)
        return original(*args)
    monkeypatch.setattr(editor, "_build_overview", delayed)
    assert editor._overview_reports() == []
    future = editor._overview_future[1]
    editor._overview_query = "no_such_record"
    release.set()
    future.result(timeout=5)
    editor.poll()
    assert editor._overview_cache is None
    assert editor._overview_reports() == []
    editor._overview_future[1].result(timeout=5)
    editor.poll()
    assert editor._overview_reports() == [] and editor._overview_future is None
    assert editor._overview_totals[1] == (4096, 4096)


def test_group_failure_is_visible_and_invalidated_before_retry(editor, tmp_path):
    plugin = editor.session.load(plugin_file(tmp_path, "Retry.esp"), game="fo4")
    key = (plugin.handle, "MISC")
    editor._cached_group_records[key] = None
    failure = Future()
    failure.set_exception(OSError("synthetic read failure"))
    editor._group_futures[key] = failure
    editor.poll()
    assert editor._group_errors[key] == "synthetic read failure"
    assert editor._cached_group_records[key] == []
    editor._invalidate_plugin_tree(plugin.handle)
    assert key not in editor._group_errors and key not in editor._cached_group_records


def test_native_header_refresh_keeps_string_master_names_and_slot_flags(editor, tmp_path):
    plugin_file(tmp_path, "Base.esm")
    plugin = editor.session.load(plugin_file(tmp_path, "Header.esp", masters=("Base.esm",)), game="fo4")
    editor._invalidate_plugin_tree(plugin.handle)
    editor._header_futures[plugin.handle].result(timeout=5)
    editor.poll()
    assert editor._latest_error is None
    assert editor._plugin_masters[plugin.handle] == ("Base.esm",)
    assert editor._plugin_flags[plugin.handle] == 0


def test_edit_invalidates_completed_analysis_and_cancels_pending_batches(editor, tmp_path):
    plugin = editor.session.load(plugin_file(tmp_path, "Edited.esp"), game="fo4")
    editor._scan_complete = True
    editor._analysis_future = Future()
    editor._analysis_queue = [["MISC"]]
    editor._after_record_edit(plugin.handle, 0x800)
    assert not editor._scan_complete and editor._conflict_scan is None
    assert editor._analysis_cancelled and not editor._analysis_queue


def test_manual_analysis_cancels_queued_auto_scan_without_false_error(editor, tmp_path):
    plugin = editor.session.load(plugin_file(tmp_path, "Manual.esp"), game="fo4")
    queued = Future()
    editor._auto_conflict_future = queued
    editor.start_override_analysis()
    assert queued.cancelled() and editor._auto_conflict_future is None
    editor._analysis_future.result(timeout=5)
    editor.poll()
    assert editor._latest_error is None
    assert "partial analysis" in editor._plugin_tooltip(plugin)


@pytest.mark.parametrize("value", ["00000800", "0x00000800", "0X00000800"])
def test_formid_widget_accepts_optional_prefix_without_loss(editor, monkeypatch, value):
    field = Field("Reference", "DATA", FieldKind.FORMID, 0, b"\0" * 4)
    for name in ("text", "text_disabled", "same_line", "set_next_item_width"):
        monkeypatch.setattr(app_module.imgui, name, lambda *args: None)
    monkeypatch.setattr(app_module.imgui, "input_text", lambda *args, **kwargs: (True, value))
    changed = []
    editor._draw_formid_widget(field, "##test", lambda: changed.append(True))
    assert field.value == 0x800 and changed
