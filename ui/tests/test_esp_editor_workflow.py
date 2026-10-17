"""Safe synthetic plugin workflows: no game paths or user files."""
from __future__ import annotations

import struct
from concurrent.futures import Future
from types import SimpleNamespace

import pytest

from creation_lib.esp.editor.fields import Field, FieldKind
from creation_lib.esp.editor.session import LoadedPlugin
from creation_lib.esp.native_runtime import (
    RecordSummary, plugin_handle_add_record_raw, plugin_handle_call,
    plugin_handle_close, plugin_handle_get, plugin_handle_load, plugin_handle_new,
    plugin_handle_record_subrecords,
)
from creation_lib.esp.schema.base import FieldSpec, SubrecordSpec
from creation_lib.esp.schema.kinds import FieldKind as SchemaKind
from ui.esp_editor import app as app_module
from ui.esp_editor.app import EspEditorApp, _SaveEntry
from ui.esp_editor.record_view import (
    comparison_rows, edit_reason, encode_edit, load_document, winning_document,
)


@pytest.fixture
def editor():
    app = EspEditorApp()
    yield app
    app.cleanup()


def plugin_file(directory, name, *, masters=(), value=1, form_id=0x800, flags=0, signature="MISC"):
    handle = plugin_handle_new(name, "fo4")
    try:
        if name.casefold().endswith(".esm"):
            plugin_handle_call(handle, "set_header_flags", 1)
        for master in masters:
            plugin_handle_call(handle, "add_master", master)
        plugin_handle_add_record_raw(handle, signature, form_id, flags, 0, 131, 0, [
            ("EDID", b"B21_Fixture\0", None),
            ("FULL", b"Fixture value\0", None),
            ("DATA", struct.pack("<if", value, 2.5), None),
            ("MODF", b"\x80", None),
            ("ZZZZ", b"opaque untouched payload", None),
        ])
        path = directory / name
        plugin_handle_call(handle, "save", str(path))
        return path
    finally:
        plugin_handle_close(handle)


def wait_selection(app):
    assert app._selection_future is not None
    app._selection_future.result(timeout=10)
    app.poll()
    assert app.selection.error is None
    assert app.selection.fields is not None


def test_masters_precede_dependent_and_selected_chain_uses_local_ids(editor, tmp_path):
    plugin_file(tmp_path, "Base.esm")
    plugin_file(tmp_path, "Other.esm", form_id=0x900)
    path = plugin_file(tmp_path, "Patch.esp", masters=("Other.esm", "Base.esm"),
                       form_id=0x01000800, value=9)
    editor.session._master_search_paths = [tmp_path]
    patch = editor.session.load(path, game="fo4")
    plugins = editor.session.plugins
    assert [p.plugin_name for p in plugins] == ["Other.esm", "Base.esm", "Patch.esp"]
    assert [p.load_order_index for p in plugins] == [0, 1, 2]
    editor._select_record(patch.handle, RecordSummary(0x01000800, "MISC", "B21_Fixture"))
    wait_selection(editor)
    docs = editor.selection.documents
    assert [(d.plugin_name, d.summary.form_id) for d in docs] == [("Base.esm", 0x800), ("Patch.esp", 0x01000800)]
    assert winning_document(docs).handle == patch.handle
    assert editor.session.resolve_form_id(0x800, source_handle=plugins[1].handle)[0] == patch.handle
    assert editor.session.form_key(patch.handle, 0x01000800) == ("base.esm", 0x800)


