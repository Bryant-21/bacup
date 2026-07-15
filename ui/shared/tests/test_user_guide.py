from __future__ import annotations


def test_provider_without_guide_is_not_guide_capable():
    from creation_lib.ui.widgets.user_guide import has_user_guide

    class NoGuide:
        pass

    assert has_user_guide(NoGuide()) is False


def test_provider_with_user_guide_is_guide_capable():
    from creation_lib.ui.widgets.user_guide import UserGuide, has_user_guide

    class WithGuide:
        def get_user_guide(self):
            return UserGuide(title="Search", body="Find records and assets.")

    assert has_user_guide(WithGuide()) is True


def test_user_guide_provider_accepts_guide_only_provider():
    from creation_lib.ui.widgets.user_guide import UserGuideProvider

    class GuideOnly:
        def get_user_guide(self):
            return None

    assert isinstance(GuideOnly(), UserGuideProvider) is True


def test_blank_user_guide_still_counts_as_present():
    from creation_lib.ui.widgets.user_guide import UserGuide, has_user_guide

    class BlankGuide:
        def get_user_guide(self):
            return UserGuide(title="", body="")

    assert has_user_guide(BlankGuide()) is True


def test_draw_help_menu_toggles_provider(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import UserGuide, draw_help_menu

    calls: list[str] = []

    class Provider:
        def get_user_guide(self):
            return UserGuide(title="Search", body="Find records and assets.")

        def toggle_user_guide(self):
            calls.append("toggle")

    monkeypatch.setattr(user_guide.imgui, "begin_menu", lambda label: label == "Help")
    monkeypatch.setattr(user_guide.imgui, "menu_item", lambda *args, **kwargs: (True, False))
    monkeypatch.setattr(user_guide.imgui, "end_menu", lambda: None)

    draw_help_menu(Provider())

    assert calls == ["toggle"]


def test_draw_help_menu_disables_missing_guide(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import draw_help_menu

    disabled = {"begin": 0, "end": 0}

    monkeypatch.setattr(user_guide.imgui, "begin_menu", lambda label: label == "Help")
    monkeypatch.setattr(user_guide.imgui, "begin_disabled", lambda: disabled.__setitem__("begin", disabled["begin"] + 1))
    monkeypatch.setattr(user_guide.imgui, "menu_item", lambda *args, **kwargs: (False, False))
    monkeypatch.setattr(user_guide.imgui, "end_disabled", lambda: disabled.__setitem__("end", disabled["end"] + 1))
    monkeypatch.setattr(user_guide.imgui, "end_menu", lambda: None)

    draw_help_menu(object())

    assert disabled == {"begin": 1, "end": 1}


def test_draw_help_menu_disables_missing_toggle(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import UserGuide, draw_help_menu

    disabled = {"begin": 0, "end": 0}

    class Provider:
        def get_user_guide(self):
            return UserGuide(title="Search", body="Find records and assets.")

    monkeypatch.setattr(user_guide.imgui, "begin_menu", lambda label: label == "Help")
    monkeypatch.setattr(user_guide.imgui, "begin_disabled", lambda: disabled.__setitem__("begin", disabled["begin"] + 1))
    monkeypatch.setattr(user_guide.imgui, "menu_item", lambda *args, **kwargs: (False, False))
    monkeypatch.setattr(user_guide.imgui, "end_disabled", lambda: disabled.__setitem__("end", disabled["end"] + 1))
    monkeypatch.setattr(user_guide.imgui, "end_menu", lambda: None)

    draw_help_menu(Provider())

    assert disabled == {"begin": 1, "end": 1}


def test_draw_generic_user_guide_window_renders_markdown_when_visible(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import UserGuide, draw_generic_user_guide_window

    rendered: list[str] = []

    monkeypatch.setattr(user_guide.imgui, "begin", lambda *args, **kwargs: (True, True))
    monkeypatch.setattr(user_guide.imgui, "end", lambda: None)
    monkeypatch.setattr(
        user_guide.hello_imgui,
        "markdown",
        lambda body: rendered.append(body),
        raising=False,
    )

    visible = draw_generic_user_guide_window(
        True,
        UserGuide("Search User Guide", "# Search\n\nFind records."),
    )

    assert visible is True
    assert rendered == ["# Search\n\nFind records."]


def test_draw_generic_user_guide_window_returns_closed_state(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import UserGuide, draw_generic_user_guide_window

    rendered: list[str] = []

    monkeypatch.setattr(user_guide.imgui, "begin", lambda *args, **kwargs: (True, False))
    monkeypatch.setattr(user_guide.imgui, "end", lambda: None)
    monkeypatch.setattr(
        user_guide.hello_imgui,
        "markdown",
        lambda body: rendered.append(body),
        raising=False,
    )

    visible = draw_generic_user_guide_window(
        True,
        UserGuide("Search User Guide", "# Search\n\nFind records."),
    )

    assert visible is False
    assert rendered == ["# Search\n\nFind records."]


def test_draw_generic_user_guide_window_uses_local_fallback_without_mutating_hello_imgui(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import UserGuide, draw_generic_user_guide_window

    monkeypatch.delattr(user_guide.hello_imgui, "markdown", raising=False)

    rendered: list[str] = []
    monkeypatch.setattr(user_guide.imgui, "begin", lambda *args, **kwargs: (True, True))
    monkeypatch.setattr(user_guide.imgui, "end", lambda: None)
    monkeypatch.setattr(user_guide.imgui, "text_wrapped", lambda body: rendered.append(body))

    visible = draw_generic_user_guide_window(
        True,
        UserGuide("Search User Guide", "# Search\n\nFind records."),
    )

    assert visible is True
    assert rendered == ["# Search\n\nFind records."]
    assert hasattr(user_guide.hello_imgui, "markdown") is False


def test_draw_docked_user_guide_window_close_button_hides_panel(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import UserGuide, draw_docked_user_guide_window

    class Provider:
        def get_user_guide(self):
            return UserGuide("Search User Guide", "# Search\n\nFind records.")

    rendered: list[str] = []
    closed = []

    monkeypatch.setattr(user_guide.imgui, "begin", lambda *args, **kwargs: (True, True))
    monkeypatch.setattr(user_guide.imgui, "end", lambda: None)
    monkeypatch.setattr(user_guide.imgui, "get_frame_height", lambda: 28.0)
    monkeypatch.setattr(user_guide.imgui, "calc_text_size", lambda text: type("Vec", (), {"x": 10.0})())
    monkeypatch.setattr(user_guide.imgui, "get_style", lambda: type("Style", (), {"frame_padding": type("Pad", (), {"x": 4.0})()})())
    monkeypatch.setattr(user_guide.imgui, "get_content_region_avail", lambda: type("Vec", (), {"x": 200.0})())
    monkeypatch.setattr(user_guide.imgui, "get_cursor_pos_x", lambda: 0.0)
    monkeypatch.setattr(user_guide.imgui, "set_cursor_pos_x", lambda x: None)
    monkeypatch.setattr(user_guide.imgui, "button", lambda label, size=None: True)
    monkeypatch.setattr(user_guide.imgui, "set_item_tooltip", lambda text: None)
    monkeypatch.setattr(user_guide.imgui, "separator", lambda: None)
    monkeypatch.setattr(user_guide.hello_imgui, "markdown", lambda body: rendered.append(body), raising=False)

    draw_docked_user_guide_window("Help##search", Provider(), on_close=lambda: closed.append(True))

    assert closed == [True]
    assert rendered == ["# Search\n\nFind records."]


def test_base_workspace_draw_user_guide_window_uses_generic_renderer(monkeypatch):
    from creation_lib.ui.shell import BaseWorkspace

    class GuideWorkspace(BaseWorkspace):
        name = "Example"
        id = "example"
        user_guide_body = "Example guide body."

    ws = GuideWorkspace()
    ws._show_user_guide = True
    calls = []

    monkeypatch.setattr(
        "creation_lib.ui.widgets.user_guide.draw_generic_user_guide_window",
        lambda visible, guide: calls.append((visible, guide.title)) or False,
    )

    ws.draw_user_guide_window()

    assert calls == [(True, "Example User Guide")]
    assert ws._show_user_guide is False
