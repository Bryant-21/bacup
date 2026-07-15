from pathlib import Path
from types import SimpleNamespace
import threading
import time

import pytest


@pytest.fixture(autouse=True)
def reset_extract_state():
    from creation_lib.ui.settings import indexes_section

    indexes_section._set_extract_state(
        extracting=False,
        extract_thread=None,
        extract_progress=0.0,
        extract_total_archives=0,
        extract_completed_archives=0,
        extract_total_files=0,
        extract_error="",
        extract_log_lines=[],
        extract_output_dir="",
        extract_mode="",
    )
    yield
    indexes_section._set_extract_state(extracting=False, extract_thread=None)


class _FakeSettings:
    def __init__(self) -> None:
        self.extracted_dirs: list[tuple[str, str]] = []

    def set_game_extracted_dir(self, game: str, path: str) -> None:
        self.extracted_dirs.append((game, path))


def _prepare_game_root(tmp_path: Path, archive_names: list[str]) -> tuple[Path, Path]:
    game_root = tmp_path / "Game"
    data_dir = game_root / "Data"
    data_dir.mkdir(parents=True)
    for name in archive_names:
        (data_dir / name).write_bytes(b"archive")
    output_dir = tmp_path / "extracted" / "fo4"
    output_dir.mkdir(parents=True)
    return game_root, output_dir


def test_extraction_only_section_has_no_yaml_or_index_actions(tmp_path):
    from creation_lib.ui.settings import indexes_section

    settings = SimpleNamespace(
        get_game_paths=lambda _game: {
            "root_dir": str(tmp_path),
            "extracted_dir": "",
        }
    )
    ctx = SimpleNamespace(settings=settings)
    imgui = indexes_section.imgui
    imgui.reset_mock()
    imgui.combo.return_value = (False, 0)
    imgui.input_int.return_value = (False, 8)
    imgui.is_item_hovered.return_value = False
    imgui.button.return_value = False

    indexes_section.make_section(extraction_only=True).draw(ctx)

    button_labels = [call.args[0] for call in imgui.button.call_args_list]
    assert button_labels == ["Smart Extract##fo4", "Full Extract##fo4"]
    assert not any("YAML" in label or "Rebuild" in label for label in button_labels)


def test_standard_section_keeps_indexes_and_yaml_drawer():
    from creation_lib.ui.settings import indexes_section

    section = indexes_section.make_section()

    assert section.label == "Indexes"
    assert section.draw is indexes_section._draw


def test_extraction_only_section_saves_only_worker_setting():
    from creation_lib.ui.settings import indexes_section

    indexes_section._state.extract_archive_workers = 6

    assert indexes_section.make_section(extraction_only=True).save() == {
        "extract_archive_workers": 6
    }


def test_direct_extract_runs_without_cli_and_updates_progress(tmp_path, monkeypatch):
    from creation_lib.preprocessor import extraction
    from creation_lib.ui.settings import indexes_section

    game_root, output_dir = _prepare_game_root(
        tmp_path,
        ["Fallout4 - Meshes.ba2", "Fallout4 - Textures.ba2"],
    )
    counts = {
        "Fallout4 - Meshes.ba2": 12,
        "Fallout4 - Textures.ba2": 34,
    }

    def fake_extract_one(archive, _output_dir, archive_format, file_workers=8, progress=None):
        assert archive_format == "ba2"
        assert file_workers == 1
        if progress is not None:
            progress({"completed": counts[archive.name], "total": counts[archive.name]})
        return archive, counts[archive.name], None

    monkeypatch.setattr(extraction, "extract_one", fake_extract_one)
    settings = _FakeSettings()

    indexes_section._reset_extract_run("fo4", output_dir, smart=False, workers=2)
    indexes_section._run_direct_extraction(
        settings,
        "fo4",
        game_root,
        output_dir,
        smart=False,
        archive_workers=2,
    )

    snapshot = indexes_section._extract_snapshot()
    assert snapshot["progress"] == 1.0
    assert snapshot["completed_archives"] == 2
    assert snapshot["total_archives"] == 2
    assert snapshot["total_files"] == 46
    assert settings.extracted_dirs == [("fo4", str(output_dir))]
    assert (output_dir / ".ba2_manifest.json").is_file()
    assert any("Extracting with 2 total worker(s)." in line for line in snapshot["log_lines"])


