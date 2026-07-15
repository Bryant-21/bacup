"""Atomic Shop (ATX) skin walker + heuristic weapon-sound discovery.

The main dependency walker (``walker.py``) follows record→record and
record→asset references but cannot find content that lives outside the
record graph entirely:

  1. **ATX skins** — Fallout 76 ships paid weapon and armor paint variants
     under ``materials/atx/<category>/<slug>/`` (and the matching textures
     under ``textures/atx/<category>/<slug>/``), where ``<category>`` is
     ``weapons`` or ``armor``. These are referenced by
     ``MaterialSwap`` records in the FO76 master ESM, but the master ESM
     records often live behind content-pack flags the walker doesn't
     traverse, so the BGSMs/DDS are never discovered. Result: ported
     weapons end up with the base skin only.

  2. **Per-weapon sound samples** — FO76 weapons store fire/charge/reload
     samples under ``sound/fx/wpn/<weapon_name>/``. These should be
     pulled in via the ``SoundDescriptor.SoundFiles`` field on each
     SNDR record, but the FO76 SNDRs sometimes reference vanilla SMG
     paths (or are missing the field entirely), so the actual gauss
     ``.wav`` files never make it into the mod even though the SNDR
     records do.

This module patches both gaps by walking the *filesystem* directly:

  - For each weapon material slug already in the dependency graph
    (e.g. ``gausspistol``), discover sibling ATX BGSMs, parse their
    texture references, and synthesize one ``MaterialSwap`` record per
    skin variant. The original→ATX substitution list is built from
    filename pattern matching (``atx_<slug>_<part>_<variant>.bgsm``).

  - For each weapon root, scan ``sound/fx/wpn/`` for subdirectories
    whose names contain tokens from the weapon's EditorID. ``GaussPistol``
    matches ``pistolgauss`` (Bethesda swaps the order in sound dirs).
    All discovered ``.wav`` files are added as sound assets so the
    scaffold phase copies them into the mod.

Both discoveries are best-effort and silent on no-match: if no ATX
content or sound files exist for the weapon, nothing is added.
"""
from __future__ import annotations

import io
import logging
import os
import re
from dataclasses import dataclass

from bacup_lib.models import AssetRef, DependencyGraph, RecordNode

_log = logging.getLogger("conversion.atx_walker")


# Filename pattern for ATX BGSMs:
#   atx_<slug>_<part>_<variant>.bgsm
# where <variant> is the skin name (matteblack, secretservice, ...) and
# <part> can contain underscores. The variant is always the last underscore
# segment before .bgsm.
_ATX_BGSM_RE = re.compile(r"^atx_(.+)_([^_]+)\.bgsm$", re.IGNORECASE)


# Texture field names on BGSMData that may carry references we want to walk.
# Mirrors the field list in orchestrator._convert_bgsm.
_BGSM_TEXTURE_FIELDS = (
    "DiffuseTexture", "NormalTexture", "SmoothSpecTexture",
    "GreyscaleTexture", "GlowTexture", "WrinklesTexture",
    "EnvmapTexture", "InnerLayerTexture", "DisplacementTexture",
    "SpecularTexture", "LightingTexture", "FlowTexture",
    "DistanceFieldAlphaTexture",
)


@dataclass
class AtxDiscoveryResult:
    """Output of discover_atx_skins — assets and synthesized records to merge."""

    new_assets: list[AssetRef]
    new_records: list[RecordNode]
    slugs_walked: list[str]


def _infer_extracted_dir(graph: DependencyGraph) -> str | None:
    """Walk graph assets to find an extracted_dir root.

    The orchestrator stores ``extracted_dir`` on the runner, not the graph,
    so we infer it from any resolved asset by stripping its source_path tail
    off the resolved_path head.

    Caveat: some assets store ``source_path`` *without* the standard data
    subdir prefix — e.g. NIFs are stored as ``Weapons/Gun.nif`` not
    ``Meshes/Weapons/Gun.nif``. Stripping naively would leave the inferred
    root as ``.../extracted/fo76/Meshes`` instead of ``.../extracted/fo76``.
    Fix: after the strip, peel off any trailing standard data subdir.
    """
    standard_subdirs = ("meshes", "materials", "textures", "sound", "music", "interface", "scripts", "strings")
    for a in graph.all_assets:
        if not a.resolved_path:
            continue
        rp = a.resolved_path.replace("\\", "/")
        sp = a.source_path.replace("\\", "/")
        idx = rp.lower().find(sp.lower())
        if idx <= 0:
            continue
        head = rp[:idx].rstrip("/")
        # Peel a trailing standard subdir if present.
        tail = head.rsplit("/", 1)[-1].lower()
        if tail in standard_subdirs:
            head = head.rsplit("/", 1)[0]
        return head
    return None


