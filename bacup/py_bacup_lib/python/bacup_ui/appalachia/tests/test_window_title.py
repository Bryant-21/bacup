from bacup_lib.upgrade_manifest import bundled_upgrade_manifest_path, load_upgrade_manifest

from bacup_ui.appalachia.window_title import appalachia_window_title


def test_appalachia_window_title_includes_manifest_version():
    manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())

    title = appalachia_window_title()

    assert title.startswith("B.A.C.U.P. - ")
    assert title.endswith(manifest.current)


def test_appalachia_window_title_falls_back_on_error(monkeypatch):
    def _raise(*args, **kwargs):
        raise OSError("manifest missing")

    monkeypatch.setattr("bacup_ui.appalachia.window_title.load_upgrade_manifest", _raise)

    title = appalachia_window_title()

    assert title == "B.A.C.U.P."
