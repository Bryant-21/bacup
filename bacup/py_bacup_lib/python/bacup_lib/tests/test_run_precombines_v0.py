"""Unit tests for bacup/scripts/run_precombines_v0.py.

Every test injects a fake `run_factory`, so `_default_run_factory` (the only
place the script imports `bacup_lib`) is never invoked, and these tests never
require the native extension to be built.
"""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

import pytest


def _load_runner_module():
    path = Path(__file__).resolve().parents[4] / "scripts" / "run_precombines_v0.py"
    spec = importlib.util.spec_from_file_location("run_precombines_v0", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules["run_precombines_v0"] = module
    spec.loader.exec_module(module)
    return module


@pytest.fixture(autouse=True)
def _no_ambient_fo4_dir(monkeypatch: pytest.MonkeyPatch) -> None:
    """`main()`'s FO4_DIR/FO4_EXTRACTED_DIR/.env fallback must not read the real
    dev machine's config in these tests; tests exercising that fallback set it
    up explicitly."""
    monkeypatch.delenv("FO4_DIR", raising=False)
    monkeypatch.delenv("FO4_EXTRACTED_DIR", raising=False)


class FakeRun:
    def __init__(
        self,
        target_plugin_path: str,
        phase_report: dict[str, Any],
        events: list[dict[str, Any]] | None = None,
    ) -> None:
        self.target_plugin_path = target_plugin_path
        self.phase_report = phase_report
        self._pending_events: list[dict[str, Any]] = list(events or [])
        self.phase_calls: list[dict[str, Any]] = []
        self.save_target_calls: list[tuple[str | None, bool]] = []
        self.drain_events_calls: list[int] = []
        self.closed = False

    def run_phase(self, name, *, mod_path, target_data_dir=None, params=None):
        self.phase_calls.append(
            {
                "name": name,
                "mod_path": mod_path,
                "target_data_dir": target_data_dir,
                "params": params,
            }
        )
        return self.phase_report

    def drain_events(self, max: int = 256) -> list[dict[str, Any]]:
        # Mirrors the real ConversionRun.drain_events: non-blocking, returns
        # [] once the queue is empty (callers loop until they see that).
        self.drain_events_calls.append(max)
        batch, self._pending_events = self._pending_events[:max], self._pending_events[max:]
        return batch

    def save_target(self, output_path=None, *, run_nvnm_validator=True):
        self.save_target_calls.append((output_path, run_nvnm_validator))
        # A real ConversionRun.save_target() writes the temp file; the runner
        # then os.replace()s it onto the ESM path, so the fake must too.
        Path(output_path).write_bytes(b"REGENERATED-ESM")

    def __enter__(self):
        return self

    def __exit__(self, *exc_info):
        self.closed = True
        return False


class FakeRunFactory:
    def __init__(
        self, phase_report: dict[str, Any], events: list[dict[str, Any]] | None = None
    ) -> None:
        self.phase_report = phase_report
        self.events = events
        self.calls: list[tuple[str, str, str]] = []
        self.runs: list[FakeRun] = []

    def __call__(self, source_game, target_game, target_plugin_path):
        self.calls.append((source_game, target_game, target_plugin_path))
        run = FakeRun(target_plugin_path, self.phase_report, self.events)
        self.runs.append(run)
        return run


def _write_esm(mod_root: Path, esm_name: str = "SeventySix.esm") -> Path:
    esm_path = mod_root / esm_name
    esm_path.parent.mkdir(parents=True, exist_ok=True)
    esm_path.write_bytes(b"ORIGINAL-ESM")
    return esm_path


def test_run_precombines_opens_the_esm_path(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    esm_path = _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 0})

    module.run_precombines(mod_root, "0062781C", 1, True, run_factory=factory)

    assert factory.calls == [("fo76", "fo4", str(esm_path))]


