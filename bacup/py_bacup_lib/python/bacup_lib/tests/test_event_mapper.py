"""Tests for EventMapper — animation event name translation."""
from __future__ import annotations

import pytest

from bacup_lib.animation.event_mapper import EventMapper
from bacup_lib.models import AnimationEvent


# ── FO3 → FO4 ──────────────────────────────────────────────────────────


class TestFO3ToFO4:
    @pytest.fixture
    def mapper(self):
        return EventMapper("fo3", "fo4")

    def test_direct_mapping_hit(self, mapper):
        ev = AnimationEvent(time=0.5, text="hit")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.5, text="HitFrame")
        assert warning is None

    def test_direct_mapping_equip(self, mapper):
        ev = AnimationEvent(time=0.0, text="Equip")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.0, text="weaponDraw")
        assert warning is None

    def test_direct_mapping_unequip(self, mapper):
        ev = AnimationEvent(time=1.0, text="Unequip")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=1.0, text="weaponSheathe")
        assert warning is None

    def test_drop_start(self, mapper):
        ev = AnimationEvent(time=0.0, text="start")
        result, warning = mapper.map_event(ev)
        assert result is None
        assert warning is None

    def test_drop_end(self, mapper):
        ev = AnimationEvent(time=2.0, text="end")
        result, warning = mapper.map_event(ev)
        assert result is None
        assert warning is None

    def test_pattern_sound(self, mapper):
        ev = AnimationEvent(time=0.3, text="Sound: WPNLaserFire")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.3, text="SoundPlay.WPNLaserFire")
        assert warning is None

    def test_pattern_attack(self, mapper):
        ev = AnimationEvent(time=0.4, text="Attack: Power")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.4, text="weaponSwing")
        assert warning is None

    def test_pattern_prn_dropped(self, mapper):
        """prn (parent node) commands have no FO4 equivalent."""
        ev = AnimationEvent(time=0.0, text="prn: Weapon")
        result, warning = mapper.map_event(ev)
        assert result is None
        assert warning is None

    def test_unmapped_passthrough(self, mapper):
        ev = AnimationEvent(time=0.7, text="CustomModEvent")
        result, warning = mapper.map_event(ev)
        assert result == ev
        assert warning == "Unmapped animation event 'CustomModEvent' passed through as-is"

    def test_map_events_batch(self, mapper):
        events = (
            AnimationEvent(time=0.0, text="start"),
            AnimationEvent(time=0.1, text="Equip"),
            AnimationEvent(time=0.5, text="hit"),
            AnimationEvent(time=0.8, text="Sound: Reload"),
            AnimationEvent(time=1.0, text="end"),
        )
        mapped, warnings = mapper.map_events(events)
        # start and end are dropped
        assert len(mapped) == 3
        assert mapped[0].text == "weaponDraw"
        assert mapped[1].text == "HitFrame"
        assert mapped[2].text == "SoundPlay.Reload"
        assert warnings == []

    def test_map_events_with_warnings(self, mapper):
        events = (
            AnimationEvent(time=0.0, text="hit"),
            AnimationEvent(time=0.5, text="Unknown"),
        )
        mapped, warnings = mapper.map_events(events)
        assert len(mapped) == 2
        assert len(warnings) == 1
        assert "Unknown" in warnings[0]

    def test_preserves_time(self, mapper):
        ev = AnimationEvent(time=1.234, text="hit")
        result, _ = mapper.map_event(ev)
        assert result.time == 1.234


# ── FO4 → FO3 ──────────────────────────────────────────────────────────


class TestFO4ToFO3:
    @pytest.fixture
    def mapper(self):
        return EventMapper("fo4", "fo3")

    def test_direct_mapping_hitframe(self, mapper):
        ev = AnimationEvent(time=0.5, text="HitFrame")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.5, text="hit")
        assert warning is None

    def test_direct_mapping_prehitframe(self, mapper):
        ev = AnimationEvent(time=0.4, text="preHitFrame")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.4, text="hit")
        assert warning is None

    def test_direct_mapping_weapon_draw(self, mapper):
        ev = AnimationEvent(time=0.0, text="weaponDraw")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.0, text="Equip")
        assert warning is None

    def test_direct_mapping_weapon_sheathe(self, mapper):
        ev = AnimationEvent(time=1.0, text="weaponSheathe")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=1.0, text="Unequip")
        assert warning is None

    def test_direct_mapping_weapon_fire(self, mapper):
        ev = AnimationEvent(time=0.2, text="weaponFire")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.2, text="Sound: WeaponFire")
        assert warning is None

    def test_direct_mapping_weapon_swing(self, mapper):
        ev = AnimationEvent(time=0.3, text="weaponSwing")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.3, text="Attack: Swing")
        assert warning is None

    def test_pattern_soundplay(self, mapper):
        ev = AnimationEvent(time=0.5, text="SoundPlay.WPNReload")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.5, text="Sound: WPNReload")
        assert warning is None

    def test_pattern_footstep_dropped(self, mapper):
        for foot in ["FootLeft", "FootRight", "FootFront", "FootBack"]:
            ev = AnimationEvent(time=0.5, text=foot)
            result, warning = mapper.map_event(ev)
            assert result is None, f"{foot} should be dropped"
            assert warning is None

    def test_unmapped_passthrough(self, mapper):
        ev = AnimationEvent(time=0.5, text="SyncLeft")
        result, warning = mapper.map_event(ev)
        assert result == ev
        assert "SyncLeft" in warning

    def test_no_drops(self, mapper):
        """FO4→FO3 has no drop list."""
        ev = AnimationEvent(time=0.0, text="start")
        result, warning = mapper.map_event(ev)
        # "start" is not in FO4→FO3 drop list, so it passes through
        assert result == ev
        assert warning is not None


