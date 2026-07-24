"""run_unified lod_hook fires after asset join, before BA2 pack.

Also covers _run_generate_lod atlas-map cleanup (the .txt intermediate that
native lodgen writes alongside the atlas DDS must be removed so it cannot land
in a DX10 texture BA2).
"""
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import bacup_lib.workflows.unified as U
import creation_lib.lod.native_runtime as _lod_rt
from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.regen_pipeline import _run_generate_lod


def _make_request(tmp_path: Path) -> PluginPortRequest:
    plugin = tmp_path / "Test.esm"
    plugin.write_bytes(b"TES4")
    return PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[plugin],
        output_root=tmp_path / "out",
        target_extracted_dir=None,
        target_data_dir=None,
        options=PluginPortOptions(),
    )


class _FakeRunner:
    def __init__(self):
        self.phase_events = []
        self.item_events = []

    def emit_log(self, level, msg): pass
    def emit_complete(self, path, summary): pass
    def emit_phase_start(self, progress):
        self.phase_events.append(("start", progress.phase_name, progress.status))
    def emit_item_progress(self, progress):
        self.item_events.append(
            (
                progress.phase_name,
                progress.completed_items,
                progress.total_items,
                progress.current_item,
            )
        )
    def emit_phase_complete(self, progress):
        self.phase_events.append(("complete", progress.phase_name, progress.status))


def _make_runner() -> _FakeRunner:
    return _FakeRunner()


def test_lod_hook_fires_after_assets_before_pack(monkeypatch, tmp_path):
    order: list[str] = []
    collision_report_dirs: list[Path] = []
    mirror_paths: list[Path] = []

    # Stub the native sink module so no real packing happens.
    class _Native:
        def sinks_create(self, _cfg): return 1
        def sinks_streamed(self, _id): return []
        def sinks_add_files(self, *_a, **_k): return 0
        def sinks_abort(self, _id): pass
        def sinks_drop(self, _id): pass
        def sinks_cleanup_spills(self, _id): pass

    monkeypatch.setattr(U, "load_native_module", lambda: _Native())

    # Stub driver + asset track + finalize so we observe pure ordering.
    class _FakeAssetRuns:
        def __init__(self):
            self.dropped = False

        def drop_all(self):
            assert not self.dropped
            self.dropped = True
            order.append("drop_assets")

    def _fake_run_asset_track(*a, **k):
        order.append("assets")
        return _FakeAssetRuns()

    monkeypatch.setattr(U, "run_asset_track", _fake_run_asset_track)
    monkeypatch.setattr(
        U,
        "_regenerate_modt_after_asset_waves",
        lambda *_a, **_k: order.append("modt"),
    )

    def _fake_finalize(*_a, **_k):
        order.append("pack")
        emit_progress = _k["pack_progress"]
        emit_progress(
            {
                "completed": 0,
                "total": 24,
                "message": (
                    "Packing archive SeventySix - LOD.ba2.tmp (1/24) "
                    "files=6592"
                ),
            }
        )
        emit_progress(
            {
                "completed": 1,
                "total": 24,
                "message": (
                    "Archive packed native: name=SeventySix - LOD.ba2.tmp "
                    "files=6592"
                ),
            }
        )
        return []

    monkeypatch.setattr(U, "finalize_sinks_for_mod", _fake_finalize)
    def _fake_collision_validation(_meshes_root, report_dir, *_a, **_k):
        order.append("collision")
        collision_report_dirs.append(Path(report_dir))

    monkeypatch.setattr(U, "_run_collision_validation", _fake_collision_validation)
    monkeypatch.setattr(
        U,
        "write_cache_manifest",
        lambda *a, **k: order.append("manifest"),
    )
    monkeypatch.setattr(U, "collect_cache_entries", lambda *a, **k: [])

    class _FakeRecordRuntime:
        _aggregate_summary = object()
        run_result = object()

    class _FakeDriver:
        def __init__(self, *a, **k):
            self.record_runtime = _FakeRecordRuntime()
            self.signals = type("S", (), {
                "record_done": type("E", (), {"set": lambda s: None})()
            })()
            self.defer_asset_a2_until_record_done = False
            self.ctx = None

        def run_record_track(self, _runner):
            order.append("records")

        def emit_complete(self, _runner):
            pass

    monkeypatch.setattr(U, "UnifiedDriver", _FakeDriver)

    class _FakeMirror:
        def __init__(self, path, *a, **k):
            mirror_paths.append(Path(path))

        def start(self): pass
        def finish(self, _s): pass

    monkeypatch.setattr(U, "RunStateMirror", _FakeMirror)
    monkeypatch.setattr(
        U.AssetWaveToggles, "from_options",
        staticmethod(lambda _o: U.AssetWaveToggles())
    )

    request = _make_request(tmp_path)
    request.diagnostics_root = tmp_path / "diagnostics"
    request.options.validate_collision = True
    runner = _make_runner()

    def _hook(mod_root: Path):
        assert order == [
            "records",
            "assets",
            "modt",
            "drop_assets",
        ], f"Expected assets released, got {order}"
        order.append("lod")

    U.run_unified(request, runner, serialize_tracks=True, lod_hook=_hook)
    assert order == [
        "records",
        "assets",
        "modt",
        "drop_assets",
        "lod",
        "collision",
        "manifest",
        "pack",
    ], f"Unexpected order: {order}"
    assert mirror_paths == [request.diagnostics_root / "run_state.json"]
    assert collision_report_dirs == [request.diagnostics_root / "collision_validation"]
    assert runner.phase_events == [
        ("start", "Regenerate MODT", "running"),
        ("complete", "Regenerate MODT", "completed"),
        ("start", "Rebuild Cell Offsets", "running"),
        ("complete", "Rebuild Cell Offsets", "completed"),
        ("start", "Generate LOD", "running"),
        ("complete", "Generate LOD", "completed"),
        ("start", "Pack BA2", "running"),
        ("complete", "Pack BA2", "completed"),
    ]
    assert runner.item_events[-3:] == [
        ("Pack BA2", 0, 0, "Reconciling and planning BA2 archives"),
        ("Pack BA2", 0, 0, "SeventySix - LOD.ba2.tmp (1/24)"),
        ("Pack BA2", 1, 24, "Packed SeventySix - LOD.ba2.tmp"),
    ]


