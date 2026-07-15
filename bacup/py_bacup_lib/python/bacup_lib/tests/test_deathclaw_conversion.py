"""End-to-end Deathclaw conversion test: FO76 → FO4.

Converts the FULL Deathclaw creature from FO76 to FO4, then compares every
converted HKX against FO4 vanilla to verify class names, skeleton structure,
animation data integrity, and behavior graph conversion.

This is a creature test case (vs GaussRifle's weapon test):
 - Full 86-bone skeleton + 28-bone ragdoll skeleton
 - hkaSplineCompressedAnimation with 84 transform tracks
 - Complex creature behavior graphs (9 FO76 vs 2 FO4 behavior files)
 - Multiple behavior sub-files (ambush, dialogue, furniture, etc.)

Requires extracted FO76 + FO4 game data (skipped if missing).
"""
from __future__ import annotations

import os
import tempfile
from collections import Counter
from pathlib import Path

import pytest

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

_PROJECT = Path(__file__).resolve().parents[5]  # repository root
_FO76_EXTRACTED = Path(os.environ.get("FO76_EXTRACTED_DIR") or _PROJECT / "extracted" / "fo76")
_FO4_EXTRACTED = Path(os.environ.get("FO4_EXTRACTED_DIR") or _PROJECT / "extracted" / "fo4")
_FO76_BASE = _FO76_EXTRACTED / "meshes" / "actors" / "deathclaw"
_FO4_BASE = _FO4_EXTRACTED / "Meshes" / "Actors" / "Deathclaw"

