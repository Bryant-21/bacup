import pytest
from bacup_lib.upgrade_manifest import (
    UpgradeManifest, UpgradeVersion, bundled_upgrade_manifest_path,
    load_upgrade_manifest, resolve_family_union,
    requires_forced_regen,
)

PAIR = "fo76:fo4"


def _version(version_id, families, *, force_regen=False, notes=()):
    return UpgradeVersion(
        version_id,
        families_by_conversion=((PAIR, tuple(families)),),
        force_regen_by_conversion=((PAIR, force_regen),),
        notes_by_conversion=((PAIR, tuple(notes)),) if notes else (),
    )


M = UpgradeManifest(
    current="alpha3",
    versions=(
        _version("alpha1", ("ALL",)),
        _version("alpha2", ("Meshes", "Materials")),
        _version("alpha3", ("Terrain",)),
    ),
)

def test_single_step():
    assert resolve_family_union(
        M, "alpha2", "alpha3", conversion_id=PAIR
    ) == frozenset({"Terrain"})

def test_multi_step_union():
    assert resolve_family_union(
        M, "alpha1", "alpha3", conversion_id=PAIR
    ) == frozenset({"Meshes", "Materials", "Terrain"})

def test_from_equals_target_runs_declared_family():
    assert resolve_family_union(
        M, "alpha3", "alpha3", conversion_id=PAIR
    ) == frozenset({"Terrain"})

def test_unknown_from_is_full_build():
    assert resolve_family_union(
        M, "prealpha", "alpha3", conversion_id=PAIR
    ) == frozenset({"ALL"})

def test_none_from_is_full_build():
    assert resolve_family_union(M, None, "alpha3", conversion_id=PAIR) == frozenset({"ALL"})

def test_all_in_range_is_full_build():
    assert resolve_family_union(
        M, "alpha0_before_alpha1", "alpha2", conversion_id=PAIR
    ) == frozenset({"ALL"})  # via unknown-from

def test_downgrade_raises():
    with pytest.raises(ValueError):
        resolve_family_union(M, "alpha3", "alpha2", conversion_id=PAIR)

def test_ordering_is_list_order_not_string():
    m = UpgradeManifest("alpha10", (
        _version("alpha2", ("Meshes",)),
        _version("alpha10", ("Scripts",)),
    ))
    assert resolve_family_union(
        m, "alpha2", "alpha10", conversion_id=PAIR
    ) == frozenset({"Scripts"})


def test_load_upgrade_manifest_rejects_legacy_global_fields(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha2\n"
        "versions:\n"
        "  - id: alpha2\n"
        "    families: [Meshes]\n",
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="legacy global field"):
        load_upgrade_manifest(manifest_path)


def test_load_upgrade_manifest_parses_force_regen_by_conversion(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha2\n"
        "versions:\n"
        "  - id: alpha2\n"
        "    families_by_conversion:\n"
        "      'fo76:fo4': [Meshes]\n"
        "      'fnvfo3:fo4': [NONE]\n"
        "      'skyrimse:fo4': [NONE]\n"
        "    force_regen_by_conversion:\n"
        "      'fo76:fo4': true\n",
        encoding="utf-8",
    )

    manifest = load_upgrade_manifest(manifest_path)

    assert manifest.versions[0].force_regen_for_conversion(PAIR) is True
    assert manifest.versions[0].force_regen_for_conversion("skyrimse:fo4") is False


def test_load_upgrade_manifest_parses_notes_by_conversion(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha4\n"
        "versions:\n"
        "  - id: alpha4\n"
        "    families_by_conversion:\n"
        "      'fo76:fo4': [NONE]\n"
        "      'fnvfo3:fo4': [NONE]\n"
        "      'skyrimse:fo4': [Textures]\n"
        "    notes_by_conversion:\n"
        "      'skyrimse:fo4':\n"
        "        - skyrim note\n",
        encoding="utf-8",
    )

    version = load_upgrade_manifest(manifest_path).versions[0]

    assert version.notes_for_conversion("skyrimse:fo4") == ("skyrim note",)
    assert version.notes_for_conversion("fo76:fo4") == ()


