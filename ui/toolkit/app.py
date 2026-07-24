"""ToolkitApp — host shell for ModBox21."""

import ctypes
import logging
import os
import subprocess
import sys
import traceback
from pathlib import Path

from imgui_bundle import hello_imgui, imgui, immapp

from creation_lib.ui.theme.window_chrome import set_ini_folder, set_native_dark_title_bar
from .app_paths import get_ini_dir
from creation_lib.ui.widgets.user_guide import (
    draw_docked_user_guide_window,
    draw_help_menu,
    draw_toolbar_help_button,
    has_user_guide,
)
from .workspace import Workspace
from .shared_panels import AI_AVAILABLE, AIChatPanel, LogPanel
from creation_lib.ui.theme import (
    apply_tab_style,
    apply_theme,
    draw_theme_selector,
    get_theme,
)
from .settings import ToolkitSettings
from creation_lib.ui.shell import SettingsWindow, WorkspaceHost
from creation_lib.ui.settings import general_section, paths_section, indexes_section
from .variants import AppVariant, get_variant
from .version import get_version
from creation_lib.ui.shell import ViewMenuHelper

_log = logging.getLogger("toolkit.app")

TEXTURE_WORKSPACE_IDS = (
    "mat_copier",
    "dds_inspector",
    "dds_png",
    "dds_resizer",
    "color_report",
    "image_utils",
    "img_upscaler",
    "img_quantizer",
)


def _resolve_window_icon_path(resource_dir: str | Path, app_variant: AppVariant | None = None) -> Path | None:
    resource_dir = Path(resource_dir)
    candidates: list[Path] = []
    if app_variant is not None:
        icon_path = Path(app_variant.icon_path)
        if icon_path.parts and icon_path.parts[0] == "resource":
            candidates.append(resource_dir.joinpath(*icon_path.parts[1:]))
        else:
            candidates.append(resource_dir / icon_path)
    candidates.append(resource_dir / "icon.ico")

    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None


def set_window_icon(app_variant: AppVariant | None = None):
    """Set the title-bar/taskbar icon via Win32 WM_SETICON.

    Win32 LoadImageW handles PNG-encoded .ico files natively (Vista+) and is
    more reliable than the PIL+GLFW path. Call from a post_init callback.
    """
    if os.name != "nt":
        return
    try:
        from .app_paths import get_resource_dir

        icon_path = _resolve_window_icon_path(get_resource_dir(), app_variant)
        if icon_path is None:
            _log.warning("Window icon: no .ico found for variant %s",
                         app_variant.id if app_variant else "?")
            return

        window_address = hello_imgui.get_glfw_window_address()
        if not window_address:
            _log.warning("Window icon: GLFW window address unavailable")
            return

        import imgui_bundle

        glfw_dll_path = os.path.join(os.path.dirname(imgui_bundle.__file__), "glfw3.dll")
        glfw = ctypes.CDLL(glfw_dll_path)
        glfw.glfwGetWin32Window.restype = ctypes.c_void_p
        glfw.glfwGetWin32Window.argtypes = [ctypes.c_void_p]
        hwnd = glfw.glfwGetWin32Window(ctypes.c_void_p(window_address))
        if not hwnd:
            _log.warning("Window icon: glfwGetWin32Window returned NULL")
            return

        user32 = ctypes.WinDLL("user32", use_last_error=True)
        user32.LoadImageW.restype = ctypes.c_void_p
        user32.LoadImageW.argtypes = [
            ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_uint,
            ctypes.c_int, ctypes.c_int, ctypes.c_uint,
        ]
        user32.SendMessageW.restype = ctypes.c_void_p
        user32.SendMessageW.argtypes = [
            ctypes.c_void_p, ctypes.c_uint, ctypes.c_void_p, ctypes.c_void_p,
        ]

        IMAGE_ICON = 1
        LR_LOADFROMFILE = 0x00000010
        LR_DEFAULTSIZE = 0x00000040
        WM_SETICON = 0x0080
        ICON_SMALL = 0
        ICON_BIG = 1

        # Load big (32x32) and small (16x16) variants from the same .ico —
        # Windows picks the closest embedded resolution.
        def _load(cx: int, cy: int) -> int | None:
            h = user32.LoadImageW(
                None, str(icon_path), IMAGE_ICON, cx, cy,
                LR_LOADFROMFILE | (LR_DEFAULTSIZE if cx == 0 else 0),
            )
            if not h:
                err = ctypes.get_last_error()
                _log.warning("Window icon: LoadImageW(%dx%d) failed err=%d", cx, cy, err)
                return None
            return h

        h_big = _load(32, 32)
        h_small = _load(16, 16)
        if h_big:
            user32.SendMessageW(hwnd, WM_SETICON, ICON_BIG, h_big)
        if h_small:
            user32.SendMessageW(hwnd, WM_SETICON, ICON_SMALL, h_small)
        if not h_big and not h_small:
            _log.warning("Window icon: no icon handles loaded from %s", icon_path)
            return

        _log.info("Window icon set from %s", icon_path)
    except Exception as e:
        logging.warning(f"Could not set window icon: {e}")


