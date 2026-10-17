"""Read-only developer latency profiling against explicit installed plugins.

No settings save, plugin save, backup, deployment, index build or game launch.
Output must stay in the repository. Timing inside phases measures API latency;
use hyperfine around this command for process-level timing.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import statistics
import threading
import time
from concurrent.futures import CancelledError, ThreadPoolExecutor
from pathlib import Path

import psutil

from creation_lib.esp.native_runtime import plugin_handle_group_record_summaries
from ui.esp_editor.app import EspEditorApp
from ui.esp_editor.record_view import load_comparison
from ui.toolkit.settings import ToolkitSettings


def fingerprint(path):
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    stat = path.stat()
    return {"size": stat.st_size, "mtime_ns": stat.st_mtime_ns, "sha256": digest}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--game", default="fo4")
    parser.add_argument("--plugin", action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    repository = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    output.relative_to(repository)  # never write a profiling artifact to game data
    settings = ToolkitSettings()
    root = Path(settings.get_game_paths(args.game)["root_dir"]) / "Data"
    paths = [root / name for name in args.plugin]
    if any(Path(name).name != name for name in args.plugin):
        raise ValueError("Pass plugin filenames from the configured game's Data directory")
    before = {p.name: fingerprint(p) for p in paths}
    editor = EspEditorApp(toolkit_settings=settings)
    rss_start = psutil.Process().memory_info().rss
    result = {"game": args.game, "requested_plugins": args.plugin, "phases": {}, "errors": []}
    gaps = []
    stop = threading.Event()
    def heartbeat():
        previous = time.perf_counter()
        while not stop.wait(0.005):
            current = time.perf_counter()
            gaps.append(current - previous)
            previous = current
    thread = threading.Thread(target=heartbeat, daemon=True)
    thread.start()
    def phase(name, work):
        start = time.perf_counter()
        value = work()
        result["phases"][name] = time.perf_counter() - start
        print(f"{name}: {result['phases'][name]:.4f}s", flush=True)
        return value
    try:
        phase("load_order_s", lambda: [editor.session.load(p, game=args.game) for p in paths])
        plugins = editor.session.plugins
        # Include any recursively loaded dependencies in the after-read integrity check.
        for plugin in plugins:
            path = Path(plugin.path)
            if path.name not in before:
                before[path.name] = fingerprint(path)
        warmups = phase("group_skeletons_s", lambda: [editor._warmup_plugin(p) for p in plugins])
        result["plugins"] = [{"name": p.plugin_name, "records": w["record_count"]}
                             for p, w in zip(plugins, warmups)]
        result["record_count"] = sum(w["record_count"] for w in warmups)
        editor._plugin_record_counts = {p.handle: w["record_count"] for p, w in zip(plugins, warmups)}
        editor._plugin_flags = {p.handle: w["flags"] for p, w in zip(plugins, warmups)}
        editor._plugin_masters = {p.handle: w["masters"] for p, w in zip(plugins, warmups)}
        editor._start_auto_conflict_scan()
        result["automatic_scan_deferred"] = editor._auto_conflict_deferred
        result["rss_loaded_bytes"] = psutil.Process().memory_info().rss
        samples = []
        group_results = []
        for plugin, warmup in zip(plugins, warmups):
            # Real heterogeneous groups; no guessed IDs or data search/dump.
            for signature in ("NPC_", "QUST", "WEAP", "CELL", "WRLD"):
                if not any(label == signature for label, _ in warmup["groups"]):
                    continue
                start = time.perf_counter()
                summaries = plugin_handle_group_record_summaries(plugin.handle, signature)
                group_results.append({"plugin": plugin.plugin_name, "signature": signature,
                                      "records": len(summaries), "seconds": time.perf_counter() - start})
                if summaries:
                    samples.append((plugin, summaries[len(summaries) // 2]))
                if plugin is plugins[0] and signature == "WRLD":
                    key = (plugin.handle, signature)
                    editor._cached_group_records[key] = summaries
                    editor._known_group_types[key] = {r.signature for r in summaries}
                    phase("initial_sort_request_main_thread_s", lambda: editor._filter_group_records(key, summaries))
                    phase("initial_sort_worker_remaining_s", lambda: editor._filter_futures[key][1].result())
                    editor.poll()
                    phase("cached_sorted_group_120_s", lambda: [editor._filter_group_records(key, summaries) for _ in range(120)])
                    editor._filter_text = "settlement"
                    phase("filter_request_main_thread_s", lambda: editor._filter_group_records(key, summaries))
                    phase("filter_worker_remaining_s", lambda: editor._filter_futures[key][1].result())
                    editor.poll()
                    editor._filter_text = ""
        result["groups"] = group_results
        cold = []
        readonly = {}
        for plugin, summary in samples[:20]:
            start = time.perf_counter()
            documents = load_comparison(plugins, plugin, summary)
            cold.append(time.perf_counter() - start)
            for doc in documents:
                for fld, reason in zip(doc.fields, doc.edit_errors):
                    if fld.signature in ("VMAD", "CTDA", "FULL", "DESC"):
                        key = (fld.signature, fld.kind.value, bool(reason))
                        readonly[key] = readonly.get(key, 0) + 1
        result["selection_samples_s"] = cold
        result["sample_field_guards"] = [{"signature": k[0], "kind": k[1], "readonly": k[2], "count": v}
                                        for k, v in readonly.items()]
        result["selection_median_s"] = statistics.median(cold) if cold else None
        result["selection_max_s"] = max(cold) if cold else None
        if samples:
            plugin, summary = samples[0]
            editor._select_record(plugin.handle, summary)
            editor._selection_future.result(timeout=60)
            editor.poll()
            phase("cached_selection_120_s", lambda: [editor._select_record(plugin.handle, summary) for _ in range(120)])
            with ThreadPoolExecutor(max_workers=1) as executor:
                scan_start = time.perf_counter()
                scan = executor.submit(editor.session.run_conflict_scan)
                # Let the native scan take its store lock before the detail read.
                time.sleep(0.01)
                phase("selection_during_scan_s", lambda: load_comparison(plugins, plugin, summary))
                report = phase("scan_remaining_s", scan.result)
                result["conflict_reports"] = len(report.reports)
                result["phases"]["full_scan_s"] = time.perf_counter() - scan_start
            analysis_start = time.perf_counter()
            phase("analysis_inventory_request_s", editor.start_override_analysis)
            phase("analysis_inventory_worker_s", lambda: editor._analysis_future.result(timeout=60))
            editor.poll()
            batches = []
            while editor._analysis_future is not None:
                start = time.perf_counter()
                label = editor._analysis_label
                if not batches:
                    time.sleep(0.01)
                    phase("selection_during_first_analysis_batch_s", lambda: load_comparison(plugins, plugin, summary))
                editor._analysis_future.result(timeout=60)
                editor.poll()
                batches.append({"label": label, "wait_and_merge_s": time.perf_counter() - start,
                                "cumulative_reports": len(editor._conflict_scan)})
                print(f"Analysis {editor._analysis_completed}/{editor._analysis_total}: {batches[-1]['wait_and_merge_s']:.4f}s", flush=True)
            result["analysis_batches"] = batches
            result["phases"]["incremental_analysis_s"] = time.perf_counter() - analysis_start
            result["analysis_complete"] = editor._scan_complete
            result["analysis_reports_match_full_scan"] = (
                set(editor._conflict_scan.by_record_key) == set(report.by_record_key)
                and all(editor._conflict_scan.by_record_key[key].status == item.status
                        and editor._conflict_scan.by_record_key[key].winner.plugin_handle == item.winner.plugin_handle
                        for key, item in report.by_record_key.items())
            )
            phase("overview_first_filter_s", editor._overview_reports)
            if editor._overview_future:
                phase("overview_worker_remaining_s", lambda: editor._overview_future[1].result(timeout=60))
                editor.poll()
            phase("overview_cached_120_s", lambda: [editor._overview_reports() for _ in range(120)])
            phase("analysis_cancel_request_s", editor.start_override_analysis)
            phase("analysis_cancel_action_s", editor.cancel_override_analysis)
            try:
                editor._analysis_future.result(timeout=60)
            except CancelledError:
                pass
            editor.poll()
            result["analysis_cancel_stopped_queue"] = editor._analysis_future is None and not editor._analysis_queue
        result["rss_peak_bytes"] = psutil.Process().memory_info().peak_wset
    except Exception as exc:
        result["errors"].append(f"{type(exc).__name__}: {exc}")
        raise
    finally:
        stop.set()
        thread.join()
        result["heartbeat_max_gap_s"] = max(gaps, default=0)
        result["rss_start_bytes"] = rss_start
        result["source_fingerprints"] = before
        result["sources_unchanged"] = all(fingerprint(root / name) == metadata for name, metadata in before.items())
        editor.cleanup()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, indent=2), encoding="utf-8")
        print(f"Read-only profile: {output}; sources unchanged: {result['sources_unchanged']}")


if __name__ == "__main__":
    main()