def test_load_upgrade_manifest_parses_conversion_family_and_force_overrides(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha3\n"
        "versions:\n"
        "  - id: alpha3\n"
        "    families_by_conversion:\n"
        "      'fo76:fo4': [Textures]\n"
        "      'skyrimse:fo4': [NONE]\n"
        "      'fnvfo3:fo4': [Meshes]\n"
        "    force_regen_by_conversion:\n"
        "      'fo76:fo4': true\n"
        "      'skyrimse:fo4': false\n",
        encoding="utf-8",
    )

    version = load_upgrade_manifest(manifest_path).versions[0]

    assert version.families_for_conversion("fo76:fo4") == ("Textures",)
    assert version.families_for_conversion("fnvfo3:fo4") == ("Meshes",)
    assert version.families_for_conversion("skyrimse:fo4") == ()
    assert version.force_regen_for_conversion("fo76:fo4") is True
    assert version.force_regen_for_conversion("skyrimse:fo4") is False


def test_conversion_scopes_skip_unrelated_versions_and_union_later_changes():
    manifest = UpgradeManifest(
        current="alpha4",
        versions=(
            UpgradeVersion(
                "alpha2",
                families_by_conversion=(("skyrimse:fo4", ("ALL",)),),
            ),
            UpgradeVersion(
                "alpha3",
                families_by_conversion=(("skyrimse:fo4", ("NONE",)),),
                force_regen_by_conversion=(("skyrimse:fo4", False),),
            ),
            UpgradeVersion(
                "alpha4",
                families_by_conversion=(("skyrimse:fo4", ("Meshes",)),),
                force_regen_by_conversion=(("skyrimse:fo4", True),),
            ),
        ),
    )

    assert resolve_family_union(
        manifest, "alpha2", "alpha3", conversion_id="skyrimse:fo4"
    ) == frozenset()
    assert resolve_family_union(
        manifest, "alpha2", "alpha4", conversion_id="skyrimse:fo4"
    ) == frozenset({"Meshes"})
    assert requires_forced_regen(
        manifest, "alpha2", "alpha3", conversion_id="skyrimse:fo4"
    ) is False
    assert requires_forced_regen(
        manifest, "alpha2", "alpha4", conversion_id="skyrimse:fo4"
    ) is True


def test_none_family_cannot_be_combined_with_other_families(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha3\n"
        "versions:\n"
        "  - id: alpha3\n"
        "    families_by_conversion:\n"
        "      'skyrimse:fo4': [NONE, Meshes]\n",
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="NONE cannot be combined"):
        load_upgrade_manifest(manifest_path)


def test_manifest_requires_explicit_family_scope_for_every_conversion(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha3\n"
        "versions:\n"
        "  - id: alpha3\n"
        "    families_by_conversion:\n"
        "      'fo76:fo4': [Textures]\n",
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match=r"use \[NONE\] for no changes"):
        load_upgrade_manifest(manifest_path)


def test_force_regen_applies_only_when_flagged_version_is_crossed():
    manifest = UpgradeManifest(
        "alpha3",
        (
            _version("alpha1", ("ALL",)),
            _version("alpha2", ("Meshes",), force_regen=True),
            _version("alpha3", ("Terrain",)),
        ),
    )

    assert requires_forced_regen(
        manifest, "alpha1", "alpha3", conversion_id=PAIR
    ) is True
    assert requires_forced_regen(
        manifest, "alpha2", "alpha3", conversion_id=PAIR
    ) is False
    assert requires_forced_regen(
        manifest, "alpha3", "alpha3", conversion_id=PAIR
    ) is False
    assert requires_forced_regen(
        manifest, None, "alpha3", conversion_id=PAIR
    ) is True