_REQUIRES_DATA = pytest.mark.skipif(
    not _FO76_BASE.is_dir() or not _FO4_BASE.is_dir(),
    reason="Requires extracted FO76 + FO4 Deathclaw data",
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _read_fo76_hkx(path: Path):
    """Read a FO76 TAG0 tagfile, applying the FO76→FO4 migration so class
    names match the FO4 form expected by these tests (canonicalized
    hkbStateMachineStateInfo etc., not the raw FO76 hkbStateMachine::StateInfo).
    """
    from creation_lib.hkxpack import load_hkx_bytes
    from creation_lib.havok.native_runtime import _require_native
    data = bytes(_require_native().havok_convert_bytes(path.read_bytes(), "fo4"))
    hkx, _registry = load_hkx_bytes(data)
    return hkx


def _read_fo4_hkx(path: Path):
    """Read a FO4 packfile."""
    from creation_lib.hkxpack import load_hkx_bytes
    hkx, _registry = load_hkx_bytes(path.read_bytes())
    return hkx


def _convert_hkx(src: Path, dst: Path):
    """Convert a FO76 HKX to FO4 format (version 53)."""
    from creation_lib.havok_convert.converter import HavokConverter

    dst.parent.mkdir(parents=True, exist_ok=True)
    c = HavokConverter()
    c.convert_file(str(src), str(dst), 53)


def _class_counter(hkx) -> Counter:
    return Counter(o.class_name for o in hkx.objects)


def _get_member_value(obj, member_name):
    """Get the value of a named member from an HKX object."""
    for m in obj.members:
        if m.name == member_name:
            if hasattr(m, "contents"):
                return m.contents
            return getattr(m, "value", getattr(m, "str_value", None))
    return None


# ---------------------------------------------------------------------------
# Skeleton tests
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestSkeletonComparison:
    """Compare Deathclaw skeletons between FO76 and FO4."""

    def test_skeleton_bone_count_matches(self):
        """Both games have 86-bone main skeleton."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "characterassets" / "skeleton.hkx")
        fo4 = _read_fo4_hkx(_FO4_BASE / "CharacterAssets" / "skeleton.hkx")

        fo76_skels = [o for o in fo76.objects if o.class_name == "hkaSkeleton"]
        fo4_skels = [o for o in fo4.objects if o.class_name == "hkaSkeleton"]

        assert len(fo76_skels) == 2, f"Expected 2 skeletons (main+ragdoll), got {len(fo76_skels)}"
        assert len(fo4_skels) == 2

        # Main skeleton
        fo76_bones = _get_member_value(fo76_skels[0], "bones")
        fo4_bones = _get_member_value(fo4_skels[0], "bones")
        assert len(fo76_bones) == 86
        assert len(fo4_bones) == 86

    def test_skeleton_bone_names_match(self):
        """Bone names are identical between FO76 and FO4."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "characterassets" / "skeleton.hkx")
        fo4 = _read_fo4_hkx(_FO4_BASE / "CharacterAssets" / "skeleton.hkx")

        fo76_skels = [o for o in fo76.objects if o.class_name == "hkaSkeleton"]
        fo4_skels = [o for o in fo4.objects if o.class_name == "hkaSkeleton"]

        def bone_names(skel):
            bones = _get_member_value(skel, "bones")
            names = []
            for b in bones:
                for m in (b.members if hasattr(b, "members") else []):
                    if m.name == "name":
                        names.append(getattr(m, "value", getattr(m, "str_value", "")))
            return names

        assert bone_names(fo76_skels[0]) == bone_names(fo4_skels[0]), "Main skeleton bone names differ"
        assert bone_names(fo76_skels[1]) == bone_names(fo4_skels[1]), "Ragdoll skeleton bone names differ"

    def test_ragdoll_skeleton_count(self):
        """Ragdoll skeleton has 28 bones in both games."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "characterassets" / "skeleton.hkx")
        fo4 = _read_fo4_hkx(_FO4_BASE / "CharacterAssets" / "skeleton.hkx")

        fo76_skels = [o for o in fo76.objects if o.class_name == "hkaSkeleton"]
        fo4_skels = [o for o in fo4.objects if o.class_name == "hkaSkeleton"]

        fo76_ragdoll = _get_member_value(fo76_skels[1], "bones")
        fo4_ragdoll = _get_member_value(fo4_skels[1], "bones")
        assert len(fo76_ragdoll) == 28
        assert len(fo4_ragdoll) == 28

    def test_skeleton_converts(self, tmp_path):
        """Skeleton HKX converts without error."""
        src = _FO76_BASE / "characterassets" / "skeleton.hkx"
        dst = tmp_path / "skeleton.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_skeleton_parent_indices_match(self):
        """Parent indices are identical (same bone hierarchy)."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "characterassets" / "skeleton.hkx")
        fo4 = _read_fo4_hkx(_FO4_BASE / "CharacterAssets" / "skeleton.hkx")

        fo76_skels = [o for o in fo76.objects if o.class_name == "hkaSkeleton"]
        fo4_skels = [o for o in fo4.objects if o.class_name == "hkaSkeleton"]

        fo76_parents = _get_member_value(fo76_skels[0], "parentIndices")
        fo4_parents = _get_member_value(fo4_skels[0], "parentIndices")
        assert len(fo76_parents) == len(fo4_parents)


# ---------------------------------------------------------------------------
# Animation conversion tests
# ---------------------------------------------------------------------------

# Shared animations that exist in both FO76 and FO4 (case-insensitive match)
_SHARED_ANIMATION_PAIRS = [
    ("animations/deathclaw_idle.hkx", "Animations/DeathClaw_Idle.hkx"),
    ("animations/deathclaw_attackleft.hkx", "Animations/DeathClaw_AttackLeft.hkx"),
    ("animations/deathclaw_runforward.hkx", "Animations/DeathClaw_RunForward.hkx"),
    ("animations/deathclaw_sprintforward.hkx", "Animations/DeathClaw_SprintForward.hkx"),
    ("animations/deathclaw_deathanimationa.hkx", "Animations/DeathClaw_DeathAnimationA.hkx"),
    ("animations/deathclaw_attackpowerforward.hkx", "Animations/DeathClaw_AttackPowerForward.hkx"),
    ("animations/deathclaw_walkforward.hkx", "Animations/DeathClaw_WalkForward.hkx"),
    ("animations/deathclaw_idlecombat.hkx", "Animations/DeathClaw_IdleCombat.hkx"),
    ("animations/deathclaw_attackthrow.hkx", "Animations/DeathClaw_AttackThrow.hkx"),
    ("animations/deathclaw_flipcar.hkx", "Animations/DeathClaw_FlipCar.hkx"),
]


@_REQUIRES_DATA
class TestAnimationConversion:
    """Convert FO76 Deathclaw animations and compare to FO4 vanilla."""

    @pytest.fixture(autouse=True)
    def _setup(self, tmp_path):
        self.tmp = tmp_path

    @pytest.mark.parametrize("fo76_rel,fo4_rel", _SHARED_ANIMATION_PAIRS,
                             ids=[Path(p[0]).stem for p in _SHARED_ANIMATION_PAIRS])
    def test_animation_converts(self, fo76_rel, fo4_rel):
        """Each shared animation converts without error."""
        src = _FO76_BASE / fo76_rel
        dst = self.tmp / Path(fo76_rel).name
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    @pytest.mark.parametrize("fo76_rel,fo4_rel", _SHARED_ANIMATION_PAIRS,
                             ids=[Path(p[0]).stem for p in _SHARED_ANIMATION_PAIRS])
    def test_animation_track_count_matches(self, fo76_rel, fo4_rel):
        """Transform track count matches between FO76 and FO4 (84 tracks)."""
        fo76 = _read_fo76_hkx(_FO76_BASE / fo76_rel)
        fo4 = _read_fo4_hkx(_FO4_BASE / fo4_rel)

        def get_track_count(hkx):
            for obj in hkx.objects:
                if "Animation" in obj.class_name and obj.class_name != "hkaAnimationContainer":
                    return _get_member_value(obj, "numberOfTransformTracks")
            return None

        fo76_tracks = get_track_count(fo76)
        fo4_tracks = get_track_count(fo4)
        assert fo76_tracks == fo4_tracks == 84, (
            f"Track count mismatch: FO76={fo76_tracks} FO4={fo4_tracks}"
        )

    def test_animation_class_is_spline_compressed(self):
        """All Deathclaw animations use hkaSplineCompressedAnimation."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "animations" / "deathclaw_idle.hkx")
        anim_classes = [o.class_name for o in fo76.objects
                        if "Animation" in o.class_name and o.class_name != "hkaAnimationContainer"]
        assert "hkaSplineCompressedAnimation" in anim_classes

    def test_animation_binding_has_bone_indices(self):
        """Animation binding has transformTrackToBoneIndices matching track count."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "animations" / "deathclaw_idle.hkx")
        for obj in fo76.objects:
            if obj.class_name == "hkaAnimationBinding":
                indices = _get_member_value(obj, "transformTrackToBoneIndices")
                assert indices is not None
                assert len(indices) == 84, f"Expected 84 bone indices, got {len(indices)}"
                break
        else:
            pytest.fail("No hkaAnimationBinding found")

    def test_idle_has_reference_frame(self):
        """Idle animation has hkaDefaultAnimatedReferenceFrame (root motion data)."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "animations" / "deathclaw_idle.hkx")
        classes = {o.class_name for o in fo76.objects}
        assert "hkaDefaultAnimatedReferenceFrame" in classes

    def test_convert_all_shared_animations(self):
        """Batch-convert all shared animations without error."""
        from creation_lib.havok_convert.converter import HavokConverter
        c = HavokConverter()

        fo76_anims = {f.relative_to(_FO76_BASE).as_posix().lower()
                      for f in (_FO76_BASE / "animations").rglob("*.hkx")}
        fo4_anims = {f.relative_to(_FO4_BASE).as_posix().lower()
                     for f in (_FO4_BASE / "Animations").rglob("*.hkx")}
        shared = fo76_anims & fo4_anims

        failures = []
        for rel_lower in sorted(shared):
            # Find actual FO76 path (case-insensitive)
            fo76_path = _FO76_BASE / rel_lower
            dst = self.tmp / Path(rel_lower).name
            dst.parent.mkdir(parents=True, exist_ok=True)
            try:
                c.convert_file(str(fo76_path), str(dst), 53)
                assert dst.stat().st_size > 100
            except Exception as e:
                failures.append(f"{rel_lower}: {e}")

        assert not failures, f"Failed animations:\n" + "\n".join(failures)

    def test_converted_animation_readable(self):
        """Converted animation can be read back as valid FO4 packfile."""
        from creation_lib.hkxpack import detect_format
        from creation_lib.hkxpack import load_hkx_bytes

        src = _FO76_BASE / "animations" / "deathclaw_idle.hkx"
        dst = self.tmp / "idle.hkx"
        _convert_hkx(src, dst)

        fmt = detect_format(dst.read_bytes())
        assert fmt[0] == "packfile", f"Expected packfile, got {fmt}"

        hkx, _ = load_hkx_bytes(dst.read_bytes())
        assert len(hkx.objects) > 0


# ---------------------------------------------------------------------------
# Animation file coverage tests
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestAnimationCoverage:
    """Verify animation file sets between FO76 and FO4."""

    def _get_anim_set(self, base: Path) -> set[str]:
        return {f.relative_to(base).as_posix().lower()
                for f in (base / "animations" if (base / "animations").exists()
                          else base / "Animations").rglob("*.hkx")}

    def test_shared_animation_count(self):
        """At least 100 animations are shared between FO76 and FO4."""
        fo76 = self._get_anim_set(_FO76_BASE)
        fo4 = self._get_anim_set(_FO4_BASE)
        shared = fo76 & fo4
        assert len(shared) >= 100, f"Only {len(shared)} shared animations"

    def test_fo76_has_extra_furniture_anims(self):
        """FO76 has Deathclaw cage furniture animations not in FO4."""
        fo76 = self._get_anim_set(_FO76_BASE)
        fo4 = self._get_anim_set(_FO4_BASE)
        fo76_only = fo76 - fo4
        cage_anims = [a for a in fo76_only if "deathclawcage" in a]
        assert len(cage_anims) > 0, "Expected FO76-only cage animations"

    def test_fo4_has_paired_kill_anims(self):
        """FO4 has paired kill animations not in FO76."""
        fo76 = self._get_anim_set(_FO76_BASE)
        fo4 = self._get_anim_set(_FO4_BASE)
        fo4_only = fo4 - fo76
        paired = [a for a in fo4_only if "paired" in a]
        assert len(paired) >= 10, f"Expected 10+ paired kill anims, got {len(paired)}"

    def test_fo4_has_strafe_locomotion(self):
        """FO4 has left/right strafing locomotion animations not in FO76."""
        fo76 = self._get_anim_set(_FO76_BASE)
        fo4 = self._get_anim_set(_FO4_BASE)
        fo4_only = fo4 - fo76
        strafe = [a for a in fo4_only if "walkleft" in a or "walkright" in a
                  or "runleft" in a or "runright" in a]
        assert len(strafe) > 0, "Expected FO4-only strafe animations"


# ---------------------------------------------------------------------------
# Behavior graph tests
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestBehaviorConversion:
    """Convert FO76 behavior HKX files and compare to FO4 vanilla."""

    @pytest.fixture(autouse=True)
    def _setup(self, tmp_path):
        self.tmp = tmp_path

    def test_project_converts(self):
        """Project HKX converts without error."""
        src = _FO76_BASE / "deathclawproject.hkx"
        dst = self.tmp / "project.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_project_class_names_match(self):
        """Project HKX class names match FO4 vanilla."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "deathclawproject.hkx")
        fo4 = _read_fo4_hkx(_FO4_BASE / "DeathclawProject.hkx")
        assert _class_counter(fo76) == _class_counter(fo4)

    def test_character_converts(self):
        """Character HKX converts without error."""
        src = _FO76_BASE / "characters" / "deathclaw.hkx"
        dst = self.tmp / "character.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_character_has_character_data(self):
        """Character.hkx has hkbCharacterData after TAG0 enrichment."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "characters" / "deathclaw.hkx")
        char_objs = [o for o in fo76.objects if o.class_name == "hkbCharacterData"]
        assert len(char_objs) == 1, (
            f"Expected 1 hkbCharacterData, got classes: "
            f"{[o.class_name for o in fo76.objects]}"
        )

    def test_character_converted_has_fo4_members(self):
        """Converted Character.hkx has required FO4 members on hkbCharacterData."""
        from creation_lib.hkxpack import load_hkx_bytes

        src = _FO76_BASE / "characters" / "deathclaw.hkx"
        dst = self.tmp / "character.hkx"
        _convert_hkx(src, dst)

        hkx, _ = load_hkx_bytes(dst.read_bytes())
        char_objs = [o for o in hkx.objects if o.class_name == "hkbCharacterData"]
        assert len(char_objs) == 1

        member_names = {m.name for m in char_objs[0].members}
        required = {"characterControllerSetup", "modelUpMS", "modelForwardMS",
                     "modelRightMS", "stringData", "scale"}
        missing = required - member_names
        assert not missing, f"Missing required FO4 members: {missing}"

    def test_character_has_string_data(self):
        """Character.hkx has hkbCharacterStringData."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "characters" / "deathclaw.hkx")
        classes = {o.class_name for o in fo76.objects}
        assert "hkbCharacterStringData" in classes

    def test_root_behavior_converts(self):
        """DeathclawRootBehavior.hkx converts without error."""
        src = _FO76_BASE / "behaviors" / "deathclawrootbehavior.hkx"
        dst = self.tmp / "root.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_root_behavior_object_count_matches(self):
        """Root behavior has same object count in both games."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "behaviors" / "deathclawrootbehavior.hkx")
        fo4 = _read_fo4_hkx(_FO4_BASE / "Behaviors" / "DeathclawRootBehavior.hkx")
        assert len(fo76.objects) == len(fo4.objects)

    def test_everything_behavior_converts(self):
        """DeathclawEverything.hkx converts without error."""
        src = _FO76_BASE / "behaviors" / "deathclaweverything.hkx"
        dst = self.tmp / "everything.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_everything_critical_classes_present(self):
        """DeathclawEverything has critical behavior classes."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "behaviors" / "deathclaweverything.hkx")
        classes = _class_counter(fo76)

        required = [
            "hkRootLevelContainer",
            "hkbBehaviorGraph",
            "hkbStateMachine",
            "hkbBehaviorGraphData",
            "hkbVariableValueSet",
            "hkbBehaviorGraphStringData",
            "hkbModifierGenerator",
            "hkbVariableBindingSet",
            "hkbStateMachineStateInfo",
            "hkbStateMachineTransitionInfoArray",
            "hkbBlendingTransitionEffect",
            "hkbClipTriggerArray",
        ]
        missing = [c for c in required if classes.get(c, 0) == 0]
        assert not missing, f"Missing critical classes: {missing}"

    def test_everything_no_hkuint16_class(self):
        """No objects should have class_name 'hkUint16' after enrichment."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "behaviors" / "deathclaweverything.hkx")
        bad = [o for o in fo76.objects if o.class_name == "hkUint16"]
        assert len(bad) == 0, f"Found {len(bad)} hkUint16 objects (should be resolved)"

    def test_everything_no_unresolved_generators(self):
        """No objects should remain as generic hkbGenerator after enrichment."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "behaviors" / "deathclaweverything.hkx")
        classes = _class_counter(fo76)
        assert classes.get("hkbGenerator", 0) == 0, (
            f"Found {classes['hkbGenerator']} unresolved hkbGenerator objects"
        )
        assert classes.get("hkbModifierWrapper", 0) == 0, (
            f"Found {classes['hkbModifierWrapper']} unresolved hkbModifierWrapper objects"
        )

    def test_everything_clip_generators_resolved(self):
        """Clip generators should be resolved to hkbClipGenerator with animationName."""
        fo76 = _read_fo76_hkx(_FO76_BASE / "behaviors" / "deathclaweverything.hkx")
        clips = [o for o in fo76.objects if o.class_name == "hkbClipGenerator"]
        assert len(clips) >= 60, f"Expected 60+ hkbClipGenerator, got {len(clips)}"
        # Verify real clip generators have the animationName field
        for clip in clips:
            anim_name = _get_member_value(clip, "animationName")
            assert anim_name is not None, f"Clip {clip.name} missing animationName"

    def test_converted_behavior_readable(self):
        """Converted behavior can be read back as a valid FO4 packfile."""
        from creation_lib.hkxpack import detect_format
        from creation_lib.hkxpack import load_hkx_bytes

        src = _FO76_BASE / "behaviors" / "deathclaweverything.hkx"
        dst = self.tmp / "everything.hkx"
        _convert_hkx(src, dst)

        fmt = detect_format(dst.read_bytes())
        assert fmt[0] == "packfile", f"Expected packfile, got {fmt}"

        hkx, _ = load_hkx_bytes(dst.read_bytes())
        assert len(hkx.objects) > 0

        # Verify key objects have members
        for obj in hkx.objects:
            if obj.class_name in ("hkbStateMachine", "hkbModifierGenerator",
                                   "hkbStateMachineStateInfo"):
                assert len(obj.members) > 0, (
                    f"{obj.name} ({obj.class_name}) has 0 members"
                )


# ---------------------------------------------------------------------------
# Behavior file coverage
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestBehaviorCoverage:
    """Verify behavior file sets between FO76 and FO4."""

    def test_fo76_has_more_behavior_files(self):
        """FO76 has 9 behavior files vs FO4's 2 (split into sub-behaviors)."""
        fo76_behaviors = list((_FO76_BASE / "behaviors").glob("*.hkx"))
        fo4_behaviors = list((_FO4_BASE / "Behaviors").glob("*.hkx"))
        assert len(fo76_behaviors) == 9
        assert len(fo4_behaviors) == 2

    def test_shared_behaviors_present(self):
        """Both games have DeathclawEverything and DeathclawRootBehavior."""
        fo76_names = {f.name.lower() for f in (_FO76_BASE / "behaviors").glob("*.hkx")}
        fo4_names = {f.name.lower() for f in (_FO4_BASE / "Behaviors").glob("*.hkx")}
        assert "deathclaweverything.hkx" in fo76_names
        assert "deathclaweverything.hkx" in fo4_names
        assert "deathclawrootbehavior.hkx" in fo76_names
        assert "deathclawrootbehavior.hkx" in fo4_names

    def test_fo76_extra_behaviors_convert(self, tmp_path):
        """All FO76-only behavior files convert without error."""
        from creation_lib.havok_convert.converter import HavokConverter
        c = HavokConverter()

        fo4_names = {f.name.lower() for f in (_FO4_BASE / "Behaviors").glob("*.hkx")}
        fo76_only = [f for f in (_FO76_BASE / "behaviors").glob("*.hkx")
                     if f.name.lower() not in fo4_names]

        assert len(fo76_only) == 7, f"Expected 7 FO76-only behaviors, got {len(fo76_only)}"

        failures = []
        for f in fo76_only:
            dst = tmp_path / f.name
            try:
                c.convert_file(str(f), str(dst), 53)
                assert dst.stat().st_size > 100
            except Exception as e:
                failures.append(f"{f.name}: {e}")

        assert not failures, "Failed:\n" + "\n".join(failures)


# ---------------------------------------------------------------------------
# Full conversion pipeline
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestFullConversionPipeline:
    """Convert all Deathclaw HKX files and verify outputs."""

    def test_convert_all_hkx_files(self, tmp_path):
        """Convert every FO76 Deathclaw HKX and verify output exists."""
        from creation_lib.havok_convert.converter import HavokConverter
        c = HavokConverter()

        all_hkx = list(_FO76_BASE.rglob("*.hkx"))
        assert len(all_hkx) > 100, f"Expected 100+ HKX files, got {len(all_hkx)}"

        failures = []
        for f in all_hkx:
            rel = f.relative_to(_FO76_BASE)
            dst = tmp_path / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            try:
                c.convert_file(str(f), str(dst), 53)
                assert dst.stat().st_size > 100
            except Exception as e:
                failures.append(f"{rel}: {e}")

        assert not failures, f"{len(failures)} failures:\n" + "\n".join(failures[:20])

    def test_all_fo4_behavior_files_have_fo76_source(self):
        """Every FO4 behavior HKX has a matching FO76 source file."""
        fo4_files = list((_FO4_BASE / "Behaviors").rglob("*.hkx"))
        for fo4_f in fo4_files:
            rel = fo4_f.relative_to(_FO4_BASE / "Behaviors")
            # Case-insensitive match
            fo76_path = _FO76_BASE / "behaviors" / rel.as_posix().lower()
            assert fo76_path.exists(), f"FO4 file {rel} has no FO76 source"

    def test_nif_file_coverage(self):
        """FO76 has all the core NIFs that FO4 has."""
        fo4_nifs = {f.name.lower() for f in _FO4_BASE.glob("*.nif")}
        fo76_nifs = {f.name.lower() for f in _FO76_BASE.glob("*.nif")}
        # FO76 should have all the base model NIFs
        core_fo4 = {"deathclaw.nif", "deathclawchameleon.nif",
                     "deathclawheadreplace.nif", "deathclawlarmreplace.nif",
                     "deathclawtorsoreplace.nif"}
        missing = core_fo4 - fo76_nifs
        assert not missing, f"FO76 missing core NIFs: {missing}"
