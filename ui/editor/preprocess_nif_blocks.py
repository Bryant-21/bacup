"""Build nif_blocks.db — full deserialized block data from all indexed NIFs.

Standalone tool for the NIF editor. Reads NIF paths from the existing
nifs.db metadata database (built by fo4_data_mcp/preprocess_nifs.py),
loads each NIF, serializes every block's fields (with enum/bitflag
resolution, field filtering, and array truncation), and writes everything
into its own SQLite database with FTS5 search.

The database lives alongside the editor at ui/editor/db/nif_blocks.db
and can be queried directly via the nif_block_db module.

Phase 1: Query nifs.db for all (nif_id, source_path, path) tuples.
Phase 2: ProcessPoolExecutor for parallel block extraction.
Phase 3: Batch write to SQLite on main thread.

Run:
  cd <your-checkout> && uv run python ui/editor/preprocess_nif_blocks.py
  cd <your-checkout> && uv run python ui/editor/preprocess_nif_blocks.py --limit 100
  cd <your-checkout> && uv run python ui/editor/preprocess_nif_blocks.py --workers 4
"""

import json
import os
import sqlite3
import sys
import time
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

_SCRIPT_DIR = Path(os.path.dirname(os.path.abspath(__file__)))
_PROJECT_ROOT = (_SCRIPT_DIR / ".." / "..").resolve()
if str(_PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(_PROJECT_ROOT))

from app.paths import get_db_dir as _get_db_dir  # noqa: E402

DB_DIR = _SCRIPT_DIR / "db"
BLOCKS_DB = DB_DIR / "fo4_nif_blocks.db"

# nifs.db lives in data/ — we only read from it
NIFS_DB = _get_db_dir() / "fo4_nifs.db"

# Same size limit as preprocess_nifs.py
MAX_NIF_SIZE = 5 * 1024 * 1024  # 5 MB

# Fields to skip — huge per-vertex/per-triangle/binary arrays
SKIP_FIELDS = {
    "Vertex Data", "Triangles", "Triangles Copy",
    "Vertex Map", "Vertex Weights", "Bone Indices",
    "Strip Lengths", "Strips",
    "Pixel Data", "Binary Data",
}

# Block types to skip entirely — only contain bulk mesh/physics data
SKIP_BLOCK_TYPES = {
    "bhkCompressedMeshShapeData",
}


# ============================================================
# Phase 2: Worker function (runs in subprocess)
# ============================================================

def _serialize_block(block, schema):
    """Serialize a block's fields with enum/bitflag resolution and filtering."""
    from creation_lib.nif.schema import build_field_def_map
    from creation_lib.nif.types import to_json

    field_defs = build_field_def_map(schema, block.type_name)

    result = {}
    for name, val in block.fields:
        display_name = name.split(":")[0] if ":" in name else name
        if display_name in SKIP_FIELDS:
            continue
        fdef = field_defs.get(name)
        jv = to_json(val)

        # Resolve enums/bitflags to readable names
        if fdef and fdef.type in schema.enums and isinstance(val, int):
            enum_def = schema.enums[fdef.type]
            jv = next((o.name for o in enum_def.options if o.value == val), str(val))
        elif fdef and fdef.type in schema.bitflags and isinstance(val, int):
            bf_def = schema.bitflags[fdef.type]
            jv = [o.name for o in bf_def.options if val & (1 << o.value)]

        # Skip raw bytes
        if isinstance(jv, bytes):
            continue

        # Truncate huge struct arrays
        if isinstance(jv, list) and len(jv) > 100 and jv and isinstance(jv[0], (dict, list)):
            jv = jv[:20] + [f"...truncated from {len(jv)}"]

        result[display_name] = jv
    return result


def _build_block_fts(block, fields_json):
    """Build FTS-searchable text content for a block."""
    parts = [block.type_name]
    name = block.get_field("Name") or ""
    if name:
        parts.append(name)
    # Include field names and string values for search
    for key, val in fields_json.items():
        if key != "Name":
            parts.append(key)
        if isinstance(val, str) and val and key != "Name":
            parts.append(val)
        elif isinstance(val, list) and val and isinstance(val[0], str):
            parts.extend(v for v in val[:10] if isinstance(v, str))
    return " ".join(parts)


