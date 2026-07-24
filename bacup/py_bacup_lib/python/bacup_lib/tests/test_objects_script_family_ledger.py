from __future__ import annotations

from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User" / "Objects"
)
PATCH_ROOT = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "script_patches"
    / "Objects"
)
TODO_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "TODO.md"
CONTRACT_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "w4-objects-script-family.md"
)

CLASSIFIED_SCRIPTS = {
    "atxholidaynucleartree.psc",
    "atxslotmachinescript.psc",
    "audio2stateactivator.psc",
    "beehivecontainerscript.psc",
    "cb04_addfactionperkscript.psc",
    "cb04_addtofactionscript.psc",
    "destructibleaudio2stateactivator.psc",
    "lc096_legendarybosstrigger.psc",
    "lgvanimcontroller.psc",
    "ud002oldtunnelterminal.psc",
    "workshopconveyor.psc",
    "xpd_ac_casinogame.psc",
    "xpd_ac_horseracingscript.psc",
    "xpd_ac_slotmachine.psc",
    "xpd_ac_slotmachinescript_westvirginia.psc",
    "xpd_ac_slotmachinescript_x5.psc",
}

MARKER_PATCHES = {
    "atxslotmachinescript.psc",
    "lc096_legendarybosstrigger.psc",
    "lgvanimcontroller.psc",
    "xpd_ac_casinogame.psc",
    "xpd_ac_slotmachine.psc",
    "xpd_ac_slotmachinescript_x5.psc",
}

ZERO_MEMBER_DEFERRED = {
    "xpd_ac_horseracingscript.psc",
    "xpd_ac_slotmachinescript_westvirginia.psc",
}


def _objects_todo_rows() -> list[str]:
    return [
        line
        for line in TODO_PATH.read_text(encoding="utf-8").splitlines()
        if line.startswith("OBJECTS-TODO|")
    ]


def test_every_generated_objects_script_has_one_family_disposition():
    generated = {path.name.lower() for path in GENERATED_ROOT.glob("*.psc")}

    assert generated == CLASSIFIED_SCRIPTS
    contract = CONTRACT_PATH.read_text(encoding="utf-8").lower()
    for script_name in CLASSIFIED_SCRIPTS:
        assert script_name.removesuffix(".psc") in contract


def test_objects_todo_rows_match_markers_and_zero_member_exceptions():
    rows = _objects_todo_rows()
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
        assert "|contract=contracts/w4-objects-script-family.md" in row
        assert "|evidence=contracts/w4-objects-script-family.md" in row
        assert "|status=" in row


def test_objects_patches_have_exactly_the_documented_todo_markers():
    patches = {path.name.lower(): path for path in PATCH_ROOT.glob("*.psc")}

    for script_name in MARKER_PATCHES:
        source = patches[script_name].read_text(encoding="utf-8")
        assert source.splitlines().count("; TODO") == 1

    for script_name, path in patches.items():
        if script_name not in MARKER_PATCHES:
            assert "; TODO" not in path.read_text(encoding="utf-8")

    for script_name in ZERO_MEMBER_DEFERRED:
        assert script_name not in patches