def test_smart_extract_skips_when_manifest_matches(tmp_path, monkeypatch):
    from creation_lib.preprocessor import extraction
    from creation_lib.ui.settings import indexes_section

    game_root, output_dir = _prepare_game_root(tmp_path, ["Fallout4 - Main.ba2"])

    monkeypatch.setattr(extraction, "load_manifest", lambda _output_dir: {"ok": True})
    monkeypatch.setattr(extraction, "manifest_matches", lambda *args: True)
    monkeypatch.setattr(
        extraction,
        "extract_one",
        lambda *args, **kwargs: (_ for _ in ()).throw(AssertionError("extract_one called")),
    )
    settings = _FakeSettings()

    indexes_section._reset_extract_run("fo4", output_dir, smart=True, workers=4)
    indexes_section._run_direct_extraction(
        settings,
        "fo4",
        game_root,
        output_dir,
        smart=True,
        archive_workers=4,
    )

    snapshot = indexes_section._extract_snapshot()
    assert snapshot["progress"] == 1.0
    assert snapshot["completed_archives"] == 1
    assert snapshot["total_archives"] == 1
    assert "skipped" in snapshot["status"].lower()
    assert settings.extracted_dirs == [("fo4", str(output_dir))]


def test_archive_worker_count_is_clamped():
    from creation_lib.ui.settings import indexes_section

    assert indexes_section._clamp_archive_workers(0) == 1
    assert indexes_section._clamp_archive_workers(99) == 99
    assert indexes_section._clamp_archive_workers("bad") >= 1


def test_direct_extract_finishes_base_phase_before_update_phase(tmp_path, monkeypatch):
    from creation_lib.preprocessor import extraction
    from creation_lib.ui.settings import indexes_section

    game_root, output_dir = _prepare_game_root(
        tmp_path,
        [
            "SeventySix - 02UpdateMain.ba2",
            "SeventySix - Textures01.ba2",
            "SeventySix - 00UpdateMain.ba2",
            "SeventySix - 01UpdateMain.ba2",
            "SeventySix - Textures02.ba2",
        ],
    )
    completed: set[str] = set()
    lock = threading.Lock()

    def fake_extract_one(archive, _output_dir, archive_format, file_workers=8, progress=None):
        assert archive_format == "ba2"
        assert file_workers == 1
        if progress is not None:
            progress({"completed": 1, "total": 1})
        if "Textures" in archive.name:
            time.sleep(0.02)
        with lock:
            if "00Update" in archive.name:
                assert {
                    "SeventySix - Textures01.ba2",
                    "SeventySix - Textures02.ba2",
                } <= completed
            if "01Update" in archive.name:
                assert "SeventySix - 00UpdateMain.ba2" in completed
            if "02Update" in archive.name:
                assert "SeventySix - 01UpdateMain.ba2" in completed
            completed.add(archive.name)
        return archive, 1, None

    monkeypatch.setattr(extraction, "extract_one", fake_extract_one)
    settings = _FakeSettings()

    indexes_section._reset_extract_run("fo76", output_dir, smart=False, workers=4)
    indexes_section._run_direct_extraction(
        settings,
        "fo76",
        game_root,
        output_dir,
        smart=False,
        archive_workers=4,
    )

    assert completed == {
        "SeventySix - Textures01.ba2",
        "SeventySix - Textures02.ba2",
        "SeventySix - 00UpdateMain.ba2",
        "SeventySix - 01UpdateMain.ba2",
        "SeventySix - 02UpdateMain.ba2",
    }
