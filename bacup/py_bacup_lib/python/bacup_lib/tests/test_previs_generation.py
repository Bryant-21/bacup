import json
from pathlib import Path

from bacup_lib import previs_generation
from bacup_lib.previs_generation import PrevisRequest, generate_previs


class FakeNative:
    def __init__(self, data_dir: Path):
        self.data_dir = data_dir
        self.finalized = False

    def prepare_previs(self, request_json, report_path, timing_path, workers=None, progress=None):
        request = json.loads(request_json)
        progress(1, 2)
        progress(2, 2)
        assert request["solve_into"] == str(self.data_dir)
        assert workers == 3
        # The stale tree is gone before the planner writes new tomes.
        assert not (self.data_dir / "Vis" / "Mod.esm" / "old.uvd").exists()
        uvd = self.data_dir / "Vis" / "Mod.esm" / "00000001.uvd"
        uvd.parent.mkdir(parents=True, exist_ok=True)
        uvd.write_bytes(b"UVD")
        self.plugin = Path(request["plugin"])
        self.report_path = Path(report_path)
        self.timing_path = Path(timing_path)
        self.report_path.parent.mkdir(parents=True, exist_ok=True)
        # The orchestration must not read or deserialize the diagnostic report.
        self.report_path.write_text("report owned by native code", encoding="utf-8")
        return self

    def finalize(self, year, month, day):
        assert (year, month, day) == (2026, 9, 26)
        self.finalized = True
        self.plugin.write_bytes(self.plugin.read_bytes() + b"+stamped")
        return json.dumps({
            "generated": 1, "stamped": 2, "patched_cells": 3,
            "failed": [["00000002", "solve failed: bad scene"]],
            "skipped_unsupported": 1, "skipped_invalid": 1,
            "warnings": ["previs: 00000002 solve failed: bad scene", "previs: invalid input skipped: broken"],
            "report_path": str(self.report_path), "timing_path": str(self.timing_path),
            "index": {"cdx_relative_path": "Mod.cdx", "cells": 3, "cells_with_visibility": 1},
        })


def test_rust_previs_writes_solved_tomes_and_stamps_only_their_cells(tmp_path, monkeypatch):
    plugin = tmp_path / "Mod.esm"
    plugin.write_bytes(b"TES4")
    data = tmp_path / "data"
    stale = data / "Vis" / "Mod.esm"
    stale.mkdir(parents=True)
    (stale / "old.uvd").write_bytes(b"old")
    native = FakeNative(data)
    monkeypatch.setattr(previs_generation, "_native", lambda: native)
    class DateAfterPlanning:
        @staticmethod
        def today():
            assert native.report_path.is_file()
            return type("Date", (), {"year": 2026, "month": 9, "day": 26})()

    monkeypatch.setattr(previs_generation._dt, "date", DateAfterPlanning)
    messages = []
    progress = []

    result = generate_previs(
        PrevisRequest(plugin_path=plugin, mod_data_dir=data, master_dirs=[], loose_roots=[],
                      archives=[], work_dir=tmp_path / "work", workers=3),
        log=lambda *message: messages.append(message),
        progress=lambda *item: progress.append(item),
    )

    assert (result.generated, result.stamped, result.patched_cells, result.skipped_unsupported) == (1, 2, 3, 1)
    assert result.skipped_invalid == 1
    assert result.failed == [("00000002", "solve failed: bad scene")]
    assert sorted(p.name for p in stale.iterdir()) == ["00000001.uvd"]
    assert native.finalized
    assert result.report_path == native.report_path
    assert (2, 2, "Solving visibility clusters (2/2)") in progress
    assert progress[-1] == (1, 1, "Stamping previs cells")
    assert any(message.startswith("previs: 2/2 clusters done (100%)") for _, message in messages)
    assert not any(message.startswith("previs: 1/2 clusters") for _, message in messages)
    assert ("WARN", "previs: invalid input skipped: broken") in messages
    assert any("1/3 CELL(s) with a UVD" in message for _, message in messages)
    assert plugin.read_bytes() == b"TES4+stamped"