def test_edit_undo_redo_save_reopen_preserves_opaque_data(editor, tmp_path):
    path = plugin_file(tmp_path, "Editable.esp")
    plugin = editor.session.load(path, game="fo4")
    editor._select_record(plugin.handle, RecordSummary(0x800, "MISC", "B21_Fixture"))
    wait_selection(editor)
    fields = editor.selection.fields
    index = next(i for i, f in enumerate(fields) if f.signature == "DATA")
    fields[index].components[0].value = 321
    fields[index].components[1].value = 4.75
    editor._commit_field(index, fields[index])
    assert struct.unpack("<if", plugin_handle_record_subrecords(plugin.handle, 0x800)[2][1]) == (321, 4.75)
    editor.undo()
    assert struct.unpack("<if", plugin_handle_record_subrecords(plugin.handle, 0x800)[2][1]) == (1, 2.5)
    editor.redo()
    assert len(editor._undo) == 1
    assert plugin.handle in editor._dirty_handles
    editor._save_popup_entries = [_SaveEntry(plugin.handle, plugin.plugin_name, str(path))]
    editor._execute_save_selected()
    editor._busy_future.result(timeout=10)
    editor.poll()
    assert plugin.handle not in editor._dirty_handles
    assert path.with_suffix(".esp.bak").is_file()
    editor._close_plugin(plugin.handle)
    assert editor.selection.record is None
    reopened = editor.session.load(path, game="fo4")
    editor._select_record(reopened.handle, RecordSummary(0x800, "MISC", "B21_Fixture"))
    wait_selection(editor)
    data = editor.selection.document.subrecords
    assert struct.unpack("<if", data[2][1]) == (321, 4.75)
    assert data[-1] == ("ZZZZ", b"opaque untouched payload", None)


def test_scalar_flags_use_member_schema_and_keep_unknown_bits(editor, tmp_path):
    path = plugin_file(tmp_path, "Flags.esp")
    plugin = editor.session.load(path, game="fo4")
    doc = load_document(plugin, RecordSummary(0x800, "MISC"))
    flags = next(f for f in doc.fields if f.signature == "MODF")
    assert flags.kind == FieldKind.FLAGSET
    assert flags.flag_options
    assert not edit_reason(flags)
    flags.value |= 1
    assert encode_edit(flags) == b"\x81"


def test_overflow_edit_restores_values_without_dirty_or_history(editor, tmp_path):
    path = plugin_file(tmp_path, "Overflow.esp")
    plugin = editor.session.load(path, game="fo4")
    editor._select_record(plugin.handle, RecordSummary(0x800, "MISC"))
    wait_selection(editor)
    fld = editor.selection.fields[2]
    fld.components[0].value = 2**40
    editor._commit_field(2, fld)
    assert editor.selection.fields[2].components[0].value == 1
    assert not editor._undo
    assert not editor._dirty_handles
    assert "Unable to edit" in editor._latest_error


def test_backup_failure_keeps_dirty_and_does_not_save(editor, monkeypatch, tmp_path):
    plugin = LoadedPlugin(22, str(tmp_path / "Keep.esp"), "fo4", False, 0, "Keep.esp")
    editor.session._plugins = [plugin]
    editor._dirty_handles.add(22)
    editor._save_popup_entries = [_SaveEntry(22, plugin.plugin_name, plugin.path)]
    def fail_backup(target):
        raise PermissionError("backup denied")
    calls = []
    monkeypatch.setattr(app_module, "_make_backup", fail_backup)
    monkeypatch.setattr(app_module, "plugin_handle_call", lambda *args: calls.append(args))
    editor._execute_save_selected()
    editor._busy_future.result(timeout=5)
    editor.poll()
    assert not calls
    assert 22 in editor._dirty_handles
    assert "backup denied" in editor._latest_error
    editor.session._plugins.clear()


def test_failed_native_edit_retains_document_and_history(editor, monkeypatch, tmp_path):
    path = plugin_file(tmp_path, "Rejected.esp")
    plugin = editor.session.load(path, game="fo4")
    editor._select_record(plugin.handle, RecordSummary(0x800, "MISC"))
    wait_selection(editor)
    monkeypatch.setattr(app_module, "plugin_handle_set_record_subrecords", lambda *args: False)
    editor._apply_field_edit(0, editor.selection.fields[0], "B21_Changed")
    assert editor.selection.fields[0].value == "B21_Fixture"
    assert not editor._dirty_handles
    assert not editor._undo


