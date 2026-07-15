from pathlib import Path


def test_resolve_launch_target_routes_bsa_files_to_viewer():
    from ui.toolkit.app import ToolkitApp

    target = ToolkitApp._resolve_launch_target(r"C:\tmp\Fallout4 - Misc.ba2")

    assert target == (
        "bsa_viewer",
        str(Path(r"C:\tmp\Fallout4 - Misc.ba2").resolve(strict=False)),
    )


def test_resolve_launch_target_routes_plugin_files_to_esp_editor():
    from ui.toolkit.app import ToolkitApp

    target = ToolkitApp._resolve_launch_target(r"C:\tmp\MyPatch.esl")

    assert target == (
        "esp_editor",
        str(Path(r"C:\tmp\MyPatch.esl").resolve(strict=False)),
    )


def test_build_file_entries_normalizes_paths():
    from ui.bsa_viewer.workspace import _build_file_entries

    entries = _build_file_entries(
        [
            r"Textures\Actors\Robot\Body.DDS",
            "meshes/weapons/pistol.nif",
        ]
    )

    assert [entry.path for entry in entries] == [
        "meshes/weapons/pistol.nif",
        "textures/actors/robot/body.dds",
    ]
    assert entries[0].folder == "meshes/weapons"
    assert entries[0].name == "pistol.nif"
    assert entries[0].ext == "nif"


def test_bsa_viewer_reads_archives_directly(monkeypatch):
    from ui.bsa_viewer.workspace import BSAViewerWorkspace

    monkeypatch.setattr(
        "ui.bsa_viewer.workspace.native_runtime.list_archive",
        lambda archive_path: ["Meshes/Test.nif"],
    )
    monkeypatch.setattr(
        "ui.bsa_viewer.workspace.native_runtime.archive_info",
        lambda archive_path: {
            "format": "fo4_gnrl",
            "version": 1,
            "file_count": 1,
        },
    )

    data = BSAViewerWorkspace()._read_archive(r"C:\tmp\Fallout4 - Meshes.ba2")

    assert data["archive_name"] == "Fallout4 - Meshes.ba2"
    assert data["backend"] == "native"
    assert data["files"] == ["Meshes/Test.nif"]
