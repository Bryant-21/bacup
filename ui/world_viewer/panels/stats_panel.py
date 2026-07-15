from __future__ import annotations

from imgui_bundle import imgui


def draw(app) -> None:
    state = app.state
    report = state.stats_report or state.last_report
    if report is not None:
        counts = _report_value(report, "counts", {})
        timings = _report_value(report, "timings_ms", {})
        if isinstance(counts, dict):
            for key, value in counts.items():
                imgui.text(f"{key}: {value}")
        if isinstance(timings, dict):
            for key, value in timings.items():
                imgui.text(f"{key}: {value} ms")

    for warning in state.warnings:
        if isinstance(warning, dict):
            code = warning.get("code", "warning")
            message = warning.get("message", "")
            imgui.text_wrapped(f"{code}: {message}")
        else:
            imgui.text_wrapped(str(warning))


def _report_value(report: object, name: str, default):
    if isinstance(report, dict):
        return report.get(name, default)
    return getattr(report, name, default)
