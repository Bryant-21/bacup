"""Headless tests for the RE Database workspace against a redb fixture DB built from tests/redb's
fixture binaries (never the real db/re)."""
from __future__ import annotations

import shutil
import sqlite3
import zlib
from pathlib import Path

import pytest

from tests.redb.helpers import FIXTURES, _rva
from tools.re.redb import config, db, findings, query, runner
from tools.re.redb.manifest import Entry
from ui.toolkit.workspaces.re_db_session import RedbSession
from ui.toolkit.workspaces.re_db_workspace import ReDbWorkspace, default_db_dir

_PSEUDO = "int sum_loop(int n) { int squares_total = 0; return squares_total; }"


def _fx_entries() -> list[Entry]:
    return [Entry(key=f"fx-{v}", game="fx", product="game", version="1", variant=v,
                  path=FIXTURES / v / "fixture.exe", pdb=FIXTURES / v / "fixture.pdb", pdb_official=True,
                  addrlib_runtime=None, anchor=False, match_with=["fx-b"] if v == "a" else [])
            for v in ("a", "b")]


def _fid(con, key: str, name: str) -> int:
    return con.execute("SELECT f.id FROM function f JOIN binary b ON b.id=f.binary_id WHERE b.key=? AND f.rva=?",
                       (key, _rva(con, key, name))).fetchone()[0]


def build_fixture_db(root: Path) -> Path:
    """redb index + an fx-a decomp db under `root/db`, with one xref, one finding and one pseudocode."""
    with pytest.MonkeyPatch.context() as mp:
        # Same isolation as tests/redb/conftest.py: no real Address Library, CommonLib or findings docs.
        for env, sub in (("RE_ADDRLIB_DIR", "addrlib"), ("RE_COMMONLIB_DIR", "commonlib")):
            (root / sub).mkdir()
            mp.setenv(env, str(root / sub))
        mp.setattr(config, "FINDINGS_ROOTS", [])
        mp.setattr(config, "RE_DIR", root / "db")
        con = db.open_db(root / "db" / "re.sqlite")
        entries = _fx_entries()
        runner.run(con, entries, ["findings"], manifest=entries)
        a = con.execute("SELECT id FROM binary WHERE key='fx-a'").fetchone()[0]
        main, loop = _rva(con, "fx-a", "main"), _rva(con, "fx-a", "sum_loop")
        con.execute("INSERT INTO xref VALUES(?,?,?,?,?,'call')",
                    (a, main + 5, _fid(con, "fx-a", "main"), loop, _fid(con, "fx-a", "sum_loop")))
        findings.add(con, "sum_loop", "squares", "sum of squares", binary="fx-a")
        con.commit()
        con.close()
        dcon = db.open_decomp("fx-a")
        dcon.execute("INSERT INTO decomp VALUES(?,?,'ok',NULL)", (loop, zlib.compress(_PSEUDO.encode())))
        dcon.execute("INSERT INTO decomp_fts(rowid, code) VALUES(?,?)", (loop, _PSEUDO))
        dcon.commit()
        dcon.close()
    return root / "db"


@pytest.fixture(scope="module")
def db_dir(tmp_path_factory):
    return build_fixture_db(tmp_path_factory.mktemp("redb_ui"))


@pytest.fixture
def session(db_dir):
    s = RedbSession(db_dir)
    s.refresh_overview()
    s.wait()
    yield s
    s.close()


def test_overview_lists_binaries_and_status(session):
    assert session.status_error == ""
    assert session.games() == ["fx"]
    assert session.binary_keys("fx") == ["fx-a", "fx-b"]
    row = next(r for r in session.status_rows if r["key"] == "fx-a")
    assert row["findings"] == "done" and row["register"] == "done"


def test_connection_is_read_only(session):
    with pytest.raises(sqlite3.OperationalError):
        session._run(session.db_dir, lambda con: con.execute("CREATE TABLE t(x)"))


def test_name_search_opens_function_detail(session):
    session.search("sum_loop", "name", binary="fx-a")
    session.wait()
    assert session.search_error == ""
    row = next(r for r in session.results if r["kind"] == "name" and r["text"] == "sum_loop")
    assert row["binary"] == "fx-a" and row["source"] == "official_pdb"
    ref = RedbSession.result_ref(row)
    assert ref == f"fx-a:{_rva(None, 'fx-a', 'sum_loop'):#x}"
    session.open(ref)
    session.wait()
    d = session.view.detail
    assert session.view.error == ""
    assert d["primary_name"] == "sum_loop"
    assert sorted(m["binary"] for m in d["members"]) == ["fx-a", "fx-b"]
    assert ("squares", False) in {(f["title"], f["carried"]) for f in d["findings"]}
    assert _PSEUDO in session.view.pseudocode
    callers = session.view.xrefs[0]["callers"]
    assert [(c["name"], c["kind"]) for c in callers] == [("main", "call")]


def test_pseudocode_missing_is_reported(session):
    session.open(f"fx-b:{_rva(None, 'fx-b', 'sum_loop'):#x}")
    session.wait()
    assert "not decompiled yet" in session.view.pseudocode


def test_search_kinds_and_filters(session):
    session.search("redb-unique", "string", game="fx")
    session.wait()
    strings = [r for r in session.results if r["kind"] == "string"]
    assert {r["binary"] for r in strings} == {"fx-a", "fx-b"}
    assert all(RedbSession.result_ref(r) for r in strings)
    session.search("squares_total", "decomp")
    session.wait()
    assert [(r["kind"], r["binary"]) for r in session.results] == [("decomp", "fx-a")]
    session.search("squares", "finding")
    session.wait()
    assert [r["text"] for r in session.results] == ["squares"]


