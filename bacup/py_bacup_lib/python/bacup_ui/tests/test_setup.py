import threading
from pathlib import Path
from types import SimpleNamespace

from bacup_ui.setup import (
    _ALPHA_ACCEPTED_KEY,
    _CURRENT_RELEASE_ID,
    _CURRENT_RELEASE_LABEL,
    _CURRENT_RELEASE_NOTES,
    _FEATURE_STATUS_FOOTER,
    _FEATURE_STATUS_INTRO,
    _FEATURE_STATUS_NOT_PRIORITY,
    _FEATURE_STATUS_TITLE,
    _FEATURE_STATUS_WORKS_BEST,
    _PERSONAL_USE_ACCEPTED_KEY,
    _STEAM_OWNERSHIP_ACCEPTED_KEY,
    _STEAM_OWNERSHIP_CHECKBOX,
    _STEAM_REQUIREMENT_TEXT,
    AppalachiaSetup,
    BacupProjectPicker,
    BacupProjectSetup,
    PROJECT_PROFILES,
    _current_release_notes,
    _dir_size_bytes,
    _estimated_extract_gb,
    appalachia_setup_needed,
    clear_pending_project_setup,
    clear_project_owned_extractions,
    games_needing_extraction,
    get_active_project,
    get_pending_project_setup,
    get_project_setup_ownership,
    project_setup_needed,
    project_owns_extracted_path,
    request_project_setup,
)
from bacup_ui.__main__ import _run_bacup_project_setup
from bacup_lib.upgrade_manifest import bundled_upgrade_manifest_path, load_upgrade_manifest


def _agreement_settings():
    return {
        _ALPHA_ACCEPTED_KEY: True,
        _PERSONAL_USE_ACCEPTED_KEY: True,
        _STEAM_OWNERSHIP_ACCEPTED_KEY: True,
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


def test_setup_needed_when_paths_missing():
    assert appalachia_setup_needed(_settings()) is True


def test_apply_window_icon_uses_appalachia_variant(monkeypatch):
    import ui.toolkit.app as app

    captured = []
    monkeypatch.setattr(app, "set_window_icon", lambda v=None: captured.append(v))
    AppalachiaSetup(_settings())._apply_window_icon()
    assert captured and captured[0].id == "appalachia"


def test_setup_not_needed_when_both_complete():
    s = _settings(
        fo4={"root_dir": "C:/FO4", "extracted_dir": "C:/x/fo4"},
        fo76={"root_dir": "C:/FO76", "extracted_dir": "C:/x/fo76"},
        workspace=_agreement_settings(),
    )
    assert appalachia_setup_needed(s) is False


def test_appalachia_setup_does_not_require_fo4_extraction():
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        fo76={"root_dir": "C:/FO76", "extracted_dir": "C:/x/fo76"},
        workspace=_agreement_settings(),
    )
    assert appalachia_setup_needed(s) is False


def test_setup_needed_when_agreements_missing_even_if_paths_complete():
    s = _settings(
        fo4={"root_dir": "C:/FO4", "extracted_dir": "C:/x/fo4"},
        fo76={"root_dir": "C:/FO76", "extracted_dir": "C:/x/fo76"},
    )
    assert appalachia_setup_needed(s) is True


def test_setup_needed_when_steam_ownership_agreement_missing():
    s = _settings(
        fo4={"root_dir": "C:/FO4", "extracted_dir": "C:/x/fo4"},
        fo76={"root_dir": "C:/FO76", "extracted_dir": "C:/x/fo76"},
        workspace={
            _ALPHA_ACCEPTED_KEY: True,
            _PERSONAL_USE_ACCEPTED_KEY: True,
        },
    )
    assert appalachia_setup_needed(s) is True


def test_games_needing_extraction_lists_only_missing():
    s = _settings(
        fo4={"root_dir": "C:/FO4", "extracted_dir": "C:/x/fo4"},
        fo76={"root_dir": "C:/FO76"},
    )
    assert games_needing_extraction(s) == ["fo76"]


def test_next_step_advances(monkeypatch):
    monkeypatch.setattr(AppalachiaSetup, "_start_space_prepare", lambda _self: None)
    setup = AppalachiaSetup(_settings())
    assert setup.step == AppalachiaSetup.STEP_WELCOME
    setup.next_step()
    assert setup.step == AppalachiaSetup.STEP_AGREEMENTS
    setup.next_step()
    assert setup.step == AppalachiaSetup.STEP_FEATURES
    setup.next_step()
    assert setup.step == AppalachiaSetup.STEP_SPACE


