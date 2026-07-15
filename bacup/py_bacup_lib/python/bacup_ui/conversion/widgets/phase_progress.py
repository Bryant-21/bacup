"""Pipeline phase progress table widget."""
from __future__ import annotations

import os

from imgui_bundle import imgui


def phase_bar_state(phase: dict) -> tuple[str, float]:
    """Decide how a phase's progress bar should render.

    Batch phases run as a single native call and never set item counts, so a
    completed phase must still show a full bar and a running one an
    indeterminate sweep rather than an empty cell.
    """
    status = str(phase.get("status") or "pending")
    if status == "completed":
        return ("complete", 1.0)
    if status == "running":
        try:
            total = int(phase.get("total_items", 0) or 0)
            completed = int(phase.get("completed_items", 0) or 0)
        except (TypeError, ValueError):
            total, completed = 0, 0
        if total > 0:
            return ("determinate", max(0.0, min(completed / total, 1.0)))
        return ("indeterminate", 0.0)
    return ("none", 0.0)


def draw_phase_progress(
    namespace: str,
    phase_names: list[str] | list[tuple[str, str]],
    phases: list[dict],
) -> None:
    """Render a 4-column phase progress table."""
    col_flags = imgui.TableColumnFlags_.no_resize.value
    tbl_flags = (
        imgui.TableFlags_.sizing_fixed_fit.value
        | imgui.TableFlags_.pad_outer_x.value
    )

    if imgui.begin_table(f"##pipeline{namespace}", 4, tbl_flags):
        imgui.table_setup_column("Status", col_flags, 36)
        imgui.table_setup_column(
            "Phase", col_flags | imgui.TableColumnFlags_.width_stretch.value
        )
        imgui.table_setup_column("Bar", col_flags, 180)
        imgui.table_setup_column(
            "Item", col_flags | imgui.TableColumnFlags_.width_stretch.value
        )

        rows = [
            row if isinstance(row, tuple) else (str(row), str(row))
            for row in phase_names
        ]
        seen = {key for key, _label in rows}
        for phase in phases:
            key = str(phase.get("ui_key") or phase.get("phase") or "")
            if not key or key in seen:
                continue
            rows.append((key, str(phase.get("phase_name") or key)))
            seen.add(key)

        for key, phase_name in rows:
            phase_data = next((p for p in phases if p.get("ui_key") == key), None)
            display_name = (
                phase_data.get("phase_name", phase_name) if phase_data else phase_name
            )
            imgui.table_next_row()

            imgui.table_set_column_index(0)
            if phase_data:
                status = phase_data.get("status", "pending")
                if status == "completed":
                    imgui.push_style_color(
                        imgui.Col_.text, imgui.ImVec4(0.3, 1.0, 0.3, 1.0)
                    )
                    imgui.text("[OK]")
                    imgui.pop_style_color()
                elif status == "error":
                    imgui.push_style_color(
                        imgui.Col_.text, imgui.ImVec4(1.0, 0.3, 0.3, 1.0)
                    )
                    imgui.text("[ERR]")
                    imgui.pop_style_color()
                elif status == "running":
                    imgui.text("[...]")
                else:
                    imgui.text_disabled("[ ]")
            else:
                imgui.text_disabled("[ ]")

            imgui.table_set_column_index(1)
            if phase_data and phase_data.get("status") in ("running", "completed", "error"):
                imgui.text(display_name)
            else:
                imgui.text_disabled(display_name)

            if not phase_data:
                continue

            status = phase_data.get("status", "pending")
            total = phase_data.get("total_items", 0)
            completed = phase_data.get("completed_items", 0)
            current = phase_data.get("current_item", "")

            imgui.table_set_column_index(2)
            bar_mode, bar_fraction = phase_bar_state(phase_data)
            if bar_mode == "complete":
                imgui.push_item_width(-1)
                overlay = f"{total}/{total}" if total > 0 else "done"
                imgui.progress_bar(1.0, imgui.ImVec2(-1, 0), overlay)
                imgui.pop_item_width()
            elif bar_mode == "determinate":
                imgui.push_item_width(-1)
                imgui.progress_bar(bar_fraction, imgui.ImVec2(-1, 0), f"{completed}/{total}")
                imgui.pop_item_width()
            elif bar_mode == "indeterminate":
                imgui.push_item_width(-1)
                imgui.progress_bar(
                    -1.0 * imgui.get_time(), imgui.ImVec2(-1, 0), "working..."
                )
                imgui.pop_item_width()

            imgui.table_set_column_index(3)
            if current and status == "running":
                label = (
                    os.path.basename(current)
                    if os.path.sep in current or "/" in current
                    else current
                )
                imgui.push_style_color(
                    imgui.Col_.text, imgui.ImVec4(0.85, 0.85, 0.85, 1.0)
                )
                imgui.text(label)
                if imgui.is_item_hovered():
                    imgui.set_tooltip(current)
                imgui.pop_style_color()

        imgui.end_table()
