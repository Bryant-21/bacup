"""Shared panels that persist across workspace switches: AI Chat + Log."""

import logging
import os
from collections import deque

from imgui_bundle import imgui

try:
    from ui.ai import AI_AVAILABLE
    from ui.ai.terminal_panel import TerminalPanel, ChatSession, _BACKENDS
except ImportError:
    AI_AVAILABLE = False


class AIChatPanel:
    """Persistent AI chat panel with multiple tabbed terminal sessions."""

    _ENGINE_TO_LABEL = {
        "claude_code": "Claude Code",
        "opencode": "OpenCode",
    }

    # Tab accent colors — cycled per session. HSL-ish, muted to fit dark theme.
    _TAB_COLORS = [
        # (inactive,                          hovered,                            active/selected,                    overline)
        (imgui.ImVec4(0.18, 0.28, 0.22, 1.0), imgui.ImVec4(0.25, 0.38, 0.30, 1.0), imgui.ImVec4(0.28, 0.42, 0.34, 1.0), imgui.ImVec4(0.40, 0.75, 0.50, 1.0)),  # green
        (imgui.ImVec4(0.22, 0.22, 0.32, 1.0), imgui.ImVec4(0.30, 0.30, 0.42, 1.0), imgui.ImVec4(0.34, 0.34, 0.48, 1.0), imgui.ImVec4(0.50, 0.50, 0.85, 1.0)),  # blue
        (imgui.ImVec4(0.30, 0.22, 0.18, 1.0), imgui.ImVec4(0.40, 0.30, 0.24, 1.0), imgui.ImVec4(0.45, 0.34, 0.26, 1.0), imgui.ImVec4(0.85, 0.55, 0.35, 1.0)),  # orange
        (imgui.ImVec4(0.28, 0.18, 0.28, 1.0), imgui.ImVec4(0.38, 0.25, 0.38, 1.0), imgui.ImVec4(0.42, 0.28, 0.42, 1.0), imgui.ImVec4(0.75, 0.40, 0.75, 1.0)),  # purple
        (imgui.ImVec4(0.18, 0.28, 0.30, 1.0), imgui.ImVec4(0.25, 0.38, 0.40, 1.0), imgui.ImVec4(0.28, 0.42, 0.45, 1.0), imgui.ImVec4(0.35, 0.75, 0.80, 1.0)),  # teal
        (imgui.ImVec4(0.30, 0.28, 0.18, 1.0), imgui.ImVec4(0.40, 0.38, 0.24, 1.0), imgui.ImVec4(0.45, 0.42, 0.26, 1.0), imgui.ImVec4(0.85, 0.78, 0.35, 1.0)),  # yellow
    ]

    def __init__(self, settings=None, cwd: str | None = None):
        if cwd is None:
            cwd = os.path.normpath(os.path.join(os.path.dirname(__file__), ".."))
        self._cwd = cwd
        self._renderer = TerminalPanel()
        self._sessions: list[ChatSession] = []
        self._next_id = 1
        self._session_counters: dict[str, int] = {}  # backend_key -> count
        self._visible = True
        self._close_queue: list[int] = []  # session IDs to close after frame
        self.mono_font = None  # Set by toolkit _load_fonts

        # Determine default backend from settings
        self._default_backend = "Claude Code"
        if settings is not None:
            default_engine = settings.ai_engines.get("default", "claude_code")
            label = self._ENGINE_TO_LABEL.get(default_engine, "Claude Code")
            if label in _BACKENDS:
                self._default_backend = label

    def _create_session(self, backend_key: str) -> ChatSession:
        """Create a new chat session with the given backend."""
        count = self._session_counters.get(backend_key, 0) + 1
        self._session_counters[backend_key] = count
        label = f"{backend_key} #{count}"
        color_idx = (self._next_id - 1) % len(self._TAB_COLORS)
        session = ChatSession(self._next_id, backend_key, label, self._cwd,
                              color_idx=color_idx)
        self._next_id += 1
        self._sessions.append(session)
        return session

    def _close_session(self, session_id: int):
        """Stop PTY and remove session."""
        for i, s in enumerate(self._sessions):
            if s.session_id == session_id:
                s.stop()
                self._sessions.pop(i)
                break

    def _queue_close(self, session_id: int):
        """Add session ID to close queue, deduplicating."""
        if session_id not in self._close_queue:
            self._close_queue.append(session_id)

    def _restart_session(self, session: ChatSession):
        """Stop and relaunch the same backend."""
        session.stop()
        session.launch()

    def draw(self):
        # Visibility is managed by hello_imgui's DockableWindow.is_visible
        # (toggled via View menu). Don't use self._visible as p_open — it
        # desynchronizes with the docking system and breaks the View toggle.
        expanded = imgui.begin("AI Chat")
        if not expanded:
            for s in self._sessions:
                s.focused = False
            imgui.end()
            return

        # Propagate font to renderer
        self._renderer.mono_font = self.mono_font

        # Auto-create initial tab on first draw (not launched — user clicks Launch)
        if not self._sessions:
            self._create_session(self._default_backend)

        # Process deferred closes from last frame
        for sid in self._close_queue:
            self._close_session(sid)
        self._close_queue.clear()

        tab_flags = (
            imgui.TabBarFlags_.reorderable.value
            | imgui.TabBarFlags_.auto_select_new_tabs.value
            | imgui.TabBarFlags_.fitting_policy_scroll.value
        )
        if imgui.begin_tab_bar("##ai_tabs", tab_flags):
            # Render each session tab
            for session in list(self._sessions):
                running_marker = ""
                if session.is_running:
                    running_marker = " \u25cf"  # filled circle
                display_label = session.label + running_marker + f"##{session.session_id}"

                # Push per-tab accent color
                inactive, hovered, active, overline = self._TAB_COLORS[session.color_idx]
                imgui.push_style_color(imgui.Col_.tab, inactive)
                imgui.push_style_color(imgui.Col_.tab_hovered, hovered)
                imgui.push_style_color(imgui.Col_.tab_selected, active)
                imgui.push_style_color(imgui.Col_.tab_selected_overline, overline)
                imgui.push_style_color(imgui.Col_.tab_dimmed, inactive)
                imgui.push_style_color(imgui.Col_.tab_dimmed_selected, active)

                selected, tab_open = imgui.begin_tab_item(display_label, True)

                imgui.pop_style_color(6)

                # Context menu on tab header
                if imgui.begin_popup_context_item(f"##tabctx_{session.session_id}"):
                    if session.is_running:
                        if imgui.menu_item("Restart", "", False)[0]:
                            self._restart_session(session)
                        if imgui.menu_item("Stop", "", False)[0]:
                            session.stop()
                    else:
                        if imgui.menu_item("Launch", "", False)[0]:
                            session.launch()
                    imgui.separator()
                    if imgui.menu_item("Close", "", False)[0]:
                        self._queue_close(session.session_id)
                    if len(self._sessions) > 1:
                        if imgui.menu_item("Close Others", "", False)[0]:
                            for other in self._sessions:
                                if other.session_id != session.session_id:
                                    self._queue_close(other.session_id)
                    imgui.end_popup()

                if selected:
                    # Track focus for keyboard capture
                    window_focused = imgui.is_window_focused(
                        imgui.FocusedFlags_.root_and_child_windows.value
                    )
                    session.focused = window_focused
                    # Unfocus other sessions
                    for other in self._sessions:
                        if other.session_id != session.session_id:
                            other.focused = False

                    self._renderer.draw_terminal(session)
                    imgui.end_tab_item()
                else:
                    session.focused = False

                if not tab_open:
                    self._queue_close(session.session_id)

            # "+" button to add new session
            if imgui.tab_item_button("+", imgui.TabItemFlags_.trailing.value):
                imgui.open_popup("##new_session")

            if imgui.begin_popup("##new_session"):
                for backend in _BACKENDS:
                    if imgui.menu_item(backend, "", False)[0]:
                        new_session = self._create_session(backend)
                        new_session.launch()
                imgui.end_popup()

            imgui.end_tab_bar()

        imgui.end()

    def cleanup(self):
        """Stop all PTY sessions."""
        for session in self._sessions:
            session.stop()
        self._sessions.clear()


