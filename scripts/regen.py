"""Unified FO76 -> FO4 regen driver.

Record conversion and asset waves run through
bacup_lib.workflows.unified. BA2s come from the sink join; deploy,
INI, and sanitizer post-steps live here.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime
import logging
import os
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))
sys.path.insert(0, str(REPO_ROOT / "py_creation_lib" / "python"))

LOG = logging.getLogger("regen")

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import (
    RegenOptions,
    RegenPaths,
)
from bacup_lib.source_pairs import (
    DEFAULT_PAIR_ID,
    SOURCE_PAIRS,
    SourcePair,
    get_pair,
)
from bacup_lib.target_assets import default_target_asset_catalog
from bacup_lib.lod_settings import load_profile_settings
from bacup_lib.upgrade_manifest import bundled_upgrade_manifest_path

# --- post-step machinery ----------------------------------------------------
_FO4_CK_CUSTOM_INI_NAME = "CreationKitCustom.ini"
# --re-use-land cache: a snapshot of the prior good output ESM (terrain + navmesh
# source) plus a small staleness marker, written next to the output plugin.
# Backward-compatible high-quality settings alias. New profiles live in
# bacup/scripts/lod_settings/.
_LOD_SETTINGS_OVERRIDE = REPO_ROOT / "bacup" / "scripts" / "lod_settings.fo76fo4.json"


class _TeeStream:
    def __init__(self, *streams):
        self._streams = streams

    def write(self, text: str) -> int:
        for stream in self._streams:
            try:
                stream.write(text)
            except UnicodeEncodeError:
                encoding = getattr(stream, "encoding", None) or "utf-8"
                safe_text = text.encode(encoding, errors="replace").decode(
                    encoding,
                    errors="replace",
                )
                stream.write(safe_text)
        return len(text)

    def flush(self) -> None:
        for stream in self._streams:
            stream.flush()


def _configure_logging(log_path: Path) -> logging.FileHandler:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    file_handler = logging.FileHandler(log_path, mode="w", encoding="utf-8")
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        handlers=[
            logging.StreamHandler(),
            file_handler,
        ],
        force=True,
    )
    return file_handler


def _install_native_stderr_tee(log_path: Path) -> None:
    """Mirror OS-level stderr into `log_path` for the life of the process.

    The Rust layer writes its step timings ([repair_timing], [eczn_timing],
    [workshop_timing], [fixups_v2], ...) via eprintln! straight to fd 2,
    bypassing Python logging — without this they exist only in console
    scrollback. Python log lines also pass through fd 2 (StreamHandler), so
    this goes to a separate file rather than regen.log to avoid duplicates.
    """
    import threading

    log_path.parent.mkdir(parents=True, exist_ok=True)
    sink = open(log_path, "ab", buffering=0)
    original_fd = os.dup(2)
    read_fd, write_fd = os.pipe()
    os.dup2(write_fd, 2)
    os.close(write_fd)

    def _pump() -> None:
        while True:
            try:
                chunk = os.read(read_fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            try:
                os.write(original_fd, chunk)
            except OSError:
                pass
            sink.write(chunk)

    threading.Thread(target=_pump, name="native-stderr-tee", daemon=True).start()



@contextlib.contextmanager
def _progress_runner(log_stream):
    from bacup_lib.runner import StreamingConversionRunner

    yield StreamingConversionRunner(stream=_TeeStream(sys.stdout, log_stream))

def _read_env_path(name: str) -> Path | None:
    value = os.environ.get(name, "").strip()
    if value:
        return Path(value)

    env_file = REPO_ROOT / ".env"
    if not env_file.is_file():
        return None

    prefix = f"{name}="
    for raw_line in env_file.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or not line.startswith(prefix):
            continue
        value = line.split("=", 1)[1].strip().strip('"').strip("'")
        if value:
            return Path(value)
    return None


def _resolve_source_extracted_dir(pair: SourcePair) -> Path:
    path = _read_env_path(pair.source_extracted_env)
    if path is not None:
        return path
    return REPO_ROOT / "extracted" / pair.source_game


def _resolve_fo4_extracted_dir() -> Path | None:
    return _read_env_path("FO4_EXTRACTED_DIR")


def _resolve_source_data_dir(pair: SourcePair) -> Path | None:
    data_path = _read_env_path(pair.source_data_env)
    if data_path is not None:
        return data_path
    path = _read_env_path(pair.source_dir_env)
    if path is None:
        return None
    return path / "Data"


def _resolve_fo76_extracted_dir() -> Path:
    return _resolve_source_extracted_dir(get_pair(DEFAULT_PAIR_ID))


def _resolve_fo76_data_dir() -> Path | None:
    return _resolve_source_data_dir(get_pair(DEFAULT_PAIR_ID))


def _resolve_grafted_data_dir(pair: SourcePair) -> Path | None:
    if pair.merge is None:
        return None
    data_path = _read_env_path(pair.merge.grafted_data_env)
    if data_path is not None:
        return data_path
    root = _read_env_path(pair.merge.grafted_dir_env)
    if root is None:
        return None
    return root / "Data"


def _resolve_grafted_extracted_dir(pair: SourcePair) -> Path | None:
    if pair.merge is None or pair.merge.grafted_game == pair.source_game:
        return None
    path = _read_env_path(pair.merge.grafted_extracted_env)
    if path is not None:
        return path
    return REPO_ROOT / "extracted" / pair.merge.grafted_game


def _resolve_fo4_data_dir() -> Path:
    path = _read_env_path("FO4_DIR")
    if path is None:
        raise RuntimeError("FO4_DIR is not set in the environment or .env")
    return path / "Data"


def _resolve_fo4_ck_ini_path() -> Path:
    path = _read_env_path("FO4_DIR")
    if path is None:
        raise RuntimeError("FO4_DIR is not set in the environment or .env")
    return path / _FO4_CK_CUSTOM_INI_NAME


def _resolve_fo4_game_ini_path() -> Path:
    return Path.home() / "Documents" / "My Games" / "Fallout4" / "Fallout4.ini"


def _resolve_fo4_custom_ini_path() -> Path:
    return Path.home() / "Documents" / "My Games" / "Fallout4" / "Fallout4Custom.ini"














































def _positive_worker_count(value: str) -> int:
    workers = int(value)
    if workers <= 0:
        raise argparse.ArgumentTypeError("--workers must be greater than 0")
    return workers


def _ensure_native_current(args) -> int:
    if getattr(args, "deploy_only", False) or getattr(args, "undeploy", False):
        return 0

    cmd = [sys.executable, str(REPO_ROOT / "scripts" / "ensure_native.py")]
    if getattr(args, "dhat_heap", False):
        cmd.append("--dhat-heap")
    result = subprocess.run(cmd, cwd=REPO_ROOT)
    return result.returncode






def _load_lod_settings(
    profile: str | None = "auto",
    lod_mode: str = "hybrid-atlas",
    atlas_mip_flooding: bool | None = None,
    pair_id: str = DEFAULT_PAIR_ID,
) -> dict:
    """Return LOD generation settings for the selected profile."""
    if pair_id != DEFAULT_PAIR_ID:
        from creation_lib.lod.default_settings import fo4_default_settings

        defaults = fo4_default_settings()
        normalized_profile = (profile or "auto").strip().lower().replace("_", "-")
        if normalized_profile == "performance":
            raise ValueError(
                "The performance LOD profile is FO76-specific; use auto, native, "
                "or high-quality for cross-game record LOD"
            )
        settings = (
            load_profile_settings(
                [REPO_ROOT], profile="high-quality", lod_mode=lod_mode
            )
            if normalized_profile == "high-quality"
            else defaults
        )
        settings["global"].update(
            {
                "worldspaces": [],
                "stride": None,
                "southwest_cell": None,
                "bounds": None,
                "generate_terrain": True,
                "generate_objects": True,
                "generate_trees": True,
            }
        )
        settings["objects"]["source"] = "records"
        for key, value in defaults["objects"].items():
            if key.startswith("fo76_bto_"):
                settings["objects"][key] = value
        settings.setdefault("trees", {})["trees_3d"] = True
    else:
        settings = load_profile_settings(
            [REPO_ROOT], profile=profile, lod_mode=lod_mode
        )
    if atlas_mip_flooding is not None:
        settings.setdefault("objects", {})["atlas_mip_flooding"] = atlas_mip_flooding
    return settings












# --- unified driver entry points --------------------------------------------


def _deploy_data_dir_from_args(args) -> Path | None:
    value = str(getattr(args, "deploy_to_mo2", "") or "").strip()
    return Path(value).expanduser() if value else None


def _archive_max_bytes_from_args(args) -> int:
    from creation_lib.build.archive_plan import gib_to_bytes

    return gib_to_bytes(float(getattr(args, "archive_max_size_gb", 16.0)))


def _create_run_logs_dir(mod_name: str) -> Path:
    base = REPO_ROOT / "mods" / mod_name / "logs"
    timestamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    stem = f"{timestamp}-pid{os.getpid()}"
    for suffix in ["", *[f"-{index}" for index in range(1, 1000)]]:
        logs_dir = base / f"{stem}{suffix}" / "logs"
        try:
            logs_dir.mkdir(parents=True, exist_ok=False)
        except FileExistsError:
            continue
        return logs_dir
    raise RuntimeError(f"could not allocate a unique run log directory under {base}")


def _resolve_regen_paths(
    mod_name: str,
    *,
    pair: SourcePair | None = None,
    parser: argparse.ArgumentParser | None = None,
    base_game_only: bool = False,
    deploy_data_dir: Path | None = None,
    diagnostics_root: Path | None = None,
) -> RegenPaths:
    pair = pair or get_pair(DEFAULT_PAIR_ID)
    if pair.pair_id == DEFAULT_PAIR_ID:
        source_extracted_dir = _resolve_fo76_extracted_dir()
        source_data_dir = _resolve_fo76_data_dir()
    else:
        source_extracted_dir = _resolve_source_extracted_dir(pair)
        source_data_dir = _resolve_source_data_dir(pair)
    merge_primary_plugin_paths: tuple[Path, ...] = ()
    merge_grafted_plugin_paths: tuple[Path, ...] = ()
    additional_source_asset_roots: tuple[Path, ...] = ()

    def fail(message: str) -> None:
        if parser is not None:
            parser.error(message)
        raise ValueError(message)

    def required_paths(
        data_dir: Path | None,
        plugin_names: tuple[str, ...],
        *,
        data_env: str,
        dir_env: str,
    ) -> tuple[Path, ...]:
        if data_dir is None:
            fail(f"Set {data_env} (or {dir_env}) to the game's Data directory")
        assert data_dir is not None
        paths = tuple(data_dir / name for name in plugin_names)
        for path in paths:
            if not path.is_file():
                fail(
                    f"required plugin not found: {path}. "
                    f"Set {data_env} (or {dir_env}) to the correct install"
                )
        return paths

    if pair.merge is not None:
        primary_names = (
            pair.source_plugins[:1] if base_game_only else pair.source_plugins
        )
        primary_paths = required_paths(
            source_data_dir,
            primary_names,
            data_env=pair.source_data_env,
            dir_env=pair.source_dir_env,
        )
        optional_paths = ()
        if not base_game_only:
            assert source_data_dir is not None
            optional_paths = tuple(
                path
                for name in pair.optional_source_plugins
                if (path := source_data_dir / name).is_file()
            )
        merge_primary_plugin_paths = (*primary_paths, *optional_paths)

        grafted_names = (
            pair.merge.grafted_plugins[:1]
            if base_game_only
            else pair.merge.grafted_plugins
        )
        if grafted_names:
            merge_grafted_plugin_paths = required_paths(
                _resolve_grafted_data_dir(pair),
                grafted_names,
                data_env=pair.merge.grafted_data_env,
                dir_env=pair.merge.grafted_dir_env,
            )
            grafted_extracted_dir = _resolve_grafted_extracted_dir(pair)
            if grafted_extracted_dir is None or not grafted_extracted_dir.is_dir():
                fail(
                    f"grafted extracted asset directory not found: "
                    f"{grafted_extracted_dir}. Set "
                    f"{pair.merge.grafted_extracted_env} to the extracted "
                    f"{pair.merge.grafted_game.upper()} asset directory"
                )
            additional_source_asset_roots = (grafted_extracted_dir,)

    return RegenPaths(
        source_extracted_dir=source_extracted_dir,
        source_data_dir=source_data_dir or source_extracted_dir,
        target_extracted_dir=_resolve_fo4_extracted_dir(),
        target_data_dir=_resolve_fo4_data_dir(),
        target_ck_ini_path=_resolve_fo4_ck_ini_path(),
        target_custom_ini_path=_resolve_fo4_custom_ini_path(),
        target_game_ini_path=_resolve_fo4_game_ini_path(),
        output_root=REPO_ROOT / "mods" / mod_name,
        mod_name=mod_name,
        resource_dir=REPO_ROOT / "resource",
        deploy_data_dir=deploy_data_dir,
        diagnostics_root=diagnostics_root,
        target_asset_catalog_path=default_target_asset_catalog(),
        merge_primary_plugin_paths=merge_primary_plugin_paths,
        merge_grafted_plugin_paths=merge_grafted_plugin_paths,
        additional_source_asset_roots=additional_source_asset_roots,
    )


def _args_to_regen_options(args) -> RegenOptions:
    return RegenOptions(
        deploy=bool(args.deploy or getattr(args, "deploy_to_mo2", None)),
        ba2_mode="expanded" if bool(getattr(args, "expanded_archives", False)) else "packed",
        archive_max_bytes=_archive_max_bytes_from_args(args),
        workers=None,
        asset_workers=args.asset_workers,
        lod_mode="convert",
        pbr_carry=bool(getattr(args, "pbr_carry", False)),
        texture_landscape_mip_flooding=bool(
            getattr(args, "texture_landscape_mip_flooding", False)
        ),
        re_use_land=bool(args.re_use_land),
        include_interior=bool(getattr(args, "include_interior", True)),
        carry_interior_previs=bool(getattr(args, "carry_interior_previs", False)),
        records_limit=args.records_limit,
        emit_btd4=bool(args.emit_btd4),
        generate_anim_text_data=bool(args.generate_anim_text_data),
        anim_text_data_native=bool(args.anim_text_data_native),
        validate_collision=bool(args.validate_collision),
        validate_output=bool(args.validate_output),
        validation_warn_only=bool(args.validation_warn_only),
        max_asset_failures=args.max_asset_failures,
        max_seconds=args.max_seconds,
        deep_invariants=bool(args.deep_invariants),
        export_yaml=bool(args.export_yaml),
        cpu_textures=bool(args.cpu_textures),
        memory_report=bool(getattr(args, "memory_report", False)),
        upgrade=bool(args.upgrade),
        mod_version=args.mod_version,
        upgrade_from=args.upgrade_from,
        upgrade_manifest_path=(
            args.upgrade_manifest_path
            or bundled_upgrade_manifest_path()
        ),
    )


def run_full_mode(
    args,
    conv_cli,
    *,
    parser: argparse.ArgumentParser | None = None,
) -> int:
    pair = get_pair(getattr(args, "pair", DEFAULT_PAIR_ID))
    mod_name = args.mod_name or pair.output_mod_name
    args.mod_name = mod_name
    diagnostics_root = getattr(args, "run_logs_dir", None) or _create_run_logs_dir(
        mod_name
    )
    paths = _resolve_regen_paths(
        mod_name,
        pair=pair,
        parser=parser,
        base_game_only=bool(args.base_game_only),
        deploy_data_dir=_deploy_data_dir_from_args(args),
        diagnostics_root=diagnostics_root,
    )
    paths.output_root.mkdir(parents=True, exist_ok=True)
    log_handler = _configure_logging(diagnostics_root / "regen.log")
    _install_native_stderr_tee(diagnostics_root / "native_stderr.log")
    LOG.info("run diagnostics: %s", diagnostics_root)
    if args.drop_trace is not None:
        # The native conversion reads these once (cached) at the first drop site.
        trace_path = diagnostics_root / "drop_trace.log"
        os.environ["MODBOX_TRACE_DROPS"] = args.drop_trace
        os.environ["MODBOX_TRACE_DROPS_FILE"] = str(trace_path)
        LOG.info("--drop-trace active (filter=%s) -> %s", args.drop_trace, trace_path)
    if args.cache:
        # The lever is plumbed (manifest written every run; consult helpers
        # tested) but v1 skip-sets are conservative — declared limitation.
        LOG.info("--cache: consult is v1-conservative (see workflows/unified.py)")
    output_plugin_names = [pair.output_plugin_name]

    if getattr(args, "scripts_only", False):
        options = _args_to_regen_options(args)
        options.deploy = False
        options.workers = conv_cli.resolve_workers(args.workers)
        with _progress_runner(log_handler.stream) as runner:
            try:
                result = regen_pipeline.run_scripts_only(
                    paths,
                    options,
                    pair=pair,
                    runner=runner,
                    papyrus_compiler=conv_cli.papyrus_compiler_from_args(args),
                )
            except (FileNotFoundError, RuntimeError, ValueError) as exc:
                LOG.error("%s", exc)
                return 2
        for failure in result.failures:
            LOG.error("%s", failure)
        LOG.info(
            "scripts-only elapsed_seconds=%.3f exit_code=%s",
            result.elapsed_seconds,
            result.exit_code,
        )
        if result.exit_code == 0:
            LOG.info("done")
        return result.exit_code

    if args.undeploy:
        return regen_pipeline.undeploy(
            paths, plugin_names=output_plugin_names
        ).exit_code

    if args.deploy_only:
        from bacup_lib.timing_report import TimingReport

        regen_pipeline._deploy_post_steps(paths, output_plugin_names, TimingReport())
        LOG.info("deploy-only done")
        return 0

    phases = conv_cli.phase_selection_from_args(args)
    options = _args_to_regen_options(args)
    options.lod_mode = phases.lod_mode
    options.workers = conv_cli.resolve_workers(args.workers)
    if args.memory_budget_probe:
        options.records_limit = 50_000
        LOG.info("memory-budget-probe mode: limiting to 50000 records for RSS gate")

    build_option_overrides = {
        "exclude_signatures": conv_cli.exclude_signatures_from_args(args),
        "papyrus_compiler": conv_cli.papyrus_compiler_from_args(args),
        "convert_precombined_nifs": args.convert_precombined_nifs,
        "disable_nif_collision_memo": args.disable_nif_collision_memo,
    }
    lod_settings = (
        _load_lod_settings(
            getattr(args, "lod_profile", "auto"),
            options.lod_mode,
            getattr(args, "lod_atlas_mip_flooding", None),
            pair.pair_id,
        )
        if options.lod_mode in {"generate", "hybrid", "hybrid-atlas"}
        else None
    )

    if args.re_use_land:
        LOG.info(
            "--re-use-land: reusing prior LAND/NAVM/NAVI cache; terrain & navmesh "
            "regen skipped. Staleness checks are warnings-only (v1)."
        )

    with _progress_runner(log_handler.stream) as runner:
        try:
            result = regen_pipeline.run_full_regen(
                paths,
                options,
                pair=pair,
                phases=phases,
                runner=runner,
                lod_settings=lod_settings,
                build_option_overrides=build_option_overrides,
            )
        except FileNotFoundError as exc:
            LOG.error("%s", exc)
            return 2

    for warning in result.warnings[:25]:
        LOG.warning("%s", warning)
    for failure in result.failures:
        LOG.error("%s", failure)
    LOG.info(
        "full conversion elapsed_seconds=%.3f exit_code=%s",
        result.elapsed_seconds,
        result.exit_code,
    )
    if result.exit_code == 0:
        LOG.info("done")
    return result.exit_code


class _PairArgumentParser(argparse.ArgumentParser):
    def parse_args(self, args=None, namespace=None):
        raw_args = list(args) if args is not None else sys.argv[1:]
        parsed = super().parse_args(args, namespace)
        parsed._lod_mode_explicit = any(
            value == "--lod-mode" or value.startswith("--lod-mode=")
            for value in raw_args
        )
        _resolve_pair_args(self, parsed)
        return parsed


def build_parser(conv_cli) -> argparse.ArgumentParser:
    parser = _PairArgumentParser(
        prog="regen.py", description="Unified source-to-FO4 regen driver"
    )
    # Full-run levers (spec §5a)
    parser.add_argument(
        "--pair",
        choices=sorted(SOURCE_PAIRS),
        default=DEFAULT_PAIR_ID,
    )
    parser.add_argument(
        "--mvp",
        action="store_true",
        help=(
            "Build the Skyrim world-only MVP: no actors, weapons, quests, "
            "scripts, voice, skeletons, animations, or behaviors."
        ),
    )
    parser.add_argument(
        "--scripts-only",
        action="store_true",
        help=(
            "Decompile FO76 client PEX, apply script_patches, and compile FO4 "
            "PSC/PEX outputs without loading or writing an ESM or processing assets."
        ),
    )
    parser.add_argument("--mod-name", default=None)
    deploy = parser.add_mutually_exclusive_group()
    deploy.add_argument("--deploy", action="store_true")
    deploy.add_argument("--undeploy", action="store_true")
    deploy.add_argument(
        "--deploy-only",
        action="store_true",
        help="Deploy the existing mods/<mod> outputs without converting.",
    )
    parser.add_argument(
        "--deploy-to-mo2",
        metavar="DIR",
        help=(
            "Deploy/undeploy to an MO2 mod folder treated as a virtual Data root. "
            "Implies --deploy for conversion runs and skips Fallout INI edits."
        ),
    )
    parser.add_argument("--base-game-only", action="store_true")
    parser.add_argument(
        "--archive-max-size-gb",
        type=float,
        default=16.0,
        help="Maximum BA2 archive size in GiB before splitting (default 16.0).",
    )
    parser.add_argument(
        "--expanded-archives",
        action="store_true",
        default=False,
        help="Use family archive labels such as Meshes and Sounds instead of Main + Textures when possible.",
    )
    parser.add_argument("--records-limit", type=_positive_worker_count)
    parser.add_argument("--max-seconds", type=_positive_worker_count)
    parser.add_argument("--max-asset-failures", type=int, default=None)
    parser.add_argument("--cpu-textures", action="store_true")
    parser.add_argument(
        "--pbr-carry",
        action="store_true",
        default=False,
        help="Carry clean FO76 PBR texture sidecars alongside the FO4 fallback bake.",
    )
    parser.add_argument(
        "--emit-btd4",
        action="store_true",
        default=False,
        help="Emit loose Terrain/<worldspace>.btd4 dense LAND sidecar files.",
    )
    parser.add_argument(
        "--generate-anim-text-data",
        "--generate-anim-data",
        dest="generate_anim_text_data",
        action="store_true",
        default=False,
        help=(
            "Generate data/Meshes/AnimTextData after Havok conversion. Uses "
            "Creation Kit -GenerateAnimInfo when CreationKit.exe is present, else "
            "the CK-free native generator."
        ),
    )
    parser.add_argument(
        "--anim-text-data-native",
        dest="anim_text_data_native",
        action="store_true",
        default=False,
        help=(
            "Force the CK-free native AnimTextData generator even when "
            "CreationKit.exe is present. Implies --generate-anim-text-data."
        ),
    )
    parser.add_argument("--validate-output", action="store_true")
    parser.add_argument(
        "--validate-collision",
        action="store_true",
        default=False,
        help="Run report-only collision validation before BA2 packing.",
    )
    parser.add_argument("--validation-warn-only", action="store_true")
    parser.add_argument("--deep-invariants", action="store_true")
    parser.add_argument(
        "--export-yaml", dest="export_yaml", action="store_true", default=True
    )
    parser.add_argument("--no-export-yaml", dest="export_yaml", action="store_false")
    parser.add_argument("--cache", action="store_true")
    parser.add_argument(
        "--re-use-land",
        dest="re_use_land",
        action="store_true",
        default=False,
        help=(
            "Reuse LAND/NAVM/NAVI + terrain-texture records from the prior run's "
            "cache (mods/<mod>/.regen_land_cache.esm), skipping terrain & navmesh "
            "regen. Re-runs all other record work. Run a full regen first to "
            "populate the cache."
        ),
    )
    parser.add_argument(
        "--upgrade",
        action="store_true",
        default=False,
        help=(
            "Upgrade-generation mode: regenerate only the asset families changed "
            "since the deployed version per --upgrade-manifest, reuse the rest "
            "from the live deployment, and deploy a selective per-family BA2 swap."
        ),
    )
    parser.add_argument(
        "--mod-version",
        dest="mod_version",
        default=None,
        metavar="VERSION",
        help="Target version to stamp into TES4 SNAM. Defaults to the manifest's `current`.",
    )
    parser.add_argument(
        "--upgrade-from",
        dest="upgrade_from",
        default=None,
        metavar="VERSION",
        help=(
            "Override the detected installed version instead of reading the "
            "deployed SeventySix.esm TES4 SNAM."
        ),
    )
    parser.add_argument(
        "--upgrade-manifest",
        dest="upgrade_manifest_path",
        type=Path,
        default=None,
        metavar="PATH",
        help=(
            "Override the bundled upgrade manifest YAML (defaults to the "
            "manifest packaged with the converter)."
        ),
    )
    parser.add_argument("--dhat-heap", action="store_true")
    parser.add_argument("--dhat-output")
    parser.add_argument("--memory-budget-probe", action="store_true")
    parser.add_argument(
        "--memory-report",
        action="store_true",
        default=False,
        help=(
            "Enable psutil-backed conversion_memory.json/md sampling. Disabled "
            "by default to avoid extra memory pressure during full regen."
        ),
    )
    parser.add_argument("--convert-precombined-nifs", action="store_true")
    parser.add_argument(
        "--disable-nif-collision-memo",
        action="store_true",
        help="Diagnostic: bypass the shared NIF/BTO collision memo during asset conversion.",
    )
    parser.add_argument(
        "--drop-trace",
        nargs="?",
        const="1",
        default=None,
        metavar="FILTER",
        help=(
            "Diagnostic: log every subrecord/record the conversion drops to stderr. "
            "Bare flag traces all drops; pass a filter to narrow, e.g. "
            "--drop-trace FACT:VENC or --drop-trace FACT,QUST. Each line is "
            "'[drop_trace] stage=<...> rec=<SIG>:<id> sub=<SUB> reason=<...>'."
        ),
    )
    parser.add_argument(
        "--serialize-tracks",
        dest="serialize_tracks",
        action="store_true",
        default=True,
        help="Default behavior: run asset waves after the record track.",
    )
    parser.add_argument(
        "--concurrent-tracks",
        dest="serialize_tracks",
        action="store_false",
        help="Experimental: run the asset waves concurrently with the record track.",
    )
    parser.add_argument(
        "--asset-workers",
        type=_positive_worker_count,
        default=None,
        help=(
            "Worker count for unified asset waves. Defaults to --workers; use "
            "this to cap concurrent asset pressure."
        ),
    )
    parser.add_argument(
        "--carry-interior-previs",
        action="store_true",
        help="Carry FO76 previs hashes on interior cells (default: strip).",
    )
    parser.add_argument(
        "--include-interior",
        dest="include_interior",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Include interior cells in output (default on; use --no-include-interior to skip).",
    )
    conv_cli.add_phase_flags(parser)  # --no-terrain ... --no-build-esp
    conv_cli.add_lod_mode_flag(parser)  # --lod-mode {convert,generate,hybrid,hybrid-atlas,none}
    conv_cli.add_lod_profile_flag(parser)  # --lod-profile {auto,native,high-quality,performance}
    conv_cli.add_lod_atlas_mip_flooding_flag(parser)
    conv_cli.add_texture_landscape_mip_flooding_flag(parser)
    conv_cli.add_common_lever_flags(parser)  # --exclude-record/--workers/...
    return parser


def _resolve_pair_args(parser: argparse.ArgumentParser, args) -> SourcePair:
    pair = get_pair(args.pair)
    if args.mod_name is None:
        args.mod_name = pair.output_mod_name
    if pair.pair_id != DEFAULT_PAIR_ID:
        if getattr(args, "lod_profile", "auto") == "performance":
            parser.error(
                f"{pair.pair_id} does not support the FO76-only performance LOD profile"
            )
        if not getattr(args, "_lod_mode_explicit", False):
            args.lod_mode = "generate"
        elif args.lod_mode not in {"generate", "none"}:
            parser.error(
                f"{pair.pair_id} supports only --lod-mode generate or none"
            )
    if getattr(args, "mvp", False):
        if pair.pair_id != "skyrimse:fo4":
            parser.error("--mvp currently supports only skyrimse:fo4")
        if not getattr(args, "_lod_mode_explicit", False):
            args.lod_mode = "generate"
        if args.lod_mode not in {"generate", "none"}:
            parser.error("Skyrim MVP LOD supports only --lod-mode generate or none")
    return pair


def _validate_output_mode(parser: argparse.ArgumentParser, args) -> None:
    try:
        _archive_max_bytes_from_args(args)
    except ValueError as exc:
        parser.error(str(exc))
    if getattr(args, "scripts_only", False):
        if getattr(args, "pair", DEFAULT_PAIR_ID) != DEFAULT_PAIR_ID:
            parser.error("--scripts-only currently supports only fo76:fo4")
        if any(
            (
                args.deploy,
                args.undeploy,
                args.deploy_only,
                getattr(args, "deploy_to_mo2", None),
                args.upgrade,
                args.mvp,
            )
        ):
            parser.error(
                "--scripts-only cannot be combined with deployment, upgrade, or MVP modes"
            )


def main(argv: list[str] | None = None) -> int:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import _conversion_cli as conv_cli

    parser = build_parser(conv_cli)
    args = parser.parse_args(argv)
    _resolve_pair_args(parser, args)
    if args.anim_text_data_native:
        args.generate_anim_text_data = True
    _validate_output_mode(parser, args)
    native_status = _ensure_native_current(args)
    if native_status != 0:
        return native_status
    args.run_logs_dir = _create_run_logs_dir(args.mod_name)
    return run_full_mode(args, conv_cli, parser=parser)


if __name__ == "__main__":
    raise SystemExit(main())
