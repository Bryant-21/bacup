from __future__ import annotations

import queue
import threading
from pathlib import Path
from typing import Any

from creation_lib.lod import generate_lod
from ui.lodgen.state import LodgenState, settings_to_json


class LodgenApp:
    def __init__(self, toolkit_settings=None):
        self.state = LodgenState()
        self._toolkit_settings = toolkit_settings
        self._queue: queue.Queue[tuple] = queue.Queue()
        self._thread: threading.Thread | None = None
        self.active = False

    def resolve_game_paths(self) -> dict:
        if self._toolkit_settings is None:
            return {}
        return dict(self._toolkit_settings.get_game_paths(self.state.game))

    def data_dirs(self) -> list[str]:
        paths = self.resolve_game_paths()
        dirs: list[str] = []
        ext = paths.get("extracted_dir")
        if ext:
            dirs.append(ext)
        root = paths.get("root_dir")
        if root:
            dirs.append(str(Path(root) / "Data"))
        for extra in paths.get("additional_paths", []) or []:
            if extra:
                dirs.append(extra)
        return dirs

    def load_worldspaces(self) -> list[str]:
        try:
            from creation_lib.world_renderer import WorldSceneBuilder  # type: ignore[import]
        except ImportError:
            self.state.error_message = "world enumeration unavailable; type the worldspace id"
            return []
        try:
            session = WorldSceneBuilder(
                game=self.state.game,
                plugin_paths=[],
                data_paths=self.data_dirs(),
                archive_paths=[],
            ).open()
            report = session.list_worldspaces()
            data = report.get("data", {}) if isinstance(report, dict) else getattr(report, "data", {})
            ws = data.get("worldspaces", []) if isinstance(data, dict) else []
            self.state.worldspaces = [
                w if isinstance(w, str) else str(w.get("editor_id") or w.get("id") or "")
                for w in ws
            ]
            self.state.worldspaces = [w for w in self.state.worldspaces if w]
            return list(self.state.worldspaces)
        except Exception as exc:  # noqa: BLE001
            self.state.error_message = str(exc)
            return []

    def apply_settings(self, settings: dict) -> None:
        self.state.game = str(settings.get("game", self.state.game) or "fo4")
        self.state.output_dir = str(settings.get("output_dir", self.state.output_dir) or "")
        ws = settings.get("worldspace")
        if ws:
            self.state.worldspace = str(ws)

    def collect_settings(self) -> dict:
        return {
            "game": self.state.game,
            "output_dir": self.state.output_dir,
            "worldspace": self.state.worldspace,
        }

    def start_generate(self) -> None:
        if self.state.running:
            return
        if not self.state.worldspace:
            self.state.error_message = "worldspace is required"
            return
        if not self.state.output_dir:
            self.state.error_message = "output directory is required"
            return
        self.state.error_message = ""
        self.state.running = True
        self.state.progress_frac = 0.0
        world = self.state.worldspace
        settings_json = settings_to_json(self.state)
        data_dirs = self.data_dirs()
        output_dir = self.state.output_dir
        q = self._queue

        def _work() -> None:
            try:
                result = generate_lod(
                    world,
                    settings_json,
                    data_dirs=data_dirs,
                    output_dir=output_dir,
                    progress=lambda m, f: q.put(("progress", m, f)),
                )
                q.put(("done", result))
            except Exception as exc:  # noqa: BLE001
                q.put(("error", exc))

        self._thread = threading.Thread(target=_work, daemon=True, name="lodgen-generate")
        self._thread.start()

    def poll(self) -> None:
        while True:
            try:
                item = self._queue.get_nowait()
            except queue.Empty:
                break
            kind = item[0]
            if kind == "progress":
                _, msg, frac = item
                self.state.progress_msg = msg
                self.state.progress_frac = float(frac)
                self.state.log_lines.append(f"{frac:.0%} {msg}")
            elif kind == "done":
                self.state.last_result = item[1]
                r = item[1]
                self.state.log_lines.append(
                    f"done: btr={r.btr} bto={r.bto} dds={r.dds} warnings={len(r.warnings)}"
                )
                self.state.running = False
            elif kind == "error":
                self.state.error_message = str(item[1])
                self.state.log_lines.append(f"ERROR: {item[1]}")
                self.state.running = False

    def cleanup(self) -> None:
        if self._thread is not None and self._thread.is_alive():
            self._thread.join(timeout=2.0)
