from __future__ import annotations

import csv
import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
PATCH_ROOT = REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"

BATCH_MANIFEST = {
    "a": {
        "AliasSendStoryEventOnActivate", "AudioActivator1State",
        "BloodEagleSpotterAlarmActivatorScript", "DefaultActivateObjectsOnActivate",
        "DefaultActorAlphaOnActivate", "DefaultDestructible2StateActivator",
        "DefaultExplosionOnTriggerEnter", "DefaultPlayClientSoundOnActivate",
        "DefaultPlaySoundOnActivateAlias", "DefaultTopicInfoTriggerCombat",
        "DefaultCompleteChallengeOnActivate", "DefaultFishingActivator",
        "DefaultLightningQuestTrigger", "DefaultOnTriggerEnterAddToMap",
    },
    "b": {
        "DefaultAliasOnActivateB", "DefaultAliasOnActivateC", "DefaultAliasOnActivateD",
        "DefaultAliasOnActivateE", "DefaultAliasOnActivateGiveItem",
        "DefaultAliasOnActivatePlayerSayCustom", "DefaultAliasOnActivateRemoveItems",
        "DefaultAliasOnActivateRemoveItemsA", "DefaultAliasSetStageOnKeypadSuccess",
        "DefaultAliasOnTriggerEnterA", "DefaultAliasOnTriggerEnterB",
        "DefaultAliasOnTriggerEnterMultiActor", "DefaultAliasOnTriggerLeaveA",
        "DefaultAliasTriggerEnterShowTutorial",
    },
    "c": {
        "DefaultCommunityDepositActivator", "DefaultActivatorVendorFactionScript",
        "DefaultOnActivateGiveItems", "DefaultOnActivateChangePrompt",
        "DefaultApplyDiseaseOnTriggerEnter", "DefaultChallengeMessageOnActivateAlias",
        "DefaultChallengeMessageOnActivateColl", "DefaultChallengeMessageOnActivateRef",
        "DefaultCollAliasOnActivateGiveItems", "DefaultCollAliasSendEventOnActivate",
        "DefaultCollectionAliasOnActivateGive", "DefaultOnActivateRemoveItemSetStages",
        "DefaultQuestTriggerRespawnVIPScript", "DefaultTriggerThrottledEventScript",
    },
    "d": {
        "DefaultKeypadTargetScript", "DefaultRefKillTriggerScript",
        "DefaultTriggerRespawnActorGroup", "DefaultKeypadScript",
        "defaultkeypadcontainerscript", "defaultkeypaddoorscript",
        "DefaultKeypadSwitchDoorScript", "DefaultKeypadTimedSwitchScript",
        "DefaultEMSBossTriggerScript", "DefaultTriggerEncounterWaveQuestScript",
        "DefaultTriggerEncounterWaveScript", "defaultrefontriggerleavesendevent",
        "DefaultRefOnTriggerEnterSendEvent", "DefaultRefOnActivateSendEvent",
    },
    "e": {
        "Default2StateSyncActivator", "DefaultDestructibleMultiStateActivator",
        "DefaultFixable2StateActivator", "DefaultMultiStateActivator",
        "DefaultMultiStateClientSideActivator", "DefaultOnActivateAnimate",
        "DefaultExplosionOnActivate", "DefaultPlayExplosionOnActivate",
        "DefaultPlayExposionAtNodeOnActivate", "DefaultPlaySoundOnActivate",
        "DefaultSequentialStateActivator", "defaultunlockandopenlinkonactivate",
    },
}

