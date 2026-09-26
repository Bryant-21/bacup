"""Merge Tales UI translations into the loose F4SE runtime table."""

from __future__ import annotations

import argparse
import codecs
import importlib.resources
from pathlib import Path
from typing import Iterable

PACKAGED_TALES_DIR = "data/translations"
PACKAGED_TALES_RESOURCE = f"{PACKAGED_TALES_DIR}/B21_TalesFromAppalachia_en.txt"
TRANSLATIONS = Path("F4SE/Plugins/B21_TalesFromAppalachia_en.txt")
LEGACY_TRANSLATIONS = Path("Interface/Translations/B21_TalesFromAppalachia_en.txt")
TABLE_PREFIX = "B21_TalesFromAppalachia_"


def translation_path(output_data: Path, language: str = "en") -> Path:
    return Path(output_data).parent / TRANSLATIONS.with_name(f"{TABLE_PREFIX}{language}.txt")


def merge_ui_translations(output_data: Path, incoming: list[str]) -> tuple[int, int]:
    output_data = Path(output_data)
    target = translation_path(output_data)
    legacy = output_data / LEGACY_TRANSLATIONS
    if legacy.is_file():
        existing = read_table(target) if target.is_file() else []
        write_table(target, merge_lines(read_table(legacy), existing))
    result = merge_into(target, incoming)
    # Remove only after the replacement was written, so a partial conversion
    # cannot strand the other UI converters' accumulated keys in the old BA2 path.
    if legacy.is_file():
        legacy.unlink()
    return result


def parse_lines(text: str) -> list[str]:
    return [line for line in text.splitlines() if line]


def _key(line: str) -> str:
    return line.split("\t", 1)[0]


def merge_lines(existing: list[str], incoming: list[str]) -> list[str]:
    incoming_by_key: dict[str, str] = {}
    for line in incoming:
        incoming_by_key[_key(line)] = line
    preserved = [line for line in existing if _key(line) not in incoming_by_key]
    return preserved + list(incoming_by_key.values())


def _decode(data: bytes) -> str:
    if data[:2] in (b"\xff\xfe", b"\xfe\xff"):
        return data.decode("utf-16")
    return data.decode("utf-8-sig")


def read_table(path: Path) -> list[str]:
    return parse_lines(_decode(Path(path).read_bytes()))


def write_table(path: Path, lines: list[str], newline: str = "\r\n") -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    text = "".join(line + newline for line in lines)
    # Bytes, not write_text(): on Windows, text-mode write_text(encoding="utf-16")
    # translates every "\n" it finds to os.linesep, so a "\r\n" we already joined
    # becomes "\r\r\n" and a second merge sees the line count double.
    path.write_bytes(codecs.BOM_UTF16_LE + text.encode("utf-16-le"))


def _detect_newline(text: str) -> str:
    return "\r\n" if "\r\n" in text else "\n"


def merge_into(accumulator: Path, incoming: list[str]) -> tuple[int, int]:
    accumulator = Path(accumulator)
    if accumulator.is_file():
        raw = _decode(accumulator.read_bytes())
        existing = parse_lines(raw)
        newline = _detect_newline(raw)
    else:
        existing = []
        # write_text(..., encoding="utf-16") on Windows historically emitted CRLF
        # for a brand-new file; match that on-disk byte shape.
        newline = "\r\n"
    incoming_keys = {_key(line) for line in incoming}
    preserved = sum(1 for line in existing if _key(line) not in incoming_keys)
    written = len(incoming_keys)
    write_table(accumulator, merge_lines(existing, incoming), newline)
    return preserved, written


def merge_sources(accumulator: Path, sources: Iterable[Path]) -> tuple[int, int]:
    incoming: list[str] = []
    for source in sources:
        incoming.extend(read_table(source))
    return merge_into(accumulator, incoming)


def packaged_tales_lines(language: str = "en") -> list[str]:
    resource = importlib.resources.files("bacup_lib") / PACKAGED_TALES_DIR / f"{TABLE_PREFIX}{language}.txt"
    return parse_lines(resource.read_text(encoding="utf-8"))


def packaged_overlay_languages() -> list[str]:
    names = (entry.name for entry in (importlib.resources.files("bacup_lib") / PACKAGED_TALES_DIR).iterdir())
    return sorted(name[len(TABLE_PREFIX):-len(".txt")] for name in names
                  if name.startswith(TABLE_PREFIX) and name.endswith(".txt") and name != f"{TABLE_PREFIX}en.txt")


def merge_packaged_overlays(accumulator: Path) -> None:
    # Tales loads _en, then overlays the sLanguage table, so an overlay only needs the keys it translates.
    for language in packaged_overlay_languages():
        merge_into(Path(accumulator).with_name(f"{TABLE_PREFIX}{language}.txt"), packaged_tales_lines(language))


def merge_packaged(accumulator: Path) -> tuple[int, int]:
    result = merge_into(accumulator, packaged_tales_lines())
    merge_packaged_overlays(accumulator)
    return result


def _main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--accumulator", type=Path, required=True)
    parser.add_argument("--source", type=Path, action="append", default=[])
    args = parser.parse_args(argv)
    if args.source:
        merge_sources(args.accumulator, args.source)
    # Packaged lines always merge last, so they're printed as the final tally.
    preserved, written = merge_packaged(args.accumulator)
    print(f"preserved={preserved} written={written}")


if __name__ == "__main__":
    _main()
