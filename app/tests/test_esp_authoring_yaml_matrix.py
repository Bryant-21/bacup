from __future__ import annotations

import argparse
from collections import Counter
import json
import os
import sys
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path
from types import SimpleNamespace
from typing import Any, Callable

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from app.env_config import build_game_context_from_env
from app.env_sync import parse_env_file
from creation_lib.esp import Plugin, build_authoring_dir, export_authoring_dir
from creation_lib.esp.schema import get_official_allowlist, iter_official_plugin_paths

_MATRIX_GAMES = ("fo3", "fnv", "fo4", "skyrimse", "fo76", "starfield")
_COARSE_NATIVE_ENTRYPOINTS = (
    "export_authoring_dir_native",
    "build_authoring_dir_streaming_native",
    "export_plugin_text_native",
    "import_plugin_text_native",
)


def resolve_game_data_dir(game: str) -> Path | None:
    env = parse_env_file()
    env.update(os.environ)
    return build_game_context_from_env(game, env).data_dir


def _default_matrix_jobs(jobs: int | None) -> int:
    if jobs is None:
        return max(1, os.cpu_count() or 1)
    return max(1, int(jobs))


def _default_record_jobs(matrix_jobs: int, record_jobs: int | None) -> int | None:
    if record_jobs is not None:
        return max(1, int(record_jobs))
    if matrix_jobs > 1:
        return 1
    return None


def _native_contract_report(native_module: Any | None) -> dict[str, Any]:
    if native_module is None:
        return {
            "native_module_loaded": False,
            "native_module_name": None,
            "coarse_native_contract_ready": False,
            "missing_coarse_entrypoints": list(_COARSE_NATIVE_ENTRYPOINTS),
        }

    missing = [
        name for name in _COARSE_NATIVE_ENTRYPOINTS
        if not callable(getattr(native_module, name, None))
    ]
    return {
        "native_module_loaded": True,
        "native_module_name": getattr(native_module, "__name__", type(native_module).__name__),
        "coarse_native_contract_ready": not missing,
        "missing_coarse_entrypoints": missing,
    }


def _authoring_backend_report() -> dict[str, Any]:
    from creation_lib.esp import native_runtime as esp_native_runtime

    try:
        contract = _native_contract_report(esp_native_runtime.load_native_module())
        native_probe_error = None
    except Exception as exc:
        contract = _native_contract_report(None)
        native_probe_error = f"{type(exc).__name__}: {exc}"
    return {
        "selected_backend": "native-authoring-dir",
        "selected_backend_reason": (
            "matrix roundtrips target the coarse native authoring-dir runtime; execution requires the Rust backend"
        ),
        "native_probe_error": native_probe_error,
        **contract,
    }


def _build_roundtrip_benchmark_result(
    *,
    game: str,
    plugin_name: str,
    format: str,
    status: str,
    detail: str,
    backend: dict[str, Any],
    timings: dict[str, float] | None = None,
    record_count: int | None = None,
    authoring_dir: Path | str | None = None,
) -> dict[str, Any]:
    normalized_timings = dict(timings or {})
    return {
        "game": game,
        "plugin": plugin_name,
        "format": format,
        "status": status,
        "detail": detail,
        "record_count": record_count,
        "authoring_dir": str(authoring_dir) if authoring_dir is not None else None,
        "backend": dict(backend),
        "timings": normalized_timings,
        "total_seconds": sum(float(value) for value in normalized_timings.values()),
    }


def _build_matrix_benchmark_summary(results: list[dict[str, Any]]) -> dict[str, Any]:
    status_counts = Counter(str(result.get("status")) for result in results)
    backend_counts = Counter(
        str(result.get("backend", {}).get("selected_backend", "unknown"))
        for result in results
    )
    return {
        "total": len(results),
        "ok": status_counts.get("ok", 0),
        "fail": status_counts.get("fail", 0),
        "exception": status_counts.get("exception", 0),
        "selected_backend_counts": dict(sorted(backend_counts.items())),
        "coarse_native_contract_ready_count": sum(
            1
            for result in results
            if bool(result.get("backend", {}).get("coarse_native_contract_ready"))
        ),
    }


