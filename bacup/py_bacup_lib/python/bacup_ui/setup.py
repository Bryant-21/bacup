"""First-run and per-project setup for B.A.C.U.P.

Each conversion project validates only its own Steam games and extracts only
its own source games. Fallout 4 is the shared target and reads its official
archives directly, so an extracted Fallout 4 tree remains optional.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache
import logging
import os
import shutil
import threading
import time
from pathlib import Path

from imgui_bundle import hello_imgui, imgui, immapp

from creation_lib.ui.theme.window_chrome import (
    set_ini_folder,
    set_native_dark_title_bar,
)
from bacup_lib.upgrade_manifest import (
    bundled_upgrade_manifest_path,
    load_upgrade_manifest,
)
from bacup_ui.conversion.widgets import draw_runner_overlay
from ui.toolkit.app_paths import get_exe_dir, get_ini_dir
from ui.toolkit.path_detector import detect_game_path, validate_game_path
from ui.toolkit.setup_wizard import (
    _GameExtractor,
    _browse_button_width,
    _browse_input_width,
)
from ui.toolkit.steam_install import SteamInstallResult, validate_steam_install_for_game

_log = logging.getLogger("toolkit.bacup_setup")


def _format_release_label(release_id: str) -> str:
    normalized = release_id.strip()
    if normalized.lower().startswith("alpha"):
        suffix = normalized[len("alpha") :].strip(" _-vV")
        if suffix:
            return f"Alpha {suffix}"
    return normalized or "Current Release"


def _load_current_release_metadata() -> tuple[str, str, tuple[str, ...]]:
    try:
        manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
        current = next(
            version for version in manifest.versions if version.id == manifest.current
        )
    except Exception as exc:
        _log.warning("Unable to load current release notes: %s", exc)
        return "", "Current Release", ()
    return (
        manifest.current,
        _format_release_label(manifest.current),
        current.notes_for_conversion("fo76:fo4"),
    )


_CURRENT_RELEASE_ID, _CURRENT_RELEASE_LABEL, _CURRENT_RELEASE_NOTES = (
    _load_current_release_metadata()
)
_PRODUCT_NAME = "B.A.C.U.P."
_PRODUCT_LONG_NAME = "Bethesda Asset Converter Universal Platform"
_LABELS = {
    "fo4": "Fallout 4 (target)",
    "fo76": "Fallout 76 (source)",
    "fnv": "Fallout: New Vegas (source)",
    "fo3": "Fallout 3 (grafted source)",
    "skyrimse": "Skyrim Special Edition (source)",
}
_WORKSPACE_ID = "appalachia"
_CLEANUP_EXTRACTED_KEY = "cleanup_app_owned_extracted"
_CLEANUP_MOD_OUTPUT_KEY = "cleanup_mod_output_after_deploy"
_APP_OWNED_EXTRACTED_GAMES_KEY = "app_owned_extracted_games"
_APP_OWNED_EXTRACTED_PATHS_KEY = "app_owned_extracted_paths"
_PENDING_PROJECT_SETUP_KEY = "pending_project_setup"
_ACTIVE_PROJECT_KEY = "active_conversion_project"
_ALPHA_ACCEPTED_KEY = "alpha_v0_0_1_accepted"
_PERSONAL_USE_ACCEPTED_KEY = "personal_use_only_accepted"
_STEAM_OWNERSHIP_ACCEPTED_KEY = "steam_owned_not_pirated_accepted"
_STEAM_REQUIREMENT_TEXT = (
    "Steam copies are required for every game used by the selected conversion "
    "project. Microsoft Store and GOG installs are not supported right now."
)
_STEAM_OWNERSHIP_CHECKBOX = (
    "I verify that I own the selected project's games on Steam, have not pirated "
    "them, and am using Steam-installed copies."
)
_FEATURE_STATUS_TITLE = f"{_CURRENT_RELEASE_LABEL} Status - Expect Random Crashes"
_FEATURE_STATUS_INTRO = (
    "The converter passes through all major asset categories, but many things "
    "are still rough."
)
_FEATURE_STATUS_WORKS_BEST = (
    "Worldspace and terrain",
    "Static objects and environment assets",
    "Textures, materials, sounds, and LOD",
)
_FEATURE_STATUS_NOT_PRIORITY = (
    "Creatures",
    "Weapons",
)
_FEATURE_STATUS_FOOTER = (
    "Creatures and weapons may work, partially work, or not work at all."
)
_FOOTER_BUTTON_WIDTH = 160.0
_FOOTER_HEIGHT = 52.0
_SETUP_WINDOW_SIZE = (900, 680)
_STEAM_INSTALL_REQUIRED_GAMES = {"fo4", "fo76", "fnv", "fo3", "skyrimse"}


@dataclass(frozen=True)
class ProjectSetupProfile:
    id: str
    title: str
    source_label: str
    conversion_id: str
    games: tuple[str, ...]
    source_games: tuple[str, ...]
    generated_mod_name: str
    description: str


@dataclass(frozen=True)
class ProjectSetupOwnership:
    cleanup_extracted: bool
    cleanup_mod_output: bool
    owned_games: frozenset[str]
    owned_paths: dict[str, str]


PROJECT_PROFILES = {
    "appalachia": ProjectSetupProfile(
        id="appalachia",
        title="Tales From Appalachia",
        source_label="Fallout 76",
        conversion_id="fo76:fo4",
        games=("fo4", "fo76"),
        source_games=("fo76",),
        generated_mod_name="SeventySix",
        description="Convert Fallout 76's Appalachia into a Fallout 4 mod.",
    ),
    "wasteland": ProjectSetupProfile(
        id="wasteland",
        title="Legends of the Wasteland",
        source_label="Fallout: New Vegas + Fallout 3 (MVP)",
        conversion_id="fnvfo3:fo4",
        games=("fo4", "fnv", "fo3"),
        source_games=("fnv", "fo3"),
        generated_mod_name="FNV_FO3_Merged",
        description="Convert Fallout: New Vegas and grafted Fallout 3 content into Fallout 4.",
    ),
    "north": ProjectSetupProfile(
        id="north",
        title="Fables of the North",
        source_label="Skyrim Special Edition (MVP)",
        conversion_id="skyrimse:fo4",
        games=("fo4", "skyrimse"),
        source_games=("skyrimse",),
        generated_mod_name="Skyrim_Merged",
        description="Convert Skyrim Special Edition's northern lands into Fallout 4.",
    ),
}

# Compatibility for callers and tests written for the original single project.
_GAMES = PROJECT_PROFILES["appalachia"].games


def get_project_profile(project_id: str) -> ProjectSetupProfile:
    try:
        return PROJECT_PROFILES[project_id]
    except KeyError as exc:
        raise ValueError(f"Unknown B.A.C.U.P. project: {project_id}") from exc


@lru_cache(maxsize=None)
def _current_release_notes(project_id: str) -> tuple[str, ...]:
    try:
        manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
        current = next(
            version for version in manifest.versions if version.id == manifest.current
        )
    except Exception as exc:
        _log.warning("Unable to load current release notes: %s", exc)
        return ()
    return current.notes_for_conversion(get_project_profile(project_id).conversion_id)


def _project_workspace_key(base_key: str, project_id: str) -> str:
    return base_key if project_id == "appalachia" else f"{project_id}_{base_key}"


def _workspace_settings(settings) -> dict:
    get_workspace = getattr(settings, "get_workspace_settings", None)
    if callable(get_workspace):
        return dict(get_workspace(_WORKSPACE_ID) or {})
    return {}


def _persist_workspace_values(settings, values: dict) -> None:
    set_workspace = getattr(settings, "set_workspace_settings", None)
    if callable(set_workspace):
        set_workspace(_WORKSPACE_ID, values)
    save = getattr(settings, "save", None)
    if callable(save):
        save()


def request_project_setup(settings, project_id: str) -> None:
    """Persist a project-specific setup request across an application restart."""
    get_project_profile(project_id)
    _persist_workspace_values(
        settings,
        {
            _PENDING_PROJECT_SETUP_KEY: project_id,
            _ACTIVE_PROJECT_KEY: project_id,
        },
    )


def set_active_project(settings, project_id: str) -> None:
    get_project_profile(project_id)
    _persist_workspace_values(settings, {_ACTIVE_PROJECT_KEY: project_id})


def get_active_project(settings) -> str:
    project_id = str(_workspace_settings(settings).get(_ACTIVE_PROJECT_KEY, ""))
    return project_id if project_id in PROJECT_PROFILES else "appalachia"


def get_pending_project_setup(settings) -> str | None:
    project_id = str(_workspace_settings(settings).get(_PENDING_PROJECT_SETUP_KEY, ""))
    return project_id if project_id in PROJECT_PROFILES else None


def clear_pending_project_setup(settings) -> None:
    _persist_workspace_values(settings, {_PENDING_PROJECT_SETUP_KEY: ""})


def get_project_setup_ownership(settings, project_id: str) -> ProjectSetupOwnership:
    """Read cleanup preferences and extraction ownership without exposing keys."""
    get_project_profile(project_id)
    workspace_settings = _workspace_settings(settings)
    games_key = _project_workspace_key(_APP_OWNED_EXTRACTED_GAMES_KEY, project_id)
    paths_key = _project_workspace_key(_APP_OWNED_EXTRACTED_PATHS_KEY, project_id)
    return ProjectSetupOwnership(
        cleanup_extracted=bool(
            workspace_settings.get(
                _project_workspace_key(_CLEANUP_EXTRACTED_KEY, project_id), False
            )
        ),
        cleanup_mod_output=bool(
            workspace_settings.get(
                _project_workspace_key(_CLEANUP_MOD_OUTPUT_KEY, project_id), False
            )
        ),
        owned_games=frozenset(workspace_settings.get(games_key, []) or []),
        owned_paths={
            str(game_id): str(path)
            for game_id, path in dict(
                workspace_settings.get(paths_key, {}) or {}
            ).items()
        },
    )


def project_owns_extracted_path(
    settings,
    project_id: str,
    game_id: str,
    path: Path | str,
    *,
    output_root: Path | None = None,
) -> bool:
    """Return whether a path is an extraction created by the selected project."""
    ownership = get_project_setup_ownership(settings, project_id)
    if game_id not in ownership.owned_games:
        return False
    recorded = ownership.owned_paths.get(game_id, "")
    expected = (
        Path(recorded)
        if recorded
        else (
            Path(output_root)
            if output_root is not None
            else get_exe_dir() / "extracted"
        )
        / game_id
    )
    try:
        return (
            expected.name.lower() == game_id.lower()
            and Path(path).resolve() == expected.resolve()
        )
    except OSError:
        return False


def _estimated_extract_gb(root_dir: str) -> float | None:
    """Rough free-disk estimate for extracting BA2/BSA archives to loose files.
    Sums install Data archives and applies a ~1.4x expansion factor.
    Returns GB, or None if the install/archives can't be read."""
    try:
        data = Path(root_dir) / "Data"
        if not data.is_dir():
            return None
        total = sum(
            p.stat().st_size
            for p in data.iterdir()
            if p.is_file() and p.suffix.lower() in {".ba2", ".bsa"}
        )
        if total <= 0:
            return None
        return (total * 1.4) / (1024**3)
    except OSError:
        return None


