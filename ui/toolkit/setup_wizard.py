"""First-run setup wizard for the ModBox21.

Multi-step ImGui wizard that runs as a separate immapp.run() invocation
before the main app. Handles game selection, path detection, user
preferences, extraction, and database building.
"""

import logging
import os
import threading
from pathlib import Path

from imgui_bundle import hello_imgui, imgui, immapp

from creation_lib.ui.theme.window_chrome import set_ini_folder, set_native_dark_title_bar
from .app_paths import get_ini_dir
from creation_lib.core.game_profiles import GAME_PROFILES
from .app import set_window_icon as _set_window_icon
from .app_paths import get_app_root, get_db_dir
from .db_builder import DbBuilder
from .path_detector import detect_game_path, validate_game_path
from .settings import ToolkitSettings

_log = logging.getLogger("toolkit.setup_wizard")
_DEFAULT_EXTRACT_WORKERS = 8


def _default_extract_workers() -> int:
    return max(1, min(_DEFAULT_EXTRACT_WORKERS, os.cpu_count() or 1))

# Wizard steps
STEP_WELCOME = 0
STEP_GAME_SELECT = 1
STEP_GAME_PATHS = 2
STEP_EXTRACT = 3
STEP_MOD_PREFIX = 4
STEP_BUILD = 5
STEP_COUNT = 6

_STEP_TITLES = [
    "Welcome",
    "Game Selection",
    "Game Paths & Data",
    "Extract Game Data",
    "Mod Prefix",
    "Extract YAML & Build Indexes",
]


def _visible_imgui_label(label: str) -> str:
    return label.split("##", 1)[0]


def _browse_button_width(label: str) -> float:
    style = imgui.get_style()
    text_width = imgui.calc_text_size(_visible_imgui_label(label)).x
    return max(80.0, text_width + style.frame_padding.x * 2.0 + 16.0)


def _browse_input_width(button_label: str) -> float:
    style = imgui.get_style()
    available = imgui.get_content_region_avail().x
    reserved = _browse_button_width(button_label) + style.item_spacing.x
    remaining = available - reserved
    if remaining >= 120.0:
        return remaining
    return max(1.0, remaining)

# Settings keys match game profile IDs directly
_PROFILE_TO_SETTINGS_KEY = {
    "fo4": "fo4",
    "skyrimse": "skyrimse",
    "fo76": "fo76",
    "starfield": "starfield",
    "fo3": "fo3",
    "fnv": "fnv",
}

# Map game profile id -> db game prefix
_PROFILE_TO_DB_GAME = {
    "fo4": "fo4",
    "skyrimse": "skyrimse",
    "starfield": "starfield",
}


def _post_init() -> None:
    _set_window_icon()
    set_native_dark_title_bar()