def _write_matrix_benchmark_report(
    report_path: Path,
    *,
    selected_games: tuple[str, ...],
    format: str,
    results: list[dict[str, Any]],
    failures: list[str],
) -> None:
    payload = {
        "report_version": 1,
        "selected_games": list(selected_games),
        "format": format,
        "summary": _build_matrix_benchmark_summary(results),
        "failures": list(failures),
        "results": results,
    }
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def _record_signatures_by_form_id(plugin: Plugin) -> dict[int, str]:
    return {
        record.form_id & 0xFFFFFFFF: record.signature
        for record in plugin.records
        if (record.form_id & 0xFFFFFFFF) != 0
    }


def _format_record_diff(original: Plugin, roundtrip: Plugin) -> str:
    original_map = _record_signatures_by_form_id(original)
    roundtrip_map = _record_signatures_by_form_id(roundtrip)

    original_ids = set(original_map)
    roundtrip_ids = set(roundtrip_map)
    missing = sorted(original_ids - roundtrip_ids)
    extra = sorted(roundtrip_ids - original_ids)
    changed = sorted(
        form_id for form_id in original_ids & roundtrip_ids if original_map[form_id] != roundtrip_map[form_id]
    )

    def sample(entries: list[int], mapping: dict[int, str]) -> str:
        shown = [f"{form_id:08X}:{mapping[form_id]}" for form_id in entries[:10]]
        suffix = "" if len(entries) <= 10 else f" ... +{len(entries) - 10} more"
        return ", ".join(shown) + suffix if shown else "none"

    return (
        f"missing={len(missing)} [{sample(missing, original_map)}]; "
        f"extra={len(extra)} [{sample(extra, roundtrip_map)}]; "
        f"type_changed={len(changed)} ["
        + ", ".join(
            f"{form_id:08X}:{original_map[form_id]}->{roundtrip_map[form_id]}"
            for form_id in changed[:10]
        )
        + ("" if len(changed) <= 10 else f" ... +{len(changed) - 10} more")
        + "]"
    )


def _record_snapshot(record) -> tuple:
    return (
        record.form_id & 0xFFFFFFFF,
        record.signature,
        record.flags,
        record.version_control,
        record.form_version,
        record.version2,
        record.raw_payload,
        record.parse_error,
        tuple((subrecord.signature, bytes(subrecord.data), subrecord.semantic_type) for subrecord in record.subrecords),
    )


def _equivalent_authoring_roundtrip(original: Plugin, roundtrip: Plugin) -> bool:
    if original.header.version != roundtrip.header.version:
        return False
    if original.header.next_object_id != roundtrip.header.next_object_id:
        return False
    if original.header.author != roundtrip.header.author:
        return False
    if original.header.description != roundtrip.header.description:
        return False
    if original.header.masters != roundtrip.header.masters:
        return False
    if original.header.master_sizes != roundtrip.header.master_sizes:
        return False
    if original.header.overridden_forms != roundtrip.header.overridden_forms:
        return False
    if original.header.flags != roundtrip.header.flags:
        return False
    if original.header.version_control != roundtrip.header.version_control:
        return False
    if original.header.form_version != roundtrip.header.form_version:
        return False
    if original.header.version2 != roundtrip.header.version2:
        return False
    if [
        (subrecord.signature, bytes(subrecord.data), subrecord.semantic_type)
        for subrecord in original.header.extra_subrecords
    ] != [
        (subrecord.signature, bytes(subrecord.data), subrecord.semantic_type)
        for subrecord in roundtrip.header.extra_subrecords
    ]:
        return False
    return sorted(_record_snapshot(record) for record in original.records) == sorted(
        _record_snapshot(record) for record in roundtrip.records
    )