def test_entering_space_starts_setup_prep_in_background(monkeypatch, tmp_path):
    started = threading.Event()
    release = threading.Event()

    def fake_detect(game_id):
        started.set()
        release.wait(2.0)
        return f"C:/{game_id}"

    monkeypatch.setattr("bacup_ui.setup.detect_game_path", fake_detect)
    monkeypatch.setattr("bacup_ui.setup.get_exe_dir", lambda: tmp_path)
    monkeypatch.setattr(
        "bacup_ui.setup._estimated_extract_gb", lambda _p: 12.0
    )
    monkeypatch.setattr("bacup_ui.setup._dir_size_bytes", lambda _p: 10)
    monkeypatch.setattr("bacup_ui.setup._archive_size_bytes", lambda _p: 5)

    setup = AppalachiaSetup(_settings())
    setup.step = AppalachiaSetup.STEP_FEATURES

    setup._run_footer_primary_action()

    assert setup.step == AppalachiaSetup.STEP_SPACE
    assert started.wait(1.0)
    assert setup.footer_primary_enabled() is False

    release.set()
    assert setup._space_prepare_thread is not None
    setup._space_prepare_thread.join(timeout=2.0)
    setup._poll_space_prepare()

    assert setup._roots["fo4"] == "C:/fo4"
    assert setup._roots["fo76"] == "C:/fo76"
    assert setup.footer_primary_enabled() is True
    assert setup._space_usage() == {"extracted": 10, "mod_output": 10, "ba2": 5}


def test_dir_size_bytes_sums_nested_files(tmp_path):
    nested = tmp_path / "one" / "two"
    nested.mkdir(parents=True)
    (tmp_path / "root.bin").write_bytes(b"1234")
    (nested / "nested.bin").write_bytes(b"123456")

    assert _dir_size_bytes(tmp_path) == 10


def test_alpha_status_copy_is_simple_and_scoped():
    copy = "\n".join(
        (
            _FEATURE_STATUS_TITLE,
            _FEATURE_STATUS_INTRO,
            *_FEATURE_STATUS_WORKS_BEST,
            *_FEATURE_STATUS_NOT_PRIORITY,
            _FEATURE_STATUS_FOOTER,
        )
    )

    assert "Expect Random Crashes" in _FEATURE_STATUS_TITLE
    assert "all major asset categories" in _FEATURE_STATUS_INTRO
    assert "Worldspace and terrain" in _FEATURE_STATUS_WORKS_BEST
    assert "Creatures" in _FEATURE_STATUS_NOT_PRIORITY
    assert "Weapons" in _FEATURE_STATUS_NOT_PRIORITY
    assert "may work, partially work, or not work at all" in _FEATURE_STATUS_FOOTER
    assert "Workshop" not in copy
    assert "workshop" not in copy


def test_setup_uses_current_upgrade_manifest_release_notes():
    manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
    current = next(
        version for version in manifest.versions if version.id == manifest.current
    )

    assert _CURRENT_RELEASE_ID == manifest.current
    assert _CURRENT_RELEASE_LABEL.lower().replace(" ", "") == manifest.current.lower()
    assert _CURRENT_RELEASE_NOTES == current.notes_for_conversion("fo76:fo4")


def test_steam_requirement_copy_is_explicit():
    copy = f"{_STEAM_REQUIREMENT_TEXT}\n{_STEAM_OWNERSHIP_CHECKBOX}"

    assert "Steam copies are required" in copy
    assert "own the selected project's games on Steam" in copy
    assert "Microsoft Store" in copy
    assert "GOG" in copy
    assert "have not pirated them" in copy


def test_footer_primary_label_counts_only_required_fo76_extraction():
    setup = AppalachiaSetup(_settings())
    setup.step = AppalachiaSetup.STEP_PATHS

    assert setup.footer_primary_label() == "Extract 1 game(s) / Continue"

    setup._extracted["fo4"] = "C:/x/fo4"
    assert setup.footer_primary_label() == "Extract 1 game(s) / Continue"

    setup._extracted["fo76"] = "C:/x/fo76"
    assert setup.footer_primary_label() == "Open Converter"


