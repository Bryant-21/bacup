"""Filter builder panel — effect card stack, parametric EQ, VST3 browser."""
from __future__ import annotations

import logging
import math
import os
import sys
import threading
import time
from pathlib import Path as _Path

from app.paths import get_app_root as _get_app_root
from typing import Any

import numpy as np
from imgui_bundle import imgui, implot

_log = logging.getLogger("toolkit.voice_changer.filter_builder")
_NS = "##voice_changer"

# Effect categories for the "Add Effect" dropdown
_EFFECT_CATEGORIES = {
    "Filters": [
        ("HighpassFilter", {"cutoff_frequency_hz": 300.0, "rolloff_db_per_oct": 12.0}),
        ("LowpassFilter", {"cutoff_frequency_hz": 4000.0, "rolloff_db_per_oct": 12.0}),
        ("PeakFilter", {"cutoff_frequency_hz": 1000.0, "gain_db": 0.0, "q": 1.0}),
        ("LowShelfFilter", {"cutoff_frequency_hz": 200.0, "gain_db": 0.0, "q": 0.707}),
        ("HighShelfFilter", {"cutoff_frequency_hz": 8000.0, "gain_db": 0.0, "q": 0.707}),
        ("LadderFilter", {"cutoff_hz": 1000.0, "resonance": 0.0, "drive": 1.0}),
    ],
    "Dynamics": [
        ("Compressor", {"threshold_db": -20.0, "ratio": 4.0, "attack_ms": 1.0, "release_ms": 100.0}),
        ("Limiter", {"threshold_db": -1.0, "release_ms": 100.0}),
        ("NoiseGate", {"threshold_db": -40.0, "ratio": 10.0, "attack_ms": 1.0, "release_ms": 100.0}),
        ("Gain", {"gain_db": 0.0}),
    ],
    "Modulation": [
        ("PitchShift", {"semitones": 0.0}),
        ("Chorus", {"rate_hz": 1.0, "depth": 0.25, "centre_delay_ms": 7.0, "feedback": 0.0, "mix": 0.5}),
        ("Phaser", {"rate_hz": 1.0, "depth": 0.5, "feedback": 0.0, "mix": 0.5}),
        ("Tremolo", {"frequency_hz": 50.0, "wet_level": 0.45}),
        ("Delay", {"delay_seconds": 0.25, "feedback": 0.0, "mix": 0.5}),
        ("Reverb", {"room_size": 0.5, "damping": 0.5, "wet_level": 0.33, "dry_level": 0.4, "width": 1.0, "freeze_mode": 0.0}),
    ],
    "Distortion": [
        ("Distortion", {"drive_db": 5.0}),
        ("Clipping", {"threshold_db": -6.0}),
        ("Bitcrush", {"bit_depth": 8.0}),
    ],
    "Custom": [
        ("CombFilter", {"delay_seconds": 0.015, "decay": 0.6}),
        ("WhiteNoiseMix", {"amplitude": 0.05}),
        ("MP3Compressor", {"vbr_quality": 2.0}),
        ("GSMFullRateCompressor", {}),
    ],
}

# Parameter display config: (min, max, format)
_PARAM_RANGES: dict[str, tuple[float, float, str]] = {
    "cutoff_frequency_hz": (20.0, 20000.0, "%.0f Hz"),
    "rolloff_db_per_oct": (6.0, 48.0, "%.0f dB/oct"),
    "cutoff_hz": (20.0, 20000.0, "%.0f Hz"),
    "gain_db": (-24.0, 24.0, "%.1f dB"),
    "q": (0.1, 10.0, "%.2f"),
    "threshold_db": (-60.0, 0.0, "%.1f dB"),
    "ratio": (1.0, 20.0, "%.1f:1"),
    "attack_ms": (0.1, 100.0, "%.1f ms"),
    "release_ms": (1.0, 1000.0, "%.0f ms"),
    "drive_db": (0.0, 40.0, "%.1f dB"),
    "drive": (0.0, 10.0, "%.2f"),
    "rate_hz": (0.1, 100.0, "%.1f Hz"),
    "depth": (0.0, 1.0, "%.2f"),
    "centre_delay_ms": (0.1, 50.0, "%.1f ms"),
    "feedback": (0.0, 1.0, "%.2f"),
    "mix": (0.0, 1.0, "%.2f"),
    "delay_seconds": (0.001, 2.0, "%.3f s"),
    "decay": (0.0, 1.0, "%.2f"),
    "room_size": (0.0, 1.0, "%.2f"),
    "damping": (0.0, 1.0, "%.2f"),
    "wet_level": (0.0, 1.0, "%.2f"),
    "dry_level": (0.0, 1.0, "%.2f"),
    "resonance": (0.0, 1.0, "%.2f"),
    "frequency_hz": (1.0, 100.0, "%.1f Hz"),
    "amplitude": (0.0, 0.05, "%.4f"),
    "semitones": (-24.0, 24.0, "%.1f st"),
    "bit_depth": (1.0, 32.0, "%.1f bit"),
    "vbr_quality": (0.0, 10.0, "%.1f"),
    "width": (0.0, 1.0, "%.2f"),
    "freeze_mode": (0.0, 1.0, "%.2f"),
}


