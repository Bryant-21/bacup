"""Generated editor/generator corpus through the actual source/frozen helper."""
import argparse
from contextlib import nullcontext
from dataclasses import replace
import hashlib
import json
from pathlib import Path
import tempfile

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.runtime import prepare, active_sessions
from creation_lib.mod_manager.launcher import Executable, fingerprint
from creation_lib.mod_manager.process_identity import terminate_owned
from .fixtures import make_mo2
from .project_runtime_probe import launch_fixture
from .test_runtime import pe


def snapshot(folder):
    return {str(path.relative_to(folder)): ("directory" if path.is_dir() else hashlib.sha256(path.read_bytes()).hexdigest())
            for path in folder.rglob("*")}


def run(native_root, frozen_app=None, *, retain=False):
    native_root = Path(native_root).absolute()
    frozen_app = Path(frozen_app).absolute() if frozen_app else None
    with (nullcontext(tempfile.mkdtemp(prefix="modbox-tool-corpus-")) if retain else tempfile.TemporaryDirectory(prefix="modbox-tool-corpus-")) as directory:
        root = Path(directory)
        mo2 = make_mo2(root)
        provider = mo2 / "mods/Override"
        contents = {"editor.txt": "editor original", "replace.txt": "replace original", "copy.txt": "copy original",
                    "mapped.txt": "mapped! payload", "readonly.txt": "readonly original", "length.txt": "length original",
                    "attributes.txt": "attributes original", "inventory/virtual.txt": "virtual inventory", "mapped-crash.txt": "crash! payload"}
        for name, value in contents.items():
            path = provider / name
            path.parent.mkdir(exist_ok=True, parents=True)
            path.write_text(value)
        library = Library(root / "library", create=True)
        library.import_mo2(inspect_instance(mo2))
        profile = library.profiles()[0].id
        game, user = root / "game", root / "user"
        (game / "Data/inventory").mkdir(parents=True)
        (game / "Data/inventory/physical.txt").write_text("physical inventory")
        (user / "plugins").mkdir(parents=True)
        (user / "ini").mkdir()
        pe(game / "Fallout4.exe")
        plan = prepare(library, profile, Executable("fixture", "Tool fixture", str(game / "Fallout4.exe")), str(game), "",
                       user_targets=(user / "plugins", user / "ini"))
        if frozen_app:
            plan = replace(plan, bundle_root=str(Path(frozen_app).parent / "_internal/creation_lib/mod_manager/_runtime/project-vfs"))
        originals = {name: snapshot(path) for name, path in (("mo2", mo2), ("game", game), ("user", user))}
        executable = native_root / "modbox_vfs_fixture.exe"
        result = {"mode": "frozen" if frozen_app else "source", "runs": []}
        if retain:
            result["fixture_root"] = str(root)
        # Every pass is a fresh helper/manifest. Remove only generated owned tree
        # through the fixture itself on repeat; no source paths are ever cleaned.
        for index in range(4):
            crashing = index == 2
            report = root / f"tools-{index}.json"
            argv = (str(executable), "tool-crash", str(game / "Data"), str(report)) if crashing else (str(executable), "tools", str(game / "Data"), str(provider), str(report))
            launch = replace(plan.launch, argv=argv,
                             executable=replace(plan.launch.executable, binary=str(executable)), signature=fingerprint(executable))
            helper, identity, session = launch_fixture(library, replace(plan, launch=launch), f"tools-{index}", frozen_app=frozen_app)
            try:
                helper.wait(timeout=60)
                status = json.loads((session / "status.json").read_text())
                assert report.exists(), status
                record = {"checks": {"forced_crash_after_flushed_owned_write": status.get("exit_code") == 196}} if crashing else json.loads(report.read_text())
                record["helper_exit"] = helper.returncode
                record["child_exit"] = status.get("exit_code")
                if crashing:
                    result["crash"] = record
                else:
                    result["runs"].append(record)
            finally:
                if helper.poll() is None:
                    terminate_owned(helper, identity)
                    helper.wait(timeout=10)
            assert not active_sessions(library.root)
            assert originals == {name: snapshot(path) for name, path in (("mo2", mo2), ("game", game), ("user", user))}, "A physical source/game/user fixture changed"
            if not all(record["checks"].values()):
                break
        result["physical_trees_unchanged"] = True
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--native-root", type=Path, default=Path("cache/mod-manager-native/Release"))
    parser.add_argument("--frozen-app", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--retain-fixture", action="store_true", help="Keep only this generated Temp fixture for failure inspection.")
    args = parser.parse_args()
    result = run(args.native_root, args.frozen_app, retain=args.retain_fixture)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if len(result["runs"]) == 3 and all(all(run["checks"].values()) and run["child_exit"] == 0 for run in result["runs"])
                     and all(result.get("crash", {}).get("checks", {}).values()) else 1)
