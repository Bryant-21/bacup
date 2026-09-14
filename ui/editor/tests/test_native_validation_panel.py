from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock

from ui.editor.panels.validation import ValidationPanel


def test_current_nif_snapshot_uses_native_read_only_validation(tmp_path, monkeypatch):
    from creation_lib.nif import native_runtime

    source = tmp_path / "current.nif"
    source.write_bytes(b"nif")
    validation_paths = []
    native_snapshot = object()

    class Nif:
        def _to_native(self):
            return native_snapshot

        def save(self, path):
            raise AssertionError("current-file validation must not mutate NifFile.save state")

    nif = Nif()
    session = SimpleNamespace(file_path=str(source), dirty=False)
    app = SimpleNamespace(
        nif_file=nif,
        registry=SimpleNamespace(active_session=session),
    )
    calls = []

    def validate(path, output_path, fix, include_optional):
        validation_path = Path(path)
        validation_paths.append(validation_path)
        assert validation_path.read_bytes() == b"current snapshot"
        calls.append((path, output_path, fix, include_optional))
        return {
            "game": "fo4",
            "findings": [
                {
                    "severity": "warning",
                    "rule": "duplicate-vertices",
                    "block_id": 7,
                    "message": "Duplicate vertices",
                }
            ],
            "warnings": [],
        }

    def save_snapshot(snapshot, path):
        assert snapshot is native_snapshot
        Path(path).write_bytes(b"current snapshot")

    monkeypatch.setattr(native_runtime, "save_nif_raw", save_snapshot)
    monkeypatch.setattr(native_runtime, "validate_nif_file_raw", validate)
    panel = ValidationPanel(app)
    panel.validate()

    assert len(calls) == 1
    assert calls[0][0] != str(source)
    assert calls[0][1:] == (None, False, False)
    assert not validation_paths[0].exists()
    assert source.read_bytes() == b"nif"
    assert panel._validated_game == "fo4"
    assert panel._issues == [
        ("WARNING", 7, "duplicate-vertices: Duplicate vertices")
    ]


def test_dirty_current_nif_is_validated_from_temporary_snapshot(tmp_path, monkeypatch):
    from creation_lib.nif import native_runtime

    source = tmp_path / "current.nif"
    source.write_bytes(b"saved")
    validation_paths = []

    class Nif:
        def save(self, path):
            Path(path).write_bytes(b"dirty snapshot")

    session = SimpleNamespace(file_path=str(source), dirty=True)
    app = SimpleNamespace(
        nif_file=Nif(),
        registry=SimpleNamespace(active_session=session),
    )

    def validate(path, output_path, fix, include_optional):
        validation_path = Path(path)
        validation_paths.append(validation_path)
        assert validation_path != source
        assert validation_path.read_bytes() == b"dirty snapshot"
        assert (output_path, fix, include_optional) == (None, False, False)
        return {"game": "fo4", "findings": [], "warnings": []}

    monkeypatch.setattr(native_runtime, "validate_nif_file_raw", validate)
    ValidationPanel(app).validate()

    assert len(validation_paths) == 1
    assert not validation_paths[0].exists()
    assert source.read_bytes() == b"saved"


def test_toolbar_validate_action_calls_current_native_validation_panel(monkeypatch):
    from ui.editor.panels.toolbar import ToolbarPanel

    validation = MagicMock()
    app = SimpleNamespace(nif_file=MagicMock(), validation=validation)
    toolbar = ToolbarPanel(app)

    monkeypatch.setattr(
        "ui.editor.panels.toolbar.imgui.begin_menu",
        lambda label, *args: label == "Tools",
    )
    monkeypatch.setattr(
        "ui.editor.panels.toolbar.imgui.menu_item",
        lambda label, *args: (label == "Validate NIF", False),
    )
    monkeypatch.setattr("ui.editor.panels.toolbar.imgui.separator", lambda: None)
    monkeypatch.setattr("ui.editor.panels.toolbar.imgui.end_menu", lambda: None)

    toolbar._tools_menu()

    validation.validate.assert_called_once_with()
