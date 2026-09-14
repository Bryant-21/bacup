"""Conversion project panel for B.A.C.U.P."""
from __future__ import annotations

import contextlib
import logging
import os
import shutil
import threading
import time
from dataclasses import dataclass
from pathlib import Path

from imgui_bundle import imgui

from creation_lib.ui.widgets.forms import (
    begin_form, end_form, form_row_label as _form_row_label, draw_combo_field, draw_path_row,
)
from creation_lib.ui.widgets.modern import (
    expandable_section,
    action_button, heading, prepare_dialog, ring_stat, scaled, section, semantic_color, status_indicator, toggle,
)
from creation_lib.ui.widgets.images import image_in_box

from creation_lib.build.archive_plan import discover_mod_archives
from creation_lib.build.packer import pack_mod
from bacup_lib.family_map import resolve_upgrade_plan
from bacup_lib.input_preflight import (
    InputPreflightReport,
    MissingInput,
    scan_conversion_inputs,
)
from bacup_lib.install_debug import audit_archive_ini, repair_archive_ini
from bacup_lib.models import PhaseProgress
from bacup_lib.regen_pipeline import RegenOptions, RegenPaths
from bacup_lib.run_diagnostics import (
    create_run_diagnostics_dir,
    full_logging_scope,
)
from bacup_lib.runner import ConversionRunner, emit_runner_status
from bacup_lib.source_pairs import (
    DEFAULT_PAIR_ID,
    SOURCE_PAIRS,
    get_pair,
    required_exclude_signatures,
)
from bacup_lib.upgrade_manifest import (
    bundled_upgrade_manifest_path,
    load_upgrade_manifest,
    requires_forced_regen,
    resolve_family_union,
)
from bacup_lib.version_stamp import read_plugin_snam_header
from bacup_lib.worker_advice import detect_system_ram_gb, recommend_workers
from bacup_lib.lod_settings import (
    PROFILE_HIGH_QUALITY,
    PROFILE_LABELS,
    PROFILE_PERFORMANCE,
    available_profiles,
    load_profile_settings,
)
from ui.toolkit.app_paths import (
    get_code_root,
    get_exe_dir,
    get_logs_dir,
    get_resource_dir,
)
from bacup_ui.setup import (
    _format_gb,
    clear_project_owned_extractions,
    get_project_profile,
    get_project_setup_ownership,
    project_owns_extracted_path,
)
from bacup_ui.storage_cleanup import (
    CleanupResult,
    CleanupTarget,
    delete_cleanup_targets,
    discover_cleanup_targets,
    is_running_as_admin,
    measure_cleanup_targets,
    restart_as_admin,
    windows_temp_dir,
)
from bacup_ui.conversion.widgets.runner_overlay import draw_runner_overlay
from bacup_ui.conversion.progress import ProgressEstimate, estimated_phase_weights
from bacup_ui.conversion.music import (
    ConversionMusicPlayer,
    format_music_time,
    prepare_theme_tracks,
    prioritize_music_tracks,
)
from creation_lib.core.fo4_version import detect_ba2_target
from creation_lib.core.store_install import StoreInstallResult, validate_store_install_for_game

_log = logging.getLogger("toolkit.appalachia")
_NS = "##appalachia"
_PHASE_ALIASES = {
    "copy_sounds": ("copy_sounds", "Copy Sounds"),
    "convert_animations": ("convert_animations", "Convert Animations"),
    "convert_btos": ("convert_terrain_assets", "Convert Terrain Assets"),
    "convert_btos_v2": ("convert_terrain_assets", "Convert Terrain Assets"),
    "convert_havok": ("convert_havok", "Convert Havok"),
    "convert_materials": ("convert_materials", "Convert Materials"),
    "convert_materials_v2": ("convert_materials", "Convert Materials"),
    "convert_nifs": ("convert_nifs", "Convert NIFs"),
    "convert_nifs_v2": ("convert_nifs", "Convert NIFs"),
    "convert_gamebryo_nifs": ("convert_nifs", "Convert NIFs"),
    "convert_skeleton": ("convert_skeleton", "Convert Skeleton"),
    "copy_materialized_facegen": ("copy_materialized_facegen", "Copy FaceGen"),
    "convert_terrain": ("convert_terrain", "Convert Terrain"),
    "convert_textures": ("convert_textures", "Convert Textures"),
    "convert_textures_v2": ("convert_textures", "Convert Textures"),
    "deploy_mod": ("deploy", "Deploy Mod"),
    "finalize_plugin_records": ("finalize_plugin_records", "Regenerate MODT / Finalize Plugin"),
    "generate_lod": ("lodgen", "Generate LOD"),
    "generate_animtextdata": ("generate_anim_text_data", "Generate AnimTextData"),
    "lod": ("lodgen", "Generate LOD"),
    "lodgen": ("lodgen", "Generate LOD"),
    "offsets": ("rebuild_cell_offsets", "Rebuild Cell Offsets"),
    "cell_offsets": ("rebuild_cell_offsets", "Rebuild Cell Offsets"),
    "pack_ba2": ("pack", "Pack BA2"),
    "postprocess_havok_assets": ("postprocess_havok_assets", "Postprocess Havok Assets"),
    "scaffold_mod": ("scaffold_mod", "Scaffold Mod"),
    "synthesize_drivers": ("synthesize_drivers", "Synthesize Drivers"),
    "translate_records": ("translate_records", "Translate Records"),
    "translate_records_rust": ("translate_records", "Translate Records"),
}
_LOD_PROFILE_VALUES = [PROFILE_HIGH_QUALITY, PROFILE_PERFORMANCE]


def _lod_profile_values(pair_id: str) -> list[str]:
    shipped = available_profiles(pair_id)
    return [p for p in _LOD_PROFILE_VALUES if p in shipped]


def _default_lod_mode(pair_id: str) -> str:
    # hybrid-atlas consumes FO76 BTO tiles; every other pair generates from
    # records, matching the mode regen.py forces for cross-game pairs.
    return "hybrid-atlas" if pair_id == DEFAULT_PAIR_ID else "generate"
_DEFAULT_ARCHIVE_MAX_GB = 8
_ARCHIVE_MIN_GB = 1
_ARCHIVE_MAX_GB = 16
_APPALACHIA_ARCHIVE_MAX_BYTES = _DEFAULT_ARCHIVE_MAX_GB * 1024**3
_UNLIMITED_ARCHIVE_MAX_BYTES = (1 << 63) - 1
_ARCHIVE_MAX_GB_KEY = "archive_max_gb"
_DEPLOY_FORMAT_KEY = "deploy_format"
_DEPLOY_FORMAT_MIGRATION_KEY = "deploy_format_standard_default_v1"
_DEFAULT_DEPLOY_FORMAT = "standard"
_DEPLOY_FORMAT_VALUES = ["expanded", "standard", "loose"]
_DEPLOY_MODE_VALUES = ["packed", "loose"]
_DEPLOY_MODE_LABELS = ["Packed BA2", "Loose files"]
_ARCHIVE_LAYOUT_VALUES = ["standard", "expanded"]
_ARCHIVE_LAYOUT_LABELS = ["Standard (recommended)", "Expanded"]
_BA2_TARGET_KEY = "ba2_target"
_DEFAULT_BA2_TARGET = "og"
_BA2_TARGET_VALUES = ["auto", "og", "nextgen"]
_BA2_TARGET_LABELS = ["Auto (detect FO4)", "Force OG (v1)", "Force Next-Gen (v8)"]
_BA2_COMPRESSION_KEY = "ba2_compression"
_BA2_COMPRESSION_VALUES = [None, 1, 6, 9]
_BA2_COMPRESSION_LABELS = [
    "Default (recommended)",
    "Fast (level 1)",
    "Balanced (level 6)",
    "High (level 9)",
]
_ATLAS_MIP_FLOODING_KEY = "atlas_mip_flooding"
_TEXTURE_LANDSCAPE_MIP_FLOODING_KEY = "texture_landscape_mip_flooding"
_GENERATE_PRECOMBINES_KEY = "generate_precombines"
_FULL_LOGGING_KEY = "full_logging"
_CONVERSION_MUSIC_MUTED_KEY = "conversion_music_muted"
_CONVERSION_MUSIC_VOLUME_KEY = "conversion_music_volume"
_DEFAULT_CONVERSION_MUSIC_VOLUME = 0.12
_WORKERS_KEY = "workers"
_LOD_PROFILE_KEY = "lod_profile"
_INSTALL_LOCATION_KEY = "install_location"
_INSTALL_LOCATION_VALUES = ["game", "mo2", "vortex", "none"]
_INSTALL_LOCATION_LABELS = ["Game Dir", "MO2", "Vortex", "None"]
_FO76_SOURCE_KEY = "fo76_source"
_FO76_SOURCE_VALUES = ["retail", "playtest"]
_FO76_SOURCE_LABELS = ["Fallout 76", "Fallout 76 Public Test Server"]
_RECOVERY_PHASE_KEY = "recovery_phase"
_RECOVERY_PHASE_VALUES = [
    "prepare",
    "records",
    "terrain",
    "terrain_assets",
    "nifs",
    "textures",
    "materials",
    "havok",
    "drivers",
    "animations",
    "sounds",
    "scaffold",
    "build",
    "modt",
    "offsets",
    "animtext",
    "lodgen",
    "pack",
    "deploy",
]
_RECOVERY_PHASE_LABELS = [
    "Prepare Conversion (full rebuild)",
    "Translate Records (full rebuild)",
    "Convert Terrain (full rebuild)",
    "Convert Terrain Assets (full rebuild)",
    "Convert NIFs",
    "Convert Textures",
    "Convert Materials",
    "Convert Havok",
    "Synthesize Drivers (reruns Havok)",
    "Convert Animations",
    "Copy Sounds",
    "Scaffold Mod (full rebuild)",
    "Build ESP (full rebuild)",
    "Regenerate MODT",
    "Rebuild Cell Offsets",
    "Generate AnimTextData",
    "Generate LOD",
    "Pack BA2",
    "Deploy Mod",
]
_RECOVERY_FULL_REBUILD_PHASES = {
    "prepare",
    "records",
    "terrain",
    "terrain_assets",
    "scaffold",
    "build",
}
_COMPANION_MOD_NAME = "B21_TalesFromAppalachia"
_COMPANION_ROOT_FILES = (f"{_COMPANION_MOD_NAME}.esp",)
_COMPANION_DEPLOY_DIRS = ("PrismaUI_F4", "F4SE")
_STORE_INSTALL_LABELS = {
    "fo4": "FO4",
    "fo76": "FO76",
    "fnv": "FNV",
    "fo3": "FO3",
    "skyrimse": "Skyrim SE",
    "starfield": "Starfield",
}
_PROJECT_SETTINGS_KEY = "conversion_projects"
_WORKSPACE_SETTINGS_ID = "appalachia"
_PROJECT_BY_PAIR = {
    "fo76:fo4": ("appalachia", "Tales From Appalachia"),
    "fnvfo3:fo4": ("wasteland", "Legends of the Wasteland"),
    "skyrimse:fo4": ("north", "Northern Lands"),
    "starfield:fo4": ("stars", "To The Stars"),
}
_GIB = 1024**3
_FO76_INSTALL_REFERENCE_BYTES = 180 * _GIB
_LOOSE_WORKSPACE_PEAK_BYTES = 120 * _GIB
_PACKED_MOD_PEAK_BYTES = 60 * _GIB
# Reference measurements and headroom are recorded in bacup/docs/ui-modernization.md.
_CONVERSION_SPACE_ESTIMATES = {
    "fo76:fo4": (_LOOSE_WORKSPACE_PEAK_BYTES, _PACKED_MOD_PEAK_BYTES),
    "fnvfo3:fo4": (25 * _GIB, 10 * _GIB),
    "skyrimse:fo4": (25 * _GIB, 15 * _GIB),
    "starfield:fo4": (75 * _GIB, 45 * _GIB),
}
_DISK_SPACE_MIN_HEADROOM_BYTES = 20 * _GIB


@dataclass(frozen=True)
class _DiskSpaceVolume:
    key: str
    path: Path
    labels: tuple[str, ...]
    required_bytes: int
    total_bytes: int
    free_bytes: int

    @property
    def insufficient(self) -> bool:
        return self.free_bytes < 0 or self.free_bytes < self.required_bytes

    @property
    def unavailable(self) -> bool:
        return self.free_bytes < 0

    @property
    def projected_fill_fraction(self) -> float:
        if self.unavailable or self.total_bytes <= 0:
            return 0.0
        projected_used = self.total_bytes - self.free_bytes + self.required_bytes
        return min(1.0, max(0.0, projected_used / self.total_bytes))

    @property
    def space_level(self) -> str:
        if self.unavailable:
            return "yellow"
        if self.insufficient:
            return "red"
        remaining_bytes = self.free_bytes - self.required_bytes
        warning_headroom = max(
            _DISK_SPACE_MIN_HEADROOM_BYTES,
            self.required_bytes // 4,
        )
        if remaining_bytes < warning_headroom:
            return "yellow"
        return "green"


def _volume_key(path: Path) -> str:
    absolute = os.path.abspath(path)
    drive, _tail = os.path.splitdrive(absolute)
    return os.path.normcase(drive or Path(absolute).anchor or absolute)


def _disk_usage_for_target(path: Path):
    candidate = Path(os.path.abspath(path))
    while not candidate.exists():
        parent = candidate.parent
        if parent == candidate:
            return None
        candidate = parent
    try:
        return shutil.disk_usage(candidate)
    except OSError:
        return None


def _project_disk_space(
    *,
    output_root: Path,
    archive_root: Path,
    loose_bytes: int = _LOOSE_WORKSPACE_PEAK_BYTES,
    packed_bytes: int = _PACKED_MOD_PEAK_BYTES,
    volume_key=_volume_key,
    disk_usage=None,
) -> tuple[_DiskSpaceVolume, ...]:
    disk_usage = disk_usage or _disk_usage_for_target
    grouped: dict[str, dict] = {}
    for path, label, required_bytes in (
        (output_root, "loose workspace", loose_bytes),
        (archive_root, "packed BA2s", packed_bytes),
    ):
        key = volume_key(path)
        entry = grouped.setdefault(
            key,
            {"path": path, "labels": [], "required_bytes": 0},
        )
        entry["labels"].append(label)
        entry["required_bytes"] += required_bytes

    volumes = []
    for key, entry in grouped.items():
        usage = disk_usage(entry["path"])
        volumes.append(
            _DiskSpaceVolume(
                key=key,
                path=entry["path"],
                labels=tuple(entry["labels"]),
                required_bytes=entry["required_bytes"],
                total_bytes=usage.total if usage is not None else 0,
                free_bytes=usage.free if usage is not None else -1,
            )
        )
    return tuple(volumes)