def _extract_blocks_worker(abs_path):
    """Process-pool worker: load one NIF and extract all block data.

    Returns (blocks_list, refs_list, textures_list) or None on error.
    """
    if str(_PROJECT_ROOT) not in sys.path:
        sys.path.insert(0, str(_PROJECT_ROOT))
    from creation_lib.nif.nif_file import NifFile
    from creation_lib.nif.schema import get_schema

    t0 = time.time()
    try:
        if os.path.getsize(abs_path) > MAX_NIF_SIZE:
            return None, time.time() - t0
        nif = NifFile.load(abs_path)
    except Exception:
        return None, time.time() - t0

    schema = nif.schema or get_schema()

    # Build parent map: child_block_id -> (parent_block_id, field_name)
    parent_map = {}
    all_refs = []  # (parent_idx, child_idx, field_name)
    for block in nif.blocks:
        if block.type_name in SKIP_BLOCK_TYPES:
            continue
        for field_name, ref_ids in block.get_all_ref_fields(schema):
            for ref_id in ref_ids:
                parent_map[ref_id] = (block.block_id, field_name)
                all_refs.append((block.block_id, ref_id, field_name))

    blocks = []
    textures = []
    for block in nif.blocks:
        if block.type_name in SKIP_BLOCK_TYPES:
            # Still record the block with empty fields
            parent_bid, parent_field = parent_map.get(block.block_id, (-1, ""))
            blocks.append({
                "block_index": block.block_id,
                "type_name": block.type_name,
                "name": "",
                "parent_index": parent_bid,
                "parent_field": parent_field,
                "fields_json": {},
                "content": block.type_name,
            })
            continue

        fields_json = _serialize_block(block, schema)
        fts_content = _build_block_fts(block, fields_json)
        parent_bid, parent_field = parent_map.get(block.block_id, (-1, ""))
        blocks.append({
            "block_index": block.block_id,
            "type_name": block.type_name,
            "name": block.get_field("Name") or "",
            "parent_index": parent_bid,
            "parent_field": parent_field,
            "fields_json": fields_json,
            "content": fts_content,
        })

        # Extract shader textures for denormalized table
        if block.type_name == "BSShaderTextureSet":
            tex_list = block.get_field("Textures")
            if isinstance(tex_list, list):
                for slot, tex in enumerate(tex_list):
                    if isinstance(tex, str) and tex.strip():
                        textures.append((block.block_id, slot, tex.strip()))

    return (blocks, all_refs, textures), time.time() - t0


# ============================================================
# Phase 3: DB schema + batch writes
# ============================================================

def create_db():
    """Create fresh nif_blocks.db with all tables and indexes."""
    os.makedirs(str(DB_DIR), exist_ok=True)
    if BLOCKS_DB.exists():
        os.remove(str(BLOCKS_DB))
    conn = sqlite3.connect(str(BLOCKS_DB))

    conn.execute("""
        CREATE TABLE nif_blocks (
            id            INTEGER PRIMARY KEY,
            nif_id        TEXT NOT NULL,
            block_index   INTEGER NOT NULL,
            type_name     TEXT NOT NULL,
            name          TEXT DEFAULT '',
            parent_index  INTEGER DEFAULT -1,
            parent_field  TEXT DEFAULT '',
            fields_json   TEXT NOT NULL,
            content       TEXT DEFAULT ''
        )
    """)
    conn.execute("CREATE INDEX idx_blocks_nif ON nif_blocks(nif_id)")
    conn.execute("CREATE INDEX idx_blocks_type ON nif_blocks(type_name)")
    conn.execute("CREATE INDEX idx_blocks_nif_idx ON nif_blocks(nif_id, block_index)")

    conn.execute("""
        CREATE VIRTUAL TABLE nif_blocks_fts USING fts5(
            type_name, name, content,
            content=nif_blocks, content_rowid=id
        )
    """)

    conn.execute("""
        CREATE TABLE nif_shader_textures (
            block_rowid   INTEGER NOT NULL,
            nif_id        TEXT NOT NULL,
            slot          INTEGER NOT NULL,
            texture_path  TEXT NOT NULL
        )
    """)
    conn.execute("CREATE INDEX idx_stex_path ON nif_shader_textures(texture_path)")
    conn.execute("CREATE INDEX idx_stex_nif ON nif_shader_textures(nif_id)")

    conn.execute("""
        CREATE TABLE nif_block_refs (
            nif_id            TEXT NOT NULL,
            parent_block_idx  INTEGER NOT NULL,
            child_block_idx   INTEGER NOT NULL,
            field_name        TEXT NOT NULL
        )
    """)
    conn.execute("CREATE INDEX idx_refs_nif ON nif_block_refs(nif_id)")
    conn.execute("CREATE INDEX idx_refs_parent ON nif_block_refs(nif_id, parent_block_idx)")

    conn.commit()
    return conn


# ============================================================
# Main build pipeline
# ============================================================

