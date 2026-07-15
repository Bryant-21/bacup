from __future__ import annotations

from dataclasses import asdict, is_dataclass

from imgui_bundle import imgui


def draw(app) -> None:
    state = app.state
    selected = state.selected_instance_id or 0
    changed, value = imgui.input_int("Instance##world_viewer_selected_instance", int(selected))
    if changed:
        app.select_instance(value if value > 0 else None)

    data = report_data(state.selected_report)
    if not data:
        return

    for key in (
        "instance_id",
        "form_key",
        "base_form_key",
        "signature",
        "cell",
        "model_path",
        "source_plugin",
        "layer_form_key",
        "static_collection_parent",
    ):
        value = data.get(key)
        if value not in (None, ""):
            imgui.text(f"{key}: {value}")


def report_data(report: object | None) -> dict:
    if report is None:
        return {}
    if isinstance(report, dict):
        data = report.get("data", {})
        return data if isinstance(data, dict) else {}
    data = getattr(report, "data", {})
    if is_dataclass(data):
        return asdict(data)
    return data if isinstance(data, dict) else {}

    for key in ("position", "rotation_degrees", "scale"):
        value = data.get(key)
        if value not in (None, ""):
            imgui.text(f"{key}: {value}")
