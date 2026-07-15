import re

from bacup_lib.terrain.xse_shell import XseShellRequest, write_xse_shell


def test_write_xse_shell_creates_worldspace_and_four_cells(tmp_path):
    mod_dir = tmp_path / "B21_AppalachiaXSE"
    request = XseShellRequest(
        mod_dir=mod_dir,
        plugin_name="B21_AppalachiaXSE.esp",
        worldspace_editor_id="B21_AppalachiaXSEWorld",
        cell_min_x=0,
        cell_min_y=0,
        cell_max_x=1,
        cell_max_y=1,
    )

    result = write_xse_shell(request)

    assert result.plugin_yaml == mod_dir / "yaml" / "plugin.yaml"
    assert result.cell_count == 4
    assert (mod_dir / ".game").read_text(encoding="utf-8") == "fo4\n"
    assert "plugin: B21_AppalachiaXSE.esp" in result.plugin_yaml.read_text(
        encoding="utf-8"
    )

    record_files = sorted((mod_dir / "yaml" / "records").rglob("RecordData.yaml"))
    assert len(record_files) == 5
    records_text = "\n".join(path.read_text(encoding="utf-8") for path in record_files)
    assert "signature: WRLD" in records_text
    assert records_text.count("signature: CELL") == 4
    assert "B21_AppalachiaXSEWorld" in records_text
    assert "B21_AppalachiaXSEWorldCellXP000YP000" in records_text
    assert "Landscape" not in records_text
    assert "signature: LAND" not in records_text


def test_write_xse_shell_negative_cell_editor_ids_survive_ck_sanitizing(tmp_path):
    mod_dir = tmp_path / "B21_AppalachiaXSE"
    request = XseShellRequest(
        mod_dir=mod_dir,
        plugin_name="B21_AppalachiaXSE.esp",
        worldspace_editor_id="B21_AppalachiaXSEWorld",
        cell_min_x=-2,
        cell_min_y=-2,
        cell_max_x=2,
        cell_max_y=2,
    )

    write_xse_shell(request)

    record_files = sorted((mod_dir / "yaml" / "records").rglob("RecordData.yaml"))
    records_text = "\n".join(path.read_text(encoding="utf-8") for path in record_files)
    eids = re.findall(r"eid: (B21_AppalachiaXSEWorldCellX[^\n]+)", records_text)
    sanitized = [re.sub(r"[^A-Za-z0-9]", "", eid) for eid in eids]

    assert len(eids) == 25
    assert len(set(sanitized)) == 25
    assert "B21_AppalachiaXSEWorldCellXN002YN002" in eids
    assert "B21_AppalachiaXSEWorldCellXP002YP002" in eids
