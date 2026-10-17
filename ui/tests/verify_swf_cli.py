"""Verify the actual packaged CLIs using disposable editor projects and movies."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from ui.swf_editor.document import SwfEditingDocument
from ui.tests.test_swf_editor_workflow import artwork, compiled


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--installed-executable", type=Path,
                        default=Path.home() / ".local" / "bin" / "modkit.exe")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    directory = args.output.resolve()
    directory.relative_to(repo)
    directory.mkdir(parents=True, exist_ok=True)
    fixtures = directory / "packaged-cli-fixtures"
    fixtures.mkdir(exist_ok=True)
    executables = {"repository": repo / "modkit.exe", "PATH": args.installed_executable.resolve()}
    assert all(path.is_file() for path in executables.values()), executables
    assert executables["repository"] != executables["PATH"], "Verify distinct repository and installed packages"
    hashes = {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in executables.items()}
    assert len(set(hashes.values())) == 1, "Repository and PATH CLIs must match"

    art = artwork()
    art.scene.layers[0].keyframes[0].duration = 3
    art.scene.insert_keyframe(2)
    art.scene.layers[0].keyframes[1].display_list[0].transform.x = 150
    art.scene.layers[0].keyframes[1].label = "B21_Finish"
    art.save(fixtures / "art.swfproj")
    source = SwfEditingDocument.menu()
    external = fixtures / "DocumentMenu.as"
    external.write_text(source.sources["DocumentMenu.as"], encoding="utf-8")
    external_hash = hashlib.sha256(external.read_bytes()).hexdigest()
    source.sources["DocumentMenu.as"] = source.sources["DocumentMenu.as"].replace(
        'title.text = "New Scaleform menu";', 'title.text = "Buffered source"; trace("B21_CLI_BUFFERED_SOURCE");')
    source.save(fixtures / "source.swfproj")
    original = fixtures / "original-menu.swf"
    original.write_bytes(compiled())
    original_hash = hashlib.sha256(original.read_bytes()).hexdigest()
    binary = SwfEditingDocument.open(original)
    binary.save(fixtures / "binary-session.swfproj")
    evidence = {"passed": False, "executable_sha256": hashes, "checks": []}

    def invoke(executable, arguments, *, succeeds=True):
        result = subprocess.run([str(executable), *map(str, arguments)], cwd=repo,
                                capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
        if succeeds:
            assert result.returncode == 0, result.stderr or result.stdout
        else:
            assert result.returncode != 0, "Binary editing sessions must not be rebuilt by the authoring packer"
        return result.stdout + result.stderr

    try:
        for name, executable in executables.items():
            doctor = json.loads(invoke(executable, ["--format", "json", "doctor", "--workspace", repo]))
            assert Path(doctor["executable"]).resolve() == executable.resolve()
            assert doctor["frozen"] and doctor["cli_source_freshness"] == "current", doctor["cli_source_freshness"]
            assert all(entry["available"] for entry in doctor["native"])
            outputs = {}
            for kind in ("art", "source"):
                output = fixtures / f"{name}-{kind}.swf"
                invoke(executable, ["swf", "pack", fixtures / f"{kind}.swfproj", "-o", output])
                info = json.loads(invoke(executable, ["--format", "json", "swf", "inspect", output]))
                bindings = json.loads(invoke(executable, ["swf", "symbols", "validate", output]))
                assert bindings["unbacked"] == []
                reopened = SwfEditingDocument.open(output)
                assert reopened.build() == output.read_bytes()
                reopened.save(fixtures / f"{name}-{kind}-reopened.swfproj")
                reopened_again = SwfEditingDocument.open(fixtures / f"{name}-{kind}-reopened.swfproj")
                second_export = fixtures / f"{name}-{kind}-reopened.swf"
                reopened_again.export(second_export)
                if kind == "art":
                    assert reopened.mode == "visual" and info["frame_count"] == 3
                    entry = reopened.scene.layers[0].keyframes[-1].display_list[0]
                    assert entry.transform.x == 150
                    assert reopened_again.scene.to_project() == reopened.scene.to_project()
                else:
                    assert reopened.mode == "binary" and info["canvas"] == "1280x720"
                    disassembly = invoke(executable, ["swf", "abc", "disasm", output, "DocumentMenu"])
                    assert "B21_CLI_BUFFERED_SOURCE" in disassembly
                    assert second_export.read_bytes() == output.read_bytes()
                outputs[kind] = {"bytes": output.stat().st_size, "info": info,
                                 "unbacked_classes": bindings["unbacked"], "save_reopen_export": True}
            rejected = fixtures / f"{name}-rejected.swf"
            refusal = invoke(executable, ["swf", "pack", fixtures / "binary-session.swfproj", "-o", rejected], succeeds=False)
            assert "Binary SWF edit" in refusal and not rejected.exists(), refusal
            assert hashlib.sha256(original.read_bytes()).hexdigest() == original_hash
            assert hashlib.sha256(external.read_bytes()).hexdigest() == external_hash
            evidence["checks"].append({"executable": str(executable), "source_freshness": doctor["cli_source_freshness"],
                                       "built_at": doctor["build"]["built_at"], "source_sha256": doctor["build"]["source"]["sha256"],
                                       "outputs": outputs, "binary_session_refused": True,
                                       "external_source_and_original_unchanged": True})
        evidence["passed"] = True
        print("Repository and PATH CLIs packed artwork and buffered AS3, reopened/saved/exported both, and refused opaque sessions")
    finally:
        (directory / "cli-rebuild-evidence.json").write_text(json.dumps(evidence, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
