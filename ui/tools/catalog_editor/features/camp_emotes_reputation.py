"""CampWallpapers, Emotes and Reputation: warnings for entries Tales would skip, taken from its readers.

Warnings never block a save; they say why a row would do nothing in game.
"""

from __future__ import annotations

from ui.tools.catalog_editor.features import FeatureExtension, ResultView, register
from ui.tools.catalog_editor.features.weapon_model_swaps import pair_issues
from ui.tools.catalog_editor.model import ADDED, OVERRIDDEN, REMOVED, Problem


def _own_rows(doc, section):
    return [row for row in doc.rows(section) if row.status in (ADDED, OVERRIDDEN) and isinstance(row.entry, dict)]


def _live_keys(doc, section) -> set[str]:
    return {row.key for row in doc.rows(section) if row.status != REMOVED}


def _warn(doc, section, row, message) -> Problem:
    return Problem(doc.name, section, row.key, message, blocking=False)


# --- CampWallpapers (CampBuildWallpaperRules.h) ---------------------------------------------------------------


def wallpaper_issues(entry: dict, model_sets: set[str]) -> list[str]:
    if entry.get("strip"):
        if entry.get("material_swap") or entry.get("model_swap"):
            return ["the 'Remove Wallpaper' row carries no swaps; Tales ignores them"]
        return []
    out = []
    if not entry.get("material_swap"):
        out.append("no material swap; Tales drops this wallpaper from the build menu")
    swap = entry.get("model_swap") or ""
    if not swap:
        out.append("no mesh swap set; Tales drops this wallpaper from the build menu")
    elif swap not in model_sets:
        out.append(f"mesh swap set {swap!r} is not in the Mesh swap sets section; Tales drops this wallpaper")
    return out


class CampWallpapersExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != "CampWallpapers":
            return []
        sets = _live_keys(doc, "model_swaps")
        out = [_warn(doc, "wallpapers", row, message)
               for row in _own_rows(doc, "wallpapers") for message in wallpaper_issues(row.entry, sets)]
        for row in (r for r in doc.rows("model_swaps") if r.status in (ADDED, OVERRIDDEN)):
            pairs = row.entry if isinstance(row.entry, list) else []
            out += [_warn(doc, "model_swaps", row, message) for message in pair_issues(pairs)]
        return out


# --- Emotes (EmoteRules.h ValidateRow) --------------------------------------------------------------------------


def _clip_ok(path: str) -> bool:
    return len(path) >= 5 and ".." not in path and ":" not in path and path.lower().endswith(".hkx")


def emote_issues(entry: dict, categories: set[str]) -> list[str]:
    kind = entry.get("kind")
    out = []
    if kind == "pet":
        return ["pet emotes are withheld; Tales has no pet emote system"]
    if entry.get("category_id") not in categories:
        out.append("category is not one of the categories; Tales drops the emote")
    if not entry.get("name") or not entry.get("icon"):
        out.append("name and icon are required; Tales drops the emote")
    duration = entry.get("duration")
    if not isinstance(duration, (int, float)) or not 0 < duration <= 60:
        out.append("duration must be above 0 and at most 60 seconds; Tales drops the emote")
    if not 0 <= entry.get("dice_faces", 0) <= 100:
        out.append("dice faces must be 0 to 100; Tales drops the emote")
    if kind == "human":
        if not entry.get("idle_id"):
            out.append("a character emote needs an idle; Tales drops the emote")
        clips = [entry.get(k) or "" for k in ("animation", "animation_female", "animation_power_armor")]
        if not any(clips):
            out.append("a character emote needs at least one animation clip; Tales drops the emote")
        out += [f"animation {clip!r} must be a relative .hkx path; Tales drops the emote" for clip in clips
                if clip and not _clip_ok(clip)]
    return out


class EmotesExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != "Emotes":
            return []
        categories = _live_keys(doc, "categories")
        return [_warn(doc, "entries", row, message)
                for row in _own_rows(doc, "entries") for message in emote_issues(row.entry, categories)]


# --- Reputation (ReputationPresentation.cpp) ----------------------------------------------------------------


def faction_issues(entry: dict) -> list[str]:
    out = []
    if entry.get("code") not in ("Crater", "Foundation"):
        out.append("only the Crater and Foundation factions have HUD art; Tales skips others")
    if not entry.get("name") or not entry.get("actor_value"):
        out.append("a faction needs a name and an actor value; Tales skips it")
    thresholds = [tier.get("threshold") for tier in entry.get("tiers", []) if isinstance(tier, dict)]
    if len(thresholds) != 7:
        out.append(f"needs exactly 7 tiers, has {len(thresholds)}; Tales skips the faction")
    elif any(not isinstance(t, (int, float)) for t in thresholds) or any(a >= b for a, b in zip(thresholds, thresholds[1:])):
        out.append("tier thresholds must rise strictly from tier 1 to tier 7; Tales skips the faction")
    return out


class ReputationExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != "Reputation":
            return []
        return [_warn(doc, "factions", row, message)
                for row in _own_rows(doc, "factions") for message in faction_issues(row.entry)]

    def result_view(self, doc, section):
        return ResultView("Tiers", draw_tiers) if doc.name == "Reputation" and section == "factions" else None

    def main_list(self, doc, section):
        return ("tiers", "tiers") if section == "factions" else None


def draw_tiers(view):
    from imgui_bundle import imgui

    from creation_lib.ui.widgets.modern import semantic_color
    from ui.tools.catalog_editor.workbench import value_text

    entry = view.entry if isinstance(view.entry, dict) else {}
    imgui.text_wrapped(f"{entry.get('name') or 'This faction'} counts reputation in "
                       f"{value_text(entry.get('actor_value', ''), view.name_of)}.")
    for message in faction_issues(entry):
        imgui.text_colored(semantic_color("warning"), message)
    tiers = [tier for tier in entry.get("tiers", []) if isinstance(tier, dict)]
    flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h
    if tiers and imgui.begin_table("##tiers", 3, flags):
        imgui.table_setup_column("Tier")
        imgui.table_setup_column("From")
        imgui.table_setup_column("Details")
        imgui.table_headers_row()
        for number, tier in enumerate(tiers, start=1):
            imgui.table_next_row()
            imgui.table_next_column()
            imgui.text(str(number))
            imgui.table_next_column()
            imgui.text(value_text(tier.get("threshold"), view.name_of))
            imgui.table_next_column()
            imgui.text_wrapped(value_text({k: v for k, v in tier.items() if k != "threshold"}, view.name_of))
        imgui.end_table()
    return False, view.entry


register("CampWallpapers", CampWallpapersExtension())
register("Emotes", EmotesExtension())
register("Reputation", ReputationExtension())
