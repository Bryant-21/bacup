import json

from bacup_lib import precombine_generation as generation


def test_precombine_orchestration_never_decodes_cells_or_diagnostics(tmp_path, monkeypatch):
    from bacup_lib._native import previs_native

    report_path = tmp_path / "work/precombine_report.json"
    timing_path = tmp_path / "work/precombine_timing.json"
    calls = []

    class Prepared:
        def finalize(self, year, month, day):
            calls.append("finalize")
            assert (year, month, day) == (2026, 9, 26)
            return json.dumps({
                "cells": 3, "groups": 8, "excluded_references": 9,
                "patched_cells": 5, "cleared_cells": 2,
                "skipped_unsupported": 7, "skipped_invalid": ["bad mesh"],
                "report_path": str(report_path), "timing_path": str(timing_path),
                "index": {"cells": 3},
            })

    def prepare(request, report, timing, workers=None, progress=None):
        assert json.loads(request)["plugin"] == str(tmp_path / "Mod.esm")
        progress(1024, 2048)
        progress(2048, 2048)
        assert (report, timing, workers) == (str(report_path), str(timing_path), 2)
        report_path.parent.mkdir()
        report_path.write_text("opaque diagnostic report owned by Rust")
        calls.append("prepare")
        return Prepared()

    class Date:
        @staticmethod
        def today():
            assert calls == ["prepare"]
            return type("Date", (), {"year": 2026, "month": 9, "day": 26})()

    monkeypatch.setattr(previs_native, "prepare_precombines", prepare, raising=False)
    monkeypatch.setattr(generation._dt, "date", Date)
    messages, progress = [], []
    result = generation.generate_precombines(
        generation.PrecombineRequest(tmp_path / "Mod.esm", tmp_path / "data", [], [], [], tmp_path / "work", 2),
        log=lambda *item: messages.append(item), progress=lambda *item: progress.append(item),
    )
    assert calls == ["prepare", "finalize"]
    assert (result.cells, result.groups, result.cleared_cells, result.index) == (3, 8, 2, {"cells": 3})
    assert result.report_path == report_path and result.timing_path == timing_path
    assert report_path.read_text() == "opaque diagnostic report owned by Rust"
    assert ("WARN", "precombine: invalid input skipped: bad mesh") in messages
    assert (2048, 2048, "Building precombined meshes (2048/2048)") in progress
    assert progress[-1] == (2, 2, "")
    assert any(message.startswith("precombine: 2048/2048 CELLs done (100%)") for _, message in messages)
