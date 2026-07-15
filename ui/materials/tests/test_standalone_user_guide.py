from __future__ import annotations


def test_material_editor_standalone_menu_uses_shared_help_menu(monkeypatch):
    from ui.materials.app import MaterialEditorApp

    app = MaterialEditorApp()
    calls = []

    monkeypatch.setattr("ui.materials.app._draw_toolbar_menu", lambda provider: None)
    monkeypatch.setattr("ui.materials.app.draw_help_menu", lambda provider: calls.append(provider))

    app.draw_standalone_menu()

    assert calls == [app]


def test_material_editor_host_menu_leaves_help_to_host(monkeypatch):
    from ui.materials.app import MaterialEditorApp

    app = MaterialEditorApp()
    calls = []

    monkeypatch.setattr("ui.materials.app._draw_toolbar_menu", lambda provider: None)
    monkeypatch.setattr("ui.materials.app.draw_help_menu", lambda provider: calls.append(provider))

    app.draw_menu()

    assert calls == []


def test_material_editor_toolbar_uses_shared_help_button(monkeypatch):
    from ui.materials.app import MaterialEditorApp

    app = MaterialEditorApp()
    calls = []

    monkeypatch.setattr(
        "ui.materials.app.draw_toolbar_help_button",
        lambda provider, icon_font=None: calls.append((provider, icon_font)),
    )

    app.draw_toolbar(icon_font="font")

    assert calls == [(app, "font")]