class FilterBuilderPanel:
    """Right dock panel — effect card stack with full parameter controls."""

    def __init__(self, app):
        self._app = app
        self.window_name = f"Filter Builder{_NS}"
        self._drag_source: int | None = None
        self._eq_bands: list[dict] = []  # [{freq, gain, q}, ...]
        self._editor_procs: list = []  # track subprocess.Popen instances
        self._expanded: dict[str, bool] = {}  # persisted open/close state
        self._seen_keys: set[str] = set()  # keys whose initial state has been applied

    def cleanup(self):
        """Kill any open editor subprocesses."""
        for proc in self._editor_procs:
            try:
                proc.terminate()
            except OSError:
                pass
        self._editor_procs.clear()

    def collect_settings(self) -> dict:
        return {"filter_builder_expanded": dict(self._expanded)}

    def restore_settings(self, expanded: dict) -> None:
        self._expanded = dict(expanded)
        self._seen_keys.clear()

    def _tree_node_tracked(self, key: str, label: str, flags: int) -> bool:
        """Tree node that defaults to closed and persists open/close state."""
        if key not in self._seen_keys:
            self._seen_keys.add(key)
            imgui.set_next_item_open(self._expanded.get(key, False), imgui.Cond_.always.value)
        is_open = imgui.tree_node_ex(label, flags)
        self._expanded[key] = is_open
        return is_open

    def draw(self):
        if self._app.focus_filter_builder:
            imgui.set_next_window_focus()
            self._app.focus_filter_builder = False
        imgui.begin(self.window_name)

        # -- Preset preview banner --
        previewing = self._app.preview_preset_slug is not None
        if previewing:
            self._draw_preview_section()
            imgui.separator()

        # -- Add Effect dropdown --
        if imgui.button("+ Add Effect"):
            imgui.open_popup("add_effect_popup")

        # Collapse / Expand all
        groups = self._app.active_chain_groups
        if groups:
            imgui.same_line()
            if imgui.small_button("Expand All"):
                for gname, _, _ in groups:
                    imgui.get_state_storage().set_int(
                        imgui.get_id(f"##grp_{gname}"), 1
                    )
            imgui.same_line()
            if imgui.small_button("Collapse All"):
                for gname, _, _ in groups:
                    imgui.get_state_storage().set_int(
                        imgui.get_id(f"##grp_{gname}"), 0
                    )

        self._draw_add_effect_popup()

        imgui.separator()

        # -- Effect card stack (grouped by preset) --
        chain = self._app.active_chain
        delete_idx = None
        swap_pair = None

        if groups:
            # Build a set of indices covered by groups
            grouped_indices: set[int] = set()
            for _, start, count in groups:
                for j in range(start, start + count):
                    grouped_indices.add(j)

            # Render each preset group
            for gname, start, count in groups:
                header_flags = (
                    imgui.TreeNodeFlags_.framed.value
                    | imgui.TreeNodeFlags_.allow_overlap.value
                )
                header_open = self._tree_node_tracked(f"grp_{gname}", f"{gname} ({count})##grp_{gname}", header_flags)
                if header_open:
                    for i in range(start, start + count):
                        if i >= len(chain):
                            break
                        imgui.push_id(i)
                        card_swap, card_delete = self._draw_effect_card(i, chain[i])
                        if card_swap is not None:
                            # Only allow swaps within this group
                            s, d = card_swap
                            if start <= s < start + count and start <= d < start + count:
                                swap_pair = card_swap
                        if card_delete:
                            delete_idx = i
                        imgui.pop_id()
                        imgui.spacing()
                    imgui.tree_pop()

            # Render any effects not in a group (manually added)
            ungrouped = [i for i in range(len(chain)) if i not in grouped_indices]
            if ungrouped:
                imgui.spacing()
                imgui.text_disabled("Custom Effects")
                imgui.separator()
                for i in ungrouped:
                    imgui.push_id(i)
                    card_swap, card_delete = self._draw_effect_card(i, chain[i])
                    if card_swap is not None:
                        swap_pair = card_swap
                    if card_delete:
                        delete_idx = i
                    imgui.pop_id()
                    imgui.spacing()
        else:
            # No groups — flat list (no presets active, or manually built chain)
            for i, node in enumerate(chain):
                imgui.push_id(i)
                card_swap, card_delete = self._draw_effect_card(i, node)
                if card_swap is not None:
                    swap_pair = card_swap
                if card_delete:
                    delete_idx = i
                imgui.pop_id()
                imgui.spacing()

        # Apply deferred mutations
        if swap_pair:
            s, d = swap_pair
            chain[s], chain[d] = chain[d], chain[s]

        if delete_idx is not None and 0 <= delete_idx < len(chain):
            chain.pop(delete_idx)

        imgui.end()

    def _draw_preview_section(self):
        """Draw the preset preview banner and read-only effect list."""
        slug = self._app.preview_preset_slug
        name = self._app.preview_preset_name
        chain = self._app.preview_chain

        # Header with enable/close buttons
        imgui.text_colored(imgui.ImVec4(0.5, 0.7, 1.0, 1.0), f"Preview: {name}")
        imgui.same_line()
        if imgui.small_button("Enable##preview"):
            self._app.add_preset(slug)
            return
        imgui.same_line()
        if imgui.small_button("X##close_preview"):
            self._app.clear_preview()
            return

        # Show effects as a compact read-only list
        imgui.begin_disabled()
        for i, node in enumerate(chain):
            effect_type = node.get("type", "?")
            display = effect_type
            if effect_type == "VST3":
                plugin_path = node.get("plugin_path", "")
                display = os.path.splitext(os.path.basename(plugin_path))[0] or "VST3"
            enabled = node.get("enabled", True)
            params = node.get("params", {})

            flags = imgui.TreeNodeFlags_.framed.value
            if imgui.tree_node_ex(f"{display}##preview_{i}", flags):
                # Show params as text (read-only since we're in begin_disabled)
                for key, value in params.items():
                    label = key.replace("_", " ").title()
                    imgui.text(f"  {label}: {value:.3g}" if isinstance(value, float) else f"  {label}: {value}")
                imgui.tree_pop()
        imgui.end_disabled()

    def _draw_effect_card(self, idx: int, node: dict):
        """Draw a single effect card with enable/disable, parameters, and delete.

        Returns (swap_pair, should_delete): swap_pair is (src, dst) if reorder
        occurred else None; should_delete is True if the X button was clicked.
        """
        effect_type = node["type"]
        enabled = node.get("enabled", True)
        params = node.get("params", {})
        swap_pair = None
        should_delete = False

        # Card header — allow_overlap lets the X button capture its own click even
        # though the framed tree node header covers the same row width.
        flags = (
            imgui.TreeNodeFlags_.framed.value
            | imgui.TreeNodeFlags_.allow_overlap.value
        )

        # Resolve display name (VST3 nodes all have type "VST3"; show plugin name instead)
        display_name = effect_type
        if effect_type == "VST3":
            plugin_path = node.get("plugin_path", "")
            for p in self._app.vst3_plugins:
                if p.path == plugin_path:
                    display_name = p.name
                    break
            else:
                display_name = os.path.basename(plugin_path) or "VST3"

        # Checkbox for enable/disable
        changed_en, new_enabled = imgui.checkbox(f"##en_{idx}", enabled)
        if changed_en:
            node["enabled"] = new_enabled
        imgui.same_line()

        expanded = self._tree_node_tracked(f"card_{idx}", f"{display_name}##card_{idx}", flags)

        # Drag reordering (must be right after tree_node_ex, on its item)
        if imgui.begin_drag_drop_source():
            imgui.set_drag_drop_payload("effect_idx", idx.to_bytes(4, "little"))
            imgui.text(effect_type)
            imgui.end_drag_drop_source()
        if imgui.begin_drag_drop_target():
            payload = imgui.accept_drag_drop_payload("effect_idx")
            if payload is not None:
                src_idx = int.from_bytes(payload.data[:4], "little")
                if src_idx != idx:
                    swap_pair = (src_idx, idx)
            imgui.end_drag_drop_target()

        # Delete button (right-aligned, account for scrollbar)
        imgui.same_line(imgui.get_content_region_avail().x + imgui.get_cursor_pos_x() - 24)
        if imgui.small_button(f"X##del_{idx}"):
            should_delete = True

        if expanded:
            if not enabled:
                imgui.begin_disabled()

            if effect_type == "ParametricEQ":
                self._draw_parametric_eq(node)
            elif node.get("type") == "VST3":
                self._draw_vst3_params(node)
            else:
                self._draw_standard_params(idx, params)

            if not enabled:
                imgui.end_disabled()
            imgui.tree_pop()

        return swap_pair, should_delete

    def _draw_standard_params(self, idx: int, params: dict):
        """Draw sliders for each parameter in the effect."""
        tbl_flags = imgui.TableFlags_.sizing_fixed_fit | imgui.TableFlags_.no_borders_in_body
        if imgui.begin_table(f"##params_{idx}", 2, tbl_flags):
            imgui.table_setup_column("##lbl", imgui.TableColumnFlags_.width_fixed, 90)
            imgui.table_setup_column("##val", imgui.TableColumnFlags_.width_stretch)
            for key, value in list(params.items()):
                lo, hi, fmt = _PARAM_RANGES.get(key, (0.0, 1.0, "%.3f"))
                label = key.replace("_", " ").title()
                imgui.table_next_row()
                imgui.table_set_column_index(0)
                imgui.align_text_to_frame_padding()
                imgui.text(label)
                imgui.table_set_column_index(1)
                imgui.set_next_item_width(-1)
                changed, new_val = imgui.slider_float(
                    f"##param_{idx}_{key}", float(value), lo, hi, fmt
                )
                if changed:
                    params[key] = new_val
            imgui.end_table()

    def _draw_vst3_params(self, node: dict):
        """Draw VST3 plugin card with native editor button + parameter sliders."""
        params = node.get("params", {})
        plugin_path = node.get("plugin_path", "")
        backend = node.get("backend", "pedalboard")
        imgui.text_disabled(f"Plugin: {plugin_path}")

        # Find the plugin info
        info = None
        for p in self._app.vst3_plugins:
            if p.path == plugin_path:
                info = p
                break

        if info is None:
            imgui.text_colored(imgui.ImVec4(1, 0.3, 0.3, 1), "Plugin not found")
            return

        # "Open Editor" button — launches the plugin's native GUI
        editor_key = f"_editor_{plugin_path}"
        editor_open = getattr(self, editor_key, False)
        if editor_open:
            imgui.begin_disabled()
        if imgui.button(f"Open Editor##vst_editor"):
            self._open_plugin_editor(plugin_path, backend, params, editor_key)
        if editor_open:
            imgui.end_disabled()
            imgui.same_line()
            imgui.text_disabled("(editor open)")

        imgui.separator()

        # Parameter sliders (collapsed by default for plugins with many params)
        num_params = len(info.parameters)
        header_label = f"Parameters ({num_params})##vst_param_header"
        flags = 0 if num_params <= 20 else imgui.TreeNodeFlags_.none.value
        if num_params > 20:
            show_params = imgui.collapsing_header(header_label)
        else:
            show_params = True
            imgui.text(f"Parameters ({num_params})")

        if show_params:
            tbl_flags = imgui.TableFlags_.sizing_fixed_fit | imgui.TableFlags_.no_borders_in_body
            if imgui.begin_table("##vst_params", 2, tbl_flags):
                imgui.table_setup_column("##lbl", imgui.TableColumnFlags_.width_fixed, 90)
                imgui.table_setup_column("##val", imgui.TableColumnFlags_.width_stretch)
                for param_name, meta in info.parameters.items():
                    label = param_name.replace("_", " ").title()
                    imgui.table_next_row()
                    imgui.table_set_column_index(0)
                    imgui.align_text_to_frame_padding()
                    imgui.text(label)
                    imgui.table_set_column_index(1)
                    imgui.set_next_item_width(-1)

                    valid_values = meta.get("valid_values")
                    if valid_values:
                        # String enum — combo dropdown
                        current_str = str(params.get(param_name, meta.get("default", valid_values[0])))
                        current_idx = 0
                        for vi, vv in enumerate(valid_values):
                            if vv == current_str:
                                current_idx = vi
                                break
                        changed, new_idx = imgui.combo(
                            f"##vst_{param_name}", current_idx, valid_values,
                        )
                        if changed:
                            params[param_name] = valid_values[new_idx]
                    else:
                        # Float slider (covers stepped floats and continuous)
                        lo = meta.get("min", 0.0)
                        hi = meta.get("max", 1.0)
                        current = params.get(param_name, meta.get("default", 0.0))
                        try:
                            current_f = float(current)
                        except (TypeError, ValueError):
                            current_f = 0.0
                        changed, new_val = imgui.slider_float(
                            f"##vst_{param_name}", current_f, float(lo), float(hi)
                        )
                        if changed:
                            params[param_name] = new_val
                imgui.end_table()

    def _open_plugin_editor(
        self, plugin_path: str, backend: str, params: dict, editor_key: str,
    ):
        """Open the plugin's native GUI in a subprocess.

        Both pedalboard and DawDreamer require the editor window on the main
        thread, which we can't block (ImGui is drawing on it).  We spawn a
        separate Python process whose main thread loads the plugin and opens
        the editor.  Current param values are sent via a temp JSON file;
        updated values are written back when the editor closes.
        """
        import json
        import subprocess
        import tempfile

        setattr(self, editor_key, True)

        # Write current params to a temp file the subprocess will read
        tmp = tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False, prefix="vst3_editor_",
        )
        json.dump({"plugin_path": plugin_path, "backend": backend, "params": params}, tmp)
        tmp.close()
        tmp_path = tmp.name

        proc = subprocess.Popen(
            [sys.executable, "-m", "ui.voice_changer._plugin_editor_host", tmp_path],
            cwd=str(_get_app_root()),
        )
        self._editor_procs.append(proc)

        def _sync_from_json():
            """Read the shared JSON and update params dict + cached instance."""
            try:
                with open(tmp_path, "r") as f:
                    updated = json.load(f)
                new_params = updated.get("params", {})
                changed = 0
                for k, v in new_params.items():
                    if k in params and params[k] != v:
                        params[k] = v
                        changed += 1
                if changed:
                    self._sync_params_to_cached_instance(plugin_path, params)
                return changed
            except (json.JSONDecodeError, FileNotFoundError, OSError):
                return 0

        def _run():
            try:
                # Poll the JSON for live param updates while editor is open
                deadline = time.monotonic() + 600
                while proc.poll() is None and time.monotonic() < deadline:
                    _sync_from_json()
                    time.sleep(0.25)

                if proc.returncode is None:
                    _log.warning("Plugin editor timed out: %s", plugin_path)
                    proc.kill()
                else:
                    _log.info("Plugin editor exited with code %d", proc.returncode)

                # Final sync after subprocess exits
                _sync_from_json()
            except Exception:
                _log.warning("Plugin editor error: %s", plugin_path, exc_info=True)
            finally:
                try:
                    os.unlink(tmp_path)
                except OSError:
                    pass
                if proc in self._editor_procs:
                    self._editor_procs.remove(proc)
                setattr(self, editor_key, False)

        threading.Thread(target=_run, daemon=True).start()

    def _sync_params_to_cached_instance(self, plugin_path: str, params: dict):
        """Push param values into the cached pedalboard plugin instance."""
        for info in self._app.vst3_plugins:
            if info.path == plugin_path and info._pedalboard_instance is not None:
                vst = info._pedalboard_instance
                for name, value in params.items():
                    if not hasattr(vst, name):
                        continue
                    try:
                        p = vst.parameters.get(name)
                        if p and getattr(p, "type", None) == bool:
                            setattr(vst, name, bool(value))
                        elif isinstance(value, str):
                            setattr(vst, name, value)
                        else:
                            setattr(vst, name, float(value))
                    except (ValueError, TypeError):
                        pass
                break

    def _draw_parametric_eq(self, node: dict):
        """Draw interactive parametric EQ with implot frequency response curve."""
        bands = node.get("bands", [])

        # Draw frequency response curve
        eq_height = 150
        if implot.begin_plot(
            "##eq_curve",
            size=imgui.ImVec2(-1, eq_height),
            flags=implot.Flags_.no_legend.value | implot.Flags_.no_title.value,
        ):
            implot.setup_axes("Frequency (Hz)", "Gain (dB)")
            implot.setup_axes_limits(
                math.log10(20), math.log10(20000), -24, 24, implot.Cond_.once.value
            )

            # Plot combined response curve
            freqs = np.logspace(np.log10(20), np.log10(20000), 500)
            response = np.zeros(500)
            for band in bands:
                f0 = band.get("freq", 1000)
                g = band.get("gain", 0)
                q = band.get("q", 1.0)
                # Simple bell curve approximation for visualization
                sigma = f0 / (q * 2)
                response += g * np.exp(-0.5 * ((freqs - f0) / sigma) ** 2)

            log_freqs = np.log10(freqs).astype(np.float32)
            response_f32 = response.astype(np.float32)
            implot.plot_line("##response", log_freqs, response_f32)

            # Draggable points for each band
            for bi, band in enumerate(bands):
                x = math.log10(band.get("freq", 1000))
                y = band.get("gain", 0.0)
                changed, new_x, new_y, *_ = implot.drag_point(
                    bi, x, y,
                    imgui.ImVec4(1, 0.5, 0.1, 1), 6.0,
                )
                if changed:
                    band["freq"] = max(20, min(20000, 10 ** new_x))
                    band["gain"] = max(-24, min(24, new_y))

            implot.end_plot()

        # Band sliders (3-col table: Freq | Gain+Q | X)
        tbl_flags = imgui.TableFlags_.sizing_stretch_same | imgui.TableFlags_.no_borders_in_body
        if imgui.begin_table("##eq_bands", 4, tbl_flags):
            imgui.table_setup_column("Freq", imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("Gain", imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("Q", imgui.TableColumnFlags_.width_fixed, 60)
            imgui.table_setup_column("##del", imgui.TableColumnFlags_.width_fixed, 24)
            remove_idx = None
            for bi, band in enumerate(bands):
                imgui.push_id(bi)
                imgui.table_next_row()
                imgui.table_set_column_index(0)
                imgui.set_next_item_width(-1)
                _, band["freq"] = imgui.slider_float("##freq", band.get("freq", 1000), 20.0, 20000.0, "%.0f Hz")
                imgui.table_set_column_index(1)
                imgui.set_next_item_width(-1)
                _, band["gain"] = imgui.slider_float("##gain", band.get("gain", 0), -24.0, 24.0, "%.1f dB")
                imgui.table_set_column_index(2)
                imgui.set_next_item_width(-1)
                _, band["q"] = imgui.slider_float("##q", band.get("q", 1.0), 0.1, 10.0, "%.1f")
                imgui.table_set_column_index(3)
                if imgui.small_button("X##band"):
                    remove_idx = bi
                imgui.pop_id()
            imgui.end_table()
            if remove_idx is not None:
                bands.pop(remove_idx)
                self._sync_eq_to_chain(node)

        if imgui.button("+ Add Band"):
            bands.append({"freq": 1000.0, "gain": 0.0, "q": 1.0})

        # Sync EQ bands to the actual chain as PeakFilter nodes
        node["bands"] = bands

    def _sync_eq_to_chain(self, node: dict):
        """No-op — the engine reads bands directly from the node (see engine._expand_chain), not through this method."""
        # The ParametricEQ node stores bands internally;
        # the engine processes them as individual PeakFilters during processing
        pass

    def _draw_add_effect_popup(self):
        """Draw the categorized "Add Effect" popup menu."""
        if imgui.begin_popup("add_effect_popup"):
            for category, effects in _EFFECT_CATEGORIES.items():
                if imgui.begin_menu(category):
                    for effect_type, default_params in effects:
                        if imgui.menu_item(effect_type, "", False)[0]:
                            self._app.active_chain.append({
                                "type": effect_type,
                                "enabled": True,
                                "params": dict(default_params),
                            })
                    imgui.end_menu()

            # Parametric EQ
            if imgui.menu_item("Parametric EQ", "", False)[0]:
                self._app.active_chain.append({
                    "type": "ParametricEQ",
                    "enabled": True,
                    "params": {},
                    "bands": [{"freq": 1000.0, "gain": 0.0, "q": 1.0}],
                })

            # VST3 plugins
            if self._app.vst3_plugins:
                if imgui.begin_menu("VST3 Plugins"):
                    for plugin in self._app.vst3_plugins:
                        if imgui.menu_item(plugin.name, "", False)[0]:
                            self._app.active_chain.append({
                                "type": "VST3",
                                "enabled": True,
                                "plugin_path": plugin.path,
                                "backend": plugin.backend,
                                "params": {
                                    name: meta.get("default", 0.0)
                                    for name, meta in plugin.parameters.items()
                                },
                            })
                    imgui.end_menu()

            imgui.end_popup()
