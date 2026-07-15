"""Standalone query API for the NIF block database.

Provides search, lookup, and filtering over nif_blocks.db without
any dependency on the fo4-data MCP server. Used by the NIF editor
UI and AI terminal.

Usage:
    from editor.nif_block_db import NifBlockDB

    db = NifBlockDB()  # auto-finds db/nif_blocks.db
    blocks = db.get_blocks("fo4/Weapons/Laser/LaserGun.nif")
    shaders = db.get_blocks("fo4/Weapons/Laser/LaserGun.nif",
                            block_type="BSLightingShaderProperty")
    hits = db.search("emissive glow")
    textures = db.get_textures("fo4/Weapons/Laser/LaserGun.nif")
"""

import json
import os
import re
import sqlite3
from pathlib import Path

_SCRIPT_DIR = Path(os.path.dirname(os.path.abspath(__file__)))
_DEFAULT_DB = _SCRIPT_DIR / "db" / "fo4_nif_blocks.db"

# FTS5 special characters to strip from user queries
_FTS5_SPECIAL = re.compile(r'["\(\)\*\:\^\{\}]')


def _fts5_escape(query: str) -> str:
    """Escape a user query for safe FTS5 MATCH."""
    if not query:
        return '""'
    cleaned = _FTS5_SPECIAL.sub(" ", query)
    words = cleaned.split()
    if not words:
        return '""'
    return " ".join(f'"{w}"' for w in words)


