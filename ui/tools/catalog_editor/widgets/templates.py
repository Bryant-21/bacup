"""Plain-language checks for the condition builder, and sentences for any catalog condition.

A check is a template over one Tales condition function: who it is about, what to pick, and how to
compare. Conditions built here are ordinary catalog conditions, so the raw editor and Tales read
them unchanged; `match` recognizes them again when a catalog is reopened.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

from ui.tools.catalog_editor.widgets.functions import BY_ID, INVENTORY

PLAYER_REF = "Fallout4.esm:000014"  # PlayerRef: built by the engine, so no plugin carries the record

# Who a check is about -> how Tales runs it. "player" names PlayerRef, so it works for item and actor lists.
WHO = {
    "player": "the player",
    "self": "the container or actor",
    "reference": "a placed reference",
}

BOOL = "bool"
NUMBER = "number"
DAY = "day"
SEX = "sex"
PERCENT = "percent"

NUMBER_WORDS = (("is", "=="), ("is not", "!="), ("is at least", ">="), ("is at most", "<="),
                ("is more than", ">"), ("is less than", "<"))
BOOL_WORDS = (("yes", True), ("no", False))
DAYS = ("Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday")

PLAYER = "Player"
SELF = "This container or actor"
LOCATION = "Location"
TIME = "Time and weather"
QUESTS = "Quests and globals"
CHANCE = "Chance"
TALES = "FO76 / Tales"
CATEGORIES = (PLAYER, SELF, LOCATION, TIME, QUESTS, CHANCE, TALES)


@dataclass(frozen=True)
class Template:
    id: str
    category: str
    text: str  # "{who} has perk {form}" / "{who}'s level {cmp} {value}"
    function: int
    kind: str = BOOL
    signatures: tuple[str, ...] = ()  # the form to pick; empty = none
    who: tuple[str, ...] = ("self",)  # allowed, first is the default
    value: float = 1.0
    extra: str = ""  # a second, numeric parameter ("stage")

    @property
    def picks_form(self) -> bool:
        return bool(self.signatures)


def _t(*args, **kwargs) -> Template:
    return Template(*args, **kwargs)


ANYONE = ("player", "self", "reference")
TEMPLATES: tuple[Template, ...] = (
    _t("perk", PLAYER, "{who} has perk {form}", 448, signatures=("PERK",), who=ANYONE),
    _t("level", PLAYER, "{who}'s level {cmp} {value}", 80, NUMBER, who=ANYONE, value=10),
    _t("actor_value", PLAYER, "{who}'s {form} {cmp} {value}", 14, NUMBER, ("AVIF",), ANYONE, value=5),
    _t("item_count", PLAYER, "{who} carries {form}: count {cmp} {value}", 47, NUMBER, INVENTORY, ANYONE, value=1),
    _t("equipped", PLAYER, "{who} has {form} equipped", 182, signatures=INVENTORY, who=ANYONE),
    _t("worn_keyword", PLAYER, "{who} wears something with {form}", 682, signatures=("KYWD", "FLST"), who=ANYONE),
    _t("magic_effect", PLAYER, "{who} is under effect {form}", 854, signatures=("MGEF",), who=ANYONE),
    _t("ghoul", PLAYER, "{who} is a ghoul (Tales player ghoul)", 10017, who=("player",)),
    _t("keyword", SELF, "{who} has keyword {form}", 560, signatures=("KYWD",), who=ANYONE[1:] + ANYONE[:1]),
    _t("is_record", SELF, "{who} is a {form}", 72, signatures=("NPC_", "CONT", "ACTI", "FLOR", "TREE", "FURN"),
       who=("self", "reference")),
    _t("race", SELF, "{who}'s race is {form}", 69, signatures=("RACE",), who=ANYONE[1:] + ANYONE[:1]),
    _t("sex", SELF, "{who} is {value}", 70, SEX, who=ANYONE[1:] + ANYONE[:1]),
    _t("ref_type", SELF, "{who} has location ref type {form}", 561, signatures=("LCRT",), who=("self", "reference")),
    _t("locked", SELF, "{who} is locked", 5, who=("self", "reference")),
    _t("lock_level", SELF, "{who}'s lock level {cmp} {value}", 65, NUMBER, who=("self", "reference"), value=25),
    _t("distance", SELF, "{who} is {cmp} {value} units from {form}", 1, NUMBER, ("REFR", "ACHR"),
       ("self", "player"), value=2048),
    _t("interior", LOCATION, "{who} is indoors", 300, who=("self", "player")),
    _t("in_location", LOCATION, "{who} is in location {form}", 359, signatures=("LCTN",), who=("self", "player")),
    _t("editor_location", LOCATION, "{who} was placed in location {form}", 565, signatures=("LCTN",), who=("self",)),
    _t("location_keyword", LOCATION, "{who}'s location has keyword {form}", 562, signatures=("KYWD",),
       who=("self", "player")),
    _t("location_ref_type", LOCATION, "{who}'s location has ref type {form}", 563, signatures=("LCRT",),
       who=("self", "player")),
    _t("workshop", LOCATION, "{who}'s location has a player-owned workshop", 788, who=("self", "player")),
    _t("location_keyword_76", LOCATION, "{who}'s location or a parent has keyword {form} (FO76)", 8007,
       signatures=("KYWD",), who=("self",)),
    _t("night", TIME, "it is night", 765, who=("self",)),
    _t("day", TIME, "it is {value}", 170, DAY, who=("self",), value=0),
    _t("weather", TIME, "the weather is {form}", 149, signatures=("WTHR", "FLST"), who=("self",)),
    _t("quest_done", QUESTS, "quest {form} is completed", 543, signatures=("QUST",), who=("player",)),
    _t("quest_running", QUESTS, "quest {form} is running", 856, signatures=("QUST",), who=("player",)),
    _t("quest_stage", QUESTS, "quest {form}'s stage {cmp} {value}", 58, NUMBER, ("QUST",), ("player",), value=10),
    _t("stage_done", QUESTS, "quest {form} has done stage {stage}", 833, signatures=("QUST",), who=("player",),
       extra="stage"),
    _t("quest_times", QUESTS, "quest {form} was completed {cmp} {value} times", 857, NUMBER, ("QUST",), ("player",)),
    _t("global", QUESTS, "global {form} {cmp} {value}", 74, NUMBER, ("GLOB",), ("player",)),
    _t("recipe", QUESTS, "the player has learned recipe {form}", 853, signatures=("COBJ",), who=("player",)),
    _t("random", CHANCE, "{value}% chance", 77, PERCENT, who=("self",), value=50),
    _t("daily_ops", TALES, "a Daily Op is running", 5002, who=("self",)),
    _t("location_keyword_ess", TALES, "{who}'s location has keyword {form} (incl. Tales' ESS rolls)", 579,
       signatures=("KYWD",), who=("self",)),
)
BY_TEMPLATE = {template.id: template for template in TEMPLATES}

DescribeFn = Callable[[str], str]


def _who_fields(who: str, reference: str = "") -> dict:
    if who == "player":
        return {"run_on": "reference", "reference": PLAYER_REF}
    if who == "reference":
        return {"run_on": "reference", "reference": reference}
    return {"run_on": "subject"}


def build(template: Template, who: str | None = None) -> dict:
    """A new catalog condition for `template`; the form and value are filled in afterwards."""
    who = who or template.who[0]
    condition = {"function": template.function, "op": "==", "value": template.value, **_who_fields(who),
                 "params": [None, None]}
    if template.kind == PERCENT:
        condition["op"] = "<"
    elif template.kind == NUMBER:
        condition["op"] = ">="
    if template.extra == "stage":
        condition["params"][1] = {"raw": 10}
    if template.kind == SEX:
        condition["params"][0] = {"raw": 0}
    return condition


def sex_of(condition: dict) -> int:
    param = (condition.get("params") or [None])[0]
    return int(param.get("raw", 0)) if isinstance(param, dict) else 0


def set_sex(condition: dict, female: bool) -> None:
    condition.setdefault("params", [None, None])[0] = {"raw": 1 if female else 0}


def who_of(condition: dict) -> str | None:
    run_on = condition.get("run_on")
    if run_on == "subject":
        return "self"
    if run_on == "reference":
        return "player" if condition.get("reference") == PLAYER_REF else "reference"
    return None


def set_who(condition: dict, who: str) -> None:
    reference = condition.get("reference", "") if who == "reference" and who_of(condition) == "reference" else ""
    condition.pop("reference", None)
    condition.update(_who_fields(who, reference))
    if not condition.get("reference"):
        condition.pop("reference", None)


def match(condition: dict) -> Template | None:
    """The builder check this condition was made with, or None (shown in the raw editor)."""
    who = who_of(condition)
    if who is None or condition.get("swap") or condition.get("global"):
        return None
    for template in TEMPLATES:
        if template.function == condition.get("function") and who in template.who:
            if template.kind == PERCENT and condition.get("op") not in ("<", "<="):
                continue
            if template.kind in (BOOL, DAY, SEX) and condition.get("op") not in ("==", "!="):
                continue
            return template
    return None


def form_of(condition: dict) -> str:
    param = (condition.get("params") or [None])[0]
    return param.get("form", "") if isinstance(param, dict) else ""


def set_form(condition: dict, key: str) -> None:
    params = condition.setdefault("params", [None, None])
    params[0] = {"source": key, "form": key} if key else None


def truth(condition: dict) -> bool:
    """A yes/no check's answer: `== 1` and `!= 0` mean yes."""
    value = float(condition.get("value", 1))
    return (value != 0) == (condition.get("op", "==") == "==")


