"""Stub imgui_bundle so conversion UI modules can be imported in tests
without pulling in the real GL/native bindings."""
from __future__ import annotations

import sys
import types


class _Anything:
    """Permissive stand-in: every attribute access yields another _Anything,
    every call returns _Anything, and the value is always 0/falsy."""

    def __init__(self, *_args, **_kwargs):
        pass

    def __getattr__(self, _name):
        return _Anything()

    def __call__(self, *_args, **_kwargs):
        return _Anything()

    def __iter__(self):
        return iter(())

    def __bool__(self):
        return False

    def __or__(self, _other):
        return _Anything()

    def __ror__(self, _other):
        return _Anything()

    def __int__(self):
        return 0

    @property
    def value(self):
        return 0


def _ensure_stub() -> None:
    if "imgui_bundle" in sys.modules and hasattr(sys.modules["imgui_bundle"], "hello_imgui"):
        return

    fake_imgui = _Anything()
    fake_hello = _Anything()
    bundle = types.ModuleType("imgui_bundle")
    bundle.imgui = fake_imgui
    bundle.hello_imgui = fake_hello
    bundle.immapp = _Anything()
    bundle.ImVec2 = _Anything()
    bundle.ImVec4 = _Anything()
    bundle.icons_fontawesome_6 = _Anything()
    bundle.imgui_md = _Anything()
    bundle.portable_file_dialogs = _Anything()
    bundle.__getattr__ = lambda _name: _Anything()
    sys.modules["imgui_bundle"] = bundle


_ensure_stub()
