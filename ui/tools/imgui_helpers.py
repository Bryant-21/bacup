"""Shared ImGui helper widgets for tool windows."""

from __future__ import annotations

from imgui_bundle import imgui

from creation_lib.ui.widgets.pick_folder import pick_file, pick_save_file

# Fixed width of the label column in all form tables (pixels).
LABEL_COL_W: int = 160


__all__ = [
    "pick_file",
    "pick_save_file",
    "LABEL_COL_W",
    "begin_form",
    "end_form",
    "draw_path_row",
    "draw_int_field",
    "draw_float_field",
    "draw_text_field",
    "draw_combo_field",
    "draw_run_cancel_buttons",
]


# ---------------------------------------------------------------------------
# Form table helpers
# ---------------------------------------------------------------------------

def begin_form(table_id: str, label_width: int = LABEL_COL_W) -> bool:
    """Start a 2-column form table (label col fixed, value col stretch).

    Call end_form() after the last field row — only when this returns True.
    """
    flags = imgui.TableFlags_.sizing_fixed_fit | imgui.TableFlags_.no_borders_in_body
    if imgui.begin_table(table_id, 2, flags):
        imgui.table_setup_column("##label", imgui.TableColumnFlags_.width_fixed, label_width)
        imgui.table_setup_column("##value", imgui.TableColumnFlags_.width_stretch)
        return True
    return False


def end_form() -> None:
    """End the form table started by begin_form()."""
    imgui.end_table()


def _form_row_label(label: str) -> None:
    """Advance to the next table row and render label in column 0, then move to column 1."""
    imgui.table_next_row()
    imgui.table_set_column_index(0)
    imgui.align_text_to_frame_padding()
    imgui.text(label)
    imgui.table_set_column_index(1)


def draw_path_row(label: str, path: str, btn_label: str = "Browse...") -> tuple[str, bool]:
    """Draw label + read-only path input + browse button as a form table row.

    Must be called inside a begin_form() block.
    Returns (current_path, button_clicked).
    """
    _form_row_label(label)
    btn_w = imgui.calc_text_size(btn_label).x + imgui.get_style().frame_padding.x * 2 + 8
    avail = imgui.get_content_region_avail().x
    imgui.set_next_item_width(avail - btn_w - imgui.get_style().item_spacing.x)
    _, path = imgui.input_text(f"##{label}_path", path, imgui.InputTextFlags_.read_only)
    imgui.same_line()
    clicked = imgui.button(f"{btn_label}##{label}_btn")
    return path, clicked


def draw_int_field(label: str, value: int, step: int = 1, step_fast: int = 10,
                   min_val: int | None = None, max_val: int | None = None) -> tuple[bool, int]:
    """Draw a labeled integer input as a form table row."""
    _form_row_label(label)
    imgui.set_next_item_width(-1)
    changed, new_val = imgui.input_int(f"##{label}", value, step, step_fast)
    if min_val is not None:
        new_val = max(min_val, new_val)
    if max_val is not None:
        new_val = min(max_val, new_val)
    return changed, new_val


def draw_float_field(label: str, value: float, step: float = 0.1, step_fast: float = 1.0,
                     fmt: str = "%.2f", min_val: float | None = None,
                     max_val: float | None = None) -> tuple[bool, float]:
    """Draw a labeled float input as a form table row."""
    _form_row_label(label)
    imgui.set_next_item_width(-1)
    changed, new_val = imgui.input_float(f"##{label}", value, step, step_fast, fmt)
    if min_val is not None:
        new_val = max(min_val, new_val)
    if max_val is not None:
        new_val = min(max_val, new_val)
    return changed, new_val


def draw_text_field(label: str, value: str) -> tuple[bool, str]:
    """Draw a labeled text input as a form table row."""
    _form_row_label(label)
    imgui.set_next_item_width(-1)
    changed, new_val = imgui.input_text(f"##{label}", value)
    return changed, new_val


def draw_combo_field(label: str, items: list[str], idx: int) -> tuple[bool, int]:
    """Draw a labeled combo box as a form table row."""
    _form_row_label(label)
    imgui.set_next_item_width(-1)
    changed, new_idx = imgui.combo(f"##{label}", idx, items)
    return changed, new_idx


# ---------------------------------------------------------------------------
# Button helpers
# ---------------------------------------------------------------------------

def draw_run_cancel_buttons(running: bool, can_run: bool = True) -> tuple[bool, bool]:
    """Draw Run and Cancel buttons. Returns (run_clicked, cancel_clicked)."""
    run_clicked = False
    cancel_clicked = False

    if running:
        imgui.begin_disabled()
    if imgui.button("Run", imgui.ImVec2(120, 0)) and can_run:
        run_clicked = True
    if running:
        imgui.end_disabled()

    if running:
        imgui.same_line()
        if imgui.button("Cancel", imgui.ImVec2(120, 0)):
            cancel_clicked = True

    return run_clicked, cancel_clicked