def test_lod_hook_none_no_change(monkeypatch, tmp_path):
    """When lod_hook is None, run_unified behaves identically to today."""
    order: list[str] = []

    class _Native:
        def sinks_create(self, _cfg): return 1
        def sinks_streamed(self, _id): return []
        def sinks_add_files(self, *_a, **_k): return 0
        def sinks_abort(self, _id): pass
        def sinks_drop(self, _id): pass
        def sinks_cleanup_spills(self, _id): pass

    monkeypatch.setattr(U, "load_native_module", lambda: _Native())

    def _fake_run_asset_track(*a, **k):
        order.append("assets")
        return type("AR", (), {"drop_all": lambda s: None})()

    monkeypatch.setattr(U, "run_asset_track", _fake_run_asset_track)
    monkeypatch.setattr(
        U,
        "_regenerate_modt_after_asset_waves",
        lambda *_a, **_k: order.append("modt"),
    )

    def _fake_finalize(*_a, **_k):
        order.append("pack")
        return []

    monkeypatch.setattr(U, "finalize_sinks_for_mod", _fake_finalize)
    monkeypatch.setattr(
        U,
        "_run_collision_validation",
        lambda *_a, **_k: (_ for _ in ()).throw(
            AssertionError("collision validation should be opt-in")
        ),
    )
    monkeypatch.setattr(
        U,
        "write_cache_manifest",
        lambda *a, **k: order.append("manifest"),
    )
    monkeypatch.setattr(U, "collect_cache_entries", lambda *a, **k: [])

    class _FakeRecordRuntime:
        _aggregate_summary = object()
        run_result = object()

    class _FakeDriver:
        def __init__(self, *a, **k):
            self.record_runtime = _FakeRecordRuntime()
            self.signals = type("S", (), {
                "record_done": type("E", (), {"set": lambda s: None})()
            })()
            self.defer_asset_a2_until_record_done = False
            self.ctx = None

        def run_record_track(self, _runner):
            order.append("records")

        def emit_complete(self, _runner):
            pass

    monkeypatch.setattr(U, "UnifiedDriver", _FakeDriver)

    class _FakeMirror:
        def __init__(self, *a, **k): pass
        def start(self): pass
        def finish(self, _s): pass

    monkeypatch.setattr(U, "RunStateMirror", _FakeMirror)
    monkeypatch.setattr(
        U.AssetWaveToggles, "from_options",
        staticmethod(lambda _o: U.AssetWaveToggles())
    )

    request = _make_request(tmp_path)
    runner = _make_runner()
    U.run_unified(request, runner, serialize_tracks=True, lod_hook=None)
    assert order == ["records", "assets", "modt", "manifest", "pack"]