EXPECTED_DEFERRED = {
    "AliasSendStoryEventOnActivate", "BloodEagleSpotterAlarmActivatorScript",
    "DefaultDestructible2StateActivator", "DefaultExplosionOnTriggerEnter",
    "DefaultCompleteChallengeOnActivate", "DefaultFishingActivator",
    "DefaultLightningQuestTrigger", "DefaultAliasOnActivateGiveItem",
    "DefaultAliasSetStageOnKeypadSuccess", "DefaultActivatorVendorFactionScript",
    "DefaultOnActivateChangePrompt", "DefaultApplyDiseaseOnTriggerEnter",
    "DefaultChallengeMessageOnActivateAlias", "DefaultChallengeMessageOnActivateColl",
    "DefaultChallengeMessageOnActivateRef", "DefaultCollAliasOnActivateGiveItems",
    "DefaultCollectionAliasOnActivateGive", "DefaultQuestTriggerRespawnVIPScript",
    "DefaultTriggerThrottledEventScript", "DefaultTriggerRespawnActorGroup",
    "DefaultKeypadScript", "defaultkeypaddoorscript", "DefaultKeypadTimedSwitchScript",
    "DefaultEMSBossTriggerScript", "DefaultTriggerEncounterWaveQuestScript",
    "DefaultTriggerEncounterWaveScript", "DefaultRefOnActivateSendEvent",
    "DefaultMultiStateActivator",
}

MARKER_PATCHES = {
    "DefaultActivatorVendorFactionScript": PATCH_ROOT / "DefaultActivatorVendorFactionScript.psc",
    "DefaultOnActivateChangePrompt": PATCH_ROOT / "DefaultOnActivateChangePrompt.psc",
    "DefaultTriggerRespawnActorGroup": PATCH_ROOT / "DefaultTriggerRespawnActorGroup.psc",
    "DefaultRefOnActivateSendEvent": PATCH_ROOT / "DefaultRefOnActivateSendEvent.psc",
    "DefaultMultiStateActivator": PATCH_ROOT / "defaultmultistateactivator.psc",
}

BEHAVIORAL_PATCHES = {
    PATCH_ROOT / "DefaultTopicInfoTriggerCombat.psc",
    PATCH_ROOT / "DefaultRefOnActivateSendEvent.psc",
    PATCH_ROOT / "DefaultMultiStateClientSideActivator.psc",
    PATCH_ROOT / "DefaultSequentialStateActivator.psc",
}


def _contract(batch: str) -> Path:
    return DOCS / "contracts" / f"ad-hoc-default-script-repairs-batch-{batch}.md"


def _contract_targets(batch: str) -> set[str]:
    text = _contract(batch).read_text(encoding="utf-8")
    expected_by_lower = {script.lower() for script in BATCH_MANIFEST[batch]}
    targets = {
        candidate
        for candidate in re.findall(r"^### ([A-Za-z][A-Za-z0-9]+)", text, re.MULTILINE)
        if candidate.lower() in expected_by_lower
    }
    for line in text.splitlines():
        line = line.lstrip()
        if not line.startswith("|"):
            continue
        candidate = line.split("|", 2)[1].strip().strip("`")
        if candidate.lower() in expected_by_lower:
            targets.add(candidate)
    return targets


def _adhoc_entries() -> list[dict[str, str]]:
    lines = (DOCS / "TODO.md").read_text(encoding="utf-8").splitlines()
    entries: list[dict[str, str]] = []
    for line in lines:
        if not line.startswith("ADHOC-DEFAULT|"):
            continue
        fields = {}
        for field in line.split("|")[1:]:
            key, value = field.split("=", 1)
            fields[key] = value
        entries.append(fields)
    return entries


def test_all_five_contracts_and_checklists_exist_and_account_for_the_68_target_manifest():
    all_targets: set[str] = set()
    for batch, expected in BATCH_MANIFEST.items():
        assert _contract(batch).is_file()
        assert (DOCS / "checklists" / f"ad-hoc-default-script-repairs-batch-{batch}.md").is_file()
        contract_targets = {target.lower() for target in _contract_targets(batch)}
        expected_targets = {target.lower() for target in expected}
        assert contract_targets == expected_targets
        assert all_targets.isdisjoint(expected_targets)
        all_targets |= expected_targets

    assert len(all_targets) == 68