def test_latest_selection_and_cancel_reject_stale_results(editor, monkeypatch):
    plugin = LoadedPlugin(1, "/fake/Test.esp", "fo4", False, 0, "Test.esp")
    editor.session._plugins = [plugin]
    futures = []
    def submit(*args):
        future = Future()
        future.set_running_or_notify_cancel()
        futures.append(future)
        return future
    monkeypatch.setattr(editor._detail_executor, "submit", submit)
    first = RecordSummary(0x800, "MISC")
    second = RecordSummary(0x801, "MISC")
    editor._select_record(1, first)
    editor._select_record(1, first)
    assert len(futures) == 1
    editor._select_record(1, second)
    futures[0].set_result(([], []))
    editor.poll()
    assert editor.selection.record is second
    assert editor.selection.fields is None
    editor.cancel_selection()
    futures[1].set_exception(RuntimeError("stale failure"))
    editor.poll()
    assert editor.selection.error.startswith("Loading cancelled")
    assert editor._latest_error is None
    editor.session._plugins.clear()


def test_repeated_selection_reuses_bounded_cache(editor, tmp_path, monkeypatch):
    path = plugin_file(tmp_path, "Cached.esp")
    plugin = editor.session.load(path, game="fo4")
    summary = RecordSummary(0x800, "MISC")
    editor._select_record(plugin.handle, summary)
    wait_selection(editor)
    def unexpected(*args):
        pytest.fail("A repeated cached selection should not fetch native details")
    monkeypatch.setattr(editor._detail_executor, "submit", unexpected)
    editor._select_record(plugin.handle, summary)
    editor.selection = app_module._Selection()
    editor._select_record(plugin.handle, summary)
    assert editor.selection.fields is not None


def test_dirty_close_requires_decision_and_busy_close_is_ignored(editor, tmp_path):
    path = plugin_file(tmp_path, "Dirty.esp")
    plugin = editor.session.load(path, game="fo4")
    editor._dirty_handles.add(plugin.handle)
    editor.close_active()
    assert editor.session.get_by_handle(plugin.handle) is plugin
    assert editor._close_popup_requested
    editor._busy = True
    editor.close_active()
    assert editor.session.get_by_handle(plugin.handle) is plugin
    editor._busy = False


def test_filter_cache_and_clipper_bound_large_group_work(editor, monkeypatch):
    records = [RecordSummary(i, "MISC", f"B21_{i}") for i in range(100_000)]
    calls = []
    monkeypatch.setattr(editor, "_draw_record_leaf", lambda h, r: calls.append(r.form_id))
    class Clipper:
        display_start, display_end = 5000, 5020
        def begin(self, count):
            self.count = count
            self.pending = bool(count)
        def step(self):
            pending, self.pending = self.pending, False
            return pending
        def end(self):
            pass
    monkeypatch.setattr(app_module.imgui, "ListClipper", Clipper)
    editor._cached_group_records[(1, "MISC")] = records
    editor._draw_group_records((1, "MISC"), records)
    assert not calls  # the first large-group sort runs off the draw path
    editor._filter_futures[(1, "MISC")][1].result(timeout=5)
    editor.poll()
    editor._draw_group_records((1, "MISC"), records)
    assert len(calls) == 20
    filtered = editor._filter_group_records((1, "MISC"), records)
    assert editor._filter_group_records((1, "MISC"), records) is filtered
    editor._filter_text = "B21_99999"
    editor._cached_group_records[(1, "MISC")] = records
    assert editor._filter_group_records((1, "MISC"), records) == []
    editor._filter_futures[(1, "MISC")][1].result(timeout=5)
    editor.poll()
    assert [r.form_id for r in editor._filter_group_records((1, "MISC"), records)] == [99999]


def test_large_filter_stale_results_and_close_are_discarded(editor):
    records = [RecordSummary(i, "MISC", f"B21_{i}") for i in range(5000)]
    key = (1, "MISC")
    editor._cached_group_records[key] = records
    editor._filter_text = "4999"
    editor._filter_group_records(key, records)
    old = editor._filter_futures[key][1]
    old.result(timeout=5)
    editor._filter_text = "4998"
    editor.poll()
    assert key not in editor._filtered_records
    assert editor._filter_group_records(key, records) == []
    editor._filter_futures[key][1].result(timeout=5)
    editor._invalidate_plugin_tree(1)
    editor.poll()
    assert key not in editor._filter_futures and key not in editor._filtered_records


