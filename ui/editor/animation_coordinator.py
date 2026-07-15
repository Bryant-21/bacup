"""Cross-NIF animation coordination.

Manages playback across all NifSessions — sequences with matching names
play simultaneously on all NIFs that contain them.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .nif_session import NifRegistry

from .particles.runtime import PARTICLE_PREVIEW_SEQUENCE

_log = logging.getLogger("nif_editor.anim_coord")


def _particle_runtime(session):
    runtime = getattr(session, "particle_runtime", None)
    if runtime is not None and getattr(runtime, "has_particles", False):
        return runtime
    return None


class AnimationCoordinator:
    """Coordinates animation playback across multiple NIF sessions."""

    def __init__(self, registry: NifRegistry):
        self._registry = registry
        self.current_sequence_name: str | None = None

    def play(self, sequence_name: str):
        """Play a sequence on animations and particle runtimes."""
        self.current_sequence_name = sequence_name
        for session in self._registry.all_sessions():
            if (
                sequence_name != PARTICLE_PREVIEW_SEQUENCE
                and session.anim_manager.has_sequence(sequence_name)
            ):
                session.anim_manager.play(sequence_name)
            runtime = _particle_runtime(session)
            if runtime is not None:
                runtime.play(sequence_name)

    def select(self, sequence_name: str):
        """Select a sequence on all NIFs that have it without starting playback."""
        self.current_sequence_name = sequence_name
        for session in self._registry.all_sessions():
            if (
                sequence_name == PARTICLE_PREVIEW_SEQUENCE
                or session.anim_manager.has_sequence(sequence_name)
            ):
                session.anim_manager.select_sequence(sequence_name)

    def pause(self):
        for session in self._registry.all_sessions():
            session.anim_manager.pause()
            runtime = _particle_runtime(session)
            if runtime is not None:
                runtime.pause()

    def resume(self):
        for session in self._registry.all_sessions():
            session.anim_manager.resume()
            runtime = _particle_runtime(session)
            if runtime is not None:
                runtime.resume()

    def stop(self):
        self.current_sequence_name = None
        for session in self._registry.all_sessions():
            session.anim_manager.stop()
            runtime = _particle_runtime(session)
            if runtime is not None:
                runtime.stop()

    def set_time(self, t: float):
        """Set playback time on all active animations."""
        for session in self._registry.all_sessions():
            session.anim_manager.set_time(t, session.scene_root)
            runtime = _particle_runtime(session)
            if runtime is not None:
                runtime.set_time(t)

    def update(self, dt: float):
        """Advance all active animations (called each frame)."""
        for session in self._registry.all_sessions():
            session.anim_manager.update(dt, session.scene_root)
            runtime = _particle_runtime(session)
            if runtime is not None:
                runtime.update(dt)

    def get_all_sequences(self) -> dict[str, list[str]]:
        """Return {sequence_name: [nif_ids that have it]}."""
        result: dict[str, list[str]] = {}
        particle_sessions: list[str] = []
        for session in self._registry.all_sessions():
            for name in session.anim_manager.get_sequences():
                result.setdefault(name, []).append(session.nif_id)
            runtime = _particle_runtime(session)
            if runtime is not None:
                particle_sessions.append(session.nif_id)
        if particle_sessions:
            result.setdefault(PARTICLE_PREVIEW_SEQUENCE, particle_sessions)
        return result