def _dir_size_bytes(path: Path) -> int:
    try:
        if path.is_file():
            return path.stat().st_size
        if not path.is_dir():
            return 0
    except OSError:
        return 0

    total = 0
    pending = [os.fspath(path)]
    while pending:
        current = pending.pop()
        try:
            with os.scandir(current) as entries:
                for entry in entries:
                    try:
                        if entry.is_dir(follow_symlinks=False):
                            pending.append(entry.path)
                        elif entry.is_file(follow_symlinks=False):
                            total += entry.stat(follow_symlinks=False).st_size
                    except OSError:
                        continue
        except OSError:
            continue
    return total


def _archive_size_bytes(path: Path) -> int:
    try:
        if not path.is_dir():
            return 0
        return sum(
            item.stat().st_size
            for item in path.iterdir()
            if item.is_file() and item.suffix.lower() in {".ba2", ".bsa"}
        )
    except OSError:
        return 0


def _ba2_size_bytes(path: Path) -> int:
    """Backward-compatible name; now counts both supported archive formats."""
    return _archive_size_bytes(path)


def _format_gb(byte_count: int) -> str:
    return f"{byte_count / (1024**3):.1f} GB"


def _pick_folder(title: str) -> str | None:
    try:
        from creation_lib.ui.widgets.pick_folder import pick_folder

        return pick_folder(title)
    except Exception as e:  # native dialog unavailable / cancelled
        _log.warning("Folder picker failed: %s", e)
        return None


