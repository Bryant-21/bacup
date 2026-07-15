"""Voice Changer app — holds effect engine, preset manager, and panels."""
from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Any

import numpy as np

from app.paths import get_app_root as _get_app_root
from ui.voice_changer.preset_manager import PresetManager
from ui.voice_changer.vst3_loader import scan_vst3_directory, VST3PluginInfo

_log = logging.getLogger("toolkit.voice_changer")

_BUILTIN_PRESETS_DIR = str(Path(__file__).resolve().parent / "presets")
_USER_PRESETS_DIR = str(_get_app_root() / "configs" / "voice_changer_presets")
_VST3_SYSTEM_DIR = r"C:\Program Files\Common Files\VST3"
_VST3_LOCAL_DIR = str(_get_app_root() / "configs" / "voice_changer_vst3")


class VoiceChangerApp:
    """Central app for the Voice Changer workspace."""

    def __init__(self, toolkit_settings=None):
        self.active = True
        self._first_frame = True
        self._panels_initialized = False
        self._toolkit_settings = toolkit_settings

        # Preset manager
        self.preset_manager = PresetManager(
            builtin_dir=_BUILTIN_PRESETS_DIR,
            user_dir=_USER_PRESETS_DIR,
        )

        # VST3 plugins
        self.vst3_plugins: list[VST3PluginInfo] = []

        # Shared state: stacked presets and merged effect chain
        self.active_chain: list[dict[str, Any]] = []
        self.active_chain_groups: list[tuple[str, int, int]] = []  # (name, start_idx, count)
        self.active_preset_slugs: list[str] = []
        self.active_preset_names: list[str] = []
        # Legacy single-preset accessors (for save-as, settings, etc.)
        self.active_preset_slug: str | None = None
        self.active_preset_name: str = "(none)"

        # Preview state: single-click preview in filter builder (not processed)
        self.preview_preset_slug: str | None = None
        self.preview_preset_name: str = ""
        self.preview_chain: list[dict[str, Any]] = []

        # Audio state (shared between recorder and bulk)
        self.original_audio: np.ndarray | None = None
        self.processed_audio: np.ndarray | None = None
        self.sample_rate: int = 44100

        # Log lines (shared output log)
        self.log_lines: list[str] = []

        # Signal to focus the filter builder on next frame (e.g. New Preset)
        self.focus_filter_builder: bool = False

        # Panels (created in _init_panels)
        self.recorder_panel = None
        self.bulk_panel = None
        self.filter_builder_panel = None
        self.preset_panel = None
        self.output_log_panel = None
        self.recordings_panel = None

    def setup(self):
        """Kick off async VST3 scan so startup doesn't block the UI thread."""
        import threading

        def _scan():
            plugins: list = []
            plugins.extend(scan_vst3_directory(_VST3_SYSTEM_DIR))
            plugins.extend(scan_vst3_directory(_VST3_LOCAL_DIR))
            self.vst3_plugins = plugins
            _log.info("Voice Changer setup complete: %d VST3 plugins", len(plugins))

        threading.Thread(target=_scan, daemon=True).start()

    def _init_panels(self):
        """Create panel instances."""
        if self._panels_initialized:
            return

        from ui.voice_changer.panels.recorder_panel import RecorderPanel
        from ui.voice_changer.panels.bulk_panel import BulkPanel
        from ui.voice_changer.panels.filter_builder_panel import FilterBuilderPanel
        from ui.voice_changer.panels.preset_panel import PresetPanel
        from ui.voice_changer.panels.output_log_panel import OutputLogPanel
        from ui.voice_changer.panels.recordings_panel import RecordingsPanel

        self.recorder_panel = RecorderPanel(self)
        self.bulk_panel = BulkPanel(self)
        self.filter_builder_panel = FilterBuilderPanel(self)
        self.preset_panel = PresetPanel(self)
        self.output_log_panel = OutputLogPanel(self)
        self.recordings_panel = RecordingsPanel(self)

        self._panels_initialized = True

    def preview_preset(self, slug: str) -> None:
        """Load a preset's chain for preview in the filter builder (no audio processing)."""
        if slug == self.preview_preset_slug:
            return  # already previewing
        data = self.preset_manager.load_preset(slug)
        if data:
            import copy
            self.preview_preset_slug = slug
            self.preview_preset_name = data.get("name", slug)
            self.preview_chain = copy.deepcopy(data.get("chain", []))

    def clear_preview(self) -> None:
        """Clear the preset preview."""
        self.preview_preset_slug = None
        self.preview_preset_name = ""
        self.preview_chain = []

    def toggle_preset(self, slug: str) -> None:
        """Add or remove a preset from the active stack."""
        if slug in self.active_preset_slugs:
            self.remove_preset(slug)
        else:
            self.add_preset(slug)

    def add_preset(self, slug: str) -> None:
        """Add a preset to the active stack."""
        if slug in self.active_preset_slugs:
            return
        data = self.preset_manager.load_preset(slug)
        if data:
            self.active_preset_slugs.append(slug)
            self.active_preset_names.append(data.get("name", slug))
            self._rebuild_chain()
            # Clear preview if this preset was being previewed
            if self.preview_preset_slug == slug:
                self.clear_preview()
            self.log(f"Enabled preset: {data.get('name', slug)}")
        else:
            self.log(f"Failed to load preset: {slug}")

    def remove_preset(self, slug: str) -> None:
        """Remove a preset from the active stack."""
        if slug not in self.active_preset_slugs:
            return
        idx = self.active_preset_slugs.index(slug)
        name = self.active_preset_names[idx]
        self.active_preset_slugs.pop(idx)
        self.active_preset_names.pop(idx)
        self._rebuild_chain()
        self.log(f"Removed preset: {name}")

    def move_preset(self, slug: str, direction: int) -> None:
        """Move a preset up (-1) or down (+1) in the active stack."""
        if slug not in self.active_preset_slugs:
            return
        idx = self.active_preset_slugs.index(slug)
        new_idx = idx + direction
        if new_idx < 0 or new_idx >= len(self.active_preset_slugs):
            return
        self.active_preset_slugs[idx], self.active_preset_slugs[new_idx] = (
            self.active_preset_slugs[new_idx], self.active_preset_slugs[idx])
        self.active_preset_names[idx], self.active_preset_names[new_idx] = (
            self.active_preset_names[new_idx], self.active_preset_names[idx])
        self._rebuild_chain()

    def move_preset_to(self, slug: str, target_idx: int) -> None:
        """Move a preset to a specific index in the active stack."""
        if slug not in self.active_preset_slugs:
            return
        idx = self.active_preset_slugs.index(slug)
        if idx == target_idx:
            return
        s = self.active_preset_slugs.pop(idx)
        n = self.active_preset_names.pop(idx)
        self.active_preset_slugs.insert(target_idx, s)
        self.active_preset_names.insert(target_idx, n)
        self._rebuild_chain()

    def clear_all_presets(self) -> None:
        """Remove all active presets."""
        self.active_preset_slugs.clear()
        self.active_preset_names.clear()
        self.active_chain.clear()
        self.active_chain_groups.clear()
        self.active_preset_slug = None
        self.active_preset_name = "(none)"
        self.log("Cleared all presets")

    def _rebuild_chain(self) -> None:
        """Merge all active preset chains into a single effect chain."""
        merged: list[dict[str, Any]] = []
        groups: list[tuple[str, int, int]] = []
        for i, slug in enumerate(self.active_preset_slugs):
            data = self.preset_manager.load_preset(slug)
            if data:
                chain = data.get("chain", [])
                name = self.active_preset_names[i] if i < len(self.active_preset_names) else slug
                groups.append((name, len(merged), len(chain)))
                merged.extend(chain)
        self.active_chain = merged
        self.active_chain_groups = groups
        # Update legacy accessors
        if self.active_preset_slugs:
            self.active_preset_slug = self.active_preset_slugs[-1]
            self.active_preset_name = self.active_preset_names[-1]
        else:
            self.active_preset_slug = None
            self.active_preset_name = "(none)"

    def load_preset(self, slug: str) -> None:
        """Replace all presets with a single one."""
        self.active_preset_slugs.clear()
        self.active_preset_names.clear()
        self.add_preset(slug)

    def process_audio(self, audio: np.ndarray, sample_rate: int) -> np.ndarray:
        """Process audio through the active effect chain."""
        from ui.voice_changer.engine import process_chain
        return process_chain(
            audio, sample_rate=sample_rate, chain=self.active_chain,
            normalize=True, vst3_plugins=self.vst3_plugins,
        )

    def log(self, message: str) -> None:
        """Append a message to the shared output log."""
        self.log_lines.append(message)
        _log.info(message)
