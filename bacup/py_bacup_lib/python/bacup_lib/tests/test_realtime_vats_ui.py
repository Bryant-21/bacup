from types import SimpleNamespace

from bacup_lib import realtime_vats_ui as ui


def test_pipeline_pair_gate_and_output_ownership(monkeypatch, tmp_path):
    from bacup_lib.workflows import unified
    source = tmp_path / "source/interface"
    source.mkdir(parents=True)
    (source / ui.MENU).touch()
    context = SimpleNamespace(source_data_dir=source.parent, mod_path=tmp_path / "converted")
    calls = []
    monkeypatch.setattr(ui, "convert_realtime_vats_ui", lambda source, output:
                        calls.append((source, output)) or {"files": [1, 2, 3]})
    for source_game, target in (("skyrimse", "fo4"), ("fnv", "fo4"), ("fo76", "skyrimse")):
        assert unified._convert_fo76_realtime_vats_ui(SimpleNamespace(source_game=source_game, target_game=target), context) == 0
    assert calls == []
    assert unified._convert_fo76_realtime_vats_ui(SimpleNamespace(source_game="fo76", target_game="fo4"), context) == 3
    assert calls == [(source.parent, context.mod_path / "data")]
