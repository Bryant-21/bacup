"""Lightweight read-only SQLite wrapper for UI access to behaviors.db.

The imgui UI cannot call MCP tools (those use stdio for Claude).
This module provides direct SQLite access to the same database,
keeping the UI decoupled from MCP while sharing the same data.
"""

import os
import sqlite3
from pathlib import Path

from app.paths import get_db_dir as _get_db_dir

_DEFAULT_CACHE_DIR = str(_get_db_dir())
_DEFAULT_DB_PATH = str(_get_db_dir() / "fo4_havok.db")


def get_available_game_dbs() -> dict[str, "BehaviorDB"]:
    """Return a BehaviorDB for each game that has a *_havok.db file on disk."""
    data_dir = _DEFAULT_CACHE_DIR
    dbs: dict[str, BehaviorDB] = {}
    for entry in sorted(os.listdir(data_dir)):
        if entry.endswith("_havok.db"):
            game = entry[: -len("_havok.db")]
            db = BehaviorDB(db_path=os.path.join(data_dir, entry))
            if db.available:
                dbs[game] = db
    return dbs


class BehaviorDB:
    """Read-only access to the behaviors SQLite database."""

    def __init__(self, db_path: str | None = None, cache_dir: str | None = None):
        self._db_path = os.path.normpath(db_path or _DEFAULT_DB_PATH)
        self._cache_dir = os.path.normpath(cache_dir or _DEFAULT_CACHE_DIR)
        self._conn: sqlite3.Connection | None = None

    @property
    def available(self) -> bool:
        """Check if the database file exists."""
        return os.path.isfile(self._db_path)

    def _get_conn(self) -> sqlite3.Connection | None:
        """Get or create a read-only connection. Returns None if DB missing."""
        if self._conn is not None:
            return self._conn
        if not self.available:
            return None
        self._conn = sqlite3.connect(
            f"file:{self._db_path}?mode=ro", uri=True, check_same_thread=False
        )
        self._conn.row_factory = sqlite3.Row
        return self._conn

    def close(self):
        if self._conn:
            self._conn.close()
            self._conn = None

    # --- Search ---

    def search_behaviors(
        self,
        query: str = "",
        category: str | None = None,
        source: str | None = None,
        limit: int = 50,
    ) -> list[dict]:
        """FTS search on behaviors. Returns list of dicts."""
        conn = self._get_conn()
        if conn is None:
            return []

        if query.strip():
            terms = [w for w in query.split() if w]
            escaped = " ".join(f'"{w}"*' for w in terms)

            like_clauses = []
            like_params: list = []
            for w in terms:
                like_clauses.append("(t.id LIKE ? OR t.name LIKE ? OR t.content LIKE ?)")
                pat = f"%{w}%"
                like_params.extend([pat, pat, pat])

            # havok_fts is content-less — query it for matching IDs, then
            # fetch full rows from havok_behaviors via IN clause.
            sql = f"""
                SELECT t.*, 1 as _src FROM havok_behaviors t
                WHERE t.id IN (
                    SELECT id FROM havok_fts
                    WHERE havok_fts MATCH ? AND entity_type = 'behavior'
                )
                UNION
                SELECT t.*, 2 as _src FROM havok_behaviors t
                WHERE {' AND '.join(like_clauses)}
            """
            params: list = [escaped] + like_params
        else:
            sql = "SELECT *, 0 as _src FROM havok_behaviors WHERE 1=1"
            params = []

        if category or source:
            filter_clauses = []
            filter_params = []
            if category:
                filter_clauses.append("category = ?")
                filter_params.append(category)
            if source:
                filter_clauses.append("source = ?")
                filter_params.append(source)
            where = " AND ".join(filter_clauses)
            sql = f"SELECT * FROM ({sql}) WHERE {where}"
            params += filter_params

        if query.strip():
            sql = f"SELECT * FROM ({sql}) GROUP BY id ORDER BY MIN(_src), name"
        else:
            sql = f"SELECT * FROM ({sql}) ORDER BY name"
        sql += " LIMIT ?"
        params.append(limit)

        rows = conn.execute(sql, params).fetchall()
        return [dict(row) for row in rows]

    # --- Metadata ---

    def get_behavior_metadata(self, behavior_id: str) -> dict | None:
        """Get full metadata + events + variables + sequences + transitions for a behavior."""
        conn = self._get_conn()
        if conn is None:
            return None

        row = conn.execute(
            "SELECT * FROM havok_behaviors WHERE id = ?", (behavior_id,)
        ).fetchone()
        if row is None:
            return None

        result = dict(row)
        result["events"] = [
            r["event_name"]
            for r in conn.execute(
                "SELECT event_name FROM behavior_events WHERE behavior_id = ? ORDER BY event_name",
                (behavior_id,),
            ).fetchall()
        ]
        result["variables"] = [
            {"name": r["variable_name"], "type": r["variable_type"]}
            for r in conn.execute(
                "SELECT variable_name, variable_type FROM behavior_variables WHERE behavior_id = ? ORDER BY variable_name",
                (behavior_id,),
            ).fetchall()
        ]
        result["sequences"] = [
            r["sequence_name"]
            for r in conn.execute(
                "SELECT sequence_name FROM behavior_sequences WHERE behavior_id = ? ORDER BY sequence_name",
                (behavior_id,),
            ).fetchall()
        ]
        result["transitions"] = [
            {"name": r["transition_name"], "duration": r["duration"]}
            for r in conn.execute(
                "SELECT transition_name, duration FROM behavior_transitions WHERE behavior_id = ? ORDER BY transition_name",
                (behavior_id,),
            ).fetchall()
        ]
        return result

    # --- XML path resolution ---

    def get_behavior_xml_path(self, behavior_id: str) -> str | None:
        """Resolve the filesystem path to the cached XML file for a behavior."""
        conn = self._get_conn()
        if conn is None:
            return None

        row = conn.execute(
            "SELECT source, source_path FROM havok_behaviors WHERE id = ?", (behavior_id,)
        ).fetchone()
        if row is None:
            return None

        source = row["source"]
        source_path = row["source_path"]

        # Replace .hkx extension with .xml — cached files are unpacked XML
        xml_path = os.path.splitext(source_path)[0] + ".xml"

        if source.startswith("ext:"):
            mod_name = source[4:]
            cache_subdir = os.path.join("xml_external_cache", mod_name)
        else:
            # Standard game id (fo4, fo76, skyrimse, starfield, …)
            cache_subdir = f"xml_{source}_cache"

        full_path = os.path.join(self._cache_dir, cache_subdir, xml_path)
        if os.path.isfile(full_path):
            return full_path
        return None

    # --- Aggregate queries for autocomplete ---

    def get_all_event_names(self) -> list[str]:
        conn = self._get_conn()
        if conn is None:
            return []
        rows = conn.execute(
            "SELECT DISTINCT event_name FROM behavior_events ORDER BY event_name"
        ).fetchall()
        return [r[0] for r in rows]

    def get_all_variable_names(self) -> list[str]:
        conn = self._get_conn()
        if conn is None:
            return []
        rows = conn.execute(
            "SELECT DISTINCT variable_name FROM behavior_variables ORDER BY variable_name"
        ).fetchall()
        return [r[0] for r in rows]

    def get_all_sequence_names(self) -> list[str]:
        conn = self._get_conn()
        if conn is None:
            return []
        rows = conn.execute(
            "SELECT DISTINCT sequence_name FROM behavior_sequences ORDER BY sequence_name"
        ).fetchall()
        return [r[0] for r in rows]

    # --- Frequency-ranked queries for import dialogs ---

    def get_events_with_counts(self, category: str | None = None) -> list[tuple[str, int]]:
        conn = self._get_conn()
        if conn is None:
            return []
        if category:
            rows = conn.execute(
                """SELECT be.event_name, COUNT(*) as cnt
                   FROM behavior_events be
                   JOIN havok_behaviors b ON be.behavior_id = b.id
                   WHERE b.category = ?
                   GROUP BY be.event_name ORDER BY cnt DESC""",
                (category,),
            ).fetchall()
        else:
            rows = conn.execute(
                """SELECT event_name, COUNT(*) as cnt
                   FROM behavior_events
                   GROUP BY event_name ORDER BY cnt DESC"""
            ).fetchall()
        return [(r[0], r[1]) for r in rows]

    def get_variables_with_counts(
        self, category: str | None = None
    ) -> list[tuple[str, str, int]]:
        conn = self._get_conn()
        if conn is None:
            return []
        if category:
            rows = conn.execute(
                """SELECT bv.variable_name, bv.variable_type, COUNT(*) as cnt
                   FROM behavior_variables bv
                   JOIN havok_behaviors b ON bv.behavior_id = b.id
                   WHERE b.category = ?
                   GROUP BY bv.variable_name ORDER BY cnt DESC""",
                (category,),
            ).fetchall()
        else:
            rows = conn.execute(
                """SELECT variable_name, variable_type, COUNT(*) as cnt
                   FROM behavior_variables
                   GROUP BY variable_name ORDER BY cnt DESC"""
            ).fetchall()
        return [(r[0], r[1], r[2]) for r in rows]

    def get_categories(self) -> list[str]:
        conn = self._get_conn()
        if conn is None:
            return []
        rows = conn.execute(
            "SELECT DISTINCT category FROM havok_behaviors ORDER BY category"
        ).fetchall()
        return [r[0] for r in rows]

    def get_sources(self) -> list[str]:
        conn = self._get_conn()
        if conn is None:
            return []
        rows = conn.execute(
            "SELECT DISTINCT source FROM havok_behaviors ORDER BY source"
        ).fetchall()
        return [r[0] for r in rows]