def _official_plugin_paths(game: str) -> list[Path]:
    data_dir = resolve_game_data_dir(game)
    if data_dir is None:
        raise FileNotFoundError(f"data directory not configured for {game}")

    plugin_paths = iter_official_plugin_paths(game, data_dir, allowlist=get_official_allowlist(game))
    if not plugin_paths:
        raise FileNotFoundError(f"no official plugins available for {game}")
    return plugin_paths


def _format_eta(seconds: float) -> str:
    s = int(seconds)
    if s < 60:
        return f"{s}s"
    return f"{s // 60}m {s % 60}s"


def _format_plugin_timing_summary(plugin_name: str, timings: dict[str, float]) -> str:
    parts = []
    for key, label in (
        ("load_seconds", "load"),
        ("export_seconds", "export"),
        ("import_seconds", "import"),
        ("verify_seconds", "verify"),
    ):
        value = timings.get(key)
        if value is not None:
            parts.append(f"{label}={float(value):.2f}s")
    parts.append(f"total={sum(float(value) for value in timings.values()):.2f}s")
    return f"  {plugin_name} timings: " + " ".join(parts)


def _roundtrip_one_official_plugin(
    *,
    game: str,
    plugin_path_str: str,
    game_out_str: str,
    record_jobs: int | None,
    format: str = "json",
    verbose: bool = True,
) -> tuple[str, str, str, list[str], dict[str, Any]]:
    plugin_path = Path(plugin_path_str)
    game_out = Path(game_out_str)
    progress_path = game_out / f"{plugin_path.name}.progress.txt"
    log_lines: list[str] = []
    backend = _authoring_backend_report()
    timings: dict[str, float] = {}
    record_count: int | None = None
    authoring_dir: Path | None = None

    def _emit(line: str) -> None:
        log_lines.append(line)
        if verbose:
            print(line, flush=True)

    # Progress forwarder: throttle captured log_lines to once per 2s so a
    # verbose+capture run doesn't flood; always print to stdout (flush=True)
    # since these pings are user-visible.
    progress_state: dict[str, Any] = {"phase": "", "last_capture": 0.0}

    def _make_progress_cb(phase: str) -> Callable[[int, int, float], None]:
        progress_state["phase"] = phase
        progress_state["last_capture"] = 0.0

        def _cb(done: int, total: int, elapsed: float) -> None:
            pct = (done / total * 100.0) if total else 0.0
            if done > 0 and elapsed > 0:
                remaining = elapsed / done * (total - done) if done < total else 0.0
                eta = _format_eta(remaining)
            else:
                eta = "--"
            line = (
                f"  {plugin_path.name}   {phase}: {done}/{total} "
                f"({pct:.1f}%, {elapsed:.1f}s, ETA {eta})"
            )
            # Always print to stdout so users see progress even when capturing.
            print(line, flush=True)
            now = time.perf_counter()
            if now - progress_state["last_capture"] >= 2.0:
                log_lines.append(line)
                progress_state["last_capture"] = now

        return _cb

    try:
        if not backend["coarse_native_contract_ready"]:
            missing = ", ".join(backend["missing_coarse_entrypoints"]) or "native runtime unavailable"
            detail = f"{game}:{plugin_path.name}: native authoring backend unavailable ({missing})"
            return (
                "exception",
                plugin_path.name,
                detail,
                log_lines,
                _build_roundtrip_benchmark_result(
                    game=game,
                    plugin_name=plugin_path.name,
                    format=format,
                    status="exception",
                    detail=detail,
                    backend=backend,
                    timings=timings,
                    record_count=record_count,
                    authoring_dir=authoring_dir,
                ),
            )
        _emit(f"  {plugin_path.name} starting...")
        progress_path.write_text(
            f"loading={plugin_path}\nstatus=exporting_authoring_dir\n",
            encoding="utf-8",
        )
        _emit(f"  {plugin_path.name} load...")
        t_load_start = time.perf_counter()
        plugin = Plugin.load(plugin_path, game=game)
        t_load = time.perf_counter() - t_load_start
        timings["load_seconds"] = t_load
        _emit(f"  {plugin_path.name} load={t_load:.2f}s")

        authoring_dir = game_out / f"{plugin_path.name}.authoring"
        record_count = plugin.record_count
        _emit(f"  {plugin_path.name} export({format}, {record_count} records)...")
        t_export_start = time.perf_counter()
        export_authoring_dir(
            plugin,
            authoring_dir,
            jobs=record_jobs,
            format=format,
            backend="native",
        )
        t_export = time.perf_counter() - t_export_start
        timings["export_seconds"] = t_export
        _emit(
            f"  {plugin_path.name} export({format}, {record_count} records)={t_export:.2f}s"
            f" -> {authoring_dir}"
        )

        progress_path.write_text(
            f"loading={plugin_path}\nstatus=importing_roundtrip_authoring_dir\nauthoring_dir={authoring_dir}\n",
            encoding="utf-8",
        )
        _emit(f"  {plugin_path.name} import({format})...")
        t_import_start = time.perf_counter()
        rebuilt_esp = game_out / f"{plugin_path.name}.rebuilt.esp"
        build_authoring_dir(authoring_dir, rebuilt_esp, game=game)
        rebuilt = Plugin.load(rebuilt_esp, game=game)
        t_import = time.perf_counter() - t_import_start
        timings["import_seconds"] = t_import
        _emit(f"  {plugin_path.name} import({format})={t_import:.2f}s")

        _emit(f"  {plugin_path.name} verify...")
        t_verify_start = time.perf_counter()
        handle_original = getattr(plugin, "_rust_handle", None)
        handle_rebuilt = getattr(rebuilt, "_rust_handle", None)
        if handle_original is not None and handle_rebuilt is not None:
            verify_ok = handle_original.roundtrip_equal(handle_rebuilt)
        else:
            verify_ok = _equivalent_authoring_roundtrip(plugin, rebuilt)
        t_verify = time.perf_counter() - t_verify_start
        timings["verify_seconds"] = t_verify
        _emit(
            f"  {plugin_path.name} verify={t_verify:.2f}s {'ok' if verify_ok else 'fail'}"
        )

        _emit(_format_plugin_timing_summary(plugin_path.name, timings))

        if not verify_ok:
            progress_path.write_text(
                f"loading={plugin_path}\nstatus=failed_roundtrip\nauthoring_dir={authoring_dir}\n",
                encoding="utf-8",
            )
            detail = (
                f"{game}:{plugin_path.name} authoring directory mismatch; "
                f"{_format_record_diff(plugin, rebuilt)}"
            )
            return (
                "fail",
                plugin_path.name,
                detail,
                log_lines,
                _build_roundtrip_benchmark_result(
                    game=game,
                    plugin_name=plugin_path.name,
                    format=format,
                    status="fail",
                    detail=detail,
                    backend=backend,
                    timings=timings,
                    record_count=record_count,
                    authoring_dir=authoring_dir,
                ),
            )
        progress_path.write_text(
            f"loading={plugin_path}\nstatus=ok\nauthoring_dir={authoring_dir}\n",
            encoding="utf-8",
        )
        detail = str(authoring_dir)
        return (
            "ok",
            plugin_path.name,
            detail,
            log_lines,
            _build_roundtrip_benchmark_result(
                game=game,
                plugin_name=plugin_path.name,
                format=format,
                status="ok",
                detail=detail,
                backend=backend,
                timings=timings,
                record_count=record_count,
                authoring_dir=authoring_dir,
            ),
        )
    except Exception as exc:
        try:
            progress_path.write_text(
                f"loading={plugin_path}\nstatus=exception\nerror={exc}\n",
                encoding="utf-8",
            )
        except Exception:
            pass
        detail = f"{game}:{plugin_path.name}: {exc}"
        return (
            "exception",
            plugin_path.name,
            detail,
            log_lines,
            _build_roundtrip_benchmark_result(
                game=game,
                plugin_name=plugin_path.name,
                format=format,
                status="exception",
                detail=detail,
                backend=backend,
                timings=timings,
                record_count=record_count,
                authoring_dir=authoring_dir,
            ),
        )