def test_xref_navigation_and_history(session):
    loop = f"fx-a:{_rva(None, 'fx-a', 'sum_loop'):#x}"
    session.open(loop)
    session.wait()
    caller = session.view.xrefs[0]["callers"][0]
    target = RedbSession.xref_ref("fx-a", caller, "callers")
    assert target == f"fx-a:{_rva(None, 'fx-a', 'main'):#x}"
    session.open(target)
    session.wait()
    assert session.view.detail["primary_name"] == "main"
    callees = session.view.xrefs[0]["callees"]
    assert any(c["name"] == "sum_loop" for c in callees)
    assert all(RedbSession.xref_ref("fx-a", c, "callees") is None for c in callees if c["kind"] == "str")
    assert session.can_back() and not session.can_forward()
    session.back()
    session.wait()
    assert session.view.ref == loop and session.can_forward()
    session.forward()
    session.wait()
    assert session.view.ref == target


def test_unknown_function_and_missing_db_report_errors(session, tmp_path):
    session.open("fx-a:0x1")
    session.wait()
    assert session.view.error
    missing = RedbSession(tmp_path / "nope")
    missing.refresh_overview()
    missing.wait()
    assert "no RE database" in missing.status_error
    missing.close()


def test_workspace_settings_and_result_selection(db_dir):
    ws = ReDbWorkspace()
    assert ws.id == "re_db"
    assert ws.get_settings_defaults() == {"db_dir": default_db_dir()}
    assert default_db_dir().replace("\\", "/").endswith("db/re")
    ws.apply_settings({"db_dir": str(db_dir)})
    assert ws.collect_settings() == {"db_dir": str(db_dir)}
    ws._query, ws._kind_idx = "sum_loop", 1
    ws.run_search()
    ws.session.wait()
    idx = next(i for i, r in enumerate(ws.session.results) if r["binary"] == "fx-b" and r["text"] == "sum_loop")
    ws.select_result(idx)
    ws.session.wait()
    assert ws.session.view.detail["resolved"].startswith("fx-b:")
    ws.cleanup()


def test_workspace_is_registered():
    from ui.toolkit.workspaces import _WORKSPACE_SPECS
    assert ("re_db", "ui.toolkit.workspaces.re_db_workspace", "ReDbWorkspace") in _WORKSPACE_SPECS


def test_function_detail_shows_matched_tier_per_member(session):
    session.open(f"fx-a:{_rva(None, 'fx-a', 'sum_loop'):#x}")
    session.wait()
    members = {m["binary"]: m for m in session.view.detail["members"]}
    (match,) = members["fx-a"]["matches"]
    assert match["with"] == f"fx-b:{_rva(None, 'fx-b', 'sum_loop'):#x}" and match["tier"].startswith("T")
    assert members["fx-b"]["matches"][0]["tier"] == match["tier"]


def test_db_dir_setting_survives_settings_close(db_dir, tmp_path):
    from ui.toolkit.settings import ToolkitSettings
    settings = ToolkitSettings(path=tmp_path / "settings.json", editor_settings_path=tmp_path / "editor.json")
    ws = ReDbWorkspace(toolkit_settings=settings)
    ws.apply_settings(settings.get_workspace_settings("re_db"))
    ws.change_db_dir(str(db_dir))
    # Closing the Settings window re-applies the stored workspace settings to every workspace.
    ws.apply_settings(settings.get_workspace_settings("re_db"))
    assert ws.session.db_dir == db_dir
    reloaded = ToolkitSettings(path=tmp_path / "settings.json", editor_settings_path=tmp_path / "editor.json")
    assert reloaded.get_workspace_settings("re_db")["db_dir"] == str(db_dir)
    ws.session.wait()
    assert ws.session.binary_keys() == ["fx-a", "fx-b"]
    ws.cleanup()


def test_db_switch_drops_in_flight_jobs(session, tmp_path):
    session.search("sum_loop", "name")
    session.set_db_dir(tmp_path / "other")
    session.wait()
    assert session.results == [] and "no RE database" in session.status_error


def test_reopening_current_function_keeps_one_history_entry(session):
    ref = f"fx-a:{_rva(None, 'fx-a', 'sum_loop'):#x}"
    session.open(ref)
    session.open(ref)
    session.wait()
    assert session.history == [ref] and not session.can_back()


def test_non_redb_file_gets_a_clear_message(tmp_path):
    (tmp_path / "re.sqlite").write_bytes(b"")
    s = RedbSession(tmp_path)
    s.refresh_overview()
    s.wait()
    assert "not a redb database" in s.binaries_error
    s.close()


def test_status_failure_keeps_filter_lists(db_dir, monkeypatch):
    from tools.re.redb import query

    def broken(con):
        raise FileNotFoundError("binaries.yaml")
    monkeypatch.setattr(query, "status", broken)
    s = RedbSession(db_dir)
    s.refresh_overview()
    s.wait()
    assert s.binary_keys() == ["fx-a", "fx-b"] and "binaries.yaml" in s.status_error
    s.close()


def test_search_notice_until_substring_index_is_built(db_dir, tmp_path):
    session = RedbSession(db_dir)
    session.search("um_loo", "name")
    session.wait()
    assert session.search_notice == "" and [r["text"] for r in session.results] == ["sum_loop"] * len(session.results)
    session.close()
    old = tmp_path / "old"
    shutil.copytree(db_dir, old)
    con = sqlite3.connect(old / "re.sqlite")
    con.executescript("DROP TABLE name_tri; DELETE FROM meta WHERE k='name_tri_built';")
    con.close()
    session = RedbSession(old)
    session.search("sum_lo", "name")
    session.wait()
    assert session.search_notice == query.INDEX_NOTICE
    assert session.results and all(r["text"] != query.INDEX_NOTICE for r in session.results)
    session.close()
