"""Conversion project panel for B.A.C.U.P."""
from __future__ import annotations

import logging
import os
import shutil
import threading
import time
from pathlib import Path

from imgui_bundle import imgui

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
from bacup_lib.runner import ConversionRunner
from bacup_lib.source_pairs import (
    DEFAULT_PAIR_ID,
    SOURCE_PAIRS,
    get_pair,
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
    load_profile_settings,
)
from ui.toolkit.app_paths import get_code_root, get_exe_dir, get_resource_dir
from bacup_ui.setup import (
    _ba2_size_bytes,
    _dir_size_bytes,
    _format_gb,
    clear_project_owned_extractions,
    get_project_profile,
    get_project_setup_ownership,
    project_owns_extracted_path,
)
from creation_lib.core.fo4_version import detect_ba2_target
from ui.toolkit.steam_install import SteamInstallResult, validate_steam_install_for_game

_log = logging.getLogger("toolkit.appalachia")
_NS = "##appalachia"
_COL_OK = imgui.ImVec4(0.40, 0.85, 0.40, 1.0)     # verified / success
_COL_ERR = imgui.ImVec4(0.95, 0.40, 0.40, 1.0)    # failed / missing
_COL_WARN = imgui.ImVec4(1.00, 0.85, 0.30, 1.0)   # warnings
_COL_ACCENT = imgui.ImVec4(0.55, 0.78, 1.00, 1.0)  # section / version accents
_PHASE_ROWS = [
    ("prepare_conversion", "Prepare Conversion"),
    ("translate_records", "Translate Records"),
    ("convert_terrain", "Convert Terrain"),
    ("convert_terrain_assets", "Convert Terrain Assets"),
    ("convert_nifs", "Convert NIFs"),
    ("convert_textures", "Convert Textures"),
    ("convert_materials", "Convert Materials"),
    ("convert_havok", "Convert Havok"),
    ("synthesize_drivers", "Synthesize Drivers"),
    ("convert_animations", "Convert Animations"),
    ("copy_sounds", "Copy Sounds"),
    ("scaffold_mod", "Scaffold Mod"),
    ("build_esp", "Build ESP"),
    ("regenerate_modt", "Regenerate MODT"),
    ("generate_anim_text_data", "Generate AnimTextData"),
    ("lodgen", "Generate LOD"),
    ("pack", "Pack BA2"),
    ("deploy", "Deploy Mod"),
]
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
    "convert_terrain": ("convert_terrain", "Convert Terrain"),
    "convert_textures": ("convert_textures", "Convert Textures"),
    "convert_textures_v2": ("convert_textures", "Convert Textures"),
    "deploy_mod": ("deploy", "Deploy Mod"),
    "generate_lod": ("lodgen", "Generate LOD"),
    "generate_animtextdata": ("generate_anim_text_data", "Generate AnimTextData"),
    "lod": ("lodgen", "Generate LOD"),
    "lodgen": ("lodgen", "Generate LOD"),
    "pack_ba2": ("pack", "Pack BA2"),
    "postprocess_havok_assets": ("postprocess_havok_assets", "Postprocess Havok Assets"),
    "scaffold_mod": ("scaffold_mod", "Scaffold Mod"),
    "synthesize_drivers": ("synthesize_drivers", "Synthesize Drivers"),
    "translate_records": ("translate_records", "Translate Records"),
    "translate_records_rust": ("translate_records", "Translate Records"),
}
_LOD_PROFILE_VALUES = [PROFILE_HIGH_QUALITY, PROFILE_PERFORMANCE]
_LOD_PROFILE_LABELS = [PROFILE_LABELS[p] for p in _LOD_PROFILE_VALUES]
_DEFAULT_ARCHIVE_MAX_GB = 4
_ARCHIVE_MIN_GB = 1
_ARCHIVE_MAX_GB = 16
_APPALACHIA_ARCHIVE_MAX_BYTES = _DEFAULT_ARCHIVE_MAX_GB * 1024**3
_ARCHIVE_MAX_GB_KEY = "archive_max_gb"
_BA2_TARGET_KEY = "ba2_target"
_BA2_TARGET_VALUES = ["auto", "og", "nextgen"]
_BA2_TARGET_LABELS = ["Auto (detect FO4)", "Force OG (v1)", "Force Next-Gen (v8)"]
_ATLAS_MIP_FLOODING_KEY = "atlas_mip_flooding"
_TEXTURE_LANDSCAPE_MIP_FLOODING_KEY = "texture_landscape_mip_flooding"
_WORKERS_KEY = "workers"
_LOD_PROFILE_KEY = "lod_profile"
_INSTALL_LOCATION_KEY = "install_location"
_INSTALL_LOCATION_VALUES = ["game", "mo2", "vortex", "none"]
_INSTALL_LOCATION_LABELS = ["Game Dir", "MO2", "Vortex", "None"]
_RECOVERY_PHASE_KEY = "recovery_phase"
_RECOVERY_PHASE_VALUES = ["nifs", "textures", "havok", "lodgen", "pack", "deploy"]
_RECOVERY_PHASE_LABELS = [
    "Convert NIFs",
    "Convert Textures",
    "Convert Havok",
    "Generate LOD",
    "Pack BA2",
    "Deploy Mod",
]
_COMPANION_MOD_NAME = "B21_TalesFromAppalachia"
_COMPANION_ROOT_FILES = (f"{_COMPANION_MOD_NAME}.esp",)
_COMPANION_DEPLOY_DIRS = ("PrismaUI_F4",)
_COMPANION_EXCLUDED_SUFFIXES = {".pdb"}
_STEAM_INSTALL_LABELS = {
    "fo4": "FO4",
    "fo76": "FO76",
    "fnv": "FNV",
    "fo3": "FO3",
    "skyrimse": "Skyrim SE",
}
_PROJECT_SETTINGS_KEY = "conversion_projects"
_WORKSPACE_SETTINGS_ID = "appalachia"
_PROJECT_BY_PAIR = {
    "fo76:fo4": ("appalachia", "Tales From Appalachia"),
    "fnvfo3:fo4": ("wasteland", "Legends of the Wasteland"),
    "skyrimse:fo4": ("north", "Fables of the North"),
}