def _roundtrip_all_official_plugins_via_authoring_yaml(
    game: str,
    out_root: Path,
    *,
    jobs: int | None = None,
    record_jobs: int | None = None,
    format: str = "json",
) -> None:
    plugin_paths = _official_plugin_paths(game)
    out_dir = out_root / game
    out_dir.mkdir(parents=True, exist_ok=True)
    max_workers = _default_matrix_jobs(jobs)
    effective_record_jobs = _default_record_jobs(max_workers, record_jobs)
    if max_workers == 1 or len(plugin_paths) <= 1:
        for plugin_path in plugin_paths:
            status, _, detail, _lines, _report = _roundtrip_one_official_plugin(
                game=game,
                plugin_path_str=str(plugin_path),
                game_out_str=str(out_dir),
                record_jobs=effective_record_jobs,
                format=format,
            )
            assert status == "ok", detail
        return

    with ProcessPoolExecutor(max_workers=max_workers) as executor:
        futures = [
            executor.submit(
                _roundtrip_one_official_plugin,
                game=game,
                plugin_path_str=str(plugin_path),
                game_out_str=str(out_dir),
                record_jobs=effective_record_jobs,
                format=format,
            )
            for plugin_path in plugin_paths
        ]
        for future in as_completed(futures):
            status, _, detail, _lines, _report = future.result()
            assert status == "ok", detail


