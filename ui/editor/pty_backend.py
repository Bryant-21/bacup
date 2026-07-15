"""PTY backend — spawns a terminal process and feeds output through pyte VT emulator.

Isolates all PTY management and terminal emulation from the UI layer.
Uses pywinpty for Windows PTY support and pyte for VT100/xterm parsing.
"""

import logging
import threading
from collections import deque

import pyte

_log = logging.getLogger("nif_editor.pty_backend")


class PtyBackend:
    """Manages a PTY process with pyte-based terminal emulation.

    Usage:
        pty = PtyBackend()
        pty.start("claude", cwd=".", cols=80, rows=24)
        # Each frame:
        pty.drain_and_update()
        screen = pty.screen  # pyte.Screen with char grid + cursor
    """

    def __init__(self):
        self._process = None
        self._read_thread = None
        self._raw_buffer = deque()  # thread-safe byte chunks from PTY
        self._stream = None
        self._screen = None
        self._lock = threading.Lock()
        self._stop_event = threading.Event()

    @property
    def screen(self) -> pyte.Screen | None:
        """The pyte Screen object (char grid + cursor). Only access from main thread."""
        return self._screen

    @property
    def is_alive(self) -> bool:
        """Whether the PTY process is still running."""
        if self._process is None:
            return False
        return self._process.isalive()

    def start(self, cmd: str, cwd: str = ".", cols: int = 80, rows: int = 24):
        """Spawn a PTY process.

        Args:
            cmd: Command to run (e.g. "claude" or "opencode").
            cwd: Working directory for the process.
            cols: Terminal width in columns.
            rows: Terminal height in rows.
        """
        self.stop()

        # Initialize pyte screen + stream
        self._screen = pyte.Screen(cols, rows)
        self._screen.set_mode(pyte.modes.LNM)  # auto newline mode
        self._stream = pyte.Stream(self._screen)

        self._stop_event.clear()
        self._raw_buffer.clear()

        try:
            from winpty import PtyProcess
            self._process = PtyProcess.spawn(
                cmd,
                cwd=cwd,
                dimensions=(rows, cols),
            )
            _log.info("PTY started: %s (cols=%d, rows=%d)", cmd, cols, rows)
        except Exception as e:
            _log.error("Failed to start PTY: %s", e)
            self._process = None
            return

        # Start daemon read thread
        self._read_thread = threading.Thread(
            target=self._read_loop, daemon=True, name="pty-reader"
        )
        self._read_thread.start()

    def _read_loop(self):
        """Background thread: read raw bytes from PTY into deque."""
        while not self._stop_event.is_set():
            try:
                if self._process is None or not self._process.isalive():
                    break
                data = self._process.read(4096)
                if data:
                    self._raw_buffer.append(data)
            except EOFError:
                break
            except Exception as e:
                if not self._stop_event.is_set():
                    _log.debug("PTY read error: %s", e)
                break
        _log.debug("PTY read loop ended")

    def drain_and_update(self):
        """Drain raw buffer and feed into pyte. Call from main thread each frame."""
        if self._stream is None:
            return

        # Drain all pending chunks
        chunks = []
        while self._raw_buffer:
            try:
                chunks.append(self._raw_buffer.popleft())
            except IndexError:
                break

        if not chunks:
            return

        # Feed into pyte (main thread only)
        combined = "".join(chunks)
        try:
            self._stream.feed(combined)
        except Exception as e:
            _log.debug("pyte feed error: %s", e)

    def write(self, data: str):
        """Write data to the PTY stdin."""
        if self._process and self._process.isalive():
            try:
                self._process.write(data)
            except Exception as e:
                _log.debug("PTY write error: %s", e)

    def resize(self, cols: int, rows: int):
        """Resize the PTY and pyte screen."""
        if cols < 1 or rows < 1:
            return
        if self._process and self._process.isalive():
            try:
                self._process.setwinsize(rows, cols)
            except Exception as e:
                _log.debug("PTY resize error: %s", e)
        if self._screen:
            self._screen.resize(rows, cols)

    def stop(self):
        """Stop the PTY process and clean up."""
        self._stop_event.set()

        if self._process:
            try:
                if self._process.isalive():
                    self._process.write("\x03")  # Ctrl+C
                    self._process.write("\x04")  # Ctrl+D
            except Exception:
                pass
            try:
                if self._process.isalive():
                    self._process.terminate()
            except Exception:
                pass
            self._process = None

        if self._read_thread and self._read_thread.is_alive():
            self._read_thread.join(timeout=2)
        self._read_thread = None

        self._raw_buffer.clear()
        self._stream = None
        self._screen = None
        _log.debug("PTY stopped")
