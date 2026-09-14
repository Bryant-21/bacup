"""Standalone Fallout 4 PC-to-PlayStation mod asset converter."""

from __future__ import annotations

import tempfile
from collections.abc import Callable, Sequence
from pathlib import Path

from creation_lib.ui.widgets import pick_file, pick_folder
from imgui_bundle import imgui

from ui.tools.animation.havok_converter import PS4_WORKERS, run_ps4_asset_batch
from ui.tools.base import BaseTool
from creation_lib.ui.widgets.forms import begin_form, draw_path_row, end_form

PLUGIN_SUFFIXES = frozenset({".esm", ".esp", ".esl"})


def discover_ba2_archives(plugin_path: str | Path) -> list[Path]:
    plugin = Path(plugin_path)
    if plugin.suffix.lower() not in PLUGIN_SUFFIXES or not plugin.is_file():
        return []
    prefix = f"{plugin.stem} - ".lower()
    return sorted(
        (
            path
            for path in plugin.parent.iterdir()
            if path.is_file()
            and path.suffix.lower() == ".ba2"
            and path.name.lower().startswith(prefix)
            and not path.stem.lower().endswith("_ps")
        ),
        key=lambda path: path.name.lower(),
    )


def validate_conversion_inputs(
    plugin_path: str | Path,
    archive_paths: Sequence[str | Path],
    output_dir: str | Path,
) -> tuple[Path, tuple[Path, ...], Path]:
    plugin = Path(plugin_path).resolve(strict=False)
    if plugin.suffix.lower() not in PLUGIN_SUFFIXES:
        raise ValueError("Select a Fallout 4 ESM, ESP, or ESL plugin")
    if not plugin.is_file():
        raise FileNotFoundError(f"Plugin not found: {plugin}")
    archives = tuple(Path(path).resolve(strict=False) for path in archive_paths)
    if not archives:
        raise ValueError("Select at least one PC BA2 archive")
    for archive in archives:
        if archive.suffix.lower() != ".ba2" or not archive.is_file():
            raise FileNotFoundError(f"BA2 archive not found: {archive}")
        if archive.stem.lower().endswith("_ps"):
            raise ValueError(f"Select a PC archive, not a PlayStation archive: {archive.name}")
    destination = Path(output_dir).resolve(strict=False)
    if destination.exists() and not destination.is_dir():
        raise NotADirectoryError(f"Output is not a directory: {destination}")
    return plugin, archives, destination


def convert_mod_to_playstation(
    plugin_path: str | Path,
    archive_paths: Sequence[str | Path],
    output_dir: str | Path,
    *,
    replace_existing: bool = False,
    convert_wav_to_at9: bool = False,
    progress: Callable[[int, int, str], None] | None = None,
    extractor: Callable[..., object] | None = None,
    asset_converter: Callable[..., dict] | None = None,
    packer: Callable[..., object] | None = None,
) -> dict:
    plugin, archives, destination = validate_conversion_inputs(
        plugin_path, archive_paths, output_dir
    )
    existing = sorted(destination.glob(f"{plugin.stem} - *_ps.ba2")) if destination.is_dir() else []
    if existing and not replace_existing:
        names = ", ".join(path.name for path in existing[:3])
        raise FileExistsError(
            f"PlayStation archive already exists: {names}. Enable replacement to overwrite it."
        )
    if extractor is None:
        from creation_lib.ba2 import native_runtime
        extractor = native_runtime.extract_archive
    if asset_converter is None:
        asset_converter = run_ps4_asset_batch
    if packer is None:
        from creation_lib.build.packer import pack_mod
        packer = pack_mod

    total_steps = len(archives) + 2
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="playstation-converter-") as temporary:
        project_root = Path(temporary)
        data_dir = project_root / "mods" / plugin.stem / "data"
        data_dir.mkdir(parents=True)
        for index, archive in enumerate(archives, start=1):
            if progress:
                progress(index - 1, total_steps, f"Extracting {archive.name}")
            extractor(str(archive), str(data_dir), workers=PS4_WORKERS)
        if progress:
            progress(len(archives), total_steps, "Converting HKX, NIF, and BTO assets")
        conversion = asset_converter(
            str(data_dir), str(data_dir), False, max_workers=PS4_WORKERS
        )
        errors = conversion.get("errors", [])
        if errors:
            first = errors[0]
            raise RuntimeError(
                f"Asset conversion failed for {first.get('path', '?')}: "
                f"{first.get('error', 'unknown error')}"
            )
        if progress:
            progress(len(archives) + 1, total_steps, "Packing PlayStation BA2 archives")
        pack_options = {
            "pc": False,
            "ps": True,
            "game": "fo4",
            "project_root": project_root,
            "archive_output_dir": destination,
            "archive_workers": PS4_WORKERS,
        }
        if convert_wav_to_at9:
            pack_options["ps_at9"] = True
        packer(plugin.stem, **pack_options)
    outputs = sorted(destination.glob(f"{plugin.stem} - *_ps.ba2"))
    if not outputs:
        raise RuntimeError("Packing completed without producing a PlayStation BA2 archive")
    if progress:
        progress(total_steps, total_steps, "Complete")
    return {
        "plugin": str(plugin),
        "archives": [str(path) for path in outputs],
        "asset_conversion": conversion,
        "workers": PS4_WORKERS,
        "at9_audio": convert_wav_to_at9,
    }


