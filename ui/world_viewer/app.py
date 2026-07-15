from __future__ import annotations

from pathlib import Path
from typing import Any

from imgui_bundle import imgui

from ui.world_viewer.state import CellBounds, WorldViewerState


class WorldViewerApp:
    def __init__(self, toolkit_settings=None):
        self.state = WorldViewerState()
        self._toolkit_settings = toolkit_settings
        self._session: Any = None
        self._scene: Any = None
        self.active = False

    def apply_settings(self, settings: dict) -> None:
        self.state.game = str(settings.get("game", self.state.game) or "fo4")
        self.state.plugin_paths = list(settings.get("plugin_paths", self.state.plugin_paths) or [])
        self.state.data_paths = list(settings.get("data_paths", self.state.data_paths) or [])
        self.state.archive_paths = list(settings.get("archive_paths", self.state.archive_paths) or [])
        self.state.worldspace = str(settings.get("worldspace", self.state.worldspace) or "")
        self.state.bounds = CellBounds(
            int(settings.get("min_x", getattr(self.state.bounds, "min_x", -1))),
            int(settings.get("min_y", getattr(self.state.bounds, "min_y", -1))),
            int(settings.get("max_x", getattr(self.state.bounds, "max_x", 1))),
            int(settings.get("max_y", getattr(self.state.bounds, "max_y", 1))),
        )

    def collect_settings(self) -> dict:
        return {
            "game": self.state.game,
            "plugin_paths": list(self.state.plugin_paths),
            "data_paths": list(self.state.data_paths),
            "archive_paths": list(self.state.archive_paths),
            "worldspace": self.state.worldspace,
            "min_x": getattr(self.state.bounds, "min_x", -1),
            "min_y": getattr(self.state.bounds, "min_y", -1),
            "max_x": getattr(self.state.bounds, "max_x", 1),
            "max_y": getattr(self.state.bounds, "max_y", 1),
        }

    def resolve_game_paths(self) -> dict:
        if self._toolkit_settings is None:
            return {}
        return dict(self._toolkit_settings.get_game_paths(self.state.game))

    def load_worldspaces(self) -> list[str]:
        try:
            session = self._ensure_session()
            report = session.list_worldspaces()
            self.state.last_report = report
            self.state.worldspaces = self._extract_worldspaces(report)
            self._store_report_warnings(report)
            return list(self.state.worldspaces)
        except Exception as exc:
            self._record_error(exc)
            return []

    def load_scene(self):
        if not self.state.worldspace:
            self._record_error(ValueError("worldspace is required"))
            return self._scene

        previous_session = self._session
        previous_scene = self._scene
        session = None
        try:
            session = self._create_session()
            scene = session.load_worldspace(
                self.state.worldspace,
                self.state.bounds,
                self.state.settings,
            )
            stats_report = scene.stats()
        except Exception as exc:
            self._record_error(exc)
            if session is not None and session is not previous_session:
                self._close_session(session)
            self._session = previous_session
            self._scene = previous_scene
            return previous_scene

        self._session = session
        self._scene = scene
        self.state.stats_report = stats_report
        self.state.last_report = stats_report
        self.state.error_message = ""
        self._store_report_warnings(stats_report)
        self._close_scene(previous_scene)
        self._close_session(previous_session)
        return scene

    def query_visible(self, camera: object | None = None):
        if self._scene is None:
            return None
        try:
            report = self._scene.query_visible(camera or {}, self.state.settings)
            self.state.visible_report = report
            self.state.last_report = report
            self._store_report_warnings(report)
            return report
        except Exception as exc:
            self._record_error(exc)
            return None

    def viewport_summary(self) -> dict[str, Any]:
        report = self.query_visible() if self._scene is not None else self.state.visible_report
        if report is None:
            report = self.state.stats_report or self.state.last_report
        data = self._report_value(report, "data", {}) if report is not None else {}
        counts = self._report_value(report, "counts", {}) if report is not None else {}
        timings = self._report_value(report, "timings_ms", {}) if report is not None else {}
        batches = data.get("batches", []) if isinstance(data, dict) else []
        return {
            "loaded": self._scene is not None,
            "worldspace": self.state.worldspace,
            "bounds": self.state.bounds,
            "counts": counts if isinstance(counts, dict) else {},
            "timings_ms": timings if isinstance(timings, dict) else {},
            "batches": batches if isinstance(batches, list) else [],
            "error_message": self.state.error_message,
        }

    def select_instance(self, instance_id: int | None):
        self.state.selected_instance_id = instance_id
        if self._scene is None or instance_id is None:
            self.state.selected_report = None
            return None
        try:
            report = self._scene.inspect_instance(instance_id)
            self.state.selected_report = report
            self.state.last_report = report
            self._store_report_warnings(report)
            return report
        except Exception as exc:
            self._record_error(exc)
            return None

    def render_current_view(self, output_path: str | Path | None = None):
        if self._scene is None:
            self._record_error(RuntimeError("no world scene is loaded"))
            return None
        try:
            api = self._world_renderer_api()
            job = api["OfflineRenderJob"](
                output_path=str(output_path or self.state.render_output_path),
                width=max(1, int(self.state.render_width)),
                height=max(1, int(self.state.render_height)),
            )
            render_fn = api["render_world_scene_offscreen"]
            report = render_fn(self._scene, job)
            self.state.last_report = report
            self._store_report_warnings(report)
            return report
        except Exception as exc:
            self._record_error(exc)
            return None

    def draw(self) -> None:
        from ui.world_viewer.panels import (
            layers_panel,
            render_panel,
            selection_panel,
            stats_panel,
            world_panel,
        )

        world_panel.draw(self)
        imgui.separator()
        layers_panel.draw(self)
        imgui.separator()
        render_panel.draw(self)
        imgui.separator()
        selection_panel.draw(self)
        imgui.separator()
        stats_panel.draw(self)

    def cleanup(self) -> None:
        self._close_scene(self._scene)
        self._close_session(self._session)
        self._scene = None
        self._session = None

    def _ensure_session(self):
        if self._session is None:
            self._session = self._create_session()
        return self._session

    def _create_session(self):
        api = self._world_renderer_api()
        paths = self.resolve_game_paths()
        data_paths = self._resolved_data_paths(paths)
        archive_paths = list(self.state.archive_paths)
        return api["WorldSceneBuilder"](
            game=self.state.game,
            plugin_paths=list(self.state.plugin_paths),
            data_paths=data_paths,
            archive_paths=archive_paths,
        ).open()

    def _resolved_data_paths(self, paths: dict) -> list[str]:
        data_paths = list(self.state.data_paths)
        for key in ("data", "data_dir", "extracted_dir"):
            path = paths.get(key)
            if path and path not in data_paths:
                data_paths.append(path)
        root_dir = paths.get("root_dir") or paths.get("game_root")
        if root_dir:
            data_dir = str(Path(root_dir) / "Data")
            if data_dir not in data_paths:
                data_paths.append(data_dir)
        return data_paths

    def _world_renderer_api(self) -> dict[str, Any]:
        try:
            from creation_lib.world_renderer import OfflineRenderJob, WorldSceneBuilder
            from creation_lib.renderer.world_offscreen import render_world_scene_offscreen
        except ImportError as exc:
            raise RuntimeError("creation_lib.world_renderer is not available") from exc
        return {
            "OfflineRenderJob": OfflineRenderJob,
            "WorldSceneBuilder": WorldSceneBuilder,
            "render_world_scene_offscreen": render_world_scene_offscreen,
        }

    def _record_error(self, exc: Exception) -> None:
        self.state.error_message = str(exc)
        self.state.warnings.append({"code": "world_viewer_error", "message": str(exc)})

    def _store_report_warnings(self, report: object) -> None:
        warnings = self._report_value(report, "warnings", [])
        if isinstance(warnings, list):
            self.state.warnings = warnings

    def _extract_worldspaces(self, report: object) -> list[str]:
        data = self._report_value(report, "data", {})
        worldspaces = data.get("worldspaces", []) if isinstance(data, dict) else []
        result: list[str] = []
        for item in worldspaces:
            if isinstance(item, str):
                result.append(item)
            elif isinstance(item, dict):
                result.append(str(item.get("editor_id") or item.get("id") or item.get("name") or ""))
        return [item for item in result if item]

    def _report_value(self, report: object, name: str, default: Any) -> Any:
        if isinstance(report, dict):
            return report.get(name, default)
        return getattr(report, name, default)

    def _close_scene(self, scene: object | None) -> None:
        close = getattr(scene, "close", None)
        if callable(close):
            close()

    def _close_session(self, session: object | None) -> None:
        close = getattr(session, "close", None)
        if callable(close):
            close()