def test_large_load_order_defers_global_scan_but_explicit_scan_runs(editor):
    editor.session._plugins = [LoadedPlugin(1, "/fixture/Large.esm", "fo4", True, 0, "Large.esm")]
    editor._plugin_record_counts[1] = 1_000_000
    calls = []
    editor.session.run_conflict_scan = lambda: calls.append(True) or app_module.ConflictScan()
    editor._start_auto_conflict_scan()
    assert editor._auto_conflict_deferred and editor._auto_conflict_future is None and not calls
    editor.run_conflict_scan()
    editor._busy_future.result(timeout=5)
    editor.poll()
    assert calls == [True] and not editor._auto_conflict_deferred
    editor.session._plugins.clear()


@pytest.mark.parametrize("kind,codec,value,raw", [
    (FieldKind.INT, "uint8", 256, b"\x01"),
    (FieldKind.STRING, "fixed_string:3", "Too long", b"abc"),
    (FieldKind.STRING, "zstring", "emoji \U0001F600", b"ok\0"),
    (FieldKind.FLOAT, "float32", float("nan"), struct.pack("<f", 1)),
])
def test_strict_encoding_rejects_lossy_or_invalid_inputs(kind, codec, value, raw):
    spec = SubrecordSpec("DATA", SchemaKind.RAW, codec=codec)
    fld = Field("Test", "DATA", kind, value, raw, spec)
    with pytest.raises((ValueError, OverflowError, struct.error)):
        encode_edit(fld)


def test_float64_and_struct_tails_preserved():
    spec = SubrecordSpec("DATA", SchemaKind.RAW, codec="float64")
    fld = Field("Double", "DATA", FieldKind.FLOAT, 3.25, struct.pack("<d", 1) + b"tail", spec)
    assert encode_edit(fld) == struct.pack("<d", 3.25) + b"tail"


def test_comparison_distinguishes_repeated_and_missing_subrecords(editor, tmp_path):
    path = plugin_file(tmp_path, "Compare.esp")
    plugin = editor.session.load(path, game="fo4")
    doc = load_document(plugin, RecordSummary(0x800, "MISC"))
    from copy import deepcopy
    other = deepcopy(doc)
    other.fields.append(deepcopy(other.fields[0]))
    rows = comparison_rows([doc, other])
    duplicate = next(row for row in rows if row[0] == ("EDID", 1))
    assert duplicate[2] == [None, "B21_Fixture"]


def test_partial_record_cannot_displace_full_winner():
    docs = [SimpleNamespace(is_partial=False), SimpleNamespace(is_partial=True)]
    assert winning_document(docs) is docs[0]


def test_save_as_updates_identity_and_preserves_old_file(editor, tmp_path, monkeypatch):
    path = plugin_file(tmp_path, "Old.esp")
    target = tmp_path / "New.esp"
    plugin = editor.session.load(path, game="fo4")
    before = path.read_bytes()
    monkeypatch.setattr(app_module, "pick_save_file", lambda **kwargs: str(target))
    editor.save_active(save_as=True)
    editor._busy_future.result(timeout=10)
    editor.poll()
    assert plugin.path == str(target.resolve())
    assert plugin.plugin_name == "New.esp"
    assert plugin_handle_get(plugin.handle, "plugin_name") == "New.esp"
    assert path.read_bytes() == before
    assert target.is_file()


def test_owned_record_fallback_cannot_impersonate_a_master_override(editor, tmp_path):
    plugin_file(tmp_path, "Base.esm")
    # This owns 0x800; it does not override Base.esm:0x800. The native summary
    # API's object-ID fallback must not turn it into a false winning override.
    path = plugin_file(tmp_path, "Own.esp", masters=("Base.esm",), form_id=0x01000800, value=99)
    owner = editor.session.load(path, game="fo4")
    base = editor.session.get_by_name("Base.esm")
    editor._select_record(base.handle, RecordSummary(0x800, "MISC"))
    wait_selection(editor)
    assert [d.plugin_name for d in editor.selection.documents] == ["Base.esm"]
    assert editor.session.resolve_form_id(0x800, source_handle=base.handle)[0] == base.handle
    assert editor.session.resolve_form_id(0x01000800, source_handle=owner.handle)[0] == owner.handle


