"""Read-only source catalogs with small, revisioned metadata snapshots.

Source databases and their geometry stay out of the UI thread. A snapshot copies
only searchable metadata, indexes it once, and pages it without loading BLOBs.
"""
from __future__ import annotations

from dataclasses import dataclass
from contextlib import closing, contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import tempfile
from threading import Event

PAGE_SIZE = 128
SNAPSHOT_VERSION = 1


class Cancelled(Exception):
    pass


def source_stamp(path: Path) -> str:
    pieces = []
    for item in (path, Path(str(path) + "-wal")):
        try:
            st = item.stat()
            # Read-only WAL opens can create an empty journal; it carries no data.
            pieces.append(None if item != path and st.st_size == 0 else (st.st_size, st.st_mtime_ns, st.st_ino))
        except FileNotFoundError:
            pieces.append(None)
    return hashlib.sha256(repr(pieces).encode()).hexdigest()[:24]


@contextmanager
def readonly(path: Path, cancel: Event | None = None):
    conn = sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True, timeout=2)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only=ON")
    if cancel:
        conn.set_progress_handler(lambda: int(cancel.is_set()), 1000)
    try:
        yield conn
    finally:
        conn.close()


@dataclass(frozen=True)
class Asset:
    key: str
    source_path: str
    source_stamp: str
    source_id: int
    name: str
    source_swf: str
    source_tag: int
    tags: str
    user_added: bool
    personal: bool
    geometry_bytes: int


@dataclass(frozen=True)
class Filters:
    query: str = ""
    category: str = ""
    origin: str = "all"
    source: str = ""


@dataclass
class Page:
    offset: int
    total: int
    assets: list[Asset]
    categories: list[str]
    sources: list[str]
    revision: str