def set_truth(condition: dict, yes: bool) -> None:
    condition["op"] = "=="
    condition["value"] = 1.0 if yes else 0.0


def compare_word(condition: dict) -> str:
    return next((word for word, op in NUMBER_WORDS if op == condition.get("op")), condition.get("op", "=="))


def sentence(condition: dict, describe: DescribeFn) -> str:
    """Plain words for any catalog condition, builder-made or not."""
    template = match(condition)
    form = form_of(condition)
    form_text = describe(form).split(" [", 1)[0] if form else "(pick one)"
    who = who_of(condition)
    who_text = {"player": "the player", "self": "this container/actor"}.get(
        who or "", describe(condition.get("reference", "")).split(" [", 1)[0] if condition.get("reference") else "?")
    if template is None:
        function = BY_ID.get(condition.get("function"))
        name = function.name if function else f"function {condition.get('function')}"
        target = condition.get("global") and f"global {describe(condition['global']).split(' [', 1)[0]}"
        compared = target or f"{condition.get('value', 0):g}"
        on = {"target": "target", "subject": "subject"}.get(condition.get("run_on"), who_text)
        return f"{name}({form_text if form else ''}) on {on} {condition.get('op', '==')} {compared}"
    value = float(condition.get("value", 0))
    stage = (condition.get("params") or [None, None])[1]
    text = template.text.format(
        who=who_text, form=form_text, cmp=compare_word(condition),
        value=(DAYS[int(value) % 7] if template.kind == DAY else
               ("female" if sex_of(condition) else "male") if template.kind == SEX else f"{value:g}"),
        stage=(stage or {}).get("raw", 0) if isinstance(stage, dict) else 0,
    )
    text = text[0].upper() + text[1:]
    if template.kind in (BOOL, DAY, SEX) and not truth(condition):
        text = f"NOT: {text}"
    return text


