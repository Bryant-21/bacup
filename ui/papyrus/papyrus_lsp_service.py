"""LspService — background thread for Papyrus LSP operations.

Single worker thread processes requests from a pending-request slot.
Results are stored in result slots, polled each frame by the UI.

Parsing is handled by the native Rust implementation (creation_lib._native.
papyrus_core), which releases the GIL inside every entry point — so the
worker thread can call parse_script directly without a subprocess.
"""
from __future__ import annotations

import logging
import os
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

from app.paths import get_db_dir as _get_db_dir

_log = logging.getLogger("toolkit.papyrus.lsp")

# Request type constants
PARSE_RESOLVE = "PARSE_RESOLVE"
COMPLETE = "COMPLETE"
DEFINITION = "DEFINITION"

_DB_DIR = _get_db_dir()


def _parse_error_recovery(
    text: str, max_iterations: int = 10
) -> tuple[list[tuple[int, int, str]], object | None]:
    """Iteratively blank lines containing errors so we can still get a clean AST.

    The native parser releases the GIL during parse_script, so this runs
    directly on the worker thread.

    Returns:
        (error_tuples, final_ast)
        error_tuples: list of (line_1indexed, col_1indexed, message)
        final_ast: ScriptNode from the clean-text parse, or None.
    """
    from creation_lib.papyrus_lsp import parse_script

    src_lines = text.split('\n')
    blanked: set[int] = set()
    error_tuples: list[tuple[int, int, str]] = []
    final_ast = None

    for _ in range(max_iterations):
        working = '\n'.join('' if i in blanked else ln
                            for i, ln in enumerate(src_lines))
        result = parse_script(text=working)
        if not result.errors:
            final_ast = result.ast
            break
        new_error = False
        for err in result.errors:
            line_0 = max(0, err.line - 1)
            if line_0 not in blanked:
                error_tuples.append((err.line, err.col, err.message))
                blanked.add(line_0)
                new_error = True
                break
        if not new_error:
            break

    return error_tuples, final_ast


@dataclass
class LspRequest:
    type: str              # PARSE_RESOLVE | COMPLETE | DEFINITION
    path: str
    text: str
    line: int = 0
    col: int = 0


