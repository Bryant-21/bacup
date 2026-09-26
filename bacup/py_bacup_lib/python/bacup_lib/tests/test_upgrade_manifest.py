import pytest
from bacup_lib.source_pairs import SOURCE_PAIRS
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


def _manifest(*versions):
    return UpgradeManifest(
        current=versions[-1][0],
        versions=tuple(_version(v, f) for v, f in versions),
    )


M = (("alpha1", ("ALL",)), ("alpha2", ("Meshes", "Materials")), ("alpha3", ("Terrain",)))


@pytest.mark.parametrize(
    ("versions", "from_id", "to_id", "expected"),
    [
        (M, "alpha2", "alpha3", {"Terrain"}),
        (M, "alpha1", "alpha3", {"Meshes", "Materials", "Terrain"}),
        (M, "alpha3", "alpha3", {"Terrain"}),
        (M, "prealpha", "alpha3", {"ALL"}),
        (M, None, "alpha3", {"ALL"}),
        ((("alpha2", ("Meshes",)), ("alpha10", ("Scripts",))), "alpha2", "alpha10", {"Scripts"}),
        ((("alpha2", ("ALL",)), ("alpha2.1", ("NIFs", "Havok"))), "alpha2.1", "alpha2.1", {"NIFs", "Havok"}),
        ((("alpha2", ("ALL",)),), "alpha2", "alpha2", {"ALL"}),
        (
            (("alpha1", ("Meshes",)), ("alpha2", ("Terrain",)), ("alpha3", ("Scripts",))),
            "alpha1", "alpha3", {"Terrain", "Scripts"},
        ),
        (
            (("alpha1", ("Meshes",)), ("alpha2", ("Scripts",)), ("alpha3", ("Terrain",))),
            "alpha3", "alpha3", {"Terrain"},
        ),
    ],
)
def test_resolve_family_union(versions, from_id, to_id, expected):
    assert resolve_family_union(
        _manifest(*versions), from_id, to_id, conversion_id=PAIR
    ) == frozenset(expected)


def test_downgrade_raises():
    with pytest.raises(ValueError):
        resolve_family_union(_manifest(*M), "alpha3", "alpha2", conversion_id=PAIR)


@pytest.mark.parametrize(
    ("body", "match"),
    [
        ("  - id: alpha3\n    families: [Meshes]\n", "legacy global field"),
        (
            "  - id: alpha3\n    families_by_conversion:\n      'skyrimse:fo4': [NONE, Meshes]\n",
            "NONE cannot be combined",
        ),
    ],
)
def test_load_upgrade_manifest_rejects_invalid(tmp_path, body, match):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text("current: alpha3\nversions:\n" + body, encoding="utf-8")

    with pytest.raises(ValueError, match=match):
        load_upgrade_manifest(manifest_path)


def test_load_upgrade_manifest_parses_per_conversion_fields(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha3\n"
        "versions:\n"
        "  - id: alpha3\n"
        "    families_by_conversion:\n"
        "      'fo76:fo4': [Textures]\n"
        "      'skyrimse:fo4': [NONE]\n"
        "    force_regen_by_conversion:\n"
        "      'fo76:fo4': true\n"
        "      'skyrimse:fo4': false\n"
        "    notes_by_conversion:\n"
        "      'skyrimse:fo4':\n"
        "        - skyrim note\n",
        encoding="utf-8",
    )

    version = load_upgrade_manifest(manifest_path).versions[0]

    assert version.families_for_conversion("fo76:fo4") == ("Textures",)
    for conversion_id in set(SOURCE_PAIRS).difference({"fo76:fo4"}):
        assert version.families_for_conversion(conversion_id) == ()
    assert version.force_regen_for_conversion("fo76:fo4") is True
    assert version.force_regen_for_conversion("skyrimse:fo4") is False
    assert version.force_regen_for_conversion("fnvfo3:fo4") is False
    assert version.notes_for_conversion("skyrimse:fo4") == ("skyrim note",)
    assert version.notes_for_conversion("fo76:fo4") == ()


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


def test_load_bundled_upgrade_manifest():
    manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
    assert manifest.index_of(manifest.current) is not None
    for version in manifest.versions:
        for conversion_id in SOURCE_PAIRS:
            version.families_for_conversion(conversion_id)


def test_load_upgrade_manifest_parses_known_issues(tmp_path):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha3\n"
        "known_issues_by_conversion:\n"
        "  'fo76:fo4':\n"
        "    - first issue\n"
        "    - second issue\n"
        "versions:\n"
        "  - id: alpha3\n"
        "    families_by_conversion:\n"
        "      'fo76:fo4': [NONE]\n",
        encoding="utf-8",
    )

    manifest = load_upgrade_manifest(manifest_path)

    assert manifest.known_issues_for_conversion("fo76:fo4") == (
        "first issue",
        "second issue",
    )
    assert manifest.known_issues_for_conversion("skyrimse:fo4") == ()


@pytest.mark.parametrize(
    ("known_issues", "match"),
    [
        ("known_issues_by_conversion:\n  'nope:fo4':\n    - x\n", "unknown conversion"),
        ("known_issues_by_conversion:\n  'fo76:fo4': x\n", "must be a list"),
        ("known_issues_by_conversion: [x]\n", "must be a mapping"),
    ],
)
def test_load_upgrade_manifest_rejects_invalid_known_issues(tmp_path, known_issues, match):
    manifest_path = tmp_path / "upgrade_manifest.yaml"
    manifest_path.write_text(
        "current: alpha3\n"
        + known_issues
        + "versions:\n  - id: alpha3\n    families_by_conversion:\n      'fo76:fo4': [NONE]\n",
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match=match):
        load_upgrade_manifest(manifest_path)
