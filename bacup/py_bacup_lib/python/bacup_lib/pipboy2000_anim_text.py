"""AnimTextData for the Tales Pip-Boy 2000 first-person arm subgraph.

Tales registers FO76's ``Pipboy2000.hkx`` arm graph on its own additive RACE, gated by a
Tales actor keyword. No converted RACE carries that subgraph, so the RACE-driven
AnimTextData pass never covers it; this step emits its buckets from the converted graph
and clips. The engine keys the files by core behavior and SAPT chain only, never by
keywords, so the gated Tales entry shares the id of FO76's ungated one.
"""
from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable, Iterable

CORE_BEHAVIOR = r"Actors\Character\_1stPerson\Behaviors\Pipboy2000.hkx"
SAPT_CHAIN = (
    r"Actors\Character\_1stPerson\Animations\Pipboy2000",
    r"Actors\Character\_1stPerson\Animations\Pipboy",
    r"Actors\Character\_1stPerson\Animations\Common",
)
SUBGRAPH_ID = 5888384441762997188
CONVERTED_ARCHIVE = "SeventySix - Main.ba2"
BASE_ARCHIVE = "Fallout4 - Animations.ba2"


def _meshes_path(windows_path: str) -> str:
    return "meshes/" + windows_path.replace("\\", "/").lower()


class DataSource:
    """Meshes looked up loose under a Data directory first, then in archives."""

    def __init__(self, data_dir: Path | None, archives: Iterable[Path] = ()):
        from creation_lib.ba2.ba2_reader import BA2File

        self.data_dir = Path(data_dir) if data_dir else None
        self.archives = [BA2File(path) for path in archives if Path(path).is_file()]

    def files(self, directory: str) -> dict[str, Callable[[], bytes | None]]:
        prefix = directory.rstrip("/") + "/"
        found: dict[str, Callable[[], bytes | None]] = {}
        for archive in reversed(self.archives):
            for name in archive.list_files(prefix):
                found[name] = lambda archive=archive, name=name: archive.extract(name)
        if self.data_dir is not None and (self.data_dir / prefix).is_dir():
            for path in (self.data_dir / prefix).rglob("*"):
                if path.is_file():
                    name = path.relative_to(self.data_dir).as_posix().lower()
                    found[name] = path.read_bytes
        return found

    def close(self) -> None:
        for archive in self.archives:
            archive.close()


def converted_source(converted_data: Path | None, target_data: Path | None) -> DataSource:
    archives = [Path(target_data) / CONVERTED_ARCHIVE] if target_data else []
    return DataSource(converted_data, archives)


def base_source(target_data: Path) -> DataSource:
    return DataSource(target_data, [Path(target_data) / BASE_ARCHIVE])


def _stage(source: DataSource, directories: Iterable[str], root: Path,
           keep: Callable[[str], bool] = lambda _name: True) -> int:
    count = 0
    for directory in directories:
        for name, read in source.files(directory).items():
            data = read() if keep(name) else None
            if data is None:
                continue
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            count += 1
    return count


def owned_files(meshes_root: Path) -> list[Path]:
    """This subgraph's bucket files under ``meshes_root``."""
    atd = meshes_root / "AnimTextData"
    if not atd.is_dir():
        return []
    return sorted(path for path in atd.glob("*/*.txt") if path.stem == str(SUBGRAPH_ID))


def emit_pipboy2000_anim_text(
    output_mod: Path,
    converted: DataSource,
    base: DataSource,
    *,
    log: Callable[[str], None] = lambda _message: None,
) -> list[Path]:
    """Write the subgraph's AnimTextData into ``output_mod/data/Meshes``; return the files."""
    from bacup_lib.native_runtime import load_native_module

    out_meshes = Path(output_mod) / "data" / "Meshes"
    for stale in owned_files(out_meshes):
        stale.unlink()
    core = _meshes_path(CORE_BEHAVIOR)
    with tempfile.TemporaryDirectory(prefix="pipboy2000_atd_") as temp:
        # Everything is staged as base data, leaving the source tree empty. The generator
        # then resolves the graph like vanilla's CK does, across the whole SAPT chain,
        # instead of renaming aliased clip generators in a graph the install won't get.
        src, staged, out = Path(temp, "src"), Path(temp, "staged"), Path(temp, "out")
        (src / "meshes").mkdir(parents=True)
        _stage(converted, [core.rsplit("/", 1)[0]], staged, keep=lambda name: name == core)
        if not (staged / core).is_file():
            raise FileNotFoundError(f"converted Pip-Boy 2000 arm graph is missing: {core}")
        if not _stage(converted, [_meshes_path(SAPT_CHAIN[0])], staged):
            raise FileNotFoundError(f"converted Pip-Boy 2000 clips are missing: {SAPT_CHAIN[0]}")
        if not _stage(base, [_meshes_path(path) for path in SAPT_CHAIN[1:]], staged):
            raise FileNotFoundError("base-game Pip-Boy clips are missing")
        messages: list[str] = []
        load_native_module().conversion_generate_subgraph_anim_text_data(
            json.dumps([{"core_behavior": CORE_BEHAVIOR, "sapt_chain": list(SAPT_CHAIN)}]),
            str(src / "meshes"),
            str(out),
            base_meshes_root=str(staged / "meshes"),
            progress_callback=messages.append,
        )
        for message in messages:
            log(message)
        written = []
        for path in owned_files(out):
            target = out_meshes / path.relative_to(out)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            written.append(target)
    if not any(path.parent.name == "AnimationFileData" for path in written):
        raise RuntimeError("no AnimationFileData was generated for the Pip-Boy 2000 subgraph")
    return written


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output-mod", type=Path, required=True,
                        help="converted-mod directory; files land in <dir>/data/Meshes/AnimTextData")
    parser.add_argument("--target-data", type=Path, required=True,
                        help=f"Fallout 4 Data directory (base clips, installed {CONVERTED_ARCHIVE})")
    parser.add_argument("--converted-data", type=Path,
                        help="converted mod's data directory with loose meshes (wins over the archive)")
    args = parser.parse_args()
    converted = converted_source(args.converted_data, args.target_data)
    base = base_source(args.target_data)
    try:
        files = emit_pipboy2000_anim_text(args.output_mod, converted, base, log=print)
    finally:
        converted.close()
        base.close()
    for path in files:
        print(path)