def test_footer_primary_enabled_tracks_agreements_and_extraction():
    setup = AppalachiaSetup(_settings())
    setup.step = AppalachiaSetup.STEP_AGREEMENTS

    assert setup.footer_primary_enabled() is False
    setup._alpha_accepted = True
    setup._personal_use_accepted = True
    setup._steam_ownership_accepted = True
    assert setup.footer_primary_enabled() is True

    setup.step = AppalachiaSetup.STEP_EXTRACT
    setup._extractor = SimpleNamespace(done=False)
    assert setup.footer_primary_enabled() is False

    setup._extractor = SimpleNamespace(done=True)
    assert setup.footer_primary_enabled() is True


def test_paths_valid_requires_both_steam_installs(monkeypatch):
    setup = AppalachiaSetup(_settings())
    setup._roots["fo4"] = "C:/FO4"
    setup._roots["fo76"] = "C:/FO76"

    monkeypatch.setattr(
        "bacup_ui.setup.validate_game_path", lambda _g, _p: True
    )
    monkeypatch.setattr(
        "bacup_ui.setup.validate_steam_install_for_game",
        lambda _g, _p: SimpleNamespace(
            ok=False, message="Fallout 4 install is missing steam_api64.dll."
        ),
    )

    assert setup._paths_valid() is False

    checked_games = []

    def fake_valid_steam_install(game_id, _path):
        checked_games.append(game_id)
        return SimpleNamespace(ok=True, message=f"{game_id} Steam install verified.")

    monkeypatch.setattr(
        "bacup_ui.setup.validate_steam_install_for_game",
        fake_valid_steam_install,
    )
    setup._steam_install_cache.clear()

    assert setup._paths_valid() is True
    assert checked_games == ["fo4", "fo76"]


def test_agreements_require_alpha_personal_use_and_steam_ownership():
    setup = AppalachiaSetup(_settings())
    assert setup.agreements_complete() is False

    setup._alpha_accepted = True
    assert setup.agreements_complete() is False

    setup._personal_use_accepted = True
    assert setup.agreements_complete() is False

    setup._steam_ownership_accepted = True
    assert setup.agreements_complete() is True


def test_persist_agreements_writes_workspace_flags():
    s = _settings()
    setup = AppalachiaSetup(s)
    setup._alpha_accepted = True
    setup._personal_use_accepted = True
    setup._steam_ownership_accepted = True

    setup.persist_agreements()

    ws = s.get_workspace_settings("appalachia")
    assert ws[_ALPHA_ACCEPTED_KEY] is True
    assert ws[_PERSONAL_USE_ACCEPTED_KEY] is True
    assert ws[_STEAM_OWNERSHIP_ACCEPTED_KEY] is True


def test_persist_paths_writes_roots():
    s = _settings()
    setup = AppalachiaSetup(s)
    setup._roots["fo4"] = "C:/FO4"
    setup._roots["fo76"] = "C:/FO76"
    setup.persist_paths()
    assert s.get_game_paths("fo4")["root_dir"] == "C:/FO4"
    assert s.get_game_paths("fo76")["root_dir"] == "C:/FO76"


def test_provided_extracted_dir_skips_that_game():
    s = _settings()
    setup = AppalachiaSetup(s)
    setup._roots["fo4"] = "C:/FO4"
    setup._roots["fo76"] = "C:/FO76"
    setup._extracted["fo76"] = "C:/already/fo76"  # user already has it
    setup.persist_paths()
    assert s.get_game_paths("fo76")["extracted_dir"] == "C:/already/fo76"
    # FO4 reads official BA2s directly; only source games require extraction.
    assert games_needing_extraction(s) == []


def test_start_extraction_skips_when_all_extracted_provided():
    s = _settings()
    setup = AppalachiaSetup(s)
    setup._roots["fo4"] = "C:/FO4"
    setup._roots["fo76"] = "C:/FO76"
    setup._extracted["fo4"] = "C:/x/fo4"
    setup._extracted["fo76"] = "C:/x/fo76"
    started = setup.start_extraction()
    assert started is False
    assert setup._extractor is None  # nothing to extract
    assert setup._completed is False


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
    assert s.get_workspace_settings("appalachia")["app_owned_extracted_games"] == [
        "fo76"
    ]


def test_cleanup_preferences_persist_to_appalachia_workspace():
    s = _settings()
    setup = AppalachiaSetup(s)
    setup._cleanup_extracted = True
    setup._cleanup_mod_output = False

    setup.persist_cleanup_preferences()

    ws = s.get_workspace_settings("appalachia")
    assert ws["cleanup_app_owned_extracted"] is True
    assert ws["cleanup_mod_output_after_deploy"] is False