def _has_root(paths: dict) -> bool:
    return bool(paths.get("root_dir"))


def _has_extracted(paths: dict) -> bool:
    return bool(paths.get("extracted_dir"))


@dataclass
class _SpacePrepareResult:
    start_roots: dict[str, str]
    roots: dict[str, str]
    extract_estimates: dict[str, tuple[str, float | None]]
    space_usage: dict[str, int]
    error: str | None = None


def project_setup_needed(settings, project_id: str) -> bool:
    profile = get_project_profile(project_id)
    workspace_settings = _workspace_settings(settings)
    if not workspace_settings.get(_ALPHA_ACCEPTED_KEY, False):
        return True
    if not workspace_settings.get(_PERSONAL_USE_ACCEPTED_KEY, False):
        return True
    if not workspace_settings.get(_STEAM_OWNERSHIP_ACCEPTED_KEY, False):
        return True
    for g in profile.games:
        p = settings.get_game_paths(g)
        if not _has_root(p):
            return True
        if g in profile.source_games and not _has_extracted(p):
            return True
    return False


def appalachia_setup_needed(settings) -> bool:
    return project_setup_needed(settings, "appalachia")


def games_needing_extraction(settings, project_id: str = "appalachia") -> list[str]:
    profile = get_project_profile(project_id)
    return [
        g
        for g in profile.source_games
        if _has_root(settings.get_game_paths(g))
        and not _has_extracted(settings.get_game_paths(g))
    ]


def clear_project_owned_extractions(
    settings,
    project_id: str,
    *,
    output_root: Path | None = None,
) -> tuple[str, ...]:
    """Remove only extraction folders created by one B.A.C.U.P. project."""
    profile = get_project_profile(project_id)
    games_key = _project_workspace_key(_APP_OWNED_EXTRACTED_GAMES_KEY, project_id)
    paths_key = _project_workspace_key(_APP_OWNED_EXTRACTED_PATHS_KEY, project_id)
    ownership = get_project_setup_ownership(settings, project_id)
    owned_games = set(ownership.owned_games)
    owned_paths = dict(ownership.owned_paths)
    default_root = (
        Path(output_root) if output_root is not None else get_exe_dir() / "extracted"
    )
    cleared: list[str] = []

    for game_id in profile.source_games:
        if game_id not in owned_games:
            continue
        current = str(settings.get_game_paths(game_id).get("extracted_dir", "") or "")
        recorded = str(owned_paths.get(game_id, "") or "")
        target = Path(recorded) if recorded else default_root / game_id
        if not current or not project_owns_extracted_path(
            settings,
            project_id,
            game_id,
            current,
            output_root=default_root,
        ):
            continue
        try:
            if target.exists():
                shutil.rmtree(target)
        except OSError as exc:
            _log.warning(
                "Could not clear %s extraction at %s: %s", game_id, target, exc
            )
            continue
        settings.set_game_extracted_dir(game_id, "")
        owned_games.discard(game_id)
        owned_paths.pop(game_id, None)
        cleared.append(game_id)

    _persist_workspace_values(
        settings,
        {games_key: sorted(owned_games), paths_key: owned_paths},
    )
    return tuple(cleared)