# ── FNV alias ───────────────────────────────────────────────────────────


class TestGameAliases:
    def test_fnv_uses_fo3_map(self):
        mapper = EventMapper("fnv", "fo4")
        ev = AnimationEvent(time=0.5, text="hit")
        result, _ = mapper.map_event(ev)
        assert result.text == "HitFrame"

    def test_missing_pair_raises(self):
        with pytest.raises(FileNotFoundError):
            EventMapper("skyrimse", "fo4")

    def test_fallout76_alias(self):
        mapper = EventMapper("fallout76", "fo4")
        ev = AnimationEvent(time=0.0, text="PathTweenerStart")
        result, _ = mapper.map_event(ev)
        assert result is None


# ── FO76 → FO4 ──────────────────────────────────────────────────────────


class TestFO76ToFO4:
    @pytest.fixture
    def mapper(self):
        return EventMapper("fo76", "fo4")

    # Drops ----------------------------------------------------------------

    def test_drop_path_tweener_start(self, mapper):
        ev = AnimationEvent(time=0.4, text="PathTweenerStart")
        result, warning = mapper.map_event(ev)
        assert result is None
        assert warning is None

    def test_drop_path_tweener_end(self, mapper):
        ev = AnimationEvent(time=0.6, text="PathTweenerEnd")
        result, warning = mapper.map_event(ev)
        assert result is None
        assert warning is None

    def test_drop_char_fx_on_off(self, mapper):
        for text in ("CharFXOn", "CharFXOff", "CharFXOnWild", "CharFXOffWild"):
            ev = AnimationEvent(time=0.1, text=text)
            result, _ = mapper.map_event(ev)
            assert result is None, f"{text} should be dropped"

    def test_drop_slam_events(self, mapper):
        for text in ("bothSlam", "leftSlam", "RightSlam"):
            ev = AnimationEvent(time=0.1, text=text)
            result, _ = mapper.map_event(ev)
            assert result is None, f"{text} should be dropped"

    def test_drop_fire_behemoth_salvo(self, mapper):
        ev = AnimationEvent(time=0.1, text="FireBehemothSalvo")
        result, _ = mapper.map_event(ev)
        assert result is None

    # Pattern rewrites -----------------------------------------------------

    def test_pattern_weapon_fire_lowercase(self, mapper):
        ev = AnimationEvent(time=0.2, text="weaponFire.1")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.2, text="weaponFire")
        assert warning is None

    def test_pattern_weapon_fire_uppercase(self, mapper):
        ev = AnimationEvent(time=0.5, text="WeaponFire.2")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.5, text="weaponFire")
        assert warning is None

    def test_pattern_weapon_fire_multi_digit(self, mapper):
        ev = AnimationEvent(time=0.5, text="weaponFire.12")
        result, warning = mapper.map_event(ev)
        assert result == AnimationEvent(time=0.5, text="weaponFire")
        assert warning is None

    # Pass-through ---------------------------------------------------------

    def test_unmapped_passthrough_with_warning(self, mapper):
        ev = AnimationEvent(time=0.7, text="SomeModderCustomEvent")
        result, warning = mapper.map_event(ev)
        assert result == ev
        assert warning is not None
        assert "SomeModderCustomEvent" in warning

    def test_common_events_pass_through(self, mapper):
        """Events snallygaster has and works fine with must NOT be dropped."""
        for text in (
            "HitFrame", "preHitFrame", "weaponSwing",
            "weaponFire",  # unparameterized — should pass through untouched
            "WeaponSweepAttackStart", "WeaponSweepAttackStop",
            "startAllowRotation", "startAnimationDriven",
            "SoundPlay.NPCSnallygasterAttackD",
            "SoundPlay.NPCMegaSlothAttackAoE1",
            "CameraShake.0.9,0.35,0.1",
            "FootLeft", "FootRight", "FootFrontLeft", "FootBackRight",
        ):
            ev = AnimationEvent(time=0.5, text=text)
            result, _ = mapper.map_event(ev)
            assert result is not None, f"{text} should not be dropped"
            assert result.text == text, f"{text} should pass through unchanged"

    def test_plain_weaponfire_untouched(self, mapper):
        """weaponFire (no .N suffix) must NOT match the pattern."""
        ev = AnimationEvent(time=0.2, text="weaponFire")
        result, warning = mapper.map_event(ev)
        # Unmapped passthrough with warning
        assert result == ev

    # Batch ---------------------------------------------------------------

    def test_batch_drops_and_renames(self, mapper):
        events = (
            AnimationEvent(time=0.0, text="SoundPlay.Attack1"),
            AnimationEvent(time=0.1, text="PathTweenerStart"),
            AnimationEvent(time=0.2, text="weaponFire.1"),
            AnimationEvent(time=0.3, text="weaponSwing"),
            AnimationEvent(time=0.4, text="bothSlam"),
            AnimationEvent(time=0.5, text="HitFrame"),
        )
        mapped, _warnings = mapper.map_events(events)
        texts = [e.text for e in mapped]
        # Drops: PathTweenerStart, bothSlam
        # Rewrite: weaponFire.1 -> weaponFire
        assert texts == [
            "SoundPlay.Attack1",
            "weaponFire",
            "weaponSwing",
            "HitFrame",
        ]
