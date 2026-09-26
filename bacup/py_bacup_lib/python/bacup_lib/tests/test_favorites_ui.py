from types import SimpleNamespace

from bacup_lib import favorites_ui as ui


def test_workflow_only_runs_for_fo76_to_fo4(tmp_path, monkeypatch):
    from bacup_lib.workflows import unified

    (tmp_path / "source/interface").mkdir(parents=True)
    (tmp_path / "source/interface" / ui.MENU).write_bytes(b"source marker")
    calls = []
    monkeypatch.setattr(ui, "convert_favorites_ui", lambda source, output, **kwargs: calls.append((source, output, kwargs["source_plugin"], kwargs["target_data_dir"])) or {"files": [1, 2, 3]})
    context = SimpleNamespace(source_data_dir=tmp_path / "source", mod_path=tmp_path / "mod", source_plugin_path=tmp_path / "SeventySix.esm",
                              target_data_dir=tmp_path / "target")
    assert unified._convert_fo76_favorites_ui(SimpleNamespace(source_game="fo76", target_game="fo4"), context) == 3
    assert calls == [(tmp_path / "source", tmp_path / "mod/data", context.source_plugin_path, context.target_data_dir)]
    for source, target in [("skyrimse", "fo4"), ("fo76", "skyrimse"), ("fnv", "fo4")]:
        assert unified._convert_fo76_favorites_ui(SimpleNamespace(source_game=source, target_game=target), context) == 0
    assert len(calls) == 1
