from creation_lib.core.game_profiles import GAME_PROFILES
from ui.toolkit import path_detector as pd


def test_fo76_folder_candidates_include_no_space_variant():
    cands = pd._steam_folder_candidates("fo76", GAME_PROFILES["fo76"])
    assert "Fallout76" in cands  # the real Steam folder name (no space)


def test_detect_fo76_via_vdf_finds_no_space_folder(tmp_path, monkeypatch):
    # Hermetic: only the VDF strategy runs (registry + common-scan stubbed so a
    # real local install cannot satisfy the test for the wrong reason).
    monkeypatch.setattr(pd, "_detect_from_registry", lambda *a, **k: None)
    monkeypatch.setattr(pd, "_detect_from_common_paths", lambda *a, **k: None)

    pf86 = tmp_path / "PF86"
    (pf86 / "Steam" / "config").mkdir(parents=True)
    lib = tmp_path / "Lib"
    fo76 = lib / "steamapps" / "common" / "Fallout76"  # no space
    (fo76 / "Data").mkdir(parents=True)
    (fo76 / "Fallout76.exe").write_bytes(b"x")
    (fo76 / "Data" / "SeventySix - 00UpdateMain.ba2").write_bytes(b"x")

    vdf = pf86 / "Steam" / "config" / "libraryfolders.vdf"
    lib_escaped = str(lib).replace("\\", "\\\\")
    vdf.write_text(
        '"libraryfolders"\n{\n\t"0"\n\t{\n\t\t"path"\t\t"%s"\n\t}\n}\n' % lib_escaped,
        encoding="utf-8",
    )
    monkeypatch.setenv("ProgramFiles(x86)", str(pf86))

    result = pd.detect_game_path("fo76")
    assert result is not None
    assert result.replace("\\", "/").endswith("Fallout76")
