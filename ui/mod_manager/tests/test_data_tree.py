from creation_lib.mod_manager.conflicts import FileEntry, build_index
from creation_lib.mod_manager.data_tree import Tree
from creation_lib.mod_manager.instances import create
from .test_standalone import game_fixture
from .test_transactions import setup_owned


def test_virtual_hierarchy_collapses_filters_case_aliases_and_caches():
    entries = tuple(FileEntry(path, ()) for path in ("Textures/a.dds", "textures/Nested/b.dds", "textures/c.dds", "root.esp"))
    tree = Tree(entries)
    closed = tree.visible(set())
    assert [row.path for row in closed] == ["Textures", "root.esp"]
    assert tree.visible(set()) is closed
    expanded = tree.visible({"textures"})
    assert [row.path for row in expanded] == ["Textures", "Textures/Nested", "Textures/a.dds", "textures/c.dds", "root.esp"]
    filtered = tree.visible(set(), "b.dds")
    assert [(row.path, row.depth) for row in filtered] == [("Textures", 0), ("Textures/Nested", 1), ("textures/Nested/b.dds", 2)]


def test_data_game_mod_overwrite_providers_and_source_bytes_are_read_only(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    game = game_fixture(tmp_path)
    other_profile = create(library, "fo4", game)
    with library._db() as db:
        files = db.execute("SELECT files FROM profiles WHERE id=?", (other_profile,)).fetchone()[0]
        db.execute("UPDATE profiles SET files=? WHERE id=?", (files, profile))
    (game / "Data" / "file.txt").write_text("generated base file")
    overwrite = library.root / "runtime" / "profiles" / profile / "overwrite"
    overwrite.mkdir(parents=True)
    (overwrite / "file.txt").write_text("generated owned output")
    original = (library.root / "mods" / mod / "file.txt").read_bytes()
    index = build_index(library, profile)
    entry = next(entry for entry in index.files if entry.path == "file.txt")
    assert [p.name for p in entry.providers] == ["Game Data", "Fixture", "Runtime overwrite"]
    assert entry.winner.name == "Runtime overwrite" and index.tree is not None
    assert (game / "Data" / "file.txt").read_text() == "generated base file"
    assert (library.root / "mods" / mod / "file.txt").read_bytes() == original