@pytest.mark.integration
@pytest.mark.skip(reason="Long-running manual matrix; run this file directly with -game=<id> or -game=ALL")
def test_fo3_authoring_yaml_roundtrip_matrix(tmp_path: Path) -> None:
    _roundtrip_all_official_plugins_via_authoring_yaml("fo3", tmp_path)


@pytest.mark.integration
@pytest.mark.skip(reason="Long-running manual matrix; run this file directly with -game=<id> or -game=ALL")
def test_fnv_authoring_yaml_roundtrip_matrix(tmp_path: Path) -> None:
    _roundtrip_all_official_plugins_via_authoring_yaml("fnv", tmp_path)


@pytest.mark.integration
@pytest.mark.skip(reason="Long-running manual matrix; run this file directly with -game=<id> or -game=ALL")
def test_fo4_authoring_yaml_roundtrip_matrix(tmp_path: Path) -> None:
    _roundtrip_all_official_plugins_via_authoring_yaml("fo4", tmp_path)


@pytest.mark.integration
@pytest.mark.skip(reason="Long-running manual matrix; run this file directly with -game=<id> or -game=ALL")
def test_skyrimse_authoring_yaml_roundtrip_matrix(tmp_path: Path) -> None:
    _roundtrip_all_official_plugins_via_authoring_yaml("skyrimse", tmp_path)


@pytest.mark.integration
@pytest.mark.skip(reason="Long-running manual matrix; run this file directly with -game=<id> or -game=ALL")
def test_fo76_authoring_yaml_roundtrip_matrix(tmp_path: Path) -> None:
    _roundtrip_all_official_plugins_via_authoring_yaml("fo76", tmp_path)


@pytest.mark.integration
@pytest.mark.skip(reason="Long-running manual matrix; run this file directly with -game=<id> or -game=ALL")
def test_starfield_authoring_yaml_roundtrip_matrix(tmp_path: Path) -> None:
    _roundtrip_all_official_plugins_via_authoring_yaml("starfield", tmp_path)