def schedule_window_icon(app_variant: AppVariant | None = None) -> None:
    """Apply the window icon after the platform backend is fully ready when possible."""
    callbacks = hello_imgui.get_runner_params().callbacks
    enqueue = getattr(callbacks, "enqueue_post_init", None)
    if callable(enqueue):
        enqueue(lambda: set_window_icon(app_variant))
        return
    set_window_icon(app_variant)


def _configure_app_window_params(params, settings, app_variant: AppVariant) -> None:
    params.app_window_params.window_title = app_variant.window_title
    ini_name = app_variant.ini_name or f"toolkit-{app_variant.id}"
    set_ini_folder(params, ini_name, get_ini_dir())

    window_width = int(settings.window_width or 1600)
    window_height = int(settings.window_height or 900)
    if app_variant.minimum_window_size is not None:
        minimum_width, minimum_height = app_variant.minimum_window_size
        window_width = max(window_width, minimum_width)
        window_height = max(window_height, minimum_height)
    if app_variant.start_centered:
        params.app_window_params.window_geometry.window_size_state = (
            hello_imgui.WindowSizeState.standard
        )
        params.app_window_params.window_geometry.position_mode = (
            hello_imgui.WindowPositionMode.monitor_center
        )
    params.app_window_params.window_geometry.size = (window_width, window_height)


def _signal_ready_file() -> None:
    ready_file = os.environ.get("MODBOX21_READY_FILE", "").strip()
    if not ready_file:
        return
    try:
        path = Path(ready_file)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("ready\n", encoding="utf-8")
        _log.info("Ready signal written: %s", path)
    except Exception:
        _log.warning("Failed to write ready signal: %s", ready_file, exc_info=True)


