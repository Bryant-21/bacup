from pathlib import Path

from ui.toolkit.steam_install import SteamInstallResult, validate_steam_install_for_game


def _steam_fo4_root(tmp_path: Path, *, with_steam_api: bool = True) -> Path:
    root = tmp_path / "SteamLibrary" / "steamapps" / "common" / "Fallout 4"
    data = root / "Data"
    data.mkdir(parents=True)
    (root / "Fallout4.exe").write_bytes(b"exe")
    (data / "Fallout4 - Main.ba2").write_bytes(b"ba2")
    if with_steam_api:
        (root / "steam_api64.dll").write_bytes(b"dll")
    return root


def _steam_fo76_root(tmp_path: Path, *, with_steam_api: bool = True) -> Path:
    root = tmp_path / "SteamLibrary" / "steamapps" / "common" / "Fallout76"
    data = root / "Data"
    data.mkdir(parents=True)
    (root / "Fallout76.exe").write_bytes(b"exe")
    (data / "SeventySix - Startup.ba2").write_bytes(b"ba2")
    if with_steam_api:
        (root / "steam_api64.dll").write_bytes(b"dll")
    return root


def _steam_fnv_root(tmp_path: Path, *, with_steam_api: bool = True) -> Path:
    root = tmp_path / "SteamLibrary" / "steamapps" / "common" / "Fallout New Vegas"
    data = root / "Data"
    data.mkdir(parents=True)
    (root / "FalloutNV.exe").write_bytes(b"exe")
    (data / "Fallout - Meshes.bsa").write_bytes(b"bsa")
    if with_steam_api:
        (root / "steam_api.dll").write_bytes(b"dll")
    return root


def _steam_fo3_root(tmp_path: Path) -> Path:
    root = tmp_path / "SteamLibrary" / "steamapps" / "common" / "Fallout 3 GOTY"
    data = root / "Data"
    data.mkdir(parents=True)
    (root / "Fallout3.exe").write_bytes(b"exe")
    (root / "steam_api.dll").write_bytes(b"dll")
    (data / "Fallout - Meshes.bsa").write_bytes(b"bsa")
    return root


def _write_manifest(root: Path, *, appid: int = 377160, installdir: str = "Fallout 4") -> Path:
    manifest = root.parents[1] / f"appmanifest_{appid}.acf"
    manifest.write_text(
        "\n".join(
            (
                '"AppState"',
                "{",
                f'    "appid"        "{appid}"',
                f'    "installdir"   "{installdir}"',
                "}",
            )
        ),
        encoding="utf-8",
    )
    return manifest


def test_validate_steam_install_accepts_matching_steam_library_install(tmp_path):
    root = _steam_fo4_root(tmp_path)
    manifest = _write_manifest(root)

    result = validate_steam_install_for_game("fo4", str(root))

    assert result == SteamInstallResult(
        ok=True,
        game_id="fo4",
        app_id=377160,
        root_dir=str(root),
        local_install_valid=True,
        steam_layout_valid=True,
        steam_api_present=True,
        appmanifest_present=True,
        appmanifest_matches=True,
        steam_library_dir=str(tmp_path / "SteamLibrary"),
        appmanifest_path=str(manifest),
        message="Fallout 4 Steam install verified.",
    )


def test_validate_steam_install_accepts_fo76_matching_steam_library_install(tmp_path):
    root = _steam_fo76_root(tmp_path)
    manifest = _write_manifest(root, appid=1151340, installdir="Fallout76")

    result = validate_steam_install_for_game("fo76", str(root))

    assert result == SteamInstallResult(
        ok=True,
        game_id="fo76",
        app_id=1151340,
        root_dir=str(root),
        local_install_valid=True,
        steam_layout_valid=True,
        steam_api_present=True,
        appmanifest_present=True,
        appmanifest_matches=True,
        steam_library_dir=str(tmp_path / "SteamLibrary"),
        appmanifest_path=str(manifest),
        message="Fallout 76 Steam install verified.",
    )


def test_validate_steam_install_accepts_fnv_32_bit_steam_api(tmp_path):
    root = _steam_fnv_root(tmp_path)
    manifest = _write_manifest(
        root,
        appid=22380,
        installdir="Fallout New Vegas",
    )

    result = validate_steam_install_for_game("fnv", str(root))

    assert result.ok is True
    assert result.steam_api_present is True
    assert result.appmanifest_path == str(manifest)
    assert result.message == "Fallout: New Vegas Steam install verified."


def test_validate_steam_install_accepts_fo3_32_bit_steam_api(tmp_path):
    root = _steam_fo3_root(tmp_path)
    _write_manifest(root, appid=22370, installdir="Fallout 3 GOTY")

    result = validate_steam_install_for_game("fo3", str(root))

    assert result.ok is True
    assert result.steam_api_present is True


def test_validate_steam_install_accepts_data_dir_input(tmp_path):
    root = _steam_fo4_root(tmp_path)
    _write_manifest(root)

    result = validate_steam_install_for_game("fo4", str(root / "Data"))

    assert result.ok is True
    assert result.root_dir == str(root)


def test_validate_steam_install_rejects_non_steam_layout(tmp_path):
    root = tmp_path / "Fallout 4"
    data = root / "Data"
    data.mkdir(parents=True)
    (root / "Fallout4.exe").write_bytes(b"exe")
    (root / "steam_api64.dll").write_bytes(b"dll")
    (data / "Fallout4 - Main.ba2").write_bytes(b"ba2")

    result = validate_steam_install_for_game("fo4", str(root))

    assert result.ok is False
    assert result.steam_layout_valid is False
    assert "steamapps\\common" in result.message


def test_validate_steam_install_rejects_missing_steam_api_dll(tmp_path):
    root = _steam_fo4_root(tmp_path, with_steam_api=False)
    _write_manifest(root)

    result = validate_steam_install_for_game("fo4", str(root))

    assert result.ok is False
    assert result.steam_api_present is False
    assert result.message == "Fallout 4 install is missing steam_api64.dll."


def test_validate_steam_install_rejects_missing_appmanifest(tmp_path):
    root = _steam_fo4_root(tmp_path)

    result = validate_steam_install_for_game("fo4", str(root))

    assert result.ok is False
    assert result.appmanifest_present is False
    assert result.message == "Steam app manifest appmanifest_377160.acf was not found."


def test_validate_steam_install_rejects_manifest_for_other_folder(tmp_path):
    root = _steam_fo4_root(tmp_path)
    _write_manifest(root, installdir="Other Folder")

    result = validate_steam_install_for_game("fo4", str(root))

    assert result.ok is False
    assert result.appmanifest_matches is False
    assert result.message == "Steam app manifest does not match the selected Fallout 4 folder."


def test_validate_steam_install_fails_before_steam_checks_when_local_install_invalid(tmp_path):
    result = validate_steam_install_for_game("fo4", str(tmp_path / "Fallout 4"))

    assert result.ok is False
    assert result.local_install_valid is False
    assert "install is invalid" in result.message
