from types import SimpleNamespace

import pytest

from bacup_ui.setup import (
    _ALPHA_ACCEPTED_KEY,
    _PERSONAL_USE_ACCEPTED_KEY,
    _STORE_OWNERSHIP_ACCEPTED_KEY,
    AppalachiaSetup,
    BacupProjectSetup,
    clear_project_owned_extractions,
    games_needing_extraction,
    get_pending_project_setup,
    project_setup_needed,
)
from bacup_ui.__main__ import _run_bacup_project_setup


def _agreement_settings():
    return {
        _ALPHA_ACCEPTED_KEY: True,
        _PERSONAL_USE_ACCEPTED_KEY: True,
        _STORE_OWNERSHIP_ACCEPTED_KEY: True,
    }


def _settings(fo4=None, fo76=None, fnv=None, fo3=None, skyrimse=None, workspace=None):
    paths = {
        "fo4": fo4 or {},
        "fo76": fo76 or {},
        "fnv": fnv or {},
        "fo3": fo3 or {},
        "skyrimse": skyrimse or {},
    }
    workspaces = {"appalachia": workspace or {}}
    return SimpleNamespace(
        get_game_paths=lambda g: dict(paths.get(g, {})),
        set_game_root_dir=lambda g, p: paths[g].__setitem__("root_dir", p),
        set_game_extracted_dir=lambda g, p: paths[g].__setitem__("extracted_dir", p),
        get_workspace_settings=lambda w: dict(workspaces.get(w, {})),
        set_workspace_settings=lambda w, s: workspaces.setdefault(w, {}).update(s),
        setup_complete=False,
        save=lambda: None,
    )


_PARTIAL_AGREEMENTS = {_ALPHA_ACCEPTED_KEY: True, _PERSONAL_USE_ACCEPTED_KEY: True}


@pytest.mark.parametrize(
    "project, games, workspace, expected_needed, expected_extraction",
    [
        ("appalachia", {}, None, True, []),
        ("appalachia", {"fo4": "", "fo76": "C:/x/fo76"}, "agreed", False, []),
        ("appalachia", {"fo4": "", "fo76": "C:/x/fo76"}, None, True, []),
        ("appalachia", {"fo4": "", "fo76": "C:/x/fo76"}, _PARTIAL_AGREEMENTS, True, []),
        ("appalachia", {"fo4": "", "fo76": "<empty>"}, "agreed", True, ["fo76"]),
        ("appalachia", {"fo4": "", "fo76": "<full>"}, "agreed", False, []),
        ("wasteland", {"fo4": "", "fnv": "", "fo3": ""}, "agreed", True, ["fnv", "fo3"]),
        ("wasteland", {"fo4": "", "fnv": "C:/x/fnv", "fo3": "C:/x/fo3"}, "agreed", False, []),
        ("north", {"fo4": "", "skyrimse": ""}, "agreed", True, ["skyrimse"]),
        ("north", {"fo4": "", "skyrimse": "C:/x/skyrimse"}, "agreed", False, []),
    ],
)
def test_project_setup_needed_and_extraction(
    tmp_path, project, games, workspace, expected_needed, expected_extraction
):
    game_paths = {}
    for game, extracted in games.items():
        if extracted in ("<empty>", "<full>"):
            selected = tmp_path / game
            selected.mkdir()
            if extracted == "<full>":
                (selected / "meshes").mkdir()
            extracted = str(selected)
        game_paths[game] = {"root_dir": f"C:/{game}", "extracted_dir": extracted}
    s = _settings(
        **game_paths,
        workspace=_agreement_settings() if workspace == "agreed" else workspace,
    )

    assert project_setup_needed(s, project) is expected_needed
    assert games_needing_extraction(s, project) == expected_extraction


def _capture_extractor(monkeypatch):
    captured = {}

    class FakeExtractor:
        def __init__(self, games, *, output_root, output_dirs):
            captured["games"] = games
            captured["output_root"] = output_root
            captured["output_dirs"] = output_dirs
            self.results = {}

        def start(self):
            captured["started"] = True

    monkeypatch.setattr("bacup_ui.setup._GameExtractor", FakeExtractor)
    return captured