def test_adhoc_registry_captures_only_the_reviewed_deferred_surfaces_with_closure_data():
    entries = _adhoc_entries()
    scripts = {entry["script"] for entry in entries}
    assert scripts == EXPECTED_DEFERRED
    assert len(entries) == len(scripts) == 28

    valid_statuses = {"evidence-blocked", "record-dependency", "unsupported-online"}
    for entry in entries:
        assert entry["contract"].startswith("contracts/ad-hoc-default-script-repairs-batch-")
        assert entry["evidence"] == entry["contract"]
        assert entry["status"] in valid_statuses
        assert entry["blocker"]
        assert entry["removal"]
        assert entry["patch"] == "none" or (REPO_ROOT / entry["patch"]).is_file()


def test_marker_counts_and_registry_metadata_stay_synchronized_without_hollow_patches():
    entries = {entry["script"]: entry for entry in _adhoc_entries()}
    marker_scripts = {script for script, entry in entries.items() if entry["marker"] == "1"}
    assert marker_scripts == set(MARKER_PATCHES)

    for script, patch in MARKER_PATCHES.items():
        source = patch.read_text(encoding="utf-8")
        assert source.count("; TODO") == 1
        assert re.search(r"^\s*(?:Function|Event)\s+", source, re.MULTILINE)
        assert entries[script]["patch"] == str(patch.relative_to(REPO_ROOT)).replace("\\", "/")


def test_intended_behavioral_patches_exist_and_no_registry_patch_is_marker_only():
    assert len(BEHAVIORAL_PATCHES) == 4
    for patch in BEHAVIORAL_PATCHES:
        source = patch.read_text(encoding="utf-8")
        assert re.search(r"^\s*(?:Function|Event)\s+", source, re.MULTILINE)

    for entry in _adhoc_entries():
        if entry["patch"] == "none":
            continue
        source = (REPO_ROOT / entry["patch"]).read_text(encoding="utf-8")
        assert re.sub(r"(?m)^\s*; TODO\s*$\n?", "", source).strip()


def test_reviewed_status_rows_point_to_their_batch_contract_or_are_explicitly_absent_from_csv():
    rows = list(csv.DictReader((DOCS / "status.csv").open(encoding="utf-8")))
    by_script = {row["script_name"].lower(): row for row in rows}
    present = {
        "BloodEagleSpotterAlarmActivatorScript": ("unsupported-online", "a"),
        "DefaultAliasOnActivateGiveItem": ("record-dependency", "b"),
        "DefaultAliasSetStageOnKeypadSuccess": ("record-dependency", "b"),
        "DefaultActivatorVendorFactionScript": ("patched", "c"),
        "DefaultChallengeMessageOnActivateAlias": ("evidence-blocked", "c"),
        "DefaultQuestTriggerRespawnVIPScript": ("evidence-blocked", "c"),
        "DefaultKeypadScript": ("evidence-blocked", "d"),
        "defaultkeypaddoorscript": ("evidence-blocked", "d"),
        "DefaultKeypadTimedSwitchScript": ("evidence-blocked", "d"),
        "Default2StateSyncActivator": ("patched", "e"),
        "DefaultDestructibleMultiStateActivator": ("patched", "e"),
        "DefaultExplosionOnActivate": ("patched", "e"),
        "defaultunlockandopenlinkonactivate": ("patched", "e"),
    }
    for script, (state, batch) in present.items():
        row = by_script[script.lower()]
        assert row["terminal_state"] == state
        assert row["evidence"] == f"contracts/ad-hoc-default-script-repairs-batch-{batch}.md"

    absent = {script for script in BATCH_MANIFEST["e"] if script.lower() not in by_script}
    assert {"DefaultMultiStateActivator", "DefaultMultiStateClientSideActivator", "DefaultSequentialStateActivator"} <= absent
