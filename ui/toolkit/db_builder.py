"""Database builder for first-run setup.

Builds records.db, nifs.db, and behaviors.db from the user's game files.
Records extraction goes through the in-process ESP authoring exporter via
``creation_lib.db.index_builder.regenerate_esm_yaml_cache``.

Runs in a background thread with progress reporting for the setup wizard UI.
"""

import logging
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

_SUBPROCESS_FLAGS = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0

from .app_paths import get_app_root, get_code_root, get_db_dir, get_resource_dir

from creation_lib.core.game_profiles import get_profile
from creation_lib.preprocessor.preprocess_runner import run_preprocess
from creation_lib.db.index_builder import regenerate_esm_yaml_cache
from creation_lib.audio.voice_reference import build_voice_reference, voice_reference_sqlite_cache_path
from creation_lib.ui.host import GAME_ESM_YAML_DIR as _GAME_ESM_YAML_DIR

_log = logging.getLogger("toolkit.db_builder")

_FO4_PLUGIN_WHITELIST = [
    "Fallout4.esm",
    "DLCRobot.esm",
    "DLCworkshop01.esm",
    "DLCCoast.esm",
    "DLCworkshop02.esm",
    "DLCworkshop03.esm",
    "DLCNukaWorld.esm",
    "ccBGSFO4044-HellfirePowerArmor.esl",
    "ccBGSFO4046-TesCan.esl",
    "ccBGSFO4096-AS_Enclave.esl",
    "ccBGSFO4110-WS_Enclave.esl",
    "ccBGSFO4115-X02.esl",
    "ccBGSFO4116-HeavyFlamer.esl",
    "ccFSVFO4007-Halloween.esl",
    "ccOTMFO4001-Remnants.esl",
    "ccSBJFO4003-Grenade.esl",
]

_SHARED_WIKI_DB_GAME = {
    "fnv": "fo3",
}


