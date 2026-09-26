from types import SimpleNamespace

from bacup_lib import challenge_ui as ui


def test_workflow_routes_catalog_and_ui_only_for_fo76_fo4(tmp_path, monkeypatch):
    from bacup_lib import challenge_catalog, challenge_hud_ui
    from bacup_lib.workflows import unified

    calls = []
    monkeypatch.setattr(challenge_catalog, "emit_challenge_catalog",
        lambda *args, **kwargs: calls.append(("catalog", args, kwargs)) or {"challenges": [1]})
    monkeypatch.setattr(ui, "convert_challenge_map_ui",
        lambda *args: calls.append(("ui", args)) or {"files": [1, 2]})
    monkeypatch.setattr(challenge_hud_ui, "convert_challenge_hud_ui",
        lambda *args: calls.append(("hud", args)) or {"files": [1, 2, 3]})
    context = SimpleNamespace(source_data_dir=tmp_path / "source", mod_path=tmp_path / "mod",
        source_plugin_path=tmp_path / "SeventySix.esm", output_plugin_name="Converted.esm")
    request = SimpleNamespace(source_game="fo76", target_game="fo4", target_data_dir=tmp_path / "Fallout4/Data")
    assert unified._convert_fo76_challenges(request, context) == 5
    assert calls == [("catalog", (context.source_plugin_path, context.mod_path, request.target_data_dir),
                      {"output_plugin_name": "Converted.esm"}),
                     ("ui", (context.source_data_dir, context.mod_path)),
                     ("hud", (context.source_data_dir, context.mod_path / "data"))]
    for source, target in [("skyrimse", "fo4"), ("fnv", "fo4"), ("fo76", "starfield")]:
        request.source_game, request.target_game = source, target
        assert unified._convert_fo76_challenges(request, context) == 0
    assert len(calls) == 3
