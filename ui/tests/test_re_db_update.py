"""Headless tests for the RE Database "Game updated" flow: tmp manifests, fixture exes (a Steam update
simulated by copying fixture b over the exe registered from fixture a) and a fake ingest launcher.
Never the real binaries.yaml or db/re."""
from __future__ import annotations

import sqlite3
import sys

import pytest

from tests.redb.test_game_update import NEW, OLD, game_dir, register, steam_update, write_manifest
from tools.re.redb import config, db, manifest, query
from ui.toolkit.workspaces.re_db_session import RedbSession
from ui.toolkit.workspaces.re_db_update import GameUpdates
from ui.toolkit.workspaces.re_db_workspace import ReDbWorkspace


class FakeProcess:
    def __init__(self):
        self.returncode = None
        self.pid = 4242

    def poll(self):
        return self.returncode


class FakeLauncher:
    def __init__(self):
        self.calls = []
        self.process = FakeProcess()

    def __call__(self, cmd, cwd, log_path):
        self.calls.append((cmd, cwd, log_path))
        return self.process


@pytest.fixture
def env(tmp_path, monkeypatch):
    """A redb DB with OLD registered from the old build, then a Steam update of its exe."""
    for name, sub in (("RE_ADDRLIB_DIR", "addrlib"), ("RE_COMMONLIB_DIR", "commonlib")):
        (tmp_path / sub).mkdir()
        monkeypatch.setenv(name, str(tmp_path / sub))
    monkeypatch.setattr(config, "FINDINGS_ROOTS", [])
    monkeypatch.setattr(config, "RE_DIR", tmp_path / "db")
    real_path = manifest._DEFAULT_PATH
    real_manifest = real_path.read_text(encoding="utf-8")
    folder = game_dir(tmp_path)
    man = write_manifest(tmp_path, folder)
    con = db.open_db(tmp_path / "db" / "re.sqlite")
    register(con, man, OLD)
    con.close()
    steam_update(folder)
    yield tmp_path / "db", man, tmp_path
    assert real_path.read_text(encoding="utf-8") == real_manifest


@pytest.fixture
def flow(env):
    db_dir, man, root = env
    session = RedbSession(db_dir)
    launcher = FakeLauncher()
    updates = GameUpdates(session, manifest_path=man, launcher=launcher, workspace_root=root)
    yield updates, launcher
    session.close()


def _checked(updates: GameUpdates):
    updates.check()
    updates.session.wait()
    assert updates.check_error == ""
    return updates.result


def _write(db_dir, sql: str, args=()) -> None:
    con = sqlite3.connect(db_dir / "re.sqlite")
    con.execute(sql, args)
    con.commit()
    con.close()


def test_check_lists_the_updated_exe(flow):
    updates, _ = flow
    result = _checked(updates)
    (change,) = result.changes
    assert change.key == OLD and change.new_version == "1.11.244.0" and change.proposal["key"] == NEW
    assert result.unregistered == ["fx-other"]


def test_check_runs_on_a_read_only_connection(flow, monkeypatch):
    updates, _ = flow
    from tools.re.redb import game_update
    seen = []

    def spy(con, entries):
        with pytest.raises(sqlite3.OperationalError):
            con.execute("CREATE TABLE t(x)")
        seen.append([e.key for e in entries])
        return game_update.CheckResult()
    monkeypatch.setattr(game_update, "check", spy)
    _checked(updates)
    assert seen == [["fx-other", OLD]]


def test_add_updates_manifest_and_launches_cheap_ingest(flow):
    updates, launcher = flow
    db_dir, root = updates.session.db_dir, updates.workspace_root
    (change,) = _checked(updates).changes
    steps = updates.add(change, heavy=False)
    assert [e.key for e in manifest.load_manifest(updates.manifest_path)] == ["fx-other", NEW]
    ((cmd, cwd, log_path),) = launcher.calls
    assert cmd == [sys.executable, "-m", "tools.re.redb", "--db", str(db_dir), "--format", "json", "ingest", NEW]
    assert cwd == root and log_path == db_dir / "logs" / NEW / "ui-ingest.log"
    assert len(steps) == 3 and "added" in steps[0] and "retired" in steps[1] and "pid 4242" in steps[2]
    assert updates.result.changes == [] and NEW in updates.runs


def test_add_with_heavy(flow):
    updates, launcher = flow
    (change,) = _checked(updates).changes
    updates.add(change, heavy=True)
    assert launcher.calls[0][0][-3:] == ["ingest", NEW, "--heavy"]


def test_repeated_add_is_idempotent_and_does_not_start_a_second_ingest(flow):
    updates, launcher = flow
    (change,) = _checked(updates).changes
    updates.add(change, heavy=False)
    after = updates.manifest_path.read_text(encoding="utf-8")
    steps = updates.add(change, heavy=False)
    assert updates.manifest_path.read_text(encoding="utf-8") == after and len(launcher.calls) == 1
    assert "already in" in steps[0] and "already retired" in steps[1] and "already running" in steps[2]


def test_failed_launch_still_clears_the_applied_row(flow):
    updates, _ = flow
    (change,) = _checked(updates).changes

    def broken(cmd, cwd, log_path):
        raise FileNotFoundError("uv")
    updates.launcher = broken
    steps = updates.add(change, heavy=False)
    assert "could not start" in steps[-1] and updates.result.changes == []


