from unittest.mock import patch

from ui.builder.mod_builder_app import ModBuilderApp


def test_builder_preserve_ini_setting_round_trips_per_mod(tmp_path):
    mods = tmp_path / "mods"
    for name in ("B21_A", "B21_B"):
        (mods / name).mkdir(parents=True)
    with patch.object(ModBuilderApp, "_refresh_mods", lambda self: None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(mods)
    ):
        app = ModBuilderApp()
        app._mod_list = ["B21_A", "B21_B"]
        app._mod_kinds = ["mod", "mod"]
        app._selected_mod_idx = 0
        assert app._preserve_xse_inis is False
        app._preserve_xse_inis = True
        app._save_mod_settings()
        app._selected_mod_idx = 1
        app._load_mod_settings()
        assert app._preserve_xse_inis is False
        app._selected_mod_idx = 0
        app._load_mod_settings()
        assert app._preserve_xse_inis is True


def test_builder_all_deploy_actions_forward_captured_ini_setting(tmp_path):
    with patch.object(ModBuilderApp, "_refresh_mods", lambda self: None):
        app = ModBuilderApp()
    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._resolve_deploy_data_path = lambda game: tmp_path / "MO2" / "B21_Test"
    app._resolve_game_dir_path = lambda game: tmp_path / "Game"
    app._check_plugin_errors_before_animdata = lambda *args: None
    queued = []
    calls = []
    app._run_fn = lambda target, **kwargs: queued.append(target)
    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "app.paths.get_resource_dir", return_value=tmp_path / "resource"
    ), patch("creation_lib.build.deployer.deploy_mod", autospec=True, side_effect=lambda *args, **kwargs: calls.append(kwargs)), patch(
        "creation_lib.build.loose_deploy.deploy_loose_assets", autospec=True, side_effect=lambda *args, **kwargs: calls.append(kwargs)
    ), patch("creation_lib.ck.automation.generate_anim_data", return_value=None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(tmp_path / "mods")
    ):
        app._preserve_xse_inis = True
        app._on_xse_deploy()
        app._on_deploy()
        app._on_deploy_loose()
        app._run_release_anim_data("B21_Test", on_done=None)
        app._preserve_xse_inis = False
        for target in queued:
            target(lambda message: None)
    assert len(calls) == 4
    assert all(call["preserve_xse_inis"] is True for call in calls)
