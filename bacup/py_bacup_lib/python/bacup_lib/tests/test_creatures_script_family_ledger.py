from __future__ import annotations

from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User" / "Creatures"
)
PATCH_ROOT = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "script_patches"
    / "Creatures"
)
TODO_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "TODO.md"
CONTRACT_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "w4-creatures-script-family.md"
)

TARGET_SCRIPTS = {
    "_default/creaturevariantscript.psc",
    "_default/glowinglootdrop.psc",
    "_default/setonfirescript.psc",
    "bosslootdrop.psc",
    "honeybeastracescript.psc",
    "liberatorracescript.psc",
    "megaslothracescript.psc",
    "moleminerracescript.psc",
    "mrhandyselfdestructscript.psc",
    "robotselfdestructscript.psc",
    "rusherdeathexplosion.psc",
    "scorchbeastsummonallieseffectscript.psc",
    "scorchedracescript.psc",
    "sentrybotshoulderclusterscript.psc",
    "snallygasterracescript.psc",
}

MARKER_PATCHES = {
    "bosslootdrop.psc",
    "liberatorracescript.psc",
    "rusherdeathexplosion.psc",
}

ZERO_MEMBER_DEFERRED = {
    "megaslothracescript.psc",
    "moleminerracescript.psc",
    "scorchbeastsummonallieseffectscript.psc",
}

NONDEFECT_WITHOUT_PATCH = {
    "_default/glowinglootdrop.psc",
    "_default/setonfirescript.psc",
    "honeybeastracescript.psc",
    "robotselfdestructscript.psc",
    "scorchedracescript.psc",
    "snallygasterracescript.psc",
}


def _relative_scripts(root: Path) -> set[str]:
    return {
        path.relative_to(root).as_posix().lower()
        for path in root.rglob("*.psc")
    }


def _creatures_todo_rows() -> list[str]:
    return [
        line
        for line in TODO_PATH.read_text(encoding="utf-8").splitlines()
        if line.startswith("CREATURES-TODO|")
    ]


def test_every_requested_creature_script_has_one_documented_disposition():
    generated = _relative_scripts(GENERATED_ROOT)

    assert TARGET_SCRIPTS <= generated
    contract = CONTRACT_PATH.read_text(encoding="utf-8").lower()
    for script_name in TARGET_SCRIPTS:
        assert Path(script_name).stem in contract


def test_creatures_todo_rows_match_markers_and_zero_member_exceptions():
    rows = _creatures_todo_rows()
    scripts = {
        field.removeprefix("script=").split(":", 1)[-1].lower() + ".psc"
        for row in rows
        for field in row.split("|")
        if field.startswith("script=")
    }

    assert scripts == MARKER_PATCHES | ZERO_MEMBER_DEFERRED
    for row in rows:
        assert "|blocker=" in row
        assert "|removal=" in row
        assert "|contract=contracts/w4-creatures-script-family.md" in row
        assert "|evidence=contracts/w4-creatures-script-family.md" in row
        assert "|status=" in row


def test_creature_patches_have_exactly_the_documented_todo_markers():
    patches = _relative_scripts(PATCH_ROOT)

    for script_name in MARKER_PATCHES:
        source = (PATCH_ROOT / script_name).read_text(encoding="utf-8")
        assert source.splitlines().count("; TODO") == 1

    for script_name in ZERO_MEMBER_DEFERRED | NONDEFECT_WITHOUT_PATCH:
        assert script_name not in patches

    for script_name in TARGET_SCRIPTS & patches - MARKER_PATCHES:
        source = (PATCH_ROOT / script_name).read_text(encoding="utf-8")
        assert "; TODO" not in source


def test_shared_self_destruct_parent_is_patched_without_a_deferred_marker():
    parent = PATCH_ROOT / "_Default" / "SelfDestructScript.psc"

    assert parent.is_file()
    assert "; TODO" not in parent.read_text(encoding="utf-8")