def _data_dir(root_dir: str) -> Path:
    p = Path(root_dir)
    return p if p.name.lower() == "data" else p / "Data"


def _archive_max_gb(value) -> int:
    try:
        gb = int(value)
    except (TypeError, ValueError):
        gb = _DEFAULT_ARCHIVE_MAX_GB
    return max(_ARCHIVE_MIN_GB, min(_ARCHIVE_MAX_GB, gb))


def _recovery_phase(value) -> str:
    phase = str(value or "").strip().lower().replace("-", "_").replace(" ", "_")
    return phase if phase in _RECOVERY_PHASE_VALUES else "lodgen"


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
        self._install_audit = None
        self._install_audit_error = None
        self.archive_max_gb = _archive_max_gb(
            workspace_settings.get(_ARCHIVE_MAX_GB_KEY, _DEFAULT_ARCHIVE_MAX_GB)
        )
        self.ba2_target = str(
            workspace_settings.get(_BA2_TARGET_KEY, "auto")
        ).strip().lower()
        if self.ba2_target not in _BA2_TARGET_VALUES:
            self.ba2_target = "auto"
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
        self.lod_mode = "hybrid-atlas"
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
        self.re_use_land = False
        self.recovery_phase = _recovery_phase(
            workspace_settings.get(_RECOVERY_PHASE_KEY, "lodgen")
        )
        self._upgrade_manifest_cache: tuple[str, object] | None = None
        self._snam_cache: tuple[tuple, str] | None = None
        self._phases: list[dict] = []
        self._summary: dict | None = None
        self._completion: dict | None = None
        self._disk_usage_cache: tuple[float, dict[str, int]] | None = None
        self._disk_usage_lock = threading.Lock()
        self._disk_usage_running = False
        self._disk_usage_thread: threading.Thread | None = None
        self._steam_install_cache: dict[str, tuple[str, SteamInstallResult]] = {}
        self._status_log_frac = 0.30
        self.upgrade = False
        self._upgrade_user_toggled = False

    def _settings(self):
        return self._workspace._toolkit_settings

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

    @staticmethod
    def _phase_number(phase: dict) -> int:
        try:
            return int(phase.get("phase", 0))
        except (TypeError, ValueError):
            return 0

    @staticmethod
    def _phase_key_and_label(phase: dict) -> tuple[str, str]:
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
        rows = list(_PHASE_ROWS)
        seen = {key for key, _label in rows}
        for phase in phases:
            key, label = cls._phase_key_and_label(phase)
            if key in seen:
                continue
            rows.append((key, label))
            seen.add(key)
        return rows

    @staticmethod
    def _phase_item_fraction(phase: dict) -> float:
        if phase.get("status") == "completed":
            return 1.0
        try:
            total = int(phase.get("total_items", 0) or 0)
            completed = int(phase.get("completed_items", 0) or 0)
        except (TypeError, ValueError):
            return 0.0
        if total <= 0:
            return 0.0
        return max(0.0, min(completed / total, 1.0))

    @classmethod
    def _runner_progress(
        cls,
        phases: list[dict],
        phase_rows: list[tuple[str, str]],
    ) -> tuple[float, str]:
        total_phases = max(len(phase_rows), 1)
        row_index = {key: index for index, (key, _label) in enumerate(phase_rows)}
        active: dict | None = None
        completed_keys: set[str] = set()
        for phase in phases:
            key, _label = cls._phase_key_and_label(phase)
            status = phase.get("status", "pending")
            if status == "completed":
                completed_keys.add(key)
            elif status == "running":
                active = phase

        if active is not None:
            active_key, active_label = cls._phase_key_and_label(active)
            progress_slots = float(row_index.get(active_key, len(phase_rows) - 1))
            progress_slots += cls._phase_item_fraction(active)
            phase_name = active_label
            current_item = str(active.get("current_item", "") or "")
            if current_item:
                item_label = (
                    os.path.basename(current_item)
                    if os.path.sep in current_item or "/" in current_item
                    else current_item
                )
                message = f"{phase_name}: {item_label}"
            else:
                message = str(phase_name)
        else:
            progress_slots = float(
                sum(1 for key, _label in phase_rows if key in completed_keys)
            )
            message = "Starting conversion..." if not completed_keys else "Finalizing..."

        fraction = max(0.0, min(progress_slots / total_phases, 1.0))
        return fraction, message

    def build_paths(self) -> RegenPaths:
        s = self._settings()
        pair = self._pair()
        source = s.get_game_paths(pair.source_game)
        target_game = s.get_game_paths(pair.target_game)
        source_root = source.get("root_dir", "") or ""
        target_root = target_game.get("root_dir", "") or ""
        target_extracted = target_game.get("extracted_dir", "") or ""
        source_data_dir = _data_dir(source_root)
        target_data_dir = _data_dir(target_root)
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
                    additional_source_asset_roots = (Path(grafted_extracted),)
        return RegenPaths(
            source_extracted_dir=Path(source.get("extracted_dir", "") or ""),
            source_data_dir=source_data_dir,
            target_extracted_dir=(
                Path(target_extracted) if target_extracted else None
            ),
            target_data_dir=target_data_dir,
            target_ck_ini_path=Path(target_root) / "CreationKitCustom.ini",
            target_custom_ini_path=docs / "Fallout4Custom.ini",
            target_game_ini_path=docs / "Fallout4.ini",
            output_root=get_exe_dir() / "mods" / pair.output_mod_name,
            mod_name=pair.output_mod_name,
            resource_dir=get_resource_dir(),
            deploy_data_dir=install_target.deploy_data_dir,
            runtime_ini_path=install_target.runtime_ini_path,
            merge_primary_plugin_paths=merge_primary_plugin_paths,
            merge_grafted_plugin_paths=merge_grafted_plugin_paths,
            additional_source_asset_roots=additional_source_asset_roots,
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
        options = RegenOptions(
            deploy=self.install_location.strip().lower() != "none",
            ba2_mode="expanded",
            archive_max_bytes=self.archive_max_gb * 1024**3,
            workers=self.workers or None,
            asset_workers=None,
            lod_mode=self.lod_mode,
            texture_landscape_mip_flooding=bool(
                getattr(self, "texture_landscape_mip_flooding", False)
            ),
            re_use_land=self.re_use_land,
            write_land_cache=False,
            include_interior=True,
            records_limit=None,
            generate_anim_text_data=True,
            anim_text_data_native=True,
            direct_deploy_archives=True,
            update_runtime_ini=self.add_archives_to_ini,
            fo4_ba2_target=self.resolve_ba2_target(),
        )
        # getattr guard: some tests construct RegenPanel via __new__ (bypassing
        # __init__) and don't set the upgrade attributes -- default to the
        # unchanged full-build path in that case, same as upgrade=False.
        if getattr(self, "upgrade", False):
            manifest = self._load_upgrade_manifest_cached()
            if manifest is not None:
                options.upgrade = True
                options.mod_version = manifest.current
                options.upgrade_from = None
                options.upgrade_manifest_path = self.upgrade_manifest_path()
        return options

    @staticmethod
    def load_lod_settings(
        profile: str = PROFILE_HIGH_QUALITY,
        lod_mode: str = "hybrid-atlas",
    ) -> dict:
        candidates = [
            get_code_root(),
            Path(__file__).resolve().parents[3],
        ]
        return load_profile_settings(candidates, profile=profile, lod_mode=lod_mode)

    def _selected_lod_settings(self, lod_mode: str) -> dict:
        settings = self.load_lod_settings(self.lod_profile, lod_mode)
        settings.setdefault("objects", {})["atlas_mip_flooding"] = bool(
            getattr(self, "atlas_mip_flooding", False)
        )
        return settings

    def can_convert(self) -> bool:
        if self._runner_running():
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
        return self._steam_installs_ok()

    def generated_plugin_path(self) -> Path:
        return self.build_paths().output_root / self._pair().output_plugin_name

    def can_deploy_existing(self) -> bool:
        if self._runner_running():
            return False
        fo4 = self._settings().get_game_paths("fo4")
        if not fo4.get("root_dir"):
            return False
        return self._steam_installs_ok() and self.generated_plugin_path().is_file()

    def _steam_install_result(self, game_id: str) -> SteamInstallResult:
        root = self._settings().get_game_paths(game_id).get("root_dir", "") or ""
        cached = self._steam_install_cache.get(game_id)
        if cached is not None and cached[0] == root:
            return cached[1]
        result = validate_steam_install_for_game(game_id, root)
        self._steam_install_cache[game_id] = (root, result)
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
        cached = self._upgrade_manifest_cache
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
        labels = ", ".join(plan.swap_labels)
        return f"Will regenerate: {families} -> swap {labels}; reuse rest"

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
                label = _STEAM_INSTALL_LABELS.get(game_id, game_id.upper())
                report.required_missing.append(
                    MissingInput(
                        f"{label} extracted directory",
                        source_root_text or "(not configured)",
                        f"Re-run {self._project_label()} setup and extract {label} assets.",
                    )
                )
        self._preflight_cache = (key, report)
        return report

    def _steam_installs_ok(self) -> bool:
        return all(
            self._steam_install_result(game_id).ok
            for game_id in self._required_game_ids()
        )

    def _require_steam_installs(self) -> None:
        for game_id in self._required_game_ids():
            result = self._steam_install_result(game_id)
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

    def start_conversion(self) -> None:
        self._phases = []
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
                self._require_steam_installs()
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
            result = regen_pipeline.run_full_regen(
                paths,
                options,
                pair=pair,
                phases=phases,
                runner=runner,
                lod_settings=lod_settings,
            )
            companion_deployed = (
                self._deploy_companion_mod(paths, runner)
                if result.deployed and self._is_default_pair()
                else []
            )
            cleanup_removed = self._cleanup_after_deploy(paths, result.deployed)
            runner.emit_complete(
                str(result.output_root),
                {
                    "deployed": result.deployed,
                    "exit_code": result.exit_code,
                    "companion_deployed": companion_deployed,
                    "cleanup_removed": cleanup_removed,
                },
            )

        self._start_runner(work)

    def start_deploy_existing(self) -> None:
        from bacup_lib import regen_pipeline

        self._require_steam_installs()

        self._phases = []
        self._summary = None
        self._completion = None
        paths = self.build_paths()
        options = self.build_options()

        def work(runner: ConversionRunner) -> None:
            runner.emit_log("INFO", f"Deploying existing {self._project_label()} output...")
            result = regen_pipeline.deploy_existing(
                paths,
                update_runtime_ini=options.update_runtime_ini,
            )
            if result.failures:
                raise RuntimeError("; ".join(result.failures))
            companion_deployed = (
                self._deploy_companion_mod(paths, runner)
                if result.deployed and self._is_default_pair()
                else []
            )
            cleanup_removed = self._cleanup_after_deploy(paths, result.deployed)
            runner.emit_complete(
                str(result.output_root),
                {
                    "deployed": result.deployed,
                    "exit_code": result.exit_code,
                    "deploy_existing": True,
                    "companion_deployed": companion_deployed,
                    "cleanup_removed": cleanup_removed,
                },
            )

        self._start_runner(work)

    def start_resume_from_phase(self) -> None:
        from bacup_lib import PhaseSelection, regen_pipeline

        self._require_steam_installs()

        self._phases = []
        self._summary = None
        self._completion = None
        paths = self.build_paths()
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
            result = regen_pipeline.run_resume_from_phase(
                paths,
                options,
                start_phase=start_phase,
                phases=phases,
                runner=runner,
                lod_settings=lod_settings,
            )
            if result.failures:
                raise RuntimeError("; ".join(result.failures))
            companion_deployed = (
                self._deploy_companion_mod(paths, runner)
                if result.deployed and self._is_default_pair()
                else []
            )
            cleanup_removed = self._cleanup_after_deploy(paths, result.deployed)
            runner.emit_complete(
                str(result.output_root),
                {
                    "deployed": result.deployed,
                    "exit_code": result.exit_code,
                    "resume_from": start_phase,
                    "companion_deployed": companion_deployed,
                    "cleanup_removed": cleanup_removed,
                },
            )

        self._start_runner(work)

    def handle_event(self, event: dict) -> None:
        etype = event.get("type", "")
        if etype in ("phase_start", "item_progress", "phase_complete"):
            data = self._normalize_phase_data(event.get("data", {}) or {})
            phase_key = data.get("ui_key", "")
            existing = next(
                (p for p in self._phases if p.get("ui_key") == phase_key),
                None,
            )
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
        elif etype == "complete":
            summary = event.get("summary", {}) or {}
            preflight_report = summary.get("preflight_report")
            if preflight_report is not None:
                self._preflight_report = preflight_report
                return
            deployed = bool(summary.get("deployed"))
            ini_snippet = None
            if not deployed:
                ini_snippet = self._read_ini_snippet(Path(event.get("mod_path", "")))
            self._completion = {
                "mod_path": event.get("mod_path", ""),
                "deployed": deployed,
                "deploy_existing": bool(summary.get("deploy_existing")),
                "resume_from": summary.get("resume_from"),
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
        ws = self._workspace
        owner = getattr(ws, "_runner_owner", self)
        if owner is self and ws._runner is not None and not ws._runner.done:
            ws._runner.cancel()

    @staticmethod
    def _same_path(left: Path, right: Path) -> bool:
        return os.path.normcase(os.path.abspath(left)) == os.path.normcase(os.path.abspath(right))

    def _compute_disk_usage_summary(self) -> dict[str, int]:
        paths = self.build_paths()
        output_root = paths.output_root
        deploy_data_dir = paths.deploy_data_dir or paths.target_data_dir
        source_extracted = str(
            self._settings()
            .get_game_paths(self._pair().source_game)
            .get("extracted_dir", "")
            or ""
        )
        extracted_paths = (
            *((paths.target_extracted_dir,) if paths.target_extracted_dir else ()),
            *((paths.source_extracted_dir,) if source_extracted else ()),
            *paths.additional_source_asset_roots,
        )
        unique_extracted_paths = {
            os.path.normcase(os.path.abspath(path)): path
            for path in extracted_paths
        }
        return {
            "extracted": sum(
                _dir_size_bytes(path) for path in unique_extracted_paths.values()
            ),
            "mod_output": _dir_size_bytes(output_root),
            "mod_ba2": _ba2_size_bytes(output_root),
            "deployed_ba2": _ba2_size_bytes(deploy_data_dir),
        }

    def _start_disk_usage_worker(self) -> None:
        with self._disk_usage_lock:
            if self._disk_usage_running or self._disk_usage_cache is not None:
                return
            self._disk_usage_running = True

        def worker() -> None:
            try:
                summary = self._compute_disk_usage_summary()
            except Exception:
                _log.warning("Disk usage scan failed", exc_info=True)
                summary = {
                    "extracted": 0,
                    "mod_output": 0,
                    "mod_ba2": 0,
                    "deployed_ba2": 0,
                }
            with self._disk_usage_lock:
                self._disk_usage_cache = (time.monotonic(), summary)
                self._disk_usage_running = False

        self._disk_usage_thread = threading.Thread(
            target=worker,
            name="appalachia-disk-usage",
            daemon=True,
        )
        self._disk_usage_thread.start()

    def disk_usage_summary(self) -> dict[str, int]:
        with self._disk_usage_lock:
            cached = self._disk_usage_cache
        if cached is not None:
            return dict(cached[1])
        self._start_disk_usage_worker()
        return {"extracted": 0, "mod_output": 0, "mod_ba2": 0, "deployed_ba2": 0}

    def disk_usage_loading(self) -> bool:
        with self._disk_usage_lock:
            return self._disk_usage_running and self._disk_usage_cache is None

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

        if runner is not None:
            runner.emit_log("INFO", f"Packing companion mod {_COMPANION_MOD_NAME}...")

        pack_mod(
            _COMPANION_MOD_NAME,
            game="fo4",
            project_root=app_root,
            archive_max_bytes=self.archive_max_gb * 1024**3,
            archive_workers=self.workers,
        )
        companion_archives = discover_mod_archives(companion_root, _COMPANION_MOD_NAME)
        if not companion_archives:
            raise FileNotFoundError(
                f"Companion mod archive not found after packing: {companion_root}"
            )
        if runner is not None:
            runner.emit_log("INFO", f"Deploying companion mod {_COMPANION_MOD_NAME}...")

        for filename in _COMPANION_ROOT_FILES:
            src = companion_root / filename
            if not src.is_file():
                raise FileNotFoundError(f"Companion mod file not found: {src}")
            dest = deploy_data_dir / filename
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dest)
            deployed.append(filename)

        companion_archive_names = {archive.name for archive in companion_archives}
        for stale_archive in discover_mod_archives(deploy_data_dir, _COMPANION_MOD_NAME):
            if stale_archive.name not in companion_archive_names:
                stale_archive.unlink()
        for archive in companion_archives:
            shutil.copy2(archive, deploy_data_dir / archive.name)
            deployed.append(archive.name)

        for dirname in _COMPANION_DEPLOY_DIRS:
            src_root = companion_root / dirname
            if not src_root.is_dir():
                raise FileNotFoundError(f"Companion mod directory not found: {src_root}")
            target_root = deploy_data_dir / dirname
            for src in sorted(src_root.rglob("*")):
                if not src.is_file() or src.suffix.lower() in _COMPANION_EXCLUDED_SUFFIXES:
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
            expected = exe_dir / "mods" / self._pair().output_mod_name
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

    def draw(self) -> None:  # imgui rendering; covered by launch smoke, not unit tests
        if not imgui.begin(f"B.A.C.U.P.{_NS}"):
            imgui.end()
            return
        imgui.text_disabled("Bethesda Asset Converter Universal Platform")
        self.draw_project()
        imgui.end()

    def draw_project(self) -> None:
        self._draw_header()
        self._draw_split()
        self._draw_preflight_modal()
        self._draw_completion_popup()

    def _draw_header(self) -> None:
        imgui.text(self._project_label())
        _, exe_version = self._detect_ba2_target()
        pair = self._pair()
        target_root = (
            self._settings().get_game_paths(pair.target_game).get("root_dir", "")
            or "(not set)"
        )
        exe_display = exe_version or f"unknown (checked: {target_root})"
        if self._is_default_pair():
            installed = self._detected_installed_version()
            imgui.text_disabled(
                f"Installed: {installed}    |    Game (Fallout4.exe): {exe_display}"
            )
        else:
            imgui.text_disabled(
                f"Pair: {pair.pair_id}    |    Game (Fallout4.exe): {exe_display}"
            )
        imgui.separator()

    def _draw_split(self) -> None:
        flags = (
            imgui.TableFlags_.resizable.value
            | imgui.TableFlags_.borders_inner_v.value
        )
        if not imgui.begin_table(f"{_NS}_split", 2, flags):
            return
        imgui.table_setup_column(
            "Settings", imgui.TableColumnFlags_.width_stretch.value, 0.8
        )
        imgui.table_setup_column(
            "Status", imgui.TableColumnFlags_.width_stretch.value, 0.2
        )
        imgui.table_next_row()
        imgui.table_set_column_index(0)
        if imgui.begin_child(f"{_NS}_settings_pane"):
            self._draw_settings_column()
        imgui.end_child()
        imgui.table_set_column_index(1)
        status_flags = (
            imgui.WindowFlags_.no_scrollbar.value
            | imgui.WindowFlags_.no_scroll_with_mouse.value
        )
        if imgui.begin_child(f"{_NS}_status_pane", window_flags=status_flags):
            self._draw_status_column()
        imgui.end_child()
        imgui.end_table()

    def _draw_status_column(self) -> None:
        from bacup_ui.conversion.widgets import draw_phase_progress

        running = self._owns_active_runner()
        phase_rows = self._phase_rows(self._phases)

        avail = imgui.get_content_region_avail()
        try:
            avail_y = float(avail.y)
        except (TypeError, ValueError):
            avail_y = 0.0
        splitter_h = 6.0
        item_spacing_y = float(imgui.get_style().item_spacing.y)
        log_h = max(80.0, self._status_log_frac * avail_y)
        top_h = max(
            80.0,
            avail_y - log_h - splitter_h - (2.0 * item_spacing_y),
        )

        imgui.begin_child(f"progress{_NS}", imgui.ImVec2(0, top_h))
        if running:
            progress_fraction, progress_message = self._runner_progress(
                self._phases,
                phase_rows,
            )
            imgui.text(
                f"Converting {self._project_label()} — "
                f"{int(round(progress_fraction * 100))}%"
            )
            imgui.text_disabled(progress_message)
            imgui.separator()
        draw_phase_progress(_NS, phase_rows, self._phases)
        imgui.end_child()

        imgui.invisible_button(f"status_splitter{_NS}", imgui.ImVec2(-1, splitter_h))
        if imgui.is_item_active() and avail_y > 0.0:
            try:
                delta_y = float(imgui.get_io().mouse_delta.y)
            except (TypeError, ValueError):
                delta_y = 0.0
            new_log_h = min(max(log_h - delta_y, 0.12 * avail_y), 0.75 * avail_y)
            self._status_log_frac = new_log_h / avail_y
        if imgui.is_item_hovered():
            imgui.set_mouse_cursor(imgui.MouseCursor_.resize_ns)

        imgui.begin_child(f"log{_NS}", imgui.ImVec2(0, log_h))
        if self._log_panel is not None:
            self._log_panel.draw_body()
        imgui.end_child()

    def _draw_settings_column(self) -> None:
        from ui.tools.imgui_helpers import (
            _form_row_label,
            begin_form,
            draw_combo_field,
            draw_path_row,
            end_form,
        )

        ws = self._workspace
        running = self._runner_running()
        s = self._settings()
        pair = self._pair()
        paths = self.build_paths()
        install_target = self._resolve_install_target(
            paths.target_data_dir, paths.target_custom_ini_path
        )

        # -- A. Install Info --------------------------------------------------
        diag = imgui.collapsing_header(
            f"Install Info{_NS}", imgui.TreeNodeFlags_.default_open.value
        )
        if isinstance(diag, tuple):
            diag = diag[0]
        if diag and begin_form(f"{_NS}_diag", 200):
            _form_row_label(f"{pair.source_game.upper()} source")
            imgui.text(
                s.get_game_paths(pair.source_game).get("extracted_dir", "?")
            )
            _form_row_label(f"{pair.target_game.upper()} target")
            imgui.text(s.get_game_paths(pair.target_game).get("root_dir", "?"))
            for game_id in self._required_game_ids():
                steam_result = self._steam_install_result(game_id)
                label = _STEAM_INSTALL_LABELS.get(game_id, game_id.upper())
                _form_row_label(f"{label} Steam install")
                if steam_result.ok:
                    imgui.text_colored(_COL_OK, "verified")
                else:
                    imgui.text_colored(_COL_ERR, steam_result.message)
            deploy_data_dir = paths.deploy_data_dir or paths.target_data_dir
            _form_row_label("Deploy folder")
            imgui.text(f"{deploy_data_dir}")
            _form_row_label("INI")
            imgui.text(f"{install_target.runtime_ini_path or '(none)'}")
            if install_target.warning:
                _form_row_label("Warning")
                imgui.text_colored(_COL_WARN, install_target.warning)
            detected_target, detected_version = self._detect_ba2_target()
            _form_row_label("Detected FO4")
            imgui.text_colored(
                _COL_ACCENT,
                f"{detected_version or 'unknown'}  ·  {detected_target}  ·  "
                f"packing: {self.resolve_ba2_target()}",
            )
            sizes = self.disk_usage_summary()
            _form_row_label("Disk use")
            if self.disk_usage_loading():
                imgui.text_disabled("calculating in background...")
            else:
                imgui.text_colored(_COL_ACCENT, _format_gb(sizes["extracted"]))
                imgui.same_line()
                imgui.text_disabled("extracted  ·")
                imgui.same_line()
                imgui.text_colored(_COL_ACCENT, _format_gb(sizes["mod_output"]))
                imgui.same_line()
                imgui.text_disabled("mod  ·")
                imgui.same_line()
                imgui.text_colored(
                    _COL_ACCENT, _format_gb(sizes["mod_ba2"] + sizes["deployed_ba2"])
                )
                imgui.same_line()
                imgui.text_disabled("BA2")
            end_form()
        imgui.separator()

        if running:
            imgui.begin_disabled()

        # -- B. Settings ------------------------------------------------------
        imgui.text_colored(_COL_ACCENT, "Settings")
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
            install_idx = (
                _INSTALL_LOCATION_VALUES.index(self.install_location)
                if self.install_location in _INSTALL_LOCATION_VALUES
                else 0
            )
            changed, install_idx = draw_combo_field(
                "Install location", _INSTALL_LOCATION_LABELS, install_idx
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
            if self.install_location == "mo2":
                _form_row_label("MO2 profile INI")
                changed, self.mo2_use_profile_ini = imgui.checkbox(
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
            ini_label = (
                "Update MO2 Custom.ini"
                if self.install_location == "mo2" and self.mo2_use_profile_ini
                else "Update Fallout4Custom.ini"
            )
            _form_row_label(ini_label)
            _, self.add_archives_to_ini = imgui.checkbox(
                "##addba2ini", self.add_archives_to_ini
            )
            _form_row_label("Max BA2 size")
            imgui.set_next_item_width(-1)
            changed, archive_max_gb = imgui.slider_int(
                "##maxba2gb",
                self.archive_max_gb,
                _ARCHIVE_MIN_GB,
                _ARCHIVE_MAX_GB,
            )
            if changed:
                self.archive_max_gb = _archive_max_gb(archive_max_gb)
                self._set_workspace_settings({_ARCHIVE_MAX_GB_KEY: self.archive_max_gb})
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
            profile_idx = (
                _LOD_PROFILE_VALUES.index(self.lod_profile)
                if self.lod_profile in _LOD_PROFILE_VALUES
                else 0
            )
            changed, profile_idx = draw_combo_field(
                "LOD quality", _LOD_PROFILE_LABELS, profile_idx
            )
            self.lod_profile = _LOD_PROFILE_VALUES[profile_idx]
            if changed:
                self._set_workspace_settings({_LOD_PROFILE_KEY: self.lod_profile})
            self.lod_mode = "hybrid-atlas"
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
            end_form()

        # -- C. Upgrade mode --------------------------------------------------
        imgui.separator()
        imgui.text_colored(_COL_ACCENT, "Upgrade mode")
        if not self._upgrade_user_toggled:
            self.upgrade = self._deployed_esm_exists()
        changed, self.upgrade = imgui.checkbox(
            f"Upgrade existing deployment{_NS}", self.upgrade
        )
        if changed:
            self._upgrade_user_toggled = True
        manifest = self._load_upgrade_manifest_cached()
        if manifest is None:
            imgui.text_disabled("No upgrade manifest found - full build only.")
        else:
            detected = self._detected_installed_version()
            imgui.text_disabled(f"Detected installed: {detected}")
            if self.upgrade:
                imgui.text_disabled(f"Target: {manifest.current} (this build)")
                imgui.text_disabled(self.upgrade_plan_preview())
        if running:
            imgui.end_disabled()

        # -- D. Actions -------------------------------------------------------
        imgui.separator()
        can_start_conversion = self.can_convert()
        can_deploy_existing = self.can_deploy_existing()
        if imgui.begin_table(
            f"{_NS}_actions", 2, imgui.TableFlags_.sizing_stretch_same.value
        ):
            imgui.table_next_row()
            imgui.table_next_column()
            if not can_start_conversion:
                imgui.begin_disabled()
            if imgui.button(
                f"Convert {self._project_label()}{_NS}", imgui.ImVec2(-1, 0)
            ):
                self.start_conversion()
            if not can_start_conversion:
                imgui.end_disabled()
            imgui.table_next_column()
            if running and self._owns_active_runner():
                if imgui.button(f"Cancel{_NS}", imgui.ImVec2(-1, 0)):
                    ws._runner.cancel()
            else:
                if not can_deploy_existing:
                    imgui.begin_disabled()
                if imgui.button(f"Deploy existing mod{_NS}", imgui.ImVec2(-1, 0)):
                    self.start_deploy_existing()
                if not can_deploy_existing:
                    imgui.end_disabled()
            imgui.end_table()
        if not self.generated_plugin_path().is_file():
            imgui.text_disabled(
                "Deploy existing mod is available after "
                f"{self._pair().output_plugin_name} exists."
            )

        # -- E. Advanced ------------------------------------------------------
        imgui.separator()
        expanded = imgui.collapsing_header(f"Advanced{_NS}")
        if isinstance(expanded, tuple):
            expanded = expanded[0]
        if expanded:
            if running:
                imgui.begin_disabled()
            changed, self.atlas_mip_flooding = imgui.checkbox(
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
            changed, self.texture_landscape_mip_flooding = imgui.checkbox(
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
            imgui.separator()
            recovery_idx = (
                _RECOVERY_PHASE_VALUES.index(self.recovery_phase)
                if self.recovery_phase in _RECOVERY_PHASE_VALUES
                else _RECOVERY_PHASE_VALUES.index("lodgen")
            )
            imgui.set_next_item_width(200)
            changed, recovery_idx = imgui.combo(
                f"Resume from{_NS}",
                recovery_idx,
                _RECOVERY_PHASE_LABELS,
            )
            if changed:
                self.recovery_phase = _RECOVERY_PHASE_VALUES[recovery_idx]
                self._set_workspace_settings({_RECOVERY_PHASE_KEY: self.recovery_phase})
            imgui.same_line()
            can_resume = self._is_default_pair() and self.can_deploy_existing()
            if not can_resume:
                imgui.begin_disabled()
            if imgui.button(f"Resume{_NS}"):
                self.start_resume_from_phase()
            if not can_resume:
                imgui.end_disabled()
            imgui.text_disabled("Resume overwrites outputs from the selected phase onward.")
            if running:
                imgui.end_disabled()
            imgui.separator()
            if imgui.button(f"Check / repair install (INI){_NS}_audit"):
                self._run_install_audit()
            self._draw_install_audit()

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
                archive_names=self._install_audit.missing_registration,
            )
            self._run_install_audit()
        except Exception as exc:  # surface in UI, never crash the panel
            self._install_audit = None
            self._install_audit_error = str(exc)

    def _draw_install_audit(self) -> None:
        if self._install_audit_error:
            imgui.text_colored(
                imgui.ImVec4(1.0, 0.4, 0.4, 1.0), self._install_audit_error
            )
            return
        report = self._install_audit
        if report is None:
            return
        if report.note:
            imgui.text_colored(imgui.ImVec4(1.0, 0.9, 0.3, 1.0), report.note)
        for row in report.rows:
            ok = row.deployed and (row.registered is True or row.registered is None)
            color = (
                imgui.ImVec4(0.4, 0.8, 0.4, 1.0)
                if ok
                else imgui.ImVec4(1.0, 0.4, 0.4, 1.0)
            )
            imgui.text_colored(
                color,
                f"{'OK' if ok else 'MISSING'}  {row.name}  "
                f"(deployed={row.deployed}, registered={row.registered})",
            )
        if report.missing_registration and report.ini_path is not None:
            if imgui.button(f"Add missing entries{_NS}_repair"):
                self._repair_install_ini()

    def _draw_preflight_modal(self) -> None:
        report = self._preflight_report
        if report is None:
            return
        if imgui.begin(f"Missing conversion inputs{_NS}_preflight"):
            imgui.text_wrapped(
                "Conversion cannot start until these inputs are extracted:"
            )
            imgui.separator()
            for item in report.required_missing:
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.9, 0.3, 1.0))
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

    def _draw_completion_popup(self) -> None:
        if not self._completion:
            return
        if imgui.begin(f"Conversion complete{_NS}_done"):
            c = self._completion
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
                if c.get("ini_snippet"):
                    imgui.separator()
                    imgui.text("Add these lines to Fallout4Custom.ini:")
                    imgui.text_wrapped(c["ini_snippet"])
            if imgui.button(f"Close{_NS}_done"):
                self._completion = None
        imgui.end()
