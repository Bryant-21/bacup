"""Every finding for the selected cell and its previs cluster."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.previs.panels.findings_panel import ACTION_COLORS
from ui.previs.state import ACTION_LABELS


def draw(app) -> None:
    state = app.state
    cell = app.selected_cell()
    cluster = app.cluster(state.selected_cluster)
    if cell is None and cluster is None:
        imgui.text_disabled("Select a cell or cluster.")
        return
    if cell is not None:
        imgui.text(cell["form_key"])
        if cell.get("editor_id"):
            imgui.text(cell["editor_id"])
        _field("Location", "interior" if cell.get("interior") else f"{cell.get('world') or '?'} {cell.get('grid') or ''}")
        _field("Winning plugin", cell.get("winning_plugin"))
        _field("Changed by mods", "yes" if cell.get("overridden_by_mod") else "no")
        _field("Previs cluster", cell.get("cluster") or "-")
        imgui.separator()
        _findings(cell.get("findings") or [], "cell")
    if cluster is not None:
        imgui.separator()
        imgui.text(f"Previs cluster {cluster['form_key']}")
        _field("Previs owner", cluster.get("previs_owner") or "unknown")
        _field("Members", ", ".join(cluster.get("members") or []))
        _findings(cluster.get("findings") or [], "cluster")
    elif cell is not None and cell.get("cluster"):
        imgui.separator()
        imgui.text_disabled(f"Cluster {cell['cluster']} has no previs findings.")


def _field(label: str, value) -> None:
    imgui.text_disabled(f"{label}:")
    imgui.same_line()
    imgui.text_wrapped(str(value))


def _findings(findings: list[dict], key: str) -> None:
    if not findings:
        imgui.text_disabled("No findings.")
        return
    for index, finding in enumerate(findings):
        imgui.push_id(f"{key}_{index}")
        imgui.push_style_color(imgui.Col_.text, ACTION_COLORS[finding["action"]])
        imgui.bullet_text(ACTION_LABELS[finding["action"]])
        imgui.pop_style_color()
        imgui.indent()
        imgui.text_wrapped(finding["message"])
        _field("Rule", finding["code"])
        if finding.get("blamed"):
            _field("Caused by", finding["blamed"])
        if finding.get("record"):
            _field("Record", finding["record"])
        imgui.unindent()
        imgui.pop_id()
