"""Execute a compiled disposable AS3 movie twice in the installed Ruffle player."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import time

from ui.swf_editor.document import SwfEditingDocument
from ui.swf_editor.swf_editor_app import SwfEditorApp


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(Path(__file__).resolve().parents[2])
    output.mkdir(parents=True, exist_ok=True)
    app = SwfEditorApp()
    evidence = []
    try:
        doc = SwfEditingDocument.menu()
        doc.sources["DocumentMenu.as"] = doc.sources["DocumentMenu.as"].replace(
            'title.text = "New Scaleform menu";', 'title.text = "Disposable preview fixture"; trace("B21_SWF_PREVIEW_READY");')
        data = doc.build()
        for iteration in (1, 2):
            app.launch_preview(data)
            directory = Path(app._preview_directory.name)
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline and "B21_SWF_PREVIEW_READY" not in app.preview_output():
                app.poll()
                if app._preview_process.poll() is not None:
                    break
                time.sleep(0.05)
            log = app.preview_output()
            evidence.append({"iteration": iteration, "returncode": app._preview_process.poll(),
                             "trace_executed": "B21_SWF_PREVIEW_READY" in log, "log": log})
            assert app._preview_process.poll() is None, log
            assert "B21_SWF_PREVIEW_READY" in log, "Player did not report the AS3 constructor trace"
            app.close_preview()
            app.close_preview()
            assert not directory.exists()
        print("Ruffle executed the compiled AS3 constructor twice; repeated close removed both disposable previews")
    finally:
        app.cleanup()
        (output / "preview-evidence.json").write_text(json.dumps(evidence, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