class BacupProjectPicker:
    def __init__(self, settings):
        self._settings = settings
        self.selected_project_id = get_active_project(settings)
        self._completed = False

    def select_project(self, project_id: str) -> None:
        get_project_profile(project_id)
        self.selected_project_id = project_id

    def confirm(self) -> None:
        set_active_project(self._settings, self.selected_project_id)
        self._completed = True
        hello_imgui.get_runner_params().app_shall_exit = True

    def run(self) -> str | None:
        params = hello_imgui.RunnerParams()
        params.app_window_params.window_title = f"{_PRODUCT_NAME} - Choose Games"
        set_ini_folder(params, "setup-bacup-project-picker", get_ini_dir())
        params.app_window_params.window_geometry.size = _SETUP_WINDOW_SIZE
        params.imgui_window_params.tweaked_theme = hello_imgui.ImGuiTweakedTheme(
            hello_imgui.ImGuiTheme_.darcula
        )
        params.callbacks.show_gui = self._draw
        params.callbacks.post_init = self._apply_window_icon
        immapp.run(params)
        return self.selected_project_id if self._completed else None

    @staticmethod
    def _apply_window_icon() -> None:
        try:
            from ui.toolkit.app import set_window_icon
            from bacup_ui.variant import BACUP_VARIANT

            set_window_icon(BACUP_VARIANT)
        except Exception as exc:
            _log.warning("Setup window icon failed: %s", exc)
        set_native_dark_title_bar()

    def _draw(self) -> None:
        imgui.text_colored(
            imgui.ImVec4(0.88, 0.94, 1.0, 1.0),
            f"{_PRODUCT_NAME} Setup",
        )
        imgui.text_disabled(f"{_CURRENT_RELEASE_LABEL} - Steam installs required")
        imgui.separator()
        imgui.spacing()
        imgui.text_colored(
            imgui.ImVec4(0.88, 0.94, 1.0, 1.0),
            "Which games do you want to set up?",
        )
        imgui.text_wrapped(
            "Choose the source-game set to extract now. You can configure the "
            "other conversions later from their Setup menu."
        )
        imgui.spacing()
        for profile in PROJECT_PROFILES.values():
            if imgui.radio_button(
                f"{profile.source_label}  ->  Fallout 4##{profile.id}",
                self.selected_project_id == profile.id,
            ):
                self.select_project(profile.id)
            imgui.indent()
            imgui.text_disabled(profile.title)
            imgui.text_wrapped(profile.description)
            imgui.unindent()
            imgui.spacing()
        imgui.separator()
        x = max(0.0, imgui.get_content_region_avail().x - _FOOTER_BUTTON_WIDTH)
        imgui.set_cursor_pos_x(x)
        if imgui.button("Continue", imgui.ImVec2(_FOOTER_BUTTON_WIDTH, 0)):
            self.confirm()


