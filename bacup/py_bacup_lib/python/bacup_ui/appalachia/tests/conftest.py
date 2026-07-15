"""Stub imgui_bundle (incl. portable_file_dialogs) for the test environment.

Mirrors ui/toolkit/tests/conftest.py so the combined suite behaves identically
regardless of which conftest loads first (MagicMock supports the ``|`` operator
used by evaluated ``imgui.X | None`` annotations in heavier toolkit modules).
"""
import sys
from unittest.mock import MagicMock


def pytest_configure(config):
    stub = MagicMock()
    for mod in [
        "imgui_bundle",
        "imgui_bundle.imgui",
        "imgui_bundle.hello_imgui",
        "imgui_bundle.immapp",
    ]:
        sys.modules.setdefault(mod, stub)

    pfd_stub = MagicMock()

    def _make_empty_dialog(result_value):
        dlg = MagicMock()
        dlg.ready = MagicMock(return_value=True)
        dlg.result = MagicMock(return_value=result_value)
        return dlg

    pfd_stub.open_file = MagicMock(side_effect=lambda *a, **kw: _make_empty_dialog([]))
    pfd_stub.save_file = MagicMock(side_effect=lambda *a, **kw: _make_empty_dialog(""))
    pfd_stub.select_folder = MagicMock(side_effect=lambda *a, **kw: _make_empty_dialog(""))
    stub.portable_file_dialogs = pfd_stub
    sys.modules.setdefault("imgui_bundle.portable_file_dialogs", pfd_stub)