def test_copy_uses_selected_source_instead_of_later_winner(editor, tmp_path):
    plugin_file(tmp_path, "Base.esm", value=12)
    plugin_file(tmp_path, "Winner.esp", masters=("Base.esm",), value=98)
    target_path = plugin_file(tmp_path, "Target.esp", form_id=0x900)
    source = editor.session.load(tmp_path / "Base.esm", game="fo4")
    editor.session.load(tmp_path / "Winner.esp", game="fo4")
    target = editor.session.load(target_path, game="fo4")
    editor._do_copy(source.handle, 0x800, target.handle, as_new=False, deep=False)
    editor._busy_future.result(timeout=10)
    editor.poll()
    assert editor._latest_error is None
    data = plugin_handle_record_subrecords(target.handle, 0x800)
    assert struct.unpack("<if", next(raw for sig, raw, _ in data if sig == "DATA"))[0] == 12
    assert target.handle in editor._dirty_handles


def test_native_conflict_reports_keep_colliding_winner_raw_ids(editor, tmp_path):
    plugin_file(tmp_path, "Other.esm", form_id=0x900)
    plugin_file(tmp_path, "A.esm", value=1)
    plugin_file(tmp_path, "B.esm", value=2)
    plugin_file(tmp_path, "APatch.esp", masters=("Other.esm", "A.esm"), form_id=0x01000800, value=3)
    plugin_file(tmp_path, "BPatch.esp", masters=("Other.esm", "B.esm"), form_id=0x01000800, value=4)
    editor.session.load(tmp_path / "APatch.esp", game="fo4")
    editor.session.load(tmp_path / "BPatch.esp", game="fo4")
    scan = editor.session.run_conflict_scan()
    assert len(scan) == 2
    assert len(scan.reports_for("MISC")) == 2
    assert len(scan.by_record_key) == 2
    assert {r.winner.plugin_name for r in scan} == {"APatch.esp", "BPatch.esp"}


def test_import_order_drops_old_scan_then_accepts_current_result(editor, tmp_path, monkeypatch):
    from creation_lib.esp.editor.conflicts import ConflictScan
    editor.session._plugins = [LoadedPlugin(1, "/fake/A.esp", "fo4", False, 0, "A.esp")]
    old = Future()
    old.set_running_or_notify_cancel()
    editor._auto_conflict_future = old
    editor._auto_conflict_stamp = editor._view_stamp()
    editor._content_revision += 1
    monkeypatch.setattr(editor.session, "run_conflict_scan", lambda: ConflictScan())
    editor._start_auto_conflict_scan()
    stale = ConflictScan()
    old.set_result(stale)
    editor.poll()
    assert editor._conflict_scan is None
    editor._auto_conflict_future.result(timeout=5)
    editor.poll()
    assert editor._conflict_scan is not stale
    editor.session._plugins.clear()


def test_save_failure_preserves_destination_and_dirty_state(editor, tmp_path, monkeypatch):
    path = tmp_path / "Keep.esp"
    path.write_bytes(b"previous bytes")
    editor._dirty_handles.add(5)
    editor._save_popup_entries = [_SaveEntry(5, "Keep.esp", str(path))]
    def fail(*args):
        raise OSError("write denied")
    monkeypatch.setattr(app_module, "plugin_handle_call", fail)
    editor._execute_save_selected()
    editor._busy_future.result(timeout=5)
    editor.poll()
    assert path.read_bytes() == b"previous bytes"
    assert path.with_suffix(".esp.bak").read_bytes() == b"previous bytes"
    assert 5 in editor._dirty_handles


def test_record_render_does_not_export_or_decode_each_frame(editor, tmp_path, monkeypatch):
    path = plugin_file(tmp_path, "Render.esp")
    plugin = editor.session.load(path, game="fo4")
    editor._select_record(plugin.handle, RecordSummary(0x800, "MISC"))
    wait_selection(editor)
    monkeypatch.setattr(app_module, "heading", lambda *args, **kwargs: None)
    for name in ("text_disabled", "separator"):
        monkeypatch.setattr(app_module.imgui, name, lambda *args: None)
    monkeypatch.setattr(app_module.imgui, "begin_tab_bar", lambda *args: False)
    def unexpected(*args):
        pytest.fail("Rendering a cached record must not export or fetch native data")
    monkeypatch.setattr(app_module, "plugin_handle_call", unexpected)
    monkeypatch.setattr(app_module, "plugin_handle_get", unexpected)
    for _ in range(120):
        editor.draw_record_view()


