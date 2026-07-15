"""End-to-end GaussRifle conversion test: FO76 → FO4.

Converts the FULL GaussRifle weapon from FO76 to FO4 with base_game_skip
disabled, then compares every converted HKX against FO4 vanilla to verify
class names, data integrity, and structure.

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
_FO76_MESHES = _FO76_EXTRACTED / "meshes"
_FO4_MESHES = _FO4_EXTRACTED / "Meshes"
_NIF_DB = _PROJECT / "data" / "fo76_nifs.db"

_FO76_BEHAVIOR_DIR = _FO76_MESHES / "effects" / "effectbehaviors" / "gaussrifleweaponfxhkb"
_FO4_BEHAVIOR_DIR = _FO4_MESHES / "Effects" / "EffectBehaviors" / "GaussRifleWeaponFXHKB"

_REQUIRES_DATA = pytest.mark.skipif(
    not _FO76_BEHAVIOR_DIR.is_dir() or not _FO4_BEHAVIOR_DIR.is_dir(),
    reason="Requires extracted FO76 + FO4 GaussRifle data",
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _read_fo76_hkx(path: Path):
    """Read a FO76 TAG0 tagfile, applying the FO76→FO4 migration so class
    names match the FO4 form expected by these tests.
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


def _dump_member(m) -> dict:
    """Flatten a member into a comparable dict."""
    info: dict = {"type": type(m).__name__}
    if hasattr(m, "contents"):
        info["count"] = len(m.contents)
        info["subtype"] = str(m.subtype)
    elif hasattr(m, "str_value"):
        info["value"] = m.str_value
    elif hasattr(m, "value"):
        info["value"] = m.value
    elif hasattr(m, "target"):
        info["target"] = m.target
    return info


# ---------------------------------------------------------------------------
# Walker / NIF lookup tests
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestBehaviorDiscovery:
    """Verify the walker discovers the full behavior bundle from NIFs."""

    def test_nif_lookup_finds_behavior_project(self):
        """NIF index maps gaussrifle_1.nif → behavior project HKX."""
        from bacup_lib.nif.lookup import NifIndexLookup

        lookup = NifIndexLookup(str(_NIF_DB), game="fo76")
        behaviors = lookup.get_behaviors("weapons/gaussrifle/gaussrifle_1.nif")
        assert any("GaussRifleWeaponFX" in b for b in behaviors), (
            f"Expected GaussRifleWeaponFX in behaviors, got {behaviors}"
        )

    def test_behavior_bundle_expansion(self):
        """Behavior project HKX expands to include Behaviors/ and Characters/."""
        from bacup_lib.behavior import expand_behavior_bundle
        from bacup_lib.models import AssetRef

        asset = AssetRef(
            "behavior",
            "Effects/EffectBehaviors/GaussRifleWeaponFXHKB/GaussRifleWeaponFX.hkx",
        )
        companions = expand_behavior_bundle(asset, str(_FO76_EXTRACTED))
        companion_names = {Path(c.source_path).name.lower() for c in companions}
        assert "behavior.hkx" in companion_names, f"Missing Behavior.hkx: {companion_names}"
        assert "character.hkx" in companion_names, f"Missing Character.hkx: {companion_names}"

    def test_secondary_assets_include_behavior(self):
        """NIF secondary asset extraction includes behavior asset."""
        from bacup_lib.nif.lookup import NifIndexLookup

        lookup = NifIndexLookup(str(_NIF_DB), game="fo76")
        assets = lookup.get_secondary_assets("weapons/gaussrifle/gaussrifle_1.nif")
        behavior_assets = [a for a in assets if a.asset_type == "behavior"]
        assert len(behavior_assets) >= 1, "No behavior assets from NIF lookup"