def build_db(limit=None, num_workers=None):
    """Build nif_blocks.db from all NIFs in nifs.db."""
    if num_workers is None:
        num_workers = max(1, (os.cpu_count() or 4) // 2)

    if not NIFS_DB.exists():
        print(f"ERROR: nifs.db not found at {NIFS_DB}")
        print("Run preprocess_nifs.py first to build the NIF metadata database.")
        sys.exit(1)

    # Phase 1: Collect NIF paths from existing nifs.db
    print("Phase 1: Collecting NIF paths from nifs.db...", flush=True)
    meta_conn = sqlite3.connect(str(NIFS_DB))
    rows = meta_conn.execute("SELECT id, source_path, path FROM nifs").fetchall()
    meta_conn.close()

    nif_entries = []
    for nif_id, source_path, rel_path in rows:
        abs_path = os.path.join(source_path, rel_path)
        if os.path.isfile(abs_path):
            nif_entries.append((nif_id, abs_path))

    if limit:
        nif_entries = nif_entries[:limit]

    total_count = len(nif_entries)
    print(f"  Found {total_count:,} NIFs to process", flush=True)

    # Create fresh DB
    conn = create_db()

    # Phase 2+3: Parallel extract -> DB write
    print(f"\nPhase 2+3: Extracting blocks with {num_workers} workers...", flush=True)
    abs_paths = [ap for _, ap in nif_entries]
    nif_ids = [nid for nid, _ in nif_entries]

    total_blocks = 0
    total_refs = 0
    total_textures = 0
    total_indexed = 0
    total_errors = 0
    t0 = time.time()

    chunksize = max(1, min(50, total_count // (num_workers * 4)))

    with ProcessPoolExecutor(max_workers=num_workers) as pool:
        results = pool.map(_extract_blocks_worker, abs_paths, chunksize=chunksize)

        for i, (data, elapsed) in enumerate(results):
            nif_id = nif_ids[i]
            done = i + 1

            if data is None:
                total_errors += 1
            else:
                blocks, refs, textures = data
                total_indexed += 1

                # Insert blocks
                for b in blocks:
                    cursor = conn.execute(
                        """INSERT INTO nif_blocks
                           (nif_id, block_index, type_name, name,
                            parent_index, parent_field, fields_json, content)
                           VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
                        (nif_id, b["block_index"], b["type_name"], b["name"],
                         b["parent_index"], b["parent_field"],
                         json.dumps(b["fields_json"], separators=(",", ":")),
                         b["content"]),
                    )
                    block_rowid = cursor.lastrowid

                    # Insert shader textures with the block's rowid
                    for tex_block_idx, slot, tex_path in textures:
                        if tex_block_idx == b["block_index"]:
                            conn.execute(
                                """INSERT INTO nif_shader_textures
                                   (block_rowid, nif_id, slot, texture_path)
                                   VALUES (?, ?, ?, ?)""",
                                (block_rowid, nif_id, slot, tex_path),
                            )

                total_blocks += len(blocks)

                # Insert refs
                for parent_idx, child_idx, field_name in refs:
                    conn.execute(
                        """INSERT INTO nif_block_refs
                           (nif_id, parent_block_idx, child_block_idx, field_name)
                           VALUES (?, ?, ?, ?)""",
                        (nif_id, parent_idx, child_idx, field_name),
                    )
                total_refs += len(refs)
                total_textures += len(textures)

            if done % 500 == 0 or done == total_count:
                wall = time.time() - t0
                rate = done / wall if wall > 0 else 0
                eta = (total_count - done) / rate if rate > 0 else 0
                pct = done * 100 // total_count
                conn.commit()
                print(f"  {done:,}/{total_count:,} ({pct}%) "
                      f"| {total_indexed:,} ok, {total_errors:,} err "
                      f"| {total_blocks:,} blocks "
                      f"| {rate:.0f}/s, ETA {eta:.0f}s",
                      flush=True)

    conn.commit()

    # Rebuild FTS index
    print(f"\n  Rebuilding FTS index...", flush=True)
    conn.execute("INSERT INTO nif_blocks_fts(nif_blocks_fts) VALUES('rebuild')")
    conn.commit()

    elapsed = time.time() - t0
    print(f"\nDone in {elapsed:.1f}s!", flush=True)
    print(f"  NIFs processed: {total_indexed:,} ({total_errors:,} errors)", flush=True)
    print(f"  Blocks: {total_blocks:,}", flush=True)
    print(f"  Refs: {total_refs:,}", flush=True)
    print(f"  Shader textures: {total_textures:,}", flush=True)

    # Report DB size
    conn.close()
    db_size = BLOCKS_DB.stat().st_size / (1024 * 1024)
    print(f"  Database size: {db_size:.1f} MB", flush=True)

    return total_indexed, total_errors, total_blocks


def main():
    limit = None
    num_workers = max(1, (os.cpu_count() or 4) // 2)
    for i, arg in enumerate(sys.argv[1:], 1):
        if arg == "--limit" and i < len(sys.argv) - 1:
            limit = int(sys.argv[i + 1])
        if arg == "--workers" and i < len(sys.argv) - 1:
            num_workers = int(sys.argv[i + 1])

    print(f"Building NIF blocks database")
    print(f"  Source: {NIFS_DB}")
    print(f"  Target: {BLOCKS_DB}")
    print(f"  Workers: {num_workers} (of {os.cpu_count()} CPUs)")
    if limit:
        print(f"  Limit: {limit} NIFs")
    print(flush=True)

    build_db(limit=limit, num_workers=num_workers)


if __name__ == "__main__":
    main()
