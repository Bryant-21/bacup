"""NpcStats: preview what a level curve gives at several levels and warn about curves that are not defined."""

from __future__ import annotations

from creation_lib.catalog.plugin_catalog import parse_form_key

from ui.tools.catalog_editor.features import FeatureExtension, RemoveWording, ResultView, register
from ui.tools.catalog_editor.model import REMOVED, FeatureDocument, Problem

FEATURE = "NpcStats"
SAMPLE_LEVELS = (1, 5, 10, 25, 50, 100)
_SCALED_SECTIONS = ("npcs", "weapons", "explosions", "armors", "races")


def curve_at(points: list, level: float) -> float:
    """Tales' CurveAt: authored order, x held to the previous x, flat outside the points, linear between."""
    if not points:
        return 0.0
    xs: list[float] = []
    for point in points[:64]:
        xs.append(max(point[0], xs[-1]) if xs else point[0])
    if level <= xs[0]:
        return float(points[0][1])
    for index in range(1, len(xs)):
        if level <= xs[index]:
            span = xs[index] - xs[index - 1]
            if span <= 0:
                return float(points[index][1])
            return points[index - 1][1] + (points[index][1] - points[index - 1][1]) * (level - xs[index - 1]) / span
    return float(points[len(xs) - 1][1])


def preview(points: list, levels=SAMPLE_LEVELS) -> list[tuple[int, float]]:
    return [(level, curve_at(points, level)) for level in levels]


def _canon(key: str) -> str:
    parsed = parse_form_key(key) if isinstance(key, str) else None
    return f"{parsed[0]}:{parsed[1]:X}" if parsed else key


def curve_points(doc: FeatureDocument) -> dict[str, list]:
    if "curves" not in doc.sections():
        return {}
    return {_canon(row.key): row.entry.get("points") or [] for row in doc.rows("curves")
            if row.status != REMOVED and isinstance(row.entry, dict)}


def used_curves(entry: dict) -> list[str]:
    """Every curve key a row of npcs/weapons/explosions/armors/races refers to."""
    out: list[str] = []

    def walk(value) -> None:
        if isinstance(value, dict):
            for name, child in value.items():
                if name in ("curve", "health_curve", "xp_curve") and isinstance(child, str) and child:
                    out.append(child)
                else:
                    walk(child)

    walk(entry)
    return out


class NpcStatsExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != FEATURE:
            return []
        known = curve_points(doc)
        return [
            Problem(doc.name, section, row.key, f"Curve {curve} is not in any Curves row, so the flat value is used.", blocking=False)
            for section in _SCALED_SECTIONS if section in doc.sections()
            for row in doc.rows(section) if row.status != REMOVED and isinstance(row.entry, dict)
            for curve in used_curves(row.entry) if _canon(curve) not in known
        ]

    def result_view(self, doc, section):
        if section == "curves":
            return ResultView("Value by level", draw_curve)
        if section in _SCALED_SECTIONS:
            return ResultView("Stats by level", draw_scaled)
        return None

    def sentence(self, view):
        if view.section == "curves":
            return ("A curve turns an actor's level into a number. Between points it blends smoothly; "
                    "below the first and above the last point it stays flat.")
        return ""

    def remove_wording(self, doc, section):
        if section not in _SCALED_SECTIONS:
            return None
        return RemoveWording("Keep Fallout 4's stats", "Your plugin removes this row, so the record keeps the stats "
                             "Fallout 4 gives it.", "Removed: the record keeps Fallout 4's own stats.")


def _level_table(name: str, points: list) -> None:
    from imgui_bundle import imgui

    flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h
    if imgui.begin_table(f"##levels{name}", 1 + len(SAMPLE_LEVELS), flags):
        imgui.table_setup_column("Curve")
        for level in SAMPLE_LEVELS:
            imgui.table_setup_column(f"L{level}")
        imgui.table_headers_row()
        imgui.table_next_row()
        imgui.table_next_column()
        imgui.text(name)
        for _, value in preview(points):
            imgui.table_next_column()
            imgui.text(f"{value:g}")
        imgui.end_table()


def draw_curve(view):
    import numpy as np
    from imgui_bundle import imgui

    points = (view.entry or {}).get("points") or [] if isinstance(view.entry, dict) else []
    if not points:
        imgui.text_disabled("The curve has no points yet.")
        return False, view.entry
    values = [curve_at(points, level) for level in range(1, 101)]
    imgui.plot_lines("##curve_values", np.array(values, dtype=np.float32), overlay_text="level 1 to 100",
                     graph_size=imgui.ImVec2(-1, max(80.0, imgui.get_content_region_avail().x * 0.2)))
    _level_table("This curve", points)
    return False, view.entry


def draw_scaled(view):
    from imgui_bundle import imgui

    from ui.tools.catalog_editor.workbench import humanize, value_text

    if not isinstance(view.entry, dict):
        return False, view.entry
    curves = curve_points(view.doc)
    used = list(dict.fromkeys(used_curves(view.entry)))
    if used:
        imgui.separator_text("Curves it follows")
    for name in used:
        points = curves.get(_canon(name))
        if points:
            _level_table(view.name_of(name), points)
        else:
            imgui.text_disabled(f"{view.name_of(name)}: not in any Curves row, so the flat value is used.")
    imgui.separator_text("Values")
    for name, value in view.entry.items():
        imgui.bullet_text(f"{humanize(name)}: {value_text(value, view.name_of)}")
    return False, view.entry


register(FEATURE, NpcStatsExtension())
