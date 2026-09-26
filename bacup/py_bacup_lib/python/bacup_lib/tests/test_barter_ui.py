from types import SimpleNamespace

from bacup_lib import barter_ui as ui


def test_regeneration_includes_barter_without_other_game_pairs(tmp_path, monkeypatch):
    from bacup_lib.workflows import unified

    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / ui.SOURCE).write_bytes(b"source")
    request = SimpleNamespace(source_game="fo76", target_game="fo4", target_data_dir=tmp_path / "game")
    ctx = SimpleNamespace(source_data_dir=interface.parent, mod_path=tmp_path / "converted")
    calls = []
    monkeypatch.setattr(ui, "convert_barter_ui", lambda *args, **kwargs: calls.append((args, kwargs)) or {"files":[1,2]})
    assert unified._convert_fo76_barter_ui(request, ctx) == 2
    assert calls == [((interface.parent, ctx.mod_path / "data"), {"target_data_dir":request.target_data_dir})]
    request.source_game = "skyrimse"
    assert unified._convert_fo76_barter_ui(request, ctx) == 0
    assert len(calls) == 1


def test_container_menu_shares_the_trade_bridge(tmp_path):
    import pytest
    from pathlib import Path
    from creation_lib.swf import native_runtime

    root = Path(__file__).resolve().parents[5] / "extracted"
    stock = root / "fo4/interface"
    if not (root / "fo76/interface" / ui.SOURCE).is_file() or not (stock / "containermenu.swf").is_file():
        pytest.skip("FO76 source or FO4 interface data unavailable")
    receipt = ui.build_barter_ui(root / "fo76", tmp_path, (stock / "bartermenu.swf").read_bytes(),
                                 (stock / "containermenu.swf").read_bytes())
    assert {ui.MENU.as_posix(), ui.CONTAINER_MENU.as_posix()} <= {entry["path"] for entry in receipt["files"]}
    movie = (tmp_path / ui.CONTAINER_MENU).read_bytes()
    traits = {trait["name"] for trait in native_runtime.abc_class_outline(movie, "ContainerMenu")["instance_traits"]}
    assert {"B21Initialize", "B21SortHint", "B21SetItem", "B21SetHUDColor"} <= traits
    code = {method["method"]: "\n".join(method["code"])
            for method in native_runtime.abc_disassemble(movie, "ContainerMenu", None)}
    assert "CallPropVoid B21Initialize" in code["constructor"]
    assert "CallProperty B21SortHint" in code["UpdateButtonHints"]
    assert "CallPropVoid B21ValidateListHighlight" in code["InvalidateLists"]
    assert "CallPropVoid B21HideStockChrome" in code["RepositionUpperBracketBars"]
