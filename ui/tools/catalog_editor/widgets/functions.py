"""Condition functions Tales answers in a leveled-list catalog.

Ids are FO76's numbering, as Tales reads them (``Fo76Conditions.h`` ``ClassifyFunction``). Fallout 4
names and parameter types come from Fallout4.exe's script function table; FO76 names from the Tales
RE notes (``docs/re/LEVELED_LISTS.md``). Where a FO76 name isn't known the label says what it does.
"""

from __future__ import annotations

from dataclasses import dataclass

FALLOUT4 = "Fallout 4"
FO76 = "FO76 / needs Tales systems"

INVENTORY = ("ALCH", "AMMO", "ARMO", "BOOK", "CMPO", "FLST", "INGR", "KEYM", "LVLI", "MISC", "NOTE", "OMOD", "WEAP")
REFERENCE = ("REFR", "ACHR")


@dataclass(frozen=True)
class Param:
    name: str
    kind: str  # "form", "int", "sex", "condition_form", "region" or "fo76_key"
    signatures: tuple[str, ...] = ()


@dataclass(frozen=True)
class Function:
    id: int
    name: str
    group: str
    params: tuple[Param, ...] = ()
    note: str = ""


def _form(name: str, *signatures: str) -> Param:
    return Param(name, "form", signatures)


QUEST = _form("Quest", "QUST")
KEYWORD = _form("Keyword", "KYWD")
LOCATION = _form("Location", "LCTN")
REF_TYPE = _form("RefType", "LCRT")