# Material categories that ship ATX skins: each has a
# ``materials/<category>/<slug>/`` base tree mirrored by a
# ``materials/atx/<category>/<slug>/`` tree of paid skin variants. Both
# weapons (WEAP) and armor (ARMO) carry ATX paints.
_ATX_MATERIAL_CATEGORIES = ("weapons", "armor")


def _collect_material_slugs(graph: DependencyGraph) -> set[tuple[str, str]]:
    """Find unique ``materials/<category>/<slug>/`` directories in walked assets.

    Returns ``(category, slug)`` pairs (e.g. ``("weapons", "gausspistol")``)
    for every ATX-bearing category (see ``_ATX_MATERIAL_CATEGORIES``).
    """
    cats = "|".join(_ATX_MATERIAL_CATEGORIES)
    pattern = re.compile(rf"^(?:materials/)?({cats})/([^/]+)/")
    slugs: set[tuple[str, str]] = set()
    for asset in graph.all_assets:
        if asset.asset_type != "material":
            continue
        path = asset.source_path.replace("\\", "/").lower()
        m = pattern.match(path)
        if m:
            slugs.add((m.group(1), m.group(2)))
    return slugs


def _read_bgsm_texture_refs(bgsm_path: str) -> list[str]:
    """Open a BGSM file and return all populated texture path fields.

    Returns paths in their on-disk form (typically ``textures\\...``). Empty
    on parse failure.
    """
    try:
        from creation_lib.material_tools.bgsm_bin import read_bgsm
    except Exception as e:
        _log.debug("Cannot import bgsm_bin: %s", e)
        return []
    try:
        with open(bgsm_path, "rb") as f:
            bgsm = read_bgsm(f)
    except Exception as e:
        _log.debug("Failed to read BGSM %s: %s", bgsm_path, e)
        return []
    refs: list[str] = []
    for field in _BGSM_TEXTURE_FIELDS:
        val = getattr(bgsm, field, None)
        if val and isinstance(val, str):
            cleaned = val.replace("\x00", "").strip()
            if cleaned:
                refs.append(cleaned)
    return refs


def _normalize_data_relative(path: str) -> str:
    """Strip a leading ``data\\`` / ``data/`` prefix and unify to forward slashes."""
    p = path.replace("\\", "/").lstrip("/")
    if p.lower().startswith("data/"):
        p = p[5:]
    return p


def _texture_resolves(extracted_dir: str, rel_path: str) -> str | None:
    """Return the absolute path of a texture under extracted_dir if it exists.

    Tries the path as-is and several common case-fold variations. Returns
    None if no match.
    """
    candidates = [rel_path]
    # Some BGSMs reference textures without the ``textures\`` prefix.
    if not rel_path.lower().startswith("textures/"):
        candidates.append("textures/" + rel_path)
    for cand in candidates:
        full = os.path.join(extracted_dir, cand)
        if os.path.isfile(full):
            return full
    return None


