"""Read-only browsing state for the RE Database workspace (no imgui here).

Every lookup goes through `tools.re.redb.query` on a background thread, each job with its own
`mode=ro` connection to `<db_dir>/re.sqlite`; `poll()` hands finished results to the UI thread.
"""
from __future__ import annotations

import sqlite3
from concurrent.futures import Future, ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

from tools.re.redb import config, query

SEARCH_KINDS = ("all", "name", "string", "finding", "decomp")


@dataclass
class FunctionView:
    ref: str
    detail: dict | None = None
    pseudocode: str = ""
    xrefs: list[dict] = field(default_factory=list)
    error: str = ""


class RedbSession:
    def __init__(self, db_dir: Path | str):
        self._pool = ThreadPoolExecutor(max_workers=2, thread_name_prefix="redb")
        self._jobs: dict[str, tuple[Future, Callable[[object], None]]] = {}
        self.db_dir = Path(db_dir)
        self.binaries: list[dict] = []
        self.status_rows: list[dict] = []
        self.results: list[dict] = []
        self.search_error = ""
        self.search_notice = ""  # query.INDEX_NOTICE, shown by the search box rather than as a result row
        self.status_error = ""
        self.binaries_error = ""
        self.view: FunctionView | None = None
        self.history: list[str] = []
        self.history_pos = -1
        self._last_search: tuple | None = None

    # -- database ------------------------------------------------------------

    @property
    def index_path(self) -> Path:
        return self.db_dir / "re.sqlite"

    def set_db_dir(self, db_dir: Path | str) -> None:
        self.db_dir = Path(db_dir)
        # Jobs still reading the old database must not land their results on the new one.
        self._jobs.clear()
        self.binaries, self.status_rows, self.results = [], [], []
        self.view, self.history, self.history_pos = None, [], -1
        self.refresh_overview()

    @staticmethod
    def _connect(db_dir: Path) -> sqlite3.Connection:
        index = db_dir / "re.sqlite"
        if not index.is_file():
            raise FileNotFoundError(f"no RE database at {index}")
        # query.decomp/search read per-binary decomp dbs from config.RE_DIR, as the redb CLI's --db does.
        config.RE_DIR = db_dir
        con = sqlite3.connect(f"{index.resolve().as_uri()}?mode=ro", uri=True)
        con.row_factory = sqlite3.Row
        return con

    @classmethod
    def _run(cls, db_dir: Path, fn: Callable[[sqlite3.Connection], object]) -> object:
        con = cls._connect(db_dir)
        try:
            return fn(con)
        except sqlite3.OperationalError as exc:
            if "no such table" in str(exc):
                raise RuntimeError(f"{db_dir / 're.sqlite'} is not a redb database (run `modkit re ingest`)") from exc
            raise
        finally:
            con.close()

    def _submit(self, slot: str, fn: Callable[[sqlite3.Connection], object], apply: Callable[[object], None]) -> None:
        """A newer job in the same slot supersedes the older one's result."""
        self._jobs[slot] = (self._pool.submit(self._run, self.db_dir, fn), apply)

    def busy(self, slot: str) -> bool:
        return slot in self._jobs

    def poll(self) -> None:
        for slot, (future, apply) in list(self._jobs.items()):
            if future.done():
                del self._jobs[slot]
                try:
                    result = future.result()
                except Exception as exc:  # surfaced in the panel, not raised into the frame
                    result = exc
                apply(result)

    def wait(self, timeout: float = 30.0) -> None:
        """Block until all queued jobs finish and are applied (tests, screenshots)."""
        while self._jobs:
            for future, _ in list(self._jobs.values()):
                future.exception(timeout=timeout)
            self.poll()

    def close(self) -> None:
        self._pool.shutdown(wait=False, cancel_futures=True)

    # -- overview ------------------------------------------------------------

    def refresh_overview(self) -> None:
        # Separate jobs: status() also reads binaries.yaml, whose failure must not empty the filters.
        self._submit("binaries", query.binaries, self._apply_binaries)
        self._submit("status", query.status, self._apply_status)

    def _apply_binaries(self, result) -> None:
        if isinstance(result, Exception):
            self.binaries, self.binaries_error = [], str(result)
        else:
            self.binaries, self.binaries_error = result, ""

    def _apply_status(self, result) -> None:
        if isinstance(result, Exception):
            self.status_rows, self.status_error = [], str(result)
        else:
            self.status_rows, self.status_error = result, ""

    def games(self) -> list[str]:
        return sorted({b["game"] for b in self.binaries if b["game"]})

    def binary_keys(self, game: str | None = None) -> list[str]:
        return [b["key"] for b in self.binaries if not game or b["game"] == game]

    # -- search --------------------------------------------------------------

    def search(self, text: str, kind: str = "all", game: str | None = None, binary: str | None = None) -> None:
        if not text.strip():
            return
        self._last_search = (text, kind, game, binary)
        kinds = ("name", "string", "finding", "decomp") if kind == "all" else (kind,)
        self._submit("search", lambda con: query.search(con, text, kinds=kinds, game=game or None,
                                                        binary=binary or None), self._apply_search)

    def repeat_search(self) -> None:
        if self._last_search:
            self.search(*self._last_search)

    def _apply_search(self, result) -> None:
        if isinstance(result, Exception):
            self.results, self.search_error, self.search_notice = [], str(result), ""
        else:
            self.results = [r for r in result if r.get("text") != query.INDEX_NOTICE]
            self.search_error = ""
            self.search_notice = query.INDEX_NOTICE if len(self.results) < len(result) else ""

    @staticmethod
    def result_ref(row: dict) -> str | None:
        """The function a search row leads to (`key:0xRVA`), or None (notes, symbol-only names)."""
        if row["kind"] == "name":
            return f"{row['binary']}:{row['rva']:#x}" if row.get("target") == "function" else None
        if row["kind"] == "finding" and row.get("rva") is not None:
            return f"{row['binary']}:{row['rva']:#x}"
        if row["kind"] == "decomp":
            return f"{row['binary']}:{row['func_rva']:#x}"
        if row["kind"] == "string":
            users = [u for u in row.get("used_by", []) if u.get("func_rva") is not None]
            return f"{row['binary']}:{users[0]['func_rva']:#x}" if users else None
        return None

    # -- function view + history --------------------------------------------

    def open(self, ref: str) -> None:
        if 0 <= self.history_pos < len(self.history) and self.history[self.history_pos] == ref:
            self._load(ref)
            return
        del self.history[self.history_pos + 1:]
        self.history.append(ref)
        self.history_pos = len(self.history) - 1
        self._load(ref)

    def can_back(self) -> bool:
        return self.history_pos > 0

    def can_forward(self) -> bool:
        return self.history_pos < len(self.history) - 1

    def back(self) -> None:
        if self.can_back():
            self.history_pos -= 1
            self._load(self.history[self.history_pos])

    def forward(self) -> None:
        if self.can_forward():
            self.history_pos += 1
            self._load(self.history[self.history_pos])

    def _load(self, ref: str) -> None:
        self.view = FunctionView(ref)

        def job(con):
            detail = query.func(con, ref)
            resolved = detail["resolved"]
            return detail, query.decomp(con, resolved), query.xrefs(con, resolved, "both", 1)

        self._submit("function", job, lambda result: self._apply_function(ref, result))

    def _apply_function(self, ref: str, result) -> None:
        view = FunctionView(ref)
        if isinstance(result, Exception):
            view.error = str(result)
        else:
            view.detail, view.pseudocode, view.xrefs = result
        self.view = view

    @staticmethod
    def xref_ref(binary: str, row: dict, direction: str) -> str | None:
        """Where clicking a caller (its containing function) or callee (its target) leads."""
        if row.get("kind") == "str":
            return None
        rva = row.get("func_rva") if direction == "callers" else row.get("to_rva")
        return f"{binary}:{rva:#x}" if rva is not None else None
