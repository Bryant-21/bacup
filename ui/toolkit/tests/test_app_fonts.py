from types import SimpleNamespace

from ui.toolkit.app import ToolkitApp


def _make_app() -> ToolkitApp:
    app = ToolkitApp.__new__(ToolkitApp)
    app._toolbar_icon_font = None
    app._small_font = None
    app._mono_font = None
    app._ai_chat = SimpleNamespace(mono_font=None)
    return app


def test_font_asset_failure_falls_back_and_still_loads_mono_font(monkeypatch):
    app = _make_app()
    mono_font = object()

    def fail_font_load(*_args, **_kwargs):
        raise RuntimeError("IM_ASSERT(false)")

    monkeypatch.setattr(
        "ui.toolkit.app.hello_imgui.load_font_ttf_with_font_awesome_icons",
        fail_font_load,
    )
    monkeypatch.setattr("ui.toolkit.app.os.path.exists", lambda _path: True)
    monkeypatch.setattr(
        "ui.toolkit.app.hello_imgui.FontLoadingParams", SimpleNamespace
    )
    monkeypatch.setattr(
        "ui.toolkit.app.hello_imgui.load_font",
        lambda *_args, **_kwargs: mono_font,
    )

    app._load_fonts()

    assert app._toolbar_icon_font is None
    assert app._small_font is None
    assert app._mono_font is mono_font
    assert app._ai_chat.mono_font is mono_font


def test_mono_font_failure_is_nonfatal(monkeypatch):
    app = _make_app()
    toolbar_font = object()
    small_font = object()
    bundled_fonts = iter((toolbar_font, small_font))

    monkeypatch.setattr(
        "ui.toolkit.app.hello_imgui.load_font_ttf_with_font_awesome_icons",
        lambda *_args, **_kwargs: object(),
    )
    monkeypatch.setattr("ui.toolkit.app.os.path.exists", lambda _path: True)
    monkeypatch.setattr(
        "ui.toolkit.app.hello_imgui.FontLoadingParams", SimpleNamespace
    )

    def load_font(path, *_args, **_kwargs):
        if path.startswith("fonts/"):
            return next(bundled_fonts)
        raise RuntimeError("font could not be loaded")

    monkeypatch.setattr("ui.toolkit.app.hello_imgui.load_font", load_font)

    app._load_fonts()

    assert app._toolbar_icon_font is toolbar_font
    assert app._small_font is small_font
    assert app._mono_font is None
    assert app._ai_chat.mono_font is None
