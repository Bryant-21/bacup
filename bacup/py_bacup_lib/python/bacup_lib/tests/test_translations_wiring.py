from pathlib import Path
from types import SimpleNamespace

from bacup_lib import inspect_ui
from bacup_lib.translations import TRANSLATIONS
from bacup_lib.workflows import unified


def _capture_merge_calls(monkeypatch):
    calls = []

    def fake(ctx, runner=None):
        calls.append((ctx, runner))

    monkeypatch.setattr(unified, "_merge_packaged_tales_translations", fake)
    return calls


def _assert_accumulator_outside_archive_data_root(ctx):
    accumulator = Path(ctx.mod_path) / TRANSLATIONS
    assert not accumulator.is_relative_to(Path(ctx.mod_path) / "data")
    assert accumulator == Path(ctx.mod_path) / "F4SE/Plugins/B21_TalesFromAppalachia_en.txt"


def test_inspect_wrapper_calls_merge_packaged_once(tmp_path, monkeypatch):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / inspect_ui.MENU).write_bytes(b"stub")
    monkeypatch.setattr(inspect_ui, "convert_inspect_ui", lambda *a, **k: {"files": ["movie", "fonts"]})
    calls = _capture_merge_calls(monkeypatch)
    ctx = SimpleNamespace(source_data_dir=interface.parent, mod_path=tmp_path / "converted")
    request = SimpleNamespace(source_game="fo76", target_game="fo4")

    assert unified._convert_fo76_inspect_ui(request, ctx) == 2

    assert len(calls) == 1
    assert calls[0][0] is ctx
    _assert_accumulator_outside_archive_data_root(ctx)
