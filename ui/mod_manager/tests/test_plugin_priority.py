from dataclasses import replace
import threading
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager import plugin_priority as priority
from ui.mod_manager.panes_ui import ManagerPanes
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.plugin_state import Catalog, Plugin


def draft():
    names = ("Core.esm", "Alpha.esp", "Hidden.esp", "Beta.esp", "Omega.esp")
    plugins = tuple(Plugin(name, "fixture/" + name, "generated", "", True, index, index == 0,
                           {"flags": {"esm": index == 0}}, () if index == 0 else ("Core.esm",)) for index, name in enumerate(names))
    catalog = Catalog("local", "generated", "fo4", plugins)
    manager = SimpleNamespace(library=object(), profile_id="local", plugin_catalog=catalog, busy=False, runtime_active=False)
    panes = ManagerPanes(SimpleNamespace(manager=manager))
    panes.catalog, panes.order, panes.active = catalog, list(names), {name.casefold() for name in names}
    return panes


def test_exact_priority_moves_in_full_draft_preserves_selection_and_only_invalidates_review():
    panes = draft()
    panes.query, panes.selected_plugins, panes.plan = "Omega", {"Alpha.esp", "Omega.esp"}, object()
    priority.apply(panes, priority.begin(panes, "Omega.esp"), "1")
    assert panes.order == ["Core.esm", "Omega.esp", "Alpha.esp", "Hidden.esp", "Beta.esp"]
    assert panes.selected_plugins == {"Alpha.esp", "Omega.esp"} and panes.plan is None
    assert panes._scroll_plugin == "Omega.esp"
    priority.apply(panes, priority.begin(panes, "Omega.esp"), "4")
    assert panes.order == [p.name for p in panes.catalog.plugins]


@pytest.mark.parametrize("value", ["-1", "5", "99999999999", "1.1", "one", "١", ""])
def test_invalid_priority_is_rejected_without_changing_draft(value):
    panes = draft()
    before, reviewed = list(panes.order), object()
    panes.plan = reviewed
    with pytest.raises(ManagerError):
        priority.apply(panes, priority.begin(panes, "Alpha.esp"), value)
    assert panes.order == before and panes.plan is reviewed


@pytest.mark.parametrize("change", ["library", "profile", "catalog", "manager-catalog", "details", "order", "active", "locked", "busy", "runtime"])
def test_frozen_edit_rejects_changed_context(change):
    panes = draft()
    plan = priority.begin(panes, "Alpha.esp")
    if change == "library":
        panes.manager.library = object()
    elif change == "profile":
        panes.manager.profile_id = "other"
    elif change == "catalog":
        panes.catalog = replace(panes.catalog)
    elif change == "manager-catalog":
        panes.manager.plugin_catalog = replace(panes.catalog)
    elif change == "details":
        panes.catalog.plugins[-1].header["flags"]["esm"] = True
    elif change == "order":
        panes.order.reverse()
    elif change == "active":
        panes.active.remove("omega.esp")
    elif change == "locked":
        panes.locked["omega.esp"] = ("Omega.esp", 4)
    else:
        setattr(panes.manager, "busy" if change == "busy" else "runtime_active", True)
    before = list(panes.order)
    with pytest.raises(ManagerError):
        priority.apply(panes, plan, "2")
    assert panes.order == before


@pytest.mark.parametrize("constraint", ["core", "selected-lock", "master", "unknown-details"])
def test_complete_plugin_review_guards_are_checked_before_draft_replacement(constraint):
    panes = draft()
    name, value = "Omega.esp", "1"
    if constraint == "core":
        name = "Core.esm"
    elif constraint == "selected-lock":
        panes.locked["omega.esp"] = ("Omega.esp", 4)
    else:
        plugins = list(panes.catalog.plugins)
        plugins[-1] = replace(plugins[-1], masters=("Beta.esp",) if constraint == "master" else None)
        panes.catalog = panes.manager.plugin_catalog = replace(panes.catalog, plugins=tuple(plugins))
    before = list(panes.order)
    with pytest.raises(ManagerError):
        priority.apply(panes, priority.begin(panes, name), value)
    assert panes.order == before


def test_move_shifts_around_absolute_locks_with_stable_free_order():
    panes = draft()
    panes.locked["alpha.esp"] = ("Alpha.esp", 1)
    priority.apply(panes, priority.begin(panes, "Omega.esp"), "1")
    assert panes.order == ["Core.esm", "Alpha.esp", "Omega.esp", "Hidden.esp", "Beta.esp"]
    panes.move_plugins({"Hidden.esp", "Beta.esp"}, 1)
    assert panes.order == ["Core.esm", "Alpha.esp", "Hidden.esp", "Beta.esp", "Omega.esp"]


@pytest.mark.parametrize("outcome", ["cancelled", "failed", "late-change"])
def test_fault_cancel_and_final_context_change_never_replace_reviewed_draft(monkeypatch, outcome):
    panes, cancel = draft(), threading.Event()
    before, plan = list(panes.order), priority.begin(panes, "Omega.esp")
    def stop():
        if outcome == "cancelled":
            cancel.set()
        elif outcome == "failed":
            raise OSError("generated draft replacement failure")
        else:
            panes.active.remove("omega.esp")
    monkeypatch.setattr(priority, "_checkpoint", stop)
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError if outcome == "failed" else ManagerError):
        priority.apply(panes, plan, "1", cancel)
    assert panes.order == before
