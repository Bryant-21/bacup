from __future__ import annotations

import threading
import time
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.workflows import unified


class _Runner:
    def __init__(self) -> None:
        self.logs: list[tuple[str, str]] = []
        self.items: list[tuple[int, int, str]] = []
        self.cancelled = False

    def is_cancelled(self) -> bool:
        return self.cancelled

    def emit_log(self, level: str, message: str) -> None:
        self.logs.append((level, message))

    def emit_item_progress(self, progress) -> None:
        self.items.append(
            (
                progress.completed_items,
                progress.total_items,
                progress.current_item,
            )
        )


def _driver(tmp_path: Path, *, source_game: str = "fo76", offsets: bool = True):
    options = SimpleNamespace(build_esp=True, rebuild_cell_offsets=offsets)
    request = SimpleNamespace(
        source_game=source_game,
        target_game="fo4",
        options=options,
    )
    ctx = SimpleNamespace(output_plugin_name="Output.esm")
    runtime = SimpleNamespace(
        _native_run_config=lambda _ctx: {"output_plugin_name": "Output.esm"}
    )
    return SimpleNamespace(_req=request, ctx=ctx, record_runtime=runtime)


def _fake_run(
    monkeypatch,
    calls: list,
    *,
    fail_at: str | None = None,
    runner: _Runner | None = None,
) -> None:
    native_cancelled = threading.Event()

    class FakeRun:
        @classmethod
        def open_existing(cls, *args, **kwargs):
            calls.append(("open", args, kwargs))
            return cls()

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            calls.append(("close",))

        def share_output_nif_dependencies(self, source):
            calls.append(("share", source))

        def run_phase(self, phase, **kwargs):
            calls.append(("phase", phase, kwargs))
            if fail_at == "cancel_during_phase" and phase == "finalize_plugin_records":
                native_cancelled.wait(timeout=1.0)
                raise RuntimeError("cancelled")
            if fail_at == "offset" and phase == "finalize_plugin_records":
                raise RuntimeError("rebuild cell offsets failed")
            if fail_at == "cancel_after_phase" and phase == "finalize_plugin_records":
                assert runner is not None
                runner.cancelled = True
            if phase == "emit_modt_manifest":
                return {"records_changed": 12, "warnings": 0}
            return {"records_changed": 5, "warnings": 1}

        def drain_events(self, _max):
            return [
                {
                    "kind": "log",
                    "level": "INFO",
                    "message": "final plugin child reports",
                }
            ]

        def save_target(self, path, **_kwargs):
            save_path = Path(path)
            calls.append(("save", save_path))
            strings = save_path.parent / "Strings"
            strings.mkdir(exist_ok=True)
            (strings / f"{save_path.stem}_en.STRINGS").write_bytes(b"orphan")
            if fail_at == "save":
                raise OSError("save failed")
            save_path.write_bytes(b"final plugin")
            if fail_at == "cancel_after_save":
                assert runner is not None
                runner.cancelled = True

        def cancel(self):
            calls.append(("cancel",))
            native_cancelled.set()

    monkeypatch.setattr("bacup_lib.run.ConversionRun", FakeRun)


@pytest.mark.parametrize(
    ("source_game", "offsets", "expected_term"),
    [("fo76", True, True), ("fo76", False, True), ("skyrimse", True, False)],
)
def test_final_plugin_batch_uses_one_session_and_preserves_operation_flags(
    tmp_path, monkeypatch, source_game, offsets, expected_term
):
    output = tmp_path / "Output.esm"
    output.write_bytes(b"built plugin")
    source = tmp_path / "Source.esm"
    source.write_bytes(b"source plugin")
    strings = tmp_path / "Strings"
    strings.mkdir()
    output_strings = strings / "Output_en.STRINGS"
    output_strings.write_bytes(b"built strings")
    calls: list = []
    _fake_run(monkeypatch, calls)
    runner = _Runner()
    progress = SimpleNamespace(
        total_items=0,
        completed_items=0,
        current_item="",
    )

    unified._finalize_plugin_records_after_asset_waves(
        _driver(tmp_path, source_game=source_game, offsets=offsets),
        runner,
        tmp_path,
        source,
        progress=progress,
        nif_run="nif-run",
    )

    opens = [call for call in calls if call[0] == "open"]
    assert len(opens) == 1
    assert opens[0][1][2] is None
    assert ("share", "nif-run") in calls
    phases = [call for call in calls if call[0] == "phase"]
    assert [call[1] for call in phases] == [
        "emit_modt_manifest",
        "finalize_plugin_records",
    ]
    params = phases[1][2]["params"]
    assert params["manifest_entries"] == 12
    assert params["repair_term_markers"] is expected_term
    assert params["rebuild_cell_offsets"] is offsets
    assert params["source_plugin_path"] == (str(source) if expected_term else None)
    assert len([call for call in calls if call[0] == "save"]) == 1
    assert output.read_bytes() == b"final plugin"
    assert not list(tmp_path.glob(".Output.esm.*.tmp"))
    assert not list((tmp_path / "Strings").glob(".Output.esm.*_en.STRINGS"))
    assert output_strings.read_bytes() == b"built strings"
    assert progress.completed_items == progress.total_items == 3
    assert runner.logs[-1] == (
        "INFO",
        "final plugin records published atomically: records_changed=5 warnings=1",
    )