class BacupProjectSetup:
    STEP_WELCOME = 0
    STEP_AGREEMENTS = 1
    STEP_FEATURES = 2
    STEP_SPACE = 3
    STEP_PATHS = 4
    STEP_EXTRACT = 5

    def __init__(self, settings, project_id: str = "appalachia"):
        self._settings = settings
        self.project_id = project_id
        self.profile = get_project_profile(project_id)
        self._games = self.profile.games
        self._source_games = self.profile.source_games
        self._release_notes = _current_release_notes(project_id)
        self.step = self.STEP_WELCOME
        self._roots: dict[str, str] = {
            g: (settings.get_game_paths(g).get("root_dir", "") or "")
            for g in self._games
        }
        # Optional pre-existing extracted dir per game. When set, that game is
        # used as-is and NOT re-extracted.
        self._extracted: dict[str, str] = {
            g: (settings.get_game_paths(g).get("extracted_dir", "") or "")
            for g in self._games
        }
        self._detected = False
        self._extractor: _GameExtractor | None = None
        self._completed = False
        workspace_settings = self._workspace_settings()
        self._cleanup_extracted = bool(
            workspace_settings.get(self._workspace_key(_CLEANUP_EXTRACTED_KEY), False)
        )
        self._cleanup_mod_output = bool(
            workspace_settings.get(self._workspace_key(_CLEANUP_MOD_OUTPUT_KEY), True)
        )
        self._alpha_accepted = bool(workspace_settings.get(_ALPHA_ACCEPTED_KEY, False))
        self._personal_use_accepted = bool(
            workspace_settings.get(_PERSONAL_USE_ACCEPTED_KEY, False)
        )
        self._steam_ownership_accepted = bool(
            workspace_settings.get(_STEAM_OWNERSHIP_ACCEPTED_KEY, False)
        )
        # Cache of extract-size estimates keyed by (game -> (root_dir, gb)) so the
        # per-frame draw does not re-stat the BA2s every frame.
        self._extract_est: dict[str, tuple[str, float | None]] = {}
        self._space_usage_cache: tuple[float, dict[str, int]] | None = None
        self._space_prepare_lock = threading.Lock()
        self._space_prepare_thread: threading.Thread | None = None
        self._space_prepare_running = False
        self._space_prepare_status = ""
        self._space_prepare_progress = 0.0
        self._space_prepare_result: _SpacePrepareResult | None = None
        self._space_prepare_error: str | None = None
        self._steam_install_cache: dict[str, tuple[str, SteamInstallResult]] = {}

    def _workspace_key(self, base_key: str) -> str:
        return _project_workspace_key(base_key, self.project_id)

    def _workspace_settings(self) -> dict:
        return _workspace_settings(self._settings)

    def _extract_estimate_gb(self, g: str) -> float | None:
        root = self._roots.get(g, "")
        cached = self._extract_est.get(g)
        if cached is not None and cached[0] == root:
            return cached[1]
        gb = _estimated_extract_gb(root) if root else None
        self._extract_est[g] = (root, gb)
        return gb

    def _space_usage(self) -> dict[str, int]:
        cached = self._space_usage_cache
        if cached is not None:
            return dict(cached[1])
        return {"extracted": 0, "mod_output": 0, "ba2": 0}

    def _measure_space_usage(self) -> dict[str, int]:
        exe_dir = get_exe_dir()
        extract_root = exe_dir / "extracted"
        mod_output = exe_dir / "mods" / self.profile.generated_mod_name
        return {
            "extracted": sum(
                _dir_size_bytes(extract_root / game_id)
                for game_id in self._source_games
            ),
            "mod_output": _dir_size_bytes(mod_output),
            "ba2": _archive_size_bytes(mod_output),
        }

    def _set_space_prepare_progress(self, status: str, progress: float) -> None:
        with self._space_prepare_lock:
            self._space_prepare_status = status
            self._space_prepare_progress = max(0.0, min(progress, 1.0))

    def _start_space_prepare(self) -> None:
        with self._space_prepare_lock:
            if self._space_prepare_running or self._space_prepare_result is not None:
                return
            if self._space_usage_cache is not None and self._detected:
                return
            self._space_prepare_running = True
            self._space_prepare_status = "Finding installed games..."
            self._space_prepare_progress = 0.05
            self._space_prepare_error = None
            start_roots = dict(self._roots)

        def worker() -> None:
            roots = dict(start_roots)
            estimates: dict[str, tuple[str, float | None]] = {}
            error: str | None = None
            _log.info(
                "Setup preparation started: project=%s games=%s",
                self.project_id,
                ",".join(self._games),
            )
            try:
                game_count = max(1, len(self._games))
                for index, g in enumerate(self._games):
                    self._set_space_prepare_progress(
                        f"Finding {_LABELS[g]}...",
                        0.15 + (index / game_count) * 0.4,
                    )
                    if not roots.get(g):
                        roots[g] = detect_game_path(g) or ""
                self._set_space_prepare_progress("Estimating extraction size...", 0.60)
                _log.info("Setup preparation: estimating source archive sizes")
                for g in self._source_games:
                    root = roots.get(g, "")
                    estimates[g] = (root, _estimated_extract_gb(root) if root else None)
                self._set_space_prepare_progress(
                    "Measuring existing setup data...", 0.78
                )
                _log.info("Setup preparation: measuring existing setup data")
                space_usage = self._measure_space_usage()
            except Exception as exc:
                error = str(exc)
                _log.exception("Setup preparation failed")
                space_usage = {"extracted": 0, "mod_output": 0, "ba2": 0}

            result = _SpacePrepareResult(
                start_roots=start_roots,
                roots=roots,
                extract_estimates=estimates,
                space_usage=space_usage,
                error=error,
            )
            with self._space_prepare_lock:
                self._space_prepare_result = result
                self._space_prepare_status = "Setup check complete."
                self._space_prepare_progress = 1.0
                self._space_prepare_running = False
            _log.info(
                "Setup preparation finished: project=%s error=%s",
                self.project_id,
                error or "none",
            )

        self._space_prepare_thread = threading.Thread(
            target=worker,
            name=f"bacup-{self.project_id}-setup-prep",
            daemon=True,
        )
        self._space_prepare_thread.start()

    def _poll_space_prepare(self) -> None:
        with self._space_prepare_lock:
            result = self._space_prepare_result
            self._space_prepare_result = None
        if result is None:
            return
        for g, root in result.roots.items():
            current = self._roots.get(g, "")
            if root and (not current or current == result.start_roots.get(g, "")):
                self._roots[g] = root
        self._extract_est.update(result.extract_estimates)
        self._space_usage_cache = (time.monotonic(), result.space_usage)
        self._detected = True
        self._space_prepare_error = result.error

    def _space_prepare_state(self) -> tuple[bool, str, float]:
        with self._space_prepare_lock:
            return (
                self._space_prepare_running,
                self._space_prepare_status,
                self._space_prepare_progress,
            )

    def next_step(self) -> None:
        self.step = min(self.step + 1, self.STEP_EXTRACT)
        if self.step == self.STEP_SPACE:
            self._start_space_prepare()

    def agreements_complete(self) -> bool:
        return (
            self._alpha_accepted
            and self._personal_use_accepted
            and self._steam_ownership_accepted
        )

    def persist_agreements(self) -> None:
        set_workspace = getattr(self._settings, "set_workspace_settings", None)
        if callable(set_workspace):
            set_workspace(
                _WORKSPACE_ID,
                {
                    _ALPHA_ACCEPTED_KEY: self._alpha_accepted,
                    _PERSONAL_USE_ACCEPTED_KEY: self._personal_use_accepted,
                    _STEAM_OWNERSHIP_ACCEPTED_KEY: self._steam_ownership_accepted,
                },
            )

    def persist_cleanup_preferences(self) -> None:
        set_workspace = getattr(self._settings, "set_workspace_settings", None)
        if callable(set_workspace):
            set_workspace(
                _WORKSPACE_ID,
                {
                    self._workspace_key(
                        _CLEANUP_EXTRACTED_KEY
                    ): self._cleanup_extracted,
                    self._workspace_key(
                        _CLEANUP_MOD_OUTPUT_KEY
                    ): self._cleanup_mod_output,
                },
            )

    def persist_paths(self) -> None:
        for g in self._games:
            root = self._roots.get(g, "")
            if root:
                self._settings.set_game_root_dir(g, root)
            extracted = self._extracted.get(g, "").strip()
            if extracted:
                self._settings.set_game_extracted_dir(g, extracted)
        self.persist_cleanup_preferences()

    def start_extraction(self, output_root: Path | None = None) -> bool:
        self.persist_paths()
        games = [
            (g, self._roots[g])
            for g in games_needing_extraction(self._settings, self.project_id)
        ]
        if not games:
            return False
        self._extractor = _GameExtractor(
            games, output_root=output_root or get_exe_dir() / "extracted"
        )
        self._extractor.start()
        return True

    def _apply_extraction_results(self) -> None:
        if self._extractor is None:
            return
        completed_games: list[str] = []
        for gid, ext_dir in self._extractor.results.items():
            if gid not in self._source_games:
                continue
            self._settings.set_game_extracted_dir(gid, ext_dir)
            completed_games.append(gid)
        if completed_games:
            ws = self._workspace_settings()
            games_key = self._workspace_key(_APP_OWNED_EXTRACTED_GAMES_KEY)
            paths_key = self._workspace_key(_APP_OWNED_EXTRACTED_PATHS_KEY)
            owned = set(ws.get(games_key, []) or [])
            owned_paths = dict(ws.get(paths_key, {}) or {})
            owned.update(completed_games)
            owned_paths.update(
                {
                    game_id: str(self._extractor.results[game_id])
                    for game_id in completed_games
                }
            )
            set_workspace = getattr(self._settings, "set_workspace_settings", None)
            if callable(set_workspace):
                set_workspace(
                    _WORKSPACE_ID,
                    {games_key: sorted(owned), paths_key: owned_paths},
                )

    def clear_owned_extractions(
        self, *, output_root: Path | None = None
    ) -> tuple[str, ...]:
        cleared = clear_project_owned_extractions(
            self._settings,
            self.project_id,
            output_root=output_root,
        )
        for game_id in cleared:
            self._extracted[game_id] = ""
        return cleared

    def _finish(self) -> None:
        self.persist_agreements()
        self.persist_cleanup_preferences()
        self._settings.setup_complete = True
        self._settings.save()
        self._completed = True

    def _open_converter(self) -> None:
        self._apply_extraction_results()
        self._finish()
        hello_imgui.get_runner_params().app_shall_exit = True

    # ---- imgui loop ----
    def _apply_window_icon(self) -> None:
        try:
            from ui.toolkit.app import set_window_icon
            from bacup_ui.variant import BACUP_VARIANT

            set_window_icon(BACUP_VARIANT)
        except Exception as e:  # icon is cosmetic; never block setup
            _log.warning("Setup window icon failed: %s", e)
        set_native_dark_title_bar()

    def run(self) -> bool:
        params = hello_imgui.RunnerParams()
        params.app_window_params.window_title = f"{_PRODUCT_NAME} - Setup"
        set_ini_folder(params, f"setup-bacup-{self.project_id}", get_ini_dir())
        params.app_window_params.window_geometry.size = _SETUP_WINDOW_SIZE
        params.imgui_window_params.tweaked_theme = hello_imgui.ImGuiTweakedTheme(
            hello_imgui.ImGuiTheme_.darcula
        )
        params.callbacks.show_gui = self._draw
        params.callbacks.post_init = self._apply_window_icon
        immapp.run(params)
        return self._completed

    def _draw(self) -> None:
        self._poll_space_prepare()
        self._draw_setup_header()
        imgui.separator()
        avail = imgui.get_content_region_avail()
        imgui.begin_child(
            "##appalachia_setup_content",
            imgui.ImVec2(0, max(0.0, avail.y - _FOOTER_HEIGHT)),
        )
        self._draw_step_content()
        imgui.end_child()
        imgui.separator()
        self._draw_footer()
        self._draw_loading_mask()

    def _draw_setup_header(self) -> None:
        imgui.text_colored(
            imgui.ImVec4(0.88, 0.94, 1.0, 1.0),
            f"{_PRODUCT_NAME} Setup - {self.profile.title}",
        )
        imgui.same_line()
        imgui.text_disabled(f"Step {self.step + 1} of {self.STEP_EXTRACT + 1}")
        imgui.text_disabled(f"{_CURRENT_RELEASE_LABEL} - Steam installs required")

    @staticmethod
    def _draw_section_title(title: str, subtitle: str = "") -> None:
        imgui.spacing()
        imgui.text_colored(imgui.ImVec4(0.88, 0.94, 1.0, 1.0), title)
        if subtitle:
            imgui.text_wrapped(subtitle)
        imgui.spacing()

    def _draw_step_content(self) -> None:
        if self.step == self.STEP_WELCOME:
            self._draw_welcome()
        elif self.step == self.STEP_AGREEMENTS:
            self._draw_agreements()
        elif self.step == self.STEP_FEATURES:
            self._draw_features()
        elif self.step == self.STEP_SPACE:
            self._draw_space()
        elif self.step == self.STEP_PATHS:
            self._draw_paths()
        else:
            self._draw_extract()

    def _paths_valid(self) -> bool:
        return all(self._game_root_valid(g) for g in self._games)

    def _game_root_valid(self, game_id: str) -> bool:
        root = self._roots.get(game_id, "")
        if not validate_game_path(game_id, root):
            return False
        if game_id not in _STEAM_INSTALL_REQUIRED_GAMES:
            return True
        return self._steam_install_result(game_id).ok

    def _steam_install_result(self, game_id: str) -> SteamInstallResult:
        root = self._roots.get(game_id, "")
        cached = self._steam_install_cache.get(game_id)
        if cached is not None and cached[0] == root:
            return cached[1]
        result = validate_steam_install_for_game(game_id, root)
        self._steam_install_cache[game_id] = (root, result)
        return result

    def _games_without_extracted_dir(self) -> list[str]:
        return [
            game_id
            for game_id in self._source_games
            if not self._extracted.get(game_id, "").strip()
        ]

    def footer_primary_label(self) -> str:
        if self.step == self.STEP_PATHS:
            to_extract = self._games_without_extracted_dir()
            if to_extract:
                return f"Extract {len(to_extract)} game(s) / Continue"
            return "Open Converter"
        if self.step == self.STEP_EXTRACT:
            return "Open Converter"
        return "Continue"

    def footer_primary_enabled(self) -> bool:
        preparing_space, _status, _progress = self._space_prepare_state()
        if self.step in (self.STEP_SPACE, self.STEP_PATHS) and preparing_space:
            return False
        if self.step == self.STEP_AGREEMENTS:
            return self.agreements_complete()
        if self.step == self.STEP_PATHS:
            return self._paths_valid()
        if self.step == self.STEP_EXTRACT:
            return self._extractor is None or self._extractor.done
        return True

    def _run_footer_primary_action(self) -> None:
        if not self.footer_primary_enabled():
            return
        if self.step == self.STEP_AGREEMENTS:
            self.persist_agreements()
            self.next_step()
        elif self.step == self.STEP_SPACE:
            self.persist_cleanup_preferences()
            self.next_step()
        elif self.step == self.STEP_PATHS:
            if self.start_extraction():
                self.next_step()
            else:
                self._open_converter()
        elif self.step == self.STEP_EXTRACT:
            self._open_converter()
        else:
            self.next_step()

    def _draw_footer(self) -> None:
        label = self.footer_primary_label()
        enabled = self.footer_primary_enabled()
        x = max(0.0, imgui.get_content_region_avail().x - _FOOTER_BUTTON_WIDTH)
        imgui.set_cursor_pos_x(x)
        if not enabled:
            imgui.begin_disabled()
        if imgui.button(label, imgui.ImVec2(_FOOTER_BUTTON_WIDTH, 0)):
            self._run_footer_primary_action()
        if not enabled:
            imgui.end_disabled()

    def _draw_loading_mask(self) -> None:
        preparing_space, status, progress = self._space_prepare_state()
        if self.step in (self.STEP_SPACE, self.STEP_PATHS) and preparing_space:
            draw_runner_overlay(f"Preparing {_PRODUCT_NAME}", status, progress)

    def _draw_welcome(self) -> None:
        self._draw_section_title(
            "Welcome",
            self.profile.description,
        )
        imgui.text_wrapped(
            f"{_PRODUCT_LONG_NAME} requires Steam-installed copies of this "
            "project's games. Setup will verify the install folders, estimate "
            "disk use, and extract source archives if loose files are not already "
            "available."
        )
        imgui.spacing()
        imgui.text_disabled(
            "The one-time extraction step is large and can take a while."
        )

    def _draw_agreements(self) -> None:
        self._draw_section_title(
            "Release Terms",
            "These acknowledgements are required before the converter can run.",
        )
        imgui.text_colored(
            imgui.ImVec4(1.0, 0.55, 0.35, 1.0), _CURRENT_RELEASE_LABEL
        )
        imgui.text_wrapped(
            "This is an early release. Bugs, crashes, failed conversions, "
            "incomplete converted content, and long-running setup or conversion "
            "failures should be expected."
        )
        imgui.spacing()
        imgui.text_colored(imgui.ImVec4(0.85, 0.9, 1.0, 1.0), "Personal Use Only")
        imgui.text_wrapped(
            "Personal use only: this app and any generated Fallout 4 mod/output "
            "are for your personal use only. Do not redistribute the app, generated "
            "plugins, archives, extracted files, or converted output."
        )
        imgui.spacing()
        imgui.text_colored(imgui.ImVec4(0.85, 0.9, 1.0, 1.0), "Steam Installs Required")
        imgui.text_wrapped(_STEAM_REQUIREMENT_TEXT)
        imgui.separator()
        imgui.spacing()
        _, self._alpha_accepted = imgui.checkbox(
            (
                f"I understand {_PRODUCT_NAME} is {_CURRENT_RELEASE_LABEL} "
                "and may contain bugs, crash, or fail."
            ),
            self._alpha_accepted,
        )
        _, self._personal_use_accepted = imgui.checkbox(
            "I agree the app and generated output are for personal use only.",
            self._personal_use_accepted,
        )
        _, self._steam_ownership_accepted = imgui.checkbox(
            _STEAM_OWNERSHIP_CHECKBOX,
            self._steam_ownership_accepted,
        )

    def _draw_features(self) -> None:
        self._draw_section_title(_FEATURE_STATUS_TITLE)
        if self._release_notes:
            imgui.text_colored(
                imgui.ImVec4(0.55, 0.78, 1.0, 1.0),
                f"What's new in {_CURRENT_RELEASE_LABEL}",
            )
            for note in self._release_notes:
                imgui.bullet_text(note)
            imgui.separator()
            imgui.spacing()
        imgui.spacing()
        imgui.text_wrapped(_FEATURE_STATUS_INTRO)
        imgui.spacing()
        imgui.text_colored(imgui.ImVec4(0.55, 0.9, 0.55, 1.0), "Expected to work best")
        for item in _FEATURE_STATUS_WORKS_BEST:
            imgui.text(f"- {item}")
        imgui.spacing()
        imgui.text_colored(imgui.ImVec4(1.0, 0.75, 0.35, 1.0), "Not a priority yet")
        for item in _FEATURE_STATUS_NOT_PRIORITY:
            imgui.text(f"- {item}")
        imgui.spacing()
        imgui.text_wrapped(_FEATURE_STATUS_FOOTER)

    def _draw_space(self) -> None:
        self._start_space_prepare()
        self._poll_space_prepare()
        exe_dir = get_exe_dir()
        extract_root = exe_dir / "extracted"
        mod_output = exe_dir / "mods" / self.profile.generated_mod_name
        space_usage = self._space_usage()
        install_estimates = [
            self._extract_estimate_gb(game_id) for game_id in self._source_games
        ]
        extract_estimate = sum(gb for gb in install_estimates if gb)

        self._draw_section_title(
            "Disk Use",
            "Review estimated extraction size and cleanup behavior before conversion.",
        )
        preparing_space, _status, _progress = self._space_prepare_state()
        if preparing_space:
            imgui.spacing()
            imgui.text_disabled("Checking game paths and current disk use...")
            return
        if self._space_prepare_error:
            imgui.spacing()
            imgui.text_colored(
                imgui.ImVec4(1.0, 0.5, 0.3, 1.0),
                f"Setup check warning: {self._space_prepare_error}",
            )
        imgui.spacing()
        imgui.text(f"Default extraction folders: {extract_root}")
        imgui.text(
            f"Current app-owned extracted dirs: {_format_gb(space_usage['extracted'])}"
        )
        if extract_estimate:
            imgui.text(f"Estimated fresh extraction: {extract_estimate:.0f} GB")
        else:
            imgui.text_disabled("Estimated fresh extraction: choose game paths next")
        imgui.spacing()
        imgui.text(f"Generated mod output: {mod_output}")
        imgui.text(f"Current total mod output: {_format_gb(space_usage['mod_output'])}")
        imgui.text(f"Current archives in mod output: {_format_gb(space_usage['ba2'])}")
        imgui.spacing()
        _, self._cleanup_mod_output = imgui.checkbox(
            "Remove generated mod output after successful deploy",
            self._cleanup_mod_output,
        )
        _, self._cleanup_extracted = imgui.checkbox(
            "Remove extracted dirs created by this app after successful deploy",
            self._cleanup_extracted,
        )

    def _draw_paths(self) -> None:
        self._start_space_prepare()
        self._poll_space_prepare()
        preparing_space, _status, _progress = self._space_prepare_state()
        if preparing_space:
            imgui.text_disabled("Checking game paths...")
            return
        self._draw_section_title(
            "Game Paths",
            "Select this project's Steam install folders and optional extracted data folders.",
        )
        imgui.text_wrapped(
            "Source games require extracted data folders. Fallout 4 reads official "
            "BA2s on demand; its extracted folder is only an optional loose-file "
            "development override."
        )
        imgui.spacing()
        green = imgui.ImVec4(0.3, 0.9, 0.3, 1.0)
        orange = imgui.ImVec4(1.0, 0.5, 0.3, 1.0)
        for g in self._games:
            imgui.text_colored(imgui.ImVec4(0.88, 0.94, 1.0, 1.0), _LABELS[g])

            imgui.text_disabled("Install folder:")
            browse_label = f"Browse...##root_{g}"
            browse_w = _browse_button_width(browse_label)
            imgui.set_next_item_width(_browse_input_width(browse_label))
            changed, value = imgui.input_text(f"##root_{g}", self._roots.get(g, ""))
            if changed:
                self._roots[g] = value
            imgui.same_line()
            if imgui.button(browse_label, imgui.ImVec2(browse_w, 0)):
                picked = _pick_folder(f"Select {_LABELS[g]} install folder")
                if picked:
                    self._roots[g] = picked
            local_ok = bool(self._roots.get(g)) and validate_game_path(
                g, self._roots[g]
            )
            ok = local_ok
            steam_result = None
            if local_ok and g in _STEAM_INSTALL_REQUIRED_GAMES:
                steam_result = self._steam_install_result(g)
                ok = steam_result.ok
            imgui.text_colored(green if ok else orange, "OK" if ok else "invalid")
            if local_ok and steam_result is not None and not steam_result.ok:
                imgui.text_disabled(steam_result.message)

            imgui.text_disabled("Extracted folder (optional):")
            browse_label = f"Browse...##ext_{g}"
            browse_w = _browse_button_width(browse_label)
            imgui.set_next_item_width(_browse_input_width(browse_label))
            ext_changed, ext_value = imgui.input_text(
                f"##ext_{g}", self._extracted.get(g, "")
            )
            if ext_changed:
                self._extracted[g] = ext_value
            imgui.same_line()
            if imgui.button(browse_label, imgui.ImVec2(browse_w, 0)):
                picked = _pick_folder(f"Select {_LABELS[g]} extracted data folder")
                if picked:
                    self._extracted[g] = picked
            ext = self._extracted.get(g, "").strip()
            if not ext:
                if g not in self._source_games:
                    imgui.text_disabled("not needed")
                else:
                    gb = self._extract_estimate_gb(g)
                    imgui.text_disabled(
                        "will extract" if gb is None else f"will extract (~{gb:.0f} GB)"
                    )
            elif Path(ext).is_dir():
                imgui.text_colored(green, "use existing")
            else:
                imgui.text_colored(orange, "not found")
            imgui.separator()

        to_extract = self._games_without_extracted_dir()
        if to_extract:
            estimates = [self._extract_estimate_gb(g) for g in to_extract]
            total = sum(e for e in estimates if e)
            size_note = f" needing ~{total:.0f} GB of free disk space" if total else ""
            imgui.spacing()
            imgui.text_colored(
                orange,
                f"No extracted folder for {len(to_extract)} game(s) - they will be "
                f"extracted now next to the app{size_note}. This is a large, "
                f"one-time step.",
            )
            imgui.spacing()

    def _draw_extract(self) -> None:
        if self._extractor is None:
            self._draw_section_title("Extraction")
            imgui.text(
                "Nothing to extract - all source games already have extracted data."
            )
            return
        self._draw_section_title("Extraction")
        imgui.progress_bar(self._extractor.progress, imgui.ImVec2(-1, 0))
        imgui.text_wrapped(self._extractor.status)
        if self._extractor.done:
            self._apply_extraction_results()
            if self._extractor.error:
                imgui.text_colored(
                    imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
                    f"Extraction error: {self._extractor.error}",
                )


# Historical names remain valid for the original Tales From Appalachia flow.
AppalachiaSetup = BacupProjectSetup
