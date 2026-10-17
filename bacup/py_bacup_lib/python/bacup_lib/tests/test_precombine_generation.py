import json
import struct

from bacup_lib import precombine_generation as generation


def test_native_cleanup_only_removes_selected_plugin_outputs(tmp_path):
    data = tmp_path / "data"
    plugin = tmp_path / "Example.esm"
    removed = [
        data / "MeShEs/pReCoMbInEd/eXaMpLe.EsM",
        data / "vIs/EXAMPLE.esm",
    ]
    for directory in removed:
        (directory / "nested").mkdir(parents=True)
        (directory / "nested/generated.bin").write_bytes(b"old")
    keep = [
        data / "MeShEs/pReCoMbInEd/Other.esm/keep.nif",
        data / "vIs/Other.esm/keep.uvd",
        data / "example - geometry.csg.bak",
        data / "Other.cdx",
        plugin,
    ]
    for path in keep:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"keep")
    stale = [
        directory / name for directory in [data, plugin.parent]
        for name in ["EXAMPLE - Geometry.CSG", "Example - Geometry.PSG", "Example.CDX", "Example - Exterior.CDX"]
    ]
    for path in stale:
        path.write_bytes(b"old")
    assert generation.remove_existing_precombines(data, plugin) == 10
    assert all(not path.exists() for path in removed + stale)
    assert all(path.read_bytes() == b"keep" for path in keep)
    assert generation.remove_existing_precombines(data, plugin) == 0


def test_native_cleanup_does_not_follow_directory_junction(tmp_path):
    import sys
    import pytest

    if sys.platform != "win32":
        pytest.skip("Windows junction regression")
    import _winapi

    data = tmp_path / "data"
    selected = data / "Meshes/PreCombined/Example.esm"
    selected.mkdir(parents=True)
    outside = tmp_path / "other-output"
    outside.mkdir()
    sentinel = outside / "keep.nif"
    sentinel.write_bytes(b"keep")
    _winapi.CreateJunction(str(outside), str(selected / "linked"))
    assert generation.remove_existing_precombines(data, tmp_path / "Example.esm") == 1
    assert sentinel.read_bytes() == b"keep"
    _winapi.CreateJunction(str(outside), str(selected))
    assert generation.remove_existing_precombines(data, tmp_path / "Example.esm") == 1
    assert sentinel.read_bytes() == b"keep"


def test_native_cleanup_propagates_locked_file_error(tmp_path):
    import sys
    import pytest

    if sys.platform != "win32":
        pytest.skip("Windows file sharing regression")
    data = tmp_path / "data"
    selected = data / "Vis/Example.esm"
    selected.mkdir(parents=True)
    locked = selected / "locked.uvd"
    locked.write_bytes(b"locked")
    with locked.open("rb"):
        with pytest.raises(OSError):
            generation.remove_existing_precombines(data, tmp_path / "Example.esm")
    assert locked.read_bytes() == b"locked"
    assert generation.remove_existing_precombines(data, tmp_path / "Example.esm") == 1


def test_precombine_orchestration_never_decodes_cells_or_diagnostics(tmp_path, monkeypatch):
    from creation_lib._native import previs_native

    report_path = tmp_path / "work/precombine_report.json"
    timing_path = tmp_path / "work/precombine_timing.json"
    calls = []
    index_resources = object()

    class Prepared:
        def take_index_resources(self):
            assert calls == ["prepare", "finalize"]
            return index_resources

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
        progress(1024, 45564)
        progress(45564, 45564)
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
    assert result.index_resources is index_resources
    assert report_path.read_text() == "opaque diagnostic report owned by Rust"
    assert ("WARN", "precombine: invalid input skipped: bad mesh") in messages
    assert progress[0] == (0, 0, "Building precombined meshes")
    assert (45564, 45564, "Building precombined meshes (45564/45564)") in progress
    assert progress[-2] == (45564, 45564, "Stamping CELL precombine data")
    assert progress[-1] == (45564, 45564, "")
    assert any(message.startswith("precombine: 45564/45564 CELLs done (100%)") for _, message in messages)


def test_native_index_resources_transfer_once_between_preparations(tmp_path):
    from creation_lib._native import previs_native

    plugin = tmp_path / "Empty.esp"
    header = b"HEDR" + struct.pack("<HfII", 12, 1.0, 0, 0x800)
    plugin.write_bytes(b"TES4" + struct.pack("<IIIII", len(header), 0, 0, 0, 0) + header)
    data = tmp_path / "data"
    data.mkdir()
    common = {"plugin": str(plugin), "master_dirs": [], "loose_roots": [], "archives": []}
    prepared = previs_native.prepare_precombines(
        json.dumps({**common, "output_dir": str(data)}),
        str(tmp_path / "precombine_report.json"), str(tmp_path / "precombine_timing.json"), workers=1,
    )
    assert prepared.take_index_resources() is None
    assert json.loads(prepared.finalize(2026, 10, 4))["cells"] == 0
    resources = prepared.take_index_resources()
    assert resources is not None
    assert prepared.take_index_resources() is None
    for _ in range(2):
        previs = previs_native.prepare_previs(
            json.dumps({**common, "scene_dir": str(tmp_path / "scenes"), "solve_into": str(data)}),
            str(tmp_path / "previs_report.json"), str(tmp_path / "previs_timing.json"),
            workers=1, index_resources=resources,
        )
        result = json.loads(previs.finalize(2026, 10, 4))
        assert result["generated"] == result["patched_cells"] == 0
        assert result["index"] is None