class ToolkitApp:
    """Host shell that manages workspaces, shared panels, and the frame loop."""

    def __init__(
        self,
        workspaces: list[Workspace],
        settings: ToolkitSettings,
        launch_path: str | None = None,
        app_variant: AppVariant | None = None,
    ):
        self._workspaces = workspaces
        self._ws_map = {ws.id: ws for ws in workspaces}
        self._settings = settings

        # Register the creation_lib.ui host seam before any shell/section code runs.
        from creation_lib.ui.host import UiHost, set_host
        from ui.toolkit.app_paths import get_app_root, get_db_dir, get_ini_dir

        def _make_db_builder(**kwargs):
            from ui.toolkit.db_builder import DbBuilder  # lazy: heavy transitive imports

            return DbBuilder(**kwargs)

        set_host(
            UiHost(
                get_app_root=get_app_root,
                get_ini_dir=get_ini_dir,
                get_db_dir=get_db_dir,
                resolve_extracted_output_dir=lambda game: Path(get_app_root())
                / "extracted"
                / game,
                db_builder_factory=_make_db_builder,
            )
        )

        self._launch_path = launch_path
        self._app_variant = app_variant or get_variant("full")
        self._launch_open_done = False

        # Shared panels
        project_root = str(Path(__file__).resolve().parents[2])
        self._ai_chat = (
            AIChatPanel(settings, cwd=project_root)
            if AI_AVAILABLE and self._app_variant.include_ai_panel
            else None
        )
        self._log_panel = LogPanel()

        # Activity bar
        # Settings window
        _env_path = Path(__file__).resolve().parents[2] / ".env"
        self._settings_window = SettingsWindow(
            settings,
            include_indexes=self._app_variant.include_index_settings,
            env_path=_env_path,
        )
        _gen_section = general_section.make_section(env_path=_env_path)
        _sw = self._settings_window

        def _trigger_rerun_setup():
            _sw.rerun_setup = True
            _sw._is_open = False

        def _refresh_paths_section():
            _sw._reload_section("paths")

        def _commit_paths_section():
            _sw._commit_section("paths")

        general_section._state.rerun_setup_cb = _trigger_rerun_setup
        general_section._state.paths_changed_cb = _refresh_paths_section
        general_section._state.paths_commit_cb = _commit_paths_section
        self._settings_window.register_section(_gen_section)
        self._settings_window.register_section(paths_section.make_section(settings))
        if self._app_variant.include_index_settings:
            self._settings_window.register_section(
                indexes_section.make_section(
                    extraction_only=self._app_variant.extraction_only_settings
                )
            )

        # Active workspace
        self._active_ws: Workspace | None = None
        self._first_frame = True
        self._show_about: bool = False
        self._show_theme_selector: bool = False
        self._toolbar_icon_font = None
        self._mono_font = None
        self._small_font = None
        self._current_theme = get_theme(self._settings.theme)

        # Apply settings to workspaces
        for ws in workspaces:
            defaults = ws.get_settings_defaults()
            settings.apply_defaults(ws.id, defaults)
        self._apply_settings_to_workspaces()

        # View menu helper — injected into all workspaces via WorkspaceHost
        shared_view_labels = ["Log"]
        if self._ai_chat is not None:
            shared_view_labels.insert(0, "AI Chat")
        self._host = WorkspaceHost(shared_view_labels=shared_view_labels)
        self._view_helper = self._host._view_helper
        for ws in workspaces:
            self._host.register(ws)

    def _apply_settings_to_workspaces(self) -> None:
        """Inject top-level settings (including indexes) into each workspace."""
        for ws in self._workspaces:
            ws_settings = self._settings.get_workspace_settings(ws.id)
            ws_settings["indexes"] = self._settings.indexes
            ws.apply_settings(ws_settings)

    def _bind_shared_panels(self):
        """Bind shared panel draw methods to their DockableWindow gui_functions."""
        dp = hello_imgui.get_runner_params().docking_params
        shared_map = {"Log": self._log_panel.draw}
        if self._ai_chat is not None:
            shared_map["AI Chat"] = self._ai_chat.draw
        for w in dp.dockable_windows:
            if w.label in shared_map:
                w.gui_function = shared_map[w.label]

    def _get_docking_params(self) -> hello_imgui.DockingParams:
        """Build the unified docking layout with all workspace panels."""
        params = hello_imgui.DockingParams()
        params.layout_condition = hello_imgui.DockingLayoutCondition.application_start
        if getattr(
            getattr(self, "_app_variant", None),
            "auto_hide_single_window_tabs",
            False,
        ):
            params.main_dock_space_node_flags |= (
                imgui.DockNodeFlags_.auto_hide_tab_bar
            )

        splits = [
            hello_imgui.DockingSplit(
                initial_dock_="MainDockSpace",
                new_dock_="LeftDock",
                direction_=imgui.Dir.left,
                ratio_=0.20,
            ),
            hello_imgui.DockingSplit(
                initial_dock_="LeftDock",
                new_dock_="LeftDockBottom",
                direction_=imgui.Dir.down,
                ratio_=0.50,
            ),
            hello_imgui.DockingSplit(
                initial_dock_="MainDockSpace",
                new_dock_="RightDock",
                direction_=imgui.Dir.right,
                ratio_=0.22,
            ),
            hello_imgui.DockingSplit(
                initial_dock_="MainDockSpace",
                new_dock_="BottomDock",
                direction_=imgui.Dir.down,
                ratio_=0.25,
            ),
        ]
        params.docking_splits = splits

        _noop = lambda: None  # noqa: E731

        def _win(label, dock):
            w = hello_imgui.DockableWindow(label_=label, dock_space_name_=dock)
            w.call_begin_end = False
            w.gui_function = _noop
            return w

        windows = []
        if self._ai_chat is not None:
            ai_chat_win = _win("AI Chat", "BottomDock")
            ai_chat_win.is_visible = False
            windows.append(ai_chat_win)

        # Shared panels — hidden by default
        log_win = _win("Log", "BottomDock")
        log_win.is_visible = False
        windows.append(log_win)

        for ws in self._workspaces:
            workspace_windows = list(ws.get_dockable_windows())
            windows.extend(workspace_windows)
            has_help_window = any(window.label.startswith("Help##") for window in workspace_windows)
            if has_user_guide(ws) and not has_help_window:
                help_label = f"Help##{ws.id}"
                help_win = _win(help_label, "RightDock")
                help_win.is_visible = False

                def _draw_help_window(workspace=ws, label=help_label, window=help_win):
                    if getattr(workspace, "active", False):
                        draw_docked_user_guide_window(
                            label,
                            workspace,
                            on_close=lambda: setattr(window, "is_visible", False),
                        )

                help_win.gui_function = _draw_help_window
                windows.append(help_win)

        params.dockable_windows = windows
        return params

    def _get_addons(self) -> immapp.AddOnsParams:
        """Merge addon requirements from all workspaces."""
        addons = immapp.AddOnsParams()
        addons.with_markdown = True
        for ws in self._workspaces:
            for key, val in ws.get_required_addons().items():
                if val:
                    setattr(addons, key, True)
        return addons

    def _ensure_initialized(self, ws: Workspace) -> bool:
        """Lazily initialize a workspace on first use (GL context must exist)."""
        if getattr(ws, "_initialized", True):
            return True
        try:
            ws.initialize()
        except Exception:
            _log.error("Failed to initialize %s:\n%s", ws.id, traceback.format_exc())
            return False

        # Post-init hooks: inject shared resources that were loaded before this workspace
        if ws.id == "nif" and self._small_font and getattr(ws, "_app", None):
            ws._app.small_font = self._small_font
        return True

    def _switch_workspace(self, workspace_id: str, *, remember: bool = True) -> bool:
        """Switch to a different workspace."""
        if self._active_ws and self._active_ws.id == workspace_id:
            return True

        new_ws = self._ws_map.get(workspace_id)
        if not new_ws:
            _log.error("Unknown workspace: %s", workspace_id)
            return False

        if not self._ensure_initialized(new_ws):
            return False

        if self._active_ws:
            self._active_ws.on_deactivate()

        self._active_ws = new_ws
        self._active_ws.on_activate()
        if remember:
            self._settings.active_workspace = workspace_id
        _log.info("Switched to workspace: %s", new_ws.name)
        return True

    @staticmethod
    def _resolve_launch_target(path: str | None) -> tuple[str, str] | None:
        """Map a startup file path to the workspace that can open it."""
        if not path:
            return None
        normalized = str(Path(path).expanduser().resolve(strict=False))
        ext = Path(normalized).suffix.lower()
        if ext == ".nif":
            return "nif", normalized
        if ext in {".bgsm", ".bgem"}:
            return "materials", normalized
        if ext in {".psc", ".pex"}:
            return "papyrus", normalized
        if ext == ".hkx":
            workspace_id = (
                "behavior"
                if Path(normalized).name.lower() == "behavior.hkx"
                else "hkx_viewer"
            )
            return workspace_id, normalized
        if ext in {".bsa", ".ba2"}:
            return "bsa_viewer", normalized
        if ext in {".esp", ".esm", ".esl"}:
            return "esp_editor", normalized
        return None

    def _handle_launch_open(self) -> None:
        """Open a file passed on the command line once the host is ready."""
        if self._launch_open_done:
            return
        self._launch_open_done = True

        target = self._resolve_launch_target(self._launch_path)
        if target is None:
            if self._launch_path:
                _log.info("No launch handler for path: %s", self._launch_path)
            return

        workspace_id, path = target
        workspace = self._ws_map.get(workspace_id)
        if workspace is None:
            _log.warning("Launch target workspace not found: %s", workspace_id)
            return

        if not self._switch_workspace(workspace_id, remember=False):
            return

        opener = getattr(workspace, "open_file", None)
        if opener is None:
            _log.warning("Workspace %s cannot open files directly", workspace_id)
            return

        try:
            opener(path)
            _log.info("Opened startup file in %s workspace: %s", workspace_id, path)
        except Exception:
            _log.error(
                "Failed to open startup file: %s\n%s", path, traceback.format_exc()
            )

    def _handle_dropped_files(
        self,
        paths: list[str],
        *,
        x: float | None = None,
        y: float | None = None,
    ) -> bool:
        ws = self._active_ws
        handler = getattr(ws, "handle_file_drop", None) if ws is not None else None
        if handler is None:
            return False
        try:
            return bool(handler(paths, x=x, y=y))
        except Exception:
            _log.error("File drop handler failed:\n%s", traceback.format_exc())
            return False

    def _install_file_drop_callback(self) -> None:
        """Register OS file drops and route them to the active workspace."""
        try:
            import imgui_bundle

            window_address = hello_imgui.get_glfw_window_address()
            if not window_address:
                _log.warning("File drop: GLFW window address unavailable")
                return

            glfw_dll_path = os.path.join(
                os.path.dirname(imgui_bundle.__file__),
                "glfw3.dll",
            )
            glfw = ctypes.CDLL(glfw_dll_path)

            drop_callback_type = ctypes.CFUNCTYPE(
                None,
                ctypes.c_void_p,
                ctypes.c_int,
                ctypes.POINTER(ctypes.c_char_p),
            )
            glfw.glfwSetDropCallback.restype = ctypes.c_void_p
            glfw.glfwSetDropCallback.argtypes = [
                ctypes.c_void_p,
                drop_callback_type,
            ]
            glfw.glfwGetCursorPos.restype = None
            glfw.glfwGetCursorPos.argtypes = [
                ctypes.c_void_p,
                ctypes.POINTER(ctypes.c_double),
                ctypes.POINTER(ctypes.c_double),
            ]

            def _drop_callback(window, count, raw_paths):
                paths = []
                for idx in range(count):
                    raw_path = raw_paths[idx]
                    if raw_path:
                        paths.append(raw_path.decode("utf-8", errors="replace"))
                x = ctypes.c_double()
                y = ctypes.c_double()
                glfw.glfwGetCursorPos(
                    ctypes.c_void_p(window),
                    ctypes.byref(x),
                    ctypes.byref(y),
                )
                self._handle_dropped_files(paths, x=x.value, y=y.value)

            self._glfw_dll = glfw
            self._glfw_drop_callback = drop_callback_type(_drop_callback)
            glfw.glfwSetDropCallback(
                ctypes.c_void_p(window_address),
                self._glfw_drop_callback,
            )
            _log.info("File drop callback installed")
        except Exception:
            _log.warning("File drop callback unavailable:\n%s", traceback.format_exc())

    def _draw_top_toolbar(self):
        """Edge toolbar callback — delegates to the active workspace."""
        ws = self._active_ws
        font = self._toolbar_icon_font
        has = ws is not None and getattr(ws, "has_toolbar", lambda: False)()
        guide = ws is not None and has_user_guide(ws)

        # Show/hide the toolbar strip itself so it doesn't leave an empty bar.
        toolbars = hello_imgui.get_runner_params().callbacks.edges_toolbars
        top = toolbars.get(hello_imgui.EdgeToolbarType.top)
        if top:
            top.options.size_em = 2.5 if has else 0.01

        if has:
            # Center the button row vertically using the larger icon font height.
            if font:
                imgui.push_font(font, font.legacy_size)
            avail_h = imgui.get_content_region_avail().y
            frame_h = imgui.get_frame_height()
            offset = max(0.0, (avail_h - frame_h) / 2.0)
            imgui.set_cursor_pos_y(imgui.get_cursor_pos_y() + offset)
            if font:
                imgui.pop_font()
            ws.draw_toolbar(font)
        if has and guide:
            draw_toolbar_help_button(ws, font, same_line=has)

    def _ws_menu_items(self, ws_ids: list[str]) -> None:
        """Render menu items for a list of workspace IDs."""
        for ws_id in ws_ids:
            ws = self._ws_map.get(ws_id)
            if ws is None:
                continue
            is_active = self._active_ws and ws.id == self._active_ws.id
            if imgui.menu_item(ws.name, "", is_active)[0]:
                if not is_active:
                    self._switch_workspace(ws.id)

    def _show_menus(self):
        """Menu bar content — called by hello_imgui inside its managed menu bar."""
        if self._app_variant.is_standalone:
            self._show_standalone_app_menu()
            if self._active_ws:
                self._active_ws.draw_menu()
                if has_user_guide(self._active_ws):
                    draw_help_menu(self._active_ws)
            return

        if imgui.begin_menu("Workspace"):
            self._ws_menu_items(
                ["mod_builder", "esp_editor", "nif", "search", "papyrus", "swf"]
            )
            imgui.separator()

            # --- Textures ---
            if imgui.begin_menu("Textures"):
                self._ws_menu_items(["palette", "materials"])
                imgui.separator()
                self._ws_menu_items(list(TEXTURE_WORKSPACE_IDS))
                imgui.end_menu()

            # --- Audio ---
            if imgui.begin_menu("Audio"):
                self._ws_menu_items(["voice_changer", "voice_browser"])
                imgui.separator()
                self._ws_menu_items(["audio_extractor", "gun_fire", "laser_beam"])
                imgui.end_menu()

            # --- Mesh ---
            if imgui.begin_menu("Mesh"):
                self._ws_menu_items(["weight_painter", "cloth_maker"])
                imgui.end_menu()

            # --- Animations ---
            if imgui.begin_menu("Animations"):
                self._ws_menu_items(["bone_editor", "aligner"])
                imgui.end_menu()

            # --- Havok ---
            if imgui.begin_menu("Havok"):
                self._ws_menu_items(["behavior"])
                imgui.separator()
                self._ws_menu_items(
                    ["annotation_extract", "hkx_packer", "hkx_converter"]
                )
                imgui.end_menu()

            # --- NIF ---
            if imgui.begin_menu("NIF"):
                self._ws_menu_items(["nif_collision", "nif_fbx", "bulk_nif"])
                imgui.end_menu()

            # --- Mod Tools ---
            if imgui.begin_menu("Mod Tools"):
                self._ws_menu_items(
                    [
                        "subgraph_maker",
                        "bsa_viewer",
                        "bsa_extractor",
                        "mass_bsa",
                        "archlist_creator",
                        "folder_renamer",
                        "modlist_merger",
                    ]
                )
                imgui.end_menu()

            imgui.separator()

            # --- Settings ---
            if imgui.begin_menu("Settings"):
                if imgui.menu_item("General...", "", False)[0]:
                    self._settings_window.open("general")
                if imgui.menu_item("Paths...", "", False)[0]:
                    self._settings_window.open("paths")
                if self._active_ws and hasattr(self._active_ws, "draw_settings"):
                    if imgui.menu_item(f"{self._active_ws.name}...", "", False)[0]:
                        self._settings_window.open(f"ws:{self._active_ws.id}")
                imgui.separator()
                if imgui.menu_item("Theme...", "", False)[0]:
                    self._show_theme_selector = True
                rp = hello_imgui.get_runner_params()
                _, rp.imgui_window_params.show_status_fps = imgui.menu_item(
                    "Show FPS", "", rp.imgui_window_params.show_status_fps
                )
                _, rp.imgui_window_params.show_status_bar = imgui.menu_item(
                    "Show Status Bar", "", rp.imgui_window_params.show_status_bar
                )
                imgui.end_menu()

            imgui.separator()
            if imgui.menu_item("About", "", False)[0]:
                self._show_about = True
            if imgui.menu_item("Quit", "", False)[0]:
                hello_imgui.get_runner_params().app_shall_exit = True
            imgui.end_menu()

        # Workspace-specific menus
        if self._active_ws:
            self._active_ws.draw_menu()
            if has_user_guide(self._active_ws):
                draw_help_menu(self._active_ws)

    def _show_standalone_app_menu(self) -> None:
        if imgui.begin_menu("App"):
            if imgui.begin_menu("Settings"):
                if imgui.menu_item("General...", "", False)[0]:
                    self._settings_window.open("general")
                if imgui.menu_item("Paths...", "", False)[0]:
                    self._settings_window.open("paths")
                if self._active_ws and hasattr(self._active_ws, "draw_settings"):
                    if imgui.menu_item(f"{self._active_ws.name}...", "", False)[0]:
                        self._settings_window.open(f"ws:{self._active_ws.id}")
                imgui.separator()
                if imgui.menu_item("Theme...", "", False)[0]:
                    self._show_theme_selector = True
                rp = hello_imgui.get_runner_params()
                _, rp.imgui_window_params.show_status_fps = imgui.menu_item(
                    "Show FPS", "", rp.imgui_window_params.show_status_fps
                )
                _, rp.imgui_window_params.show_status_bar = imgui.menu_item(
                    "Show Status Bar", "", rp.imgui_window_params.show_status_bar
                )
                imgui.end_menu()
            imgui.separator()
            if imgui.menu_item("About", "", False)[0]:
                self._show_about = True
            if imgui.menu_item("Quit", "", False)[0]:
                hello_imgui.get_runner_params().app_shall_exit = True
            imgui.end_menu()

    def _gui(self):
        """Main frame loop — called by hello_imgui each frame."""
        # Reapply tab style each frame — hello_imgui re-applies the base Darcula theme
        # on certain events (focus change, docking), which would otherwise reset our overrides.
        self._apply_tab_style()

        if self._first_frame:
            self._log_panel.install()
            self._bind_shared_panels()
            # Only initialize the active workspace on first frame —
            # others are lazily initialized when first switched to.
            initial_id = self._settings.active_workspace
            launch_target = self._resolve_launch_target(self._launch_path)
            remember_initial_workspace = launch_target is None
            if launch_target is not None:
                initial_id = launch_target[0]
            if initial_id not in self._ws_map:
                initial_id = self._workspaces[0].id
            self._switch_workspace(initial_id, remember=remember_initial_workspace)
            self._handle_launch_open()
            self._first_frame = False

        # Menu content drawn via hello_imgui's show_menus callback (see _show_menus)

        # Tag log entries with active workspace
        if self._active_ws:
            self._log_panel._active_workspace_id = self._active_ws.id

        # Shared panels (AI Chat, Log) drawn via DockableWindow gui_functions
        self._settings_window._active_workspace = self._active_ws
        self._settings_window.draw()
        if self._settings_window.saved_and_closed:
            self._settings_window.saved_and_closed = False
            self._apply_settings_to_workspaces()
        if self._settings_window.rerun_setup:
            self._settings_window.rerun_setup = False
            self._settings.setup_complete = False
            self._settings.save()
            # Relaunch so the setup wizard runs at startup in the new process
            from app.paths import is_frozen

            if is_frozen():
                subprocess.Popen([sys.executable])
            else:
                subprocess.Popen([sys.executable] + sys.argv)
            hello_imgui.get_runner_params().app_shall_exit = True

        # About dialog
        if self._show_about:
            imgui.open_popup("About ModBox21##about")
            self._show_about = False
        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        if imgui.begin_popup_modal(
            "About ModBox21##about", None, imgui.WindowFlags_.always_auto_resize
        )[0]:
            imgui.text("ModBox21 — Bethesda Modding Toolkit")
            imgui.separator()
            imgui.text(f"Version:  {get_version()}")
            imgui.spacing()
            imgui.set_cursor_pos_x((imgui.get_window_width() - 80) * 0.5)
            if imgui.button("OK", imgui.ImVec2(80, 0)):
                imgui.close_current_popup()
            imgui.end_popup()

        # Theme selector modal
        if self._show_theme_selector:
            imgui.open_popup("Theme##theme_modal")
            self._show_theme_selector = False
        if imgui.begin_popup_modal(
            "Theme##theme_modal", None, imgui.WindowFlags_.always_auto_resize
        )[0]:
            imgui.text("Select a theme:")
            imgui.spacing()
            new_id = draw_theme_selector(self._current_theme.id)
            if new_id is not None:
                self._current_theme = get_theme(new_id)
                apply_theme(self._current_theme)
                self._settings.theme = new_id
            imgui.spacing()
            imgui.separator()
            imgui.spacing()
            # Still expose hello_imgui's built-in tweaker for fine-tuning
            tweaked_theme = (
                hello_imgui.get_runner_params().imgui_window_params.tweaked_theme
            )
            if imgui.tree_node("Advanced Tweaks"):
                if hello_imgui.show_theme_tweak_gui(tweaked_theme):
                    hello_imgui.apply_tweaked_theme(tweaked_theme)
                imgui.tree_pop()
            imgui.spacing()
            imgui.set_cursor_pos_x((imgui.get_window_width() - 80) * 0.5)
            if imgui.button("Close", imgui.ImVec2(80, 0)):
                imgui.close_current_popup()
            imgui.end_popup()

        # Active workspace
        if self._active_ws:
            try:
                self._active_ws.draw()
                guide_drawer = getattr(self._active_ws, "draw_user_guide_window", None)
                if guide_drawer is not None:
                    guide_drawer()
            except Exception:
                _log.error("Workspace error:\n%s", traceback.format_exc())
                imgui.begin("Viewport##error")
                imgui.text_colored(
                    imgui.ImVec4(1, 0.3, 0.3, 1),
                    "Workspace error — see Log panel",
                )
                imgui.end()

    def _on_exit(self):
        """Called by hello_imgui before exit."""
        # Cleanup all workspaces
        for ws in self._workspaces:
            try:
                ws.cleanup()
            except Exception:
                _log.error("Cleanup error for %s:\n%s", ws.id, traceback.format_exc())

        # Collect and save settings
        for ws in self._workspaces:
            try:
                self._settings.set_workspace_settings(ws.id, ws.collect_settings())
            except Exception:
                pass
        self._settings.save()

        # Cleanup shared panels
        if self._ai_chat:
            self._ai_chat.cleanup()
        self._log_panel.uninstall()

    def _load_fonts(self):
        """Load fonts: default + FontAwesome icons, then Cascadia Mono for the terminal."""
        try:
            # Load Roboto as default font, merged with FontAwesome 6 for toolbar icons.
            # Roboto-Regular.ttf ships inside imgui_bundle assets (inside_assets=True by default).
            hello_imgui.load_font_ttf_with_font_awesome_icons(
                "fonts/Roboto/Roboto-Regular.ttf", 16.0
            )

            # Larger FA6 icon font for the top toolbar buttons.
            self._toolbar_icon_font = hello_imgui.load_font(
                "fonts/Font_Awesome_6_Free-Solid-900.otf", 20.0
            )

            # Smaller Roboto for compact overlays (e.g. controls HUD).
            small = hello_imgui.load_font("fonts/Roboto/Roboto-Regular.ttf", 12.0)
            if small:
                self._small_font = small
        except Exception:
            _log.warning(
                "Bundled UI fonts unavailable; using ImGui's default font",
                exc_info=True,
            )

        candidates = [
            r"C:\Windows\Fonts\CascadiaMono.ttf",
            r"C:\Windows\Fonts\CascadiaCode.ttf",
            r"C:\Windows\Fonts\consola.ttf",
        ]
        font_path = next((p for p in candidates if os.path.exists(p)), None)
        if font_path is None:
            return
        try:
            params = hello_imgui.FontLoadingParams()
            params.inside_assets = False
            font = hello_imgui.load_font(font_path, 14.0, params)
            if font:
                self._mono_font = font
                if self._ai_chat:
                    self._ai_chat.mono_font = font
        except Exception:
            _log.warning(
                "Monospace UI font unavailable; using ImGui's default font",
                exc_info=True,
            )

    def _post_init(self):
        initial_display_size = getattr(self, "_initial_display_size", None)
        if initial_display_size is not None:
            io = imgui.get_io()
            if io.display_size.x < 0.0 or io.display_size.y < 0.0:
                width, height = initial_display_size
                io.display_size = (float(width), float(height))
        set_window_icon(self._app_variant)
        set_native_dark_title_bar()
        apply_theme(self._current_theme)
        _signal_ready_file()
        if self._mono_font:
            papyrus_ws = self._ws_map.get("papyrus")
            if papyrus_ws is not None:
                papyrus_ws.mono_font = self._mono_font

    def _apply_tab_style(self):
        """Reapply themed tab colors each frame (hello_imgui resets on certain events)."""
        apply_tab_style(self._current_theme)

    def run(self):
        """Launch the toolkit."""
        params = hello_imgui.RunnerParams()
        _configure_app_window_params(params, self._settings, self._app_variant)
        self._initial_display_size = tuple(
            params.app_window_params.window_geometry.size
        )
        params.callbacks.show_gui = self._gui
        params.callbacks.before_exit = self._on_exit
        params.callbacks.load_additional_fonts = self._load_fonts
        params.callbacks.default_icon_font = hello_imgui.DefaultIconFont.font_awesome6
        params.callbacks.post_init = self._post_init
        params.callbacks.post_init_add_platform_backend_callbacks = (
            self._install_file_drop_callback
        )
        params.imgui_window_params.default_imgui_window_type = (
            hello_imgui.DefaultImGuiWindowType.provide_full_screen_dock_space
        )
        params.imgui_window_params.show_menu_bar = True
        params.imgui_window_params.show_menu_app = False
        params.imgui_window_params.show_menu_view = False
        params.callbacks.show_menus = self._show_menus
        params.imgui_window_params.show_status_bar = True
        params.imgui_window_params.show_status_fps = True
        params.imgui_window_params.tweaked_theme = hello_imgui.ImGuiTweakedTheme(
            hello_imgui.ImGuiTheme_.darcula
        )
        params.docking_params = self._get_docking_params()

        # FPS management — cap idle FPS to save CPU/GPU when not interacting
        params.fps_idling.enable_idling = True
        params.fps_idling.fps_idle = 10.0

        # Top icon toolbar — context-sensitive per workspace
        _toolbar_opts = hello_imgui.EdgeToolbarOptions()
        _toolbar_opts.size_em = 2.5
        params.callbacks.add_edge_toolbar(
            hello_imgui.EdgeToolbarType.top,
            self._draw_top_toolbar,
            _toolbar_opts,
        )

        try:
            immapp.run(runner_params=params, add_ons_params=self._get_addons())
        except RuntimeError as e:
            if "ini parsing failed" in str(e):
                # Corrupted ini file (e.g. truncated from abrupt exit) — delete and retry once
                from pathlib import Path

                ini_path = Path(params.ini_filename)
                if ini_path.is_file():
                    _log.warning(
                        "Corrupted ini file detected (%s), deleting and retrying: %s",
                        ini_path.name,
                        e,
                    )
                    ini_path.unlink()
                    immapp.run(runner_params=params, add_ons_params=self._get_addons())
                else:
                    raise
            else:
                raise