def test_partial_override_winning_semantics_in_session_scan_and_view(editor, tmp_path):
    plugin_file(tmp_path, "Base.esm", signature="DIAL")
    path = plugin_file(tmp_path, "Partial.esp", signature="DIAL", masters=("Base.esm",), flags=1 << 14)
    partial = editor.session.load(path, game="fo4")
    base = editor.session.get_by_name("Base.esm")
    editor._select_record(partial.handle, RecordSummary(0x800, "DIAL"))
    wait_selection(editor)
    assert winning_document(editor.selection.documents).handle == base.handle
    assert editor.selection.documents[-1].is_partial
    assert editor.session.resolve_form_id(0x800, source_handle=partial.handle)[0] == base.handle
    scan = editor.session.run_conflict_scan()
    assert scan.report_for(partial.handle, 0x800).winner.plugin_handle == base.handle


def test_tree_invalidation_ignores_running_stale_results(editor):
    stale_tree, stale_group = Future(), Future()
    stale_tree.set_running_or_notify_cancel()
    stale_group.set_running_or_notify_cancel()
    editor._tree_futures[7] = stale_tree
    editor._group_futures[(7, "MISC")] = stale_group
    editor._invalidate_plugin_tree(7)
    stale_tree.set_result([("MISC", 100)])
    stale_group.set_result([RecordSummary(0x800, "MISC")])
    editor.poll()
    assert 7 not in editor._cached_groups
    assert (7, "MISC") not in editor._cached_group_records


def test_enum_picker_preserves_unknown_value_until_a_known_value_is_chosen(editor, monkeypatch):
    fld = Field("Enum", "TNAM", FieldKind.ENUM, 99, b"", enum_options=[(0, "None"), (2, "Weapon")])
    changes = []
    monkeypatch.setattr(app_module.imgui, "set_next_item_width", lambda *args: None)
    monkeypatch.setattr(app_module.imgui, "combo", lambda *args: (True, 0))
    editor._draw_enum_widget(fld, "##test", lambda: changes.append(fld.value))
    assert fld.value == 99 and not changes
    monkeypatch.setattr(app_module.imgui, "combo", lambda *args: (True, 2))
    editor._draw_enum_widget(fld, "##test", lambda: changes.append(fld.value))
    assert fld.value == 2 and changes == [2]


def test_native_enum_edit_survives_reopen(editor, tmp_path):
    path = plugin_file(tmp_path, "Enum.esp", signature="KYWD")
    plugin = editor.session.load(path, game="fo4")
    payload = plugin_handle_record_subrecords(plugin.handle, 0x800)
    payload.append(("TNAM", struct.pack("<I", 99), None))
    plugin_handle_call(plugin.handle, "set_record_subrecords", 0x800, payload)
    editor._select_record(plugin.handle, RecordSummary(0x800, "KYWD"))
    wait_selection(editor)
    fld = editor.selection.fields[-1]
    assert fld.kind == FieldKind.ENUM and fld.value == 99
    chosen = fld.enum_options[0][0]
    editor._apply_field_edit(len(editor.selection.fields) - 1, fld, chosen)
    plugin_handle_call(plugin.handle, "save", str(path))
    handle = plugin_handle_load(str(path), game="fo4")
    try:
        assert struct.unpack("<I", plugin_handle_record_subrecords(handle, 0x800)[-1][1])[0] == chosen
    finally:
        plugin_handle_close(handle)


def modal_buttons(monkeypatch, button):
    monkeypatch.setattr(app_module, "prepare_dialog", lambda *args: None)
    monkeypatch.setattr(app_module.imgui, "begin_popup_modal", lambda *args: (True, None))
    monkeypatch.setattr(app_module.imgui, "button", lambda label, *args: label == button)
    monkeypatch.setattr(app_module.imgui, "checkbox", lambda label, value: (False, value))
    monkeypatch.setattr(app_module.imgui, "set_next_window_size_constraints", lambda *args: None)
    for name in ("text", "text_wrapped", "separator", "same_line", "open_popup", "end_popup", "close_current_popup"):
        monkeypatch.setattr(app_module.imgui, name, lambda *args: None)


