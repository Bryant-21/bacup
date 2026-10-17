import json

import pytest

from creation_lib.mod_manager import nexus_links
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import ManagerError
from .fixtures import make_mo2


@pytest.fixture
def links(tmp_path, monkeypatch):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0].id
    mod = next(row.id for row in library.rows(profile) if row.kind == "external")
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_mod_id=123 WHERE id=?", (mod,))
    opened = []
    monkeypatch.setattr(nexus_links, "open_browser", lambda url, **kwargs: opened.append((url, kwargs)) or True)
    return library, profile, mod, opened


def test_known_game_mod_identity_and_explicit_origin_game_open_only_static_https_pages(links):
    library, profile, mod, opened = links
    nexus_links.open_game(library, profile)
    nexus_links.open_mod(library, profile, mod)
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_game='skyrimse' WHERE id=?", (mod,))
    nexus_links.open_mod(library, profile, mod)
    assert opened == [(url, {"new": 2, "autoraise": True}) for url in
                      ("https://www.nexusmods.com/fallout4", "https://www.nexusmods.com/fallout4/mods/123",
                       "https://www.nexusmods.com/skyrimspecialedition/mods/123")]


@pytest.mark.parametrize("identity", [0, -1, 10**10, "not an ID"])
def test_unknown_or_invalid_mod_identity_does_not_open_any_browser(links, identity):
    library, profile, mod, opened = links
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_mod_id=? WHERE id=?", (identity, mod))
    with pytest.raises(ManagerError, match="known Nexus mod identity"):
        nexus_links.open_mod(library, profile, mod)
    assert not opened


def test_unknown_origin_game_and_missing_membership_never_guess_a_link(links):
    library, profile, mod, opened = links
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_game='https://unexpected.invalid/script' WHERE id=?", (mod,))
    with pytest.raises(ManagerError, match="game identity"):
        nexus_links.open_mod(library, profile, mod)
    with library._db() as db:
        db.execute("DELETE FROM entries WHERE profile_id=? AND mod_id=?", (profile, mod))
    with pytest.raises(ManagerError, match="mod identity"):
        nexus_links.open_mod(library, profile, mod)
    assert not opened


def test_unknown_game_profile_and_browser_refusal_report_errors(links, monkeypatch):
    library, profile, mod, opened = links
    with pytest.raises(ManagerError, match="existing local"):
        nexus_links.open_game(library, "missing")
    monkeypatch.setattr(nexus_links, "open_browser", lambda *args, **kwargs: False)
    with pytest.raises(ManagerError, match="did not accept"):
        nexus_links.open_game(library, profile)
    with library._db() as db:
        files = json.loads(db.execute("SELECT files FROM profiles WHERE id=?", (profile,)).fetchone()[0])
        files["_modbox_game"] = "Unknown"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
    with pytest.raises(ManagerError, match="known Nexus game"):
        nexus_links.open_game(library, profile)
    assert not opened
