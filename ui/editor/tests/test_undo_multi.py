"""Tests for NIF-aware undo system."""
import pytest
from unittest.mock import MagicMock, patch
from ui.editor.undo import UndoManager
from ui.editor.nif_session import NifSession, NifRegistry
from creation_lib.renderer.scene_renderer import SceneNode
from creation_lib.nif.actions import NifAction


class FakeAction(NifAction):
    """Minimal action for testing."""
    def __init__(self, desc="test"):
        self._desc = desc
        self.executed = False
        self.undone = False

    def execute(self, nif):
        self.executed = True
        return None

    def undo(self, nif):
        self.undone = True

    def description(self):
        return self._desc


def _make_registry():
    reg = NifRegistry()
    for nid, path in [("main", "weapon.nif"), ("child_0", "scope.nif")]:
        nif = MagicMock()
        root = SceneNode(name="root", block_id=-1, nif_id=nid)
        reg.add_session(NifSession(
            nif_id=nid, nif=nif, file_path=path,
            scene_root=root, anim_manager=MagicMock(),
            parent_nif_id=None if nid == "main" else "main",
        ))
    return reg


class TestMultiNifUndo:
    def test_push_and_undo_routes_to_correct_nif(self):
        reg = _make_registry()
        mgr = UndoManager(registry=reg, max_history=50)
        action = FakeAction("modify scope")
        mgr.push("child_0", action)
        desc = mgr.undo()
        assert desc == "modify scope"
        assert action.undone

    def test_global_stack_order(self):
        reg = _make_registry()
        mgr = UndoManager(registry=reg, max_history=50)
        a1 = FakeAction("edit main")
        a2 = FakeAction("edit child")
        mgr.push("main", a1)
        mgr.push("child_0", a2)
        # Undo pops child_0 action first (most recent)
        desc = mgr.undo()
        assert desc == "edit child"
        desc = mgr.undo()
        assert desc == "edit main"

    def test_dirty_flag_set_on_push(self):
        reg = _make_registry()
        mgr = UndoManager(registry=reg, max_history=50)
        assert reg.get_session("child_0").dirty is False
        mgr.push("child_0", FakeAction())
        assert reg.get_session("child_0").dirty is True

    def test_max_history_trims(self):
        reg = _make_registry()
        mgr = UndoManager(registry=reg, max_history=3)
        for i in range(5):
            mgr.push("main", FakeAction(f"a{i}"))
        assert len(mgr._undo_stack) == 3

    def test_filter_by_nif_id(self):
        reg = _make_registry()
        mgr = UndoManager(registry=reg, max_history=50)
        mgr.push("main", FakeAction("m1"))
        mgr.push("child_0", FakeAction("c1"))
        mgr.push("main", FakeAction("m2"))
        mgr.filter_nif(nif_id="child_0")
        # Only main actions remain
        assert len(mgr._undo_stack) == 2
        assert all(nid == "main" for nid, _ in mgr._undo_stack)

    def test_redo(self):
        reg = _make_registry()
        mgr = UndoManager(registry=reg, max_history=50)
        mgr.push("main", FakeAction("edit"))
        mgr.undo()
        assert mgr.can_redo
        desc = mgr.redo()
        assert desc == "edit"