def test_run_precombines_dispatches_phase_with_exactly_five_param_keys(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 0})

    module.run_precombines(mod_root, "0062781C", 3, False, run_factory=factory)

    [call] = factory.runs[0].phase_calls
    assert call["name"] == "generate_precombines"
    assert call["mod_path"] == str(mod_root)
    assert call["target_data_dir"] == str(mod_root / "data")
    assert call["params"] == {
        "include_cells": ["0062781C"],
        "min_eligible_refs": 3,
        "no_previs": False,
        "mesh_extract_roots": [],
        "mesh_archives": [],
    }
    # Exactly these five keys — no legacy/handle keys of any kind.
    assert set(call["params"].keys()) == {
        "include_cells",
        "min_eligible_refs",
        "no_previs",
        "mesh_extract_roots",
        "mesh_archives",
    }
    for stale_key in ("output_handle_id", "own_index", "vc_stamp", "target_handle_id"):
        assert stale_key not in call["params"]


def test_run_precombines_forwards_mesh_archives_in_order(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 0})

    module.run_precombines(
        mod_root,
        "0062781C",
        1,
        True,
        mesh_archives=["C:/game/Fallout4 - Meshes.ba2", "C:/game/DLCCoast - Main.ba2"],
        run_factory=factory,
    )

    [call] = factory.runs[0].phase_calls
    assert call["params"]["mesh_archives"] == [
        "C:/game/Fallout4 - Meshes.ba2",
        "C:/game/DLCCoast - Main.ba2",
    ]


def test_run_precombines_forwards_mesh_extract_roots(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 0})

    module.run_precombines(
        mod_root,
        "0062781C",
        1,
        True,
        mesh_extract_roots=["C:/extracted/fo4"],
        mesh_archives=["C:/game/Fallout4 - Meshes.ba2"],
        run_factory=factory,
    )

    [call] = factory.runs[0].phase_calls
    assert call["params"]["mesh_extract_roots"] == ["C:/extracted/fo4"]
    assert call["params"]["mesh_archives"] == ["C:/game/Fallout4 - Meshes.ba2"]


def test_zero_asset_report_leaves_esm_untouched(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    esm_path = _write_esm(mod_root)
    original_bytes = esm_path.read_bytes()
    factory = FakeRunFactory({"assets_written": 0, "records_changed": 0})

    report = module.run_precombines(mod_root, "0062781C", 1, True, run_factory=factory)

    assert report == {"assets_written": 0, "records_changed": 0, "warning_messages": []}
    assert esm_path.read_bytes() == original_bytes
    assert factory.runs[0].save_target_calls == []
    assert list(mod_root.glob("*.tmp")) == []


def test_nonzero_asset_report_replaces_esm_atomically(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    esm_path = _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 2, "records_changed": 3})

    report = module.run_precombines(mod_root, "0062781C", 1, True, run_factory=factory)

    assert report == {"assets_written": 2, "records_changed": 3, "warning_messages": []}
    assert esm_path.read_bytes() == b"REGENERATED-ESM"
    [(_output_path, run_nvnm_validator)] = factory.runs[0].save_target_calls
    assert run_nvnm_validator is False
    assert list(mod_root.glob("*.tmp")) == []