def chain_mode(rows: list) -> str:
    """'all' (every row must pass), 'any' (one is enough) or 'mixed'; FO76's OR binds a row to the next."""
    if len(rows) < 2:
        return "all"
    flags = [bool(row.get("or")) for row in rows[:-1]]
    if not any(flags):
        return "all"
    if all(flags):
        return "any"
    return "mixed"


def set_chain_mode(rows: list, mode: str) -> None:
    for index, row in enumerate(rows):
        if mode == "any" and index < len(rows) - 1:
            row["or"] = True
        else:
            row.pop("or", None)


# What the builder's "Add a check" list calls each check.
TITLES = {
    "perk": "Has a perk", "level": "Level", "actor_value": "Actor value (SPECIAL, health...)",
    "item_count": "Carries an item (count)", "equipped": "Has an item equipped", "worn_keyword": "Wears an item with a keyword",
    "magic_effect": "Is under a magic effect", "ghoul": "Is a ghoul (Tales)", "keyword": "Has a keyword",
    "is_record": "Is a specific actor/container/object", "race": "Race is", "sex": "Is male / female",
    "ref_type": "Has a location ref type", "locked": "Is locked", "lock_level": "Lock level",
    "distance": "Distance to a placed reference", "interior": "Is indoors", "in_location": "Is in a location",
    "editor_location": "Was placed in a location", "location_keyword": "Location has a keyword",
    "location_ref_type": "Location has a ref type", "workshop": "Location has a player-owned workshop",
    "location_keyword_76": "Location or a parent has a keyword (FO76)", "night": "It is night",
    "day": "Day of the week", "weather": "Weather is", "quest_done": "Quest completed", "quest_running": "Quest running",
    "quest_stage": "Quest stage", "stage_done": "Quest stage done", "quest_times": "Quest completed N times",
    "global": "Global value", "recipe": "Recipe learned", "random": "Random % chance",
    "daily_ops": "A Daily Op is running (Tales)", "location_keyword_ess": "Location has a keyword, incl. ESS rolls (FO76)",
}
