"""Animation path classification helpers.

These filter animation directories that are already in a native dependency graph.
"""
from __future__ import annotations

import logging

_log = logging.getLogger("conversion.animation_lookup")


def is_grip_keyword(editor_id: str) -> bool:
    """Return True if this keyword is a shared grip/class keyword.

    Grip keywords follow the naming convention ``AnimsGrip*``
    (e.g., AnimsGripPistol, AnimsGripRifleStraight).  Their animation
    paths are shared by many weapons and already exist in the target game.
    """
    return editor_id.startswith("AnimsGrip")


def _is_fallback_path(path: str, *, weapon_name: str | None = None) -> bool:
    """Return True if this path should be INCLUDED (is NOT a generic fallback).

    Even weapon-specific keywords include generic fallback dirs in their
    AnimationPaths (Common, Paired, Player, etc.).  Filter those out —
    we only want dirs that actually contain weapon-specific animations.

    Creature-specific paths (actors/Snallygaster/animations) always pass
    because their ``/animations`` dir IS the creature-specific content.
    Only paths under the shared human/power-armor trees are filtered.

    When *weapon_name* is provided, applies positive-match filtering under
    shared actor trees: the path must contain a segment matching the weapon
    name (case-insensitive).  This rejects wrong-weapon directories that
    FO76 bundles into the same Race subgraph entry (e.g., GammaGun paths
    when converting GaussPistol).  Race-specific subdirs (Player/, Synth/)
    are allowed if they're nested under a matching weapon folder.
    """
    lower = path.lower().replace("\\", "/")

    # Only filter under shared actor trees.  Creature paths like
    # actors/megasloth/animations are kept — they contain the base
    # animation set the creature needs.  Override subdirs like
    # actors/megasloth/animations/ogua layer on top.
    _SHARED_ACTOR_TREES = (
        "actors/character/", "actors/powerarmor/",
        "actors/supermutant/", "actors/scorched/",
    )
    is_shared = any(lower.startswith(p) or f"/{p}" in lower
                     for p in _SHARED_ACTOR_TREES)
    if not is_shared:
        return True

    # Subdirectory patterns that can appear at any depth.
    # E.g., Actors/Character/Animations/Common/Emotes/Female
    _GENERIC_SUBSTRINGS = {"/common/", "/emotes/"}
    for sub in _GENERIC_SUBSTRINGS:
        if sub in lower:
            return False

    # Generic terminal directory names shared across all weapons.
    # Race-specific subdirs (Player, Synth) are only generic when they're
    # at the top level (e.g., .../Animations/Player).  When nested under
    # a weapon folder (e.g., .../Weapon/GaussPistol/Player), they contain
    # race-specific copies of that weapon's animations and should be kept.
    _GENERIC_SUFFIXES = {
        "/common", "/paired", "/animations", "/emotes",
    }
    _RACE_SUBDIRS = {"/player", "/synth"}

    for suffix in _GENERIC_SUFFIXES:
        if lower.endswith(suffix):
            return False

    for suffix in _RACE_SUBDIRS:
        if lower.endswith(suffix):
            # Allow race subdirs when nested under a weapon-name folder.
            # E.g., .../Weapon/GaussPistol/Player -> keep
            #        .../Animations/Player           -> reject
            parent = lower[:lower.rfind("/")]
            parent_leaf = parent.rsplit("/", 1)[-1]
            # If we have a weapon name, check if the parent matches it
            if weapon_name and weapon_name.lower() in parent_leaf:
                return True
            # Without weapon name, reject bare race subdirs (conservative)
            return False

    # Positive-match filtering: when weapon_name is set, require that
    # at least one path segment contains the weapon name.  This rejects
    # wrong-weapon dirs (GammaGun, Pistol) bundled in the same subgraph.
    if weapon_name and is_shared:
        wn_lower = weapon_name.lower()
        segments = lower.split("/")
        if not any(wn_lower in seg for seg in segments):
            _log.debug(
                "Rejected animation path (no '%s' segment): %s",
                weapon_name, path,
            )
            return False

    return True
