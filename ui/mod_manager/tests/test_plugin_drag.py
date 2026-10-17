"""One frozen gesture moves complete draft blocks and rejects stale context."""
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.plugin_drag import begin, drop


def draft():
    panes = SimpleNamespace(manager=SimpleNamespace(profile_id="local", busy=False, runtime_active=False),
                            catalog=object(), order=["Core.esm", "A.esp", "Hidden.esp", "B.esp", "C.esp"],
                            active={"core.esm", "a.esp"}, locked={}, selected_plugins={"A.esp", "B.esp"})
    def move(chosen, destination):
        moving = [name for name in panes.order if name in chosen]
        remaining = [name for name in panes.order if name not in chosen]
        panes.order = remaining[:destination] + moving + remaining[destination:]
    panes.move_plugins = move
    return panes


def test_filtered_noncontiguous_block_preserves_order_and_destination_slot():
    panes = draft()
    gesture = begin(panes, "B.esp")
    drop(panes, gesture, "C.esp")
    assert panes.order == ["Core.esm", "Hidden.esp", "A.esp", "B.esp", "C.esp"]
    drop(panes, begin(panes, "A.esp"), "Hidden.esp")
    assert panes.order == ["Core.esm", "A.esp", "B.esp", "Hidden.esp", "C.esp"]
    before = list(panes.order)
    drop(panes, begin(panes, "A.esp"), "B.esp")
    assert panes.order == before
    # Beginning from an unselected row does not move a stale, hidden selection.
    assert begin(panes, "C.esp").selected == ("C.esp",)


@pytest.mark.parametrize("change", ["profile", "catalog", "order", "active", "locked", "busy", "runtime"])
def test_mid_gesture_context_changes_are_rejected_without_replacing_draft(change):
    panes = draft()
    gesture = begin(panes, "A.esp")
    if change == "profile":
        panes.manager.profile_id = "other"
    elif change == "catalog":
        panes.catalog = object()
    elif change == "order":
        panes.order.reverse()
    elif change == "active":
        panes.active.add("b.esp")
    elif change == "locked":
        panes.locked["hidden.esp"] = ("Hidden.esp", 2)
    else:
        setattr(panes.manager, "runtime_active" if change == "runtime" else "busy", True)
    before = list(panes.order)
    with pytest.raises(ManagerError):
        drop(panes, gesture, "C.esp")
    assert panes.order == before


def test_disappeared_selection_and_target_are_rejected():
    panes = draft()
    panes.selected_plugins.add("Gone.esp")
    with pytest.raises(ManagerError, match="Selected plugins changed"):
        begin(panes, "A.esp")
    panes.selected_plugins.remove("Gone.esp")
    gesture = begin(panes, "A.esp")
    with pytest.raises(ManagerError, match="target plugin changed"):
        drop(panes, gesture, "Gone.esp")
