import argparse
import hashlib
from functools import partial
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def command(*args, executable=None):
    prefix = [str(executable)] if executable else [sys.executable, "-m", "cli.main"]
    result = subprocess.run([*prefix, "--format", "json", "manager", *map(str, args)],
                            capture_output=True, text=True, encoding="utf-8", timeout=240)
    if result.returncode:
        raise RuntimeError(result.stderr.strip())
    return json.loads(result.stdout)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--game-root", required=True)
    parser.add_argument("--packages", type=Path, required=True, help="JSON array of explicit package paths")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--executable", type=Path, help="Test a packaged modkit.exe instead of the source CLI.")
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    library = Path(tempfile.mkdtemp(prefix="installer-library-", dir=args.output.parent)) / "library"
    run = partial(command, executable=args.executable)
    run("create-instance", "--library", library, "--game-root", args.game_root, "--name", "Acceptance")
    report = {"library": str(library), "packages": [], "passed": True}
    for package in map(Path, json.loads(args.packages.read_text(encoding="utf-8-sig"))):
        before = digest(package)
        row = {"package": str(package), "sha256": before, "variants": []}
        try:
            base = ["install", "--library", library, "--profile", "Acceptance", "--package", package]
            preview = run(*base)
            row["fomod"] = preview["fomod"]
            variants = [("default", {})]
            for step in preview["steps"]:
                for group in step["groups"]:
                    if group["type"] in {"SelectExactlyOne", "SelectAtMostOne"}:
                        alternative = next((o for o in reversed(group["options"]) if not o["selected"] and o["type"] != "NotUsable"), None)
                        if alternative:
                            variants.append((group["name"], {group["id"]: [alternative["id"]]}))
            for label, choices in variants[:5]:
                preview = run(*base, "--choices", json.dumps(choices)) if choices else preview
                if preview["errors"]:
                    raise RuntimeError("; ".join(preview["errors"]))
                result = run(*base, "--choices", json.dumps(choices), "--name", package.stem[:100] + " " + label[:50],
                                 "--apply", "--expect", preview["review_token"])
                destination = library / "mods" / result["mod_id"]
                installed = {p.relative_to(destination).as_posix() for p in destination.rglob("*") if p.is_file()}
                if installed != set(result["plan"]):
                    raise RuntimeError("Installed files differ from the reviewed plan")
                row["variants"].append({"label": label, "files": len(installed), "mod_id": result["mod_id"], "choices": choices})
        except (RuntimeError, subprocess.TimeoutExpired) as exc:
            row["error"] = str(exc)
            report["passed"] = False
        row["source_unchanged"] = digest(package) == before
        report["passed"] &= row["source_unchanged"]
        report["packages"].append(row)
        args.output.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps({"package": package.name, "variants": len(row["variants"]), "error": row.get("error", ""),
                          "source_unchanged": row["source_unchanged"]}), flush=True)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