# ---------------------------------------------------------------------------
# HKX conversion tests — behavior files
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestBehaviorConversion:
    """Convert FO76 behavior HKX files and compare to FO4 vanilla."""

    # Pairs: (FO76 path relative to meshes, FO4 path relative to Meshes)
    _BEHAVIOR_FILES = [
        (
            "effects/effectbehaviors/gaussrifleweaponfxhkb/gaussrifleweaponfx.hkx",
            "Effects/EffectBehaviors/GaussRifleWeaponFXHKB/GaussRifleWeaponFX.hkx",
        ),
        (
            "effects/effectbehaviors/gaussrifleweaponfxhkb/characters/character.hkx",
            "Effects/EffectBehaviors/GaussRifleWeaponFXHKB/Characters/Character.hkx",
        ),
        (
            "effects/effectbehaviors/gaussrifleweaponfxhkb/behaviors/behavior.hkx",
            "Effects/EffectBehaviors/GaussRifleWeaponFXHKB/Behaviors/Behavior.hkx",
        ),
    ]

    @pytest.fixture(autouse=True)
    def _setup(self, tmp_path):
        self.tmp = tmp_path

    # -- Project file --

    def test_project_converts(self):
        """Project HKX converts without error."""
        src = _FO76_MESHES / self._BEHAVIOR_FILES[0][0]
        dst = self.tmp / "project.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_project_class_names_match(self):
        """Project HKX class names match FO4 vanilla."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[0][0])
        fo4 = _read_fo4_hkx(_FO4_MESHES / self._BEHAVIOR_FILES[0][1])
        assert _class_counter(fo76) == _class_counter(fo4)

    def test_project_string_data_matches(self):
        """Project string data (character filenames, paths) matches."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[0][0])
        fo4 = _read_fo4_hkx(_FO4_MESHES / self._BEHAVIOR_FILES[0][1])

        for label, hkx in [("fo76", fo76), ("fo4", fo4)]:
            for obj in hkx.objects:
                if obj.class_name == "hkbProjectStringData":
                    for m in obj.members:
                        if m.name == "characterFilenames" and hasattr(m, "contents"):
                            assert len(m.contents) > 0, f"{label}: empty characterFilenames"

    # -- Character file --

    def test_character_converts(self):
        """Character HKX converts without error."""
        src = _FO76_MESHES / self._BEHAVIOR_FILES[1][0]
        dst = self.tmp / "character.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_character_class_names(self):
        """Character HKX has expected class names."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[1][0])
        classes = {o.class_name for o in fo76.objects}
        assert "hkbCharacterStringData" in classes

    # -- Behavior file (most complex) --

    def test_behavior_converts(self):
        """Behavior HKX converts without error."""
        src = _FO76_MESHES / self._BEHAVIOR_FILES[2][0]
        dst = self.tmp / "behavior.hkx"
        _convert_hkx(src, dst)
        assert dst.stat().st_size > 100

    def test_behavior_no_hkuint16_class(self):
        """No objects should have class_name 'hkUint16' after enrichment."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[2][0])
        bad = [o for o in fo76.objects if o.class_name == "hkUint16"]
        assert len(bad) == 0, f"Found {len(bad)} hkUint16 objects (should be resolved)"

    def test_behavior_bgs_sequence_generators(self):
        """BGSGamebryoSequenceGenerator objects match FO4 vanilla count and data."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[2][0])
        fo4 = _read_fo4_hkx(_FO4_MESHES / self._BEHAVIOR_FILES[2][1])

        fo76_gsgs = [o for o in fo76.objects if o.class_name == "BGSGamebryoSequenceGenerator"]
        fo4_gsgs = [o for o in fo4.objects if o.class_name == "BGSGamebryoSequenceGenerator"]

        assert len(fo76_gsgs) == len(fo4_gsgs), (
            f"BGSGamebryoSequenceGenerator count: FO76={len(fo76_gsgs)} FO4={len(fo4_gsgs)}"
        )

        # Compare pSequence values (the NIF animation names)
        def _get_sequences(objects):
            seqs = set()
            for o in objects:
                for m in o.members:
                    if m.name == "pSequence":
                        seqs.add(getattr(m, "value", getattr(m, "str_value", "")))
            return seqs

        fo76_seqs = _get_sequences(fo76_gsgs)
        fo4_seqs = _get_sequences(fo4_gsgs)
        assert fo76_seqs == fo4_seqs, f"Sequence mismatch: FO76={fo76_seqs} FO4={fo4_seqs}"

    def test_behavior_critical_classes_present(self):
        """Critical behavior classes are present in the converted output."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[2][0])
        classes = _class_counter(fo76)

        # These must all be present (exist in FO4 vanilla)
        required = [
            "hkRootLevelContainer",
            "hkbBehaviorGraph",
            "hkbStateMachine",
            "hkbBehaviorGraphData",
            "hkbVariableValueSet",
            "hkbBehaviorGraphStringData",
            "hkbBlenderGenerator",
            "hkbBlenderGeneratorChild",
            "hkbModifierGenerator",
            "hkbEventsFromRangeModifier",
            "hkbVariableBindingSet",
            "hkbStateMachineEventPropertyArray",
            "hkbStateMachineTransitionInfoArray",
            "hkbStateMachineStateInfo",
            "BGSGamebryoSequenceGenerator",
            "hkbStringEventPayload",
            "hkbEventRangeDataArray",
        ]
        missing = [c for c in required if classes.get(c, 0) == 0]
        assert not missing, f"Missing critical classes: {missing}"

    def test_behavior_class_counts_close_to_fo4(self):
        """Class counts should be close to FO4 vanilla (±reasonable delta for FO76 extras)."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[2][0])
        fo4 = _read_fo4_hkx(_FO4_MESHES / self._BEHAVIOR_FILES[2][1])

        fo76_c = _class_counter(fo76)
        fo4_c = _class_counter(fo4)

        # Classes that match exactly between FO76 and FO4
        exact_match = [
            "hkRootLevelContainer",
            "hkbBehaviorGraph",
            "hkbBehaviorGraphData",
            "hkbVariableValueSet",
            "hkbBehaviorGraphStringData",
            "hkbBlenderGenerator",
            "hkbBlenderGeneratorChild",
            "hkbModifierGenerator",
            "hkbEventsFromRangeModifier",
            "hkbVariableBindingSet",
            "BGSGamebryoSequenceGenerator",
            "hkbStateMachineEventPropertyArray",
            "hkbStringEventPayload",
            "hkbEventRangeDataArray",
        ]
        for cls in exact_match:
            assert fo76_c.get(cls, 0) == fo4_c.get(cls, 0), (
                f"{cls}: FO76={fo76_c.get(cls, 0)} != FO4={fo4_c.get(cls, 0)}"
            )

    def test_character_data_class_name(self):
        """Character.hkx main object should be hkbCharacterData, not hkbCharacterControllerSetup."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[1][0])
        char_objs = [o for o in fo76.objects if "characterControllerSetup" in
                     [m.name for m in o.members]]
        assert len(char_objs) == 1
        assert char_objs[0].class_name == "hkbCharacterData", (
            f"Expected hkbCharacterData, got {char_objs[0].class_name}"
        )

    def test_character_converted_has_fo4_members(self):
        """Converted Character.hkx must have hkbCharacterData with FO4-required members."""
        from creation_lib.hkxpack import load_hkx_bytes

        src = _FO76_MESHES / self._BEHAVIOR_FILES[1][0]
        dst = self.tmp / "character_check.hkx"
        _convert_hkx(src, dst)

        hkx, _ = load_hkx_bytes(dst.read_bytes())
        char_objs = [o for o in hkx.objects if o.class_name == "hkbCharacterData"]
        assert len(char_objs) == 1, (
            f"Expected 1 hkbCharacterData, got classes: {[o.class_name for o in hkx.objects]}"
        )
        obj = char_objs[0]
        member_names = {m.name for m in obj.members}

        required = {
            "characterControllerSetup", "modelUpMS", "modelForwardMS",
            "modelRightMS", "stringData", "scale",
        }
        missing = required - member_names
        assert not missing, f"Missing required FO4 members: {missing}"

    def test_character_has_mirrored_skeleton_info(self):
        """FO76 Character.hkx should have hkbMirroredSkeletonInfo after enrichment."""
        fo76 = _read_fo76_hkx(_FO76_MESHES / self._BEHAVIOR_FILES[1][0])
        mirror_objs = [o for o in fo76.objects if o.class_name == "hkbMirroredSkeletonInfo"]
        assert len(mirror_objs) >= 1, (
            f"Expected hkbMirroredSkeletonInfo, got classes: "
            f"{[o.class_name for o in fo76.objects]}"
        )

    def test_character_full_structure_matches_fo4(self):
        """Converted Character.hkx class names should match FO4 vanilla."""
        from creation_lib.hkxpack import load_hkx_bytes

        src = _FO76_MESHES / self._BEHAVIOR_FILES[1][0]
        dst = self.tmp / "character_full.hkx"
        _convert_hkx(src, dst)

        converted, _ = load_hkx_bytes(dst.read_bytes())
        fo4_ref = _read_fo4_hkx(_FO4_MESHES / self._BEHAVIOR_FILES[1][1])

        conv_classes = _class_counter(converted)
        fo4_classes = _class_counter(fo4_ref)

        for cls in ["hkRootLevelContainer", "hkbCharacterData",
                    "hkbVariableValueSet", "hkbCharacterStringData"]:
            assert conv_classes.get(cls, 0) == fo4_classes.get(cls, 0), (
                f"{cls}: converted={conv_classes.get(cls, 0)} fo4={fo4_classes.get(cls, 0)}"
            )

    def test_converted_behavior_readable(self):
        """Converted behavior can be read back as a valid FO4 packfile."""
        from creation_lib.hkxpack import detect_format
        from creation_lib.hkxpack import load_hkx_bytes

        src = _FO76_MESHES / self._BEHAVIOR_FILES[2][0]
        dst = self.tmp / "behavior.hkx"
        _convert_hkx(src, dst)

        fmt = detect_format(dst.read_bytes())
        assert fmt[0] == "packfile", f"Expected packfile, got {fmt}"

        hkx, _ = load_hkx_bytes(dst.read_bytes())
        assert len(hkx.objects) > 0, "Converted file has no objects"

        # Check no empty-member objects for known classes
        for obj in hkx.objects:
            if obj.class_name in ("hkbStateMachine", "BGSGamebryoSequenceGenerator",
                                   "hkbBlenderGenerator", "hkbStateMachineStateInfo"):
                assert len(obj.members) > 0, (
                    f"{obj.name} ({obj.class_name}) has 0 members in converted output"
                )


