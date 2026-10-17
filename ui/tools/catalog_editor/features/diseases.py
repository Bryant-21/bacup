"""Diseases: who can give each disease, what cures them, and warnings when a disease is a dead end."""

from __future__ import annotations

from ui.tools.catalog_editor.features import FeatureExtension, ResultView, register
from ui.tools.catalog_editor.features.leveled_lists import _canon
from ui.tools.catalog_editor.model import REMOVED, FeatureDocument, Problem, Row

FEATURE = "Diseases"
_PLACES = {"water": "water", "bed": "a bed", "hazard": "toxic air", "trap": "a trap"}


def _entries(doc: FeatureDocument, section: str) -> list[dict]:
    if section not in doc.sections():
        return []
    return [row.entry for row in doc.rows(section) if row.status != REMOVED and isinstance(row.entry, dict)]


def sources_of(doc: FeatureDocument, disease: str) -> list[str]:
    """Plain-language ways the player can catch `disease` (a spell FormKey)."""
    key = _canon(disease)
    out = []
    for entry in _entries(doc, "chance_rows"):
        if _canon(entry.get("disease", "")) == key:
            out.append(f"{_PLACES.get(entry.get('type'), 'a place')}: {entry.get('chance', 0):g}% base chance ({entry.get('risk', '')})")
    for entry in _entries(doc, "hits"):
        if _canon(entry.get("disease", "")) == key:
            who = entry.get("aggressor") or ", ".join(entry.get("aggressor_races") or []) or "any creature"
            out.append(f"hit by {who}: {entry.get('chance', 0):g}% base chance")
    for entry in _entries(doc, "vectors"):
        if key in {_canon(item) for item in entry.get("diseases") or []}:
            out.append(f"{entry.get('kind', 'item')} with effect {entry.get('effect', '')}")
    return out


def cure_count(doc: FeatureDocument) -> int:
    return len(_entries(doc, "cures"))


def problem_messages(doc: FeatureDocument) -> list[tuple[str | None, str | None, str]]:
    out: list[tuple[str | None, str | None, str]] = []
    diseases = [row for row in doc.rows("diseases") if row.status != REMOVED] if "diseases" in doc.sections() else []
    for row in diseases:
        if not sources_of(doc, row.entry.get("spell", row.key)):
            out.append(("diseases", row.key, "Nothing gives this disease. Add a water, bed or trap chance, a creature hit, "
                                              "or a food or drink effect that lists it."))
    if diseases and not cure_count(doc):
        out.append(("cures", None, "There are diseases but no cure effects, so nothing can cure them early."))
    if _entries(doc, "carriers") and not _entries(doc, "hits"):
        out.append(("carriers", None, "Creatures can be diseased but no creature hit gives a disease."))
    return out


class DiseasesExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != FEATURE:
            return []
        return [Problem(doc.name, section, key, message, blocking=False) for section, key, message in problem_messages(doc)]

    def draw_detail(self, tool, doc, section, row: Row) -> None:
        from imgui_bundle import imgui

        if row.status == REMOVED or not isinstance(row.entry, dict):
            return
        if section == "diseases":
            imgui.text_wrapped("The disease ends by itself after its duration, then can't return for the immunity time.")
        elif section == "chance_rows":
            imgui.text_wrapped("Each roll uses this base chance, made smaller by the player's Endurance, resistance "
                               "and protective gear (see Settings), never below 1%.")
        elif section == "hits":
            imgui.text_wrapped("When this creature hits the player and is a carrier, the base chance is scaled by its level "
                               "(see carrier settings) and capped at 85%.")

    def result_view(self, doc, section):
        return ResultView("How it is caught", draw_caught) if section == "diseases" else None

    def sentence(self, view):
        if view.section != "diseases" or not isinstance(view.entry, dict):
            return ""
        ways = sources_of(view.doc, view.entry.get("spell", view.row.key))
        return (f"The player can catch this in {len(ways)} way(s); {cure_count(view.doc)} cure effect(s) remove it early."
                if ways else "Nothing gives this disease yet.")


def draw_caught(view):
    from imgui_bundle import imgui

    from creation_lib.ui.widgets.modern import semantic_color

    if not isinstance(view.entry, dict):
        return False, view.entry
    ways = sources_of(view.doc, view.entry.get("spell", view.row.key))
    if not ways:
        imgui.text_colored(semantic_color("warning"), "Nothing gives this disease. Add a water, bed or trap chance, "
                           "a creature hit, or a food or drink effect that lists it.")
    for line in ways:
        imgui.bullet_text(line)
    imgui.spacing()
    imgui.text_wrapped(f"{cure_count(view.doc)} cure effect(s) can remove diseases early.")
    return False, view.entry


register(FEATURE, DiseasesExtension())
