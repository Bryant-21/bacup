from dataclasses import replace

from creation_lib.mod_manager.conflicts import FileEntry, FileIndex, Provider
from creation_lib.mod_manager.library import ModRow
from creation_lib.mod_manager.status import badges, file_states
from creation_lib.mod_manager.tree import ModTree


def row(identifier, name, priority, group="", kind="external"):
    return ModRow(identifier, name, "generated/source", kind, "1", "", "", "", priority, True, group)


def test_nested_tree_collapse_filter_reveal_anchors_and_priority_order():
    rows = [row("anchor", "Visual_separator", 0, "Visual", "separator"),
            row("a", "A", 1, "Visual/Textures"), row("b", "B", 2, "Visual/Lighting"), row("c", "C", 3)]
    tree = ModTree(rows, rows)
    assert [node.key for node in tree.roots] == ["group:Visual", "c"]
    assert tree.groups["Visual"].row.id == "anchor" and tree.groups["Visual"].count == 2
    assert [node.key for node in tree.visible({"group:Visual"})] == ["group:Visual", "c"]
    filtered = ModTree(rows, [rows[1]])
    assert [node.key for node in filtered.visible({"group:Visual"}, reveal=True)] == ["group:Visual", "group:Visual/Textures", "a"]
    assert filtered.groups["Visual"].priority == 0 and rows[1].priority == 1


def test_visual_sort_keeps_group_priority_and_duplicate_leaf_identity():
    rows = [row("a", "##same", 0, "First"), row("b", "##same", 1, "First"), row("c", "C", 2, "Last")]
    tree = ModTree(rows, list(reversed(rows)))
    assert [node.name for node in tree.roots] == ["First", "Last"]
    assert [node.key for node in tree.groups["First"].children] == ["b", "a"]
    assert [node.priority for node in tree.groups["First"].children] == [1, 0]


def test_large_tree_flattening_is_bounded_by_expansion():
    rows = [row(str(i), f"Mod {i}", i, f"Root/Group{i % 10}") for i in range(20_000)]
    tree = ModTree(rows, rows)
    assert len(tree.visible()) == 20_011
    assert len(tree.visible({"group:Root"})) == 1
    assert len(tree.visible({"group:Root"}, reveal=True)) == 20_011


def test_badges_distinguish_unknown_conflicts_versions_and_endorsement():
    mod = replace(row("mod", "Mod", 0), nexus_game="fallout4", nexus_mod_id=42)
    values = badges(mod, None, {}, {})
    assert [badge.text for badge in values] == ["?", "?", "?"]
    assert "unknown" in values[2].description and "Refresh Nexus" in values[2].description
    difference = badges(mod, None, {}, {("fallout4", 42): "0.5"})[1]
    assert difference.text == "≠" and "does not establish a newer" in difference.description


def test_loose_file_badges_and_incomplete_inventory_never_claim_known_precedence():
    a, b = row("a", "A", 0), row("b", "B", 1)
    def provider(mod):
        return Provider(mod.id, mod.name, mod.priority, True, False, "fixture/path", 1, 1)
    index = FileIndex("profile", "Fixture", (FileEntry("path", (provider(a), provider(b))),), (), ())
    states = file_states(index)
    assert badges(a, index, states, {})[0].text == "L"
    assert badges(b, index, states, {})[0].text == "W"
    contextual = replace(index, issues=("Unmanaged game content is not indexed: Generated DLC",))
    assert badges(a, contextual, states, {})[0].text == "L"
    assert badges(b, contextual, states, {})[0].text == "W"
    partial = replace(index, issues=("Unavailable source",), unavailable_mod_ids=("a",))
    assert badges(a, partial, states, {})[0].text == "!"
    assert badges(b, partial, states, {})[0].text == "?"


def test_fetched_file_replacements_and_account_state_drive_truthful_badges():
    from creation_lib.mod_manager.nexus_state import File, State
    mapped = replace(row("mapped", "Mapped", 0), nexus_game="fallout4", nexus_mod_id=123, nexus_file_id=1)
    account = State("fallout4", 123, (File(2, "new.zip", "2", "Main"),), ((1, 2),), "Endorsed", True)
    actual = badges(mapped, None, {}, {}, {("fallout4", 123): account})
    assert actual[1].text == "update" and actual[2].text == "endorsed"
    assert "published" in actual[1].description and "2" in actual[1].description
    for status in ("Undecided", "Abstained"):
        assert badges(mapped, None, {}, {}, {("fallout4", 123): replace(account, endorsement=status)})[2].text == "unendorsed"
    assert badges(mapped, None, {}, {}, {})[2].text == "?"
    variant = replace(mapped, nexus_file_id=0, version="explicit target version")
    assert badges(variant, None, {}, {}, {("fallout4", 123): account})[1].text == "?"
    assert badges(variant, None, {}, {}, {("fallout4", 123): account})[2].text == "endorsed"


def test_replacement_graph_cache_is_bounded_and_excludes_archived_leaves():
    from creation_lib.mod_manager.nexus_state import File, State
    files = (File(1000, "new.zip", "2", "Main"), File(1001, "archived.zip", "3", "Archived"))
    account = State("fallout4", 123, files, tuple((old, 1000) for old in range(1, 100)) + ((1, 1001),))
    for old in range(1, 100):
        assert account.replacements(old) == files[:1]
    assert len(account._replacement_cache) == 64 and 1 not in account._replacement_cache
    assert account.replacements(99) is account.replacements(99)
    assert not account.replacements(1000) and not account.replacements(0)