class Catalog:
    def __init__(self, base: Path | None, personal: Path, cache_dir: Path, game: str):
        self.base = base.resolve() if base else None
        self.personal = personal.resolve()
        self.cache_dir = cache_dir.resolve()
        self.game = game
        self._ready = {}

    def sources(self):
        return [(p, personal) for p, personal in ((self.base, False), (self.personal, True)) if p and p.is_file()]

    def revision(self):
        data = [(str(p), source_stamp(p), personal) for p, personal in self.sources()]
        return hashlib.sha256(repr((SNAPSHOT_VERSION, self.game, data)).encode()).hexdigest()

    def snapshot(self, cancel: Event | None = None):
        cancel = cancel or Event()
        revision = self.revision()
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        destination = self.cache_dir / f"catalog-{revision}.sqlite"
        if destination.is_file() and self._ready.get(destination) == source_stamp(destination):
            return destination, revision
        if destination.is_file():
            try:
                with readonly(destination) as check:
                    if check.execute("SELECT value FROM header WHERE name='version'").fetchone()[0] == str(SNAPSHOT_VERSION):
                        self._ready[destination] = source_stamp(destination)
                        return destination, revision
            except (sqlite3.Error, TypeError, IndexError):
                destination.unlink(missing_ok=True)
        handle, temporary = tempfile.mkstemp(prefix="catalog-build-", suffix=".sqlite", dir=self.cache_dir)
        os.close(handle)
        temp = Path(temporary)
        try:
            with closing(sqlite3.connect(temp)) as output, output:
                output.executescript("""
                    CREATE TABLE header(name TEXT PRIMARY KEY,value TEXT);
                    CREATE TABLE assets(seq INTEGER PRIMARY KEY,key TEXT UNIQUE,source_path TEXT,source_stamp TEXT,
                       source_id INTEGER,name TEXT,source_swf TEXT,source_tag INTEGER,tags TEXT,user_added INTEGER,
                       personal INTEGER,geometry_bytes INTEGER);
                    CREATE VIRTUAL TABLE assets_fts USING fts5(name,tags,source_swf,content='assets',content_rowid='seq');
                    CREATE INDEX assets_order ON assets(source_swf,source_tag,key);
                    CREATE INDEX assets_origin ON assets(user_added);
                    CREATE INDEX assets_tags ON assets(tags);
                """)
                output.set_progress_handler(lambda: int(cancel.is_set()), 1000)
                for path, personal in self.sources():
                    stamp = source_stamp(path)
                    namespace = hashlib.sha256(str(path).casefold().encode()).hexdigest()[:20]
                    with readonly(path, cancel) as source:
                        columns = {row[1] for row in source.execute("PRAGMA table_info(shapes)")}
                        if not {"id", "name", "source_swf", "source_tag", "tags", "shape_data"}.issubset(columns):
                            raise ValueError(f"{path.name} is not a supported indexed vector library")
                        user = "coalesce(user_added,0)" if "user_added" in columns else "0"
                        where, parameters = (" WHERE game=? AND deleted=0", (self.game,)) if personal else ("", ())
                        sql = ("SELECT id,coalesce(name,''),coalesce(source_swf,''),coalesce(source_tag,0),"
                               f"coalesce(tags,''),{user},length(shape_data) FROM shapes" + where)
                        cursor = source.execute(sql, parameters)
                        while batch := cursor.fetchmany(512):
                            if cancel.is_set():
                                raise Cancelled()
                            output.executemany("INSERT INTO assets(key,source_path,source_stamp,source_id,name,source_swf,source_tag,tags,user_added,personal,geometry_bytes) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                                ((f"{namespace}:{stamp}:{row[0]}", str(path), stamp, row[0], row[1], row[2], row[3], row[4],
                                  int(personal or row[5]), int(personal), row[6] or 0) for row in batch))
                if cancel.is_set():
                    raise Cancelled()
                output.execute("INSERT INTO assets_fts(assets_fts) VALUES ('rebuild')")
                output.execute("INSERT INTO header VALUES ('version',?)", (str(SNAPSHOT_VERSION),))
            if revision != self.revision():
                raise Cancelled("Library changed during metadata load")
            os.replace(temp, destination)
            self._ready[destination] = source_stamp(destination)
            # Only our fingerprinted sidecars are removed, never source catalogs.
            older = sorted(self.cache_dir.glob("catalog-*.sqlite"), key=lambda p: p.stat().st_mtime, reverse=True)
            for item in older[4:]:
                if item != destination and item.parent == self.cache_dir:
                    try:
                        item.unlink(missing_ok=True)
                    except OSError:
                        pass  # Another toolkit instance may still be reading it.
            return destination, revision
        except sqlite3.OperationalError as exc:
            if cancel.is_set():
                raise Cancelled() from exc
            raise
        finally:
            temp.unlink(missing_ok=True)

    def query(self, filters: Filters, offset: int = 0, limit: int = PAGE_SIZE, cancel: Event | None = None):
        path, revision = self.snapshot(cancel)
        clauses, parameters = [], []
        if filters.query.strip():
            tokens = re.findall(r"\w+", filters.query, re.UNICODE)
            if tokens:
                clauses.append("seq IN (SELECT rowid FROM assets_fts WHERE assets_fts MATCH ?)")
                parameters.append(" AND ".join('"' + word.replace('"', '""') + '"*' for word in tokens))
            else:
                clauses.append("(name LIKE ? ESCAPE '\\' OR source_swf LIKE ? ESCAPE '\\' OR tags LIKE ? ESCAPE '\\')")
                escaped = filters.query.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
                parameters.extend([f"%{escaped}%"] * 3)
        if filters.category:
            clauses.append("(tags=? OR substr(tags,1,length(?)+1)=?||'/')")
            parameters.extend([filters.category] * 3)
        if filters.origin in {"base", "user"}:
            clauses.append("user_added=?")
            parameters.append(int(filters.origin == "user"))
        if filters.source:
            clauses.append("source_swf=?")
            parameters.append(filters.source)
        where = " WHERE " + " AND ".join(clauses) if clauses else ""
        try:
            with readonly(path, cancel) as conn:
                total = conn.execute("SELECT count(*) FROM assets" + where, parameters).fetchone()[0]
                rows = conn.execute("SELECT * FROM assets" + where + " ORDER BY source_swf,source_tag,key LIMIT ? OFFSET ?",
                                    [*parameters, min(max(limit, 1), PAGE_SIZE), max(offset, 0)]).fetchall()
                if offset == 0:
                    tags = [row[0] for row in conn.execute("SELECT DISTINCT tags FROM assets WHERE tags != '' ORDER BY tags")]
                    categories = sorted(set(tags + [tag.split('/')[0] for tag in tags]))
                    sources = [row[0] for row in conn.execute("SELECT DISTINCT source_swf FROM assets ORDER BY source_swf")]
                else:
                    categories, sources = [], []
            assets = [Asset(row["key"], row["source_path"], row["source_stamp"], row["source_id"], row["name"],
                            row["source_swf"], row["source_tag"], row["tags"], bool(row["user_added"]), bool(row["personal"]),
                            row["geometry_bytes"]) for row in rows]
            return Page(offset, total, assets, categories, sources, revision)
        except sqlite3.OperationalError as exc:
            if cancel and cancel.is_set():
                raise Cancelled() from exc
            raise


