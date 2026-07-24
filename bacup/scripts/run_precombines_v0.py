"""v0 CK-free precombine generation runner (path-based, no CK dependency).

See docs/superpowers/plans/2026-07-12-precombine-generation-v0.md, Task 6.

Backs up the target ESM once, opens it through `bacup_lib.run.ConversionRun`,
dispatches the source-free `generate_precombines` phase for exactly one
interior CELL, and atomically replaces the ESM only when the phase actually
wrote new precombine assets.
"""
from __future__ import annotations

import argparse
import os
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable

# (source_game, target_game, target_plugin_path) -> an open ConversionRun-like
# context manager. Kept as a plain callable (not a Protocol) — the only thing
# callers need is the injection seam for tests.
RunFactory = Callable[[str, str, str], Any]

BACKUP_SUFFIX = ".pre_precombine.bak"
BASE_MESH_ARCHIVE = "Fallout4 - Meshes.ba2"

# bacup/scripts/run_precombines_v0.py -> bacup/scripts -> bacup -> repo root.
REPO_ROOT = Path(__file__).resolve().parents[2]


def _read_env_path(name: str) -> Path | None:
    """Mirrors scripts/regen.py's `_read_env_path`: env var first, then a
    quoted `KEY="value"` line in the repo-root `.env`."""
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


def discover_mesh_archives(game_dir: Path | None, extra_archives: list[str]) -> list[str]:
    """Build the ordered `mesh_archives` list: the base game archive, then DLC
    archives (sorted for determinism), then any `--archive` extras.

    Missing game archives are warned-and-skipped rather than erroring, since a
    dev box may only have some DLCs installed. `--archive` extras are trusted
    as given (no existence check) and resolved to absolute paths.
    """
    archives: list[str] = []
    if game_dir is None:
        print("no --game-dir given and FO4_DIR is not set — proceeding with no game mesh archives")
    else:
        data_dir = game_dir / "Data"
        candidates = [data_dir / BASE_MESH_ARCHIVE, *sorted(data_dir.glob("DLC* - Main.ba2"))]
        for candidate in candidates:
            if candidate.is_file():
                archives.append(str(candidate))
            else:
                print(f"warning: mesh archive not found, skipping: {candidate}")

    archives.extend(str(Path(extra).resolve()) for extra in extra_archives)

    names = ", ".join(Path(a).name for a in archives) if archives else "none"
    print(f"mesh archives ({len(archives)}): {names}")
    return archives


def discover_mesh_extract_roots(extract_dir: Path | None) -> list[str]:
    """Return `[str(extract_dir)]` if a pre-extracted vanilla asset tree exists
    on disk (the `FO4_EXTRACTED_DIR` convention: `meshes/`, `materials/`,
    `textures/`, etc. as direct children — see `scripts/regen.py`'s
    `_resolve_fo4_extracted_dir`), else `[]`.

    ck-side searches extract roots after the mod's own loose data root and
    before `mesh_archives`, so this is the primary source when configured.
    """
    if extract_dir is None:
        return []
    if not extract_dir.is_dir():
        print(f"warning: extract dir not found, skipping: {extract_dir}")
        return []
    resolved = str(extract_dir.resolve())
    print(f"mesh extract root: {resolved}")
    return [resolved]


def _default_run_factory(source_game: str, target_game: str, target_plugin_path: str) -> Any:
    # Lazy import: importing this module must not require the native
    # extension to be built. See test_run_precombines_v0.py, which injects a
    # fake run_factory and never reaches this function.
    from bacup_lib.run import ConversionRun

    return ConversionRun.open_existing(source_game, target_game, None, target_plugin_path)


def backup_once(esm_path: Path) -> Path:
    """Copy `esm_path` to `<name>.pre_precombine.bak` if no backup exists yet."""
    backup_path = esm_path.with_name(esm_path.name + BACKUP_SUFFIX)
    if not backup_path.exists():
        shutil.copy2(esm_path, backup_path)
    return backup_path


def _drain_warning_messages(run: Any) -> list[str]:
    """Collect WARN-level log message text from `run`'s event queue.

    `run.drain_events()` is non-blocking and must be drained (and read) while
    `run` is still open — its queue is discarded once the `with` block exits.
    Phase report fields only carry a warning *count*; this is the only way to
    get the actual message text (e.g. which mesh path failed to resolve).
    """
    messages: list[str] = []
    while True:
        batch = run.drain_events(256)
        if not batch:
            return messages
        for event in batch:
            if event.get("kind") == "log" and event.get("level") == "WARN":
                messages.append(event["message"])