FUNCTIONS: tuple[Function, ...] = (
    Function(1, "GetDistance", FALLOUT4, (_form("Reference", *REFERENCE),)),
    Function(5, "GetLocked", FALLOUT4),
    Function(14, "GetValue", FALLOUT4, (_form("Actor value", "AVIF"),)),
    Function(47, "GetItemCount", FALLOUT4, (_form("Item", *INVENTORY),)),
    Function(58, "GetStage", FALLOUT4, (QUEST,)),
    Function(65, "GetLockLevel", FALLOUT4),
    Function(69, "GetIsRace", FALLOUT4, (_form("Race", "RACE"),)),
    Function(70, "GetIsSex", FALLOUT4, (Param("Sex", "sex"),)),
    Function(72, "GetIsID", FALLOUT4, (_form("Object"),), "An empty parameter answers 0, as FO76 does."),
    Function(74, "GetGlobalValue", FALLOUT4, (_form("Global", "GLOB"),), "An empty parameter answers -1, as FO76 does."),
    Function(77, "GetRandomPercent", FALLOUT4),
    Function(80, "GetLevel", FALLOUT4),
    Function(149, "GetIsCurrentWeather", FALLOUT4, (_form("Weather", "WTHR", "FLST"), Param("Region (FO76)", "region")),
             "With a region, true while the player is inside it and the weather matches."),
    Function(170, "GetDayOfWeek", FALLOUT4),
    Function(182, "GetEquipped", FALLOUT4, (_form("Item", *INVENTORY),)),
    Function(300, "IsInInterior", FALLOUT4),
    Function(359, "GetInCurrentLocation", FALLOUT4, (LOCATION,), "An empty parameter answers 0, as FO76 does."),
    Function(448, "HasPerk", FALLOUT4, (_form("Perk", "PERK"),)),
    Function(543, "GetQuestCompleted", FALLOUT4, (QUEST,)),
    Function(560, "HasKeyword", FALLOUT4, (KEYWORD,)),
    Function(561, "HasRefType", FALLOUT4, (REF_TYPE,)),
    Function(562, "LocationHasKeyword", FALLOUT4, (KEYWORD,)),
    Function(563, "LocationHasRefType", FALLOUT4, (REF_TYPE,)),
    Function(565, "GetIsEditorLocation", FALLOUT4, (LOCATION,)),
    Function(682, "WornHasKeyword", FALLOUT4, (_form("Keyword or list", "KYWD", "FLST"),)),
    Function(788, "LocationHasPlayerOwnedWorkshop", FALLOUT4),
    # FO76 ids Tales answers with a Fallout 4 function.
    Function(276, "IsInInteriorAcousticSpace (as IsInInterior)", FO76),
    Function(828, "GetIsPlayer", FO76),
    Function(833, "FO76 833 (as GetStageDone)", FO76, (QUEST, Param("Stage", "int"))),
    Function(837, "IsActivePlayer", FO76),
    Function(844, "GetIsCurrentLocationExact (as GetInCurrentLocation)", FO76, (LOCATION,)),
    Function(854, "HasActiveMagicEffect", FO76, (_form("Magic effect", "MGEF"),)),
    Function(856, "GetQuestRunningUnique", FO76, (QUEST,)),
    Function(865, "IsInWeather", FO76, (_form("Weather", "WTHR"),)),
    Function(876, "IsQuestActive", FO76, (QUEST,)),
    Function(917, "GetStageDonePlayerInstance", FO76, (QUEST, Param("Stage", "int"))),
    Function(5004, "FO76 5004 (as GetQuestRunning)", FO76, (QUEST,)),
    Function(10016, "GetIsDisguisedWithKeyword (as WornHasKeyword)", FO76, (_form("Keyword or list", "KYWD", "FLST"),)),
    # Tales providers.
    Function(579, "EditorLocationHasKeyword", FO76, (KEYWORD,), "Includes keywords Tales' ESS roll added."),
    Function(737, "Current weather has keyword", FO76, (Param("FO76 weather keyword", "fo76_key"),),
             "Reads fishing's FO76 weather table; only FO76 keywords match."),
    Function(765, "IsTimeSpanAnyNight", FO76),
    Function(846, "GetActorValueForCurrentLocation", FO76, (Param("Actor value", "fo76_key"),),
             "Only SQ_VerdantSeasonAV (SeventySix 00129D), always 0."),
    Function(849, "GetIsInRegion", FO76, (Param("Region", "region"),), "FO76 regions only; nuke regions follow live zones."),
    Function(853, "HasLearnedRecipe", FO76, (_form("Recipe", "COBJ"),), "An empty parameter answers 0, as FO76 does."),
    Function(857, "GetNumTimesCompletedQuest", FO76, (QUEST,)),
    Function(875, "IsTrueForConditionForm", FO76, (Param("Condition form", "condition_form"),),
             "Run on the subject or target only."),
    Function(906, "HasCompletedChallenge", FO76, (Param("FO76 challenge", "fo76_key"),)),
    Function(938, "HasActiveChallenge", FO76, (Param("FO76 challenge", "fo76_key"),)),
    Function(5002, "GetIsInDailyOps", FO76),
    Function(8007, "LocationHierarchyHasKeyword", FO76, (KEYWORD,)),
    Function(10017, "GetIsPlayerGhoul", FO76),
    # Fixed answers.
    Function(859, "HasEntitlement", FO76, note="Always 1."),
    Function(898, "GetWorldType", FO76, note="Always 1 (Adventure)."),
    Function(915, "IsPlayerFO1Member", FO76, note="Always 1."),
    Function(916, "IsServerOverEWSActorBudget", FO76, note="Always 0."),
    Function(936, "CAMP weather station has keyword", FO76, note="Always 0."),
    Function(940, "HasActivePet", FO76, note="Always 0."),
    Function(941, "GetPetLevel", FO76, note="Always 0."),
    Function(5006, "GetIsInExpedition", FO76, note="Always 0."),
    Function(5007, "CAMP weather station present", FO76, note="Always 0."),
    Function(8005, "IsFishingBaitSelected", FO76, note="Always 0."),
    Function(10013, "GetPublicEventHasMutation", FO76, note="Always 0."),
    Function(11001, "LocationHasEventQuest", FO76, note="Always 0."),
)

BY_ID: dict[int, Function] = {function.id: function for function in FUNCTIONS}


def label(function_id: int) -> str:
    function = BY_ID.get(function_id)
    return f"{function.name} ({function_id})" if function else f"Unsupported ({function_id})"

# Ids Tales answers itself (Fo76Conditions.h providers), as opposed to through a Fallout 4 function.
TALES_PROVIDERS = frozenset((579, 737, 765, 846, 8007, 849, 853, 857, 875, 906, 938, 5002, 10017))
ANSWERED_FO4, ANSWERED_TALES, FALLS_BACK = "Fallout 4", "Tales", "falls back"


def answered_by(function_id) -> str:
    """Who answers a check in game: Fallout 4's condition, Tales, or nobody (the whole list falls back)."""
    function = BY_ID.get(function_id)
    if function is None:
        return FALLS_BACK
    if function_id in TALES_PROVIDERS or function.note.startswith("Always"):
        return ANSWERED_TALES
    return ANSWERED_FO4


ANSWER_ROLES = {ANSWERED_FO4: "success", ANSWERED_TALES: "accent", FALLS_BACK: "error"}
