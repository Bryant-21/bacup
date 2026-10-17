"""Synthetic navigation CPU benchmark; deliberately excludes native I/O/GPU.

Run with hyperfine; the unclipped mode reproduces the previous group traversal.
"""
from __future__ import annotations

import argparse
import json

from creation_lib.esp.native_runtime import RecordSummary
from ui.esp_editor import app as module
from ui.esp_editor.app import EspEditorApp


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("unclipped", "clipped"), required=True)
    parser.add_argument("--records", type=int, default=100_000)
    parser.add_argument("--frames", type=int, default=120)
    args = parser.parse_args()
    app = EspEditorApp()
    records = [RecordSummary(i, "MISC", f"B21_{i}") for i in range(args.records)]
    draws = 0
    filters = 0
    def legacy_match(record):
        """Keep the original matcher, so new label search cannot inflate it."""
        nonlocal filters
        filters += 1
        signature = app._filter_signature.strip().upper()
        if signature and record.signature.upper() != signature:
            return False
        text = app._filter_text.strip().lower()
        if not text:
            return True
        return text in (record.editor_id or "").lower() or text in f"{record.form_id:08x}"
    def leaf(handle, record):
        nonlocal draws
        draws += 1
    class Clipper:
        def begin(self, count):
            self.display_start = min(500, count)
            self.display_end = min(530, count)
            self.pending = count > 0
        def step(self):
            pending, self.pending = self.pending, False
            return pending
        def end(self):
            pass
    if args.mode == "unclipped":
        app._record_matches_filter = legacy_match
    app._draw_record_leaf = leaf
    module.imgui.ListClipper = Clipper
    try:
        if args.mode == "clipped":
            app._cached_group_records[(1, "MISC")] = records
            app._filter_group_records((1, "MISC"), records)
            if (1, "MISC") in app._filter_futures:
                app._filter_futures[(1, "MISC")][1].result(timeout=10)
                app.poll()
        for _ in range(args.frames):
            if args.mode == "unclipped":
                for record in records:
                    app._draw_item(1, record)
            else:
                app._draw_group_records((1, "MISC"), records)
        print(json.dumps({"mode": args.mode, "records": args.records, "frames": args.frames,
                          "filter_calls": filters, "leaf_calls": draws,
                          "visible_rows": 30, "excludes": ["native I/O", "GPU", "real load order"]}))
    finally:
        app.cleanup()


if __name__ == "__main__":
    main()
