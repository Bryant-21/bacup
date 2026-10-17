"""GoldBullion: who sells for each currency and price warnings."""

from __future__ import annotations

from ui.tools.catalog_editor.features import FeatureExtension, RemoveWording, ResultView, register
from ui.tools.catalog_editor.model import REMOVED, Problem

FEATURE = "GoldBullion"
HOLDING_LIMIT = 10000
HIGH_PRICE = 100000
VENDORS = {
    "bullion": "Regs, Mortimer, Samuel, Minerva (25% discount) and Windy take gold bullion.",
    "stamps": "Giuseppe's stamp shop takes stamps.",
    "tadpole": "The tadpole vendors take tadpole badges.",
    "possum": "The possum vendors take possum badges.",
}


def price_warning(section: str, price) -> str | None:
    if not isinstance(price, int) or isinstance(price, bool):
        return None
    if section == "bullion" and price > HOLDING_LIMIT:
        return f"Costs more than the {HOLDING_LIMIT:,} bullion a player can hold, so nobody can buy it."
    if price > HIGH_PRICE:
        return f"{price:,} is unusually high."
    return None


def discounted(price: int, discount: float = 0.25) -> int:
    return round(price * (1 - discount))


class GoldBullionExtension(FeatureExtension):
    def problems(self, doc, forms):
        if doc.name != FEATURE:
            return []
        out = []
        for section in VENDORS:
            if section not in doc.sections():
                continue
            for row in doc.rows(section):
                if row.status == REMOVED or not isinstance(row.entry, dict):
                    continue
                message = price_warning(section, row.entry.get("price"))
                if message:
                    out.append(Problem(doc.name, section, row.key, message, blocking=False))
        return out

    def result_view(self, doc, section):
        return ResultView("Who sells it", draw_vendors) if section in VENDORS else None

    def sentence(self, view):
        if view.section not in VENDORS or not isinstance(view.entry, dict):
            return ""
        price = view.entry.get("price")
        return f"Sold for {price:,} {view.section}." if isinstance(price, int) else ""

    def remove_wording(self, doc, section):
        if section not in VENDORS:
            return None
        return RemoveWording("Stop selling it", f"Your plugin removes the item from the {section} vendors' list, so "
                             "they don't sell it.", f"Removed: the {section} vendors don't sell it.")


def draw_vendors(view):
    from imgui_bundle import imgui

    from creation_lib.ui.widgets.modern import semantic_color

    if not isinstance(view.entry, dict):
        return False, view.entry
    imgui.text_wrapped(VENDORS[view.section] + " Items missing from this list are not sold for it.")
    price = view.entry.get("price")
    if isinstance(price, int):
        imgui.text(f"Price: {price:,}")
        if view.section == "bullion":
            imgui.text_disabled(f"Minerva charges {discounted(price):,} after her 25% discount.")
    message = price_warning(view.section, price)
    if message:
        imgui.text_colored(semantic_color("warning"), message)
    return False, view.entry


register(FEATURE, GoldBullionExtension())
