from __future__ import annotations


def test_scope_aligner_draw_menu_uses_shared_help_menu(monkeypatch):
    from ui.aligner.aligner_app import ScopeAlignerApp

    app = ScopeAlignerApp()
    calls = []

    monkeypatch.setattr("ui.aligner.aligner_app.draw_help_menu", lambda provider: calls.append(provider))

    app.draw_menu()

    assert calls == [app]


def test_scope_aligner_toolbar_uses_shared_help_button(monkeypatch):
    from ui.aligner.aligner_app import ScopeAlignerApp

    app = ScopeAlignerApp()
    calls = []

    monkeypatch.setattr(
        "ui.aligner.aligner_app.draw_toolbar_help_button",
        lambda provider, icon_font=None: calls.append((provider, icon_font)),
    )

    app.draw_toolbar(icon_font="font")

    assert calls == [(app, "font")]
