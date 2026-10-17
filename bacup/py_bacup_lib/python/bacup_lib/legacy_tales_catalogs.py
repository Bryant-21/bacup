import logging
from pathlib import Path

LOG = logging.getLogger(__name__)

_TALES_ROOT = "F4SE/Plugins/B21_TalesFromAppalachia"

# CampWallpapers.json is deliberately absent: Tales still reads it until that catalog migrates.
_LEGACY_NAMES = (
    "WeaponModelSwaps.json",
    "FoodSpoilage.json",
    "perk_card_extras.json",
    "Diseases.json",
    "challenges.json",
    "exploration_challenges.json",
    "combat_perks.json",
    "contextual_loot.json",
    "daily_ops_refs.json",
    "daily_ops_entrances.json",
    "daily_ops_voice.json",
    "emotes.json",
    "EquipmentBonusCurves.json",
    "PerkCards.json",
    "GoldBullion.json",
    "Stamps.json",
    "TadpoleBadges.json",
    "PossumBadges.json",
    "infestations.json",
    "LegendaryPerks.json",
    "player_ghoul.json",
    "player_mutations.json",
    "pets.json",
    "quest_categories.json",
    "realtime_vats.json",
    "reputation.json",
    "LeveledLists/SeventySix.json",
    "NpcStats/SeventySix.json",
    "ObjectiveAreas/B21_DailyOps.json",
)

LEGACY_TALES_CATALOGS = tuple(f"{_TALES_ROOT}/{name}" for name in _LEGACY_NAMES)

# Catalogs the Tales companion mod itself ships (its deploy cleans these, not BACUP's emitters).
LEGACY_TALES_SHIPPED_CATALOGS = tuple(
    f"{_TALES_ROOT}/{name}"
    for name in (
        "exploration_challenges.json",
        "contextual_loot.json",
        "daily_ops_refs.json",
        "daily_ops_entrances.json",
        "player_mutations.json",
        "ObjectiveAreas/B21_DailyOps.json",
    )
)


def remove_legacy_tales_catalogs(root: Path) -> list[Path]:
    removed: list[Path] = []
    for relative in LEGACY_TALES_CATALOGS:
        path = Path(root) / relative
        if path.is_file():
            path.unlink()
            removed.append(path)
    if removed:
        LOG.info("removed %s legacy Tales catalog(s) under %s", len(removed), root)
    return removed