def test_cancel_save_popup_starts_no_write_and_retains_dirty(editor, monkeypatch):
    modal_buttons(monkeypatch, "Cancel")
    editor._save_popup_open = True
    editor._dirty_handles.add(7)
    editor._save_popup_entries = [_SaveEntry(7, "Keep.esp", "/fake/Keep.esp")]
    editor._draw_save_popup()
    assert editor._busy_future is None
    assert 7 in editor._dirty_handles


def test_cancel_unsaved_close_keeps_plugin_and_dirty_edits(editor, tmp_path, monkeypatch):
    path = plugin_file(tmp_path, "Keep.esp")
    plugin = editor.session.load(path, game="fo4")
    editor._dirty_handles.add(plugin.handle)
    editor.close_active()
    modal_buttons(monkeypatch, "Cancel")
    editor._draw_close_popup()
    assert editor._busy_future is None
    assert editor.session.get_by_handle(plugin.handle) is plugin
    assert plugin.handle in editor._dirty_handles


def test_save_and_close_failure_keeps_plugin_and_dirty_state(editor, tmp_path, monkeypatch):
    path = plugin_file(tmp_path, "Keep.esp")
    original = path.read_bytes()
    plugin = editor.session.load(path, game="fo4")
    editor._dirty_handles.add(plugin.handle)
    editor.close_active()
    modal_buttons(monkeypatch, "Save and close")
    def fail(*args):
        raise OSError("save denied")
    monkeypatch.setattr(app_module, "plugin_handle_call", fail)
    editor._draw_close_popup()
    with pytest.raises(OSError):
        editor._busy_future.result(timeout=5)
    editor.poll()
    assert editor.session.get_by_handle(plugin.handle) is plugin
    assert plugin.handle in editor._dirty_handles
    assert path.read_bytes() == original


@pytest.mark.parametrize("raw", [b"x\0", struct.pack("<I", 42)])
def test_localized_payload_is_readonly_even_if_malformed(editor, tmp_path, raw):
    path = plugin_file(tmp_path, "Localized.esp")
    plugin = editor.session.load(path, game="fo4")
    plugin_handle_call(plugin.handle, "set_header_flags", 0x80)
    payload = plugin_handle_record_subrecords(plugin.handle, 0x800)
    full_index = next(i for i, item in enumerate(payload) if item[0] == "FULL")
    payload[full_index] = ("FULL", raw, None)
    plugin_handle_call(plugin.handle, "set_record_subrecords", 0x800, payload)
    editor._select_record(plugin.handle, RecordSummary(0x800, "MISC"))
    wait_selection(editor)
    doc = editor.selection.document
    assert doc.is_localized and doc.fields[full_index].kind == FieldKind.LSTRING
    assert "Localized" in doc.edit_errors[full_index]
    editor._apply_field_edit(full_index, doc.fields[full_index], 99)
    assert plugin_handle_record_subrecords(plugin.handle, 0x800)[full_index][1] == raw
    assert not editor._dirty_handles and not editor._undo
    assert "Localized" in doc.edit_errors[full_index]  # rejection/restore retains guard


def test_selector_only_and_nested_complex_fields_are_readonly():
    spec = SubrecordSpec("VMAD", SchemaKind.PARSED, codec="struct:h,h",
                         fields=(FieldSpec("Version", "int16"), FieldSpec("Payload", "raw", nested_fields=(FieldSpec("X", "uint32"),))))
    fld = Field("Scripts", "VMAD", FieldKind.BYTES, b"\0" * 4, b"\0" * 4, spec)
    assert "dedicated editor" in edit_reason(fld)
    spec = SubrecordSpec("DATA", SchemaKind.PARSED, codec="uint32", union_selector="Type")
    fld = Field("Conditional", "DATA", FieldKind.INT, 0, b"\0" * 4, spec)
    assert "Conditional" in edit_reason(fld)