# ---------------------------------------------------------------------------
# _run_generate_lod: atlas-map .txt cleanup
# ---------------------------------------------------------------------------

def test_run_generate_lod_removes_atlas_txt(monkeypatch, tmp_path):
    """generate_lod() leaves a <world>.Objects.txt alongside the atlas DDS.
    _run_generate_lod must delete every *.txt under
    Textures/Terrain/<world>/Objects/ so it cannot land in a DX10 BA2."""
    world = "APPALACHIA"
    mod_root = tmp_path / "SeventySix"
    mod_root.mkdir()

    # Pre-create the on-disk artefact that native lodgen would produce.
    obj_dir = mod_root / "data" / "Textures" / "Terrain" / world / "Objects"
    obj_dir.mkdir(parents=True)
    atlas_txt = obj_dir / f"{world}.Objects.txt"
    atlas_txt.write_text("fake atlas map")

    # A .dds sitting next to the .txt must survive.
    atlas_dds = obj_dir / f"{world}.Objects.dds"
    atlas_dds.write_bytes(b"DDS ")

    fake_result = SimpleNamespace(btr=1, bto=2, dds=3, lod_written=True, warnings=[])
    monkeypatch.setattr(_lod_rt, "generate_lod", lambda *a, **k: fake_result)

    logs: list[tuple[str, str]] = []
    _run_generate_lod(
        mod_root=mod_root,
        worldspaces=[world],
        working_esm=tmp_path / "SeventySix.esm",
        asset_dirs=[],
        settings={},
        runner_log=lambda level, msg: logs.append((level, msg)),
    )

    assert not atlas_txt.exists(), "atlas-map .txt should have been removed"
    assert atlas_dds.exists(), "atlas .dds must not be removed"
    removed = [msg for _lvl, msg in logs if "atlas-map intermediate" in msg]
    assert removed, f"expected removal log message; got logs={logs}"


def test_run_generate_lod_no_objects_dir_is_noop(monkeypatch, tmp_path):
    """If lodgen produced no Objects/ dir (e.g. zero BTO output), no error."""
    world = "TESTWORLD"
    mod_root = tmp_path / "Mod"
    mod_root.mkdir()

    fake_result = SimpleNamespace(btr=1, bto=0, dds=2, lod_written=True, warnings=[])
    monkeypatch.setattr(_lod_rt, "generate_lod", lambda *a, **k: fake_result)

    logs: list[tuple[str, str]] = []
    # Should complete without raising even though Textures/Terrain/ doesn't exist.
    _run_generate_lod(
        mod_root=mod_root,
        worldspaces=[world],
        working_esm=tmp_path / "Mod.esm",
        asset_dirs=[],
        settings={},
        runner_log=lambda level, msg: logs.append((level, msg)),
    )
    # No removal log; no crash.
    assert not any("atlas-map intermediate" in msg for _lvl, msg in logs)