def discover_atx_skins(
    graph: DependencyGraph,
    extracted_dir: str | None = None,
    mod_name: str = "Mod",
) -> AtxDiscoveryResult:
    """Discover ATX skin BGSMs+textures and synthesize MaterialSwap records.

    For each unique ``materials/<category>/<slug>/`` directory referenced in
    the existing dependency graph (``<category>`` is ``weapons`` or ``armor``;
    both ship ATX paints), look for a ``materials/atx/<category>/<slug>/``
    sibling under ``extracted_dir`` and pull in every ``atx_*.bgsm`` it finds.
    Parses each ATX BGSM to capture its referenced textures (which may live
    under ``textures/atx/<category>/<slug>/`` with a slightly different naming
    convention — Bethesda inconsistency).

    Groups the discovered ATX BGSMs by skin variant (matteblack,
    secretservice, ...) and emits one synthesized ``MaterialSwap``
    ``RecordNode`` per variant. The Substitutions list maps each original
    BGSM path to the corresponding ATX BGSM, ready for the translator
    and YAML writer to handle in their normal passes.

    Returns an ``AtxDiscoveryResult`` with new assets and records that the
    caller should append to ``graph.all_assets`` / ``graph.all_records``.
    """
    if extracted_dir is None:
        extracted_dir = _infer_extracted_dir(graph)
    if not extracted_dir or not os.path.isdir(extracted_dir):
        return AtxDiscoveryResult([], [], [])

    slugs = _collect_material_slugs(graph)
    if not slugs:
        return AtxDiscoveryResult([], [], [])

    # Slug guard: only walk slugs that belong to the root weapon's
    # family. When a shared ATX style entry references materials for multiple
    # weapon families (e.g. "MatteBlack" for gausspistol + gaussrifle +
    # gaussshotgun), all three slugs appear in the graph. Without this guard
    # we'd pull in ~48 MB of unrelated weapon textures per conversion.
    #
    # ATX slugs are lowercased compound words (e.g. "gausspistol") with no
    # internal separators. Root EditorIDs are CamelCase (e.g. "GaussPistol").
    # We check substring containment: a slug is "primary" for this weapon if
    # ALL root EID tokens that appear in at least one collected slug are
    # present in that slug's string. Tokens that don't appear in any slug
    # (e.g. mod author prefixes like "b21") are excluded from the filter —
    # they can't distinguish weapon families anyway.
    #
    # If the root has no useful tokens (empty EID or only generic tokens),
    # fall through with no filtering — safe fallback.
    root_tokens = _editor_id_tokens(graph.root.editor_id) if graph.root.editor_id else []
    if root_tokens:
        # Only keep tokens that actually appear in at least one slug string —
        # strips author prefixes (e.g. "b21") that aren't part of slug names.
        slug_lower_set = {slug.lower() for (_cat, slug) in slugs}
        relevant_tokens = [t for t in root_tokens if any(t in s for s in slug_lower_set)]
        if relevant_tokens:
            slugs = {(cat, slug) for (cat, slug) in slugs if all(tok in slug.lower() for tok in relevant_tokens)}

    new_assets: list[AssetRef] = []
    new_records: list[RecordNode] = []
    seen_asset_keys: set[tuple[str, str]] = {
        (a.asset_type, a.source_path.replace("\\", "/").lower())
        for a in graph.all_assets
    }
    walked_slugs: list[str] = []

    def _add_asset(asset_type: str, src_path: str, resolved: str | None) -> None:
        key = (asset_type, src_path.replace("\\", "/").lower())
        if key in seen_asset_keys:
            return
        seen_asset_keys.add(key)
        new_assets.append(AssetRef(
            asset_type=asset_type,
            source_path=src_path,
            resolved_path=resolved,
        ))

    mswp_counter = 0

    for category, slug in sorted(slugs):
        atx_dir = os.path.join(extracted_dir, "materials", "atx", category, slug)
        if not os.path.isdir(atx_dir):
            continue
        walked_slugs.append(f"{category}/{slug}")

        # Bucket BGSMs by skin variant.
        # variant -> list of (part_name, atx_relative_path, original_relative_path)
        variants: dict[str, list[tuple[str, str, str]]] = {}

        for fname in sorted(os.listdir(atx_dir)):
            if not fname.lower().endswith(".bgsm"):
                continue
            m = _ATX_BGSM_RE.match(fname)
            if not m:
                continue
            stem = m.group(1)        # everything between atx_ and _<variant>
            variant = m.group(2).lower()

            # Original BGSM filename is the ATX filename with the ``atx_``
            # prefix and ``_<variant>`` suffix removed. This preserves any
            # typo in the source filename (e.g. ``atx_grausspistol_scope_*``
            # → ``grausspistol_scope.bgsm``) so the substitution actually
            # points at a file that exists under
            # ``materials/<category>/<slug>/``.
            original_filename = stem + ".bgsm"
            # Bare part name (slug stripped where possible) — used as a
            # bucket key inside the variant. This is best-effort and only
            # affects logging / sort order.
            part = stem
            if part.lower().startswith(slug.lower()):
                part = part[len(slug):].lstrip("_")

            atx_full = os.path.join(atx_dir, fname)
            atx_rel = f"materials/atx/{category}/{slug}/{fname}"
            original_rel = f"materials/{category}/{slug}/{original_filename}"

            variants.setdefault(variant, []).append((part, atx_rel, original_rel))

            # Add the ATX BGSM as a material asset.
            _add_asset("material", atx_rel, atx_full)

            # Walk its texture refs and add any that resolve under extracted_dir.
            for tex_ref in _read_bgsm_texture_refs(atx_full):
                tex_rel = _normalize_data_relative(tex_ref)
                resolved = _texture_resolves(extracted_dir, tex_rel)
                # The BGSM may reference textures with or without the
                # ``textures\`` prefix; the asset's source_path needs to
                # match the resolved location so the scaffold phase puts
                # it in the right output dir.
                if resolved:
                    out_rel = os.path.relpath(resolved, extracted_dir).replace("\\", "/")
                    _add_asset("texture", out_rel, resolved)
                else:
                    # Add as unresolved so the run log surfaces the gap.
                    _add_asset("texture", tex_rel, None)

        # Synthesize one MaterialSwap record per skin variant.
        for variant in sorted(variants.keys()):
            entries = variants[variant]
            substitutions = [
                {
                    "OriginalMaterial": orig.replace("/", "\\"),
                    "ReplacementMaterial": atx.replace("/", "\\"),
                }
                for (_part, atx, orig) in entries
            ]
            # Camel-case-ish editor ID, e.g. GaussPistolMatteBlack.
            slug_eid = "".join(p.capitalize() for p in re.split(r"[_\-]", slug) if p)
            variant_eid = "".join(p.capitalize() for p in re.split(r"[_\-]", variant) if p)
            editor_id = f"{slug_eid}{variant_eid}MaterialSwap"

            synthetic_fk = f"ATX_MSWP_{mswp_counter}:{mod_name}.esp"
            mswp_counter += 1

            new_records.append(RecordNode(
                form_key=synthetic_fk,
                editor_id=editor_id,
                record_type="MSWP",
            ))

    return AtxDiscoveryResult(
        new_assets=new_assets,
        new_records=new_records,
        slugs_walked=walked_slugs,
    )