class LspService:
    """Background worker thread for LSP parse/complete/definition operations."""

    def __init__(self, db_path: str | None = None, extra_source_dirs: list[str] | None = None,
                 game_id: str = "fo4"):
        self.game_id = game_id

        # Resolve DB path from game profile
        if db_path is None:
            db_path = self._resolve_db_path(game_id)
        self._db_path = db_path
        self._db = None
        self._extra_source_dirs: list[str] = list(extra_source_dirs or [])

        self._lock = threading.Lock()
        self._work_event = threading.Event()
        self._stop_flag = False
        self._thread: threading.Thread | None = None

        self._pending_parse: dict[str, LspRequest] = {}
        self._pending_complete: Optional[LspRequest] = None
        self._pending_definition: Optional[LspRequest] = None

        self._result_diagnostics: dict[str, list] = {}
        self._result_complete: Optional["CompletionResult"] = None
        self._result_definition: Optional["DefinitionResult"] = None

    @staticmethod
    def _resolve_db_path(game_id: str) -> str:
        """Resolve script DB path from game profile."""
        try:
            from creation_lib.core.game_profiles import get_profile
            profile = get_profile(game_id)
            if profile.papyrus_script_db:
                return str(_DB_DIR / profile.papyrus_script_db)
        except (ImportError, KeyError):
            pass
        # Fallback to FO4
        return str(_DB_DIR / "fo4_scripts.db")

    def start(self):
        """Start the background worker thread."""
        self._stop_flag = False
        self._thread = threading.Thread(target=self._worker, name="LspWorker", daemon=True)
        self._thread.start()
        _log.info("LspService started")

    def stop(self):
        """Signal the worker to stop and wait for it to finish."""
        self._stop_flag = True
        self._work_event.set()
        if self._thread:
            self._thread.join(timeout=3.0)
        self._thread = None
        _log.info("LspService stopped")

    def submit(self, request: LspRequest):
        """Submit a request. For PARSE_RESOLVE, replaces any pending request for the same path."""
        with self._lock:
            if request.type == PARSE_RESOLVE:
                self._pending_parse[request.path] = request
            elif request.type == COMPLETE:
                self._pending_complete = request
            elif request.type == DEFINITION:
                self._pending_definition = request
        self._work_event.set()

    def poll_diagnostics(self) -> dict[str, list]:
        """Return and clear accumulated diagnostics since last poll.

        Returns:
            Dict mapping path -> list[Diagnostic].
        """
        with self._lock:
            result = dict(self._result_diagnostics)
            self._result_diagnostics.clear()
        return result

    def poll_completions(self) -> Optional["CompletionResult"]:
        """Return and clear the latest CompletionResult, or None."""
        with self._lock:
            result = self._result_complete
            self._result_complete = None
        return result

    def poll_definition(self) -> Optional["DefinitionResult"]:
        """Return and clear the latest DefinitionResult, or None."""
        with self._lock:
            result = self._result_definition
            self._result_definition = None
        return result

    # --- Worker thread ---

    def _ensure_db(self):
        """Open scripts.db on first use (in worker thread)."""
        if self._db is None and os.path.exists(self._db_path):
            from creation_lib.papyrus_lsp import ScriptDB
            source_dirs = self._discover_source_dirs()
            self._db = ScriptDB(self._db_path, source_dirs=source_dirs)
            _log.info("LspService opened scripts.db at %s", self._db_path)

    def _discover_source_dirs(self) -> list[str]:
        """Return directories to search for user script sources."""
        dirs: list[str] = []
        # Project-local scripts: top-level and per-mod under mods/*/Scripts/Source/User/
        project_root = Path(self._db_path).parents[1]
        user_scripts = project_root / "Scripts" / "Source" / "User"
        if user_scripts.is_dir():
            dirs.append(str(user_scripts))
        for mod_user in sorted((project_root / "mods").glob("*/Scripts/Source/User")):
            norm = os.path.normpath(mod_user)
            if os.path.isdir(norm) and norm not in dirs:
                dirs.append(norm)
        # User-configured paths (including per-game scripts_user_dir /
        # scripts_base_dir sourced from ToolkitSettings by the caller)
        for p in self._extra_source_dirs:
            norm = os.path.normpath(p)
            if os.path.isdir(norm) and norm not in dirs:
                dirs.append(norm)
        return dirs

    def _warmup_imports(self):
        """Pre-import heavy modules while the user is idle to avoid a GIL stall on first use."""
        try:
            from creation_lib.papyrus_lsp import parse_script as _  # noqa: F401
            from creation_lib.papyrus_lsp import Diagnostic as _d   # noqa: F401
        except Exception:
            pass

    def _worker(self):
        """Worker thread main loop."""
        self._warmup_imports()
        self._ensure_db()
        while not self._stop_flag:
            self._work_event.wait(timeout=0.1)
            self._work_event.clear()
            if self._stop_flag:
                break
            self._process_pending()

    def _process_pending(self):
        """Drain all pending requests in one pass."""
        # Grab current pending under lock
        with self._lock:
            parse_reqs = list(self._pending_parse.values())
            self._pending_parse.clear()
            complete_req = self._pending_complete
            self._pending_complete = None
            def_req = self._pending_definition
            self._pending_definition = None

        for req in parse_reqs:
            self._handle_parse_resolve(req)

        if complete_req:
            self._handle_complete(complete_req)

        if def_req:
            self._handle_definition(def_req)

    def _handle_parse_resolve(self, req: LspRequest):
        from creation_lib.papyrus_lsp import Diagnostic

        try:
            error_tuples, final_ast = _parse_error_recovery(req.text)
        except Exception:
            _log.exception("LspService: parse_resolve failed for %s", req.path)
            return

        diags: list[Diagnostic] = []
        for (line, col, message) in error_tuples:
            diags.append(Diagnostic(
                path=req.path,
                line=max(0, line - 1),
                col=max(0, col - 1),
                end_line=max(0, line - 1),
                end_col=max(0, col + 10),
                message=message,
                severity="error",
            ))

        if final_ast is not None and self._db is not None:
            from creation_lib.papyrus_lsp import resolve
            from creation_lib.papyrus_lsp.native_runtime import DiagnosticSeverity
            self._db.register_ast(final_ast)
            resolver_diags = resolve(final_ast, self._db)
            for d in resolver_diags:
                sev = "error" if d.severity == DiagnosticSeverity.ERROR else "warning"
                diags.append(Diagnostic(
                    path=req.path,
                    line=max(0, d.line - 1),
                    col=max(0, d.col - 1),
                    end_line=max(0, d.end_line - 1),
                    end_col=max(0, d.end_col - 1),
                    message=d.message,
                    severity=sev,
                ))

        with self._lock:
            self._result_diagnostics[req.path] = diags

    def _handle_complete(self, req: LspRequest):
        from creation_lib.papyrus_lsp import get_completions, CompletionResult

        if self._db is None:
            return
        try:
            items = get_completions(req.text, req.line, req.col, self._db)
            with self._lock:
                self._result_complete = CompletionResult(path=req.path, items=items)
        except Exception:
            _log.exception("LspService: complete failed for %s", req.path)

    def _handle_definition(self, req: LspRequest):
        from creation_lib.papyrus_lsp import get_definition

        if self._db is None:
            return
        try:
            result = get_definition(req.text, req.line, req.col, self._db)
            with self._lock:
                self._result_definition = result
        except Exception:
            _log.exception("LspService: definition failed for %s", req.path)
