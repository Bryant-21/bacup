"""
SearchApp — unified game data search powered by creation-data databases.

Searches game records, Papyrus wiki, CK wiki, Papyrus scripts, behaviors,
NIFs, and external mods. Supports keyword, semantic, FormKey, EditorID,
script name, reference, keyword-tag, function lookup, and browse modes.

Can be embedded as a toolkit workspace (draw_search_panel / draw_content_panel)
or run standalone (gui()).
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

# Ensure project root is on sys.path when run standalone
_HERE = os.path.dirname(os.path.abspath(__file__))

from imgui_bundle import imgui, imgui_md

from creation_lib.core.game_profiles import GAME_PROFILES
from creation_lib.ui.theme.window_chrome import AsyncWorker
from ui.core.imgui_widgets import (
    SOURCE_COLORS,
    draw_highlighted_text,
    draw_status_bar,
    syntax_highlight_papyrus,
    syntax_highlight_yaml,
)

# ---------------------------------------------------------------------------
# Service layer
# ---------------------------------------------------------------------------
import creation_lib.creation_data as cd
from creation_lib.creation_data._db_resolver import db_available as _cd_db_available


def _resolve_db_dir() -> str:
    """Resolve database directory."""
    from app.paths import get_db_dir
    return str(get_db_dir())


_DB_DIR = _resolve_db_dir()

# Active game (module-level, updated by SearchApp.set_game)
_ACTIVE_GAME: str = "fo4"


# ---------------------------------------------------------------------------
# Result adapters — transform service layer dicts to UI format
# ---------------------------------------------------------------------------

def _adapt_record(h: dict) -> dict:
    return {
        "title": h.get("editor_id", ""),
        "subtitle": h.get("name", "") or "",
        "record_type": h.get("record_type", ""),
        "source_label": "Game Records",
        "source_key": "records",
        "score": h.get("score", 1.0),
        "form_key": h.get("form_key", ""),
        "keywords": h.get("keywords", "") or "",
        "source": h.get("source", ""),
    }


def _adapt_script(h: dict) -> dict:
    return {
        "title": h.get("script_name", ""),
        "subtitle": f"extends {h['extends']}" if h.get("extends") else "",
        "record_type": h.get("category", ""),
        "source_label": "Papyrus Scripts",
        "source_key": "scripts",
        "score": h.get("score", 1.0),
        "script_id": h.get("script_id", ""),
        "source": h.get("source", ""),
        "extends": h.get("extends", ""),
        "category": h.get("category", ""),
    }


def _adapt_wiki(h: dict) -> dict:
    return {
        "title": h.get("title", "") or h.get("filename", ""),
        "subtitle": f"({h['category']})" if h.get("category") else "",
        "record_type": h.get("category", ""),
        "source_label": "Wiki",
        "source_key": "wiki",
        "score": h.get("score", 1.0),
        "filename": h.get("filename", ""),
        "category": h.get("category", ""),
    }


def _adapt_havok(h: dict) -> dict:
    entity_type = h.get("entity_type", "")
    category = h.get("category", "") or ""
    subtitle = entity_type.title() if entity_type else ""
    if category:
        subtitle = f"{subtitle} - {category}" if subtitle else category
    return {
        "title": h.get("name", ""),
        "subtitle": subtitle,
        "record_type": category or (entity_type.title() if entity_type else ""),
        "source_label": "Havok",
        "source_key": "havok",
        "score": h.get("score", 1.0),
        "entity_id": h.get("id", ""),
        "entity_type": entity_type,
        "category": category,
    }


def _adapt_nif(h: dict) -> dict:
    return {
        "title": h.get("name", ""),
        "subtitle": h.get("path", ""),
        "record_type": h.get("category", ""),
        "source_label": "NIFs",
        "source_key": "nifs",
        "score": h.get("score", 1.0),
        "nif_id": h.get("id", ""),
        "category": h.get("category", ""),
    }


def _adapt_ext_record(h: dict) -> dict:
    return {
        "title": h.get("editor_id", ""),
        "subtitle": h.get("name", "") or "",
        "record_type": h.get("record_type", ""),
        "source_label": f"Ext: {h.get('source', '').replace('ext:', '')}",
        "source_key": "ext_records",
        "score": h.get("score", 1.0),
        "form_key": h.get("form_key", ""),
    }


def _adapt_ext_script(h: dict) -> dict:
    return {
        "title": h.get("script_name", ""),
        "subtitle": f"extends {h['extends']}" if h.get("extends") else "",
        "record_type": h.get("category", ""),
        "source_label": f"Ext: {h.get('source', '').replace('ext:', '')}",
        "source_key": "ext_scripts",
        "score": h.get("score", 1.0),
        "script_id": h.get("script_id", ""),
    }


_DOMAIN_ADAPTER = {
    "records": _adapt_record,
    "scripts": _adapt_script,
    "wiki": _adapt_wiki,
    "behaviors": _adapt_havok,
    "havok": _adapt_havok,
    "nifs": _adapt_nif,
    "ext_records": _adapt_ext_record,
    "ext_scripts": _adapt_ext_script,
}


def _search_domain(domain: str, query: str, game: str, db_dir: str,
                   max_results: int = 50, semantic: bool = False,
                   **kwargs) -> list[dict]:
    """Search a domain via the service layer and adapt results to UI format."""
    adapter = _DOMAIN_ADAPTER.get(domain, lambda h: h)
    try:
        if semantic:
            hits = cd.semantic_search(
                domain=domain, query=query, game=game, db_dir=db_dir,
                max_results=max_results, **kwargs,
            )
        else:
            hits = cd.search(
                domain=domain, query=query, game=game, db_dir=db_dir,
                max_results=max_results, **kwargs,
            )
        return [adapter(h) for h in hits]
    except Exception as e:
        return [{
            "title": f"[Error: {domain}]",
            "subtitle": str(e),
            "record_type": "error",
            "source_label": domain,
            "source_key": domain,
            "score": 0,
        }]


# ---------------------------------------------------------------------------
# Detail fetchers
# ---------------------------------------------------------------------------

def _fetch_record_detail(hit: dict, game: str, db_dir: str) -> str:
    form_key = hit.get("form_key", "")
    if not form_key:
        return "[No FormKey]"
    result = cd.get_content(domain="records", id=form_key, game=game, db_dir=db_dir)
    if "error" in result:
        return f"[{result['error']}]"
    yaml_content = result.get("yaml_content", "")
    if yaml_content:
        header = (
            f"FormKey:     {result.get('form_key', '')}\n"
            f"EditorID:    {result.get('editor_id', '')}\n"
            f"Name:        {result.get('name', '')}\n"
            f"Record Type: {result.get('record_type', '')}\n"
            f"Source:      {result.get('source', '')}\n"
            f"{'=' * 80}\n\n"
        )
        return header + yaml_content
    return f"[No YAML content found for {form_key}]"


def _fetch_ext_record_detail(hit: dict, game: str, db_dir: str) -> str:
    form_key = hit.get("form_key", "")
    if not form_key:
        return "[No FormKey]"
    result = cd.get_content(domain="ext_records", id=form_key, game=game, db_dir=db_dir)
    if "error" in result:
        return f"[{result['error']}]"
    yaml_content = result.get("yaml_content", "")
    if yaml_content:
        header = (
            f"FormKey:     {result.get('form_key', '')}\n"
            f"EditorID:    {result.get('editor_id', '')}\n"
            f"Name:        {result.get('name', '')}\n"
            f"Record Type: {result.get('record_type', '')}\n"
            f"Source:      {result.get('source', '') or result.get('mod_name', '')}\n"
            f"{'=' * 80}\n\n"
        )
        return header + yaml_content
    return f"[No YAML content found for {form_key}]"


def _fetch_script_detail(hit: dict, game: str, db_dir: str) -> str:
    script_name = hit.get("script_id", "") or hit.get("title", "")
    if not script_name:
        return "[No script name]"
    result = cd.get_content(domain="scripts", id=script_name, game=game, db_dir=db_dir)
    if "error" in result:
        return f"[{result['error']}]"
    source_code = result.get("source_code", "")
    # If the service layer didn't get source, try PEX decompilation
    if not source_code:
        script_path = result.get("script_path", "")
        if script_path and os.path.isfile(script_path) and script_path.lower().endswith(".pex"):
            try:
                from creation_lib.pex import decompile_pex
                source_code = decompile_pex(Path(script_path))
            except Exception:
                pass
        if not source_code:
            source_code = result.get("content", "")
    if source_code:
        header = (
            f"Script:   {result.get('script_name', '')}\n"
            f"Extends:  {result.get('extends', '')}\n"
            f"Source:   {result.get('source', '')}\n"
            f"Category: {result.get('category', '')}\n"
            f"{'=' * 80}\n\n"
        )
        return header + source_code
    return f"[No source code found for {script_name}]"


def _fetch_ext_script_detail(hit: dict, game: str, db_dir: str) -> str:
    script_id = hit.get("script_id", "") or hit.get("title", "")
    if not script_id:
        return "[No script ID]"
    result = cd.get_content(domain="ext_scripts", id=script_id, game=game, db_dir=db_dir)
    if "error" in result:
        return f"[{result['error']}]"
    source_code = result.get("source_code", "")
    if source_code:
        header = (
            f"Script:   {result.get('script_name', '')}\n"
            f"Extends:  {result.get('extends', '')}\n"
            f"Mod:      {result.get('mod_name', '')}\n"
            f"{'=' * 80}\n\n"
        )
        return header + source_code
    return f"[No source code found for {script_id}]"


def _fetch_wiki_detail(hit: dict, game: str, db_dir: str) -> str:
    filename = hit.get("filename", "")
    if not filename:
        return "[No page filename]"
    result = cd.get_content(domain="wiki", id=filename, game=game, db_dir=db_dir)
    if "error" in result:
        return f"[{result['error']}]"
    content = result.get("content", "")
    if content:
        # Update hit with full page metadata for the detail panel
        hit.update({k: v for k, v in result.items() if k != "content"})
        return content
    return f"[No content found for {filename}]"


def _format_json_list(raw: str, fallback: str = "-") -> str:
    if not raw:
        return fallback
    try:
        items = json.loads(raw)
    except Exception:
        return raw
    if not items:
        return fallback
    return "\n".join(f"- {item}" for item in items)


def _fetch_havok_detail(hit: dict, game: str, db_dir: str) -> str:
    entity_id = hit.get("entity_id", "")
    entity_type = hit.get("entity_type", "")
    if not entity_id:
        return "[Missing Havok entity metadata]"

    # Use service layer for behaviors (it returns rich metadata)
    if entity_type in ("behavior", ""):
        result = cd.get_content(domain="behaviors", id=entity_id, game=game, db_dir=db_dir)
        if "error" not in result and result.get("content_type") == "behavior":
            events = result.get("events", [])
            variables = result.get("variables", [])
            sequences = result.get("sequences", [])
            transitions = result.get("transitions", [])
            vars_formatted = [f"{v['name']} ({v['type']})" for v in variables] if variables else []
            trans_formatted = [f"{t['name']} [{t.get('duration', '')}]" for t in transitions] if transitions else []
            header = (
                f"Behavior:    {result.get('name', '')}\n"
                f"Category:    {result.get('category', '')}\n"
                f"Source:      {result.get('source', '')}\n"
                f"Path:        {result.get('source_path', '')}\n"
                f"Graph Path:  {result.get('graph_path', '')}\n"
                f"Node Count:  {result.get('node_count', 0)}\n"
                f"Usable:      {'Yes' if result.get('usable') else 'No'}\n"
                f"Events:      {', '.join(events) if events else '-'}\n"
                f"Variables:   {', '.join(vars_formatted) if vars_formatted else '-'}\n"
                f"Sequences:   {', '.join(sequences) if sequences else '-'}\n"
                f"Transitions: {', '.join(trans_formatted) if trans_formatted else '-'}\n"
                f"{'=' * 80}\n\n"
            )
            return header + (result.get("content", "") or "[No behavior content indexed]")

    # For other havok entities, use the generic havok domain
    result = cd.get_content(domain="havok", id=entity_id, game=game, db_dir=db_dir)
    if "error" in result:
        return f"[{result['error']}]"

    ctype = result.get("content_type", "")
    if ctype == "animation":
        return (
            f"Animation:    {result.get('name', '')}\n"
            f"Source:       {result.get('source', '')}\n"
            f"Path:         {result.get('source_path', '')}\n"
            f"Actor:        {result.get('actor', '')}\n"
            f"Category:     {result.get('category', '')}\n"
            f"Subcategory:  {result.get('subcategory', '')}\n"
            f"Compression:  {result.get('compression_type', '')}\n"
            f"Bone Count:   {result.get('bone_count', '')}\n"
            f"Duration:     {result.get('duration', '')}\n"
            f"Frame Count:  {result.get('frame_count', '')}\n"
            f"Annotations:\n{_format_json_list(result.get('annotation_tracks', ''))}"
        )
    if ctype == "skeleton":
        return (
            f"Skeleton:      {result.get('name', '')}\n"
            f"Source:        {result.get('source', '')}\n"
            f"Path:          {result.get('source_path', '')}\n"
            f"Bone Count:    {result.get('bone_count', '')}\n"
            f"Float Slots:   {result.get('float_count', '')}\n"
            f"Partitions:\n{_format_json_list(result.get('partition_names', ''))}\n\n"
            f"Bones:\n{_format_json_list(result.get('bone_names', ''))}"
        )
    if ctype == "project":
        return (
            f"Project:       {result.get('name', '')}\n"
            f"Category:      {result.get('category', '')}\n"
            f"Source:        {result.get('source', '')}\n"
            f"Path:          {result.get('source_path', '')}\n"
            f"Havok Version: {result.get('havok_version') or '-'}"
        )
    if ctype == "manifest":
        files = result.get("files", [])
        deps = result.get("dependencies", [])
        file_strs = [f"- {f.get('file_path', '')} ({f.get('file_type', 'unknown')}, {f.get('role', 'owned')})" for f in files]
        dep_strs = [f"- {d.get('depends_on', '')}" for d in deps]
        return (
            f"Manifest:      {result.get('name', '')}\n"
            f"Type:          {result.get('manifest_type', '')}\n"
            f"Source:        {result.get('source', '')}\n"
            f"Project ID:    {result.get('project_id') or '-'}\n"
            f"File Count:    {result.get('file_count', '')}\n"
            f"Total Size:    {result.get('total_size', '')}\n\n"
            f"Dependencies:\n{chr(10).join(dep_strs) if dep_strs else '-'}\n\n"
            f"Files:\n{chr(10).join(file_strs) if file_strs else '-'}"
        )
    # Behavior fallback (from havok domain)
    if ctype == "behavior":
        events = result.get("events", [])
        variables = result.get("variables", [])
        sequences = result.get("sequences", [])
        transitions = result.get("transitions", [])
        vars_formatted = [f"{v['name']} ({v['type']})" for v in variables] if variables else []
        trans_formatted = [f"{t['name']} [{t.get('duration', '')}]" for t in transitions] if transitions else []
        header = (
            f"Behavior:    {result.get('name', '')}\n"
            f"Category:    {result.get('category', '')}\n"
            f"Source:      {result.get('source', '')}\n"
            f"Path:        {result.get('source_path', '')}\n"
            f"Graph Path:  {result.get('graph_path', '')}\n"
            f"Node Count:  {result.get('node_count', 0)}\n"
            f"Events:      {', '.join(events) if events else '-'}\n"
            f"Variables:   {', '.join(vars_formatted) if vars_formatted else '-'}\n"
            f"Sequences:   {', '.join(sequences) if sequences else '-'}\n"
            f"Transitions: {', '.join(trans_formatted) if trans_formatted else '-'}\n"
            f"{'=' * 80}\n"
        )
        return header
    return f"[No detail renderer for content_type '{ctype}']"


def _fetch_nif_detail(hit: dict, game: str, db_dir: str) -> str:
    nif_id = hit.get("nif_id", "")
    if not nif_id:
        return "[No NIF ID]"
    result = cd.get_content(domain="nifs", id=nif_id, game=game, db_dir=db_dir)
    if "error" in result:
        return f"[{result['error']}]"
    textures = result.get("textures", [])
    materials = result.get("materials", [])
    behavior_refs = result.get("behavior_refs", [])
    sequences = result.get("sequences", [])
    block_types = result.get("block_types", {})
    header = (
        f"NIF:          {result.get('name', '')}\n"
        f"Path:         {result.get('path', '')}\n"
        f"Category:     {result.get('category', '')}\n"
        f"Source:       {result.get('source', '')}\n"
        f"Root Type:    {result.get('root_type', '')}\n"
        f"Block Count:  {result.get('block_count', 0)}\n"
        f"Particles:    {'Yes' if result.get('has_particles') else 'No'}\n"
        f"Behavior:     {'Yes' if result.get('has_behavior') else 'No'}\n"
        f"Controllers:  {'Yes' if result.get('has_controllers') else 'No'}\n"
    )
    sections = []
    if block_types:
        lines = [f"  {name}: {count}" for name, count in sorted(block_types.items(), key=lambda x: -x[1])]
        sections.append("Block Types:\n" + "\n".join(lines))
    if textures:
        sections.append("Textures:\n" + "\n".join(f"  - {t}" for t in textures))
    if materials:
        sections.append("Materials:\n" + "\n".join(f"  - {m}" for m in materials))
    if behavior_refs:
        sections.append("Behavior Refs:\n" + "\n".join(f"  - {b}" for b in behavior_refs))
    if sequences:
        sections.append("Sequences:\n" + "\n".join(f"  - {s}" for s in sequences))
    return header + "\n" + "\n\n".join(sections) if sections else header


def _fetch_function_detail(hit: dict, game: str, db_dir: str) -> str:
    """Detail fetcher for function lookup results."""
    content = hit.get("content", "")
    if content:
        return content
    filename = hit.get("filename", "")
    if filename:
        result = cd.get_content(domain="wiki", id=filename, game=game, db_dir=db_dir)
        if "error" not in result:
            return result.get("content", "[No content]")
    return "[No function detail available]"


_DETAIL_FETCHERS = {
    "records": _fetch_record_detail,
    "scripts": _fetch_script_detail,
    "wiki": _fetch_wiki_detail,
    "havok": _fetch_havok_detail,
    "nifs": _fetch_nif_detail,
    "ext_records": _fetch_ext_record_detail,
    "ext_scripts": _fetch_ext_script_detail,
    "function": _fetch_function_detail,
}


# ---------------------------------------------------------------------------
# Query modes
# ---------------------------------------------------------------------------
_QUERY_MODES = [
    ("Full-text Search", "full_text"),
    ("Semantic Search", "semantic"),
    ("FormKey Lookup", "formkey"),
    ("EditorID Lookup", "editor_id"),
    ("Script Name", "script"),
    ("Find References", "references"),
    ("Keyword Search", "keyword_search"),
    ("Function Lookup", "function"),
    ("Browse / List", "browse"),
]
_QUERY_MODE_LABELS = [m[0] for m in _QUERY_MODES]
_QUERY_MODE_KEYS = [m[1] for m in _QUERY_MODES]


# ---------------------------------------------------------------------------
# SearchApp
# ---------------------------------------------------------------------------

class SearchApp:
    """Game data search UI.

    In toolkit mode: call draw_search_panel() / draw_content_panel() inside
    your own imgui.begin/end blocks.

    In standalone mode: call gui() which manages its own windows.
    """

    def __init__(self, game: str = "fo4"):
        global _ACTIVE_GAME
        _ACTIVE_GAME = game

        self._results: list[dict] = []
        self._search_worker: AsyncWorker | None = None
        self._detail_worker: AsyncWorker | None = None
        self._current_hit: dict | None = None

        self._query = ""
        self._query_mode_idx = 0
        self._name_only = True

        # Source toggles
        self._src_records = True
        self._src_wiki = True
        self._src_scripts = True
        self._src_havok = True
        self._src_nifs = False
        self._src_ext_records = False
        self._src_ext_scripts = False

        # DB availability
        self._db_records_ok = _cd_db_available("records", game, _DB_DIR)
        self._db_wiki_ok = _cd_db_available("wiki", game, _DB_DIR)
        self._db_scripts_ok = _cd_db_available("scripts", game, _DB_DIR)
        self._db_havok_ok = _cd_db_available("havok", game, _DB_DIR)
        self._db_nifs_ok = _cd_db_available("nifs", game, _DB_DIR)
        self._db_ext_ok = _cd_db_available("ext_records", game, _DB_DIR)

        self._selected_idx = -1

        self._content_text = ""
        self._content_source_key = ""
        self._content_lines: list[list[tuple]] | None = None
        self._content_markdown = ""
        self._raw_mode = False

        self._status_text = "Ready"
        self._searching = False
        self._fo4_data_enabled: bool = True

        # Browse mode state
        self._browse_domain_idx = 0
        self._browse_domains = [
            "record_types", "records", "scripts", "extends_types",
            "wiki_categories", "script_types", "behaviors", "havok",
            "nifs", "nif_categories", "ext_mods",
        ]

    def set_game(self, game: str) -> None:
        """Switch active game — updates DB availability checks."""
        global _ACTIVE_GAME
        if game == _ACTIVE_GAME:
            return
        _ACTIVE_GAME = game
        self._db_records_ok = _cd_db_available("records", game, _DB_DIR)
        self._db_wiki_ok = _cd_db_available("wiki", game, _DB_DIR)
        self._db_scripts_ok = _cd_db_available("scripts", game, _DB_DIR)
        self._db_havok_ok = _cd_db_available("havok", game, _DB_DIR)
        self._db_nifs_ok = _cd_db_available("nifs", game, _DB_DIR)
        self._db_ext_ok = _cd_db_available("ext_records", game, _DB_DIR)
        self._results = []
        self._current_hit = None
        self._content_text = ""

    def set_index_flags(self, fo4_data: bool = True, nifs: bool = True, behaviors: bool = True) -> None:
        """Control which index-dependent features are visible."""
        self._fo4_data_enabled = fo4_data

    # -----------------------------------------------------------------------
    # Worker polling (must be called once per frame before drawing)
    # -----------------------------------------------------------------------

    def _poll_search_worker(self):
        if self._search_worker and self._search_worker.done:
            if self._search_worker.error:
                self._status_text = f"Error: {self._search_worker.error}"
                self._results = []
            else:
                results, elapsed = self._search_worker.result
                self._results = results
                count = len(results)
                self._status_text = (
                    f"{count} result{'s' if count != 1 else ''} in {elapsed:.3f}s"
                )
            self._search_worker = None
            self._searching = False

    def _poll_detail_worker(self):
        if self._detail_worker and self._detail_worker.done:
            if self._detail_worker.error:
                self._content_text = f"[Error: {self._detail_worker.error}]"
                self._content_source_key = ""
                self._content_lines = None
                self._content_markdown = ""
            else:
                text, source_key = self._detail_worker.result
                self._content_source_key = source_key
                self._content_text = text
                if source_key in ("wiki", "function"):
                    hit = self._current_hit or {}
                    meta_parts = []
                    if hit.get("category"):
                        meta_parts.append(f"**Category:** {hit['category']}")
                    if hit.get("parent_script"):
                        meta_parts.append(f"**Script:** {hit['parent_script']}")
                    if hit.get("function_name"):
                        meta_parts.append(f"**Function:** {hit['function_name']}")
                    meta_line = "  |  ".join(meta_parts)
                    title = hit.get("title", "")
                    self._content_markdown = f"## {title}\n\n{meta_line}\n\n---\n\n{text}"
                    self._content_lines = None
                elif source_key in ("records", "ext_records"):
                    self._content_lines = syntax_highlight_yaml(text)
                    self._content_markdown = ""
                elif source_key in ("scripts", "ext_scripts"):
                    self._content_lines = syntax_highlight_papyrus(text)
                    self._content_markdown = ""
                else:
                    self._content_lines = None
                    self._content_markdown = ""
            self._detail_worker = None

    # -----------------------------------------------------------------------
    # Search dispatch
    # -----------------------------------------------------------------------

    def _start_search(self):
        query = self._query.strip()
        mode = _QUERY_MODE_KEYS[self._query_mode_idx]

        # Browse mode doesn't need a query
        if mode != "browse" and not query:
            return

        self._searching = True
        self._status_text = "Searching..."
        self._results = []
        self._selected_idx = -1
        self._content_text = ""
        self._content_source_key = ""
        self._content_lines = None
        self._content_markdown = ""

        game = _ACTIVE_GAME
        db_dir = _DB_DIR

        if mode in ("full_text", "semantic"):
            sources = []
            if self._src_records and self._db_records_ok:
                sources.append("records")
            if self._src_wiki and self._db_wiki_ok:
                sources.append("wiki")
            if self._src_scripts and self._db_scripts_ok:
                sources.append("scripts")
            if self._src_havok and self._db_havok_ok:
                sources.append("havok")
            if self._src_nifs and self._db_nifs_ok:
                sources.append("nifs")
            if self._src_ext_records and self._db_ext_ok:
                sources.append("ext_records")
            if self._src_ext_scripts and self._db_ext_ok:
                sources.append("ext_scripts")
            if not sources:
                self._status_text = "No sources selected"
                self._searching = False
                return

            is_semantic = mode == "semantic"

            def _do_search(q, srcs, sem, g, d):
                t0 = time.perf_counter()
                all_hits = []
                for src in srcs:
                    all_hits.extend(_search_domain(src, q, g, d, max_results=50, semantic=sem))
                if sem:
                    all_hits.sort(key=lambda h: h.get("score", 0), reverse=True)
                return all_hits, time.perf_counter() - t0

            self._search_worker = AsyncWorker(
                target_fn=_do_search, args=(query, sources, is_semantic, game, db_dir)
            )
            self._search_worker.start()

        elif mode == "formkey":
            def _do_formkey(q, g, d):
                try:
                    result = cd.get_record(form_key=q, include_content=False, game=g, db_dir=d)
                    if "error" in result:
                        return [{"title": "[Not found]", "subtitle": result["error"],
                                 "record_type": "error", "source_label": "Records",
                                 "source_key": "records", "score": 0}], 0.0
                    return [_adapt_record(result)], 0.0
                except Exception as e:
                    return [{"title": "[Error]", "subtitle": str(e),
                             "record_type": "error", "source_label": "Records",
                             "source_key": "records", "score": 0}], 0.0
            self._search_worker = AsyncWorker(target_fn=_do_formkey, args=(query, game, db_dir))
            self._search_worker.start()

        elif mode == "editor_id":
            def _do_editor_id(q, g, d):
                try:
                    results = cd.lookup_editor_id(editor_id=q, game=g, db_dir=d)
                    return [_adapt_record(r) for r in results], 0.0
                except ValueError as e:
                    return [{"title": "[Not found]", "subtitle": str(e),
                             "record_type": "error", "source_label": "Records",
                             "source_key": "records", "score": 0}], 0.0
            self._search_worker = AsyncWorker(target_fn=_do_editor_id, args=(query, game, db_dir))
            self._search_worker.start()

        elif mode == "script":
            def _do_script(q, g, d):
                result = cd.get_content(domain="scripts", id=q, game=g, db_dir=d)
                if "error" in result:
                    return [{"title": "[Not found]", "subtitle": result["error"],
                             "record_type": "error", "source_label": "Scripts",
                             "source_key": "scripts", "score": 0}], 0.0
                return [_adapt_script(result)], 0.0
            self._search_worker = AsyncWorker(target_fn=_do_script, args=(query, game, db_dir))
            self._search_worker.start()

        elif mode == "references":
            def _do_refs(q, g, d):
                try:
                    results = cd.get_references(form_key=q, game=g, db_dir=d, max_results=100)
                    return [_adapt_record(r) for r in results], 0.0
                except ValueError as e:
                    return [{"title": "[Not found]", "subtitle": str(e),
                             "record_type": "info", "source_label": "Records",
                             "source_key": "records", "score": 0}], 0.0
            self._search_worker = AsyncWorker(target_fn=_do_refs, args=(query, game, db_dir))
            self._search_worker.start()

        elif mode == "keyword_search":
            def _do_keyword(q, g, d):
                try:
                    results = cd.search_by_keyword(keyword=q, game=g, db_dir=d, max_results=50)
                    return [_adapt_record(r) for r in results], 0.0
                except ValueError as e:
                    return [{"title": "[Not found]", "subtitle": str(e),
                             "record_type": "error", "source_label": "Records",
                             "source_key": "records", "score": 0}], 0.0
            self._search_worker = AsyncWorker(target_fn=_do_keyword, args=(query, game, db_dir))
            self._search_worker.start()

        elif mode == "function":
            def _do_function(q, g, d):
                result = cd.get_function(function_name=q, game=g, db_dir=d)
                if isinstance(result, dict) and "error" in result:
                    return [{"title": "[Not found]", "subtitle": result["error"],
                             "record_type": "error", "source_label": "Wiki",
                             "source_key": "function", "score": 0}], 0.0
                if isinstance(result, list):
                    adapted = []
                    for r in result:
                        adapted.append({
                            "title": r.get("function_name", "") or r.get("title", ""),
                            "subtitle": f"on {r.get('parent_script', '')}"
                                        + (f" (inherited from {r['inherited_from']})" if r.get("inherited_from") else ""),
                            "record_type": r.get("category", ""),
                            "source_label": "Wiki",
                            "source_key": "function",
                            "score": 1.0,
                            "filename": r.get("filename", ""),
                            "category": r.get("category", ""),
                            "parent_script": r.get("parent_script", ""),
                            "function_name": r.get("function_name", ""),
                            "content": r.get("content", ""),
                        })
                    return adapted, 0.0
                return [], 0.0
            self._search_worker = AsyncWorker(target_fn=_do_function, args=(query, game, db_dir))
            self._search_worker.start()

        elif mode == "browse":
            domain = self._browse_domains[self._browse_domain_idx]

            def _do_browse(dom, g, d):
                try:
                    results = cd.list_items(domain=dom, game=g, db_dir=d, max_results=200)
                    # Adapt results based on what list_items returns
                    if isinstance(results, dict):
                        # Count dict (e.g. record_types, extends_types)
                        adapted = [{"title": k, "subtitle": f"{v} items",
                                    "record_type": "count", "source_label": dom,
                                    "source_key": "browse", "score": v}
                                   for k, v in sorted(results.items(), key=lambda x: -x[1])]
                        return adapted, 0.0
                    if isinstance(results, list):
                        if results and isinstance(results[0], str):
                            # List of strings
                            adapted = [{"title": s, "subtitle": "",
                                        "record_type": dom, "source_label": dom,
                                        "source_key": "browse", "score": 1.0}
                                       for s in results]
                            return adapted, 0.0
                        if results and isinstance(results[0], dict):
                            adapter = _DOMAIN_ADAPTER.get(dom)
                            if adapter:
                                return [adapter(r) for r in results], 0.0
                            # Generic dict display
                            adapted = []
                            for r in results:
                                title = (r.get("editor_id") or r.get("script_name")
                                         or r.get("mod_name") or r.get("name") or str(r))
                                subtitle = r.get("record_type", "") or r.get("extends", "")
                                adapted.append({"title": title, "subtitle": subtitle,
                                                "record_type": dom, "source_label": dom,
                                                "source_key": "browse", "score": 1.0, **r})
                            return adapted, 0.0
                    return [], 0.0
                except Exception as e:
                    return [{"title": "[Error]", "subtitle": str(e),
                             "record_type": "error", "source_label": dom,
                             "source_key": "browse", "score": 0}], 0.0

            self._search_worker = AsyncWorker(target_fn=_do_browse, args=(domain, game, db_dir))
            self._search_worker.start()

    def _start_detail_fetch(self, hit: dict):
        self._current_hit = hit
        self._content_text = "Loading..."
        self._content_source_key = ""
        self._content_lines = None
        self._content_markdown = ""

        source_key = hit.get("source_key", "")
        fetcher = _DETAIL_FETCHERS.get(source_key)
        if not fetcher:
            self._content_text = f"[Unknown source type: {source_key}]"
            return

        game = _ACTIVE_GAME
        db_dir = _DB_DIR

        def _do_fetch(h, sk, fn, g, d):
            try:
                text = fn(h, g, d)
            except Exception as e:
                return f"[Error loading detail: {e}]", ""
            return text, sk

        self._detail_worker = AsyncWorker(target_fn=_do_fetch, args=(hit, source_key, fetcher, game, db_dir))
        self._detail_worker.start()

    # -----------------------------------------------------------------------
    # Drawing primitives
    # -----------------------------------------------------------------------

    def _draw_controls(self):
        avail_w = imgui.get_content_region_avail().x

        mode = _QUERY_MODE_KEYS[self._query_mode_idx]

        # Query input (hide for browse mode)
        if mode != "browse":
            imgui.set_next_item_width(avail_w - 80)
            changed, self._query = imgui.input_text_with_hint(
                "##search", "Search, FormKey, EditorID, script name...", self._query
            )
            if imgui.is_item_deactivated_after_edit():
                self._start_search()
            imgui.same_line()
            if imgui.button("Search", imgui.ImVec2(70, 0)) and not self._searching:
                self._start_search()

        # Game selector
        game_ids = list(GAME_PROFILES.keys())
        game_labels = [GAME_PROFILES[g].display_name for g in game_ids]
        current_game_idx = game_ids.index(_ACTIVE_GAME) if _ACTIVE_GAME in game_ids else 0
        imgui.text("Game:")
        imgui.same_line()
        imgui.set_next_item_width(160)
        game_changed, new_game_idx = imgui.combo("##search_game", current_game_idx, game_labels)
        if game_changed:
            self.set_game(game_ids[new_game_idx])

        imgui.text("Query mode:")
        imgui.same_line()
        imgui.set_next_item_width(200)
        _, self._query_mode_idx = imgui.combo("##mode", self._query_mode_idx, _QUERY_MODE_LABELS)

        # Browse domain selector
        if mode == "browse":
            imgui.text("Domain:")
            imgui.same_line()
            imgui.set_next_item_width(200)
            changed, self._browse_domain_idx = imgui.combo(
                "##browse_domain", self._browse_domain_idx, self._browse_domains
            )
            if imgui.button("Browse", imgui.ImVec2(70, 0)) and not self._searching:
                self._start_search()

        # Source checkboxes — only for full_text and semantic modes
        if mode in ("full_text", "semantic"):
            # Scope toggle (only for full_text)
            if mode != "full_text":
                imgui.begin_disabled()
            _, self._name_only = imgui.checkbox("Name/title only", self._name_only)
            if mode != "full_text":
                imgui.end_disabled()
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Checked: search name/EditorID columns only (precise, fewer results)\n"
                    "Unchecked: search full content incl. source code and YAML (noisy)"
                )

            imgui.text("Sources:")

            for enabled, db_ok, attr, label in [
                (self._src_records,     self._db_records_ok, "_src_records",     "Records"),
                (self._src_wiki,        self._db_wiki_ok,    "_src_wiki",        "Wiki"),
                (self._src_scripts,     self._db_scripts_ok, "_src_scripts",     "Scripts"),
                (self._src_havok,       self._db_havok_ok,   "_src_havok",       "Havok"),
                (self._src_nifs,        self._db_nifs_ok,    "_src_nifs",        "NIFs"),
                (self._src_ext_records, self._db_ext_ok,     "_src_ext_records", "Ext Records"),
                (self._src_ext_scripts, self._db_ext_ok,     "_src_ext_scripts", "Ext Scripts"),
            ]:
                if not db_ok:
                    imgui.begin_disabled()
                _, val = imgui.checkbox(label, enabled)
                if not db_ok:
                    imgui.end_disabled()
                else:
                    setattr(self, attr, val)

        imgui.separator()

    def _draw_results(self):
        if not self._results:
            imgui.text_disabled("Searching..." if self._searching else "No results")
            return

        imgui.begin_child("results_scroll", imgui.ImVec2(0, 0))
        for i, hit in enumerate(self._results):
            source_key = hit.get("source_key", "")
            color = SOURCE_COLORS.get(source_key, imgui.ImVec4(0.7, 0.7, 0.7, 1.0))

            title = hit.get("title", "?")
            subtitle = hit.get("subtitle", "")
            label = title if not subtitle else f"{title}  --  {subtitle}"
            record_type = hit.get("record_type", "")
            source_label = hit.get("source_label", "")

            imgui.text_colored(color, f"[{source_label}]")
            imgui.same_line()

            is_selected = i == self._selected_idx
            if imgui.selectable(
                f"{label}##result_{i}",
                is_selected,
                imgui.SelectableFlags_.none.value,
            )[0]:
                if i != self._selected_idx:
                    self._selected_idx = i
                    self._start_detail_fetch(hit)

            if record_type:
                imgui.text_disabled(f"    {record_type}")

        imgui.end_child()

    def _draw_content(self):
        if not self._content_text and not self._content_markdown:
            imgui.text_disabled("Select a result to view details")
            return

        has_formatted = bool(self._content_markdown or self._content_lines)
        if has_formatted:
            _, self._raw_mode = imgui.checkbox("Raw", self._raw_mode)
            imgui.separator()

        avail = imgui.get_content_region_avail()

        if has_formatted and not self._raw_mode:
            if self._content_markdown:
                imgui.begin_child("content_scroll", imgui.ImVec2(0, 0))
                imgui_md.render(self._content_markdown)
                imgui.end_child()
            else:
                imgui.begin_child(
                    "content_scroll", imgui.ImVec2(0, 0),
                    child_flags=imgui.ChildFlags_.none.value,
                )
                for spans in self._content_lines:
                    draw_highlighted_text(spans)
                imgui.end_child()
        else:
            imgui.input_text_multiline(
                "##content",
                self._content_text,
                imgui.ImVec2(avail.x, avail.y),
                imgui.InputTextFlags_.read_only,
            )

    # -----------------------------------------------------------------------
    # Public API
    # -----------------------------------------------------------------------

    def draw_search_panel(self):
        """Draw controls + results. Caller owns the imgui.begin/end."""
        if not self._fo4_data_enabled:
            imgui.spacing()
            imgui.text_disabled("Search indexes are not built.")
            imgui.text_disabled("Go to Workspace > Settings > Indexes to build them.")
            return
        self._poll_search_worker()
        self._draw_controls()
        self._draw_results()

    def draw_content_panel(self):
        """Draw the content viewer. Caller owns the imgui.begin/end."""
        self._poll_detail_worker()
        self._draw_content()

    def gui(self):
        """Standalone mode: draw all panels with built-in windows + status bar."""
        self._poll_search_worker()
        self._poll_detail_worker()

        self._draw_controls()

        if imgui.begin("Results"):
            self._draw_results()
        imgui.end()

        if imgui.begin("Content"):
            self._draw_content()
        imgui.end()

        draw_status_bar(self._status_text)
