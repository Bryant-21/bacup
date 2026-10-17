"""Summary, filters and the cell/cluster findings tables."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.previs.state import ACTION_LABELS, ACTIONS

ACTION_COLORS = {
    "rebuild_precombines": (1.0, 0.45, 0.4, 1.0),
    "rebuild_previs": (1.0, 0.7, 0.35, 1.0),
    "fix": (0.95, 0.9, 0.45, 1.0),
    "warn": (0.75, 0.75, 0.6, 1.0),
    "info": (0.6, 0.6, 0.6, 1.0),
}
_TABLE_FLAGS = (
    imgui.TableFlags_.resizable
    | imgui.TableFlags_.row_bg
    | imgui.TableFlags_.scroll_y
    | imgui.TableFlags_.borders_inner_v
)


def draw(app) -> None:
    state = app.state
    report = state.report
    if report is None:
        imgui.text_disabled("Choose a load order and press Check load order.")
        return
    _draw_summary(report["summary"])
    imgui.separator()
    for action in ACTIONS:
        enabled = action in state.action_filter
        imgui.push_style_color(imgui.Col_.text, ACTION_COLORS[action])
        changed, enabled = imgui.checkbox(f"{ACTION_LABELS[action]}##previs_filter_{action}", enabled)
        imgui.pop_style_color()
        if changed:
            (state.action_filter.add if enabled else state.action_filter.discard)(action)
        imgui.same_line()
    _, state.show_overridden_only = imgui.checkbox("Mod-overridden, no findings##previs_filter_overridden", state.show_overridden_only)
    imgui.set_next_item_width(-1)
    _, state.search = imgui.input_text_with_hint("##previs_search", "Search cell, EditorID, worldspace, plugin or rule", state.search)

    if imgui.begin_tab_bar("##previs_tabs"):
        cells = state.visible_cells()
        if imgui.begin_tab_item(f"Cells ({len(cells)})###previs_tab_cells")[0]:
            _draw_cells(state, cells)
            imgui.end_tab_item()
        clusters = state.visible_clusters()
        if imgui.begin_tab_item(f"Previs clusters ({len(clusters)})###previs_tab_clusters")[0]:
            _draw_clusters(state, clusters)
            imgui.end_tab_item()
        imgui.end_tab_bar()


def _draw_summary(summary: dict) -> None:
    counts = dict((a, n) for a, n in summary.get("actions", []))
    imgui.text(f"{summary['plugins']} plugins, {summary['cells']:,} cells checked in {summary['seconds']:.1f} s")
    imgui.text(f"{summary['overridden_by_mod']:,} cells changed by mods, {summary['clusters_to_rebuild']} previs clusters to rebuild")
    parts = [f"{ACTION_LABELS[a]}: {counts[a]}" for a in ACTIONS if counts.get(a)]
    imgui.text("Cells needing work: " + (", ".join(parts) if parts else "none"))
    if summary.get("skipped_esl_origin"):
        count = summary["skipped_esl_origin"]
        imgui.text_disabled(f"{count} {'cell' if count == 1 else 'cells'} defined in ESL-flagged plugins can't have previsbines and {'was' if count == 1 else 'were'} skipped.")
    if summary.get("loose_previsbines"):
        imgui.push_style_color(imgui.Col_.text, ACTION_COLORS["warn"])
        imgui.text_wrapped("Loose precombined meshes or tomes were found; their owning plugin is unknown, so ownership checks are less reliable.")
        imgui.pop_style_color()


def _draw_cells(state, cells: list[dict]) -> None:
    if not imgui.begin_table("##previs_cells", 6, _TABLE_FLAGS):
        return
    imgui.table_setup_scroll_freeze(0, 1)
    imgui.table_setup_column("Action", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("Rebuild precombines").x)
    imgui.table_setup_column("Cell")
    imgui.table_setup_column("EditorID")
    imgui.table_setup_column("Location")
    imgui.table_setup_column("Winning plugin")
    imgui.table_setup_column("First finding", imgui.TableColumnFlags_.width_stretch, 2.0)
    imgui.table_headers_row()
    clipper = imgui.ListClipper()
    clipper.begin(len(cells))
    while clipper.step():
        for index in range(clipper.display_start, clipper.display_end):
            cell = cells[index]
            imgui.table_next_row()
            imgui.table_next_column()
            _action_label(cell.get("action"))
            imgui.table_next_column()
            selected = state.selected_cell == cell["form_key"]
            flags = imgui.SelectableFlags_.span_all_columns | imgui.SelectableFlags_.allow_overlap
            if imgui.selectable(f"{cell['form_key']}##previs_cell_{index}", selected, flags)[0]:
                state.selected_cell = cell["form_key"]
                state.selected_cluster = cell.get("cluster") or ""
            imgui.table_next_column()
            imgui.text_unformatted(cell.get("editor_id") or "")
            imgui.table_next_column()
            imgui.text_unformatted(_location(cell))
            imgui.table_next_column()
            imgui.text_unformatted(cell.get("winning_plugin") or "")
            imgui.table_next_column()
            findings = cell.get("findings") or []
            imgui.text_unformatted(findings[0]["message"] if findings else "changed by a mod; no findings")
    imgui.end_table()


def _draw_clusters(state, clusters: list[dict]) -> None:
    if not imgui.begin_table("##previs_clusters", 5, _TABLE_FLAGS):
        return
    imgui.table_setup_scroll_freeze(0, 1)
    imgui.table_setup_column("Action", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("Rebuild precombines").x)
    imgui.table_setup_column("Cluster root")
    imgui.table_setup_column("Location")
    imgui.table_setup_column("Previs owner")
    imgui.table_setup_column("First finding", imgui.TableColumnFlags_.width_stretch, 2.0)
    imgui.table_headers_row()
    clipper = imgui.ListClipper()
    clipper.begin(len(clusters))
    while clipper.step():
        for index in range(clipper.display_start, clipper.display_end):
            cluster = clusters[index]
            imgui.table_next_row()
            imgui.table_next_column()
            _action_label(cluster.get("action"))
            imgui.table_next_column()
            flags = imgui.SelectableFlags_.span_all_columns | imgui.SelectableFlags_.allow_overlap
            if imgui.selectable(f"{cluster['form_key']}##previs_cluster_{index}", state.selected_cluster == cluster["form_key"], flags)[0]:
                state.selected_cluster = cluster["form_key"]
                state.selected_cell = ""
            imgui.table_next_column()
            imgui.text_unformatted(_location(cluster))
            imgui.table_next_column()
            imgui.text_unformatted(cluster.get("previs_owner") or "")
            imgui.table_next_column()
            findings = cluster.get("findings") or []
            imgui.text_unformatted(findings[0]["message"] if findings else "")
    imgui.end_table()


def _action_label(action: str | None) -> None:
    if action is None:
        imgui.text_disabled("-")
        return
    imgui.push_style_color(imgui.Col_.text, ACTION_COLORS[action])
    imgui.text_unformatted(ACTION_LABELS[action])
    imgui.pop_style_color()


def _location(item: dict) -> str:
    if item.get("interior"):
        return "interior"
    world, grid = item.get("world"), item.get("grid")
    if grid:
        return f"{world or '?'} {grid[0]},{grid[1]}"
    return world or "interior"
