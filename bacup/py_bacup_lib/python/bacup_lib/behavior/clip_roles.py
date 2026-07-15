from __future__ import annotations

import re
from pathlib import Path

ROLE_ORDER = (
    "idle",
    "locomotion",
    "attack",
    "hit_react",
    "death",
    "special",
    "unknown",
)

_NON_ALNUM_RE = re.compile(r"[^a-z0-9]+")

_DEATH_MARKERS = ("death", "dying", "kill")
_HIT_REACT_MARKERS = (
    "crithit",
    "hitreact",
    "hit",
    "stagger",
    "flinch",
    "recoil",
    "injured",
    "cripple",
)
_ATTACK_MARKERS = (
    "attack",
    "h2h",
    "melee",
    "bite",
    "claw",
    "slam",
    "bash",
    "throw",
    "spit",
)
_IDLE_MARKERS = ("idle", "breath", "combatidle")
_LOCOMOTION_MARKERS = (
    "walk",
    "run",
    "sprint",
    "turn",
    "evade",
    "jump",
    "land",
    "fall",
    "forward",
    "back",
    "left",
    "right",
    "strafe",
    "locomotion",
)
_SPECIAL_MARKERS = (
    "special",
    "equip",
    "unequip",
    "activate",
    "ambush",
    "flip",
    "taunt",
    "roar",
    "scratch",
    "detect",
    "dialogue",
    "furniture",
    "cage",
)
_TRAILING_VARIANT_MARKERS = ("hurt",)


def _normalized_candidates(filename: str) -> tuple[str, ...]:
    stem = Path(filename).stem.lower()
    parts = [part for part in _NON_ALNUM_RE.split(stem) if part]
    collapsed = "".join(parts)
    if len(parts) <= 1:
        return (collapsed,)
    candidates = ["".join(parts[1:])]
    if len(parts[-1]) <= 2 or parts[-1] in _TRAILING_VARIANT_MARKERS:
        candidates.append("".join(parts[:-1]))
    return tuple(dict.fromkeys(candidate for candidate in candidates if candidate))


def classify_clip_role(filename: str) -> str:
    for stem in _normalized_candidates(filename):
        role = _classify_normalized_stem(stem)
        if role != "unknown":
            return role
    return "unknown"


def _classify_normalized_stem(stem: str) -> str:
    if any(marker in stem for marker in _DEATH_MARKERS):
        return "death"
    if any(marker in stem for marker in _HIT_REACT_MARKERS):
        return "hit_react"
    if any(marker in stem for marker in _ATTACK_MARKERS):
        return "attack"
    if any(marker in stem for marker in _IDLE_MARKERS):
        return "idle"
    if any(marker in stem for marker in _LOCOMOTION_MARKERS):
        return "locomotion"
    if any(marker in stem for marker in _SPECIAL_MARKERS):
        return "special"
    return "unknown"


def bucket_clips_by_role(filenames: list[str]) -> dict[str, tuple[str, ...]]:
    buckets: dict[str, list[str]] = {role: [] for role in ROLE_ORDER}
    for filename in sorted(filenames, key=lambda value: (value.lower(), value)):
        buckets[classify_clip_role(filename)].append(filename)
    return {role: tuple(values) for role, values in buckets.items()}
