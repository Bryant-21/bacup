from __future__ import annotations

from ui.previs.state import PrevisState


def _report():
    finding = {"action": "rebuild_precombines", "code": "game_disables_precombines", "message": "m", "blamed": "Mod.esp", "record": None}
    return {
        "summary": {},
        "cells": [
            {"form_key": "000800:Fallout4.esm", "editor_id": "Diner", "world": None, "winning_plugin": "Mod.esp",
             "action": "rebuild_precombines", "findings": [finding]},
            {"form_key": "000900:Fallout4.esm", "editor_id": "Shack", "world": None, "winning_plugin": "Fallout4.esm",
             "action": "warn", "findings": [{**finding, "action": "warn", "code": "previs_disabled", "blamed": "Other.esp"}]},
            {"form_key": "000A00:Fallout4.esm", "editor_id": None, "world": "Commonwealth", "winning_plugin": "Mod.esp",
             "action": None, "findings": []},
        ],
        "clusters": [{"form_key": "000800:Fallout4.esm", "action": "rebuild_previs", "findings": []}],
    }


def test_filters_by_action_overrides_and_search():
    state = PrevisState(report=_report())
    assert [c["form_key"] for c in state.visible_cells()] == ["000800:Fallout4.esm", "000900:Fallout4.esm"]
    state.action_filter = {"warn"}
    assert [c["editor_id"] for c in state.visible_cells()] == ["Shack"]
    state.action_filter = {"rebuild_precombines", "warn"}
    state.show_overridden_only = True
    assert len(state.visible_cells()) == 3
    state.search = "other.esp"
    assert [c["editor_id"] for c in state.visible_cells()] == ["Shack"]
    state.search = ""
    assert state.visible_clusters() == []
    state.action_filter.add("rebuild_previs")
    assert len(state.visible_clusters()) == 1


def test_workspace_registers_and_roundtrips_settings():
    from ui.previs.workspace import PrevisWorkspace
    from ui.toolkit.workspaces import _WORKSPACE_SPECS

    assert ("previs", "ui.previs.workspace", "PrevisWorkspace") in _WORKSPACE_SPECS

    class _Settings:
        def get_game_paths(self, game):
            return {"root_dir": "C:/FO4"}

    workspace = PrevisWorkspace(toolkit_settings=_Settings())
    assert set(workspace.get_settings_defaults()) == set(workspace.collect_settings())
    workspace.apply_settings({"source_kind": "plugin", "plugin_path": "C:/Data/Mod.esp", "unprecombined_threshold": 9})
    collected = workspace.collect_settings()
    assert (collected["source_kind"], collected["plugin_path"], collected["unprecombined_threshold"]) == ("plugin", "C:/Data/Mod.esp", 9)
    assert str(workspace.app.game_root()).replace("\\", "/") == "C:/FO4"