class DbBuilder:
    """Build user-specific databases (nifs.db, behaviors.db) in a background thread."""

    def __init__(
        self,
        game_root: str = "",
        extracted_dir: str = "",
        build_fo4_data: bool = True,
        build_scripts: bool = True,
        build_wiki: bool = True,
        build_nifs: bool = True,
        build_behaviors: bool = True,
        build_swf: bool = False,
        build_voice_reference_index: bool = False,
        force_rebuild: bool = False,
        game: str = "fo4",
        smart: bool = False,
        # Legacy alias kept for callers using the old keyword
        fo4_root: str = "",
    ):
        self._game_root = game_root or fo4_root
        self._extracted_dir = extracted_dir
        self._game = game
        self._db_dir = get_db_dir()
        self._build_fo4_data = build_fo4_data
        self._build_scripts = build_scripts
        self._build_wiki = build_wiki
        self._build_nifs = build_nifs
        self._build_behaviors = build_behaviors
        self._build_swf = build_swf
        self._build_voice_reference_index = build_voice_reference_index
        self._force_rebuild = force_rebuild
        self._smart = smart

        self._thread: threading.Thread | None = None
        self._lock = threading.Lock()

        # Progress state (read from main thread, written from worker)
        self._progress: float = 0.0
        self._status: str = "Waiting..."
        self._phase: str = "idle"  # idle, nifs, behaviors, done, error
        self._done: bool = False
        self._error: str = ""

    @property
    def progress(self) -> float:
        with self._lock:
            return self._progress

    @property
    def status(self) -> str:
        with self._lock:
            return self._status

    @property
    def phase(self) -> str:
        with self._lock:
            return self._phase

    @property
    def done(self) -> bool:
        with self._lock:
            return self._done

    @property
    def error(self) -> str:
        with self._lock:
            return self._error

    def _set_state(
        self,
        progress: float = None,
        status: str = None,
        phase: str = None,
        done: bool = None,
        error: str = None,
    ):
        with self._lock:
            if progress is not None:
                self._progress = progress
            if status is not None:
                self._status = status
            if phase is not None:
                self._phase = phase
            if done is not None:
                self._done = done
            if error is not None:
                self._error = error

    def start(self):
        """Start building databases in background thread."""
        self._db_dir.mkdir(parents=True, exist_ok=True)
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def _compute_progress_ranges(self) -> dict:
        """Return {phase_name: (start, end)} for each enabled phase, evenly distributed 0->1."""
        phases = []
        if self._build_fo4_data:
            phases.append("fo4_data")
        if self._build_scripts:
            phases.append("scripts")
        if self._build_wiki:
            phases.append("wiki")
        if self._build_nifs:
            phases.append("nifs")
        if self._build_behaviors:
            phases.append("behaviors")
        if self._build_swf:
            phases.append("swf")
        if self._build_voice_reference_index:
            phases.append("voice_reference")
        n = len(phases)
        if n == 0:
            return {}
        return {phase: (i / n, (i + 1) / n) for i, phase in enumerate(phases)}

    def _run(self):
        """Worker thread: build enabled indexes."""
        try:
            ranges = self._compute_progress_ranges()
            if not ranges:
                self._set_state(
                    progress=1.0, status="No indexes selected.", phase="done", done=True
                )
                return
            if self._build_fo4_data:
                start, end = ranges["fo4_data"]
                self._build_records(progress_start=start, progress_end=end)
            if self._build_scripts:
                start, end = ranges["scripts"]
                self._build_scripts_phase(progress_start=start, progress_end=end)
            if self._build_wiki:
                start, end = ranges["wiki"]
                self._build_wiki_phase(progress_start=start, progress_end=end)
            if self._build_nifs:
                start, end = ranges["nifs"]
                self._build_nifs_phase(progress_start=start, progress_end=end)
            if self._build_behaviors:
                start, end = ranges["behaviors"]
                self._build_behaviors_phase(progress_start=start, progress_end=end)
            if self._build_swf:
                start, end = ranges["swf"]
                self._build_swf_phase(progress_start=start, progress_end=end)
            if self._build_voice_reference_index:
                start, end = ranges["voice_reference"]
                self._build_voice_reference_phase(progress_start=start, progress_end=end)
            self._set_state(
                progress=1.0,
                status="All databases built successfully.",
                phase="done",
                done=True,
            )
        except Exception as e:
            _log.error("Database build failed: %s", e, exc_info=True)
            self._set_state(
                status=f"Error: {e}", phase="error", done=True, error=str(e)
            )

    def _extract_plugins(
        self,
        plugin_paths: list[Path],
        game_data: Path,
        progress_start: float,
        progress_end: float,
    ) -> None:
        """Re-export the given master plugins into the game's YAML cache."""
        if not plugin_paths:
            return
        names = [p.name for p in plugin_paths]
        total = len(names)

        def _on_progress(msg: str) -> None:
            # Best-effort progress mapping: fraction of plugins serialized.
            done = sum(1 for n in names if f"- {n}" in msg)
            frac = (done / total) if total else 1.0
            self._set_state(
                progress=progress_start + frac * (progress_end - progress_start),
                status=msg,
            )

        regenerate_esm_yaml_cache(
            self._game,
            game_data_dir=game_data,
            project_root=get_app_root(),
            db_dir=self._db_dir,
            plugins=names,
            on_progress=_on_progress,
        )

    def _detect_smart_changes(
        self, plugin_files: list, esm_yaml_dir: Path
    ) -> tuple[list, list]:
        """Compare plugin mtimes against YAML dirs to find what needs (re)extraction.

        The YAML cache uses ``<plugin_stem>/`` (e.g. ``Fallout4/`` for
        ``Fallout4.esm``), matching ``regenerate_esm_yaml_cache``.

        Returns:
            to_extract: plugins with no YAML dir yet (new)
            to_reextract: plugins whose ESM mtime is newer than its YAML dir (modified)
        """
        to_extract = []
        to_reextract = []
        for plugin in plugin_files:
            out_dir = esm_yaml_dir / plugin.stem
            if not out_dir.exists():
                to_extract.append(plugin)
            elif plugin.stat().st_mtime > out_dir.stat().st_mtime:
                to_reextract.append(plugin)
        return to_extract, to_reextract

    def _build_records(self, progress_start: float = 0.0, progress_end: float = 0.25):
        """Extract ESM/ESL YAML then build ``{game}_records.db``."""
        if self._smart:
            self._build_records_smart(progress_start, progress_end)
            return

        game = self._game
        self._set_state(
            progress=progress_start,
            status="Checking records database...",
            phase="records",
        )

        db_path = self._db_dir / f"{game}_records.db"
        esm_yaml_dir = self._db_dir / _GAME_ESM_YAML_DIR.get(game, "fo4_esm_yaml")

        if self._force_rebuild and db_path.is_file():
            db_path.unlink()
            _log.info("Deleted old %s for rebuild", db_path.name)

        if db_path.is_file() and not self._force_rebuild:
            self._set_state(
                progress=progress_end,
                status="Records database already exists — skipping.",
            )
            _log.info("%s already exists, skipping records build", db_path.name)
            return

        if not self._game_root:
            self._set_state(
                progress=progress_end,
                status="No game path set — skipping records index.",
            )
            _log.warning("game_root not set for %s, skipping records build", game)
            return

        game_data = Path(self._game_root) / "Data"
        if not game_data.is_dir():
            self._set_state(
                progress=progress_end,
                status=f"Data folder not found: {game_data} — skipping records.",
            )
            _log.warning("Game Data not found at %s", game_data)
            return

        # Plugin list: FO4 uses curated whitelist; other games scan Data/*.esm.
        if game == "fo4":
            plugin_files = [
                game_data / name
                for name in _FO4_PLUGIN_WHITELIST
                if (game_data / name).is_file()
            ]
        else:
            try:
                plugin_files = sorted(
                    p for p in game_data.iterdir() if p.suffix.lower() == ".esm"
                )
            except OSError:
                plugin_files = []

        if not plugin_files:
            self._set_state(
                progress=progress_end, status="No ESM files found — skipping records."
            )
            return

        esm_yaml_dir.mkdir(parents=True, exist_ok=True)
        needs_extract = (
            plugin_files
            if self._force_rebuild
            else [p for p in plugin_files if not (esm_yaml_dir / p.stem).is_dir()]
        )
        extract_end = (
            progress_start + (progress_end - progress_start) * 0.8
            if needs_extract else progress_start
        )

        if needs_extract:
            self._extract_plugins(needs_extract, game_data, progress_start, extract_end)

        self._set_state(
            progress=extract_end,
            status="Building records search index...",
            phase="records",
        )
        self._run_preprocess(
            script="preprocess_records.py",
            phase="records",
            progress_start=extract_end,
            progress_end=progress_end,
            extra_args=["--game", game, "--db-path", str(db_path)],
        )

    def _build_scripts_phase(
        self, progress_start: float = 0.0, progress_end: float = 1.0
    ):
        """Build ``{game}_scripts.db`` from Papyrus source files."""
        game = self._game
        profile = get_profile(game)
        self._set_state(
            progress=progress_start,
            status="Building Papyrus scripts index...",
            phase="scripts",
        )

        if not profile.papyrus_script_db and not profile.papyrus_source_subpath:
            self._set_state(
                progress=progress_end,
                status=f"No Papyrus scripts configured for {profile.display_name} — skipping.",
            )
            return

        db_path = self._db_dir / f"{game}_scripts.db"
        if self._force_rebuild and db_path.is_file():
            db_path.unlink()
            _log.info("Deleted old %s for rebuild", db_path.name)
        if db_path.is_file() and not self._force_rebuild:
            self._set_state(
                progress=progress_end,
                status="Papyrus scripts database already exists — skipping.",
            )
            return
        if not self._game_root:
            self._set_state(
                progress=progress_end,
                status="No game path set — skipping Papyrus scripts index.",
            )
            return

        extra_args = ["--game", game, "--db-path", str(db_path)]
        extra_args += ["--game-dir", self._game_root]

        self._run_preprocess(
            script="preprocess_scripts.py",
            phase="scripts",
            progress_start=progress_start,
            progress_end=progress_end,
            extra_args=extra_args,
        )

    def _build_wiki_phase(
        self, progress_start: float = 0.0, progress_end: float = 1.0
    ):
        """Build ``{game}_wiki.db`` from the configured local wiki mirror."""
        game = self._game
        profile = get_profile(game)
        self._set_state(
            progress=progress_start,
            status="Building wiki index...",
            phase="wiki",
        )

        if not profile.wiki_dir:
            self._set_state(
                progress=progress_end,
                status=f"No wiki configured for {profile.display_name} — skipping.",
            )
            return

        wiki_dir = get_app_root() / "Wiki" / profile.wiki_dir
        if not wiki_dir.is_dir():
            self._set_state(
                progress=progress_end,
                status=f"Wiki source not found: {wiki_dir} — skipping.",
            )
            return

        db_game = _SHARED_WIKI_DB_GAME.get(game, game)
        db_path = self._db_dir / f"{db_game}_wiki.db"
        if self._force_rebuild and db_path.is_file():
            db_path.unlink()
            _log.info("Deleted old %s for rebuild", db_path.name)
        if db_path.is_file() and not self._force_rebuild:
            self._set_state(
                progress=progress_end,
                status="Wiki database already exists — skipping.",
            )
            return

        self._run_preprocess(
            script="preprocess_wiki.py",
            phase="wiki",
            progress_start=progress_start,
            progress_end=progress_end,
            extra_args=[
                "--game",
                game,
                "--wiki-dir",
                str(wiki_dir),
                "--db-path",
                str(db_path),
            ],
        )

    def _build_records_smart(
        self, progress_start: float = 0.0, progress_end: float = 0.25
    ):
        """Incremental records update: extract only new/modified ESMs and upsert into DB."""
        game = self._game
        self._set_state(
            progress=progress_start,
            status="Checking for new ESM plugins...",
            phase="records",
        )

        db_path = self._db_dir / f"{game}_records.db"
        esm_yaml_dir = self._db_dir / _GAME_ESM_YAML_DIR.get(game, "fo4_esm_yaml")

        if not self._game_root:
            self._set_state(
                progress=progress_end,
                status="No game path set — skipping records index.",
            )
            return

        game_data = Path(self._game_root) / "Data"
        if not game_data.is_dir():
            self._set_state(
                progress=progress_end,
                status=f"Data folder not found: {game_data} — skipping.",
            )
            return

        if game == "fo4":
            plugin_files = [
                game_data / name
                for name in _FO4_PLUGIN_WHITELIST
                if (game_data / name).is_file()
            ]
        else:
            try:
                plugin_files = sorted(
                    p for p in game_data.iterdir() if p.suffix.lower() == ".esm"
                )
            except OSError:
                plugin_files = []

        if not plugin_files:
            self._set_state(
                progress=progress_end, status="No ESM files found — skipping records."
            )
            return

        esm_yaml_dir.mkdir(parents=True, exist_ok=True)
        to_extract, to_reextract = self._detect_smart_changes(
            plugin_files, esm_yaml_dir
        )

        if not to_extract and not to_reextract:
            self._set_state(
                progress=progress_end,
                status="Smart YAML: all plugins up to date — nothing to do.",
            )
            _log.info("Smart YAML: no changes detected for %s", game)
            return

        _log.info(
            "Smart YAML [%s]: %d new, %d modified plugins",
            game,
            len(to_extract),
            len(to_reextract),
        )

        for plugin in to_reextract:
            stale = esm_yaml_dir / plugin.stem
            shutil.rmtree(stale, ignore_errors=True)
            _log.info("Smart YAML: removed stale YAML dir %s", stale.name)

        plugins_to_run = to_extract + to_reextract
        extract_end = progress_start + (progress_end - progress_start) * 0.8

        self._extract_plugins(plugins_to_run, game_data, progress_start, extract_end)

        self._set_state(
            progress=extract_end, status="Updating records index...", phase="records"
        )
        all_sources = ",".join(p.name for p in plugins_to_run)
        extra_args = [
            "--game",
            game,
            "--db-path",
            str(db_path),
            "--incremental",
            "--sources",
            all_sources,
        ]
        delete_names = ",".join(p.name for p in to_reextract)
        if delete_names:
            extra_args += ["--delete-sources", delete_names]

        self._run_preprocess(
            script="preprocess_records.py",
            phase="records",
            progress_start=extract_end,
            progress_end=progress_end,
            extra_args=extra_args,
        )

    def _build_nifs_phase(
        self, progress_start: float = 0.25, progress_end: float = 0.85
    ):
        """Build {game}_nifs.db using preprocess_nifs.py."""
        game = self._game
        self._set_state(
            progress=progress_start, status="Building NIF index...", phase="nifs"
        )

        db_path = self._db_dir / f"{game}_nifs.db"

        if self._force_rebuild and db_path.is_file():
            db_path.unlink()
            _log.info("Deleted old %s for rebuild", db_path.name)

        extra_args = ["--game", game, "--db-path", str(db_path)]
        if self._extracted_dir:
            extra_args += ["--extracted-dir", self._extracted_dir]
        elif self._game_root:
            extra_args += ["--extracted-dir", os.path.join(self._game_root, "Data")]

        self._run_preprocess(
            script="preprocess_nifs.py",
            phase="nifs",
            progress_start=progress_start,
            progress_end=progress_end,
            extra_args=extra_args,
        )

    def _build_behaviors_phase(
        self, progress_start: float = 0.85, progress_end: float = 1.0
    ):
        """Build havok.db (behaviors, skeletons, animations, manifests)."""
        self._set_state(
            progress=progress_start, status="Building Havok index...", phase="behaviors"
        )

        if self._force_rebuild:
            havok_db = self._db_dir / f"{self._game}_havok.db"
            if havok_db.is_file():
                havok_db.unlink()
                _log.info("Deleted old %s for rebuild", havok_db.name)

        extra_args = ["--game", self._game, "--db-path", str(self._db_dir / f"{self._game}_havok.db")]
        if self._extracted_dir:
            extra_args += ["--extracted-dir", self._extracted_dir]
        elif self._game_root:
            extra_args += ["--extracted-dir", os.path.join(self._game_root, "Data")]

        self._run_preprocess(
            script="preprocess_havok.py",
            phase="behaviors",
            progress_start=progress_start,
            progress_end=progress_end,
            extra_args=extra_args,
        )

    def _build_swf_phase(self, progress_start: float = 0.0, progress_end: float = 1.0):
        """Build {game}_swf_shapes.db using preprocess_swf.py."""
        game = self._game
        self._set_state(
            progress=progress_start, status="Building SWF shape library...", phase="swf"
        )

        db_path = self._db_dir / f"{game}_swf_shapes.db"

        if self._force_rebuild and db_path.is_file():
            db_path.unlink()
            _log.info("Deleted old %s for rebuild", db_path.name)

        extra_args = ["--game", game, "--db-path", str(db_path)]
        extracted = self._extracted_dir or (
            os.path.join(self._game_root, "Data") if self._game_root else ""
        )
        if extracted:
            extra_args += ["--extracted-dir", extracted]

        self._run_preprocess(
            script="preprocess_swf.py",
            phase="swf",
            progress_start=progress_start,
            progress_end=progress_end,
            extra_args=extra_args,
        )

    def _build_voice_reference_phase(self, progress_start: float = 0.0, progress_end: float = 1.0):
        """Build the native SQLite voice-reference cache for the selected game."""
        game = self._game
        self._set_state(
            progress=progress_start,
            status="Building voice reference index...",
            phase="voice_reference",
        )
        data_dir, strings_dir = self._resolve_voice_reference_paths()
        if data_dir is None:
            self._set_state(
                progress=progress_end,
                status="No game Data folder found - skipping voice reference index.",
            )
            return

        cache_path = voice_reference_sqlite_cache_path(
            game=game,
            data_dir=data_dir,
            strings_dir=strings_dir,
            db_dir=self._db_dir,
        )
        if cache_path is not None and cache_path.is_file() and not self._force_rebuild:
            self._set_state(
                progress=progress_end,
                status="Voice reference index already exists - skipping.",
            )
            return

        progress_range = progress_end - progress_start

        def _on_progress(current: int, total: int, message: str) -> None:
            ratio = float(current) / max(1.0, float(total))
            self._set_state(
                progress=min(progress_end, progress_start + ratio * progress_range),
                status=message,
                phase="voice_reference",
            )

        index = build_voice_reference(
            game=game,
            data_dir=data_dir,
            strings_dir=strings_dir,
            db_dir=self._db_dir,
            force=self._force_rebuild,
            progress=_on_progress,
        )
        self._set_state(
            progress=progress_end,
            status=f"Voice reference index complete: {len(index.lines):,} voice line(s).",
        )

    def _resolve_voice_reference_paths(self) -> tuple[Path | None, Path | None]:
        if not self._game_root:
            return None, None
        root = Path(self._game_root)
        data_dir = root / "Data"
        if not data_dir.is_dir() and root.name.lower() == "data":
            data_dir = root
        if not data_dir.is_dir():
            return None, None
        strings_dir = data_dir / "Strings"
        if not strings_dir.is_dir() and self._extracted_dir:
            extracted = Path(self._extracted_dir)
            for candidate in (extracted / "Strings", extracted / "Data" / "Strings"):
                if candidate.is_dir():
                    strings_dir = candidate
                    break
        return data_dir, strings_dir

    def _run_preprocess(
        self,
        script: str,
        phase: str,
        progress_start: float,
        progress_end: float,
        extra_args: list | None = None,
    ):
        """Run a preprocess entrypoint in-process, parsing output for progress."""

        try:
            progress_range = progress_end - progress_start
            line_count = 0

            def _handle_line(line: str) -> None:
                nonlocal line_count
                line = line.strip()
                if not line:
                    return
                line_count += 1

                # Parse progress from common patterns in preprocess scripts
                self._set_state(status=line)

                # Try to extract numeric progress from lines like "1,234/5,678 files"
                parsed = False
                if "/" in line:
                    parts = line.split()
                    for part in parts:
                        if "/" in part:
                            try:
                                num, den = part.split("/")
                                ratio = int(num.replace(",", "")) / int(
                                    den.replace(",", "")
                                )
                                p = progress_start + ratio * progress_range
                                self._set_state(progress=min(p, progress_end))
                                parsed = True
                            except (ValueError, ZeroDivisionError):
                                pass
                            break

                # Fallback: parse percentage like "(42%)" or "42%"
                if not parsed:
                    m = re.search(r"\((\d+)%\)", line) or re.search(r"\b(\d+)%", line)
                    if m:
                        pct = int(m.group(1)) / 100.0
                        p = progress_start + pct * progress_range
                        self._set_state(progress=min(p, progress_end))

            result = run_preprocess(
                script,
                *(extra_args or []),
                cwd=get_code_root(),
                on_line=_handle_line,
            )
            if result != 0:
                _log.warning("Preprocess script %s exited with code %d", script, result)
                self._set_state(status=f"{phase} build failed", progress=progress_end)
                return

            self._set_state(status=f"{phase} build complete", progress=progress_end)
        except Exception as e:
            self._set_state(
                status=f"Error running {script}: {e}", progress=progress_end
            )
            _log.error("Preprocess failed: %s", e, exc_info=True)
