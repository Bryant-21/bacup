from types import SimpleNamespace

from ui.toolkit.app import (
    ToolkitApp,
    _resolve_window_icon_path,
    schedule_window_icon,
)
from ui.toolkit.variants import get_variant


def test_resolve_window_icon_prefers_variant_icon(tmp_path):
    resource_dir = tmp_path / "resource"
    variant_icon = resource_dir / "icons" / "modbox21-nif.ico"
    fallback_icon = resource_dir / "icon.ico"
    variant_icon.parent.mkdir(parents=True)
    variant_icon.write_bytes(b"variant")
    fallback_icon.write_bytes(b"fallback")

    assert _resolve_window_icon_path(resource_dir, get_variant("nif")) == variant_icon


def test_resolve_window_icon_uses_root_icon_for_full_variant(tmp_path):
    resource_dir = tmp_path / "resource"
    root_icon = resource_dir / "icon.ico"
    legacy_full_icon = resource_dir / "icons" / "modbox21-full.ico"
    legacy_full_icon.parent.mkdir(parents=True)
    root_icon.write_bytes(b"root")
    legacy_full_icon.write_bytes(b"legacy")

    assert _resolve_window_icon_path(resource_dir, get_variant("full")) == root_icon


def test_resolve_window_icon_falls_back_to_generic_icon(tmp_path):
    resource_dir = tmp_path / "resource"
    fallback_icon = resource_dir / "icon.ico"
    resource_dir.mkdir()
    fallback_icon.write_bytes(b"fallback")

    assert _resolve_window_icon_path(resource_dir, get_variant("nif")) == fallback_icon


def test_schedule_window_icon_uses_enqueue_post_init(monkeypatch):
    queued: list[object] = []
    called: list[object] = []

    callbacks = SimpleNamespace(enqueue_post_init=lambda fn: queued.append(fn))
    runner_params = SimpleNamespace(callbacks=callbacks)

    monkeypatch.setattr("ui.toolkit.app.hello_imgui.get_runner_params", lambda: runner_params)
    monkeypatch.setattr("ui.toolkit.app.set_window_icon", lambda app_variant=None: called.append(app_variant))

    variant = get_variant("nif")
    schedule_window_icon(variant)

    assert called == []
    assert len(queued) == 1

    queued[0]()

    assert called == [variant]


def test_schedule_window_icon_falls_back_without_enqueue(monkeypatch):
    called: list[object] = []

    callbacks = SimpleNamespace(enqueue_post_init=None)
    runner_params = SimpleNamespace(callbacks=callbacks)

    monkeypatch.setattr("ui.toolkit.app.hello_imgui.get_runner_params", lambda: runner_params)
    monkeypatch.setattr("ui.toolkit.app.set_window_icon", lambda app_variant=None: called.append(app_variant))

    variant = get_variant("nif")
    schedule_window_icon(variant)

    assert called == [variant]


def test_toolkit_post_init_sets_variant_window_icon(monkeypatch):
    called: list[object] = []
    variant = get_variant("full")
    app = ToolkitApp.__new__(ToolkitApp)
    app._app_variant = variant
    app._current_theme = "dark"
    app._mono_font = None
    app._ws_map = {}

    monkeypatch.setattr("ui.toolkit.app.set_window_icon", lambda app_variant=None: called.append(app_variant))
    monkeypatch.setattr("ui.toolkit.app.set_native_dark_title_bar", lambda: None)
    monkeypatch.setattr("ui.toolkit.app.apply_theme", lambda _theme: None)
    monkeypatch.setattr("ui.toolkit.app._signal_ready_file", lambda: None)

    app._post_init()

    assert called == [variant]


def test_standard_setup_post_init_sets_window_icon(monkeypatch):
    called: list[object | None] = []

    monkeypatch.setattr("ui.toolkit.setup_wizard._set_window_icon", lambda app_variant=None: called.append(app_variant))
    monkeypatch.setattr("ui.toolkit.setup_wizard.set_native_dark_title_bar", lambda: None)

    from ui.toolkit.setup_wizard import _post_init

    _post_init()

    assert called == [None]