def payload(asset: Asset):
    from creation_lib.swf.library_assets import MAX_ASSET_BYTES
    if asset.geometry_bytes > MAX_ASSET_BYTES:
        raise ValueError('Library geometry exceeds the 16 MB asset limit')
    path = Path(asset.source_path)
    if source_stamp(path) != asset.source_stamp:
        raise ValueError("Library source changed; refresh before using this asset")
    with readonly(path) as conn:
        columns = {row[1] for row in conn.execute("PRAGMA table_info(shapes)")}
        svg = "svg_preview" if "svg_preview" in columns else "''"
        row = conn.execute(f"SELECT shape_data,{svg} FROM shapes WHERE id=?", (asset.source_id,)).fetchone()
    if row is None or source_stamp(path) != asset.source_stamp:
        raise ValueError("Library asset changed or was removed; refresh before using it")
    return bytes(row[0] or b""), row[1] or ""


def add_personal(path: Path, game: str, entries: list[tuple[str, bytes, str, str]], cancelled: Event | None = None):
    """Store copied, validated geometry; never alter the imported file or base index."""
    from creation_lib.preprocessor.swf import CREATE_TABLES_DDL
    from creation_lib.swf.library_assets import decode_geometry
    checked = []
    for name, blob, source, tags in entries:
        if cancelled and cancelled.is_set():
            raise Cancelled()
        shape, _ = decode_geometry(blob)
        checked.append((name, source, shape.shape_id, json.dumps(shape.bounds_px), '[]', 1, blob, '', tags,
                        1, game, hashlib.sha256(blob).hexdigest()))
    path.parent.mkdir(parents=True, exist_ok=True)
    with closing(sqlite3.connect(path)) as conn, conn:
        conn.executescript(CREATE_TABLES_DDL)
        columns = {row[1] for row in conn.execute("PRAGMA table_info(shapes)")}
        for name, declaration in (("game", "TEXT"), ("digest", "TEXT"), ("deleted", "INTEGER DEFAULT 0")):
            if name not in columns:
                conn.execute(f"ALTER TABLE shapes ADD COLUMN {name} {declaration}")
        conn.executemany("INSERT INTO shapes(name,source_swf,source_tag,bounds,fill_colors,path_count,shape_data,svg_preview,tags,user_added,game,digest) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)", checked)
        conn.execute("INSERT INTO shapes_fts(shapes_fts) VALUES ('rebuild')")
        if cancelled and cancelled.is_set():
            raise Cancelled()
    return len(checked)