def test_manifest_defaults_to_the_workspace_root_the_ingest_runs_from(env, monkeypatch):
    db_dir, _, root = env
    from ui.toolkit.workspaces import re_db_update
    monkeypatch.setattr(re_db_update, "is_frozen", lambda: True)
    session = RedbSession(db_dir)
    launcher = FakeLauncher()
    updates = GameUpdates(session, launcher=launcher, workspace_root=root)
    assert updates.manifest_path == root / "tools" / "re" / "redb" / "binaries.yaml"
    assert updates.manifest_path != manifest._DEFAULT_PATH
    assert re_db_update.redb_command(db_dir, ["ingest", "k"])[:4] == ["uv", "run", "--no-sync", "python"]
    session.close()


def test_progress_follows_stage_run_and_log_tail_then_refreshes_on_exit(flow, monkeypatch):
    updates, launcher = flow
    session, db_dir = updates.session, updates.session.db_dir
    (change,) = _checked(updates).changes
    updates.add(change, heavy=False)
    session.wait()
    # What the ingest subprocess writes: the new key's binary row, a running stage and its log.
    _write(db_dir, "INSERT INTO binary(key,game,product,version,variant,path,sha256,size,image_base) "
                   "VALUES(?,'fo4','game','1.11.244','retail','x','s',1,0)", (NEW,))
    _write(db_dir, "INSERT INTO stage_run SELECT id,'register',1,'h','done',NULL,NULL,NULL FROM binary WHERE key=?",
           (NEW,))
    _write(db_dir, "INSERT INTO stage_run SELECT id,'pe',1,'h','running',NULL,NULL,NULL FROM binary WHERE key=?",
           (NEW,))
    (db_dir / "logs" / NEW).mkdir(parents=True, exist_ok=True)
    (db_dir / "logs" / NEW / "pe.log").write_text("line one\nfunctions: 123\n", encoding="utf-8")
    monkeypatch.setattr(manifest, "_DEFAULT_PATH", updates.manifest_path)  # status() lists manifest keys
    searches = []
    monkeypatch.setattr(session, "repeat_search", lambda: searches.append(1))
    updates.tick(now=100.0)
    session.wait()
    p = updates.progress(NEW)
    assert p.stage == "pe" and p.done == 1 and "functions: 123" in p.log_tail and p.running
    _write(db_dir, "UPDATE stage_run SET status='done' WHERE stage='pe'")
    launcher.process.returncode = 0
    updates.tick(now=100.5)
    session.wait()
    assert searches == [1] and not updates.progress(NEW).running and updates.runs[NEW].returncode == 0


def test_reopened_workspace_shows_running_ingest_from_stage_run(flow, monkeypatch):
    updates, _ = flow
    session, db_dir = updates.session, updates.session.db_dir
    _write(db_dir, "INSERT INTO stage_run SELECT id,'pe',1,'h','running',NULL,NULL,NULL FROM binary WHERE key=?",
           (OLD,))
    reopened = GameUpdates(session, manifest_path=updates.manifest_path, launcher=FakeLauncher(),
                           workspace_root=updates.workspace_root)
    session.refresh_overview()
    session.wait()
    assert reopened.running_keys() == [OLD] and reopened.progress(OLD).stage == "pe"
    searches = []
    monkeypatch.setattr(session, "repeat_search", lambda: searches.append(1))
    reopened.tick(now=10.0)
    session.wait()
    _write(db_dir, "UPDATE stage_run SET status='done' WHERE stage='pe'")
    reopened.tick(now=20.0)
    session.wait()
    reopened.tick(now=20.1)
    assert reopened.running_keys() == [] and searches == [1]


def test_workspace_dialog_flow(env, monkeypatch):
    db_dir, man, root = env
    launcher = FakeLauncher()
    ws = ReDbWorkspace()
    ws.apply_settings({"db_dir": str(db_dir)})
    ws.updates = GameUpdates(ws.session, manifest_path=man, launcher=launcher, workspace_root=root)
    ws.updates.check()
    ws.session.wait()
    (change,) = ws.updates.result.changes
    ws.start_update(change)
    assert not ws._update_heavy and change.proposal["key"] == NEW
    with monkeypatch.context() as mp:  # e.g. binaries.yaml locked by an editor: shown, not raised into the frame
        mp.setattr(manifest, "_write_atomic", lambda path, text: (_ for _ in ()).throw(PermissionError("denied")))
        ws.confirm_update()
    assert "denied" in ws._update_error and launcher.calls == [] and ws._update_steps == []
    ws._update_heavy = True
    ws.confirm_update()
    assert ws._update_error == "" and len(ws._update_steps) == 3 and launcher.calls[0][0][-1] == "--heavy"
    ws.cleanup()


def test_retired_key_rows_stay_searchable_after_add(flow):
    updates, _ = flow
    (change,) = _checked(updates).changes
    updates.add(change, heavy=False)
    con = sqlite3.connect(updates.session.index_path)
    con.row_factory = sqlite3.Row
    assert [b["key"] for b in query.binaries(con)] == [OLD]
    con.close()
