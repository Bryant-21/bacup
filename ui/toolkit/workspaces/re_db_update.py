"""The RE Database workspace's "Game updated" flow (no imgui here): check every manifest exe for a
Steam in-place update, swap the manifest over to the new build, start `ingest <new key>` as a
detached subprocess, and follow it through `stage_run` and the per-stage logs.

These are the workspace's only writes: the manifest file (`tools.re.redb.game_update.apply`) and the
subprocess launch. The check itself is a read-only session job.
"""
from __future__ import annotations

import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from tools.re.redb import game_update, manifest
from ui.toolkit.app_paths import get_app_root, is_frozen
from ui.toolkit.workspaces.re_db_session import RedbSession

REFRESH_SECONDS = 2.0
_TAIL_BYTES = 4096
_INGEST_LOG = "ui-ingest.log"
MANIFEST_RELPATH = Path("tools/re/redb/binaries.yaml")


def redb_command(db_dir: Path, args: list[str]) -> list[str]:
    """Same invocation as cli/re_commands.py (`--db` is what RE_DB_DIR sets)."""
    interpreter = ["uv", "run", "--no-sync", "python"] if is_frozen() else [sys.executable]
    return [*interpreter, "-m", "tools.re.redb", "--db", str(db_dir), "--format", "json", *args]


def spawn_detached(cmd: list[str], cwd: Path, log_path: Path) -> subprocess.Popen:
    """Start `cmd` with its own process group and no console, so it outlives the toolkit."""
    log_path.parent.mkdir(parents=True, exist_ok=True)
    flags = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
    with open(log_path, "ab") as log:
        return subprocess.Popen(cmd, cwd=cwd, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                creationflags=flags, close_fds=True)


@dataclass
class IngestRun:
    key: str
    heavy: bool
    handle: object  # has .poll() -> exit code or None
    log_path: Path
    returncode: int | None = None


@dataclass(frozen=True)
class Progress:
    running: bool
    stage: str | None  # the stage stage_run shows as running
    done: int
    total: int
    log_tail: str


def _tail(path: Path) -> str:
    try:
        size = path.stat().st_size
        with open(path, "rb") as f:
            f.seek(max(0, size - _TAIL_BYTES))
            text = f.read().decode("utf-8", "replace")
    except OSError:
        return ""
    return text.split("\n", 1)[-1] if size > _TAIL_BYTES else text  # drop the cut first line


class GameUpdates:
    def __init__(self, session: RedbSession, *, manifest_path: Path | None = None,
                 launcher: Callable[[list[str], Path, Path], object] = spawn_detached,
                 workspace_root: Path | None = None):
        self.session = session
        self.launcher = launcher
        self.workspace_root = Path(workspace_root or get_app_root())
        # The manifest the ingest subprocess reads (`-m tools.re.redb` run from workspace_root), never
        # this process's own copy: in a frozen build that is inside the bundle.
        self.manifest_path = Path(manifest_path or self.workspace_root / MANIFEST_RELPATH)
        self.result: game_update.CheckResult | None = None
        self.check_error = ""
        self.runs: dict[str, IngestRun] = {}
        self._last_refresh = 0.0
        self._was_running: set[str] = set()

    # -- check ---------------------------------------------------------------

    def check(self) -> None:
        path = self.manifest_path
        self.session._submit("update_check", lambda con: game_update.check(con, manifest.load_manifest(path)),
                             self._apply_check)

    @property
    def checking(self) -> bool:
        return self.session.busy("update_check")

    def _apply_check(self, result) -> None:
        if isinstance(result, Exception):
            self.result, self.check_error = None, str(result)
        else:
            self.result, self.check_error = result, ""

    # -- add new version -----------------------------------------------------

    def add(self, change: game_update.Change, heavy: bool) -> list[str]:
        """Swap the manifest to the new build (key derived, never typed), then start `ingest <key>`
        unless one is already running for it; one line per step."""
        key, steps = game_update.apply(change, self.manifest_path)
        if self.result is not None:
            self.result.changes = [c for c in self.result.changes if c.key != change.key]
        self.session.refresh_overview()
        if key in self.running_keys():
            steps.append(f"`ingest {key}` is already running; not started again")
            return steps
        db_dir = self.session.db_dir
        args = ["ingest", key, *(["--heavy"] if heavy else [])]
        log_path = db_dir / "logs" / key / _INGEST_LOG
        try:
            handle = self.launcher(redb_command(db_dir, args), self.workspace_root, log_path)
        except OSError as exc:
            steps.append(f"could not start the ingest: {exc}; run `modkit re ingest {key}`")
            return steps
        self.runs[key] = IngestRun(key, heavy, handle, log_path)
        steps.append(f"started `ingest {key}`{' --heavy' if heavy else ''} (pid {getattr(handle, 'pid', '?')}); "
                     f"output in {log_path}")
        return steps

    # -- progress ------------------------------------------------------------

    def _row(self, key: str) -> dict | None:
        return next((r for r in self.session.status_rows if r["key"] == key), None)

    def running_keys(self) -> list[str]:
        """Keys with a live ingest started here, or with a stage `stage_run` shows as running (an
        ingest started before the workspace was reopened)."""
        keys = [k for k, run in self.runs.items() if run.returncode is None]
        keys += [r["key"] for r in self.session.status_rows
                 if "running" in r.values() and r["key"] not in keys]
        return keys

    def progress(self, key: str) -> Progress:
        row = self._row(key) or {}
        stages = {k: v for k, v in row.items() if k != "key"}
        stage = next((s for s, v in stages.items() if v == "running"), None)
        run = self.runs.get(key)
        log = self.session.db_dir / "logs" / key / f"{stage}.log" if stage else (run.log_path if run else None)
        return Progress(running=key in self.running_keys(), stage=stage,
                        done=sum(v == "done" for v in stages.values()), total=len(stages),
                        log_tail=_tail(log) if log else "")

    def tick(self, now: float | None = None) -> None:
        """Per frame: reap finished ingests and, while any runs, refresh the status every few seconds.
        When one finishes, refresh the status and the last search."""
        now = time.monotonic() if now is None else now
        finished = False
        for run in self.runs.values():
            if run.returncode is None and (code := run.handle.poll()) is not None:
                run.returncode, finished = code, True
        running = set(self.running_keys())
        finished |= bool(self._was_running - running)
        self._was_running = running
        if finished:
            self.session.refresh_overview()
            self.session.repeat_search()
            self._last_refresh = now
        elif running and now - self._last_refresh >= REFRESH_SECONDS and not self.session.busy("status"):
            self.session.refresh_overview()
            self._last_refresh = now
