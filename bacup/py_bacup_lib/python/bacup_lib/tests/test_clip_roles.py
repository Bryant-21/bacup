from __future__ import annotations

from bacup_lib.behavior.clip_roles import (
    ROLE_ORDER,
    bucket_clips_by_role,
    classify_clip_role,
)


def test_classify_clip_role_for_deathclaw_examples() -> None:
    cases = {
        "deathclaw_idle.kf": "idle",
        "deathclaw_idlecombat.kf": "idle",
        "deathclaw_walkforward.kf": "locomotion",
        "deathclaw_runforward.kf": "locomotion",
        "deathclaw_sprintforward.kf": "locomotion",
        "deathclaw_h2hattackleft.kf": "attack",
        "deathclaw_attackpowerforward.kf": "attack",
        "deathclaw_crithitrightleg.kf": "hit_react",
        "deathclaw_staggerforwardsmall.kf": "hit_react",
        "deathclaw_deathanimationa.kf": "death",
        "deathclaw_flipcar.kf": "special",
        "deathclaw_whatevermystery.kf": "unknown",
    }

    for filename, expected in cases.items():
        assert classify_clip_role(filename) == expected


def test_classify_clip_role_for_real_deathclaw_filenames() -> None:
    cases = {
        "h2hattackleft_a.kf": "attack",
        "h2hattackright_b.kf": "attack",
        "idleanims/specialidle_cageexit.kf": "special",
        "idleanims/specialidle_hithead.kf": "hit_react",
        "locomotion/hurt/mtforward_hurt.kf": "locomotion",
        "locomotion/hurt/mtturnleft_hurt.kf": "locomotion",
        "locomotion/mtfoward.kf": "unknown",
    }

    for filename, expected in cases.items():
        assert classify_clip_role(filename) == expected


def test_bucket_clips_by_role_is_complete_and_deterministic() -> None:
    buckets = bucket_clips_by_role(
        [
            "DeathClaw_AttackRight.kf",
            "DeathClaw_IdleCombat.kf",
            "DeathClaw_Idle.kf",
            "h2hattackleft_a.kf",
            "locomotion/hurt/mtforward_hurt.kf",
            "DeathClaw_RunForward.kf",
            "DeathClaw_AttackLeft.kf",
            "DeathClaw_DeathAnimationB.kf",
            "DeathClaw_FlipCar.kf",
        ]
    )

    assert tuple(buckets) == ROLE_ORDER
    assert buckets["idle"] == ("DeathClaw_Idle.kf", "DeathClaw_IdleCombat.kf")
    assert buckets["locomotion"] == (
        "DeathClaw_RunForward.kf",
        "locomotion/hurt/mtforward_hurt.kf",
    )
    assert buckets["attack"] == (
        "DeathClaw_AttackLeft.kf",
        "DeathClaw_AttackRight.kf",
        "h2hattackleft_a.kf",
    )
    assert buckets["hit_react"] == ()
    assert buckets["death"] == ("DeathClaw_DeathAnimationB.kf",)
    assert buckets["special"] == ("DeathClaw_FlipCar.kf",)
    assert buckets["unknown"] == ()