class PlayStationConverterTool(BaseTool):
    name = "PlayStation Converter"
    tool_id = "playstation_converter"
    description = "Convert a Fallout 4 plugin's PC BA2 assets and build native _ps.ba2 archives"
    category = "Assets"

    def __init__(self):
        super().__init__()
        self._plugin_path = ""
        self._archive_paths: list[str] = []
        self._output_dir = ""
        self._replace_existing = False
        self._convert_wav_to_at9 = False

    def draw_content(self) -> None:
        imgui.text_wrapped(
            "Select one Fallout 4 plugin and its PC BA2 archives. The plugin is not modified; "
            "its filename determines the new _ps.ba2 archive names."
        )
        imgui.spacing()
        if begin_form("##playstation_converter_inputs"):
            _, clicked = draw_path_row("Plugin", self._plugin_path, "Select...")
            if clicked and not self._running:
                selected = pick_file(
                    "Select Fallout 4 plugin",
                    [("Fallout 4 plugins", "*.esm *.esp *.esl")],
                    str(Path(self._plugin_path).parent) if self._plugin_path else "",
                )
                if selected:
                    self._set_plugin(selected)
            _, clicked = draw_path_row("Output folder", self._output_dir)
            if clicked and not self._running:
                selected = pick_folder("Select PlayStation BA2 output folder", self._output_dir)
                if selected:
                    self._output_dir = selected
            end_form()

        imgui.separator_text("PC BA2 archives")
        if imgui.button("Add BA2...") and not self._running:
            selected = pick_file(
                "Select PC BA2 archive",
                [("BA2 archives", "*.ba2")],
                str(Path(self._plugin_path).parent) if self._plugin_path else "",
            )
            if selected and selected not in self._archive_paths:
                self._archive_paths.append(selected)
                self._archive_paths.sort(key=str.lower)
        imgui.same_line()
        if imgui.button("Find matching BA2s") and not self._running and self._plugin_path:
            self._archive_paths = [str(path) for path in discover_ba2_archives(self._plugin_path)]
        if not self._archive_paths:
            imgui.text_disabled("No BA2 archives selected")
        for index, archive in enumerate(tuple(self._archive_paths)):
            imgui.push_id(index)
            if imgui.small_button("Remove") and not self._running:
                self._archive_paths.remove(archive)
                imgui.pop_id()
                break
            imgui.same_line()
            imgui.text_wrapped(archive)
            imgui.pop_id()

        imgui.separator()
        _, self._replace_existing = imgui.checkbox(
            "Replace existing _ps.ba2 archives", self._replace_existing
        )
        _, self._convert_wav_to_at9 = imgui.checkbox(
            "Convert WAV/XWM audio to ATRAC9", self._convert_wav_to_at9
        )
        if self._convert_wav_to_at9:
            imgui.text_disabled("Uses the built-in native ATRAC9 encoder.")
        imgui.text_disabled("HKX, NIF, and BTO conversion uses 16 worker threads.")
        can_convert = bool(
            self._plugin_path and self._archive_paths and self._output_dir and not self._running
        )
        if not can_convert:
            imgui.begin_disabled()
        if imgui.button("Build PlayStation Archives", imgui.ImVec2(220, 0)):
            self._start_batch(self._convert)
        if not can_convert:
            imgui.end_disabled()

    def _set_plugin(self, plugin_path: str) -> None:
        self._plugin_path = plugin_path
        plugin = Path(plugin_path)
        self._output_dir = str(plugin.parent)
        self._archive_paths = [str(path) for path in discover_ba2_archives(plugin)]

    def _convert(self) -> None:
        result = convert_mod_to_playstation(
            self._plugin_path,
            self._archive_paths,
            self._output_dir,
            replace_existing=self._replace_existing,
            convert_wav_to_at9=self._convert_wav_to_at9,
            progress=self._on_progress,
        )
        outputs = result["archives"]
        converted = result["asset_conversion"].get("converted", 0)
        self._result_msg = (
            f"Created {len(outputs)} PlayStation archive(s); converted {converted} HKX/NIF/BTO "
            f"asset(s). Output: {self._output_dir}"
        )

    def get_default_settings(self) -> dict:
        return {
            "replace_existing": False,
            "convert_wav_to_at9": False,
        }

    def apply_settings(self, settings: dict) -> None:
        self._replace_existing = bool(settings.get("replace_existing", False))
        self._convert_wav_to_at9 = bool(settings.get("convert_wav_to_at9", False))

    def collect_settings(self) -> dict:
        return {
            "replace_existing": self._replace_existing,
            "convert_wav_to_at9": self._convert_wav_to_at9,
        }