class NifBlockDB:
    """Query interface for nif_blocks.db."""

    def __init__(self, db_path: str | Path | None = None):
        self.db_path = Path(db_path) if db_path else _DEFAULT_DB
        self._conn: sqlite3.Connection | None = None

    @property
    def available(self) -> bool:
        """Check if the database file exists."""
        return self.db_path.is_file()

    def _get_conn(self) -> sqlite3.Connection:
        if self._conn is None:
            if not self.available:
                raise RuntimeError(
                    f"NIF blocks database not found: {self.db_path}\n"
                    f"Run: uv run python ui/editor/preprocess_nif_blocks.py"
                )
            self._conn = sqlite3.connect(
                f"file:{self.db_path}?mode=ro", uri=True, check_same_thread=False
            )
            self._conn.row_factory = sqlite3.Row
        return self._conn

    def close(self):
        if self._conn is not None:
            self._conn.close()
            self._conn = None

    # ----------------------------------------------------------
    # Core queries
    # ----------------------------------------------------------

    def get_blocks(
        self,
        nif_id: str,
        block_type: str = "",
        block_index: int = -1,
    ) -> list[dict]:
        """Get deserialized blocks for a NIF.

        Args:
            nif_id: NIF identifier (e.g. "fo4/Weapons/Laser/LaserGun.nif")
            block_type: Filter by block type (e.g. "BSLightingShaderProperty")
            block_index: Get a specific block by index, or -1 for all

        Returns list of block dicts with parsed 'fields' (not raw JSON).
        """
        conn = self._get_conn()
        if block_index >= 0:
            rows = conn.execute(
                "SELECT * FROM nif_blocks WHERE nif_id = ? AND block_index = ?",
                (nif_id, block_index),
            ).fetchall()
        elif block_type:
            rows = conn.execute(
                "SELECT * FROM nif_blocks WHERE nif_id = ? AND type_name = ? ORDER BY block_index",
                (nif_id, block_type),
            ).fetchall()
        else:
            rows = conn.execute(
                "SELECT * FROM nif_blocks WHERE nif_id = ? ORDER BY block_index",
                (nif_id,),
            ).fetchall()
        return [self._parse_block_row(r) for r in rows]

    def search(
        self,
        query: str,
        block_type: str = "",
        nif_id: str = "",
        max_results: int = 20,
    ) -> list[dict]:
        """FTS5 search across all blocks.

        Args:
            query: Search text (e.g. "emissive glow", "BSTriShape")
            block_type: Filter by block type
            nif_id: Filter by NIF ID
            max_results: Maximum results to return

        Returns list of block dicts with parsed 'fields'.
        """
        conn = self._get_conn()
        escaped = _fts5_escape(query)

        sql = """
            SELECT t.* FROM nif_blocks t
            JOIN nif_blocks_fts f ON t.rowid = f.rowid
            WHERE nif_blocks_fts MATCH ?
        """
        params: list = [escaped]

        if block_type:
            sql += " AND t.type_name = ?"
            params.append(block_type)
        if nif_id:
            sql += " AND t.nif_id = ?"
            params.append(nif_id)

        sql += " ORDER BY rank LIMIT ?"
        params.append(max_results)

        rows = conn.execute(sql, params).fetchall()
        results = [self._parse_block_row(r) for r in rows]

        # Fallback: OR individual words if few results
        if len(results) < 3:
            cleaned = _FTS5_SPECIAL.sub(" ", query)
            words = [w for w in cleaned.split() if len(w) >= 3]
            if len(words) > 1:
                or_query = " OR ".join(f'"{w}"' for w in words)
                sql2 = """
                    SELECT t.* FROM nif_blocks t
                    JOIN nif_blocks_fts f ON t.rowid = f.rowid
                    WHERE nif_blocks_fts MATCH ?
                """
                params2: list = [or_query]
                if block_type:
                    sql2 += " AND t.type_name = ?"
                    params2.append(block_type)
                if nif_id:
                    sql2 += " AND t.nif_id = ?"
                    params2.append(nif_id)
                sql2 += " ORDER BY rank LIMIT ?"
                params2.append(max_results)
                extra = conn.execute(sql2, params2).fetchall()
                seen_ids = {r["id"] for r in results}
                for row in extra:
                    d = self._parse_block_row(row)
                    if d["id"] not in seen_ids:
                        results.append(d)
                        seen_ids.add(d["id"])
                results = results[:max_results]

        return results

    def get_textures(self, nif_id: str) -> list[dict]:
        """Get all shader textures for a NIF.

        Returns list of {block_rowid, slot, texture_path} dicts.
        """
        conn = self._get_conn()
        rows = conn.execute(
            "SELECT * FROM nif_shader_textures WHERE nif_id = ? ORDER BY slot",
            (nif_id,),
        ).fetchall()
        return [dict(r) for r in rows]

    def get_refs(self, nif_id: str, parent_index: int = -1) -> list[dict]:
        """Get block reference edges for a NIF.

        Args:
            nif_id: NIF identifier
            parent_index: If >= 0, only get refs from this parent block

        Returns list of {parent_block_idx, child_block_idx, field_name} dicts.
        """
        conn = self._get_conn()
        if parent_index >= 0:
            rows = conn.execute(
                "SELECT * FROM nif_block_refs WHERE nif_id = ? AND parent_block_idx = ?",
                (nif_id, parent_index),
            ).fetchall()
        else:
            rows = conn.execute(
                "SELECT * FROM nif_block_refs WHERE nif_id = ?",
                (nif_id,),
            ).fetchall()
        return [dict(r) for r in rows]

    def get_block_type_counts(self, nif_id: str = "") -> dict[str, int]:
        """Get block type distribution.

        Args:
            nif_id: If set, counts for that NIF only. Otherwise global counts.
        """
        conn = self._get_conn()
        if nif_id:
            rows = conn.execute(
                "SELECT type_name, COUNT(*) as cnt FROM nif_blocks WHERE nif_id = ? GROUP BY type_name ORDER BY cnt DESC",
                (nif_id,),
            ).fetchall()
        else:
            rows = conn.execute(
                "SELECT type_name, COUNT(*) as cnt FROM nif_blocks GROUP BY type_name ORDER BY cnt DESC"
            ).fetchall()
        return {r["type_name"]: r["cnt"] for r in rows}

    def find_blocks_by_type(
        self,
        block_type: str,
        max_results: int = 50,
    ) -> list[dict]:
        """Find all blocks of a given type across all NIFs.

        Returns list of block dicts (fields parsed from JSON).
        """
        conn = self._get_conn()
        rows = conn.execute(
            "SELECT * FROM nif_blocks WHERE type_name = ? LIMIT ?",
            (block_type, max_results),
        ).fetchall()
        return [self._parse_block_row(r) for r in rows]

    def find_by_texture(
        self,
        texture_path: str,
        max_results: int = 20,
    ) -> list[dict]:
        """Find shader texture entries matching a path pattern (LIKE search).

        Returns list of {block_rowid, nif_id, slot, texture_path} dicts.
        """
        conn = self._get_conn()
        rows = conn.execute(
            "SELECT * FROM nif_shader_textures WHERE texture_path LIKE ? LIMIT ?",
            (f"%{texture_path}%", max_results),
        ).fetchall()
        return [dict(r) for r in rows]

    def stats(self) -> dict:
        """Get database statistics."""
        conn = self._get_conn()
        block_count = conn.execute("SELECT COUNT(*) FROM nif_blocks").fetchone()[0]
        nif_count = conn.execute("SELECT COUNT(DISTINCT nif_id) FROM nif_blocks").fetchone()[0]
        tex_count = conn.execute("SELECT COUNT(*) FROM nif_shader_textures").fetchone()[0]
        ref_count = conn.execute("SELECT COUNT(*) FROM nif_block_refs").fetchone()[0]
        db_size = self.db_path.stat().st_size / (1024 * 1024)
        return {
            "blocks": block_count,
            "nifs": nif_count,
            "shader_textures": tex_count,
            "refs": ref_count,
            "db_size_mb": round(db_size, 1),
        }

    # ----------------------------------------------------------
    # Internal helpers
    # ----------------------------------------------------------

    @staticmethod
    def _parse_block_row(row) -> dict:
        """Convert a DB row to a block dict with parsed fields."""
        d = dict(row)
        fj = d.pop("fields_json", "")
        fields = {}
        if fj:
            try:
                fields = json.loads(fj)
            except Exception:
                pass
        d.pop("content", None)
        d["fields"] = fields
        return d
