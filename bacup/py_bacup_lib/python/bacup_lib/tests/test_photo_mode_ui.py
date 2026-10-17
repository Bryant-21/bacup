from bacup_lib import photo_mode_ui as ui


def test_workflow_routes_only_fo76_fo4(monkeypatch, tmp_path):
    from types import SimpleNamespace
    from bacup_lib.workflows import unified
    calls = []
    monkeypatch.setattr(ui, "convert_photo_mode_ui", lambda source, target, **kwargs: calls.append((source, target, kwargs)) or {"files": [1]})
    monkeypatch.setattr(unified, "_merge_packaged_tales_translations", lambda *args: None)
    ctx = SimpleNamespace(source_data_dir=tmp_path / "source", mod_path=tmp_path / "converted",
                          source_plugin_path=tmp_path / "SeventySix.esm", target_extracted_dir=tmp_path / "fo4")
    assert unified._convert_fo76_photo_mode_ui(SimpleNamespace(source_game="fo76", target_game="fo4"), ctx) == 1
    assert calls == [(ctx.source_data_dir, ctx.mod_path / "data",
                      {"source_plugin": ctx.source_plugin_path, "target_root": ctx.target_extracted_dir})]
    assert unified._convert_fo76_photo_mode_ui(SimpleNamespace(source_game="skyrimse", target_game="fo4"), ctx) == 0