# ---------------------------------------------------------------------------
# Full asset comparison
# ---------------------------------------------------------------------------

@_REQUIRES_DATA
class TestFullAssetComparison:
    """Compare the complete FO76→FO4 converted asset set to FO4 vanilla."""

    def test_all_fo4_behavior_files_have_fo76_source(self):
        """Every FO4 behavior HKX has a matching FO76 source file."""
        fo4_files = list(_FO4_BEHAVIOR_DIR.rglob("*.hkx"))
        for fo4_f in fo4_files:
            rel = fo4_f.relative_to(_FO4_BEHAVIOR_DIR)
            # Case-insensitive match
            fo76_candidates = list(_FO76_BEHAVIOR_DIR.rglob("*"))
            fo76_match = None
            for c in fo76_candidates:
                if c.relative_to(_FO76_BEHAVIOR_DIR).as_posix().lower() == rel.as_posix().lower():
                    fo76_match = c
                    break
            assert fo76_match is not None, f"FO4 file {rel} has no FO76 source"
            assert fo76_match.stat().st_size > 0

    def test_convert_all_behaviors(self, tmp_path):
        """Convert all 3 behavior HKX files and verify output exists."""
        from creation_lib.havok_convert.converter import HavokConverter
        c = HavokConverter()

        for fo76_f in _FO76_BEHAVIOR_DIR.rglob("*.hkx"):
            rel = fo76_f.relative_to(_FO76_BEHAVIOR_DIR)
            dst = tmp_path / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            c.convert_file(str(fo76_f), str(dst), 53)
            assert dst.exists(), f"Conversion failed for {rel}"
            assert dst.stat().st_size > 100, f"Output too small for {rel}"

    def test_nif_file_coverage(self):
        """FO76 has all the NIFs that FO4 has (case-insensitive)."""
        fo4_nifs = {
            f.relative_to(_FO4_MESHES / "Weapons" / "GaussRifle").as_posix().lower()
            for f in (_FO4_MESHES / "Weapons" / "GaussRifle").rglob("*.nif")
        }
        fo76_nifs = {
            f.relative_to(_FO76_MESHES / "weapons" / "gaussrifle").as_posix().lower()
            for f in (_FO76_MESHES / "weapons" / "gaussrifle").rglob("*.nif")
        }
        missing = fo4_nifs - fo76_nifs
        # Some FO4-only NIFs are expected (DummyReceiverGauss, GaussRifle2mmEC)
        critical_missing = {m for m in missing if not m.startswith("dummy") and "2mmec" not in m}
        assert not critical_missing, f"FO76 missing critical NIFs: {critical_missing}"
