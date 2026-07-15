from pathlib import Path

from ui.toolkit.setup_wizard import _GameExtractor


def test_output_root_defaults_to_app_root(monkeypatch):
    fake_app_root = Path("X:/fake_app_root")
    monkeypatch.setattr(
        "ui.toolkit.setup_wizard.get_app_root", lambda: fake_app_root, raising=False
    )
    ex = _GameExtractor([("fo4", "C:/Games/Fallout 4")])
    assert ex._output_root == fake_app_root / "extracted"


def test_output_root_honours_explicit_dir():
    ex = _GameExtractor(
        [("fo76", "C:/Games/Fallout76")], output_root=Path("D:/extract_here")
    )
    assert ex._output_root == Path("D:/extract_here")