class LogPanel:
    """Scrollable log panel that captures Python logging output.

    Each message is tagged with the active workspace ID for filtering.
    """

    _MAX_LINES = 5000

    # Colors per log level
    _LEVEL_COLORS = {
        logging.DEBUG: imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
        logging.INFO: imgui.ImVec4(0.8, 0.8, 0.8, 1.0),
        logging.WARNING: imgui.ImVec4(1.0, 0.8, 0.2, 1.0),
        logging.ERROR: imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
        logging.CRITICAL: imgui.ImVec4(1.0, 0.2, 0.2, 1.0),
    }

    # Level filter buttons (label, level constant)
    _LEVEL_BUTTONS = [
        ("Debug", logging.DEBUG),
        ("Info", logging.INFO),
        ("Warning", logging.WARNING),
        ("Error", logging.ERROR),
    ]

    def __init__(self):
        self._lines: deque[tuple[int, str, str]] = deque(maxlen=self._MAX_LINES)
        self._handler = _LogHandler(self)
        self._handler.setFormatter(
            logging.Formatter("%(asctime)s [%(name)s] %(message)s", datefmt="%H:%M:%S")
        )
        self._auto_scroll = True
        self._filter_text = ""
        self._active_workspace_id = ""  # set by host each frame
        self._level_filter: set[int] = {
            logging.DEBUG, logging.INFO, logging.WARNING, logging.ERROR, logging.CRITICAL
        }

    def install(self):
        """Attach handler to root logger."""
        logging.getLogger().addHandler(self._handler)

    def uninstall(self):
        """Remove handler from root logger."""
        logging.getLogger().removeHandler(self._handler)

    def _append(self, level: int, text: str):
        self._lines.append((level, self._active_workspace_id, text))

    def draw(self):
        imgui.begin("Log")

        # Level filter toggle buttons (plain — no per-level button color)
        _dim_button = imgui.ImVec4(0.15, 0.15, 0.17, 1.0)
        _dim_hovered = imgui.ImVec4(0.20, 0.20, 0.22, 1.0)
        for label, level in self._LEVEL_BUTTONS:
            active = level in self._level_filter
            if not active:
                imgui.push_style_color(imgui.Col_.button, _dim_button)
                imgui.push_style_color(imgui.Col_.button_hovered, _dim_hovered)
            if imgui.button(label):
                if active:
                    self._level_filter.discard(level)
                else:
                    self._level_filter.add(level)
            if not active:
                imgui.pop_style_color(2)
            imgui.same_line()

        # Filter text input
        imgui.set_next_item_width(160)
        _, self._filter_text = imgui.input_text("##filter", self._filter_text, 256)
        imgui.same_line()
        if imgui.button("Clear"):
            self._lines.clear()
        imgui.same_line()
        _, self._auto_scroll = imgui.checkbox("Auto-scroll", self._auto_scroll)

        imgui.separator()

        # Scrollable log region
        imgui.begin_child("log_scroll", imgui.ImVec2(0, 0))

        filt = self._filter_text.lower()
        for level, ws_id, text in list(self._lines):
            if level not in self._level_filter:
                continue
            if filt and filt not in text.lower():
                continue
            color = self._LEVEL_COLORS.get(level, self._LEVEL_COLORS[logging.INFO])
            imgui.text_colored(color, text)

        if self._auto_scroll and imgui.get_scroll_y() >= imgui.get_scroll_max_y() - 10:
            imgui.set_scroll_here_y(1.0)

        imgui.end_child()
        imgui.end()


class _LogHandler(logging.Handler):
    """Logging handler that pushes records into LogPanel._lines."""

    def __init__(self, panel: LogPanel):
        super().__init__()
        self._panel = panel

    def emit(self, record):
        try:
            msg = self.format(record)
            self._panel._append(record.levelno, msg)
        except Exception:
            pass