# ----------------------------------------------------------------------------
# Heuristic weapon-sound discovery
# ----------------------------------------------------------------------------


# Meta tokens that should be stripped from EditorID before computing match
# keys — these are universal markers that don't help distinguish between
# weapons. Category words like ``pistol``, ``rifle``, ``shotgun`` are
# intentionally NOT in this list because they discriminate weapon class
# (e.g. ``GaussPistol`` -> match ``pistolgauss``, NOT ``riflegauss``).
_GENERIC_WEAPON_TOKENS = frozenset({
    "weapon", "weapons",
    "first", "third", "person",
    "fo4", "fo76", "wpn",
    "nonplayable", "playable",
    "1st", "3rd",
})


def _editor_id_tokens(editor_id: str) -> list[str]:
    """Split a CamelCase / snake_case EditorID into lowercase token list.

    ``GaussPistol`` -> ``[gauss, pistol]``
    ``WPNGaussRifle`` -> ``[wpn, gauss, rifle]``  (then ``wpn`` is filtered)
    """
    if not editor_id:
        return []
    # Split on underscores first.
    parts = re.split(r"[_\-]", editor_id)
    tokens: list[str] = []
    for part in parts:
        # Then split CamelCase: insert space before each uppercase that
        # follows a lowercase or precedes a lowercase.
        spaced = re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", part)
        spaced = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1 \2", spaced)
        for token in spaced.split():
            t = token.lower()
            if t and t not in _GENERIC_WEAPON_TOKENS:
                tokens.append(t)
    return tokens


def discover_weapon_sounds(
    graph: DependencyGraph,
    extracted_dir: str | None = None,
) -> list[AssetRef]:
    """Scan ``sound/fx/wpn/`` for files matching the weapon root's EID tokens.

    For weapon roots, walks ``extracted_dir/sound/fx/wpn/`` and copies any
    ``.wav`` whose containing directory name contains at least one
    distinctive token from the weapon's EditorID. Token matching is
    case-insensitive.

    This is a heuristic — it will not find sounds in unusual layouts and
    can over-match weapons whose names share substrings (e.g. ``Pistol``
    matches every pistol). The token filter ``_GENERIC_WEAPON_TOKENS``
    drops the obvious offenders.
    """
    if extracted_dir is None:
        extracted_dir = _infer_extracted_dir(graph)
    if not extracted_dir or not os.path.isdir(extracted_dir):
        return []

    if graph.root.record_type != "Weapons":
        return []

    tokens = _editor_id_tokens(graph.root.editor_id)
    if not tokens:
        return []

    wpn_root = os.path.join(extracted_dir, "sound", "fx", "wpn")
    if not os.path.isdir(wpn_root):
        return []

    seen_asset_keys: set[tuple[str, str]] = {
        (a.asset_type, a.source_path.replace("\\", "/").lower())
        for a in graph.all_assets
    }
    new_assets: list[AssetRef] = []

    for entry in sorted(os.listdir(wpn_root)):
        sub = os.path.join(wpn_root, entry)
        if not os.path.isdir(sub):
            continue
        entry_lower = entry.lower()
        # Require ALL tokens to appear somewhere in the directory name —
        # this prevents single-token over-matches like "pistol" pulling
        # in every pistol's sound directory.
        if not all(tok in entry_lower for tok in tokens):
            continue

        # Walk the matched directory recursively for .wav files.
        for root, _dirs, files in os.walk(sub):
            for fname in files:
                if not fname.lower().endswith(".wav"):
                    continue
                full = os.path.join(root, fname)
                rel = os.path.relpath(full, extracted_dir).replace("\\", "/")
                key = ("sound", rel.lower())
                if key in seen_asset_keys:
                    continue
                seen_asset_keys.add(key)
                # Strip the leading ``sound/`` so the source_path matches
                # what the SoundDescriptor extractor produces.
                src_rel = rel
                if src_rel.lower().startswith("sound/"):
                    src_rel = src_rel[6:]
                new_assets.append(AssetRef(
                    asset_type="sound",
                    source_path=src_rel,
                    resolved_path=full,
                ))

    return new_assets