def test_load_bundled_upgrade_manifest_has_notes():
    manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
    by_id = {v.id: v for v in manifest.versions}
    assert manifest.current == "alpha2.1"
    assert by_id["alpha1"].notes_for_conversion("fo76:fo4") != ()
    assert by_id["alpha2"].notes_for_conversion("fo76:fo4") != ()
    assert by_id["alpha2.1"].notes_for_conversion("fo76:fo4") != ()
    assert by_id["alpha2.1"].families_for_conversion("fo76:fo4") == (
        "NIFs",
        "Havok",
        "Scripts",
        "Textures",
    )
    assert resolve_family_union(
        manifest,
        "alpha2",
        "alpha2.1",
        conversion_id="fo76:fo4",
    ) == frozenset({"NIFs", "Havok", "Scripts", "Textures"})
    assert requires_forced_regen(
        manifest,
        "alpha2",
        "alpha2.1",
        conversion_id="fo76:fo4",
    ) is False
    assert by_id["alpha2"].notes_for_conversion("skyrimse:fo4") != ()
    assert by_id["alpha1"].families_for_conversion("skyrimse:fo4") == ()
    assert by_id["alpha2"].families_for_conversion("skyrimse:fo4") == ()
    assert by_id["alpha2"].force_regen_for_conversion("skyrimse:fo4") is True
# --- target families --------------------------------------------------------


def test_target_scripts_family_runs_when_already_current():
    manifest = UpgradeManifest(
        current="alpha3",
        versions=(
            _version("alpha2", ("Meshes",)),
            _version("alpha3", ("Scripts",)),
        ),
    )

    assert resolve_family_union(
        manifest, "alpha3", "alpha3", conversion_id=PAIR
    ) == frozenset({"Scripts"})


def test_target_nifs_havok_families_run_when_already_current():
    manifest = UpgradeManifest(
        current="alpha2.1",
        versions=(
            _version("alpha2", ("ALL",)),
            _version("alpha2.1", ("NIFs", "Havok")),
        ),
    )

    assert resolve_family_union(
        manifest, "alpha2.1", "alpha2.1", conversion_id=PAIR
    ) == frozenset({"NIFs", "Havok"})


def test_target_textures_family_runs_when_already_current():
    manifest = UpgradeManifest(
        current="alpha2.1",
        versions=(
            _version("alpha2", ("ALL",)),
            _version("alpha2.1", ("Textures",)),
        ),
    )

    assert resolve_family_union(
        manifest, "alpha2.1", "alpha2.1", conversion_id=PAIR
    ) == frozenset({"Textures"})


def test_target_lod_family_runs_when_already_current():
    manifest = UpgradeManifest(
        current="alpha2.1",
        versions=(
            _version("alpha2", ("ALL",)),
            _version("alpha2.1", ("LOD",)),
        ),
    )

    assert resolve_family_union(
        manifest, "alpha2.1", "alpha2.1", conversion_id=PAIR
    ) == frozenset({"LOD"})


def test_target_all_runs_full_build_when_already_current():
    manifest = UpgradeManifest(
        current="alpha2",
        versions=(_version("alpha2", ("ALL",)),),
    )

    assert resolve_family_union(
        manifest, "alpha2", "alpha2", conversion_id=PAIR
    ) == frozenset({"ALL"})


def test_target_scripts_family_joins_version_range_changes():
    manifest = UpgradeManifest(
        current="alpha3",
        versions=(
            _version("alpha1", ("Meshes",)),
            _version("alpha2", ("Terrain",)),
            _version("alpha3", ("Scripts",)),
        ),
    )

    assert resolve_family_union(
        manifest, "alpha1", "alpha3", conversion_id=PAIR
    ) == frozenset({"Terrain", "Scripts"})


def test_historical_scripts_family_does_not_join_current_target_family():
    manifest = UpgradeManifest(
        current="alpha3",
        versions=(
            _version("alpha1", ("Meshes",)),
            _version("alpha2", ("Scripts",)),
            _version("alpha3", ("Terrain",)),
        ),
    )

    assert resolve_family_union(
        manifest, "alpha3", "alpha3", conversion_id=PAIR
    ) == frozenset({"Terrain"})
