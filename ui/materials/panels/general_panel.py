"""General tab — iterates GENERAL_FIELDS, calls draw_field() for each."""

from __future__ import annotations

from typing import TYPE_CHECKING

from ..field_registry import GENERAL_FIELDS
from ..widgets import draw_field

if TYPE_CHECKING:
    from ..app import MaterialEditorApp


def draw_general_panel(app: MaterialEditorApp) -> None:
    for field in GENERAL_FIELDS:
        value = app.fields_dict.get(field.attr)
        changed, new_val = draw_field(field, value, app.version, app.fields_dict)
        if changed:
            app.set_field(field.attr, new_val)