# ----------------------------------------------------------------------------
# Per-weapon animation discovery
# ----------------------------------------------------------------------------

# FO76 weapon-specific animation clips live under per-weapon subdirectories of
# these roots (3rd- and 1st-person, character and power-armor). A converted
# weapon's additive RACE references these via SAPT paths, but the SGNM
# behaviour graphs are all vanilla FO4 — so only the clip .hkx files need
# converting + packing. The native dependency walker does not extract RACE
# SAPT directories, so these are invisible to it; this filesystem pass fills
# the gap, exactly as discover_weapon_sounds does for sound/fx/wpn.
_WEAPON_ANIM_ROOTS = (
    ("meshes", "actors", "character", "animations", "weapon"),
    ("meshes", "actors", "character", "_1stperson", "animations"),
    ("meshes", "actors", "powerarmor", "animations", "weapons"),
    ("meshes", "actors", "powerarmor", "_1stperson", "animations"),
)


def discover_weapon_animations(
    graph: DependencyGraph,
    extracted_dir: str | None = None,
) -> list[AssetRef]:
    """Scan FO76 per-weapon animation dirs for clips matching the root EID.

    For each weapon-animation root (see ``_WEAPON_ANIM_ROOTS``), matches the
    immediate subdirectory whose name contains *all* distinctive EditorID
    tokens (e.g. ``GaussPistol`` -> ``gausspistol``) and emits every ``.hkx``
    underneath as an ``animation`` asset. The all-tokens rule is what keeps
    this scoped: a bare ``pistol`` or sibling ``gaussrifle`` dir does NOT match,
    so we never convert (and thus override) vanilla shared-class animations.

    Heuristic, like discover_weapon_sounds: silent when nothing matches.
    """
    if extracted_dir is None:
        extracted_dir = _infer_extracted_dir(graph)
    if not extracted_dir or not os.path.isdir(extracted_dir):
        return []

    # The native walker stores the raw 4-char signature ("WEAP") in
    # record_type, while authoring/fixture code uses the display label
    # ("Weapons"). Compare on the canonical signature so the guard holds for
    # both. (discover_weapon_sounds checks the bare display label and is dead
    # on the production walk path for the same reason.)
    from creation_lib.esp.record_types import record_type_signature

    if record_type_signature(graph.root.record_type) != "WEAP":
        return []

    tokens = _editor_id_tokens(graph.root.editor_id)
    if not tokens:
        return []

    seen_asset_keys: set[tuple[str, str]] = {
        (a.asset_type, a.source_path.replace("\\", "/").lower())
        for a in graph.all_assets
    }
    new_assets: list[AssetRef] = []

    for root_parts in _WEAPON_ANIM_ROOTS:
        anim_root = os.path.join(extracted_dir, *root_parts)
        if not os.path.isdir(anim_root):
            continue
        for entry in sorted(os.listdir(anim_root)):
            sub = os.path.join(anim_root, entry)
            if not os.path.isdir(sub):
                continue
            entry_lower = entry.lower()
            if not all(tok in entry_lower for tok in tokens):
                continue
            for cur_root, _dirs, files in os.walk(sub):
                for fname in files:
                    if not fname.lower().endswith(".hkx"):
                        continue
                    full = os.path.join(cur_root, fname)
                    rel = os.path.relpath(full, extracted_dir).replace("\\", "/")
                    key = ("animation", rel.lower())
                    if key in seen_asset_keys:
                        continue
                    seen_asset_keys.add(key)
                    new_assets.append(AssetRef(
                        asset_type="animation",
                        source_path=rel,
                        resolved_path=full,
                    ))

    return new_assets