def test_project_extractor_receives_only_owned_sources(monkeypatch, tmp_path):
    captured = _capture_extractor(monkeypatch)
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        fnv={"root_dir": "C:/FNV"},
        fo3={"root_dir": "C:/FO3"},
    )
    assert BacupProjectSetup(s, "wasteland").start_extraction(output_root=tmp_path) is True
    assert captured == {
        "games": [("fnv", "C:/FNV"), ("fo3", "C:/FO3")],
        "output_root": tmp_path,
        "output_dirs": {},
        "started": True,
    }

    selected = tmp_path / "selected"
    selected.mkdir()
    captured = _capture_extractor(monkeypatch)
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        fo76={"root_dir": "C:/FO76", "extracted_dir": str(selected)},
    )
    assert AppalachiaSetup(s).start_extraction(output_root=tmp_path / "default") is True
    assert captured["games"] == [("fo76", "C:/FO76")]
    assert captured["output_dirs"] == {"fo76": selected}


def test_extract_footer_opens_converter_and_applies_results(monkeypatch):
    s = _settings()
    runner_params = SimpleNamespace(app_shall_exit=False)
    monkeypatch.setattr(
        "bacup_ui.setup.hello_imgui.get_runner_params",
        lambda: runner_params,
    )
    setup = AppalachiaSetup(s)
    setup.step = AppalachiaSetup.STEP_EXTRACT
    setup._extractor = SimpleNamespace(
        done=True,
        error=None,
        results={"fo4": "X:/app/extracted/fo4", "fo76": "X:/app/extracted/fo76"},
    )

    setup._run_footer_primary_action()

    assert setup._completed is True
    assert s.setup_complete is True
    assert runner_params.app_shall_exit is True
    assert not s.get_game_paths("fo4").get("extracted_dir")
    assert s.get_game_paths("fo76")["extracted_dir"] == "X:/app/extracted/fo76"
    workspace = s.get_workspace_settings("appalachia")
    assert workspace["app_owned_extracted_games"] == ["fo76"]
    assert workspace["app_owned_extracted_paths"] == {"fo76": "X:/app/extracted/fo76"}


def test_clear_owned_extractions_is_project_scoped(tmp_path):
    extraction_root = tmp_path / "extracted"
    fnv = extraction_root / "fnv"
    fo3 = extraction_root / "fo3"
    fo76 = extraction_root / "fo76"
    skyrim = extraction_root / "skyrimse"
    for path in (fnv, fo3, fo76, skyrim):
        path.mkdir(parents=True)
        (path / "kept-or-cleared.txt").write_text(path.name, encoding="utf-8")
    s = _settings(
        fnv={"root_dir": "C:/FNV", "extracted_dir": str(fnv)},
        fo3={"root_dir": "C:/FO3", "extracted_dir": str(fo3)},
        fo76={"root_dir": "C:/FO76", "extracted_dir": str(fo76)},
        skyrimse={"root_dir": "C:/Skyrim", "extracted_dir": str(skyrim)},
        workspace={
            "wasteland_app_owned_extracted_games": ["fnv", "fo3"],
            "wasteland_app_owned_extracted_paths": {
                "fnv": str(fnv),
                "fo3": str(fo3),
            },
            "app_owned_extracted_games": ["fo76"],
            "app_owned_extracted_paths": {"fo76": str(fo76)},
            "north_app_owned_extracted_games": ["skyrimse"],
            "north_app_owned_extracted_paths": {"skyrimse": str(skyrim)},
        },
    )

    assert clear_project_owned_extractions(
        s, "wasteland", output_root=extraction_root
    ) == ("fnv", "fo3")
    assert not fnv.exists()
    assert not fo3.exists()
    assert fo76.exists()
    assert skyrim.exists()
    assert s.get_game_paths("fnv")["extracted_dir"] == ""
    assert s.get_game_paths("fo3")["extracted_dir"] == ""
    assert s.get_game_paths("fo76")["extracted_dir"] == str(fo76)
    assert s.get_game_paths("skyrimse")["extracted_dir"] == str(skyrim)


@pytest.mark.parametrize(
    "workspace, picked, expected",
    [
        (None, "north", ((True, True), ["north"])),
        (None, None, ((True, False), [])),
        ({"pending_project_setup": "wasteland"}, "north", ((True, True), ["wasteland"])),
    ],
)
def test_project_setup_routes_to_selected_project(monkeypatch, workspace, picked, expected):
    calls = []
    s = _settings(workspace=workspace)

    class FakePicker:
        def __init__(self, _settings):
            pass

        def run(self):
            return picked

    class FakeSetup:
        def __init__(self, _settings, project_id):
            calls.append(project_id)

        def run(self):
            return True

    monkeypatch.setattr("bacup_ui.setup.BacupProjectPicker", FakePicker)
    monkeypatch.setattr("bacup_ui.setup.BacupProjectSetup", FakeSetup)

    assert (_run_bacup_project_setup(s), calls) == expected
    assert get_pending_project_setup(s) is None
