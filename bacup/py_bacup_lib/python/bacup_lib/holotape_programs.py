"""Ship the FO76-only Pip-Boy holotape games.

A Program holotape (NOTE.DNAM = Program) names its game in NOTE.PNAM, which FO4
loads from ``Programs/<name>``. The games FO76 shares with FO4 resolve to FO4's own
copies; the framework files they share (fonts_programs.swf, the pause/quit
assets) are byte-identical in both games, so FO76's own games run unchanged.
"""
from __future__ import annotations

from pathlib import Path

FO76_ONLY_PROGRAMS = ("nukatapper", "wastelad")


def copy_fo76_holotape_programs(source_root: Path, output_data: Path) -> list[str]:
    source = Path(source_root) / "programs"
    copied = []
    for path in sorted(source.rglob("*")):
        if not path.is_file() or not path.name.casefold().startswith(FO76_ONLY_PROGRAMS):
            continue
        relative = Path("Programs") / path.relative_to(source)
        target = Path(output_data) / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(path.read_bytes())
        copied.append(relative.as_posix())
    return copied
