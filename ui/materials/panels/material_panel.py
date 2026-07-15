"""Material tab — iterates MATERIAL_FIELDS, calls draw_field() for each.

Only drawn when file_type == "bgsm".
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from ..field_registry import MATERIAL_FIELDS
from ..widgets import draw_field

if TYPE_CHECKING:
    from ..app import MaterialEditorApp


def draw_material_panel(app: MaterialEditorApp) -> None:
    for field in MATERIAL_FIELDS:
        value = app.fields_dict.get(field.attr)
        changed, new_val = draw_field(field, value, app.version, app.fields_dict)
        if changed:
            app.set_field(field.attr, new_val)
