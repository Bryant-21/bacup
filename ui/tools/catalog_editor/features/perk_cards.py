"""PerkCards: what each rank of a card costs and does, described from the perk records."""

from __future__ import annotations

from ui.tools.catalog_editor.features import FeatureExtension, RemoveWording, ResultView, register
from ui.tools.catalog_editor.model import REMOVED, Problem

FEATURE = "PerkCards"
SPECIAL = ("Strength", "Perception", "Endurance", "Charisma", "Intelligence", "Agility", "Luck")
RACES = {0: "everyone", 1: "humans only", 2: "ghouls only"}


def rank_perk(rank: dict) -> str:
    perk = rank.get("perk")
    return perk.get("form", "") if isinstance(perk, dict) else ""


def card_problems(entry: dict) -> list[str]:
    out = []
    for number, rank in enumerate(entry.get("ranks") or [], start=1):
        if not rank_perk(rank):
            out.append(f"Rank {number} grants no perk, so it does nothing.")
        if not rank.get("cost"):
            out.append(f"Rank {number} costs no SPECIAL points.")
    if not entry.get("ranks"):
        out.append("The card has no ranks.")
    return out


def card_summary(entry: dict) -> str:
    special = entry.get("special")
    stat = SPECIAL[special] if isinstance(special, int) and 0 <= special < len(SPECIAL) else "no SPECIAL stat"
    costs = "/".join(str(rank.get("cost", 0)) for rank in entry.get("ranks") or [])
    who = RACES.get(entry.get("race_restriction", 0), "everyone")
    return f"{stat} card for {who}, from level {entry.get('min_level', 0)}. Rank costs: {costs or 'none'}."


class PerkCardsExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != FEATURE or "cards" not in doc.sections():
            return []
        return [Problem(doc.name, "cards", row.key, message, blocking=False)
                for row in doc.rows("cards") if row.status != REMOVED and isinstance(row.entry, dict)
                for message in card_problems(row.entry)]

    def result_view(self, doc, section):
        return ResultView("Ranks", draw_ranks) if section == "cards" else None

    def sentence(self, view):
        return card_summary(view.entry) if view.section == "cards" and isinstance(view.entry, dict) else ""

    def main_list(self, doc, section):
        return ("ranks", "ranks") if section == "cards" else None

    def remove_wording(self, doc, section):
        if section != "cards":
            return None
        return RemoveWording("Stop offering the card", "Your plugin removes the card, so Tales doesn't offer it.",
                             "Removed: Tales doesn't offer this card.")


def draw_ranks(view):
    from imgui_bundle import imgui

    if not isinstance(view.entry, dict):
        return False, view.entry
    ranks = view.entry.get("ranks") or []
    if not ranks:
        imgui.text_disabled("The card has no ranks.")
        return False, view.entry
    flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h | imgui.TableFlags_.resizable
    if imgui.begin_table("##ranks", 5, flags):
        for title, weight in (("Rank", 0.5), ("Cost", 0.5), ("Name", 1.5), ("Perk", 1.5), ("Does", 4.0)):
            imgui.table_setup_column(title, imgui.TableColumnFlags_.width_stretch, weight)
        imgui.table_headers_row()
        for number, rank in enumerate(ranks, start=1):
            imgui.table_next_row()
            imgui.table_next_column()
            imgui.text(str(number))
            imgui.table_next_column()
            imgui.text(str(rank.get("cost", 0)))
            imgui.table_next_column()
            imgui.text(rank.get("name") or "(unnamed)")
            imgui.table_next_column()
            perk = rank_perk(rank)
            if perk:
                imgui.text(view.name_of(perk))
            else:
                imgui.text_disabled("no perk")
            imgui.table_next_column()
            imgui.text_wrapped(rank.get("description") or "")
        imgui.end_table()
    return False, view.entry


register(FEATURE, PerkCardsExtension())
