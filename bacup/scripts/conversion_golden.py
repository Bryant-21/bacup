"""Byte-equivalence golden harness for the conversion pipeline.

capture  — run the current-`main` engine for a target and archive its output tree.
compare  — run the target again (through whatever the engine is now) and assert
           the output tree is byte-identical to the archived golden.

The golden archive is local-only (large ESM/BA2); never committed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

# Files whose bytes legitimately vary run-to-run — excluded from the byte gate.
_NONDETERMINISTIC = {
    "conversion_timing.json",
    "conversion_memory.json",
    "conversion_memory.md",
    "conversion_report.md",
    "asset_provenance.jsonl",
    "record_provenance.jsonl",
    "asset_map.json",
    "script_port_report.json",
}
_NONDETERMINISTIC_SUFFIXES = {".log"}
_NONDETERMINISTIC_LOWER = {name.lower() for name in _NONDETERMINISTIC}

# bytes vary run-to-run (rayon-parallel NIF conversion; archive packing) -> set+count parity only, never byte-compared
_SET_ONLY_SUFFIXES = (".nif", ".ba2")


def _is_set_only(rel: str) -> bool:
    return rel.lower().endswith(_SET_ONLY_SUFFIXES)


def _is_diagnostic_debug(rel: str) -> bool:
    segs = [s.lower() for s in rel.replace("\\", "/").split("/")]
    if "debug" not in segs:
        return False
    # diagnostic debug dir sits beside data/, never inside it; assets under data/.../debug/ are real
    return "data" not in segs[: segs.index("debug")]


def _is_excluded(rel: str) -> bool:
    if _is_diagnostic_debug(rel):
        return True
    name = rel.replace("\\", "/").rsplit("/", 1)[-1]
    if name.lower() in _NONDETERMINISTIC_LOWER:
        return True
    return any(name.lower().endswith(suffix) for suffix in _NONDETERMINISTIC_SUFFIXES)


def hash_tree(root: Path) -> dict[str, str]:
    """Map relative-path -> sha256 for every file under *root*, minus excludes."""
    root = Path(root)
    out: dict[str, str] = {}
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if _is_excluded(rel):
            continue
        out[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


def _ci_lookup(tree: dict[str, str]) -> dict[str, tuple[str, str]]:
    """Map lowercased-posix key -> (original-key, hash); first-seen wins on collision."""
    out: dict[str, tuple[str, str]] = {}
    for key, value in tree.items():
        ci = key.replace("\\", "/").lower()
        if ci not in out:
            out[ci] = (key, value)
    return out


def diff_trees(golden: dict[str, str], actual: dict[str, str]) -> list[str]:
    """Return human-readable mismatch lines; empty list == pass.

    Comparison is case-insensitive (directory case is non-deterministic). .nif/.ba2
    files are checked for presence only — their bytes vary run-to-run.
    """
    problems: list[str] = []
    golden_ci = _ci_lookup(golden)
    actual_ci = _ci_lookup(actual)

    for ci in sorted(set(golden_ci) - set(actual_ci)):
        problems.append(f"{golden_ci[ci][0]}: missing in actual")
    for ci in sorted(set(actual_ci) - set(golden_ci)):
        problems.append(f"{actual_ci[ci][0]}: unexpected in actual")
    for ci in sorted(set(golden_ci) & set(actual_ci)):
        if _is_set_only(ci):
            continue
        if golden_ci[ci][1] != actual_ci[ci][1]:
            problems.append(f"{actual_ci[ci][0]}: hash mismatch")
    return problems


def _manifest_path(golden_dir: Path) -> Path:
    return golden_dir / "_manifest.json"


def cmd_capture(args: argparse.Namespace) -> int:
    output_dir = Path(args.output_dir)
    golden_dir = Path(args.golden_dir)
    if golden_dir.exists():
        shutil.rmtree(golden_dir)
    if args.manifest_only:
        golden_dir.mkdir(parents=True)
        manifest = hash_tree(output_dir)
        _manifest_path(golden_dir).write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        print(f"captured {len(manifest)} files (manifest-only) -> {golden_dir}")
    else:
        shutil.copytree(output_dir, golden_dir / "tree")
        manifest = hash_tree(golden_dir / "tree")
        _manifest_path(golden_dir).write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        print(f"captured {len(manifest)} files -> {golden_dir}")
    return 0


def cmd_compare(args: argparse.Namespace) -> int:
    output_dir = Path(args.output_dir)
    golden_dir = Path(args.golden_dir)
    manifest_file = _manifest_path(golden_dir)
    if not manifest_file.is_file():
        print(f"ERROR: no golden manifest at {manifest_file}; run `capture` first", file=sys.stderr)
        return 2
    golden = json.loads(manifest_file.read_text(encoding="utf-8"))
    actual = hash_tree(output_dir)
    problems = diff_trees(golden, actual)

    set_only = sum(1 for rel in golden if _is_set_only(rel))
    byte_checked = len(golden) - set_only
    print(
        f"summary: golden={len(golden)} byte-checked={byte_checked} set-only(nif/ba2)={set_only}"
        " | case-insensitive paths; nif/ba2 bytes excluded as non-deterministic (rayon-parallel conversion)"
    )

    if problems:
        print(f"BYTE MISMATCH ({len(problems)} problem(s)):", file=sys.stderr)
        for line in problems[:100]:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"OK: {byte_checked} files byte-identical, {set_only} nif/ba2 set+count-parity")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Conversion byte-equivalence golden harness")
    sub = parser.add_subparsers(dest="cmd", required=True)

    cap = sub.add_parser("capture", help="archive an output tree as the golden")
    cap.add_argument("--output-dir", required=True, help="conversion output dir to archive")
    cap.add_argument("--golden-dir", required=True, help="where to store the golden")
    cap.add_argument("--manifest-only", action="store_true", help="write only the sha manifest; skip archiving the output tree (for large outputs)")
    cap.set_defaults(func=cmd_capture)

    cmp_ = sub.add_parser("compare", help="diff an output tree against the golden")
    cmp_.add_argument("--output-dir", required=True, help="freshly produced output dir")
    cmp_.add_argument("--golden-dir", required=True, help="archived golden")
    cmp_.set_defaults(func=cmd_compare)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
