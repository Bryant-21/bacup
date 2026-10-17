from dataclasses import replace

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.conflicts import FileEntry, FileIndex, Provider, rows_signature
from creation_lib.mod_manager.nexus_state import File, State
from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.status_query import inventory, normalized
from creation_lib.mod_manager.tree import ModTree
from .test_tree_status import row
from .test_launch_targets import targets  # noqa: F401
from .test_download_install import install_owned


def synthetic():
    rows = [row("anchor", "Group", 0, "Root", "separator"),
            replace(row("a", "A", 1, "Root/Sub"), nexus_game="fallout4", nexus_mod_id=123, nexus_file_id=1),
            replace(row("b", "B", 2, "Root/Sub"), nexus_game="fallout4", nexus_mod_id=124, nexus_file_id=3),
            row("c", "C", 3, "Other"), row("empty", "Empty", 4),
            replace(row("missing", "Missing", 5), source=""), replace(row("off", "Off", 6), enabled=False)]
    def provider(mod, enabled=True, hidden=False):
        return Provider(mod.id, mod.name, mod.priority, enabled, hidden, mod.source + "/file.txt", 1, 1)
    index = FileIndex("p", "Fixture", (FileEntry("shared.txt", (provider(rows[1]), provider(rows[2]))),
                                        FileEntry("alone.txt", (provider(rows[1]),)),
                                        FileEntry("c.txt", (provider(rows[3]),)),
                                        FileEntry("off.txt", (provider(rows[-1], enabled=False),))), (), rows_signature(rows))
    accounts = {("fallout4", 123): State("fallout4", 123, (File(2, "new.zip", "2", "Main"),), ((1, 2),), "Endorsed", True),
                ("fallout4", 124): State("fallout4", 124, (), (), "Abstained", False)}
    return rows, index, accounts


def test_status_combines_filters_retains_anchors_and_priority():
    m = ManagerController()
    try:
        rows, m.file_index, m.nexus_states = synthetic()
        m.rows, m.updates = rows, {("fallout4", 124): "other version"}
        chosen = m.filtered(statuses={"conflicts": "loses", "updates": "published", "endorsement": "endorsed", "tracking": "tracked"})
        assert [item.id for item in chosen] == ["a"]
        tree = ModTree(rows, chosen)
        assert [node.key for node in tree.visible({"group:Root"}, reveal=True)] == ["group:Root", "group:Root/Sub", "a"]
        assert tree.groups["Root"].row.id == "anchor"
        assert [item.priority for item in rows] == list(range(7))
        assert [item.id for item in m.filtered(statuses={"updates": "different"})] == ["b"]
        assert [item.id for item in m.filtered(statuses={"endorsement": "unendorsed"})] == ["b"]
        assert [item.id for item in m.filtered(statuses={"tracking": "untracked"})] == ["b"]
    finally:
        m.cleanup()


@pytest.mark.parametrize("state,expected", [("conflicting", {"a", "b"}), ("wins", {"b"}), ("loses", {"a"}),
                                            ("uncontested", {"c"}), ("inactive", {"off"}), ("empty", {"empty"}), ("unavailable", {"missing"})])
def test_measured_loose_states_are_distinct(state, expected):
    m = ManagerController()
    try:
        m.rows, m.file_index, m.nexus_states = synthetic()
        assert {item.id for item in m.filtered(statuses={"conflicts": state})} == expected
    finally:
        m.cleanup()


def test_stale_partial_inventory_and_unknown_account_are_not_positive_results():
    rows, index, accounts = synthetic()
    partial = replace(index, issues=("Unavailable source inventory",))
    result = inventory(rows, partial, {}, accounts)
    assert result["a"]["conflicts"] == result["b"]["conflicts"] == "unknown"
    changed = [replace(item, enabled=False) if item.id == "a" else item for item in rows]
    result = inventory(changed, index, {}, {})
    assert result["a"]["conflicts"] == "unknown" and result["a"]["updates"] == "unknown"
    assert result["a"]["endorsement"] == result["a"]["tracking"] == "unknown"


def test_status_cache_invalidates_account_and_version_events_without_row_refresh():
    m = ManagerController()
    try:
        m.rows, m.file_index, m.nexus_states = synthetic()
        m.rows_by_id = {item.id: item for item in m.rows}
        chosen = m.filtered(statuses={"updates": "published"})
        assert m.filtered(statuses={"updates": "published"}) is chosen
        assert m.status_rows() is m.status_rows()
        m.disconnect_nexus()
        assert m.filtered(statuses={"updates": "published"}) == []
        assert m.filtered(statuses={"endorsement": "endorsed"}) == []
        m.updates[("fallout4", 124)] = "new"
        m._status_revision += 1
        assert [item.id for item in m.filtered(statuses={"updates": "different"})] == ["b"]
        priorities = [(item.id, item.priority) for item in m.rows]
        assert m.filtered(sort_by="updates")[0].id == "b"
        assert m.filtered(sort_by="updates", descending=True)[-1].id == "b"
        assert [(item.id, item.priority) for item in m.rows] == priorities
    finally:
        m.cleanup()


def test_target_overrides_clear_published_file_state_and_follow_effective_enable(targets, tmp_path):  # noqa: F811
    from creation_lib.mod_manager.launch_targets import TargetLibrary
    from creation_lib.mod_manager import target_mod_sources as sources
    library, profile, _, ids = targets
    mod = install_owned(library, profile, tmp_path)
    m = ManagerController()
    try:
        m.library, m.profile_id = library, profile
        m.refresh()
        account = State("fallout4", 123, (File(2, "new.zip", "2", "Main"),), ((1, 2),), "Endorsed", True)
        m.nexus_states[("fallout4", 123)] = account
        assert [item.id for item in m.filtered(statuses={"updates": "published"})] == [mod]
        variant = tmp_path / "variant"
        variant.mkdir()
        (variant / "file.txt").write_bytes(b"variant")
        sources.apply(library, sources.review(library, ids[0], profile, mod, source=str(variant), enabled=False), accept_identity=True)
        m.target_id, m._context_view = ids[0], TargetLibrary(library, ids[0])
        m.refresh()
        assert m.filtered(statuses={"updates": "published"}) == []
        assert m.filtered(enabled_only=True, statuses={"endorsement": "endorsed"}) == []
        assert m.filtered(statuses={"updates": "unknown", "endorsement": "endorsed"})[0].id == mod
        m.target_id, m._context_view = ids[1], None
        m.refresh()
        assert m.filtered(enabled_only=True, statuses={"updates": "published"})[0].id == mod
    finally:
        m.cleanup()


def test_large_fanout_status_queries_use_linear_graph_membership_without_per_row_traversal(monkeypatch):
    account = State("fallout4", 123, (File(20001, "new.zip", "2", "Main"),), tuple((i, i + 1) for i in range(1, 20001)))
    rows = [replace(row(str(i), str(i), i), nexus_game="fallout4", nexus_mod_id=123, nexus_file_id=i) for i in range(1, 20001)]
    monkeypatch.setattr(State, "replacements", lambda *args: pytest.fail("Per-row graph traversal"))
    assert len(account._updatable_files) == 20000
    values = inventory(rows, None, {}, {("fallout4", 123): account})
    assert len(values) == 20000 and all(value["updates"] == "published" for value in values.values())
    assert not account._replacement_cache


def test_invalid_status_filters_are_explicit():
    for value in ({"conflicts": "guess"}, {"unsupported": "all"}):
        with pytest.raises(ManagerError):
            normalized(value)
