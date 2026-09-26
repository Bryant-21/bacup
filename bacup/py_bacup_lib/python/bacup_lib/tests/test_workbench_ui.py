from types import SimpleNamespace

from bacup_lib import workbench_ui as ui


def test_workbench_regeneration_is_fo76_to_fo4_only(tmp_path, monkeypatch):
    from bacup_lib.workflows import unified

    request = SimpleNamespace(source_game="fo76", target_game="fo4", target_data_dir=tmp_path / "game")
    ctx = SimpleNamespace(mod_path=tmp_path / "converted")
    calls = []
    monkeypatch.setattr(ui, "convert_workbench_ui", lambda *args, **kwargs: calls.append((args, kwargs)))
    assert unified._convert_fo76_workbench_ui(request, ctx) == 1
    assert calls == [((ctx.mod_path / "data",), {"target_data_dir":request.target_data_dir})]
    request.source_game = "skyrimse"
    assert unified._convert_fo76_workbench_ui(request, ctx) == 0
    assert len(calls) == 1