@pytest.mark.parametrize("fail_at", ["offset", "save", "replace"])
def test_final_plugin_batch_failure_keeps_built_plugin_and_cleans_temps(
    tmp_path, monkeypatch, fail_at
):
    output = tmp_path / "Output.esm"
    output.write_bytes(b"built plugin")
    source = tmp_path / "Source.esm"
    source.write_bytes(b"source plugin")
    strings = tmp_path / "Strings"
    strings.mkdir()
    output_strings = strings / "Output_en.STRINGS"
    output_strings.write_bytes(b"built strings")
    calls: list = []
    _fake_run(monkeypatch, calls, fail_at=fail_at)
    if fail_at == "replace":
        monkeypatch.setattr(
            unified.os,
            "replace",
            lambda *_args: (_ for _ in ()).throw(OSError("replace failed")),
        )

    with pytest.raises((OSError, RuntimeError), match=f"{fail_at}|final phase"):
        unified._finalize_plugin_records_after_asset_waves(
            _driver(tmp_path),
            _Runner(),
            tmp_path,
            source,
            nif_run="nif-run",
        )

    assert output.read_bytes() == b"built plugin"
    assert output_strings.read_bytes() == b"built strings"
    assert len([call for call in calls if call[0] == "open"]) == 1
    assert (
        len(
            [
                call
                for call in calls
                if call[0] == "phase" and call[1] == "finalize_plugin_records"
            ]
        )
        == 1
    )
    assert len([call for call in calls if call[0] == "save"]) == (
        0 if fail_at == "offset" else 1
    )
    assert not list(tmp_path.glob(".Output.esm.*.tmp"))
    strings = tmp_path / "Strings"
    assert not strings.exists() or not list(strings.glob(".Output.esm.*_en.STRINGS"))


@pytest.mark.parametrize("cancel_at", ["cancel_after_phase", "cancel_after_save"])
def test_final_plugin_cancellation_before_publication_keeps_built_plugin(
    tmp_path, monkeypatch, cancel_at
):
    output = tmp_path / "Output.esm"
    output.write_bytes(b"built plugin")
    source = tmp_path / "Source.esm"
    source.write_bytes(b"source plugin")
    calls: list = []
    runner = _Runner()
    _fake_run(monkeypatch, calls, fail_at=cancel_at, runner=runner)

    with pytest.raises(RuntimeError, match="conversion cancelled"):
        unified._finalize_plugin_records_after_asset_waves(
            _driver(tmp_path),
            runner,
            tmp_path,
            source,
        )

    assert output.read_bytes() == b"built plugin"
    assert not list(tmp_path.glob(".Output.esm.*.tmp"))


def test_final_plugin_forwards_cancellation_to_running_native_phase(
    tmp_path, monkeypatch
):
    output = tmp_path / "Output.esm"
    output.write_bytes(b"built plugin")
    source = tmp_path / "Source.esm"
    source.write_bytes(b"source plugin")
    calls: list = []
    runner = _Runner()
    _fake_run(
        monkeypatch,
        calls,
        fail_at="cancel_during_phase",
        runner=runner,
    )

    trigger = threading.Thread(
        target=lambda: (time.sleep(0.02), setattr(runner, "cancelled", True))
    )
    trigger.start()
    with pytest.raises(RuntimeError, match="cancelled"):
        unified._finalize_plugin_records_after_asset_waves(
            _driver(tmp_path),
            runner,
            tmp_path,
            source,
        )
    trigger.join(timeout=1.0)

    assert ("cancel",) in calls
    assert output.read_bytes() == b"built plugin"
    assert not list(tmp_path.glob(".Output.esm.*.tmp"))