class _GameExtractor:
    """Background thread runner for extracting BA2/BSA archives."""

    def __init__(self, games: list[tuple[str, str]], *, output_root: Path | None = None):
        """games: list of (game_id, game_root_dir) tuples to extract.

        output_root: directory under which each game's `<game_id>/` extracted
        tree is written. Defaults to ``get_app_root()/extracted`` — never
        ``get_code_root()`` (that is ``_MEIPASS``, read-only in frozen builds).
        """
        self._games = games
        self._output_root = output_root if output_root is not None else get_app_root() / "extracted"
        self._thread: threading.Thread | None = None
        self._lock = threading.Lock()
        self._progress: float = 0.0
        self._status: str = "Waiting..."
        self._done: bool = False
        self._error: str = ""
        self._results: dict[str, str] = {}  # game_id -> extracted_dir

    @property
    def progress(self) -> float:
        with self._lock:
            return self._progress

    @property
    def status(self) -> str:
        with self._lock:
            return self._status

    @property
    def done(self) -> bool:
        with self._lock:
            return self._done

    @property
    def error(self) -> str:
        with self._lock:
            return self._error

    @property
    def results(self) -> dict[str, str]:
        with self._lock:
            return dict(self._results)

    def _set_state(self, **kwargs):
        with self._lock:
            for k, v in kwargs.items():
                setattr(self, f"_{k}", v)

    def start(self):
        self._thread = threading.Thread(
            target=self._run,
            name="game-archive-extraction",
            daemon=True,
        )
        self._thread.start()

    def _run(self):
        from concurrent.futures import ThreadPoolExecutor, as_completed
        from pathlib import Path as P
        from creation_lib.preprocessor.extraction import (
            build_manifest,
            extract_one,
            find_archives,
            group_archives_by_update_phase,
            plan_archive_extraction_batches,
            save_manifest,
        )

        n = len(self._games)

        for idx, (game_id, game_root) in enumerate(self._games):
            profile = GAME_PROFILES.get(game_id)
            if not profile:
                continue

            game_base = idx / n
            game_share = 1.0 / n

            output_dir = self._output_root / game_id
            self._set_state(
                status=f"Extracting {profile.display_name} ({idx + 1}/{n})...",
                progress=game_base,
            )

            try:
                output_dir.mkdir(parents=True, exist_ok=True)
                data_dir = P(game_root) / "Data"
                if not data_dir.is_dir():
                    _log.warning("Data dir not found: %s", data_dir)
                    continue

                archives = find_archives(data_dir, profile.archive_format)
                if not archives:
                    _log.warning("No archives found in %s", data_dir)
                    continue

                total = len(archives)
                workers = _default_extract_workers()
                completed = 0
                total_files = 0
                _log.info(
                    "Archive extraction starting: game=%s archives=%d worker_budget=%d output=%s",
                    game_id,
                    total,
                    workers,
                    output_dir,
                )
                self._set_state(
                    status=f"Extracting {profile.display_name}: {total} archive(s) with {workers} worker(s)",
                    progress=game_base,
                )
                def _progress_callback(archive_name: str):
                    def _progress(event: dict) -> bool:
                        completed_files = int(event.get("completed", 0) or 0)
                        total_archive_files = int(event.get("total", 0) or 0)
                        self._set_state(
                            status=(
                                f"Extracting {profile.display_name}: {archive_name} "
                                f"{completed_files:,}/{total_archive_files:,} file(s)"
                            )
                        )
                        return True

                    return _progress

                for archive_group in group_archives_by_update_phase(archives):
                    for batch in plan_archive_extraction_batches(archive_group, workers):
                        _log.info(
                            "Archive extraction batch: game=%s %s",
                            game_id,
                            ", ".join(
                                f"{task.archive.name}[files={task.file_count},workers={task.file_workers}]"
                                for task in batch
                            ),
                        )
                        with ThreadPoolExecutor(max_workers=len(batch)) as pool:
                            futures = {
                                pool.submit(
                                    extract_one,
                                    task.archive,
                                    output_dir,
                                    profile.archive_format,
                                    task.file_workers,
                                    _progress_callback(task.archive.name),
                                ): task.archive
                                for task in batch
                            }
                            for future in as_completed(futures):
                                archive = futures[future]
                                _archive, count, error = future.result()
                                completed += 1
                                if error:
                                    _log.error("Error extracting %s: %s", archive.name, error)
                                else:
                                    total_files += int(count)
                                self._set_state(
                                    status=(
                                        f"Extracting {profile.display_name}: "
                                        f"{completed}/{total} archive(s), {total_files:,} file(s)"
                                    ),
                                    progress=game_base + (completed / total) * game_share,
                                )
                self._set_state(
                    progress=game_base + game_share,
                    status=f"Extracted {profile.display_name} ({total} archives)",
                )
                _log.info(
                    "Archive extraction finished: game=%s archives=%d files=%d",
                    game_id,
                    total,
                    total_files,
                )

                # Write manifest
                manifest = build_manifest(game_id, data_dir, archives)
                save_manifest(output_dir, manifest)

                with self._lock:
                    self._results[game_id] = str(output_dir)

            except Exception as e:
                _log.error("Extraction failed for %s: %s", game_id, e, exc_info=True)
                self._set_state(status=f"Error extracting {profile.display_name}: {e}")

        self._set_state(progress=1.0, status="Extraction complete.", done=True)


