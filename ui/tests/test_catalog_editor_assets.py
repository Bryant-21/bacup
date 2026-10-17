"""AssetSource: loose files and BA2 listings (archive reader monkeypatched), path conversions, browser filtering."""

from __future__ import annotations

import pytest

from ui.tools.catalog_editor.assets import (
    AssetBrowser,
    AssetFile,
    AssetSource,
    filter_files,
    folder_tree,
    to_full_path,
    to_stored_path,
)


def touch(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"x")


@pytest.fixture
def roots(tmp_path):
    plugin_dir, data = tmp_path / "mod", tmp_path / "Data"
    touch(plugin_dir / "Meshes" / "Weapons" / "Gun.nif")
    touch(plugin_dir / "Meshes" / "Weapons" / "readme.txt")
    touch(plugin_dir / "Textures" / "a" / "tex.dds")
    touch(data / "Meshes" / "weapons" / "gun.nif")
    touch(data / "Meshes" / "Armor" / "Vest.nif")
    touch(data / "Interface" / "Menu.swf")
    touch(data / "Sound" / "fx" / "boom.wav")
    touch(data / "Materials" / "m.bgsm")
    (data / "Fallout4 - Meshes.ba2").write_bytes(b"BTDX")
    (tmp_path / "elsewhere").mkdir()
    touch(tmp_path / "elsewhere" / "Meshes" / "Hidden.nif")
    return plugin_dir, data


def make(roots, listing=None):
    names = {"Fallout4 - Meshes.ba2": ["meshes/Creatures/Mole.nif", "Meshes/Armor/vest.nif", "textures/x.dds"]}
    calls = []

    def lister(path):
        calls.append(path)
        from pathlib import Path

        return (names if listing is None else listing).get(Path(path).name)

    source = AssetSource(roots, list_archive=lister)
    return source, calls


def listed(source, kind, root=""):
    source.start(root)
    source.wait()
    return source.snapshot(kind, root)


def test_nif_listing_merges_loose_roots_and_archives_without_duplicates(roots):
    source, _ = make(roots)
    files = listed(source, "nif", "Meshes")
    paths = {f.path.casefold(): f.source for f in files}
    assert set(paths) == {"meshes\\weapons\\gun.nif", "meshes\\armor\\vest.nif", "meshes\\creatures\\mole.nif"}
    assert paths["meshes\\weapons\\gun.nif"] == "mod"
    assert paths["meshes\\armor\\vest.nif"] == "Data"
    assert paths["meshes\\creatures\\mole.nif"] == "Fallout4 - Meshes.ba2"
    assert "Hidden" not in " ".join(paths)
    source.close()


def test_kinds_filter_by_extension(roots):
    source, _ = make(roots)
    assert [f.path for f in listed(source, "swf", "Interface")] == ["Interface\\Menu.swf"]
    assert [f.path for f in listed(source, "sound", "Sound")] == ["Sound\\fx\\boom.wav"]
    assert [f.path for f in listed(source, "material", "Materials")] == ["Materials\\m.bgsm"]
    textures = {f.path for f in listed(source, "texture", "Textures")}
    assert textures == {"Textures\\a\\tex.dds", "textures\\x.dds"}
    everything = {f.path.casefold() for f in listed(source, "any", "")}
    assert "meshes\\weapons\\readme.txt" in everything and "interface\\menu.swf" in everything
    source.close()


def test_archives_are_listed_once_per_mtime(roots):
    source, calls = make(roots)
    listed(source, "nif", "Meshes")
    listed(source, "texture", "Textures")
    assert len(calls) == 1
    source.close()


def test_snapshot_is_empty_until_the_worker_finishes_and_never_raises(roots):
    source, _ = make(roots, listing={})
    assert source.snapshot("nif", "Meshes") == []
    assert {f.path.casefold() for f in listed(source, "nif", "Meshes")} == {
        "meshes\\weapons\\gun.nif", "meshes\\armor\\vest.nif"}
    source.close()


def test_exists_ignores_case_and_slash_style_and_strip_root(roots):
    source, _ = make(roots)
    listed(source, "nif", "Meshes")
    assert source.exists("weapons/GUN.nif", "nif", "Meshes", strip_root=True)
    assert source.exists("MESHES/armor/vest.NIF", "nif", "Meshes", strip_root=False)
    assert not source.exists("weapons/none.nif", "nif", "Meshes", strip_root=True)
    source.close()


def test_missing_roots_are_ignored(tmp_path):
    source = AssetSource([tmp_path / "nope", ""])
    assert source.roots == []
    source.close()


def test_path_conversion_round_trip():
    assert to_stored_path("Meshes\\A\\b.nif", "Meshes", True) == "A\\b.nif"
    assert to_stored_path("meshes/A/b.nif", "Meshes", True) == "A\\b.nif"
    assert to_stored_path("Meshes\\A\\b.nif", "Meshes", False) == "Meshes\\A\\b.nif"
    assert to_stored_path("Sound\\a.wav", "Meshes", True) == "Sound\\a.wav"
    assert to_full_path("A/b.nif", "Meshes", True) == "Meshes\\A\\b.nif"
    assert to_full_path("Meshes\\A\\b.nif", "Meshes", True) == "Meshes\\A\\b.nif"
    assert to_full_path("A\\b.nif", "", True) == "A\\b.nif"
    assert to_full_path("", "Meshes", True) == ""


def test_folder_tree_and_filter():
    files = [AssetFile("Meshes\\A\\x.nif", "s"), AssetFile("Meshes\\A\\B\\y.nif", "s"), AssetFile("Meshes\\C\\z.nif", "t")]
    assert folder_tree(f.path for f in files) == {"Meshes": {"A": {"B": {}}, "C": {}}}
    assert [f.path for f in filter_files(files, "meshes\\a", "")] == ["Meshes\\A\\x.nif", "Meshes\\A\\B\\y.nif"]
    assert [f.path for f in filter_files(files, "", "b y")] == ["Meshes\\A\\B\\y.nif"]


def test_browser_request_take_and_headless_draw(roots):
    from imgui_bundle import imgui

    source, _ = make(roots)
    browser = AssetBrowser(source)
    context = imgui.create_context()
    io = imgui.get_io()
    io.display_size = imgui.ImVec2(1600, 1000)
    io.delta_time = 1 / 60
    io.backend_flags |= imgui.BackendFlags_.renderer_has_textures
    try:
        browser.request("slot", {"kind": "nif", "root": "Meshes", "strip_root": True}, "Weapons\\Gun.nif")
        assert browser._typed == "Meshes\\Weapons\\Gun.nif"
        source.wait()
        for _ in range(4):
            imgui.new_frame()
            imgui.begin("t")
            browser.draw()
            imgui.end()
            imgui.end_frame()
        assert [f.path.casefold() for f in browser._shown][:1] == ["meshes\\armor\\vest.nif"]
        browser.landed["slot"] = "Meshes\\x.nif"
        assert browser.take("slot") == "Meshes\\x.nif" and browser.take("slot") is None
    finally:
        imgui.destroy_context(context)
        source.close()
