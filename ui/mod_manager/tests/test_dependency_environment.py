import threading

import pytest

from creation_lib.mod_manager.dependency_environment import known_game_files
from creation_lib.mod_manager.fomod import Environment, Fomod, matches
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_standalone import game_fixture


def test_fomod_base_dlc_inactive_missing_and_requested_assets_are_read_only(tmp_path):
    game = game_fixture(tmp_path)
    for name in ("Fallout4.esm", "DLCRobot.esm", "Inactive.esp"):
        (game / "Data" / name).write_bytes(b"generated plugin placeholder")
    (game / "Data" / "textures").mkdir()
    (game / "Data" / "textures" / "physical.txt").write_bytes(b"generated asset")
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game)
    fomod = Fomod(b'''<config><moduleName>Game context</moduleName><moduleDependencies><dependencies>
        <fileDependency file="Fallout4.esm" state="Active"/><fileDependency file="DLCRobot.esm" state="Active"/>
        <fileDependency file="Inactive.esp" state="Inactive"/><fileDependency file="Missing.esp" state="Missing"/>
        <fileDependency file="textures/physical.txt" state="Active"/>
        </dependencies></moduleDependencies></config>''')
    original = {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()}
    known = known_game_files(library, profile_id, fomod)
    environment = Environment(known, complete=False)
    assert matches(fomod.root.find("moduleDependencies"), environment, {})
    assert {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()} == original
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        known_game_files(library, profile_id, fomod, cancel)


def test_missing_non_plugin_asset_remains_unknown_because_archives_may_supply_it(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game_fixture(tmp_path))
    fomod = Fomod(b'''<config><moduleName>Archive unknown</moduleName><moduleDependencies>
        <fileDependency file="textures/possibly-archived.dds" state="Missing"/>
        </moduleDependencies></config>''')
    known = known_game_files(library, profile_id, fomod)
    assert "textures/possibly-archived.dds" not in known
    with pytest.raises(ManagerError, match="unknown"):
        matches(fomod.root.find("moduleDependencies"), Environment(known, complete=False), {})