def test_apply_extraction_results_marks_app_created_dirs():
    s = _settings()
    setup = AppalachiaSetup(s)
    setup._extractor = SimpleNamespace(results={"fo76": "X:/app/extracted/fo76"})

    setup._apply_extraction_results()

    workspace = s.get_workspace_settings("appalachia")
    assert workspace["app_owned_extracted_games"] == ["fo76"]
    assert workspace["app_owned_extracted_paths"] == {"fo76": "X:/app/extracted/fo76"}


def test_project_profiles_have_independent_requirements():
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        fnv={"root_dir": "C:/FNV", "extracted_dir": "C:/x/fnv"},
        fo3={"root_dir": "C:/FO3", "extracted_dir": "C:/x/fo3"},
        workspace=_agreement_settings(),
    )

    assert project_setup_needed(s, "wasteland") is False
    assert project_setup_needed(s, "appalachia") is True
    assert project_setup_needed(s, "north") is True


def test_fables_setup_does_not_require_unrelated_games():
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        skyrimse={"root_dir": "C:/Skyrim", "extracted_dir": "C:/x/skyrimse"},
        workspace=_agreement_settings(),
    )

    assert project_setup_needed(s, "north") is False


def test_wasteland_extraction_includes_fnv_and_grafted_fo3():
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        fnv={"root_dir": "C:/FNV"},
        fo3={"root_dir": "C:/FO3"},
    )

    assert games_needing_extraction(s, "wasteland") == ["fnv", "fo3"]
    s.set_game_extracted_dir("fnv", "C:/x/fnv")
    assert games_needing_extraction(s, "wasteland") == ["fo3"]


def test_north_extraction_requires_skyrim_only():
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        skyrimse={"root_dir": "C:/Skyrim"},
    )

    assert games_needing_extraction(s, "north") == ["skyrimse"]


def test_project_extractor_receives_only_owned_sources(monkeypatch, tmp_path):
    captured = {}

    class FakeExtractor:
        def __init__(self, games, *, output_root):
            captured["games"] = games
            captured["output_root"] = output_root
            self.results = {}

        def start(self):
            captured["started"] = True

    monkeypatch.setattr("bacup_ui.setup._GameExtractor", FakeExtractor)
    s = _settings(
        fo4={"root_dir": "C:/FO4"},
        fnv={"root_dir": "C:/FNV"},
        fo3={"root_dir": "C:/FO3"},
    )

    setup = BacupProjectSetup(s, "wasteland")
    assert setup.start_extraction(output_root=tmp_path) is True
    assert captured == {
        "games": [("fnv", "C:/FNV"), ("fo3", "C:/FO3")],
        "output_root": tmp_path,
        "started": True,
    }


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


def test_project_ownership_helpers_hide_workspace_keys(tmp_path):
    fnv = tmp_path / "extracted" / "fnv"
    fnv.mkdir(parents=True)
    s = _settings(
        fnv={"root_dir": "C:/FNV", "extracted_dir": str(fnv)},
        workspace={
            "wasteland_cleanup_app_owned_extracted": True,
            "wasteland_cleanup_mod_output_after_deploy": False,
            "wasteland_app_owned_extracted_games": ["fnv"],
            "wasteland_app_owned_extracted_paths": {"fnv": str(fnv)},
        },
    )

    ownership = get_project_setup_ownership(s, "wasteland")
    assert ownership.cleanup_extracted is True
    assert ownership.cleanup_mod_output is False
    assert ownership.owned_games == frozenset({"fnv"})
    assert ownership.owned_paths == {"fnv": str(fnv)}
    assert project_owns_extracted_path(s, "wasteland", "fnv", fnv) is True
    assert project_owns_extracted_path(s, "appalachia", "fnv", fnv) is False


def test_project_setup_request_round_trip():
    s = _settings()

    request_project_setup(s, "north")
    assert get_pending_project_setup(s) == "north"
    assert get_active_project(s) == "north"
    clear_pending_project_setup(s)
    assert get_pending_project_setup(s) is None


def test_project_picker_persists_selected_source_set(monkeypatch):
    s = _settings()
    runner_params = SimpleNamespace(app_shall_exit=False)
    monkeypatch.setattr(
        "bacup_ui.setup.hello_imgui.get_runner_params",
        lambda: runner_params,
    )
    picker = BacupProjectPicker(s)

    picker.select_project("wasteland")
    picker.confirm()

    assert get_active_project(s) == "wasteland"
    assert runner_params.app_shall_exit is True


