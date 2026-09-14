from __future__ import annotations

from pathlib import Path


SCRIPT_PATH = (
    Path(__file__).resolve().parents[1]
    / "script_additions"
    / "fo76_fo4"
    / "B21"
    / "LocalEncounterMaterializer.psc"
)


def _source() -> str:
    return SCRIPT_PATH.read_text(encoding="utf-8")


def test_materializer_pre_spawns_disabled_into_original_collections():
    source = _source()

    assert source.startswith("Scriptname B21:LocalEncounterMaterializer Extends Quest")
    assert "spawnMarker.PlaceAtMe(actorBase, 1, True, True, False)" in source
    assert "waveCollection.AddRef(spawnedRef)" in source
    assert "ReplaceCollectionOnPrepare[aiWaveIndex]" in source
    assert "ClearMaterializedCollection(waveCollection)" in source
    assert "PreparedWaveIndices.Find(aiWaveIndex) >= 0" in source
    assert "StageAfterPreparation[aiWaveIndex]" in source


def test_materializer_leaves_combat_and_completion_to_wave_owner():
    source = _source()

    assert ".Enable(" not in source
    assert ".EnableNoWait(" not in source
    assert ".StartCombat(" not in source
    assert "SetStage(nextStage)" in source
    assert "StageToSetAtEnd" not in source
    assert "OnDeath" not in source


def test_materializer_reconciles_load_and_cleans_its_spawned_references():
    source = _source()

    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in source
    assert "Event Actor.OnPlayerLoadGame(Actor akSender)" in source
    assert "PrepareEligibleWaves()" in source
    assert "Event OnQuestShutdown()" in source
    assert "ClearAllMaterializedReferences()" in source
    assert "waveCollection.RemoveRef(spawnedRef)" in source
    assert "spawnedRef.DisableNoWait()" in source
    assert "spawnedRef.Delete()" in source
