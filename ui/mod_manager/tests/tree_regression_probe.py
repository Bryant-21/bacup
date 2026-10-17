"""Compare repository-owned tree renderers in the same disposable native host.

Use the existing framebuffer/frame-delta probe rather than a custom stopwatch.
The baseline is read from the local Git HEAD, never checked out or written over
working files. It is developer test code, not an installer extension mechanism.
"""
import argparse
import cProfile
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType
from unittest.mock import patch


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline-tree", action="store_true")
    parser.add_argument("--baseline-revision", default="HEAD", help="Select an immutable local commit for reproducible comparisons.")
    parser.add_argument("--disable-observer", action="store_true")
    parser.add_argument("--profile", type=Path)
    args, render_args = parser.parse_known_args()
    from . import render_workspace
    from creation_lib.mod_manager.metadata_observer import Observer
    from ui.mod_manager import workspace
    tree = workspace.draw_tree
    baseline = ""
    if args.baseline_tree:
        repository = Path(__file__).resolve().parents[3]
        baseline = subprocess.check_output(["git", "rev-parse", "--verify", args.baseline_revision + "^{commit}"], cwd=repository, text=True).strip()
        source = subprocess.check_output(["git", "show", baseline + ":ui/mod_manager/mod_list_ui.py"], cwd=repository)
        module = ModuleType("ui.mod_manager._local_baseline_tree")
        module.__package__ = "ui.mod_manager"
        exec(compile(source, "<repository-owned-baseline-tree>", "exec"), module.__dict__)
        tree = module.draw_tree
    observer = (lambda self, manager: None) if args.disable_observer else Observer.poll
    profiler = cProfile.Profile() if args.profile else None
    try:
        with patch.object(workspace, "draw_tree", tree), patch.object(Observer, "poll", observer), patch.object(sys, "argv", [sys.argv[0], *render_args]):
            if profiler:
                profiler.enable()
            render_workspace.main()
    finally:
        if profiler:
            profiler.disable()
            args.profile.parent.mkdir(parents=True, exist_ok=True)
            profiler.dump_stats(str(args.profile))
    if "--metrics" in render_args:
        report = Path(render_args[render_args.index("--metrics") + 1])
        data = json.loads(report.read_text(encoding="utf-8"))
        data.update(baseline_git_commit=baseline, metadata_observer_disabled=args.disable_observer)
        report.write_text(json.dumps(data, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
