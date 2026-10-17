"""FoodSpoilage: how long a food lasts at several levels and in cold storage."""

from __future__ import annotations

from ui.tools.catalog_editor.features import FeatureExtension, RemoveWording, ResultView, register
from ui.tools.catalog_editor.model import REMOVED, Problem

FEATURE = "FoodSpoilage"
SAMPLE_LEVELS = (1, 10, 25, 50, 100)


def curve_at(points: list[dict], x: float) -> float:
    """Tales' CurveAt: flat before the first and after the last point, linear between."""
    if not points:
        return 0.0
    if x <= points[0]["x"]:
        return float(points[0]["y"])
    for left, right in zip(points, points[1:]):
        if x <= right["x"]:
            return left["y"] + (right["y"] - left["y"]) * (x - left["x"]) / (right["x"] - left["x"])
    return float(points[-1]["y"])


def minutes_to_spoil(points: list[dict], level: float, points_per_minute: float, rate: float = 1.0) -> float | None:
    """Minutes of play until fresh food at `level` spoils; None when it never does."""
    speed = points_per_minute * rate
    return curve_at(points, level) / speed if speed > 0 else None


def duration(minutes: float | None) -> str:
    if minutes is None:
        return "never"
    if minutes < 60:
        return f"{minutes:.0f} min"
    hours = minutes / 60
    return f"{hours:.1f} h" if hours < 48 else f"{hours / 24:.1f} days"


def strictly_increasing(points: list[dict]) -> bool:
    return all(b["x"] > a["x"] for a, b in zip(points, points[1:]))


class FoodSpoilageExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != FEATURE or "foods" not in doc.sections():
            return []
        return [
            Problem(doc.name, "foods", row.key, "The curve's levels (x) must increase from point to point.", blocking=False)
            for row in doc.rows("foods")
            if row.status != REMOVED and isinstance(row.entry, dict) and not strictly_increasing(row.entry.get("curve") or [])
        ]

    def result_view(self, doc, section):
        return ResultView("Spoilage over time", draw_spoilage) if section == "foods" else None

    def sentence(self, view):
        if view.section != "foods" or not isinstance(view.entry, dict):
            return ""
        points = view.entry.get("curve") or []
        if not points:
            return "This food has no curve of its own, so it spoils like its category's default (see Settings)."
        speed = _speed(view.doc)
        return (f"Fresh at level 25, it spoils after {duration(minutes_to_spoil(points, 25, speed))} of play; "
                f"at level 100 after {duration(minutes_to_spoil(points, 100, speed))}.")

    def remove_wording(self, doc, section):
        if section != "foods":
            return None
        return RemoveWording("Use the category default", "Your plugin removes this food's own curve, so it spoils like "
                             "its category's default from Settings.", "Removed: the food spoils like its category's default.")


def _speed(doc) -> float:
    settings = {name: value for name, value, _, _ in doc.settings()}
    return float(settings.get("points_per_minute", 1.0) or 0)


def draw_spoilage(view):
    """How long the food lasts at each level, fresh and in each kind of cold storage."""
    import numpy as np
    from imgui_bundle import imgui

    entry = view.entry if isinstance(view.entry, dict) else {}
    points = entry.get("curve") or []
    if not points:
        imgui.text_wrapped("This food has no curve of its own; it uses its category's default from Settings.")
        return False, view.entry
    settings = {name: value for name, value, _, _ in view.doc.settings()}
    speed = _speed(view.doc)
    storage = settings.get("storage") or {}
    imgui.text_disabled(f"Minutes of play until fresh food spoils, by level. Ageing speed {speed:g} point(s) per minute.")
    minutes = [minutes_to_spoil(points, level, speed) or 0.0 for level in range(1, 101)]
    imgui.plot_lines("##spoilage", np.array(minutes, dtype=np.float32), overlay_text="level 1 to 100",
                     graph_size=imgui.ImVec2(-1, max(80.0, imgui.get_content_region_avail().x * 0.2)))
    kinds = list(storage)
    flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h
    if imgui.begin_table("##spoil_table", 2 + len(kinds), flags):
        imgui.table_setup_column("Level")
        imgui.table_setup_column("Fresh")
        for kind in kinds:
            imgui.table_setup_column(f"In {kind}")
        imgui.table_headers_row()
        for level in SAMPLE_LEVELS:
            imgui.table_next_row()
            imgui.table_next_column()
            imgui.text(str(level))
            imgui.table_next_column()
            imgui.text(duration(minutes_to_spoil(points, level, speed)))
            for kind in kinds:
                imgui.table_next_column()
                rate = float((storage[kind] or {}).get("rate", 1.0))
                imgui.text(duration(minutes_to_spoil(points, level, speed, rate)))
        imgui.end_table()
    return False, view.entry


register(FEATURE, FoodSpoilageExtension())