def test_authoring_backend_report_marks_native_runtime_missing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("creation_lib.esp.native_runtime.load_native_module", lambda: None)

    report = _authoring_backend_report()

    assert report["selected_backend"] == "native-authoring-dir"
    assert report["native_module_loaded"] is False
    assert report["coarse_native_contract_ready"] is False
    assert report["missing_coarse_entrypoints"] == list(_COARSE_NATIVE_ENTRYPOINTS)
    assert report["native_probe_error"] is None


def test_authoring_backend_report_records_partial_native_contract(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    partial_runtime = SimpleNamespace(
        __name__="esp_authoring_core",
        export_authoring_dir_native=lambda *args: None,
    )
    monkeypatch.setattr("creation_lib.esp.native_runtime.load_native_module", lambda: partial_runtime)

    report = _authoring_backend_report()

    assert report["native_module_loaded"] is True
    assert report["native_module_name"] == "esp_authoring_core"
    assert report["coarse_native_contract_ready"] is False
    assert report["missing_coarse_entrypoints"] == [
        "build_authoring_dir_streaming_native",
        "export_plugin_text_native",
        "import_plugin_text_native",
    ]
    assert report["selected_backend"] == "native-authoring-dir"


def test_authoring_backend_report_prefers_native_when_coarse_contract_is_ready(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    ready_runtime = SimpleNamespace(
        __name__="esp_authoring_core",
        export_authoring_dir_native=lambda *args: None,
        build_authoring_dir_streaming_native=lambda *args: None,
        export_plugin_text_native=lambda *args: None,
        import_plugin_text_native=lambda *args: None,
    )
    monkeypatch.setattr("creation_lib.esp.native_runtime.load_native_module", lambda: ready_runtime)

    report = _authoring_backend_report()

    assert report["selected_backend"] == "native-authoring-dir"
    assert report["native_module_loaded"] is True
    assert report["coarse_native_contract_ready"] is True
    assert report["missing_coarse_entrypoints"] == []


def test_authoring_backend_report_records_probe_errors_without_fallback(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def _boom() -> None:
        raise RuntimeError("probe crashed")

    monkeypatch.setattr("creation_lib.esp.native_runtime.load_native_module", _boom)

    report = _authoring_backend_report()

    assert report["selected_backend"] == "native-authoring-dir"
    assert report["native_module_loaded"] is False
    assert report["native_probe_error"] == "RuntimeError: probe crashed"


def test_build_roundtrip_benchmark_result_includes_backend_and_total_time() -> None:
    result = _build_roundtrip_benchmark_result(
        game="fo4",
        plugin_name="Example.esm",
        format="yaml",
        status="ok",
        detail="tmp/Example.esm.authoring",
        backend={
            "selected_backend": "native-authoring-dir",
            "native_module_loaded": True,
            "coarse_native_contract_ready": True,
            "missing_coarse_entrypoints": [],
        },
        timings={
            "load_seconds": 1.25,
            "export_seconds": 2.5,
            "import_seconds": 0.75,
        },
        record_count=7,
        authoring_dir=Path("tmp") / "Example.esm.authoring",
    )

    assert result["game"] == "fo4"
    assert result["plugin"] == "Example.esm"
    assert result["record_count"] == 7
    assert result["authoring_dir"] == str(Path("tmp") / "Example.esm.authoring")
    assert result["total_seconds"] == pytest.approx(4.5)
    assert result["backend"]["selected_backend"] == "native-authoring-dir"


def test_format_plugin_timing_summary_lists_all_phases() -> None:
    line = _format_plugin_timing_summary(
        "Example.esm",
        {
            "load_seconds": 1.25,
            "export_seconds": 2.5,
            "import_seconds": 0.75,
            "verify_seconds": 0.5,
        },
    )

    assert line == "  Example.esm timings: load=1.25s export=2.50s import=0.75s verify=0.50s total=5.00s"


def test_run_matrix_cli_writes_benchmark_report_json(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    plugin_path = tmp_path / "Benchmark.esm"
    plugin_path.write_bytes(b"test")
    report_path = tmp_path / "reports" / "authoring-matrix.json"
    module = sys.modules[__name__]

    def fake_official_plugin_paths(game: str) -> list[Path]:
        assert game == "fo4"
        return [plugin_path]

    def fake_roundtrip_one_official_plugin(**kwargs: Any) -> tuple[str, str, str, list[str], dict[str, Any]]:
        detail = str(tmp_path / "matrix" / "fo4" / "Benchmark.esm.authoring")
        return (
            "ok",
            plugin_path.name,
            detail,
            ["synthetic log"],
            _build_roundtrip_benchmark_result(
                game=str(kwargs["game"]),
                plugin_name=plugin_path.name,
                format=str(kwargs["format"]),
                status="ok",
                detail=detail,
                backend={
                    "selected_backend": "native-authoring-dir",
                    "native_module_loaded": True,
                    "coarse_native_contract_ready": True,
                    "missing_coarse_entrypoints": [],
                },
                timings={"load_seconds": 1.0, "export_seconds": 2.0},
                record_count=1,
                authoring_dir=detail,
            ),
        )

    monkeypatch.setattr(module, "_official_plugin_paths", fake_official_plugin_paths)
    monkeypatch.setattr(module, "_roundtrip_one_official_plugin", fake_roundtrip_one_official_plugin)

    exit_code = _run_matrix_cli(
        "fo4",
        tmp_path / "matrix",
        jobs=1,
        format="yaml",
        verbose=False,
        report_json=report_path,
    )

    assert exit_code == 0
    payload = json.loads(report_path.read_text(encoding="utf-8"))
    assert payload["selected_games"] == ["fo4"]
    assert payload["format"] == "yaml"
    assert payload["summary"]["ok"] == 1
    assert payload["summary"]["selected_backend_counts"] == {"native-authoring-dir": 1}
    assert payload["failures"] == []
    assert payload["results"][0]["plugin"] == "Benchmark.esm"
    assert payload["results"][0]["total_seconds"] == pytest.approx(3.0)


def _run_matrix_cli(
    game: str,
    out_dir: Path,
    *,
    jobs: int | None = None,
    record_jobs: int | None = None,
    format: str = "json",
    verbose: bool = True,
    report_json: Path | None = None,
) -> int:
    selected_games = _MATRIX_GAMES if game.upper() == "ALL" else (game.lower(),)
    failures: list[str] = []
    reports: list[dict[str, Any]] = []
    max_workers = _default_matrix_jobs(jobs)

    for game_id in selected_games:
        print(f"[{game_id}]", flush=True)
        try:
            plugin_paths = _official_plugin_paths(game_id)
        except FileNotFoundError as exc:
            print(f"  skip: {exc}", flush=True)
            failures.append(f"{game_id}: {exc}")
            continue

        plugin_paths = sorted(
            plugin_paths,
            key=lambda candidate: candidate.stat().st_size,
            reverse=True,
        )

        game_out = out_dir / game_id
        game_out.mkdir(parents=True, exist_ok=True)

        plugin_parallel = max_workers > 1 and len(plugin_paths) >= max_workers
        if plugin_parallel:
            effective_record_jobs = _default_record_jobs(max_workers, record_jobs)
        elif record_jobs is not None:
            effective_record_jobs = max(1, int(record_jobs))
        elif max_workers > 1:
            effective_record_jobs = max_workers
        else:
            effective_record_jobs = None

        mode = "plugin-parallel" if plugin_parallel else "sequential-per-plugin"
        effective_workers = min(max_workers, len(plugin_paths))
        print(
            f"[{game_id}] starting {len(plugin_paths)} plugins, "
            f"workers={effective_workers} "
            f"record_jobs={effective_record_jobs or 'auto'} "
            f"format={format} mode={mode}",
            flush=True,
        )
        print(
            f"  workers={effective_workers} "
            f"record_jobs={effective_record_jobs or 'auto'} "
            f"format={format} plugins={len(plugin_paths)} "
            f"mode={mode}",
            flush=True,
        )
        if not plugin_parallel:
            for plugin_path in plugin_paths:
                print(f"  {plugin_path.name}", flush=True)
                status, _, detail, _lines, report = _roundtrip_one_official_plugin(
                    game=game_id,
                    plugin_path_str=str(plugin_path),
                    game_out_str=str(game_out),
                    record_jobs=effective_record_jobs,
                    format=format,
                    verbose=verbose,
                )
                reports.append(report)
                if status == "ok":
                    print(f"    OK -> {detail}", flush=True)
                else:
                    failures.append(detail)
                    print(f"    FAIL: {detail}", flush=True)
            continue

        with ProcessPoolExecutor(max_workers=max_workers) as executor:
            futures = {
                executor.submit(
                    _roundtrip_one_official_plugin,
                    game=game_id,
                    plugin_path_str=str(plugin_path),
                    game_out_str=str(game_out),
                    record_jobs=effective_record_jobs,
                    format=format,
                    verbose=False,
                ): plugin_path.name
                for plugin_path in plugin_paths
            }
            for future in as_completed(futures):
                plugin_name = futures[future]
                print(f"  {plugin_name}", flush=True)
                status, _, detail, log_lines, report = future.result()
                reports.append(report)
                if verbose:
                    for line in log_lines:
                        print(line, flush=True)
                if status == "ok":
                    print(f"    OK -> {detail}", flush=True)
                else:
                    failures.append(detail)
                    print(f"    FAIL: {detail}", flush=True)

    if report_json is not None:
        _write_matrix_benchmark_report(
            report_json,
            selected_games=selected_games,
            format=format,
            results=reports,
            failures=failures,
        )
        print(f"Benchmark report -> {report_json}", flush=True)

    if failures:
        print("", flush=True)
        print("Failures:", flush=True)
        for failure in failures:
            print(f"  - {failure}", flush=True)
        return 1

    print("", flush=True)
    print(f"All selected games passed. Output ({format}) under: {out_dir}", flush=True)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Run direct authoring-dir roundtrip matrix by game (format: json|yaml)"
    )
    parser.add_argument(
        "-game",
        "--game",
        default="ALL",
        choices=[*_MATRIX_GAMES, "ALL"],
        help="Game to test, or ALL",
    )
    parser.add_argument(
        "-out",
        "--out-dir",
        default=str(Path("tmp") / "authoring_yaml_matrix"),
        help="Directory to write generated authoring files into",
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=None,
        help="Number of plugins to process in parallel. Defaults to CPU count.",
    )
    parser.add_argument(
        "--record-jobs",
        type=int,
        default=None,
        help="Per-plugin record export jobs. Defaults to 1 when --jobs > 1, otherwise exporter auto mode.",
    )
    parser.add_argument(
        "--format",
        default="json",
        choices=["json", "yaml"],
        help="Output serialization format (default: json).",
    )
    parser.add_argument(
        "--report-json",
        default=None,
        help="Optional JSON path for a structured benchmark summary.",
    )
    verbose_group = parser.add_mutually_exclusive_group()
    verbose_group.add_argument(
        "-v",
        "--verbose",
        dest="verbose",
        action="store_true",
        default=True,
        help="Print per-step timing lines for each plugin (default: on).",
    )
    verbose_group.add_argument(
        "-q",
        "--quiet",
        "--no-verbose",
        dest="verbose",
        action="store_false",
        help="Suppress per-step timing lines; only print one OK/FAIL per plugin.",
    )
    args = parser.parse_args(argv)
    return _run_matrix_cli(
        args.game,
        Path(args.out_dir).resolve(),
        jobs=args.jobs,
        record_jobs=args.record_jobs,
        format=args.format,
        verbose=args.verbose,
        report_json=Path(args.report_json).resolve() if args.report_json else None,
    )


if __name__ == "__main__":
    sys.exit(main())