def _mod_archive_sizes(
    output_root: Path,
    deploy_root: Path,
    mod_name: str,
) -> tuple[int, int]:
    seen_roots: set[str] = set()
    seen_archives: set[str] = set()
    totals = []
    for root in (output_root, deploy_root):
        root_key = os.path.normcase(os.path.abspath(root))
        if root_key in seen_roots:
            totals.append(0)
            continue
        seen_roots.add(root_key)
        total = 0
        for archive in discover_mod_archives(root, mod_name):
            archive_key = os.path.normcase(os.path.abspath(archive))
            if archive_key in seen_archives:
                continue
            seen_archives.add(archive_key)
            try:
                total += archive.stat().st_size
            except OSError:
                continue
        totals.append(total)
    return totals[0], totals[1]


def _data_dir(root_dir: str) -> Path:
    p = Path(root_dir)
    return p if p.name.lower() == "data" else p / "Data"


def _conversion_output_root(
    exe_dir: Path,
    mod_name: str,
    protected_paths: tuple[Path, ...],
) -> Path:
    def overlaps(candidate: Path) -> bool:
        resolved_candidate = candidate.resolve()
        return any(
            resolved_candidate == protected
            or resolved_candidate in protected.parents
            or protected in resolved_candidate.parents
            for protected in (path.resolve() for path in protected_paths)
        )

    output_root = exe_dir / "mods" / mod_name
    if not overlaps(output_root):
        return output_root
    for parent in (exe_dir, *exe_dir.parents):
        output_root = parent / "BACUP Workspace" / "mods" / mod_name
        if not overlaps(output_root):
            return output_root
    raise ValueError("could not find a conversion workspace outside protected game paths")


def _archive_max_gb(value) -> int:
    try:
        gb = int(value)
    except (TypeError, ValueError):
        gb = _DEFAULT_ARCHIVE_MAX_GB
    return max(_ARCHIVE_MIN_GB, min(_ARCHIVE_MAX_GB, gb))


def _recovery_phase(value) -> str:
    phase = str(value or "").strip().lower().replace("-", "_").replace(" ", "_")
    return phase if phase in _RECOVERY_PHASE_VALUES else "lodgen"


def _format_duration(seconds: float) -> str:
    minutes, seconds = divmod(round(seconds), 60)
    hours, minutes = divmod(minutes, 60)
    if hours:
        return f"{hours}h {minutes}m {seconds}s"
    if minutes:
        return f"{minutes}m {seconds}s"
    return f"{seconds}s"


def _pick_folder(title: str, default_path: str = "") -> str | None:
    try:
        from creation_lib.ui.widgets.pick_folder import pick_folder

        return pick_folder(title, default_path)
    except Exception as e:
        _log.warning("Folder picker failed: %s", e)
        return None


def _phase_slug(value: object) -> str:
    text = str(value or "").strip().lower()
    text = text.replace("(rust)", "")
    out: list[str] = []
    last_was_sep = False
    for ch in text:
        if ch.isalnum():
            out.append(ch)
            last_was_sep = False
        elif not last_was_sep:
            out.append("_")
            last_was_sep = True
    return "".join(out).strip("_")


