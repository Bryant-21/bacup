"""Shared engine-semantics gate for OMOD relationship passes.

OMOD relationship discovery can over-approximate because records may
touch overlapping FormKeys without being selectable on the same weapon.
This module supplies the gate that mirrors what the FO4 workbench engine
actually does at runtime:

  (a) ``AttachPoint`` matches one of the slots the weapon already
      exposes (collected from forward-walked OMODs' AttachPoint fields).
  (b) ``TargetOmodKeywords`` is a SUBSET of the weapon's ``Keywords`` —
      every listed keyword must be present on the weapon, not merely
      one of them.

Without (a)+(b) the lookup pulls hundreds of unrelated OMODs whenever
the weapon shares a generic capability tag like FO76 ``ma_Gun_Appearance``
(present on 592+ records — every paintable gun in the game).
"""
from __future__ import annotations

from pathlib import Path
from typing import Iterable

import yaml

_WHITELIST_PATH = (
    Path(__file__).resolve().parent
    / "record"
    / "whitelists"
    / "universal_omod_keywords.yaml"
)
_universal_kw_cache: dict[str, set[str]] | None = None


def _load_universal_keywords() -> dict[str, set[str]]:
    """Lazy-load the per-game universal-keyword whitelist."""
    global _universal_kw_cache
    if _universal_kw_cache is not None:
        return _universal_kw_cache

    out: dict[str, set[str]] = {}
    if _WHITELIST_PATH.is_file():
        try:
            with open(_WHITELIST_PATH, encoding="utf-8") as f:
                data = yaml.safe_load(f) or {}
            raw = data.get("universal_target_omod_keywords") or {}
            for game, eids in raw.items():
                if isinstance(eids, list):
                    out[game] = {str(e) for e in eids if isinstance(e, str) and e}
                else:
                    out[game] = set()
        except Exception:
            out = {}
    _universal_kw_cache = out
    return out


def get_universal_omod_keyword_eids(game: str) -> set[str]:
    """Return the set of EditorIDs whose presence alone in ``TargetOmodKeywords``
    is too broad to seed a sibling-OMOD reverse lookup for ``game``.

    Returns an empty set when the game is unknown or the whitelist is
    missing — callers degrade to the existing behaviour (no skip).
    """
    return set(_load_universal_keywords().get(game, set()))


def omod_matches_weapon(
    weapon_keywords: Iterable[str],
    weapon_attach_points: Iterable[str],
    omod_yaml: dict,
) -> bool:
    """Return ``True`` iff the OMOD would actually attach to the weapon.

    Mirrors the FO4 workbench engine's selectability check:
      - ``AttachPoint`` is one of the slots the weapon exposes
      - ``TargetOmodKeywords`` is a non-empty subset of the weapon's
        ``Keywords``

    An empty ``TargetOmodKeywords`` is rejected: with no keyword filter
    the engine would show the OMOD on any weapon with the matching
    AttachPoint, which is too broad for an automated reverse-lookup.
    """
    if not isinstance(omod_yaml, dict):
        return False

    ap = omod_yaml.get("AttachPoint")
    if not isinstance(ap, str) or not ap:
        return False
    weapon_aps = set(weapon_attach_points)
    if ap not in weapon_aps:
        return False

    tok = omod_yaml.get("TargetOmodKeywords") or []
    if not isinstance(tok, list):
        return False
    tok_set = {k for k in tok if isinstance(k, str) and k}
    if not tok_set:
        return False

    weapon_kws = set(weapon_keywords)
    return tok_set.issubset(weapon_kws)
