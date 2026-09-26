from __future__ import annotations

from bacup_lib.workflows.unified import (
    _script_patch_source,
)


SCRIPT_NAME = "Vault79_SentryBotPodScript"


def test_sentry_pod_completion_signal_prevents_a_respawn():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    gate = patch.index("If encounterComplete")
    blocked = patch.index("If IsActivationBlocked()")
    assert "sentryBot.IsDead()" in patch[:gate]
    assert "playerRef.GetValue(myActorValue) >= 1.0" in patch[:gate]
    assert "sentryBot.GetValue(myActorValue) >= 1.0" in patch[:gate]
    assert "podDoor.SetOpen(True)" in patch[gate:]
    assert patch.index("Return", gate) < blocked
    assert blocked < patch.index("BlockActivation(True, True)")
    assert blocked < patch.index("sentryActivator.Activate(Self)")


def test_sentry_pod_releases_the_bound_actor_after_opening_the_pod():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    start_sound = patch.index("SentryPodSoundStart.Play(soundMarker)")
    klaxon = patch.index("klaxonMarker.Activate(Self)")
    steam = patch.index("steamMarker.Enable()")
    door = patch.rindex("podDoor.SetOpen(True)")
    activator = patch.index("sentryActivator.Activate(Self)")
    fallback = patch.index("sentryBot.Enable()")
    assert start_sound < klaxon < steam < door < activator < fallback
    assert "BlockActivation(True, True)" in patch
    assert "BlockActivation(False)" not in patch
