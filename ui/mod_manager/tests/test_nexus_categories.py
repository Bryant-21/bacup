import copy
import json
import threading

import pytest

from creation_lib.mod_manager.categories import nexus_catalog, inventory, rename, save
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import ManagerError, Cancelled
from creation_lib.mod_manager.nexus import NexusClient
from .test_nexus import Response


CATALOG = [
    {"category_id": 3, "name": "Rifles", "parent_category": 2},
    {"category_id": 1, "name": "Fallout 4", "parent_category": False},
    {"category_id": 2, "name": "Weapons", "parent_category": 1},
]


class Client:
    def __init__(self, rows=None):
        self.rows, self.calls = copy.deepcopy(CATALOG if rows is None else rows), []

    def categories(self, game):
        self.calls.append(game)
        return self.rows


def test_full_hierarchy_cache_import_and_local_rename_survive_reopen(tmp_path):
    library = Library(tmp_path / "library", create=True)
    client = Client()
    save(library, "My own category", 0)
    first = nexus_catalog(library, client, "fallout4", True, True)
    assert first["imported"] == 3 and client.calls == ["fallout4"]
    assert next(r for r in first["categories"] if r["id"] == 3)["path"] == "Fallout 4/Weapons/Rifles"
    rename(library, "Fallout 4/Weapons", "Equipment", inventory(library)[1])
    reopened = Library(library.root)
    offline = nexus_catalog(reopened, None, "fallout4", False, True)
    assert offline["imported"] == 0
    assert next(r for r in offline["categories"] if r["id"] == 3)["local_name"] == "Equipment/Rifles"
    assert {r.name for r in inventory(reopened)[0]} == {"My own category", "Fallout 4", "Equipment", "Equipment/Rifles"}


@pytest.mark.parametrize("invalid", [[], CATALOG + [CATALOG[0]],
    [{"category_id": 1, "name": "Cycle", "parent_category": 1}],
    [{"category_id": 1, "name": "Missing", "parent_category": 9}],
    [{"category_id": 1, "name": "", "parent_category": False}]])
def test_invalid_catalog_preserves_previous_cache_and_local_categories(tmp_path, invalid):
    library = Library(tmp_path / "library", create=True)
    before = nexus_catalog(library, Client(), "fallout4", True, True)
    with pytest.raises(ManagerError):
        nexus_catalog(library, Client(invalid), "fallout4", True, True)
    after = nexus_catalog(library, None, "fallout4")
    assert after["categories"] == before["categories"]


def test_category_domains_remain_separate_and_cancel_does_not_commit(tmp_path):
    library = Library(tmp_path / "library", create=True)
    nexus_catalog(library, Client(), "fallout4", True, True)
    other = [{"category_id": 1, "name": "Skyrim", "parent_category": False}]
    nexus_catalog(library, Client(other), "skyrim", True, True)
    assert len(nexus_catalog(library, None, "skyrim")["categories"]) == 1
    assert len(nexus_catalog(library, None, "fallout4")["categories"]) == 3
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        nexus_catalog(library, Client(), "fallout4", True, True, cancel)


def test_api_category_contract_uses_game_details_endpoint():
    def transport(request, timeout):
        assert request.full_url == "https://api.nexusmods.com/v1/games/fallout4.json"
        return Response(json.dumps({"categories": CATALOG}).encode())
    assert NexusClient("fixture", transport=transport).categories("fallout4") == CATALOG
