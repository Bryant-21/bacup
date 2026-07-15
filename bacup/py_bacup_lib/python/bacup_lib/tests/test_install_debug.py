from __future__ import annotations

from pathlib import Path

from bacup_lib.install_debug import audit_archive_ini, repair_archive_ini

_MOD_NAME = "SeventySix"
_PLUGIN_NAME = "SeventySix.esm"
_MAIN_BA2 = "SeventySix - Main.ba2"
_TEXTURES_BA2 = "SeventySix - Textures.ba2"


def _seed_deploy_dir(tmp_path: Path) -> Path:
    deploy_dir = tmp_path / "Data"
    deploy_dir.mkdir()
    (deploy_dir / _PLUGIN_NAME).write_bytes(b"")
    (deploy_dir / _MAIN_BA2).write_bytes(b"main")
    (deploy_dir / _TEXTURES_BA2).write_bytes(b"textures")
    return deploy_dir


def _seed_ini(tmp_path: Path) -> Path:
    # Only the Textures shard is registered (under its DX10 key); Main is not.
    ini_path = tmp_path / "Fallout4Custom.ini"
    ini_path.write_text(
        f"[Archive]\nsResourceIndexFileList={_TEXTURES_BA2}\n",
        encoding="utf-8",
    )
    return ini_path


def test_audit_reports_deployed_and_registered_state(tmp_path: Path) -> None:
    deploy_dir = _seed_deploy_dir(tmp_path)
    ini_path = _seed_ini(tmp_path)

    report = audit_archive_ini(
        deploy_dir=deploy_dir,
        ini_path=ini_path,
        mod_name=_MOD_NAME,
        plugin_name=_PLUGIN_NAME,
    )

    assert report.note is None
    rows_by_name = {row.name: row for row in report.rows}

    esm_row = rows_by_name[_PLUGIN_NAME]
    assert esm_row.kind == "esm"
    assert esm_row.deployed is True
    assert esm_row.registered is None

    textures_row = rows_by_name[_TEXTURES_BA2]
    assert textures_row.kind == "ba2"
    assert textures_row.deployed is True
    assert textures_row.registered is True

    main_row = rows_by_name[_MAIN_BA2]
    assert main_row.deployed is True
    assert main_row.registered is False

    assert report.missing_registration == [_MAIN_BA2]


def test_repair_registers_missing_names_and_reaudit_is_clean(tmp_path: Path) -> None:
    deploy_dir = _seed_deploy_dir(tmp_path)
    ini_path = _seed_ini(tmp_path)
    base_ini_path = tmp_path / "Fallout4.ini"  # deliberately absent; seeding is optional

    report = audit_archive_ini(
        deploy_dir=deploy_dir,
        ini_path=ini_path,
        mod_name=_MOD_NAME,
        plugin_name=_PLUGIN_NAME,
    )
    assert report.missing_registration == [_MAIN_BA2]

    added = repair_archive_ini(
        ini_path=ini_path,
        base_ini_path=base_ini_path,
        archive_names=report.missing_registration,
    )
    assert added == [_MAIN_BA2]

    repaired_report = audit_archive_ini(
        deploy_dir=deploy_dir,
        ini_path=ini_path,
        mod_name=_MOD_NAME,
        plugin_name=_PLUGIN_NAME,
    )
    assert repaired_report.missing_registration == []
    rows_by_name = {row.name: row for row in repaired_report.rows}
    assert rows_by_name[_MAIN_BA2].registered is True
    assert rows_by_name[_TEXTURES_BA2].registered is True


def test_audit_with_no_ini_path_reports_none_and_note(tmp_path: Path) -> None:
    deploy_dir = _seed_deploy_dir(tmp_path)

    report = audit_archive_ini(
        deploy_dir=deploy_dir,
        ini_path=None,
        mod_name=_MOD_NAME,
        plugin_name=_PLUGIN_NAME,
    )

    assert report.note == "No INI target for this install location."
    ba2_rows = [row for row in report.rows if row.kind == "ba2"]
    assert {row.name for row in ba2_rows} == {_MAIN_BA2, _TEXTURES_BA2}
    assert all(row.registered is None for row in ba2_rows)
    assert report.missing_registration == []


def test_audit_with_missing_ini_file_reports_none_and_note(tmp_path: Path) -> None:
    deploy_dir = _seed_deploy_dir(tmp_path)
    missing_ini_path = tmp_path / "does_not_exist.ini"

    report = audit_archive_ini(
        deploy_dir=deploy_dir,
        ini_path=missing_ini_path,
        mod_name=_MOD_NAME,
        plugin_name=_PLUGIN_NAME,
    )

    assert report.note == f"INI not found: {missing_ini_path}"
    ba2_rows = [row for row in report.rows if row.kind == "ba2"]
    assert all(row.registered is None for row in ba2_rows)
    assert report.missing_registration == []
