"""Generic undo/redo stack for mesh workspaces.

Extracted from WeightPainterApp's inline undo logic. Each entry
is a (label, snapshot) tuple where snapshot is an opaque object
that the workspace subclass knows how to restore.
"""
from __future__ import annotations

from typing import Generic, TypeVar

T = TypeVar("T")


class UndoStack(Generic[T]):
    """Fixed-capacity undo/redo stack."""

    def __init__(self, max_entries: int = 50):
        self._max = max_entries
        self._undo: list[tuple[str, T]] = []
        self._redo: list[tuple[str, T]] = []

    def push(self, label: str, snapshot: T) -> None:
        """Push a snapshot onto the undo stack. Clears redo."""
        self._undo.append((label, snapshot))
        if len(self._undo) > self._max:
            self._undo.pop(0)
        self._redo.clear()

    def undo(self) -> tuple[str, T] | None:
        """Pop and return the most recent snapshot, or None."""
        if not self._undo:
            return None
        entry = self._undo.pop()
        self._redo.append(entry)
        return entry

    def redo(self) -> tuple[str, T] | None:
        """Re-apply the most recently undone snapshot, or None."""
        if not self._redo:
            return None
        entry = self._redo.pop()
        self._undo.append(entry)
        return entry

    def clear(self) -> None:
        self._undo.clear()
        self._redo.clear()

    @property
    def can_undo(self) -> bool:
        return len(self._undo) > 0

    @property
    def can_redo(self) -> bool:
        return len(self._redo) > 0

    @property
    def undo_label(self) -> str:
        return self._undo[-1][0] if self._undo else ""

    @property
    def redo_label(self) -> str:
        return self._redo[-1][0] if self._redo else ""