class RegenPanel:
    def __init__(
        self,
        workspace,
        log_panel=None,
        *,
        fixed_pair_id: str | None = None,
        project_id: str | None = None,
        project_label: str | None = None,
    ) -> None:
        self._workspace = workspace
        self._log_panel = log_panel
        if fixed_pair_id is not None:
            get_pair(fixed_pair_id)
        self.fixed_pair_id = fixed_pair_id
        self.pair_id = fixed_pair_id or DEFAULT_PAIR_ID
        default_project_id, default_project_label = _PROJECT_BY_PAIR[self.pair_id]
        self.project_id = project_id or default_project_id
        self.project_label = project_label or default_project_label
        workspace_settings = self._workspace_settings()
        self.install_location = str(
            workspace_settings.get(_INSTALL_LOCATION_KEY, "game")
        ).strip().lower()
        if self.install_location not in _INSTALL_LOCATION_VALUES:
            self.install_location = "game"
        self.deploy = self.install_location != "none"
        self.install_path = str(workspace_settings.get("install_path", "") or "")
        self.mo2_use_profile_ini = bool(
            workspace_settings.get("mo2_use_profile_ini", True)
        )
        self.add_archives_to_ini = True
        self.fo76_source = str(
            workspace_settings.get(_FO76_SOURCE_KEY, "retail") or "retail"
        ).strip().lower()
        if self.fo76_source not in _FO76_SOURCE_VALUES:
            self.fo76_source = "retail"
        if not self._fo76_playtest_root():
            self.fo76_source = "retail"
        self._install_audit = None
        self._install_audit_error = None
        self.archive_max_gb = _archive_max_gb(
            workspace_settings.get(_ARCHIVE_MAX_GB_KEY, _DEFAULT_ARCHIVE_MAX_GB)
        )
        self.deploy_format = str(
            workspace_settings.get(_DEPLOY_FORMAT_KEY, _DEFAULT_DEPLOY_FORMAT)
        ).strip().lower()
        if self.deploy_format not in _DEPLOY_FORMAT_VALUES:
            self.deploy_format = _DEFAULT_DEPLOY_FORMAT
        if not bool(workspace_settings.get(_DEPLOY_FORMAT_MIGRATION_KEY, False)):
            if self.deploy_format == "expanded":
                self.deploy_format = _DEFAULT_DEPLOY_FORMAT
            self._set_workspace_settings(
                {
                    _DEPLOY_FORMAT_KEY: self.deploy_format,
                    _DEPLOY_FORMAT_MIGRATION_KEY: True,
                }
            )
        self.ba2_target = str(
            workspace_settings.get(_BA2_TARGET_KEY, _DEFAULT_BA2_TARGET)
        ).strip().lower()
        if self.ba2_target not in _BA2_TARGET_VALUES:
            self.ba2_target = _DEFAULT_BA2_TARGET
        saved_compression = workspace_settings.get(_BA2_COMPRESSION_KEY)
        self.ba2_compression_level = (
            saved_compression
            if saved_compression in _BA2_COMPRESSION_VALUES
            else None
        )
        self._ba2_detect_cache: tuple[str, tuple] | None = None
        self._preflight_report = None
        self._preflight_cache: tuple[str, object] | None = None
        self._worker_rec = recommend_workers(detect_system_ram_gb(), os.cpu_count() or 2)
        max_workers = max(1, (os.cpu_count() or 2) - 1)  # never let the user select all threads
        saved_workers = workspace_settings.get(_WORKERS_KEY)
        self.workers = (
            max(0, min(int(saved_workers), max_workers))
            if isinstance(saved_workers, int)
            else min(self._worker_rec.recommended, max_workers)
        )
        self.lod_mode = _default_lod_mode(self.pair_id)
        self.lod_profile = str(
            workspace_settings.get(_LOD_PROFILE_KEY, PROFILE_HIGH_QUALITY)
        )
        if self.lod_profile not in _LOD_PROFILE_VALUES:
            self.lod_profile = PROFILE_HIGH_QUALITY
        self.atlas_mip_flooding = bool(
            workspace_settings.get(_ATLAS_MIP_FLOODING_KEY, False)
        )
        self.texture_landscape_mip_flooding = bool(
            workspace_settings.get(_TEXTURE_LANDSCAPE_MIP_FLOODING_KEY, False)
        )
        self.full_logging = bool(workspace_settings.get(_FULL_LOGGING_KEY, False))
        self.music_muted = bool(
            workspace_settings.get(_CONVERSION_MUSIC_MUTED_KEY, True)
        )
        try:
            configured_music_volume = float(
                workspace_settings.get(
                    _CONVERSION_MUSIC_VOLUME_KEY,
                    _DEFAULT_CONVERSION_MUSIC_VOLUME,
                )
            )
        except (TypeError, ValueError):
            configured_music_volume = _DEFAULT_CONVERSION_MUSIC_VOLUME
        self.music_volume = max(0.0, min(configured_music_volume, 1.0))
        self._music_player = ConversionMusicPlayer(volume=self.music_volume)
        self._conversion_music_session_active = False
        self._music_missing_logged = False
        # Experimental precombine generation — default off (see
        # models.PhaseSelection.generate_precombines).
        self.generate_precombines = bool(
            workspace_settings.get(_GENERATE_PRECOMBINES_KEY, False)
        )
        self.re_use_land = False
        self.recovery_phase = _recovery_phase(
            workspace_settings.get(_RECOVERY_PHASE_KEY, "lodgen")
        )
        self._upgrade_manifest_cache: tuple[str, object] | None = None
        self._snam_cache: tuple[tuple, str] | None = None
        self._phases: list[dict] = []
        self._runner_status = "Starting conversion"
        self._reset_progress()
        self._summary: dict | None = None
        self._completion: dict | None = None
        self._disk_usage_cache: tuple[float, dict[str, int]] | None = None
        self._disk_usage_cache_key: tuple[str, str] | None = None
        self._disk_space_cache: tuple[
            tuple[str, str], tuple[_DiskSpaceVolume, ...]
        ] | None = None
        self._disk_usage_lock = threading.Lock()
        self._disk_usage_running = False
        self._disk_usage_thread: threading.Thread | None = None
        self._waiting_for_space_check = False
        self._low_space_warning: tuple[_DiskSpaceVolume, ...] | None = None
        self._cleanup_lock = threading.Lock()
        self._cleanup_dialog_open = False
        self._cleanup_status = "idle"
        self._cleanup_targets: tuple[CleanupTarget, ...] = ()
        self._cleanup_selected: set[str] = set()
        self._cleanup_pending_result: CleanupResult | None = None
        self._cleanup_disk_refresh_pending = False
        self._cleanup_message: str | None = None
        self._cleanup_error: str | None = None
        self._admin_restart_error: str | None = None
        self._is_admin = is_running_as_admin()
        self._store_install_cache: dict[str, tuple[str, StoreInstallResult]] = {}
        self._status_log_frac = 0.44
        self.upgrade = False
        self._upgrade_user_toggled = False

    def _settings(self):
        return self._workspace._toolkit_settings

    def _fo76_playtest_root(self) -> str:
        return str(
            self._settings().get_game_paths("fo76").get("pts_root_dir", "") or ""
        ).strip()

    def _pair(self):
        pair_id = getattr(self, "fixed_pair_id", None) or getattr(
            self, "pair_id", DEFAULT_PAIR_ID
        )
        return get_pair(pair_id)

    def _is_default_pair(self) -> bool:
        return self._pair().pair_id == DEFAULT_PAIR_ID

    def _project_label(self) -> str:
        return getattr(
            self,
            "project_label",
            _PROJECT_BY_PAIR[self._pair().pair_id][1],
        )

    def _runner_running(self) -> bool:
        runner = getattr(self._workspace, "_runner", None)
        return runner is not None and not runner.done

    def _owns_active_runner(self) -> bool:
        if not self._runner_running():
            return False
        return getattr(self._workspace, "_runner_owner", self) is self

    def _required_game_ids(self) -> tuple[str, ...]:
        pair = self._pair()
        games = [pair.target_game, pair.source_game]
        if pair.merge is not None:
            games.append(pair.merge.grafted_game)
        return tuple(dict.fromkeys(games))

    def _source_asset_game_ids(self) -> tuple[str, ...]:
        project_id = (
            getattr(self, "project_id", _PROJECT_BY_PAIR[self._pair().pair_id][0])
            if getattr(self, "fixed_pair_id", None) is not None
            else _PROJECT_BY_PAIR[self._pair().pair_id][0]
        )
        return get_project_profile(project_id).source_games

    def _workspace_settings(self) -> dict:
        get_workspace = getattr(self._settings(), "get_workspace_settings", None)
        if callable(get_workspace):
            settings = dict(get_workspace(_WORKSPACE_SETTINGS_ID) or {})
            if getattr(self, "project_id", "appalachia") == "appalachia":
                return settings
            projects = settings.get(_PROJECT_SETTINGS_KEY, {})
            if isinstance(projects, dict):
                project_settings = projects.get(self.project_id, {})
                if isinstance(project_settings, dict):
                    return dict(project_settings)
        return {}

    def _set_workspace_settings(self, values: dict) -> None:
        set_workspace = getattr(self._settings(), "set_workspace_settings", None)
        if callable(set_workspace):
            if getattr(self, "project_id", "appalachia") == "appalachia":
                set_workspace(_WORKSPACE_SETTINGS_ID, values)
                return
            get_workspace = getattr(self._settings(), "get_workspace_settings", None)
            root = (
                dict(get_workspace(_WORKSPACE_SETTINGS_ID) or {})
                if callable(get_workspace)
                else {}
            )
            projects = dict(root.get(_PROJECT_SETTINGS_KEY, {}) or {})
            project_settings = dict(projects.get(self.project_id, {}) or {})
            project_settings.update(values)
            projects[self.project_id] = project_settings
            set_workspace(_WORKSPACE_SETTINGS_ID, {_PROJECT_SETTINGS_KEY: projects})

    def _music_dir_groups_for_game(
        self,
        game_id: str,
    ) -> tuple[tuple[str, tuple[Path, ...]], ...]:
        game_paths = self._settings().get_game_paths(game_id)
        extracted_dir = str(game_paths.get("extracted_dir", "") or "")
        roots: list[Path] = []
        if extracted_dir:
            roots.append(Path(extracted_dir))
        for root in (get_exe_dir(), get_code_root()):
            roots.append(root / "extracted" / game_id)
        if game_id == "fo76":
            if extracted_dir:
                roots.append(Path(extracted_dir).parent / "fo76sound")
            for root in (get_exe_dir(), get_code_root()):
                roots.append(root / "extracted" / "fo76sound")
        unique_roots = tuple(dict.fromkeys(roots))
        return (
            (
                "special",
                tuple(root / "music" / "special" for root in unique_roots),
            ),
            (
                "radio_licensed",
                tuple(
                    root / "sound" / "songs" / "radio" / "licensed"
                    for root in unique_roots
                ),
            ),
            (
                "radio",
                tuple(
                    root / "sound" / "fx" / "mus" / "radio"
                    for root in unique_roots
                ),
            ),
        )

    def _main_title_tracks_for_game(self, game_id: str) -> tuple[Path, ...]:
        root_dir = str(
            self._settings().get_game_paths(game_id).get("root_dir", "") or ""
        )
        if not root_dir:
            return ()
        main_title = Path(root_dir) / "MainTitle.wav"
        return (main_title,) if main_title.is_file() else ()

    def _conversion_music_tracks(self) -> tuple[Path, ...]:
        pair = self._pair()
        source_games = [pair.source_game]
        if pair.merge is not None and pair.merge.grafted_game not in source_games:
            source_games.append(pair.merge.grafted_game)
        tracks: list[Path] = []
        cache_root = get_exe_dir() / "cache" / "conversion" / "music"
        for game_id in source_games:
            tracks.extend(self._main_title_tracks_for_game(game_id))
            for group_name, theme_dirs in self._music_dir_groups_for_game(game_id):
                tracks.extend(
                    prepare_theme_tracks(
                        theme_dirs,
                        cache_root / game_id / group_name,
                    )
                )
        return prioritize_music_tracks(tracks)

    def _sync_conversion_music(self) -> None:
        player = getattr(self, "_music_player", None)
        if player is None:
            player = ConversionMusicPlayer(
                volume=getattr(
                    self,
                    "music_volume",
                    _DEFAULT_CONVERSION_MUSIC_VOLUME,
                )
            )
            self._music_player = player
        active = bool(getattr(self, "_conversion_music_session_active", False))
        muted = bool(getattr(self, "music_muted", True))
        if not active or muted:
            player.stop()
            return
        try:
            tracks = self._conversion_music_tracks()
        except Exception as exc:
            _log.warning("Unable to discover conversion music: %s", exc)
            return
        if not tracks:
            if not getattr(self, "_music_missing_logged", False):
                _log.warning(
                    "No conversion themes were found for %s",
                    self._pair().source_game,
                )
                self._music_missing_logged = True
            return
        self._music_missing_logged = False
        player.start(tracks)

    def _set_music_muted(self, muted: bool) -> None:
        self.music_muted = bool(muted)
        self._set_workspace_settings(
            {_CONVERSION_MUSIC_MUTED_KEY: self.music_muted}
        )
        self._sync_conversion_music()

    def _set_music_volume(self, volume: float) -> None:
        self.music_volume = max(0.0, min(float(volume), 1.0))
        self._music_player.set_volume(self.music_volume)
        self._set_workspace_settings(
            {_CONVERSION_MUSIC_VOLUME_KEY: self.music_volume}
        )

    def _begin_conversion_music_session(self) -> None:
        self._conversion_music_session_active = True
        self._sync_conversion_music()

    def _end_conversion_music_session(self) -> None:
        self._conversion_music_session_active = False
        player = getattr(self, "_music_player", None)
        if player is not None:
            player.stop()

    @staticmethod
    def _phase_number(phase: dict) -> int:
        try:
            return int(phase.get("phase", 0))
        except (TypeError, ValueError):
            return 0

    @staticmethod
    def _phase_key_and_label(phase: dict) -> tuple[str, str]:
        if phase.get("ui_key"):
            return str(phase["ui_key"]), str(phase.get("phase_name") or phase["ui_key"])
        raw_name = phase.get("phase_name") or phase.get("phase") or ""
        slug = _phase_slug(raw_name)
        if slug in _PHASE_ALIASES:
            return _PHASE_ALIASES[slug]
        label = str(phase.get("phase_name") or "").replace("(Rust)", "").strip()
        if not label:
            label = str(raw_name or "Unnamed phase").strip()
        return slug or f"phase_{RegenPanel._phase_number(phase)}", label

    @classmethod
    def _normalize_phase_data(cls, data: dict) -> dict:
        normalized = dict(data)
        key, label = cls._phase_key_and_label(normalized)
        normalized["ui_key"] = key
        normalized["phase_name"] = label
        return normalized

    @classmethod
    def _phase_rows(cls, phases: list[dict]) -> list[tuple[str, str]]:
        rows = []
        seen = set()
        for phase in phases:
            if phase.get("status", "pending") in {"pending", "skipped"}:
                continue
            key, label = cls._phase_key_and_label(phase)
            if key in seen:
                continue
            rows.append((key, label))
            seen.add(key)
        return rows

    def _reset_progress(self, start_phase: str = "prepare") -> None:
        self._phase_scroll_frames = 0
        self._progress_estimate = ProgressEstimate(
            estimated_phase_weights(
                self._pair().pair_id,
                lod_mode=self.lod_mode,
                packed=self.deploy_format != "loose",
                deploy=self.deploy and self.install_location != "none",
                precombines=getattr(self, "generate_precombines", False),
                start_phase=start_phase,
            )
        )
        self._progress_succeeded = False

    def _runner_progress(self) -> tuple[float, str]:
        messages = []
        for phase in self._phases:
            if phase.get("status") != "running":
                continue
            _key, label = self._phase_key_and_label(phase)
            item = str(phase.get("current_item") or "").replace("\\", "/").rsplit("/", 1)[-1]
            messages.append(f"{label}: {item}" if item else label)
        message = " | ".join(messages) or self._runner_status or "Starting conversion"
        if self._progress_succeeded:
            return 1.0, "Conversion complete"
        return self._progress_estimate.fraction(advance=self._owns_active_runner()), message

    def build_paths(self) -> RegenPaths:
        s = self._settings()
        pair = self._pair()
        source = s.get_game_paths(pair.source_game)
        target_game = s.get_game_paths(pair.target_game)
        source_root = source.get("root_dir", "") or ""
        if pair.pair_id == DEFAULT_PAIR_ID and self.fo76_source == "playtest":
            source_root = self._fo76_playtest_root() or source_root
        target_root = target_game.get("root_dir", "") or ""
        source_extracted = str(source.get("extracted_dir", "") or "")
        target_extracted = target_game.get("extracted_dir", "") or ""
        source_data_dir = _data_dir(source_root)
        target_data_dir = _data_dir(target_root)
        target_asset_root = get_exe_dir() / "cache" / "conversion"
        docs = Path.home() / "Documents" / "My Games" / "Fallout4"
        install_target = self._resolve_install_target(
            target_data_dir, docs / "Fallout4Custom.ini"
        )
        merge_primary_plugin_paths = ()
        merge_grafted_plugin_paths = ()
        additional_source_asset_roots = ()
        if pair.merge is not None:
            optional_paths = tuple(
                path
                for name in pair.optional_source_plugins
                if (path := source_data_dir / name).is_file()
            )
            merge_primary_plugin_paths = (
                *(source_data_dir / name for name in pair.source_plugins),
                *optional_paths,
            )
            if pair.merge.grafted_plugins:
                grafted = s.get_game_paths(pair.merge.grafted_game)
                grafted_data_dir = _data_dir(grafted.get("root_dir", "") or "")
                merge_grafted_plugin_paths = tuple(
                    grafted_data_dir / name for name in pair.merge.grafted_plugins
                )
                grafted_extracted = str(grafted.get("extracted_dir", "") or "")
                if grafted_extracted:
                    additional_source_asset_roots = (
                        Path(grafted_extracted),
                        grafted_data_dir,
                    )
        protected_output_paths = (
            *((source_data_dir,) if source_root else ()),
            *((Path(source_extracted),) if source_extracted else ()),
            *((target_data_dir,) if target_root else ()),
            *((Path(target_extracted),) if target_extracted else ()),
            *((install_target.deploy_data_dir,) if install_target.deploy_data_dir else ()),
            *additional_source_asset_roots,
        )
        return RegenPaths(
            source_extracted_dir=Path(source_extracted),
            source_data_dir=source_data_dir,
            target_extracted_dir=(
                Path(target_extracted) if target_extracted else None
            ),
            target_data_dir=target_data_dir,
            target_ck_ini_path=Path(target_root) / "CreationKitCustom.ini",
            target_custom_ini_path=docs / "Fallout4Custom.ini",
            target_game_ini_path=docs / "Fallout4.ini",
            output_root=_conversion_output_root(
                get_exe_dir(),
                pair.output_mod_name,
                protected_output_paths,
            ),
            mod_name=pair.output_mod_name,
            archive_base_name=Path(pair.output_plugin_name).stem,
            resource_dir=get_resource_dir(),
            deploy_data_dir=install_target.deploy_data_dir,
            runtime_ini_path=install_target.runtime_ini_path,
            merge_primary_plugin_paths=merge_primary_plugin_paths,
            merge_grafted_plugin_paths=merge_grafted_plugin_paths,
            additional_source_asset_roots=additional_source_asset_roots,
            target_asset_catalog_path=(
                target_asset_root / "fo4_target_assets.sqlite3"
            ),
            target_asset_cache_dir=target_asset_root / "target_assets",
        )

    def _resolve_install_target(self, fo4_data_dir, docs_custom_ini):
        from bacup_lib.install_targets import resolve_deploy_and_ini

        return resolve_deploy_and_ini(
            install_location=self.install_location,
            install_path=self.install_path,
            fo4_data_dir=fo4_data_dir,
            docs_custom_ini=docs_custom_ini,
            mo2_use_profile_ini=self.mo2_use_profile_ini,
        )

    def build_options(self) -> RegenOptions:
        deploy_format = getattr(self, "deploy_format", _DEFAULT_DEPLOY_FORMAT)
        deploy_loose = deploy_format == "loose"
        standard_archives = deploy_format == "standard"
        options = RegenOptions(
            deploy=self.install_location.strip().lower() != "none",
            ba2_mode="packed" if standard_archives else "expanded",
            archive_max_bytes=(
                _UNLIMITED_ARCHIVE_MAX_BYTES
                if standard_archives
                else self.archive_max_gb * 1024**3
            ),
            ba2_compression_level=getattr(self, "ba2_compression_level", None),
            deploy_loose=deploy_loose,
            workers=self.workers or None,
            asset_workers=None,
            lod_mode=self.lod_mode,
            texture_landscape_mip_flooding=bool(
                getattr(self, "texture_landscape_mip_flooding", False)
            ),
            generate_precombines=bool(
                getattr(self, "generate_precombines", False)
            ),
            re_use_land=self.re_use_land,
            write_land_cache=False,
            include_interior=True,
            records_limit=None,
            generate_anim_text_data=True,
            anim_text_data_native=True,
            direct_deploy_archives=not deploy_loose,
            update_runtime_ini=(
                self.add_archives_to_ini if deploy_format == "expanded" else True
            ),
            fo4_ba2_target=self.resolve_ba2_target(),
            memory_report=bool(getattr(self, "full_logging", False)),
        )
        manifest = self._load_upgrade_manifest_cached()
        if manifest is not None:
            options.mod_version = manifest.current
            # getattr guard: some tests construct RegenPanel via __new__
            # (bypassing __init__) and don't set the upgrade attributes.
            if getattr(self, "upgrade", False):
                options.upgrade = True
                options.hydrate_upgrade_from_deployed = True
                options.upgrade_from = None
                options.upgrade_manifest_path = self.upgrade_manifest_path()
        return options

    def load_lod_settings(
        self,
        profile: str = PROFILE_HIGH_QUALITY,
        lod_mode: str = "hybrid-atlas",
    ) -> dict:
        candidates = [
            get_code_root(),
            Path(__file__).resolve().parents[3],
        ]
        return load_profile_settings(
            candidates,
            profile=profile,
            lod_mode=lod_mode,
            pair_id=self._pair().pair_id,
        )

    def _selected_lod_settings(self, lod_mode: str) -> dict:
        settings = self.load_lod_settings(self.lod_profile, lod_mode)
        settings.setdefault("objects", {})["atlas_mip_flooding"] = bool(
            getattr(self, "atlas_mip_flooding", False)
        )
        return settings

    def _build_option_overrides(self) -> dict | None:
        excluded = required_exclude_signatures(self._pair().pair_id)
        return {"exclude_signatures": excluded} if excluded else None

    def can_convert(self) -> bool:
        if self._runner_running() or getattr(self, "_cleanup_status", "idle") == "deleting":
            return False
        s = self._settings()
        for g in self._required_game_ids():
            p = s.get_game_paths(g)
            if not p.get("root_dir"):
                return False
        for g in self._source_asset_game_ids():
            if not s.get_game_paths(g).get("extracted_dir"):
                return False
        pair = self._pair()
        if pair.merge is not None and pair.merge.grafted_plugins:
            grafted = s.get_game_paths(pair.merge.grafted_game)
            if not grafted.get("root_dir"):
                return False
        return self._store_installs_ok()

    def generated_plugin_path(self) -> Path:
        return self.build_paths().output_root / self._pair().output_plugin_name

    def can_deploy_existing(self) -> bool:
        if self._runner_running() or getattr(self, "_cleanup_status", "idle") == "deleting":
            return False
        fo4 = self._settings().get_game_paths("fo4")
        if not fo4.get("root_dir"):
            return False
        return self._store_installs_ok() and self.generated_plugin_path().is_file()

    def _store_install_result(self, game_id: str) -> StoreInstallResult:
        root = self._settings().get_game_paths(game_id).get("root_dir", "") or ""
        cached = self._store_install_cache.get(game_id)
        if cached is not None and cached[0] == root:
            return cached[1]
        result = validate_store_install_for_game(game_id, root)
        self._store_install_cache[game_id] = (root, result)
        return result

    def _detect_ba2_target(self) -> tuple[str, str | None]:
        fo4_root = self._settings().get_game_paths("fo4").get("root_dir", "") or ""
        cached = self._ba2_detect_cache
        if cached is not None and cached[0] == fo4_root:
            return cached[1]
        result = detect_ba2_target(fo4_root)
        self._ba2_detect_cache = (fo4_root, result)
        return result

    def resolve_ba2_target(self) -> str:
        if self.ba2_target in ("og", "nextgen"):
            return self.ba2_target
        target, _version = self._detect_ba2_target()
        return target

    def upgrade_manifest_path(self) -> Path:
        return bundled_upgrade_manifest_path()

    def _load_upgrade_manifest_cached(self):
        path = self.upgrade_manifest_path()
        key = str(path)
        cached = getattr(self, "_upgrade_manifest_cache", None)
        if cached is not None and cached[0] == key:
            return cached[1]
        try:
            manifest = load_upgrade_manifest(path)
        except OSError:
            manifest = None
        self._upgrade_manifest_cache = (key, manifest)
        return manifest

    def _deployed_esm_path(self) -> Path:
        paths = self.build_paths()
        return (
            paths.deploy_data_dir or paths.target_data_dir
        ) / self._pair().output_plugin_name

    def _deployed_esm_exists(self) -> bool:
        try:
            return self._deployed_esm_path().is_file()
        except Exception:
            return False

    def _detected_installed_version(self) -> str:
        esm = self._deployed_esm_path()
        try:
            if not esm.is_file():
                return "(not deployed)"
            key = (str(esm), esm.stat().st_mtime_ns)
        except OSError:
            return "(not deployed)"
        cached = self._snam_cache
        if cached is not None and cached[0] == key:
            return cached[1]
        # alpha1 shipped without a SNAM stamp, so an existing-but-unstamped ESM reads as alpha1.
        version = read_plugin_snam_header(esm) or "alpha1"
        self._snam_cache = (key, version)
        return version

    def upgrade_plan_preview(self) -> str:
        manifest = self._load_upgrade_manifest_cached()
        if manifest is None:
            return "No upgrade manifest found - full build only."
        target = manifest.current
        from_version = self._detected_installed_version()
        try:
            family_union = resolve_family_union(
                manifest, from_version, target,
                conversion_id=self._pair().pair_id,
            )
        except ValueError as exc:
            return f"Cannot resolve upgrade plan: {exc}"
        if requires_forced_regen(
            manifest,
            from_version,
            target,
            conversion_id=self._pair().pair_id,
        ):
            return "Full clean build required by this upgrade (local output will be cleared)."
        if not family_union:
            if from_version == target:
                return "Already at the current version."
            return f"No changes for {self._project_label()} in this upgrade."
        plan = resolve_upgrade_plan(family_union)
        if plan.full_build:
            return "Full build (no reuse)."
        families = ", ".join(sorted(family_union))
        output_format = {
            "expanded": "Expanded BA2s",
            "standard": "Standard BA2s",
            "loose": "loose files",
        }.get(
            getattr(self, "deploy_format", _DEFAULT_DEPLOY_FORMAT),
            "the selected format",
        )
        return (
            f"Will regenerate: {families}; reuse complete local loose assets or "
            f"restore them from the deployed BA2s; redeploy the complete mod as "
            f"{output_format}."
        )

    def _input_preflight_report(self):
        paths = self.build_paths()
        source_asset_roots = tuple(
            str(self._settings().get_game_paths(game_id).get("extracted_dir", "") or "")
            for game_id in self._source_asset_game_ids()
        )
        key = (
            self._pair().pair_id,
            str(paths.source_data_dir),
            str(paths.source_extracted_dir),
            source_asset_roots,
            str(paths.target_data_dir),
            str(paths.target_asset_catalog_path),
        )
        cached = self._preflight_cache
        if cached is not None and cached[0] == key:
            return cached[1]
        if self._is_default_pair():
            report = scan_conversion_inputs(paths)
        else:
            report = InputPreflightReport()
            for game_id, source_root_text in zip(
                self._source_asset_game_ids(),
                source_asset_roots,
                strict=True,
            ):
                source_root = Path(source_root_text) if source_root_text else None
                if source_root is not None and source_root.is_dir():
                    continue
                label = _STORE_INSTALL_LABELS.get(game_id, game_id.upper())
                report.required_missing.append(
                    MissingInput(
                        f"{label} extracted directory",
                        source_root_text or "(not configured)",
                        f"Re-run {self._project_label()} setup and extract {label} assets.",
                    )
                )
        self._preflight_cache = (key, report)
        return report

    def _store_installs_ok(self) -> bool:
        return all(
            self._store_install_result(game_id).ok
            for game_id in self._required_game_ids()
        )

    def _require_store_installs(self) -> None:
        for game_id in self._required_game_ids():
            result = self._store_install_result(game_id)
            if not result.ok:
                raise RuntimeError(result.message)

    def _start_runner(self, work) -> None:
        runner = ConversionRunner(work)
        start_runner = getattr(self._workspace, "start_conversion_runner", None)
        if callable(start_runner):
            start_runner(self, runner)
        else:
            self._workspace._runner = runner
            self._workspace._runner_owner = self
            runner.start()

    def _prepare_run_diagnostics(self, paths: RegenPaths) -> None:
        if getattr(self, "full_logging", False):
            paths.diagnostics_root = create_run_diagnostics_dir(
                get_logs_dir(),
                paths.mod_name,
            )

    def _run_logging_scope(self, paths: RegenPaths, runner: ConversionRunner):
        if paths.diagnostics_root is None:
            return contextlib.nullcontext(runner)
        return full_logging_scope(paths.diagnostics_root, runner)

    def start_conversion(self) -> None:
        self._phases = []
        self._reset_progress()
        self._runner_status = "Starting conversion"
        self._summary = None
        self._completion = None
        self._preflight_report = None

        def work(runner: ConversionRunner) -> None:
            from bacup_lib import PhaseSelection, regen_pipeline

            preparation = PhaseProgress(
                phase=0,
                phase_name="Prepare Conversion",
                total_items=4,
                current_item="Verifying game installations",
                status="running",
            )
            preparation_started = time.perf_counter()
            runner.emit_phase_start(preparation)

            def advance_preparation(completed_items: int, current_item: str) -> None:
                preparation.completed_items = completed_items
                preparation.current_item = current_item
                runner.emit_item_progress(preparation)

            try:
                self._require_store_installs()
                advance_preparation(1, "Checking required conversion inputs")

                report = self._input_preflight_report()
                advance_preparation(2, "Preparing conversion paths and options")
                if report is not None and not report.ok:
                    preparation.status = "completed"
                    preparation.elapsed_seconds = (
                        time.perf_counter() - preparation_started
                    )
                    runner.emit_phase_complete(preparation)
                    runner.emit_complete("", {"preflight_report": report})
                    return

                paths = self.build_paths()
                self._prepare_run_diagnostics(paths)
                options = self.build_options()
                pair = self._pair()
                phases = PhaseSelection()
                phases.lod_mode = options.lod_mode
                advance_preparation(3, "Loading LOD configuration")
                lod_settings = (
                    self._selected_lod_settings(options.lod_mode)
                    if options.lod_mode in {"generate", "hybrid", "hybrid-atlas"}
                    else None
                )
                advance_preparation(4, "Starting conversion")
            except Exception as exc:
                preparation.status = "error"
                preparation.error = str(exc)
                preparation.elapsed_seconds = time.perf_counter() - preparation_started
                runner.emit_phase_complete(preparation)
                raise

            if runner.is_cancelled():
                preparation.status = "cancelled"
                preparation.elapsed_seconds = time.perf_counter() - preparation_started
                runner.emit_phase_complete(preparation)
                return

            preparation.status = "completed"
            preparation.elapsed_seconds = time.perf_counter() - preparation_started
            runner.emit_phase_complete(preparation)
            self._begin_conversion_music_session()
            try:
                with self._run_logging_scope(paths, runner) as active_runner:
                    result = regen_pipeline.run_full_regen(
                        paths,
                        options,
                        pair=pair,
                        phases=phases,
                        runner=active_runner,
                        lod_settings=lod_settings,
                        build_option_overrides=self._build_option_overrides(),
                    )
                    companion_deployed = (
                        self._deploy_companion_mod(paths, active_runner)
                        if result.deployed and self._is_default_pair()
                        else []
                    )
                    if result.deployed:
                        emit_runner_status(
                            active_runner,
                            "Removing temporary conversion data",
                        )
                        self._end_conversion_music_session()
                    cleanup_removed = self._cleanup_after_deploy(paths, result.deployed)
                    active_runner.emit_complete(
                        str(result.output_root),
                        {
                            "deployed": result.deployed,
                            "exit_code": result.exit_code,
                            "elapsed_seconds": time.perf_counter() - preparation_started,
                            "companion_deployed": companion_deployed,
                            "cleanup_removed": cleanup_removed,
                        },
                    )
            finally:
                self._end_conversion_music_session()

        self._start_runner(work)

    def start_deploy_existing(self) -> None:
        from bacup_lib import regen_pipeline

        self._require_store_installs()

        self._phases = []
        self._reset_progress("deploy")
        self._runner_status = "Preparing existing mod deployment"
        self._summary = None
        self._completion = None
        paths = self.build_paths()
        options = self.build_options()

        def work(runner: ConversionRunner) -> None:
            started = time.perf_counter()
            emit_runner_status(runner, "Deploying existing converted mod")
            runner.emit_log("INFO", f"Deploying existing {self._project_label()} output...")
            result = regen_pipeline.deploy_existing(
                paths,
                options=options,
            )
            if result.failures:
                raise RuntimeError("; ".join(result.failures))
            companion_deployed = (
                self._deploy_companion_mod(paths, runner)
                if result.deployed and self._is_default_pair()
                else []
            )
            if result.deployed:
                emit_runner_status(runner, "Removing temporary conversion data")
            cleanup_removed = self._cleanup_after_deploy(paths, result.deployed)
            runner.emit_complete(
                str(result.output_root),
                {
                    "deployed": result.deployed,
                    "exit_code": result.exit_code,
                    "deploy_existing": True,
                    "elapsed_seconds": time.perf_counter() - started,
                    "companion_deployed": companion_deployed,
                    "cleanup_removed": cleanup_removed,
                },
            )

        self._start_runner(work)

    def start_resume_from_phase(self) -> None:
        from bacup_lib import PhaseSelection, regen_pipeline

        self._require_store_installs()

        self._phases = []
        self._reset_progress(_recovery_phase(self.recovery_phase))
        self._runner_status = "Preparing conversion recovery"
        self._summary = None
        self._completion = None
        paths = self.build_paths()
        self._prepare_run_diagnostics(paths)
        options = self.build_options()
        phases = PhaseSelection()
        phases.lod_mode = options.lod_mode
        start_phase = _recovery_phase(self.recovery_phase)
        lod_settings = (
            self._selected_lod_settings(options.lod_mode)
            if options.lod_mode in {"generate", "hybrid", "hybrid-atlas"}
            else None
        )

        def work(runner: ConversionRunner) -> None:
            started = time.perf_counter()
            self._begin_conversion_music_session()
            try:
                with self._run_logging_scope(paths, runner) as active_runner:
                    result = regen_pipeline.run_resume_from_phase(
                        paths,
                        options,
                        start_phase=start_phase,
                        phases=phases,
                        runner=active_runner,
                        lod_settings=lod_settings,
                    )
                    if result.failures:
                        raise RuntimeError("; ".join(result.failures))
                    companion_deployed = (
                        self._deploy_companion_mod(paths, active_runner)
                        if result.deployed and self._is_default_pair()
                        else []
                    )
                    if result.deployed:
                        emit_runner_status(
                            active_runner,
                            "Removing temporary conversion data",
                        )
                        self._end_conversion_music_session()
                    cleanup_removed = self._cleanup_after_deploy(paths, result.deployed)
                    active_runner.emit_complete(
                        str(result.output_root),
                        {
                            "deployed": result.deployed,
                            "exit_code": result.exit_code,
                            "resume_from": start_phase,
                            "elapsed_seconds": time.perf_counter() - started,
                            "companion_deployed": companion_deployed,
                            "cleanup_removed": cleanup_removed,
                        },
                    )
            finally:
                self._end_conversion_music_session()

        self._start_runner(work)

    def handle_event(self, event: dict) -> None:
        etype = event.get("type", "")
        if etype == "status":
            self._runner_status = str(event.get("message", "") or "")
        elif etype in ("phase_start", "item_progress", "phase_complete"):
            data = self._normalize_phase_data(event.get("data", {}) or {})
            phase_key = data.get("ui_key", "")
            existing = next(
                (p for p in self._phases if p.get("ui_key") == phase_key),
                None,
            )
            previous_status = existing.get("status") if existing else None
            if existing:
                if etype == "item_progress" and existing.get("status") in {
                    "completed",
                    "error",
                    "cancelled",
                }:
                    return
                existing.update(data)
            else:
                self._phases.append(dict(data))
            if (
                etype in {"phase_start", "phase_complete"}
                or existing is None
                or data.get("status", previous_status) != previous_status
            ):
                # A new scrollbar can reflow wrapped rows on the following frame.
                self._phase_scroll_frames = 2
            self._progress_estimate.update(existing or data)
        elif etype == "complete":
            summary = event.get("summary", {}) or {}
            preflight_report = summary.get("preflight_report")
            if preflight_report is not None:
                self._preflight_report = preflight_report
                return
            if "exit_code" in summary:
                self._progress_succeeded = summary["exit_code"] == 0
            deployed = bool(summary.get("deployed"))
            ini_snippet = None
            if not deployed and self.deploy_format != "standard":
                ini_snippet = self._read_ini_snippet(Path(event.get("mod_path", "")))
            self._completion = {
                "mod_path": event.get("mod_path", ""),
                "deployed": deployed,
                "deploy_existing": bool(summary.get("deploy_existing")),
                "resume_from": summary.get("resume_from"),
                "elapsed_seconds": summary.get("elapsed_seconds"),
                "ini_snippet": ini_snippet,
                "companion_deployed": summary.get("companion_deployed", []),
                "cleanup_removed": summary.get("cleanup_removed", []),
            }

    @staticmethod
    def _read_ini_snippet(mod_path: Path) -> str | None:
        snippet = mod_path / "Fallout4Custom.ini.snippet"
        try:
            return snippet.read_text(encoding="utf-8") if snippet.is_file() else None
        except OSError:
            return None

    def cleanup(self) -> None:
        self._end_conversion_music_session()
        ws = self._workspace
        owner = getattr(ws, "_runner_owner", self)
        if owner is self and ws._runner is not None and not ws._runner.done:
            ws._runner.cancel()

    @staticmethod
    def _same_path(left: Path, right: Path) -> bool:
        return os.path.normcase(os.path.abspath(left)) == os.path.normcase(os.path.abspath(right))

    def _compute_disk_usage_summary(
        self,
        paths: RegenPaths | None = None,
    ) -> dict[str, int]:
        paths = paths or self.build_paths()
        output_root = paths.output_root
        deploy_data_dir = paths.deploy_data_dir or paths.target_data_dir
        mod_ba2, deployed_ba2 = _mod_archive_sizes(
            output_root,
            deploy_data_dir,
            paths.mod_name,
        )
        return {
            "extracted": 0,
            "mod_output": mod_ba2,
            "mod_ba2": mod_ba2,
            "deployed_ba2": deployed_ba2,
        }

    def _disk_space_target(
        self,
        paths: RegenPaths,
        options: RegenOptions | None = None,
    ) -> tuple[tuple[str, str], Path]:
        deploy = (
            options.deploy
            if options is not None
            else self.install_location.strip().lower() != "none"
        )
        direct_deploy_archives = (
            options.direct_deploy_archives if options is not None else True
        )
        deploy_loose = bool(options and getattr(options, "deploy_loose", False))
        archive_root = (
            paths.deploy_data_dir or paths.target_data_dir
            if deploy and (direct_deploy_archives or deploy_loose)
            else paths.output_root
        )
        key = (
            os.path.normcase(os.path.abspath(paths.output_root)),
            os.path.normcase(os.path.abspath(archive_root)),
        )
        return key, archive_root

    def _start_disk_usage_worker(
        self,
        *,
        paths: RegenPaths | None = None,
        options: RegenOptions | None = None,
    ) -> None:
        paths = paths or self.build_paths()
        space_key, archive_root = self._disk_space_target(paths, options)
        loose_estimate, packed_estimate = _CONVERSION_SPACE_ESTIMATES[self._pair().pair_id]
        with self._disk_usage_lock:
            cached_space = getattr(self, "_disk_space_cache", None)
            cached_summary = (
                dict(self._disk_usage_cache[1])
                if self._disk_usage_cache is not None
                and getattr(self, "_disk_usage_cache_key", None) == space_key
                else None
            )
            if self._disk_usage_running:
                return
            if (
                cached_summary is not None
                and cached_space is not None
                and cached_space[0] == space_key
            ):
                return
            self._disk_usage_running = True

        def worker() -> None:
            fallback_summary = cached_summary or {
                "extracted": 0,
                "mod_output": 0,
                "mod_ba2": 0,
                "deployed_ba2": 0,
            }

            def estimates(summary: dict[str, int]) -> tuple[int, int]:
                return (
                    max(
                        loose_estimate,
                        summary["mod_output"] - summary["mod_ba2"],
                    ),
                    max(
                        packed_estimate,
                        summary["mod_ba2"],
                        summary["deployed_ba2"],
                    ),
                )

            def project(summary: dict[str, int]) -> tuple[_DiskSpaceVolume, ...]:
                loose_bytes, packed_bytes = estimates(summary)
                return _project_disk_space(
                    output_root=paths.output_root,
                    archive_root=archive_root,
                    loose_bytes=loose_bytes,
                    packed_bytes=packed_bytes,
                )

            summary = cached_summary
            if summary is None:
                try:
                    summary = self._compute_disk_usage_summary(paths)
                except Exception:
                    _log.warning("Disk usage scan failed", exc_info=True)
                    summary = fallback_summary
            try:
                projection = project(summary)
            except Exception:
                _log.warning("Available disk space scan failed", exc_info=True)
                loose_bytes, packed_bytes = estimates(summary)
                projection = _project_disk_space(
                    output_root=paths.output_root,
                    archive_root=archive_root,
                    loose_bytes=loose_bytes,
                    packed_bytes=packed_bytes,
                    disk_usage=lambda _path: None,
                )
            with self._disk_usage_lock:
                self._disk_usage_cache = (time.monotonic(), summary)
                self._disk_usage_cache_key = space_key
                self._disk_space_cache = (space_key, projection)
                self._disk_usage_running = False

        self._disk_usage_thread = threading.Thread(
            target=worker,
            name="appalachia-disk-usage",
            daemon=True,
        )
        self._disk_usage_thread.start()

    def disk_usage_summary(self) -> dict[str, int]:
        paths = self.build_paths()
        space_key, _archive_root = self._disk_space_target(paths)
        with self._disk_usage_lock:
            cached = self._disk_usage_cache
            cached_key = getattr(self, "_disk_usage_cache_key", None)
        if cached is not None and cached_key == space_key:
            return dict(cached[1])
        self._start_disk_usage_worker(paths=paths)
        return {"extracted": 0, "mod_output": 0, "mod_ba2": 0, "deployed_ba2": 0}

    def disk_usage_loading(self) -> bool:
        with self._disk_usage_lock:
            return self._disk_usage_running

    def _disk_space_projection(
        self,
        *,
        paths: RegenPaths | None = None,
        options: RegenOptions | None = None,
    ) -> tuple[_DiskSpaceVolume, ...] | None:
        paths = paths or self.build_paths()
        space_key, _archive_root = self._disk_space_target(paths, options)
        with self._disk_usage_lock:
            cached = getattr(self, "_disk_space_cache", None)
        if cached is not None and cached[0] == space_key:
            return cached[1]
        self._start_disk_usage_worker(paths=paths, options=options)
        return None

    def _finish_space_check(
        self,
        projection: tuple[_DiskSpaceVolume, ...],
    ) -> None:
        self._waiting_for_space_check = False
        low_space = tuple(volume for volume in projection if volume.insufficient)
        if low_space:
            self._low_space_warning = low_space
            return
        self._low_space_warning = None
        self.start_conversion()

    def _request_conversion(self) -> None:
        self._waiting_for_space_check = True
        self._low_space_warning = None
        with self._disk_usage_lock:
            self._disk_space_cache = None
        self._start_disk_usage_worker()

    def _resolve_pending_space_check(self) -> None:
        if not getattr(self, "_waiting_for_space_check", False):
            return
        projection = self._disk_space_projection()
        if projection is not None:
            self._finish_space_check(projection)

    def _continue_conversion_with_low_space(self) -> None:
        self._waiting_for_space_check = False
        self._low_space_warning = None
        self.start_conversion()

    def _deploy_companion_mod(
        self,
        paths: RegenPaths,
        runner: ConversionRunner | None = None,
    ) -> list[str]:
        app_root = get_exe_dir()
        companion_root = app_root / "mods" / _COMPANION_MOD_NAME
        if not companion_root.is_dir():
            raise FileNotFoundError(f"Companion mod directory not found: {companion_root}")
        companion_data_root = companion_root / "data"
        if not companion_data_root.is_dir():
            raise FileNotFoundError(
                f"Companion mod directory not found: {companion_data_root}"
            )

        deploy_data_dir = paths.deploy_data_dir or paths.target_data_dir
        deploy_data_dir.mkdir(parents=True, exist_ok=True)
        deployed: list[str] = []
        options = self.build_options()
        from creation_lib.build.loose_deploy import (
            MANIFEST_NAME,
            deploy_loose_assets,
            undeploy_loose_assets,
        )

        if (companion_root / MANIFEST_NAME).is_file():
            try:
                undeploy_loose_assets(
                    _COMPANION_MOD_NAME,
                    project_root=app_root,
                )
            except FileNotFoundError:
                (companion_root / MANIFEST_NAME).unlink(missing_ok=True)

        if runner is not None:
            if options.deploy_loose:
                emit_runner_status(runner, "Preparing loose companion mod")
                runner.emit_log(
                    "INFO",
                    f"Preparing loose companion mod {_COMPANION_MOD_NAME}...",
                )
            else:
                emit_runner_status(runner, "Packing companion mod")
                runner.emit_log(
                    "INFO",
                    f"Packing companion mod {_COMPANION_MOD_NAME}...",
                )

        companion_archives: list[Path] = []
        if options.deploy_loose:
            loose_result = deploy_loose_assets(
                _COMPANION_MOD_NAME,
                game="fo4",
                game_data_dir=paths.target_data_dir,
                deploy_data_dir=deploy_data_dir,
                skip_build=True,
                skip_validation=True,
                skip_papyrus_compile=True,
                workers=self.workers,
                project_root=app_root,
            )
            deployed.append(loose_result.plugin)
        else:
            pack_mod(
                _COMPANION_MOD_NAME,
                game="fo4",
                project_root=app_root,
                archive_max_bytes=_UNLIMITED_ARCHIVE_MAX_BYTES,
                expanded_archives=False,
                archive_workers=self.workers,
                fo4_ba2_target=options.fo4_ba2_target,
            )
            companion_archives = discover_mod_archives(
                companion_root,
                _COMPANION_MOD_NAME,
            )
            if not companion_archives:
                raise FileNotFoundError(
                    "Companion mod archive not found after packing: "
                    f"{companion_root}"
                )
        if runner is not None:
            emit_runner_status(runner, "Deploying companion mod")
            runner.emit_log("INFO", f"Deploying companion mod {_COMPANION_MOD_NAME}...")

        if not options.deploy_loose:
            for filename in _COMPANION_ROOT_FILES:
                src = companion_root / filename
                if not src.is_file():
                    raise FileNotFoundError(f"Companion mod file not found: {src}")
                dest = deploy_data_dir / filename
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dest)
                deployed.append(filename)

        companion_archive_names = {
            archive.name for archive in companion_archives
        }
        for stale_archive in discover_mod_archives(
            deploy_data_dir,
            _COMPANION_MOD_NAME,
        ):
            if (
                options.deploy_loose
                or stale_archive.name not in companion_archive_names
            ):
                stale_archive.unlink()
        if not options.deploy_loose:
            for archive in companion_archives:
                shutil.copy2(archive, deploy_data_dir / archive.name)
                deployed.append(archive.name)

        for dirname in _COMPANION_DEPLOY_DIRS:
            src_root = companion_root / dirname
            if not src_root.is_dir():
                raise FileNotFoundError(f"Companion mod directory not found: {src_root}")
            target_root = deploy_data_dir / dirname
            for src in sorted(src_root.rglob("*")):
                if not src.is_file():
                    continue
                rel = src.relative_to(src_root)
                dest = target_root / rel
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dest)
                deployed.append((Path(dirname) / rel).as_posix())

        if runner is not None:
            runner.emit_log(
                "INFO",
                f"Companion mod {_COMPANION_MOD_NAME} deployed ({len(deployed)} file(s))",
            )
        return deployed

    def _cleanup_after_deploy(self, paths: RegenPaths, deployed: bool) -> list[str]:
        if not deployed:
            return []
        settings = self._settings()
        project_id = getattr(
            self,
            "project_id",
            _PROJECT_BY_PAIR[self._pair().pair_id][0],
        )
        ownership = get_project_setup_ownership(settings, project_id)
        removed: list[str] = []
        exe_dir = get_exe_dir()
        if ownership.cleanup_mod_output:
            output_root = Path(paths.output_root)
            expected = self.build_paths().output_root
            if output_root.is_dir() and self._same_path(output_root, expected):
                shutil.rmtree(output_root)
                removed.append(str(output_root))
        if ownership.cleanup_extracted:
            extracted_root = exe_dir / "extracted"
            owned_paths: dict[str, str] = {}
            for game_id in self._source_asset_game_ids():
                extracted_dir = str(
                    settings.get_game_paths(game_id).get("extracted_dir", "") or ""
                )
                if extracted_dir and project_owns_extracted_path(
                    settings,
                    project_id,
                    game_id,
                    extracted_dir,
                    output_root=extracted_root,
                ):
                    owned_paths[game_id] = extracted_dir
            cleared_games = clear_project_owned_extractions(
                settings,
                project_id,
                output_root=extracted_root,
            )
            removed.extend(
                owned_paths[game_id]
                for game_id in cleared_games
                if game_id in owned_paths
            )
        return removed

    def _ensure_cleanup_state(self) -> None:
        if hasattr(self, "_cleanup_lock"):
            return
        self._cleanup_lock = threading.Lock()
        self._cleanup_dialog_open = False
        self._cleanup_status = "idle"
        self._cleanup_targets = ()
        self._cleanup_selected = set()
        self._cleanup_pending_result = None
        self._cleanup_disk_refresh_pending = False
        self._cleanup_message = None
        self._cleanup_error = None
        self._admin_restart_error = None
        self._is_admin = is_running_as_admin()

    def _open_cleanup_dialog(self) -> None:
        self._ensure_cleanup_state()
        if self._runner_running() or self._cleanup_status != "idle":
            return
        settings = self._settings()
        fo4 = settings.get_game_paths("fo4")
        fo76 = settings.get_game_paths("fo76")
        fo4_root = str(fo4.get("root_dir", "") or "")
        fo76_root = str(fo76.get("root_dir", "") or "")
        forbidden = [get_exe_dir(), Path.home()]
        for root in (fo4_root, fo76_root):
            if root:
                forbidden.extend((Path(root), _data_dir(root)))
        fo4_extracted = str(fo4.get("extracted_dir", "") or "")
        fo76_extracted = str(fo76.get("extracted_dir", "") or "")

        with self._cleanup_lock:
            self._cleanup_dialog_open = True
            self._cleanup_status = "scanning"
            self._cleanup_targets = ()
            self._cleanup_selected.clear()
            self._cleanup_message = None
            self._cleanup_error = None

        def worker() -> None:
            try:
                targets = discover_cleanup_targets(
                    fo4_extracted_dir=(Path(fo4_extracted) if fo4_extracted else None),
                    fo76_extracted_dir=(Path(fo76_extracted) if fo76_extracted else None),
                    forbidden_roots=forbidden,
                    game_roots=tuple(Path(root) for root in (fo4_root, fo76_root) if root),
                )
                targets = measure_cleanup_targets(targets)
                error = None
            except Exception as exc:
                _log.warning("Cleanup target scan failed", exc_info=True)
                targets = ()
                error = str(exc)
            with self._cleanup_lock:
                self._cleanup_targets = targets
                self._cleanup_error = error
                self._cleanup_status = "idle"

        threading.Thread(
            target=worker,
            name="bacup-cleanup-scan",
            daemon=True,
        ).start()

    def _start_cleanup_delete(self) -> None:
        self._ensure_cleanup_state()
        if self._runner_running() or self._cleanup_status != "idle":
            return
        with self._cleanup_lock:
            selected = tuple(
                target
                for target in self._cleanup_targets
                if target.key in self._cleanup_selected
            )
            if not selected:
                return
            self._cleanup_status = "deleting"
            self._cleanup_message = None
            self._cleanup_error = None

        def worker() -> None:
            result = delete_cleanup_targets(selected)
            with self._cleanup_lock:
                self._cleanup_pending_result = result
                self._cleanup_status = "idle"

        threading.Thread(
            target=worker,
            name="bacup-cleanup-delete",
            daemon=True,
        ).start()

    def _poll_cleanup_result(self) -> None:
        self._ensure_cleanup_state()
        if self._cleanup_disk_refresh_pending:
            with self._disk_usage_lock:
                disk_scan_running = self._disk_usage_running
                if not disk_scan_running:
                    self._disk_usage_cache = None
                    self._disk_usage_cache_key = None
                    self._disk_space_cache = None
                    self._cleanup_disk_refresh_pending = False
            if not disk_scan_running:
                self._start_disk_usage_worker()
        with self._cleanup_lock:
            result = self._cleanup_pending_result
            self._cleanup_pending_result = None
        if result is None:
            return
        if "fo4_extracted" in result.deleted_keys:
            self._settings().set_game_extracted_dir("fo4", "")
        with self._cleanup_lock:
            deleted = set(result.deleted_keys)
            self._cleanup_targets = tuple(
                target for target in self._cleanup_targets if target.key not in deleted
            )
            self._cleanup_selected.clear()
            self._cleanup_message = (
                f"Freed {_format_gb(result.freed_bytes)} from "
                f"{len(result.removed_paths)} path(s)."
            )
            self._cleanup_error = "\n".join(result.failures) or None
        self._cleanup_disk_refresh_pending = True

    def _restart_elevated(self) -> None:
        self._ensure_cleanup_state()
        try:
            restart_as_admin()
        except OSError as exc:
            self._admin_restart_error = str(exc)
            return
        from imgui_bundle import hello_imgui

        hello_imgui.get_runner_params().app_shall_exit = True

    def draw(self) -> None:  # imgui rendering; covered by launch smoke, not unit tests
        if not imgui.begin(f"B.A.C.U.P.{_NS}"):
            imgui.end()
            return
        imgui.text_disabled("Bethesda Asset Converter Universal Platform")
        self.draw_project()
        imgui.end()

    def draw_project(self) -> None:
        self._poll_cleanup_result()
        self._resolve_pending_space_check()
        self._draw_header()
        self._draw_split()
        self._draw_preflight_modal()
        self._draw_low_space_modal()
        self._draw_cleanup_dialog()
        self._draw_completion_popup()

    def _draw_header(self) -> None:
        from ui.toolkit.app_paths import get_resource_dir

        logo = get_resource_dir() / "bacup" / "projects" / f"{self.project_id}.png"
        pos, available = imgui.get_cursor_screen_pos(), imgui.get_content_region_avail()
        logo_width = min(scaled(300), available.x * .32) if logo.is_file() else 0
        if logo_width:
            image_in_box(logo, (pos.x + available.x - logo_width, pos.y - scaled(4)),
                         (logo_width, scaled(92)), trim_padding=True)
            imgui.push_text_wrap_pos(imgui.get_cursor_pos_x() + available.x - logo_width - scaled(20))
        heading(self._project_label(), size=30)
        profile = get_project_profile(self.project_id)
        imgui.text_disabled(f"{profile.source_label}  →  Fallout 4")
        _, exe_version = self._detect_ba2_target()
        pair = self._pair()
        target_root = (
            self._settings().get_game_paths(pair.target_game).get("root_dir", "")
            or "(not set)"
        )
        exe_display = exe_version or f"unknown (checked: {target_root})"
        if self._is_default_pair():
            installed = self._detected_installed_version()
            status_indicator(
                f"Installed: {installed}    |    Game (Fallout4.exe): {exe_display}"
            )
        else:
            status_indicator(
                f"Pair: {pair.pair_id}    |    Game (Fallout4.exe): {exe_display}"
            )
        if logo_width:
            imgui.pop_text_wrap_pos()
        imgui.spacing()

    def _draw_overall_progress(self) -> None:
        running = self._owns_active_runner()
        if not running and not self._phases and not self._progress_succeeded:
            return
        fraction, message = self._runner_progress()
        imgui.progress_bar(
            fraction, imgui.ImVec2(-1, 0),
            f"Overall progress{' (estimated)' if fraction < 1.0 else ''}: {fraction:.0%}",
        )
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "Weighted by approximate phase durations. Parallel phases count "
                "independently; 100% means conversion and final cleanup succeeded."
            )
        if running:
            imgui.text_wrapped(message)
        elif self._progress_succeeded:
            imgui.text_wrapped(message)
        else:
            imgui.text_disabled("Conversion stopped before completion")
        imgui.separator()

    def _draw_split(self) -> None:
        flags = (
            imgui.TableFlags_.resizable.value
            | imgui.TableFlags_.borders_inner_v.value
        )
        if not imgui.begin_table(f"{_NS}_split", 2, flags):
            return
        imgui.table_setup_column(
            "Settings", imgui.TableColumnFlags_.width_stretch.value, 0.67
        )
        imgui.table_setup_column(
            "Status", imgui.TableColumnFlags_.width_stretch.value, 0.33
        )
        imgui.table_next_row()
        imgui.table_set_column_index(0)
        available = imgui.get_content_region_avail()
        spacing = imgui.get_style().item_spacing
        action_rows = 1 if available.x >= scaled(330) + spacing.x else 2
        action_height = action_rows * (scaled(52) + spacing.y)
        hint = self._deploy_existing_hint()
        if hint:
            action_height += spacing.y + imgui.calc_text_size(hint, wrap_width=max(1, available.x)).y
        imgui.push_style_color(imgui.Col_.child_bg, semantic_color("background"))
        settings_visible = imgui.begin_child(
            f"{_NS}_settings_pane", imgui.ImVec2(0, max(scaled(60), available.y - action_height)),
        )
        imgui.pop_style_color()
        if settings_visible:
            self._draw_settings_column()
        imgui.end_child()
        self._draw_actions()
        imgui.table_set_column_index(1)
        status_flags = (
            imgui.WindowFlags_.no_scrollbar.value
            | imgui.WindowFlags_.no_scroll_with_mouse.value
        )
        imgui.push_style_color(imgui.Col_.child_bg, semantic_color("background"))
        status_visible = imgui.begin_child(f"{_NS}_status_pane", window_flags=status_flags)
        imgui.pop_style_color()
        if status_visible:
            self._draw_overall_progress()
            self._draw_status_column()
        imgui.end_child()
        imgui.end_table()

    def _draw_status_column(self) -> None:
        from bacup_ui.conversion.widgets import draw_phase_progress

        running = self._owns_active_runner()
        phase_rows = self._phase_rows(self._phases)
        show_logs = getattr(self._workspace, "show_logs", False)

        avail = imgui.get_content_region_avail()
        try:
            avail_y = float(avail.y)
        except (TypeError, ValueError):
            avail_y = 0.0
        splitter_h = scaled(6)
        item_spacing_y = float(imgui.get_style().item_spacing.y)
        log_min = min(scaled(200), avail_y * .60)
        log_h = max(log_min, self._status_log_frac * avail_y)
        top_h = max(
            scaled(80),
            avail_y - log_h - splitter_h - (2.0 * item_spacing_y),
        ) if show_logs else 0

        visible = imgui.begin_child(f"progress{_NS}", imgui.ImVec2(0, top_h), imgui.ChildFlags_.borders)
        heading("Run status")
        if not running and not self._phases:
            imgui.text_wrapped("Ready to convert" if self.can_convert() else "Complete setup to convert")
            imgui.text_disabled("No conversion running")
        imgui.separator()
        if not phase_rows:
            imgui.text_disabled("Phases appear here as they start.")
        draw_phase_progress(_NS, phase_rows, self._phases)
        if visible and phase_rows and self._phase_scroll_frames:
            imgui.set_scroll_here_y(1.0)
            self._phase_scroll_frames -= 1
        imgui.end_child()

        if not show_logs:
            return

        imgui.invisible_button(f"status_splitter{_NS}", imgui.ImVec2(-1, splitter_h))
        if imgui.is_item_active() and avail_y > 0.0:
            try:
                delta_y = float(imgui.get_io().mouse_delta.y)
            except (TypeError, ValueError):
                delta_y = 0.0
            new_log_h = min(max(log_h - delta_y, log_min), 0.75 * avail_y)
            self._status_log_frac = new_log_h / avail_y
        if imgui.is_item_hovered():
            imgui.set_mouse_cursor(imgui.MouseCursor_.resize_ns)

        imgui.begin_child(f"log{_NS}", imgui.ImVec2(0, log_h), imgui.ChildFlags_.borders)
        heading("Conversion log")
        if self._log_panel is not None:
            self._log_panel.draw_body()
        imgui.end_child()

    def _draw_music_controls(self) -> None:
        from imgui_bundle import icons_fontawesome_6 as fa

        running = self._owns_active_runner()
        music_enabled = not bool(getattr(self, "music_muted", True))
        changed, music_enabled = toggle(
            f"Conversion music{_NS}",
            music_enabled,
        )
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "Plays themes extracted from the selected source game during "
                "conversion. Music and volume preferences are remembered."
            )
        if changed:
            self._set_music_muted(not music_enabled)
        player = getattr(self, "_music_player", None)
        if music_enabled and player is not None:
            music_state = player.snapshot()
            imgui.push_text_wrap_pos(0)
            if music_state.current_track is not None:
                imgui.text_disabled(
                    f"{music_state.track_index + 1}/{music_state.track_count}  ·  "
                    f"{music_state.current_track.stem}"
                )
            elif running:
                imgui.text_disabled("Preparing source-game soundtrack...")
            else:
                imgui.text_disabled("Playback starts with conversion.")

            imgui.pop_text_wrap_pos()

            transport_disabled = not music_state.active
            if transport_disabled:
                imgui.begin_disabled()
            if imgui.button(
                f"{getattr(fa, 'ICON_FA_BACKWARD_STEP', '|<')}{_NS}_music_previous"
            ):
                player.previous()
            if imgui.is_item_hovered():
                imgui.set_tooltip("Previous track")
            imgui.same_line()
            play_icon = (
                getattr(fa, "ICON_FA_PLAY", ">")
                if music_state.paused
                else getattr(fa, "ICON_FA_PAUSE", "||")
            )
            if imgui.button(f"{play_icon}{_NS}_music_pause"):
                if music_state.paused:
                    player.resume()
                else:
                    player.pause()
            if imgui.is_item_hovered():
                imgui.set_tooltip("Resume" if music_state.paused else "Pause")
            imgui.same_line()
            if imgui.button(
                f"{getattr(fa, 'ICON_FA_FORWARD_STEP', '>|')}{_NS}_music_next"
            ):
                player.next()
            if imgui.is_item_hovered():
                imgui.set_tooltip("Next track")
            if transport_disabled:
                imgui.end_disabled()

            imgui.text_disabled("Volume")
            imgui.same_line()
            imgui.set_next_item_width(-1)
            changed, music_volume = imgui.slider_float(
                f"##conversion_music_volume{_NS}",
                music_state.volume * 100.0,
                0.0,
                100.0,
                format="%.0f%%",
            )
            if changed:
                self._set_music_volume(music_volume / 100.0)

            timeline_disabled = (
                not music_state.active or music_state.duration_seconds <= 0.0
            )
            if timeline_disabled:
                imgui.begin_disabled()
            imgui.text_disabled(
                f"{format_music_time(music_state.position_seconds)} / "
                f"{format_music_time(music_state.duration_seconds)}"
            )
            imgui.set_next_item_width(-1)
            changed, music_progress = imgui.slider_float(
                f"##conversion_music_timeline{_NS}",
                music_state.progress,
                0.0,
                1.0,
                format="",
            )
            if changed:
                player.seek(music_progress)
            if timeline_disabled:
                imgui.end_disabled()

    def _draw_settings_column(self) -> None:
        for identifier, title, draw in (
            ("storage", "Storage and maintenance", self._draw_storage),
            ("options", "Conversion settings", self._draw_options),
        ):
            with section(f"{identifier}{_NS}", title) as visible:
                if visible:
                    draw()
        with expandable_section(f"Game install information{_NS}",
            description=f"{get_project_profile(self.project_id).source_label} → Fallout 4") as expanded:
            if expanded:
                self._draw_install_information()
        self._draw_advanced()

    def _draw_install_information(self) -> None:
        from creation_lib.core.game_profiles import GAME_PROFILES

        games = self._required_game_ids()
        if imgui.begin_table(f"{_NS}_install_summary", len(games), imgui.TableFlags_.sizing_stretch_same):
            for game_id in games:
                imgui.table_next_column()
                imgui.text_wrapped(GAME_PROFILES[game_id].display_name)
                result = self._store_install_result(game_id)
                imgui.text_colored(semantic_color("success" if result.ok else "error"),
                                   "Verified" if result.ok else "Setup required")
                if imgui.is_item_hovered() and not result.ok:
                    imgui.set_tooltip(result.message)
            imgui.end_table()
        self._draw_install_details()

    def _draw_install_details(self) -> None:
        s = self._settings()
        pair = self._pair()
        paths = self.build_paths()
        install_target = self._resolve_install_target(paths.target_data_dir, paths.target_custom_ini_path)

        if begin_form(f"{_NS}_diag", 200):
            _form_row_label(f"{pair.source_game.upper()} source")
            imgui.text_wrapped(
                s.get_game_paths(pair.source_game).get("extracted_dir", "?")
            )
            _form_row_label(f"{pair.target_game.upper()} target")
            imgui.text_wrapped(s.get_game_paths(pair.target_game).get("root_dir", "?"))
            for game_id in self._required_game_ids():
                store_result = self._store_install_result(game_id)
                label = _STORE_INSTALL_LABELS.get(game_id, game_id.upper())
                _form_row_label(f"{label} store install")
                if store_result.ok:
                    store_name = "GOG" if store_result.store == "gog" else "Steam"
                    imgui.text_colored(semantic_color("success"), f"verified ({store_name})")
                else:
                    imgui.text_colored(semantic_color("error"), store_result.message)
            deploy_data_dir = paths.deploy_data_dir or paths.target_data_dir
            _form_row_label("Deploy folder")
            imgui.text_wrapped(f"{deploy_data_dir}")
            if self.deploy_format != "standard":
                _form_row_label("INI")
                imgui.text(f"{install_target.runtime_ini_path or '(none)'}")
            show_install_warning = install_target.warning and (
                self.deploy_format != "standard"
                or "ini" not in install_target.warning.lower()
            )
            if show_install_warning:
                _form_row_label("Warning")
                imgui.text_colored(semantic_color("warning"), install_target.warning)
            end_form()

    def _draw_storage(self) -> None:
        running = self._runner_running()
        paths = self.build_paths()
        sizes = self.disk_usage_summary()
        if self.disk_usage_loading():
            archive_detail = "Checking built archives..."
        else:
            packed_now = sizes["mod_ba2"] + sizes["deployed_ba2"]
            archive_detail = f"Built BA2s: {_format_gb(packed_now)}"
        if self._is_default_pair():
            archive_detail += (
                "\n"
                f"Footprint guide: FO76 ~{_format_gb(_FO76_INSTALL_REFERENCE_BYTES)} already installed; "
                f"loose workspace ~{_format_gb(_LOOSE_WORKSPACE_PEAK_BYTES)}; "
                f"packed mod ~{_format_gb(_PACKED_MOD_PEAK_BYTES)}. "
                "Projected use includes the additional conversion reserve."
            )
        else:
            loose_estimate, packed_estimate = _CONVERSION_SPACE_ESTIMATES[self._pair().pair_id]
            archive_detail += (
                f"\nFootprint guide: loose workspace ~{_format_gb(loose_estimate)}; "
                f"packed mod ~{_format_gb(packed_estimate)}. "
                "Based on an existing conversion, with 25% headroom and rounded up. "
                "Projected use includes the additional conversion reserve."
            )
        self._draw_storage_charts(paths, archive_detail)
        self._ensure_cleanup_state()
        cleanup_status = self._cleanup_status
        cleanup_busy = cleanup_status != "idle"
        restart_disabled = running or cleanup_status == "deleting"
        cleanup_disabled = running or cleanup_busy
        show_ini_controls = self.deploy_format != "standard"
        if imgui.begin_table(
            f"{_NS}_install_maintenance",
            3 if show_ini_controls else 2,
            imgui.TableFlags_.sizing_stretch_same.value,
        ):
            imgui.table_next_row()
            imgui.table_next_column()
            if self._is_admin:
                imgui.begin_disabled()
                imgui.button(
                    f"Running as administrator{_NS}", imgui.ImVec2(-1, 0)
                )
                imgui.end_disabled()
            else:
                if restart_disabled:
                    imgui.begin_disabled()
                if imgui.button(
                    f"Restart as administrator{_NS}", imgui.ImVec2(-1, 0)
                ):
                    self._restart_elevated()
                if restart_disabled:
                    imgui.end_disabled()
            if show_ini_controls:
                imgui.table_next_column()
                if running:
                    imgui.begin_disabled()
                if imgui.button(
                    f"Check / repair INI{_NS}_audit", imgui.ImVec2(-1, 0)
                ):
                    self._run_install_audit()
                if running:
                    imgui.end_disabled()
            imgui.table_next_column()
            if cleanup_disabled:
                imgui.begin_disabled()
            if imgui.button(
                f"Free up space...{_NS}", imgui.ImVec2(-1, 0)
            ):
                self._open_cleanup_dialog()
            if cleanup_disabled:
                imgui.end_disabled()
            imgui.end_table()
        if self._admin_restart_error:
            imgui.text_colored(semantic_color("error"), self._admin_restart_error)
        if show_ini_controls:
            self._draw_install_audit()

    def _draw_storage_charts(self, paths: RegenPaths, archive_detail: str) -> None:
        projection = self._disk_space_projection(paths=paths)
        multiple_drives = projection is not None and len(projection) > 1
        columns = 2 if imgui.get_content_region_avail().x >= scaled(500) else 1
        if not imgui.begin_table(f"storage_charts{_NS}", columns, imgui.TableFlags_.sizing_stretch_same):
            return
        for index, volume in enumerate(projection or (None,)):
            measured = volume is not None and not volume.unavailable and volume.total_bytes > 0
            estimated = measured
            current = (volume.total_bytes - volume.free_bytes) / volume.total_bytes if measured else None
            projected = volume.projected_fill_fraction if estimated else None
            drive = (volume.key.upper() or volume.path.anchor) if volume is not None else ""
            suffix = f" · {drive}" if drive else ""
            current_detail = f"{_format_gb(volume.free_bytes)} free" if measured else "Measuring..." if volume is None else "Space unavailable"
            if estimated:
                remaining = volume.free_bytes - volume.required_bytes
                projected_detail = (
                    f"{_format_gb(volume.required_bytes)} estimated\n"
                    + (f"{_format_gb(remaining)} free after" if remaining >= 0 else f"{_format_gb(-remaining)} short")
                )
                role = {"green": "success", "yellow": "warning", "red": "error"}[volume.space_level]
            else:
                projected_detail = "Measuring..." if volume is None else "Estimate unavailable"
                role = "muted"
            tooltip = archive_detail
            if volume is not None:
                tooltip += f"\n{volume.path}\n{', '.join(volume.labels)}"
                if measured:
                    tooltip += f"\nTotal capacity: {_format_gb(volume.total_bytes)}"
                    tooltip += f"\nCurrent use: {current:.0%}"
                if estimated:
                    tooltip += f"\nAdditional conversion reserve: {_format_gb(volume.required_bytes)}"
            imgui.push_id(index)
            if multiple_drives:
                imgui.table_next_column()
                if estimated:
                    detail = (f"{_format_gb(volume.free_bytes)} free now\n"
                              + (f"{_format_gb(remaining)} free after" if remaining >= 0
                                 else f"{_format_gb(-remaining)} short"))
                else:
                    detail = current_detail + "\nEstimate unavailable"
                ring_stat("drive", f"{drive} · {'projected use' if estimated else 'current use'}",
                          projected if estimated else current, detail=detail,
                          role=role if estimated else "accent" if measured else "muted", tooltip=tooltip)
            else:
                imgui.table_next_column()
                ring_stat("current", f"Current use{suffix}", current, detail=current_detail, tooltip=tooltip)
                imgui.table_next_column()
                ring_stat("projected", f"After conversion{suffix}", projected,
                          detail=projected_detail, role=role, tooltip=tooltip)
            imgui.pop_id()
        imgui.end_table()

    def _draw_options(self) -> None:
        running = self._runner_running()

        if running:
            imgui.begin_disabled()

        if begin_form(f"{_NS}_settings", 200):
            if getattr(self, "fixed_pair_id", None) is None:
                pair_ids = sorted(SOURCE_PAIRS)
                pair_idx = pair_ids.index(self.pair_id) if self.pair_id in pair_ids else 0
                _form_row_label("Conversion pair")
                imgui.set_next_item_width(-1)
                changed, pair_idx = imgui.combo(
                    f"##conversion_pair{_NS}", pair_idx, pair_ids
                )
                if changed:
                    self.pair_id = pair_ids[pair_idx]
                    self.upgrade = False
                    self._upgrade_user_toggled = False
                    self._preflight_report = None
                    self._preflight_cache = None
            playtest_root = self._fo76_playtest_root()
            if self._pair().pair_id == DEFAULT_PAIR_ID and playtest_root:
                source_idx = _FO76_SOURCE_VALUES.index(self.fo76_source)
                changed, source_idx = draw_combo_field(
                    "FO76 source", _FO76_SOURCE_LABELS, source_idx
                )
                if changed:
                    self.fo76_source = _FO76_SOURCE_VALUES[source_idx]
                    self._preflight_report = None
                    self._preflight_cache = None
                    self._set_workspace_settings(
                        {_FO76_SOURCE_KEY: self.fo76_source}
                    )
            install_idx = (
                _INSTALL_LOCATION_VALUES.index(self.install_location)
                if self.install_location in _INSTALL_LOCATION_VALUES
                else 0
            )
            changed, install_idx = draw_combo_field(
                "Deploy To:", _INSTALL_LOCATION_LABELS, install_idx
            )
            if changed:
                self.install_location = _INSTALL_LOCATION_VALUES[install_idx]
                self.deploy = self.install_location != "none"
                self._set_workspace_settings({_INSTALL_LOCATION_KEY: self.install_location})
            if self.install_location in ("mo2", "vortex"):
                _, clicked = draw_path_row("Install folder", self.install_path)
                if clicked:
                    picked = _pick_folder("Select install folder", self.install_path)
                    if picked:
                        self.install_path = os.path.normpath(picked)
                        self._set_workspace_settings({"install_path": self.install_path})
            deploy_mode = "loose" if self.deploy_format == "loose" else "packed"
            deploy_mode_idx = _DEPLOY_MODE_VALUES.index(deploy_mode)
            changed, deploy_mode_idx = draw_combo_field(
                "Assets",
                _DEPLOY_MODE_LABELS,
                deploy_mode_idx,
            )
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Packed writes standard BA2 archives. Loose copies the converted "
                    "asset tree directly into the install folder. Advanced archive "
                    "layout controls are available below."
                )
            if changed:
                self.deploy_format = (
                    "loose"
                    if _DEPLOY_MODE_VALUES[deploy_mode_idx] == "loose"
                    else _DEFAULT_DEPLOY_FORMAT
                )
                self._set_workspace_settings(
                    {_DEPLOY_FORMAT_KEY: self.deploy_format}
                )
            if self.install_location == "mo2" and self.deploy_format != "standard":
                _form_row_label("MO2 profile INI")
                changed, self.mo2_use_profile_ini = toggle(
                    "##mo2ini", self.mo2_use_profile_ini
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Off = register archives in the global Documents\\My Games\\Fallout4"
                        "\\Fallout4Custom.ini instead (for MO2 profiles that don't use "
                        "profile-specific game INIs)."
                    )
                if changed:
                    self._set_workspace_settings(
                        {"mo2_use_profile_ini": self.mo2_use_profile_ini}
                    )
            if self.deploy_format != "standard":
                ini_label = (
                    "Update MO2 Custom.ini"
                    if self.install_location == "mo2" and self.mo2_use_profile_ini
                    else "Update Fallout4Custom.ini"
                )
                _form_row_label(ini_label)
                manages_archive_registration = self.deploy_format == "expanded"
                if not manages_archive_registration:
                    imgui.begin_disabled()
                changed, add_archives_to_ini = toggle(
                    "##addba2ini",
                    self.add_archives_to_ini if manages_archive_registration else False,
                )
                if manages_archive_registration and changed:
                    self.add_archives_to_ini = add_archives_to_ini
                if not manages_archive_registration:
                    imgui.end_disabled()
                    if imgui.is_item_hovered():
                        imgui.set_tooltip(
                            "Loose deployment does not register BA2 archives. Existing "
                            "entries for this mod are removed automatically when possible."
                        )
            target_idx = (
                _BA2_TARGET_VALUES.index(self.ba2_target)
                if self.ba2_target in _BA2_TARGET_VALUES
                else 0
            )
            changed, target_idx = draw_combo_field(
                "BA2 target", _BA2_TARGET_LABELS, target_idx
            )
            if changed:
                self.ba2_target = _BA2_TARGET_VALUES[target_idx]
                self._set_workspace_settings({_BA2_TARGET_KEY: self.ba2_target})
            pair_id = self._pair().pair_id
            profile_values = _lod_profile_values(pair_id)
            if profile_values:
                profile_idx = (
                    profile_values.index(self.lod_profile)
                    if self.lod_profile in profile_values
                    else 0
                )
                changed, profile_idx = draw_combo_field(
                    "LOD quality",
                    [PROFILE_LABELS[p] for p in profile_values],
                    profile_idx,
                )
                self.lod_profile = profile_values[profile_idx]
                if changed:
                    self._set_workspace_settings({_LOD_PROFILE_KEY: self.lod_profile})
            else:
                _form_row_label("LOD quality")
                imgui.text_disabled("Engine defaults")
            self.lod_mode = _default_lod_mode(pair_id)
            if required_exclude_signatures(pair_id):
                _form_row_label("Record scope")
                fnv_quest_gate = pair_id == "fnvfo3:fo4"
                imgui.text_disabled(
                    "All non-quest records" if fnv_quest_gate else "World only"
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Only quest, dialogue, and scene runtime records remain gated."
                        if fnv_quest_gate
                        else "Actors, quests, dialogue, weapons, and packages are "
                        "excluded. This pair converts world content only for now."
                    )
            _form_row_label("Workers")
            imgui.set_next_item_width(-1)
            max_workers = max(1, (os.cpu_count() or 2) - 1)
            changed, self.workers = imgui.slider_int(
                "##workers", self.workers, 0, max_workers
            )
            if changed:
                self._set_workspace_settings({_WORKERS_KEY: self.workers})
            _form_row_label("")
            imgui.text_disabled(self._worker_rec.note)
            if not self._upgrade_user_toggled:
                self.upgrade = self._deployed_esm_exists()
            _form_row_label("Upgrade mode")
            changed, self.upgrade = toggle(
                f"Upgrade existing deployment{_NS}", self.upgrade
            )
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Reuses the BACUP workspace when all eight loose-asset folders "
                    "are present; otherwise extracts the currently deployed BA2s. "
                    "Then converts changed assets and redeploys the complete mod "
                    "using the selected BA2 format."
                )
            if changed:
                self._upgrade_user_toggled = True
            manifest = self._load_upgrade_manifest_cached()
            _form_row_label("")
            if manifest is None:
                imgui.text_disabled("No upgrade manifest found - full build only.")
            else:
                detected = self._detected_installed_version()
                imgui.text_disabled(f"Detected installed: {detected}")
                if self.upgrade:
                    imgui.text_disabled(f"Target: {manifest.current} (this build)")
                    imgui.text_disabled(self.upgrade_plan_preview())
            end_form()
        if running:
            imgui.end_disabled()

    def _draw_actions(self) -> None:
        from imgui_bundle import icons_fontawesome_6 as fa

        ws = self._workspace
        running = self._runner_running()

        can_start_conversion = self.can_convert()
        can_deploy_existing = self.can_deploy_existing()
        available = imgui.get_content_region_avail().x
        gap = imgui.get_style().item_spacing.x
        side_by_side = available >= scaled(330) + gap
        convert_width = min(scaled(200), available - gap - scaled(220) if side_by_side else available)
        secondary_width = min(scaled(220), available)
        button_height = scaled(52)
        if action_button(
            f"Convert{_NS}", primary=True, width=convert_width, height=button_height,
            icon=fa.ICON_FA_PLAY, enabled=can_start_conversion,
        ):
            self._request_conversion()
        if side_by_side:
            imgui.same_line()
        if running and self._owns_active_runner():
            if action_button(f"Cancel{_NS}", width=secondary_width, height=button_height,
                             icon=fa.ICON_FA_STOP):
                ws._runner.cancel()
        elif action_button(
            f"Deploy existing mod{_NS}", width=secondary_width, height=button_height,
            icon=fa.ICON_FA_FOLDER_OPEN, enabled=can_deploy_existing,
        ):
            self.start_deploy_existing()
        hint = self._deploy_existing_hint()
        if hint:
            imgui.text_wrapped(hint)

    def _deploy_existing_hint(self) -> str:
        if self.generated_plugin_path().is_file():
            return ""
        return f"Deploy existing mod is available after {self._pair().output_plugin_name} exists."

    def _draw_advanced(self) -> None:
        running = self._runner_running()

        with expandable_section(f"Advanced{_NS}",
            description="Archive layout, compression & textures") as expanded:
            if expanded:
                if running:
                    imgui.begin_disabled()
                if self.deploy_format != "loose":
                    archive_layout_idx = (
                        _ARCHIVE_LAYOUT_VALUES.index(self.deploy_format)
                        if self.deploy_format in _ARCHIVE_LAYOUT_VALUES
                        else 0
                    )
                    imgui.set_next_item_width(scaled(220))
                    changed, archive_layout_idx = imgui.combo(
                        f"BA2 layout{_NS}",
                        archive_layout_idx,
                        _ARCHIVE_LAYOUT_LABELS,
                    )
                    if imgui.is_item_hovered():
                        imgui.set_tooltip(
                            "Standard writes one Main and one Textures BA2 and should "
                            "almost always be used. Expanded creates family archives "
                            "and requires runtime INI registration."
                        )
                    if changed:
                        self.deploy_format = _ARCHIVE_LAYOUT_VALUES[archive_layout_idx]
                        self._set_workspace_settings(
                            {_DEPLOY_FORMAT_KEY: self.deploy_format}
                        )
                    if self.deploy_format == "expanded":
                        imgui.set_next_item_width(scaled(220))
                        changed, archive_max_gb = imgui.slider_int(
                            f"Max BA2 size (GiB){_NS}",
                            self.archive_max_gb,
                            _ARCHIVE_MIN_GB,
                            _ARCHIVE_MAX_GB,
                        )
                        if changed:
                            self.archive_max_gb = _archive_max_gb(archive_max_gb)
                            self._set_workspace_settings(
                                {_ARCHIVE_MAX_GB_KEY: self.archive_max_gb}
                            )
                compression_level = getattr(self, "ba2_compression_level", None)
                compression_idx = (
                    _BA2_COMPRESSION_VALUES.index(compression_level)
                    if compression_level in _BA2_COMPRESSION_VALUES
                    else 0
                )
                imgui.set_next_item_width(scaled(220))
                changed, compression_idx = imgui.combo(
                    f"BA2 compression{_NS}",
                    compression_idx,
                    _BA2_COMPRESSION_LABELS,
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Higher levels make smaller BA2s but take longer to pack. "
                        "Default preserves the format-specific levels (general 6, textures 4)."
                    )
                if changed:
                    self.ba2_compression_level = _BA2_COMPRESSION_VALUES[compression_idx]
                    self._set_workspace_settings(
                        {_BA2_COMPRESSION_KEY: self.ba2_compression_level}
                    )
                changed, self.atlas_mip_flooding = toggle(
                    f"Atlas mip flooding{_NS}",
                    self.atlas_mip_flooding,
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Flood atlas base RGB through transparent pixels in Rust before normal DDS "
                        "mip generation. Alpha-bearing BC1 atlases are promoted to BC3 so flooded "
                        "RGB survives compression; infinite dilation is the fallback."
                    )
                if changed:
                    self._set_workspace_settings(
                        {_ATLAS_MIP_FLOODING_KEY: self.atlas_mip_flooding}
                    )
                changed, self.texture_landscape_mip_flooding = toggle(
                    f"Landscape diffuse mip flooding{_NS}",
                    self.texture_landscape_mip_flooding,
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Flood converted landscape *_d.dds base RGB through transparent pixels "
                        "before normal DDS mip generation. Alpha-bearing BC1 inputs are promoted "
                        "to BC3; effects and other texture folders are not changed."
                    )
                if changed:
                    self._set_workspace_settings(
                        {
                            _TEXTURE_LANDSCAPE_MIP_FLOODING_KEY: (
                                self.texture_landscape_mip_flooding
                            )
                        }
                    )
                changed, self.generate_precombines = toggle(
                    f"Generate precombines (experimental){_NS}",
                    self.generate_precombines,
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "EXPERIMENTAL. Bake interior precombined meshes (*_OC.nif) and "
                        "stamp CELL PCMB/XCRI + REFR VC into the built ESM after the "
                        "asset waves. Off by default; leave unchecked unless testing."
                    )
                if changed:
                    self._set_workspace_settings(
                        {_GENERATE_PRECOMBINES_KEY: self.generate_precombines}
                    )
                changed, self.full_logging = toggle(
                    f"Full logging{_NS}",
                    self.full_logging,
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Write per-conversion INFO logs, native stderr, memory/timing "
                        "reports, and terrain debug output. Per-record drop tracing is "
                        "included only when MODBOX_TRACE_DROPS is set and can use "
                        "substantial disk space."
                    )
                if changed:
                    self._set_workspace_settings({_FULL_LOGGING_KEY: self.full_logging})
                imgui.separator()
                recovery_idx = (
                    _RECOVERY_PHASE_VALUES.index(self.recovery_phase)
                    if self.recovery_phase in _RECOVERY_PHASE_VALUES
                    else _RECOVERY_PHASE_VALUES.index("lodgen")
                )
                imgui.set_next_item_width(scaled(200))
                changed, recovery_idx = imgui.combo(
                    f"Resume from{_NS}",
                    recovery_idx,
                    _RECOVERY_PHASE_LABELS,
                )
                if changed:
                    self.recovery_phase = _RECOVERY_PHASE_VALUES[recovery_idx]
                    self._set_workspace_settings({_RECOVERY_PHASE_KEY: self.recovery_phase})
                imgui.same_line()
                needs_full_rebuild = self.recovery_phase in _RECOVERY_FULL_REBUILD_PHASES
                can_resume = self._is_default_pair() and (
                    self.can_convert() if needs_full_rebuild else self.can_deploy_existing()
                )
                if not can_resume:
                    imgui.begin_disabled()
                if imgui.button(f"Resume{_NS}"):
                    self.start_resume_from_phase()
                if not can_resume:
                    imgui.end_disabled()
                if needs_full_rebuild:
                    imgui.text_disabled(
                        "No durable ESP exists at this point; recovery reruns record conversion."
                    )
                else:
                    imgui.text_disabled(
                        "Resume overwrites outputs from the selected phase onward."
                    )
                if running:
                    imgui.end_disabled()

    def _run_install_audit(self):
        try:
            paths = self.build_paths()
            target = self._resolve_install_target(
                paths.target_data_dir, paths.target_custom_ini_path
            )
            mode = self.install_location.strip().lower()
            deploy_dir = (
                paths.output_root
                if mode == "none"
                else (target.deploy_data_dir or paths.target_data_dir)
            )
            self._install_audit = audit_archive_ini(
                deploy_dir=deploy_dir,
                ini_path=target.runtime_ini_path,
                mod_name=paths.mod_name,
                plugin_name=self._pair().output_plugin_name,
            )
            self._install_audit_error = None
        except Exception as exc:  # surface in UI, never crash the panel
            self._install_audit = None
            self._install_audit_error = str(exc)

    def _repair_install_ini(self):
        try:
            paths = self.build_paths()
            repair_archive_ini(
                ini_path=self._install_audit.ini_path,
                base_ini_path=paths.target_game_ini_path,
                archive_names=[
                    row.name
                    for row in self._install_audit.rows
                    if row.kind == "ba2" and row.deployed
                ],
                plugin_name=self._pair().output_plugin_name,
            )
            self._run_install_audit()
        except Exception as exc:  # surface in UI, never crash the panel
            self._install_audit = None
            self._install_audit_error = str(exc)

    def _draw_install_audit(self) -> None:
        if self._install_audit_error:
            imgui.text_colored(
                semantic_color("error"), self._install_audit_error
            )
            return
        report = self._install_audit
        if report is None:
            return
        if report.note:
            imgui.text_colored(semantic_color("warning"), report.note)
        for row in report.rows:
            ok = row.deployed and (row.registered is True or row.registered is None)
            color = (
                semantic_color("success")
                if ok
                else semantic_color("error")
            )
            imgui.text_colored(
                color,
                f"{'OK' if ok else 'MISSING'}  {row.name}  "
                f"(deployed={row.deployed}, registered={row.registered})",
            )
        stale_registration = getattr(report, "stale_registration", [])
        for name in stale_registration:
            imgui.text_colored(
                semantic_color("warning"),
                f"STALE  {name}  (no matching deployed archive)",
            )
        if (
            report.missing_registration or stale_registration
        ) and report.ini_path is not None:
            if imgui.button(f"Update archive entries{_NS}_repair"):
                self._repair_install_ini()

    def _draw_preflight_modal(self) -> None:
        report = self._preflight_report
        if report is None:
            return
        prepare_dialog(640, 440)
        if imgui.begin(f"Missing conversion inputs{_NS}_preflight"):
            imgui.text_wrapped(
                "Conversion cannot start until these inputs are extracted:"
            )
            imgui.separator()
            for item in report.required_missing:
                imgui.push_style_color(imgui.Col_.text, semantic_color("warning"))
                imgui.text(item.label)
                imgui.pop_style_color()
                imgui.text_wrapped(item.checked_path)
                imgui.text_wrapped(item.fix_hint)
                imgui.separator()
            for item in report.optional_missing:
                imgui.text_disabled(f"Optional: {item.label} — {item.fix_hint}")
            if imgui.button(f"Open FO76 extracted folder{_NS}_pf_open"):
                import os

                fo76_ext = self._settings().get_game_paths("fo76").get("extracted_dir", "")
                if fo76_ext and os.path.isdir(fo76_ext):
                    os.startfile(fo76_ext)  # noqa: S606 (Windows-only UI)
            imgui.same_line()
            if imgui.button(f"Close{_NS}_pf_close"):
                self._preflight_report = None
                self._preflight_cache = None
        imgui.end()

    def _draw_low_space_modal(self) -> None:
        if getattr(self, "_waiting_for_space_check", False):
            prepare_dialog(560, 210)
            if imgui.begin(f"Measuring available disk space{_NS}_disk_space_wait"):
                imgui.text_wrapped(
                    "Checking the conversion and install drives before starting."
                )
                imgui.text_disabled("This runs in the background.")
                if imgui.button(f"Go back{_NS}_disk_wait_back"):
                    self._waiting_for_space_check = False
            imgui.end()
            return
        volumes = getattr(self, "_low_space_warning", None)
        if not volumes:
            return
        prepare_dialog(640, 380)
        if imgui.begin(f"Not enough free space{_NS}_disk_space"):
            imgui.text_wrapped(
                "The conversion may run out of disk space. The estimate includes the "
                "loose conversion workspace and the packed BA2 output. Fallout 76's "
                "existing install is not counted again."
            )
            imgui.separator()
            for volume in volumes:
                if volume.unavailable:
                    imgui.text_colored(
                        semantic_color("warning"),
                        f"{volume.key or volume.path.anchor}: available space "
                        "could not be measured",
                    )
                    imgui.text_disabled(" + ".join(volume.labels))
                    imgui.text_wrapped(str(volume.path))
                    imgui.separator()
                    continue
                imgui.text_colored(
                    semantic_color("warning"),
                    f"{volume.key or volume.path.anchor}: "
                    f"{_format_gb(volume.required_bytes)} needed, "
                    f"{_format_gb(volume.free_bytes)} free",
                )
                imgui.text_disabled(" + ".join(volume.labels))
                imgui.text_wrapped(str(volume.path))
                imgui.separator()
            if imgui.button(f"Go back{_NS}_disk_back"):
                self._low_space_warning = None
            imgui.same_line()
            if imgui.button(f"Continue anyway{_NS}_disk_continue"):
                self._continue_conversion_with_low_space()
        imgui.end()

    def _draw_cleanup_dialog(self) -> None:
        self._ensure_cleanup_state()
        if not self._cleanup_dialog_open:
            return
        with self._cleanup_lock:
            status = self._cleanup_status
            targets = self._cleanup_targets
            selected = set(self._cleanup_selected)
            message = self._cleanup_message
            error = self._cleanup_error
        prepare_dialog(720, 430)
        window_flags = (
            imgui.WindowFlags_.no_scrollbar.value
            | imgui.WindowFlags_.no_scroll_with_mouse.value
        )
        if imgui.begin(f"Free up disk space{_NS}_cleanup", flags=window_flags):
            busy = status != "idle"
            if busy:
                imgui.begin_disabled()
            imgui.text_wrapped(
                "Select the cache categories to remove. Deleted extracted data must "
                "be recreated if it is needed later."
            )
            imgui.separator()
            if not targets and status == "idle":
                imgui.text_disabled("No safe BACUP cleanup targets were found.")

            table_flags = (
                imgui.TableFlags_.borders.value
                | imgui.TableFlags_.row_bg.value
                | imgui.TableFlags_.scroll_y.value
                | imgui.TableFlags_.sizing_stretch_prop.value
            )
            table_height = max(
                scaled(120),
                min(scaled(240), imgui.get_content_region_avail().y - scaled(82)),
            )
            if imgui.begin_table(
                f"{_NS}_cleanup_grid",
                3,
                table_flags,
                imgui.ImVec2(0, table_height),
            ):
                fixed = imgui.TableColumnFlags_.width_fixed.value
                stretch = imgui.TableColumnFlags_.width_stretch.value
                no_resize = imgui.TableColumnFlags_.no_resize.value
                imgui.table_setup_column("", fixed | no_resize, scaled(34))
                imgui.table_setup_column("Cleanup category", stretch | no_resize)
                imgui.table_setup_column("Space saved", fixed | no_resize, scaled(120))
                imgui.table_headers_row()
                for target in targets:
                    imgui.table_next_row()
                    imgui.table_set_column_index(0)
                    checked = target.key in selected
                    changed, checked = imgui.checkbox(
                        f"##cleanup_select_{target.key}", checked
                    )
                    if changed:
                        with self._cleanup_lock:
                            if checked:
                                self._cleanup_selected.add(target.key)
                            else:
                                self._cleanup_selected.discard(target.key)
                        if checked:
                            selected.add(target.key)
                        else:
                            selected.discard(target.key)
                    imgui.table_set_column_index(1)
                    imgui.text_wrapped(target.label)
                    imgui.table_set_column_index(2)
                    if target.size_bytes > 0:
                        imgui.text_colored(semantic_color("success"), _format_gb(target.size_bytes))
                    else:
                        imgui.text_disabled(_format_gb(0))
                imgui.end_table()

            selected_size = sum(
                target.size_bytes for target in targets if target.key in selected
            )
            cannot_delete = not selected or busy or self._runner_running()
            if cannot_delete:
                imgui.begin_disabled()
            if imgui.button(
                f"Delete selected ({_format_gb(selected_size)}){_NS}_cleanup_delete"
            ):
                self._start_cleanup_delete()
            if cannot_delete:
                imgui.end_disabled()
            imgui.same_line()
            if imgui.button(f"Close{_NS}_cleanup_close"):
                self._cleanup_dialog_open = False
            imgui.same_line()
            if imgui.button(f"Open Windows Temp{_NS}_cleanup_temp"):
                try:
                    os.startfile(windows_temp_dir())
                except OSError as exc:
                    self._cleanup_error = str(exc)
            if message:
                imgui.text_colored(semantic_color("success"), message)
            if error:
                imgui.text_colored(semantic_color("error"), error)
            if busy:
                imgui.end_disabled()
            if status == "scanning":
                draw_runner_overlay(
                    "Calculating cleanup space",
                    "Measuring reclaimable data...",
                    None,
                )
            elif status == "deleting":
                draw_runner_overlay(
                    "Deleting selected data",
                    "Removing cleanup categories...",
                    None,
                )
        imgui.end()

    def _draw_completion_popup(self) -> None:
        if not self._completion:
            return
        prepare_dialog(600, 360)
        if imgui.begin(f"Conversion complete{_NS}_done"):
            c = self._completion
            if c.get("elapsed_seconds") is not None:
                operation = "deployment" if c.get("deploy_existing") else (
                    "recovery" if c.get("resume_from") else "conversion"
                )
                heading(f"Total {operation} time: {_format_duration(c['elapsed_seconds'])}")
                imgui.separator()
            if c["deployed"]:
                if c.get("deploy_existing"):
                    imgui.text("Existing mod deployed.")
                elif c.get("resume_from"):
                    imgui.text("Resumed conversion deployed.")
                else:
                    imgui.text("Deployed to Fallout 4 - launch the game to play.")
                if c.get("companion_deployed"):
                    imgui.text(
                        f"Companion mod deployed: {len(c['companion_deployed'])} file(s)"
                    )
                if c.get("cleanup_removed"):
                    imgui.text("Cleanup removed:")
                    for path in c["cleanup_removed"]:
                        imgui.text_wrapped(path)
            else:
                imgui.text("Mod built at:")
                imgui.text_wrapped(c["mod_path"])
                if imgui.button(f"Open folder{_NS}"):
                    import os

                    os.startfile(c["mod_path"])  # noqa: S606 (Windows-only UI)
                if self.deploy_format != "standard" and c.get("ini_snippet"):
                    imgui.separator()
                    imgui.text("Add these lines to Fallout4Custom.ini:")
                    imgui.text_wrapped(c["ini_snippet"])
            if imgui.button(f"Close{_NS}_done"):
                self._completion = None
        imgui.end()
