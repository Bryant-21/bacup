"""Stub imgui_bundle (incl. portable_file_dialogs) for the test environment.

portable_file_dialogs is stubbed defensively so no test can ever open a real
native picker (open_file / save_file / select_folder), even indirectly via
modules that import it at module load time.
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
    stub.ImVec2 = MagicMock()
    stub.ImVec4 = MagicMock()

    pfd_stub = MagicMock()
    # Each dialog factory returns an object whose result() yields an empty
    # selection (no file picked) and whose ready() is immediately True.
    def _make_empty_dialog(result_value):
        dlg = MagicMock()
        dlg.ready = MagicMock(return_value=True)
        dlg.result = MagicMock(return_value=result_value)
        return dlg

    pfd_stub.open_file = MagicMock(side_effect=lambda *a, **kw: _make_empty_dialog([]))
    pfd_stub.save_file = MagicMock(side_effect=lambda *a, **kw: _make_empty_dialog(""))
    pfd_stub.select_folder = MagicMock(side_effect=lambda *a, **kw: _make_empty_dialog(""))
    # Bind on the parent stub so `from imgui_bundle import portable_file_dialogs`
    # resolves via getattr (CPython's _handle_fromlist path for stubbed modules).
    stub.portable_file_dialogs = pfd_stub
    sys.modules.setdefault("imgui_bundle.portable_file_dialogs", pfd_stub)
