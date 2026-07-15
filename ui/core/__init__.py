"""Core ImGui framework — shared by all UI modules."""

from app.paths import get_app_root as _get_app_root
from creation_lib.ui.theme.window_chrome import CommandRunner, AsyncWorker

PROJECT_ROOT = _get_app_root()
from ui.core.imgui_widgets import *
