"""NifSession, NifRegistry, and AttachmentNode for multi-NIF editing."""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from creation_lib.renderer.scene_renderer import SceneNode


@dataclass
class NifSession:
    """One per open NIF file — isolates file handle, metadata, and animation state."""
    nif_id: str
    nif: Any  # NifFile
    file_path: str
    scene_root: SceneNode
    anim_manager: Any  # AnimationManager
    particle_models: list[Any] = field(default_factory=list)
    particle_runtime: Any = None
    dirty: bool = False
    parent_nif_id: str | None = None
    attachment_point: str | None = None
    attachment_node: SceneNode | None = None
    hidden_block_ids: set[int] = field(default_factory=set)
    game_profile: Any = None  # GameProfile from creation_lib.core.game_profiles
    cross_game_mismatch: bool = False  # True when child game != parent game
    read_only: bool = False


@dataclass
class AttachmentNode(SceneNode):
    """Synthetic scene node bridging a parent connect point to a child NIF subtree.

    Never serialized to any NIF file. The renderer skips drawing it but applies
    its transform to children.
    """
    parent_nif_id: str = ""
    child_nif_id: str = ""
    connect_point_name: str = ""
    is_attachment: bool = True


class NifRegistry:
    """Manages all open NifSessions. Lives on app."""

    def __init__(self):
        self.sessions: dict[str, NifSession] = {}
        self.main_id: str = "main"
        self.active_id: str = "main"

    @property
    def active_session(self) -> NifSession:
        return self.sessions[self.active_id]

    @property
    def has_multiple_nifs(self) -> bool:
        return len(self.sessions) > 1

    def add_session(self, session: NifSession) -> None:
        # Cross-game attachment: flag mismatch instead of blocking
        if session.parent_nif_id and session.parent_nif_id in self.sessions:
            parent = self.sessions[session.parent_nif_id]
            if (
                parent.game_profile is not None
                and session.game_profile is not None
                and parent.game_profile.id != session.game_profile.id
            ):
                import logging
                logging.getLogger("nif_editor.session").warning(
                    "Cross-game attachment: parent=%s (%s), child=%s (%s)",
                    parent.game_profile.display_name, parent.nif_id,
                    session.game_profile.display_name, session.nif_id,
                )
                session.cross_game_mismatch = True
        self.sessions[session.nif_id] = session

    def remove_session(self, nif_id: str) -> None:
        del self.sessions[nif_id]
        if self.active_id == nif_id:
            self.active_id = self.main_id

    def get_session(self, nif_id: str) -> NifSession:
        return self.sessions[nif_id]

    def get_children(self, nif_id: str) -> list[NifSession]:
        return [s for s in self.sessions.values() if s.parent_nif_id == nif_id]

    def all_sessions(self) -> list[NifSession]:
        # Main first, then children in insertion order
        result = []
        if self.main_id in self.sessions:
            result.append(self.sessions[self.main_id])
        for nid, session in self.sessions.items():
            if nid != self.main_id:
                result.append(session)
        return result

    def next_child_id(self) -> str:
        """Generate the next available child_N ID."""
        i = 0
        while f"child_{i}" in self.sessions:
            i += 1
        return f"child_{i}"

    def clear(self) -> None:
        """Remove all sessions (used when loading a new main NIF)."""
        self.sessions.clear()
        self.active_id = self.main_id