def test_first_run_picker_routes_to_selected_project_setup(monkeypatch):
    calls = []
    s = _settings()

    class FakePicker:
        def __init__(self, settings):
            calls.append(("picker", settings))

        def run(self):
            return "north"

    class FakeSetup:
        def __init__(self, settings, project_id):
            calls.append(("setup", settings, project_id))

        def run(self):
            return True

    monkeypatch.setattr("bacup_ui.setup.BacupProjectPicker", FakePicker)
    monkeypatch.setattr("bacup_ui.setup.BacupProjectSetup", FakeSetup)

    assert _run_bacup_project_setup(s) == (True, True)
    assert calls == [("picker", s), ("setup", s, "north")]


def test_cancelled_first_run_picker_closes_without_setup(monkeypatch):
    s = _settings()

    class FakePicker:
        def __init__(self, _settings):
            pass

        def run(self):
            return None

    monkeypatch.setattr("bacup_ui.setup.BacupProjectPicker", FakePicker)

    assert _run_bacup_project_setup(s) == (True, False)


def test_pending_project_routes_to_selected_setup_and_is_consumed(monkeypatch):
    calls = []
    s = _settings(workspace={"pending_project_setup": "wasteland"})

    class FakeSetup:
        def __init__(self, settings, project_id):
            calls.append((settings, project_id))

        def run(self):
            return True

    monkeypatch.setattr("bacup_ui.setup.BacupProjectSetup", FakeSetup)

    assert _run_bacup_project_setup(s) == (True, True)
    assert calls == [(s, "wasteland")]
    assert get_pending_project_setup(s) is None


def test_cancelled_pending_project_setup_is_consumed(monkeypatch):
    s = _settings(workspace={"pending_project_setup": "north"})

    class FakeSetup:
        def __init__(self, _settings, project_id):
            assert project_id == "north"

        def run(self):
            return False

    monkeypatch.setattr("bacup_ui.setup.BacupProjectSetup", FakeSetup)

    assert _run_bacup_project_setup(s) == (True, False)
    assert get_pending_project_setup(s) is None


def test_completed_setup_does_not_force_unconfigured_appalachia(monkeypatch):
    s = _settings()
    s.setup_complete = True

    class UnexpectedSetup:
        def __init__(self, *_args):
            raise AssertionError("unrelated Appalachia setup should not run")

    monkeypatch.setattr("bacup_ui.setup.AppalachiaSetup", UnexpectedSetup)

    assert _run_bacup_project_setup(s) == (False, True)


def test_extract_estimate_counts_ba2_and_bsa(tmp_path):
    data = tmp_path / "Data"
    data.mkdir()
    (data / "Fallout4.ba2").write_bytes(b"x" * 100)
    (data / "FalloutNV.BSA").write_bytes(b"x" * 200)
    (data / "ignored.txt").write_bytes(b"x" * 500)

    assert _estimated_extract_gb(str(tmp_path)) == (300 * 1.4) / (1024**3)


def test_bacup_profile_names_and_build_constants():
    assert [profile.title for profile in PROJECT_PROFILES.values()] == [
        "Tales From Appalachia",
        "Legends of the Wasteland",
        "Fables of the North",
    ]
    assert [profile.conversion_id for profile in PROJECT_PROFILES.values()] == [
        "fo76:fo4",
        "fnvfo3:fo4",
        "skyrimse:fo4",
    ]
    manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
    current = next(
        version for version in manifest.versions if version.id == manifest.current
    )
    for project_id, profile in PROJECT_PROFILES.items():
        assert _current_release_notes(project_id) == current.notes_for_conversion(
            profile.conversion_id
        )
    bacup_root = Path(__file__).parents[4]
    script = (bacup_root / "build_bacup.ps1").read_text(encoding="utf-8")
    batch = (bacup_root / "build_bacup.bat").read_text(encoding="utf-8")

    assert '$ExeName  = "BACUP"' in script
    assert '$Folder   = "BACUP"' in script
    assert '"build\\bacup"' in script
    for mod_name in (
        "SeventySix",
        "FNV_FO3_Merged",
        "MojaveCapital",
        "Skyrim_Merged",
        "Skyrim",
    ):
        assert f'    "{mod_name}"' in script
    assert '$runtimeDirs = @("data", "PrismaUI_F4")' in script
    assert '$_.Extension -ine ".pdb"' in script
    assert 'foreach ($dir in @("utils", "tools"))' not in script
    assert "Assert-NoDeveloperPayload $PayloadRoot" in script
    assert "Bethesda Asset Converter Universal Platform" in script
    assert "Bethesda Asset Converter Universal Platform" in batch
