from bacup_ui.storage_cleanup import (
    CleanupTarget,
    delete_cleanup_targets,
    discover_cleanup_targets,
    measure_cleanup_targets,
)


def test_discovers_only_safe_known_cleanup_targets(tmp_path):
    fo4_root = tmp_path / "Fallout 4"
    fo4_root.mkdir()
    fo4_extracted = tmp_path / "extracted" / "fo4"
    fo4_extracted.mkdir(parents=True)
    fo76_extracted = tmp_path / "extracted" / "fo76"
    geoexporter = fo76_extracted / "GeoExporter"
    vis = fo76_extracted / "VIS"
    geoexporter.mkdir(parents=True)
    vis.mkdir()
    temp_root = tmp_path / "Temp"
    expected_temp = temp_root / "hkxunpack_old"
    ignored_temp = temp_root / "unrelated-app"
    expected_temp.mkdir(parents=True)
    ignored_temp.mkdir()
    (temp_root / "bacup-current.log").write_text("keep", encoding="utf-8")
    legacy_local_data = tmp_path / "LocalAppData" / "modkit21" / "conversion"
    legacy_local_data.mkdir(parents=True)

    targets = discover_cleanup_targets(
        fo4_extracted_dir=fo4_extracted,
        fo76_extracted_dir=fo76_extracted,
        forbidden_roots=(fo4_root,),
        temp_root=temp_root,
        legacy_local_data_root=legacy_local_data,
    )

    by_key = {target.key: target for target in targets}
    assert set(by_key) == {
        "fo4_extracted",
        "fo76_geoexporter",
        "fo76_vis",
        "bacup_temp",
        "legacy_local_data",
    }
    assert by_key["fo76_geoexporter"].paths == (geoexporter,)
    assert by_key["fo76_vis"].paths == (vis,)
    assert by_key["bacup_temp"].paths == (expected_temp,)
    assert by_key["legacy_local_data"].paths == (legacy_local_data,)


def test_all_cleanup_categories_respect_protected_game_directories(tmp_path):
    game_root = tmp_path / "Fallout 4"
    game_data = game_root / "Data"
    for relative in ("GeoExporter", "VIS", "bacup-old", "legacy-cache"):
        directory = game_data / relative
        directory.mkdir(parents=True)
        (directory / "keep.bin").write_bytes(b"installed asset")

    targets = discover_cleanup_targets(
        fo4_extracted_dir=game_data,
        fo76_extracted_dir=game_data,
        forbidden_roots=iter((game_root,)),
        game_roots=iter((game_root,)),
        temp_root=game_data,
        legacy_local_data_root=game_data / "legacy-cache",
    )

    assert targets == ()
    assert len(list(game_data.rglob("keep.bin"))) == 4


def test_measure_and_delete_touch_only_selected_target(tmp_path):
    selected_dir = tmp_path / "selected"
    kept_dir = tmp_path / "kept"
    selected_dir.mkdir()
    kept_dir.mkdir()
    (selected_dir / "large.bin").write_bytes(b"12345")
    (kept_dir / "keep.bin").write_bytes(b"keep")
    targets = measure_cleanup_targets(
        (
            CleanupTarget("selected", "Selected", "", (selected_dir,)),
            CleanupTarget("kept", "Kept", "", (kept_dir,)),
        )
    )

    result = delete_cleanup_targets((targets[0],))

    assert result.deleted_keys == ("selected",)
    assert result.freed_bytes == 5
    assert not selected_dir.exists()
    assert kept_dir.is_dir()