class SetupWizard:
    """Multi-step first-run setup wizard."""

    def __init__(self, settings: ToolkitSettings):
        self._settings = settings
        self._step = STEP_WELCOME
        self._completed = False
        self._cancelled = False

        # Step 1: Game Selection — checkboxes for moddable games
        self._moddable_games = [p for p in GAME_PROFILES.values() if p.is_moddable]

        # Step 2: Per-game paths — detect first so selection can reflect results
        self._game_paths: dict[str, dict] = {}
        for p in self._moddable_games:
            detected = detect_game_path(p.id)
            self._game_paths[p.id] = {
                "path": detected or "",
                "valid": validate_game_path(p.id, detected) if detected else False,
                "auto": detected is not None,
            }

        # Auto-select any game that was successfully detected and validated
        self._selected_games: dict[str, bool] = {
            p.id: self._game_paths[p.id]["valid"] for p in self._moddable_games
        }
        # Always select FO4 by default even if not detected
        if "fo4" in self._selected_games and not self._selected_games["fo4"]:
            self._selected_games["fo4"] = True

        # Step 2 also: Extracted data dirs (shown alongside base paths)
        self._extracted_dirs: dict[str, str] = {p.id: "" for p in self._moddable_games}
        self._extracted_valid: dict[str, bool] = {
            p.id: False for p in self._moddable_games
        }

        # Step 3: Extraction
        self._extract_games: dict[str, bool] = {}  # game_id -> extract now?
        self._skip_extract_confirmed: bool = False  # user confirmed slower loading
        self._extractor: _GameExtractor | None = None
        self._extract_started: bool = False

        # Step 4: Mod Prefix
        self._mod_prefix: str = settings.mod_prefix or ""

        # Step 5: Build
        self._builder: DbBuilder | None = None
        self._build_queue: list[tuple[str, DbBuilder]] = []  # (game_id, builder)
        self._build_queue_idx: int = 0
        self._build_started: bool = False
        self._build_skipped: bool = False

        # Index selection — per active game
        self._build_indexes: dict[str, bool] = {}  # populated in _init_build_state

    def run(self) -> bool:
        """Run the wizard as a blocking immapp.run() call.

        Returns True if completed, False if cancelled/closed.
        """
        params = hello_imgui.RunnerParams()
        params.app_window_params.window_title = "ModBox21 — Setup"
        set_ini_folder(params, "setup", get_ini_dir())
        params.app_window_params.window_geometry.size = (700, 550)
        params.app_window_params.window_geometry.window_size_state = (
            hello_imgui.WindowSizeState.standard
        )
        params.callbacks.show_gui = self._gui
        params.callbacks.before_exit = self._on_exit
        params.callbacks.post_init = _post_init
        params.imgui_window_params.default_imgui_window_type = (
            hello_imgui.DefaultImGuiWindowType.provide_full_screen_window
        )
        params.imgui_window_params.show_menu_bar = False
        params.imgui_window_params.show_status_bar = False
        params.imgui_window_params.tweaked_theme = hello_imgui.ImGuiTweakedTheme(
            hello_imgui.ImGuiTheme_.darcula
        )
        params.fps_idling.enable_idling = True
        params.fps_idling.fps_idle = 20.0

        immapp.run(runner_params=params)
        return self._completed

    def _on_exit(self):
        if not self._completed:
            self._cancelled = True

    def _gui(self):
        vp = imgui.get_main_viewport()
        imgui.set_next_window_pos(vp.pos)
        imgui.set_next_window_size(vp.size)
        flags = (
            imgui.WindowFlags_.no_decoration
            | imgui.WindowFlags_.no_move
            | imgui.WindowFlags_.no_resize
        )
        imgui.begin("##wizard", flags=flags)

        self._draw_header()
        imgui.separator()
        imgui.spacing()

        avail = imgui.get_content_region_avail()
        imgui.begin_child("##step_content", imgui.ImVec2(0, avail.y - 50))
        if self._step == STEP_WELCOME:
            self._draw_welcome()
        elif self._step == STEP_GAME_SELECT:
            self._draw_game_select()
        elif self._step == STEP_GAME_PATHS:
            self._draw_game_paths()
        elif self._step == STEP_EXTRACT:
            self._draw_extract()
        elif self._step == STEP_MOD_PREFIX:
            self._draw_mod_prefix()
        elif self._step == STEP_BUILD:
            self._draw_build()
        imgui.end_child()

        imgui.separator()
        self._draw_nav_buttons()
        imgui.end()

    def _draw_header(self):
        imgui.text("Setup Wizard")
        imgui.same_line(imgui.get_content_region_avail().x - 220)
        imgui.text_disabled(
            f"Step {self._step + 1} of {STEP_COUNT}: {_STEP_TITLES[self._step]}"
        )

    def _draw_nav_buttons(self):
        btn_w = 100.0
        spacing = 10.0

        # Don't show Cancel/Back/Next during active extraction or build
        extracting = self._extractor and not self._extractor.done
        footer_avail_x = imgui.get_content_region_avail().x

        if self._step < STEP_BUILD and not extracting:
            if imgui.button("Cancel", imgui.ImVec2(btn_w, 0)):
                self._cancelled = True
                hello_imgui.get_runner_params().app_shall_exit = True

        show_back = self._step > STEP_WELCOME and self._step < STEP_BUILD and not extracting
        show_next = self._step < STEP_BUILD and not extracting
        if show_back or show_next:
            right_width = btn_w
            if show_back:
                right_width += btn_w + spacing
            imgui.same_line(max(0.0, footer_avail_x - right_width))

        if show_back:
            if imgui.button("Back", imgui.ImVec2(btn_w, 0)):
                prev = self._step - 1
                # Skip extract step going back if all games have extracted dirs
                if prev == STEP_EXTRACT and not self._games_needing_extraction():
                    prev = STEP_GAME_PATHS
                self._step = prev

        if show_next:
            if show_back:
                imgui.same_line()
            can_next = self._can_advance()
            if not can_next:
                imgui.begin_disabled()

            label = "Next" if self._step < STEP_MOD_PREFIX else "Finish"
            if imgui.button(label, imgui.ImVec2(btn_w, 0)):
                if self._step == STEP_GAME_PATHS:
                    # After paths: go to extract if any games lack extracted dirs
                    if self._games_needing_extraction():
                        self._init_extract_state()
                        self._step = STEP_EXTRACT
                    else:
                        self._step = STEP_MOD_PREFIX
                elif self._step == STEP_EXTRACT:
                    self._step = STEP_MOD_PREFIX
                elif self._step == STEP_MOD_PREFIX:
                    self._apply_settings()
                    self._init_build_state()
                    self._step = STEP_BUILD
                else:
                    self._step += 1

            if not can_next:
                imgui.end_disabled()
        elif self._step == STEP_BUILD:
            imgui.set_cursor_pos_x(max(0.0, footer_avail_x - btn_w))
            all_done = (
                (
                    self._build_queue_idx >= len(self._build_queue) - 1
                    and self._builder is not None
                    and self._builder.done
                )
                if self._build_queue
                else False
            )
            build_done = all_done or self._build_skipped
            if not build_done:
                imgui.begin_disabled()
            if imgui.button("Done", imgui.ImVec2(btn_w, 0)):
                self._completed = True
                hello_imgui.get_runner_params().app_shall_exit = True
            if not build_done:
                imgui.end_disabled()

    def _can_advance(self) -> bool:
        if self._step == STEP_GAME_SELECT:
            return any(self._selected_games.values())
        if self._step == STEP_GAME_PATHS:
            # At least one selected game must have a valid path
            return any(
                self._game_paths[gid]["valid"]
                for gid, sel in self._selected_games.items()
                if sel
            )
        if self._step == STEP_EXTRACT:
            # Can advance if: extraction finished, or confirmed skip
            if self._extractor and self._extractor.done:
                return True
            if self._skip_extract_confirmed:
                return True
            return False
        return True

    def _games_needing_extraction(self) -> list[str]:
        """Return game IDs that are selected+valid but have no extracted dir."""
        result = []
        for p in self._moddable_games:
            gid = p.id
            if not self._selected_games.get(gid):
                continue
            if not self._game_paths[gid]["valid"]:
                continue
            if self._extracted_dirs[gid] and self._extracted_valid[gid]:
                continue
            result.append(gid)
        return result

    def _init_extract_state(self):
        """Populate extraction checkboxes for games needing extraction."""
        self._extract_games = {}
        self._skip_extract_confirmed = False
        self._extractor = None
        self._extract_started = False
        for gid in self._games_needing_extraction():
            self._extract_games[gid] = True  # default: extract

    # ------------------------------------------------------------------
    # Step drawings
    # ------------------------------------------------------------------

    def _draw_welcome(self):
        imgui.spacing()
        imgui.spacing()
        imgui.text("Welcome to the ModBox21!")
        imgui.spacing()
        imgui.text_wrapped(
            "This wizard will help you set up the toolkit for one or more "
            "Bethesda games. You'll select which games to configure, set up "
            "installation paths, configure your preferences, and build the "
            "search indexes needed for the app to work."
        )
        imgui.spacing()
        imgui.text_wrapped(
            "You can change any of these settings later from the Settings menu."
        )
        imgui.spacing()
        imgui.spacing()
        imgui.text_disabled("Click Next to begin.")

    def _draw_game_select(self):
        imgui.text("Select Games to Configure")
        imgui.spacing()
        imgui.text_wrapped(
            "Choose which games you want to set up. You can add more games "
            "later from Settings."
        )
        imgui.spacing()
        imgui.spacing()

        for profile in self._moddable_games:
            changed, val = imgui.checkbox(
                f"{profile.display_name}##{profile.id}",
                self._selected_games[profile.id],
            )
            if changed:
                self._selected_games[profile.id] = val

            # Show auto-detected indicator
            gp = self._game_paths[profile.id]
            if gp["auto"] and gp["valid"]:
                imgui.same_line()
                imgui.text_colored(
                    imgui.ImVec4(0.4, 0.8, 0.4, 1.0),
                    "(auto-detected)",
                )
            imgui.spacing()

    def _draw_game_paths(self):
        imgui.text("Game Paths & Data")
        imgui.spacing()
        imgui.text_wrapped(
            "Set the installation directory and (optionally) an extracted data folder "
            "for each game. Extracted folders contain loose files (Meshes/, Textures/) "
            "and allow much faster indexing."
        )
        imgui.spacing()

        selected = [
            p for p in self._moddable_games if self._selected_games.get(p.id, False)
        ]

        for profile in selected:
            gid = profile.id
            gp = self._game_paths[gid]
            imgui.push_id(gid)

            imgui.text(profile.display_name)
            if gp["auto"]:
                imgui.same_line()
                imgui.text_colored(imgui.ImVec4(0.4, 0.8, 0.4, 1.0), "(auto-detected)")

            # Base install dir
            imgui.text_disabled("Install Directory:")
            browse_label = "Browse##base"
            browse_w = _browse_button_width(browse_label)
            imgui.set_next_item_width(_browse_input_width(browse_label))
            changed, new_val = imgui.input_text("##path", gp["path"])
            if changed:
                gp["path"] = new_val
                gp["valid"] = validate_game_path(gid, new_val)

            imgui.same_line()
            if imgui.button(browse_label, imgui.ImVec2(browse_w, 0)):
                path = _pick_folder(f"Select {profile.display_name} Directory")
                if path:
                    gp["path"] = path
                    gp["valid"] = validate_game_path(gid, path)

            if gp["path"]:
                if gp["valid"]:
                    exe = profile.executable_name or "executable"
                    imgui.text_colored(
                        imgui.ImVec4(0.3, 0.9, 0.3, 1.0),
                        f"Valid — {exe} and Data/ found.",
                    )
                else:
                    imgui.text_colored(
                        imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
                        "Invalid — executable or Data/ not found.",
                    )
            else:
                imgui.text_disabled("Enter the installation path.")

            # Extracted dir (optional)
            if gp["valid"]:
                imgui.spacing()
                imgui.text_disabled("Extracted Data Directory (optional):")
                browse_label = "Browse##ext"
                browse_w = _browse_button_width(browse_label)
                imgui.set_next_item_width(_browse_input_width(browse_label))
                changed, val = imgui.input_text("##ext", self._extracted_dirs[gid])
                if changed:
                    self._extracted_dirs[gid] = val
                    self._extracted_valid[gid] = self._validate_extracted(val)

                imgui.same_line()
                if imgui.button(browse_label, imgui.ImVec2(browse_w, 0)):
                    path = _pick_folder("Select Extracted Data Directory")
                    if path:
                        self._extracted_dirs[gid] = path
                        self._extracted_valid[gid] = self._validate_extracted(path)

                if self._extracted_dirs[gid]:
                    if self._extracted_valid[gid]:
                        imgui.text_colored(
                            imgui.ImVec4(0.3, 0.9, 0.3, 1.0),
                            "Valid — Meshes/ directory found.",
                        )
                    else:
                        imgui.text_colored(
                            imgui.ImVec4(1.0, 0.8, 0.3, 1.0),
                            "No Meshes/ directory found — will check subdirectories.",
                        )

            imgui.pop_id()
            imgui.spacing()
            imgui.separator()
            imgui.spacing()

    def _draw_extract(self):
        imgui.text("Extract Game Data")
        imgui.spacing()
        imgui.text_colored(
            imgui.ImVec4(1.0, 1.0, 1.0, 1.0),
            "Extracted game data is required for enhanced features.",
        )
        imgui.text_wrapped(
            "The NIF mesh index, behavior graph index, and asset search all depend "
            "on loose extracted files. Without an extracted data folder, these features "
            "will be disabled or unavailable."
        )
        imgui.spacing()

        # Show extraction progress if running
        if self._extractor:
            progress = self._extractor.progress
            status = self._extractor.status

            imgui.text("Extracting archives...")
            imgui.spacing()
            imgui.progress_bar(progress, imgui.ImVec2(-1, 0))
            imgui.spacing()
            imgui.text_wrapped(status)

            if self._extractor.done:
                # Apply extracted dirs from results
                for gid, ext_dir in self._extractor.results.items():
                    self._extracted_dirs[gid] = ext_dir
                    self._extracted_valid[gid] = self._validate_extracted(ext_dir)

                if self._extractor.error:
                    imgui.spacing()
                    imgui.text_colored(
                        imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
                        f"Extraction error: {self._extractor.error}",
                    )
                else:
                    imgui.spacing()
                    imgui.text_colored(
                        imgui.ImVec4(0.3, 0.9, 0.3, 1.0),
                        "Extraction complete!",
                    )
                imgui.spacing()
                imgui.text_disabled("Click Next to continue.")
            return

        # Not started yet — show options
        needs = self._games_needing_extraction()
        if not needs:
            imgui.text_colored(
                imgui.ImVec4(0.4, 0.8, 0.4, 1.0),
                "All selected games have extracted data directories.",
            )
            self._skip_extract_confirmed = True
            return

        imgui.text_wrapped(
            "The following games do not have an extracted data folder. "
            "Extracting the game archives (BA2/BSA) into loose files allows "
            "much faster indexing and asset loading."
        )
        imgui.spacing()
        imgui.text_wrapped(
            "Without extracted data, the toolkit must parse archive files "
            "each time it needs to access game assets, which is significantly slower."
        )
        imgui.spacing()
        imgui.spacing()

        imgui.text("Select games to extract:")
        imgui.spacing()

        for gid in needs:
            profile = GAME_PROFILES.get(gid)
            if not profile:
                continue
            _, self._extract_games[gid] = imgui.checkbox(
                f"{profile.display_name}##{gid}",
                self._extract_games.get(gid, True),
            )

        any_selected = any(self._extract_games.values())

        imgui.spacing()
        imgui.spacing()

        if any_selected:
            self._skip_extract_confirmed = False
            if imgui.button("Extract Now", imgui.ImVec2(120, 0)):
                self._start_extraction()
            imgui.same_line()
            imgui.text_disabled("This may take a while depending on game size.")
        else:
            # No games selected for extraction — show warning
            imgui.text_colored(
                imgui.ImVec4(1.0, 0.8, 0.3, 1.0),
                "Warning:",
            )
            imgui.same_line()
            imgui.text_wrapped(
                "Without extracted data, loading will be slower as the toolkit "
                "must parse BA2/BSA archive files each time it accesses game assets. "
                "NIF mesh and behavior indexing will also take longer."
            )
            imgui.spacing()
            _, self._skip_extract_confirmed = imgui.checkbox(
                "I understand — continue without extracting",
                self._skip_extract_confirmed,
            )

    def _start_extraction(self):
        """Launch background extraction for selected games."""
        games = []
        for gid, do_extract in self._extract_games.items():
            if do_extract:
                game_root = self._game_paths[gid]["path"]
                if game_root:
                    games.append((gid, game_root))
        if not games:
            return
        self._extractor = _GameExtractor(games)
        self._extractor.start()
        self._extract_started = True

    def _draw_mod_prefix(self):
        imgui.text("Author Mod Prefix")
        imgui.spacing()
        imgui.text_wrapped(
            "Your mod prefix is used as a naming convention for all mods you create. "
            "It's prepended to mod names, EditorIDs, and keywords to avoid conflicts. "
            "For example, if your prefix is 'ABC', mods will be named 'ABC_ModName'."
        )
        imgui.spacing()
        imgui.text("Prefix:")
        imgui.set_next_item_width(200)
        _, self._mod_prefix = imgui.input_text("##mod_prefix", self._mod_prefix)

        imgui.spacing()
        if self._mod_prefix:
            imgui.text_disabled(f"Example mod name: {self._mod_prefix}_MyWeapon")
        else:
            imgui.text_disabled("Leave blank to set later in Settings.")

    def _init_build_state(self):
        """Populate build index toggles based on selected games."""
        db_dir = get_db_dir()
        self._build_indexes = {}

        # First selected+valid game becomes the active game
        first_game = None
        for p in self._moddable_games:
            gid = p.id
            if not self._selected_games.get(gid) or not self._game_paths[gid]["valid"]:
                continue
            if first_game is None:
                first_game = gid

            self._build_indexes[f"{gid}:voice_reference"] = not self._voice_index_exists(gid)

            db_game = _PROFILE_TO_DB_GAME.get(gid)
            if db_game is None:
                continue

            self._build_indexes[f"{gid}:records"] = not (
                db_dir / f"{db_game}_records.db"
            ).is_file()
            self._build_indexes[f"{gid}:nifs"] = not (
                db_dir / f"{db_game}_nifs.db"
            ).is_file()
            if p.has_havok_behaviors:
                self._build_indexes[f"{gid}:behaviors"] = not (
                    db_dir / f"{db_game}_havok.db"
                ).is_file()

        # Set the active game to the first selected one
        if first_game:
            self._settings.set_active_game(first_game)

    def _draw_build(self):
        imgui.text("Extract YAML & Build Indexes")
        imgui.spacing()
        imgui.text_wrapped(
            "ModBox21 serializes your game's ESM/ESL plugins into YAML, "
            "then builds fast search indexes from them. This powers the Records search, "
            "NIF browser, behavior editor, and Voice Browser — all offline, without re-parsing archives."
        )
        imgui.spacing()
        imgui.text_colored(
            imgui.ImVec4(1.0, 1.0, 1.0, 1.0),
            "This step is optional.",
        )
        imgui.text_disabled("You can skip it and run it later from Settings > Indexes.")
        imgui.separator()
        imgui.spacing()

        if not self._build_indexes:
            imgui.text_disabled("No games with valid paths selected. Nothing to build.")
            self._build_skipped = True
            imgui.spacing()
            imgui.text_disabled("Click Done to start the toolkit.")
            return

        # Check if all enabled indexes already exist
        all_exist = not any(self._build_indexes.values())
        if all_exist and not self._build_started:
            imgui.text_colored(
                imgui.ImVec4(0.4, 0.8, 0.4, 1.0),
                "All selected indexes already exist. No build needed.",
            )
            self._build_skipped = True
            imgui.spacing()
            imgui.text_disabled("Click Done to start the toolkit.")
            return

        # Index selection checkboxes
        if not self._build_started and not self._build_skipped:
            imgui.text("Select what to build:")
            imgui.spacing()

            for p in self._moddable_games:
                gid = p.id
                if (
                    not self._selected_games.get(gid)
                    or not self._game_paths[gid]["valid"]
                ):
                    continue
                has_db_indexes = _PROFILE_TO_DB_GAME.get(gid) is not None
                has_voice_index = f"{gid}:voice_reference" in self._build_indexes
                if not has_db_indexes and not has_voice_index:
                    continue

                imgui.text(p.display_name)
                imgui.indent(20)

                rkey = f"{gid}:records"
                if has_db_indexes and rkey in self._build_indexes:
                    _, self._build_indexes[rkey] = imgui.checkbox(
                        f"Extract YAML + Records Index (enables Search)##{gid}",
                        self._build_indexes[rkey],
                    )

                nkey = f"{gid}:nifs"
                if has_db_indexes and nkey in self._build_indexes:
                    _, self._build_indexes[nkey] = imgui.checkbox(
                        f"NIF Mesh Index — requires extracted files##{gid}",
                        self._build_indexes[nkey],
                    )

                bkey = f"{gid}:behaviors"
                if has_db_indexes and bkey in self._build_indexes and p.has_havok_behaviors:
                    _, self._build_indexes[bkey] = imgui.checkbox(
                        f"Havok Behavior Index — requires extracted files##{gid}",
                        self._build_indexes[bkey],
                    )

                vkey = f"{gid}:voice_reference"
                if vkey in self._build_indexes:
                    _, self._build_indexes[vkey] = imgui.checkbox(
                        f"Voice Reference Index — enables Voice Browser##{gid}",
                        self._build_indexes[vkey],
                    )

                imgui.unindent(20)
                imgui.spacing()

            imgui.spacing()

            if imgui.button("Skip", imgui.ImVec2(100, 0)):
                for k in self._build_indexes:
                    self._build_indexes[k] = False
                self._build_skipped = True
                return

            imgui.same_line()

            any_selected = any(self._build_indexes.values())
            if not any_selected:
                imgui.begin_disabled()
            if imgui.button("Build Now", imgui.ImVec2(100, 0)):
                self._start_build()
            if not any_selected:
                imgui.end_disabled()

            imgui.spacing()

        # Progress — advance queue when current builder finishes
        if self._build_started and self._builder:
            if self._builder.done:
                next_idx = self._build_queue_idx + 1
                if next_idx < len(self._build_queue):
                    self._build_queue_idx = next_idx
                    self._builder = self._build_queue[next_idx][1]
                    self._builder.start()

            total = len(self._build_queue)
            game_id, _ = self._build_queue[self._build_queue_idx]
            profile = GAME_PROFILES.get(game_id)
            game_label = profile.display_name if profile else game_id

            imgui.spacing()
            if total > 1:
                overall = (self._build_queue_idx + self._builder.progress) / total
                imgui.text(f"Game {self._build_queue_idx + 1} of {total}: {game_label}")
                imgui.progress_bar(overall, imgui.ImVec2(-1, 0))
                imgui.spacing()
            else:
                imgui.text(game_label)

            imgui.progress_bar(self._builder.progress, imgui.ImVec2(-1, 0))
            imgui.spacing()
            imgui.text_wrapped(self._builder.status)

            all_done = self._build_queue_idx >= total - 1 and self._builder.done
            if all_done:
                imgui.spacing()
                if self._builder.error:
                    imgui.text_colored(
                        imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
                        f"Build error: {self._builder.error}",
                    )
                    imgui.text_disabled(
                        "You can rebuild indexes later from Settings > Indexes."
                    )
                else:
                    imgui.text_colored(
                        imgui.ImVec4(0.3, 0.9, 0.3, 1.0), "Build complete!"
                    )
                imgui.spacing()
                imgui.text_disabled("Click Done to start the toolkit.")

    def _start_build(self):
        """Build a queue of DbBuilders for all selected games and start the first."""
        self._apply_settings()
        self._build_queue = []

        for p in self._moddable_games:
            gid = p.id
            build_records = self._build_indexes.get(f"{gid}:records", False)
            build_nifs = self._build_indexes.get(f"{gid}:nifs", False)
            build_behaviors = self._build_indexes.get(f"{gid}:behaviors", False)
            build_voice = self._build_indexes.get(f"{gid}:voice_reference", False)

            if not (build_records or build_nifs or build_behaviors or build_voice):
                continue

            db_game = _PROFILE_TO_DB_GAME.get(gid) or gid
            game_root = self._game_paths[gid]["path"]
            extracted = self._extracted_dirs.get(gid) or ""

            builder = DbBuilder(
                fo4_root=game_root,
                extracted_dir=extracted,
                build_fo4_data=build_records,
                build_scripts=False,
                build_wiki=False,
                build_nifs=build_nifs,
                build_behaviors=build_behaviors,
                build_voice_reference_index=build_voice,
                game=db_game,
            )
            self._build_queue.append((gid, builder))

        if self._build_queue:
            self._build_queue_idx = 0
            self._builder = self._build_queue[0][1]
            self._builder.start()
            self._build_started = True

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    def _apply_settings(self):
        """Save wizard choices to settings."""
        for p in self._moddable_games:
            gid = p.id
            if not self._selected_games.get(gid):
                continue
            settings_key = _PROFILE_TO_SETTINGS_KEY.get(gid)
            if settings_key is None:
                continue
            gp = self._game_paths[gid]
            if gp["valid"]:
                self._settings._paths[settings_key]["root_dir"] = gp["path"]
            if self._extracted_dirs.get(gid):
                self._settings._paths[settings_key]["extracted_dir"] = (
                    self._extracted_dirs[gid]
                )

        self._settings.mod_prefix = self._mod_prefix

        self._settings.setup_complete = True
        self._settings.save()
        from app.env_sync import export_settings_to_env

        export_settings_to_env(self._settings)

    @staticmethod
    def _validate_extracted(path: str) -> bool:
        if not path:
            return False
        from pathlib import Path as P

        p = P(path)
        return (p / "Meshes").is_dir() or (p / "Data" / "Meshes").is_dir()

    def _voice_index_exists(self, game: str) -> bool:
        from creation_lib.audio.voice_reference import voice_reference_sqlite_cache_path

        root_value = str(self._game_paths.get(game, {}).get("path", "") or "").strip()
        if not root_value:
            return False
        root_dir = Path(root_value).expanduser()
        data_dir = root_dir / "Data"
        if not data_dir.is_dir() and root_dir.name.lower() == "data":
            data_dir = root_dir
        if not data_dir.is_dir():
            return False
        strings_dir = data_dir / "Strings"
        extracted_value = str(self._extracted_dirs.get(game, "") or "").strip()
        if not strings_dir.is_dir() and extracted_value:
            extracted = Path(extracted_value).expanduser()
            for candidate in (extracted / "Strings", extracted / "Data" / "Strings"):
                if candidate.is_dir():
                    strings_dir = candidate
                    break
        cache_path = voice_reference_sqlite_cache_path(
            game=game,
            data_dir=data_dir,
            strings_dir=strings_dir,
            cache_dir=get_app_root() / "data" / "cache",
        )
        return bool(cache_path and cache_path.is_file())


def _pick_folder(title: str = "Select Folder") -> str | None:
    try:
        from creation_lib.ui.widgets.pick_folder import pick_folder
        return pick_folder(title)
    except Exception as e:
        _log.warning("Folder picker failed: %s", e)
        return None