def test_report_collects_warn_level_log_events_only(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    events = [
        {"kind": "log", "phase": "generate_precombines", "level": "WARN", "message": "warn one"},
        {"kind": "log", "phase": "generate_precombines", "level": "INFO", "message": "info, ignored"},
        {"kind": "log", "phase": "generate_precombines", "level": "WARN", "message": "warn two"},
    ]
    factory = FakeRunFactory({"assets_written": 0, "records_changed": 0}, events=events)

    report = module.run_precombines(mod_root, "0062781C", 1, True, run_factory=factory)

    assert report["warning_messages"] == ["warn one", "warn two"]


def test_backup_is_created_once(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    esm_path = _write_esm(mod_root)
    backup_path = esm_path.with_name(esm_path.name + module.BACKUP_SUFFIX)
    factory = FakeRunFactory({"assets_written": 0})

    module.run_precombines(mod_root, "0062781C", 1, True, run_factory=factory)
    assert backup_path.is_file()
    assert backup_path.read_bytes() == b"ORIGINAL-ESM"

    # Mutate the on-disk ESM to prove a second run does not re-backup it.
    esm_path.write_bytes(b"MUTATED-ESM")
    module.run_precombines(mod_root, "0062781C", 1, True, run_factory=factory)
    assert backup_path.read_bytes() == b"ORIGINAL-ESM"


def test_missing_esm_raises_before_any_run_is_opened(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    mod_root.mkdir()
    factory = FakeRunFactory({"assets_written": 0})

    with pytest.raises(FileNotFoundError):
        module.run_precombines(mod_root, "0062781C", 1, True, run_factory=factory)
    assert factory.calls == []


def test_main_parses_defaults_and_forwards_to_run_precombines(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    module = _load_runner_module()
    # No .env in this fake repo root: the FO4_DIR fallback deterministically
    # resolves to None regardless of the real dev machine's config.
    monkeypatch.setattr(module, "REPO_ROOT", tmp_path / "fake_repo_root")
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    # assets_written > 0 keeps this test's assertions about param defaults
    # decoupled from exit-code semantics, which are covered separately by
    # test_main_returns_exit_code_{0,2}_on_*_assets.
    factory = FakeRunFactory({"assets_written": 1})

    rc = module.main(["--mod-root", str(mod_root)], run_factory=factory)

    assert rc == 0
    [call] = factory.runs[0].phase_calls
    assert call["params"] == {
        "include_cells": ["0062781C"],
        "min_eligible_refs": 1,
        "no_previs": True,
        "mesh_extract_roots": [],
        "mesh_archives": [],
    }


def test_main_no_previs_clear_maps_to_false(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 0})

    module.main(["--mod-root", str(mod_root), "--no-previs", "clear"], run_factory=factory)

    [call] = factory.runs[0].phase_calls
    assert call["params"]["no_previs"] is False


def test_main_returns_exit_code_2_on_zero_assets(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    esm_path = _write_esm(mod_root)
    original_bytes = esm_path.read_bytes()
    factory = FakeRunFactory({"assets_written": 0, "records_changed": 0})

    rc = module.main(["--mod-root", str(mod_root), "--cell", "0062781C"], run_factory=factory)

    assert rc == 2
    assert esm_path.read_bytes() == original_bytes
    assert factory.runs[0].save_target_calls == []
    out = capsys.readouterr().out
    assert "no precombines generated for cell 0062781C" in out
    assert str(mod_root / "data") in out


def test_main_returns_exit_code_0_on_nonzero_assets(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 2, "records_changed": 3})

    rc = module.main(["--mod-root", str(mod_root)], run_factory=factory)

    assert rc == 0


def test_main_prints_warning_messages(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    events = [
        {
            "kind": "log",
            "phase": "generate_precombines",
            "level": "WARN",
            "message": "source mesh not found on disk as a loose file: meshes/foo.nif",
        },
        {"kind": "log", "phase": "generate_precombines", "level": "INFO", "message": "should not print"},
    ]
    factory = FakeRunFactory({"assets_written": 0, "records_changed": 0}, events=events)

    module.main(["--mod-root", str(mod_root)], run_factory=factory)

    out = capsys.readouterr().out
    assert "source mesh not found on disk as a loose file: meshes/foo.nif" in out
    assert "should not print" not in out


# ---------------------------------------------------------------------------
# discover_mesh_archives / _read_env_path
# ---------------------------------------------------------------------------


def _write_ba2(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"BA2")


def test_discover_mesh_archives_orders_base_then_sorted_dlc(tmp_path: Path) -> None:
    module = _load_runner_module()
    game_dir = tmp_path / "Fallout 4"
    _write_ba2(game_dir / "Data" / "Fallout4 - Meshes.ba2")
    # Written out of order to prove the result is sorted, not insertion order.
    _write_ba2(game_dir / "Data" / "DLCRobot - Main.ba2")
    _write_ba2(game_dir / "Data" / "DLCCoast - Main.ba2")
    _write_ba2(game_dir / "Data" / "DLCNukaWorld - Main.ba2")

    archives = module.discover_mesh_archives(game_dir, [])

    assert [Path(a).name for a in archives] == [
        "Fallout4 - Meshes.ba2",
        "DLCCoast - Main.ba2",
        "DLCNukaWorld - Main.ba2",
        "DLCRobot - Main.ba2",
    ]
    assert all(Path(a).is_absolute() for a in archives)


def test_discover_mesh_archives_skips_missing_files_with_a_warning(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    module = _load_runner_module()
    game_dir = tmp_path / "Fallout 4"
    # Base archive missing entirely; only one DLC present.
    _write_ba2(game_dir / "Data" / "DLCCoast - Main.ba2")

    archives = module.discover_mesh_archives(game_dir, [])

    assert [Path(a).name for a in archives] == ["DLCCoast - Main.ba2"]
    out = capsys.readouterr().out
    assert "warning: mesh archive not found, skipping" in out
    assert "Fallout4 - Meshes.ba2" in out


def test_discover_mesh_archives_appends_extra_archives_after_game_archives(tmp_path: Path) -> None:
    module = _load_runner_module()
    game_dir = tmp_path / "Fallout 4"
    _write_ba2(game_dir / "Data" / "Fallout4 - Meshes.ba2")
    extra = tmp_path / "MyMod" / "Extra - Main.ba2"
    _write_ba2(extra)

    archives = module.discover_mesh_archives(game_dir, [str(extra)])

    assert [Path(a).name for a in archives] == ["Fallout4 - Meshes.ba2", "Extra - Main.ba2"]


def test_discover_mesh_archives_extras_do_not_require_game_dir(tmp_path: Path) -> None:
    module = _load_runner_module()
    extra = tmp_path / "MyMod" / "Extra - Main.ba2"
    _write_ba2(extra)

    archives = module.discover_mesh_archives(None, [str(extra)])

    assert archives == [str(extra.resolve())]


def test_discover_mesh_archives_empty_when_no_game_dir_and_no_extras(
    capsys: pytest.CaptureFixture[str],
) -> None:
    module = _load_runner_module()

    archives = module.discover_mesh_archives(None, [])

    assert archives == []
    out = capsys.readouterr().out
    assert "no --game-dir given and FO4_DIR is not set" in out


# ---------------------------------------------------------------------------
# discover_mesh_extract_roots
# ---------------------------------------------------------------------------


def test_discover_mesh_extract_roots_returns_root_when_it_exists(tmp_path: Path) -> None:
    module = _load_runner_module()
    extract_dir = tmp_path / "extracted" / "fo4"
    (extract_dir / "meshes").mkdir(parents=True)

    roots = module.discover_mesh_extract_roots(extract_dir)

    assert roots == [str(extract_dir.resolve())]


def test_discover_mesh_extract_roots_skips_missing_dir_with_a_warning(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    module = _load_runner_module()
    extract_dir = tmp_path / "extracted" / "fo4"  # never created

    roots = module.discover_mesh_extract_roots(extract_dir)

    assert roots == []
    out = capsys.readouterr().out
    assert "warning: extract dir not found, skipping" in out
    assert str(extract_dir) in out


def test_discover_mesh_extract_roots_empty_when_none(capsys: pytest.CaptureFixture[str]) -> None:
    module = _load_runner_module()

    roots = module.discover_mesh_extract_roots(None)

    assert roots == []
    assert capsys.readouterr().out == ""


def test_read_env_path_prefers_os_environ_over_dotenv(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    module = _load_runner_module()
    monkeypatch.setattr(module, "REPO_ROOT", tmp_path)
    (tmp_path / ".env").write_text('FO4_DIR="C:/from/dotenv"\n', encoding="utf-8")
    monkeypatch.setenv("FO4_DIR", "C:/from/os/environ")

    assert module._read_env_path("FO4_DIR") == Path("C:/from/os/environ")


def test_read_env_path_falls_back_to_quoted_dotenv_value(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    module = _load_runner_module()
    monkeypatch.setattr(module, "REPO_ROOT", tmp_path)
    (tmp_path / ".env").write_text(
        '# comment\nOTHER_VAR="nope"\nFO4_DIR="N:\\Steam Games\\Fallout 4"\n', encoding="utf-8"
    )

    assert module._read_env_path("FO4_DIR") == Path(r"N:\Steam Games\Fallout 4")


def test_read_env_path_returns_none_when_unset_and_no_dotenv(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    module = _load_runner_module()
    monkeypatch.setattr(module, "REPO_ROOT", tmp_path / "no_dotenv_here")

    assert module._read_env_path("FO4_DIR") is None


def test_main_wires_game_dir_flag_into_params(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    game_dir = tmp_path / "Fallout 4"
    _write_ba2(game_dir / "Data" / "Fallout4 - Meshes.ba2")
    factory = FakeRunFactory({"assets_written": 0})

    module.main(["--mod-root", str(mod_root), "--game-dir", str(game_dir)], run_factory=factory)

    [call] = factory.runs[0].phase_calls
    assert call["params"]["mesh_archives"] == [str((game_dir / "Data" / "Fallout4 - Meshes.ba2").resolve())]


def test_main_wires_repeated_archive_flag_into_params(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    extra_a = tmp_path / "a.ba2"
    extra_b = tmp_path / "b.ba2"
    _write_ba2(extra_a)
    _write_ba2(extra_b)
    factory = FakeRunFactory({"assets_written": 0})

    module.main(
        [
            "--mod-root", str(mod_root),
            "--archive", str(extra_a),
            "--archive", str(extra_b),
        ],
        run_factory=factory,
    )

    [call] = factory.runs[0].phase_calls
    assert call["params"]["mesh_archives"] == [str(extra_a.resolve()), str(extra_b.resolve())]


def test_main_wires_extract_dir_flag_into_params(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    extract_dir = tmp_path / "extracted" / "fo4"
    (extract_dir / "meshes").mkdir(parents=True)
    factory = FakeRunFactory({"assets_written": 0})

    module.main(["--mod-root", str(mod_root), "--extract-dir", str(extract_dir)], run_factory=factory)

    [call] = factory.runs[0].phase_calls
    assert call["params"]["mesh_extract_roots"] == [str(extract_dir.resolve())]
    # No --game-dir/FO4_DIR: archives stay empty, extract-dir mode is primary.
    assert call["params"]["mesh_archives"] == []


def test_main_extract_dir_and_game_dir_both_present_coexist_correctly_ordered(tmp_path: Path) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    extract_dir = tmp_path / "extracted" / "fo4"
    (extract_dir / "meshes").mkdir(parents=True)
    game_dir = tmp_path / "Fallout 4"
    _write_ba2(game_dir / "Data" / "Fallout4 - Meshes.ba2")
    factory = FakeRunFactory({"assets_written": 0})

    module.main(
        [
            "--mod-root", str(mod_root),
            "--extract-dir", str(extract_dir),
            "--game-dir", str(game_dir),
        ],
        run_factory=factory,
    )

    [call] = factory.runs[0].phase_calls
    assert call["params"]["mesh_extract_roots"] == [str(extract_dir.resolve())]
    assert call["params"]["mesh_archives"] == [
        str((game_dir / "Data" / "Fallout4 - Meshes.ba2").resolve())
    ]


def test_main_falls_back_to_fo4_extracted_dir_env(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    module = _load_runner_module()
    monkeypatch.setattr(module, "REPO_ROOT", tmp_path / "fake_repo_root")
    extract_dir = tmp_path / "extracted" / "fo4"
    (extract_dir / "meshes").mkdir(parents=True)
    monkeypatch.setenv("FO4_EXTRACTED_DIR", str(extract_dir))
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    factory = FakeRunFactory({"assets_written": 0})

    module.main(["--mod-root", str(mod_root)], run_factory=factory)

    [call] = factory.runs[0].phase_calls
    assert call["params"]["mesh_extract_roots"] == [str(extract_dir.resolve())]


@pytest.mark.parametrize(
    "with_extract_dir,with_game_dir,expected_fragment",
    [
        (True, True, "extract-dir (primary) + archives (fallback)"),
        (True, False, "mesh source mode: extract-dir"),
        (False, True, "mesh source mode: archives"),
        (False, False, "mesh source mode: none"),
    ],
)
def test_main_prints_mesh_source_mode(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    with_extract_dir: bool,
    with_game_dir: bool,
    expected_fragment: str,
) -> None:
    module = _load_runner_module()
    mod_root = tmp_path / "SeventySix"
    _write_esm(mod_root)
    args = ["--mod-root", str(mod_root)]
    if with_extract_dir:
        extract_dir = tmp_path / "extracted" / "fo4"
        (extract_dir / "meshes").mkdir(parents=True)
        args += ["--extract-dir", str(extract_dir)]
    if with_game_dir:
        game_dir = tmp_path / "Fallout 4"
        _write_ba2(game_dir / "Data" / "Fallout4 - Meshes.ba2")
        args += ["--game-dir", str(game_dir)]
    factory = FakeRunFactory({"assets_written": 0})

    module.main(args, run_factory=factory)

    out = capsys.readouterr().out
    assert expected_fragment in out
