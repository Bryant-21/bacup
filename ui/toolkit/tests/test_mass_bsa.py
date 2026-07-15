from __future__ import annotations

from pathlib import Path

from ui.tools.assets import mass_bsa


def test_scan_mo2_mods_hides_archived_mods_but_counts_them(tmp_path, monkeypatch):
    loose = tmp_path / "Loose Mod"
    archived = tmp_path / "Archived Mod"
    empty = tmp_path / "Empty Mod"
    for path in (loose, archived, empty):
        path.mkdir()

    (loose / "Meshes").mkdir()
    (loose / "Meshes" / "thing.nif").write_bytes(b"nif")
    (loose / "Patch.esp").write_bytes(b"plugin")
    (archived / "Existing - Main.ba2").write_bytes(b"archive")

    monkeypatch.setattr(mass_bsa, "detect_game", lambda plugin, fallback="fo4": "skyrimse")

    mods = mass_bsa.scan_mo2_mods(tmp_path, default_game="fo4")

    by_name = {mod.name: mod for mod in mods}
    assert by_name["Loose Mod"].selected is True
    assert by_name["Loose Mod"].game == "skyrimse"
    assert by_name["Loose Mod"].asset_dirs == ["Meshes"]
    assert by_name["Archived Mod"].has_archive is True
    assert by_name["Archived Mod"].selected is False
    assert by_name["Empty Mod"].status == "No loose asset folders"


def test_convert_mod_to_archives_packs_and_removes_loose_asset_dirs(tmp_path, monkeypatch):
    mod_dir = tmp_path / "B21_Test"
    (mod_dir / "Meshes").mkdir(parents=True)
    (mod_dir / "Textures").mkdir()
    (mod_dir / "Meshes" / "thing.nif").write_bytes(b"nif")
    (mod_dir / "Textures" / "thing.dds").write_bytes(b"dds")
    (mod_dir / "B21_Test.esp").write_bytes(b"plugin")

    calls: list[tuple[str, str, str, list[str]]] = []

    def fake_pack(source_dir, output_path, archive_type, **kwargs):
        staged = sorted(path.relative_to(source_dir).as_posix() for path in Path(source_dir).rglob("*") if path.is_file())
        calls.append((Path(source_dir).name, Path(output_path).name, archive_type, staged))
        Path(output_path).write_bytes(b"archive")
        return len(staged)

    monkeypatch.setattr(mass_bsa.native_runtime, "pack_archive", fake_pack)

    mod = mass_bsa.MassBsaMod(
        path=mod_dir,
        name="B21_Test",
        has_archive=False,
        asset_dirs=["Meshes", "Textures"],
        plugin_names=["B21_Test.esp"],
        game="fo4",
    )

    written = mass_bsa.convert_mod_to_archives(mod)

    assert [path.name for path in written] == ["B21_Test - Main.ba2", "B21_Test - Textures.ba2"]
    assert calls == [
        ("main", "B21_Test - Main.ba2", "fo4", ["Meshes/thing.nif"]),
        ("textures", "B21_Test - Textures.ba2", "fo4dds", ["Textures/thing.dds"]),
    ]
    assert not (mod_dir / "Meshes").exists()
    assert not (mod_dir / "Textures").exists()
    assert (mod_dir / "B21_Test.esp").is_file()
