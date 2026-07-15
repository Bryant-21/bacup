import sys, os
from pathlib import Path

import pytest
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from ui.toolkit.db_builder import DbBuilder


def test_defaults_all_true():
    b = DbBuilder(fo4_root="")
    assert b._build_fo4_data is True
    assert b._build_scripts is True
    assert b._build_wiki is True
    assert b._build_nifs is True
    assert b._build_behaviors is True
    assert b._force_rebuild is False


def test_flags_stored():
    b = DbBuilder(
        fo4_root="",
        build_fo4_data=False,
        build_scripts=False,
        build_wiki=True,
        build_nifs=True,
        build_behaviors=False,
    )
    assert b._build_fo4_data is False
    assert b._build_scripts is False
    assert b._build_wiki is True
    assert b._build_nifs is True
    assert b._build_behaviors is False


def test_force_rebuild_stored():
    b = DbBuilder(fo4_root="", force_rebuild=True)
    assert b._force_rebuild is True


def test_progress_ranges_all_enabled():
    b = DbBuilder(fo4_root="")
    ranges = b._compute_progress_ranges()
    assert ranges["fo4_data"] == (0.0, pytest.approx(1/5, abs=0.01))
    assert ranges["scripts"][0] == pytest.approx(1/5, abs=0.01)
    assert ranges["wiki"][0] == pytest.approx(2/5, abs=0.01)
    assert ranges["nifs"][0] == pytest.approx(3/5, abs=0.01)
    assert ranges["behaviors"][1] == pytest.approx(1.0, abs=0.01)


def test_progress_ranges_nifs_only():
    b = DbBuilder(
        fo4_root="",
        build_fo4_data=False,
        build_scripts=False,
        build_wiki=False,
        build_nifs=True,
        build_behaviors=False,
    )
    ranges = b._compute_progress_ranges()
    assert "fo4_data" not in ranges
    assert "scripts" not in ranges
    assert "wiki" not in ranges
    assert "behaviors" not in ranges
    assert ranges["nifs"] == (pytest.approx(0.0), pytest.approx(1.0))


def test_progress_ranges_fo4_and_behaviors():
    b = DbBuilder(
        fo4_root="",
        build_fo4_data=True,
        build_scripts=False,
        build_wiki=False,
        build_nifs=False,
        build_behaviors=True,
    )
    ranges = b._compute_progress_ranges()
    assert "nifs" not in ranges
    assert ranges["fo4_data"][0] == pytest.approx(0.0)
    assert ranges["behaviors"][1] == pytest.approx(1.0)


def test_progress_ranges_scripts_only():
    b = DbBuilder(
        fo4_root="",
        build_fo4_data=False,
        build_scripts=True,
        build_wiki=False,
        build_nifs=False,
        build_behaviors=False,
    )
    ranges = b._compute_progress_ranges()
    assert ranges == {"scripts": (pytest.approx(0.0), pytest.approx(1.0))}


def test_scripts_phase_skips_without_game_path(tmp_path, monkeypatch):
    monkeypatch.setattr("ui.toolkit.db_builder.get_db_dir", lambda: tmp_path)
    b = DbBuilder(
        fo4_root="",
        build_fo4_data=False,
        build_scripts=True,
        build_wiki=False,
        build_nifs=False,
        build_behaviors=False,
    )
    b._build_scripts_phase()

    assert b.status == "No game path set — skipping Papyrus scripts index."


def test_scripts_phase_skips_for_non_papyrus_game(tmp_path, monkeypatch):
    monkeypatch.setattr("ui.toolkit.db_builder.get_db_dir", lambda: tmp_path)
    b = DbBuilder(
        fo4_root="X:/Games/Fallout 3",
        build_fo4_data=False,
        build_scripts=True,
        build_wiki=False,
        build_nifs=False,
        build_behaviors=False,
        game="fo3",
    )
    b._build_scripts_phase()

    assert b.status == "No Papyrus scripts configured for Fallout 3 — skipping."


def test_fnv_wiki_phase_targets_shared_fo3_database(tmp_path, monkeypatch):
    monkeypatch.setattr("ui.toolkit.db_builder.get_db_dir", lambda: tmp_path)
    (tmp_path / "Wiki" / "fo3_nv_wiki").mkdir(parents=True)
    monkeypatch.setattr(
        "ui.toolkit.db_builder.get_app_root",
        lambda: tmp_path,
    )

    seen = {}

    def fake_run_preprocess(**kwargs):
        seen["extra_args"] = kwargs["extra_args"]

    b = DbBuilder(
        fo4_root="X:/Games/Fallout New Vegas",
        build_fo4_data=False,
        build_scripts=False,
        build_wiki=True,
        build_nifs=False,
        build_behaviors=False,
        game="fnv",
        force_rebuild=True,
    )
    monkeypatch.setattr(b, "_run_preprocess", fake_run_preprocess)
    b._build_wiki_phase()

    assert str(tmp_path / "fo3_wiki.db") in seen["extra_args"]