def run_precombines(
    mod_root: Path,
    cell: str,
    min_eligible_refs: int,
    no_previs: bool,
    *,
    esm_name: str = "SeventySix.esm",
    source_game: str = "fo76",
    target_game: str = "fo4",
    mesh_extract_roots: list[str] | None = None,
    mesh_archives: list[str] | None = None,
    run_factory: RunFactory = _default_run_factory,
) -> dict[str, Any]:
    """Run the v0 `generate_precombines` phase against `mod_root`'s ESM.

    The ESM is backed up once regardless of outcome. It is only replaced on
    disk if the phase reports `assets_written > 0`; a zero-asset report is
    returned unchanged and the on-disk ESM is left untouched.
    """
    esm_path = mod_root / esm_name
    if not esm_path.is_file():
        raise FileNotFoundError(f"target plugin not found: {esm_path}")

    backup_once(esm_path)

    params = {
        "include_cells": [cell],
        "min_eligible_refs": min_eligible_refs,
        "no_previs": no_previs,
        "mesh_extract_roots": list(mesh_extract_roots or []),
        "mesh_archives": list(mesh_archives or []),
    }
    data_dir = mod_root / "data"

    with run_factory(source_game, target_game, str(esm_path)) as run:
        report = run.run_phase(
            "generate_precombines",
            mod_path=str(mod_root),
            target_data_dir=str(data_dir),
            params=params,
        )
        # Must drain while `run` is still open — its event queue is gone once
        # the `with` block exits.
        report["warning_messages"] = _drain_warning_messages(run)
        if not report.get("assets_written"):
            return report

        temp_fd, temp_name = tempfile.mkstemp(
            dir=esm_path.parent, prefix=f".{esm_path.name}.", suffix=".tmp"
        )
        temp_path = Path(temp_name)
        os.close(temp_fd)
        try:
            run.save_target(str(temp_path), run_nvnm_validator=False)
        except Exception:
            temp_path.unlink(missing_ok=True)
            raise

    os.replace(temp_path, esm_path)
    return report


def main(argv: list[str] | None = None, *, run_factory: RunFactory = _default_run_factory) -> int:
    parser = argparse.ArgumentParser(description="v0 CK-free precombine generation runner")
    parser.add_argument("--mod-root", default="mods/SeventySix")
    parser.add_argument("--cell", default="0062781C")
    parser.add_argument("--min-eligible-refs", type=int, default=1)
    parser.add_argument(
        "--no-previs",
        choices=("set", "clear"),
        default="set",
        help="'clear' runs the v0 No-Previs flag experiment without a Rust rebuild",
    )
    parser.add_argument(
        "--game-dir",
        default=None,
        help="Fallout 4 install dir (containing Data\\); defaults to FO4_DIR from .env",
    )
    parser.add_argument(
        "--extract-dir",
        default=None,
        help=(
            "pre-extracted FO4 vanilla asset tree (meshes/, materials/, ... as "
            "direct children); defaults to FO4_EXTRACTED_DIR from .env. Searched "
            "before mesh_archives when present."
        ),
    )
    parser.add_argument(
        "--archive",
        action="append",
        default=[],
        metavar="PATH",
        help="extra .ba2 to search after the game archives (repeatable)",
    )
    args = parser.parse_args(argv)

    extract_dir = Path(args.extract_dir).resolve() if args.extract_dir else _read_env_path("FO4_EXTRACTED_DIR")
    mesh_extract_roots = discover_mesh_extract_roots(extract_dir)

    game_dir = Path(args.game_dir).resolve() if args.game_dir else _read_env_path("FO4_DIR")
    mesh_archives = discover_mesh_archives(game_dir, args.archive)

    if mesh_extract_roots and mesh_archives:
        mode = "extract-dir (primary) + archives (fallback)"
    elif mesh_extract_roots:
        mode = "extract-dir"
    elif mesh_archives:
        mode = "archives"
    else:
        mode = "none (no source meshes configured)"
    print(f"mesh source mode: {mode}")

    report = run_precombines(
        Path(args.mod_root),
        args.cell,
        args.min_eligible_refs,
        args.no_previs == "set",
        mesh_extract_roots=mesh_extract_roots,
        mesh_archives=mesh_archives,
        run_factory=run_factory,
    )
    warning_messages = report.get("warning_messages", [])
    summary = {key: value for key, value in report.items() if key != "warning_messages"}
    print(summary)
    for message in warning_messages:
        print(f"  warning: {message}")

    if not report.get("assets_written"):
        data_dir = Path(args.mod_root) / "data"
        print(
            f"no precombines generated for cell {args.cell} — check that source meshes "
            f"exist as loose files under {data_dir}"
        )
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
