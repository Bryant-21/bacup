"""Generated archive/PE context only; sources stay inert and read-only."""
import struct

import pytest

from creation_lib.mod_manager import installer_context, installer_versions
from creation_lib.mod_manager.fomod import Fomod
from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.pe_metadata import packed_runtime
from .test_installer_context import context_fixture as context_fixture, script, change_files
from .test_archive_index import ba2_fixture
from .test_pe_metadata import image, fixture_version


@pytest.mark.parametrize("activation,expected", [("listed", "Active"), ("attached", "Active"), ("unlisted", "Missing"), ("nonstandard", None)])
def test_requested_archive_dependency_uses_selected_fo4_policy_without_mutation(context_fixture, activation, expected):
    library, profile, game, _, mod, _ = context_fixture
    name = {"listed": "Base.ba2", "attached": "Addon - Main.ba2", "unlisted": "Other.ba2", "nonstandard": "Addon - Odd.ba2"}[activation]
    archive = ba2_fixture(mod / name, ["textures/generated.txt", "Packed.esp"])
    if activation == "listed":
        change_files(library, profile, **{"Fallout4.ini": "[Archive]\nsResourceArchiveList=Base.ba2\n"})
    original = archive.read_bytes()
    context, states = installer_context.capture(library, profile, script("textures/generated.txt", "Packed.esp", "textures/absent.txt"))
    assert states.get("textures/generated.txt") == expected
    assert states["packed.esp"] == "Missing"  # Plugins inside BA2 are not loadable loose plugins.
    assert states["textures/absent.txt"] == "Missing"
    assert context.archives.archives and archive.read_bytes() == original
    installer_context.validate(library, context)


@pytest.mark.parametrize("change", ["delete", "replace-members", "deactivate", "ini", "game-folder"])
def test_archive_dependency_changes_invalidate_review(context_fixture, change, tmp_path):
    library, profile, game, mod_id, mod, _ = context_fixture
    archive = ba2_fixture(mod / "Base.ba2", ["textures/generated.txt"])
    change_files(library, profile, **{"Fallout4.ini": "[Archive]\nsResourceArchiveList=Base.ba2\n"})
    context, _ = installer_context.capture(library, profile, script("textures/generated.txt"))
    if change == "delete":
        archive.unlink()
    elif change == "replace-members":
        ba2_fixture(archive, ["textures/replacement.txt"])
    elif change == "deactivate":
        library.customize(profile, mod_id, enabled=False)
    elif change == "ini":
        change_files(library, profile, **{"Fallout4.ini": "[Archive]\nsResourceArchiveList=\n"})
    else:
        replacement = tmp_path / "different generated game"
        (replacement / "Data").mkdir(parents=True)
        with library._db() as db:
            import json
            row = db.execute("SELECT id,configuration FROM instances LIMIT 1").fetchone()
            config = json.loads(row["configuration"])
            config["game"]["gamepath"] = str(replacement)
            db.execute("UPDATE instances SET configuration=? WHERE id=?", (json.dumps(config), row["id"]))
    with pytest.raises(ManagerError, match="changed|unavailable|Prepare"):
        installer_context.validate(library, context)


def test_unreadable_archive_cannot_prove_missing(context_fixture):
    library, profile, _, _, mod, _ = context_fixture
    (mod / "Broken.ba2").write_bytes(b"generated invalid archive")
    context, states = installer_context.capture(library, profile, script("textures/unknown.txt"))
    assert "textures/unknown.txt" not in states and context.archives.issues


def generated_f4se(game, monkeypatch):
    image(game / "Fallout4.exe", version=(1, 10, 163, 0))
    image(game / "f4se_loader.exe", version=(0, 6, 23, 0))
    image(game / "f4se_steam_loader.dll")
    image(game / "f4se_1_10_163.dll", "F4SECore_Version", struct.pack("<II", 1, packed_runtime((1, 10, 163))))
    (game / "Data" / "Scripts").mkdir()
    (game / "Data" / "Scripts" / "F4SE.pex").write_bytes(b"generated inert script fixture")
    monkeypatch.setattr(installer_versions, "file_version", fixture_version)
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", fixture_version)


def version_script():
    return Fomod(b'<config><moduleName>Versions</moduleName><moduleDependencies><gameDependency version="1.10.163"/><f4seDependency version="0.6.23"/></moduleDependencies></config>')


@pytest.mark.parametrize("change", ["none", "loader-version", "core-runtime", "missing-dll", "missing-script"])
def test_matching_f4se_discovery_and_mismatches_are_truthful(context_fixture, monkeypatch, change):
    library, profile, game, _, _, _ = context_fixture
    generated_f4se(game, monkeypatch)
    if change == "loader-version":
        image(game / "f4se_loader.exe", version=(9, 9, 9, 0))
    elif change == "core-runtime":
        image(game / "f4se_1_10_163.dll", "F4SECore_Version", struct.pack("<II", 1, packed_runtime((1, 10, 984))))
    elif change == "missing-dll":
        (game / "f4se_1_10_163.dll").unlink()
    elif change == "missing-script":
        (game / "Data" / "Scripts" / "F4SE.pex").unlink()
    before = {p: p.read_bytes() for p in game.iterdir() if p.is_file()}
    context, _ = installer_context.capture(library, profile, version_script())
    versions = dict(context.versions.versions)
    assert versions["game"] == "1.10.163.0"
    assert versions.get("f4se") == ("0.6.23" if change == "none" else None)
    assert {p: p.read_bytes() for p in before} == before
    if change == "none":
        installer_context.validate(library, context)
        image(game / "f4se_loader.exe", version=(9, 9, 9, 0))
        with pytest.raises(ManagerError, match="changed"):
            installer_context.validate(library, context)
    else:
        assert context.versions.issues
