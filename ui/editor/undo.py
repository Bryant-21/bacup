"""Undo/redo engine for the NIF editor.

NIF-aware global stack: each action is stored as (nif_id, action).
Undo/redo resolves the target NIF dynamically via the registry.
"""
import logging

from creation_lib.nif.actions import (
    NifAction, SetFieldAction, SnapshotAction, CompositeAction,
    VertexEditAction, OperationResult,
)

_log = logging.getLogger("nif_editor.undo")

# Re-export for backward compatibility with any remaining imports
__all__ = [
    "NifAction", "SetFieldAction", "SnapshotAction", "CompositeAction",
    "VertexEditAction", "OperationResult", "UndoManager",
]


class MultiNifCompositeAction(NifAction):
    """Wraps actions targeting multiple NIFs into one undo step."""

    def __init__(self, actions: list[tuple[str, NifAction]]):
        self._actions = actions  # [(nif_id, action), ...]

    def execute(self, nif):
        # nif param ignored — each sub-action resolves its own NIF
        pass  # Actions already executed before push

    def undo(self, nif):
        # nif param ignored — registry lookup happens in UndoManager
        pass

    def description(self):
        return f"batch ({len(self._actions)} operations)"


class UndoManager:
    """NIF-aware undo/redo with global stack.

    Each action is stored as (nif_id, action). Undo/redo resolves the
    target NIF dynamically via the registry.
    """

    def __init__(self, registry=None, max_history: int = 50):
        self._registry = registry
        self._undo_stack: list[tuple[str, NifAction]] = []
        self._redo_stack: list[tuple[str, NifAction]] = []
        self._max_history = max_history

    def set_registry(self, registry):
        """Update the registry reference."""
        self._registry = registry

    def push(self, nif_id: str, action: NifAction):
        """Push an action that has already been executed."""
        self._undo_stack.append((nif_id, action))
        if len(self._undo_stack) > self._max_history:
            self._undo_stack.pop(0)
        self._redo_stack.clear()
        if self._registry:
            try:
                self._registry.get_session(nif_id).dirty = True
            except KeyError:
                pass
        _log.info("Pushed: %s [nif=%s] (stack=%d, can_undo=%s)",
                  action.description(), nif_id, len(self._undo_stack), self.can_undo)

    def undo(self) -> str | None:
        """Undo the last action (global stack). Returns description or None."""
        _log.info("undo() called: stack_size=%d, has_registry=%s",
                  len(self._undo_stack), self._registry is not None)
        if not self._undo_stack or not self._registry:
            _log.info("undo() skipped: stack_empty=%s, no_registry=%s",
                      not self._undo_stack, not self._registry)
            return None
        nif_id, action = self._undo_stack.pop()
        if isinstance(action, MultiNifCompositeAction):
            for sub_nid, sub_action in reversed(action._actions):
                try:
                    sub_nif = self._registry.get_session(sub_nid).nif
                    sub_action.undo(sub_nif)
                except KeyError:
                    _log.warning("Undo: session %s gone, skipping", sub_nid)
        else:
            try:
                nif = self._registry.get_session(nif_id).nif
            except KeyError:
                _log.warning("Undo: session %s no longer exists, skipping", nif_id)
                return None
            action.undo(nif)
        self._redo_stack.append((nif_id, action))
        _log.debug("Undo: %s [%s]", action.description(), nif_id)
        return action.description()

    def redo(self) -> str | None:
        """Redo the last undone action. Returns description or None."""
        if not self._redo_stack or not self._registry:
            return None
        nif_id, action = self._redo_stack.pop()
        if isinstance(action, MultiNifCompositeAction):
            for sub_nid, sub_action in action._actions:
                try:
                    sub_nif = self._registry.get_session(sub_nid).nif
                    sub_action.execute(sub_nif)
                except KeyError:
                    _log.warning("Redo: session %s gone, skipping", sub_nid)
        else:
            try:
                nif = self._registry.get_session(nif_id).nif
            except KeyError:
                _log.warning("Redo: session %s no longer exists, skipping", nif_id)
                return None
            action.execute(nif)
        self._undo_stack.append((nif_id, action))
        _log.debug("Redo: %s [%s]", action.description(), nif_id)
        return action.description()

    def filter_nif(self, nif_id: str):
        """Remove all actions for a given nif_id (used on detach)."""
        self._undo_stack = [(n, a) for n, a in self._undo_stack if n != nif_id]
        self._redo_stack = [(n, a) for n, a in self._redo_stack if n != nif_id]

    def clear(self):
        self._undo_stack.clear()
        self._redo_stack.clear()

    @property
    def can_undo(self) -> bool:
        return bool(self._undo_stack)

    @property
    def can_redo(self) -> bool:
        return bool(self._redo_stack)

    @property
    def undo_description(self) -> str:
        if self._undo_stack:
            return self._undo_stack[-1][1].description()
        return ""

    @property
    def redo_description(self) -> str:
        if self._redo_stack:
            return self._redo_stack[-1][1].description()
        return ""

    def push_composite(self, actions: list[tuple[str, NifAction]]):
        """Push a cross-NIF composite as a single undo step.

        Used by MCP batch() when commands target different NIFs.
        Undo/redo applies all sub-actions together.
        """
        if not actions:
            return
        composite = MultiNifCompositeAction(actions)
        # Use the first action's nif_id as the "primary" for stack entry
        primary_nif_id = actions[0][0]
        self._undo_stack.append((primary_nif_id, composite))
        if len(self._undo_stack) > self._max_history:
            self._undo_stack.pop(0)
        self._redo_stack.clear()
        # Mark all affected sessions dirty
        if self._registry:
            for nid, _ in actions:
                try:
                    self._registry.get_session(nid).dirty = True
                except KeyError:
                    pass
