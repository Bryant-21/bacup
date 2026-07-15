from __future__ import annotations

import json
import os


def mod_version_path(mod_dir: str) -> str:
    return os.path.join(mod_dir, ".version")


def read_mod_version(mod_dir: str) -> str:
    path = mod_version_path(mod_dir)
    if not os.path.isfile(path):
        return ""
    try:
        with open(path, encoding="utf-8") as f:
            return f.read().strip()
    except Exception:
        return ""


def write_mod_version(mod_dir: str, version: str) -> None:
    with open(mod_version_path(mod_dir), "w", encoding="utf-8") as f:
        f.write(version.strip())
        f.write("\n")


def release_history_path(release_dir: str) -> str:
    return os.path.join(release_dir, "release_history.json")


def read_release_history(release_dir: str) -> list[dict]:
    path = release_history_path(release_dir)
    if not os.path.isfile(path):
        return []
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        if isinstance(data, list):
            return [item for item in data if isinstance(item, dict)]
    except Exception:
        pass
    return []


def latest_tracked_version(release_dir: str) -> str:
    for entry in read_release_history(release_dir):
        version = str(entry.get("version", "")).strip()
        if version:
            return version
    return ""


def sanitize_release_token(value: str) -> str:
    cleaned = "".join(ch if ch.isalnum() or ch in ("-", "_", ".") else "_" for ch in value.strip())
    return cleaned.strip("._") or "release"


def render_release_notes(entry: dict) -> str:
    version = entry.get("version", "") or "unversioned"
    lines = [
        f"# {entry.get('mod', 'Mod')} Release Notes",
        "",
        f"- Version: {version}",
        f"- Released: {entry.get('released_at', '')}",
        f"- Game: {entry.get('game', '')}",
    ]
    previous_version = str(entry.get("previous_version", "")).strip()
    if previous_version:
        lines.append(f"- Previous Version: {previous_version}")
    plugin = entry.get("plugin", "")
    if plugin:
        lines.append(f"- Plugin: {plugin}")
    git_commit = entry.get("git_commit", "")
    if git_commit:
        lines.append(f"- Git Commit: {git_commit}")

    options = [str(opt).strip() for opt in entry.get("options", []) if str(opt).strip()]
    lines.extend(["", "## Package Options"])
    if options:
        lines.extend([f"- {opt}" for opt in options])
    else:
        lines.append("- Default packaging")

    lines.extend(["", "## Included Files"])
    artifacts = [str(name).strip() for name in entry.get("artifacts", []) if str(name).strip()]
    lines.extend([f"- {name}" for name in artifacts] or ["- No packaged artifacts recorded"])

    lines.extend(["", "## Notes"])
    notes = str(entry.get("notes", "")).strip()
    if notes:
        lines.extend(notes.splitlines())
    else:
        lines.append("No release notes entered.")
    lines.append("")
    return "\n".join(lines)


def render_changelog(entries: list[dict]) -> str:
    lines = ["# Changelog", ""]
    if not entries:
        lines.append("No releases recorded.")
        lines.append("")
        return "\n".join(lines)

    for entry in entries:
        version = entry.get("version", "") or "unversioned"
        released_at = entry.get("released_at", "")
        lines.extend([f"## {version} ({released_at})", ""])
        previous_version = str(entry.get("previous_version", "")).strip()
        if previous_version:
            lines.append(f"- Changes since `{previous_version}`")
        notes = str(entry.get("notes", "")).strip()
        if notes:
            lines.extend(notes.splitlines())
        else:
            lines.append("- No release notes entered.")
        git_commit = entry.get("git_commit", "")
        if git_commit:
            lines.append(f"- Git commit: `{git_commit}`")
        lines.append("")
    return "\n".join(lines)


def update_release_history(release_dir: str, entry: dict) -> list[dict]:
    history = read_release_history(release_dir)
    latest = history[0] if history else None
    same_as_latest = bool(latest) and all(
        str(latest.get(key, "")).strip() == str(entry.get(key, "")).strip()
        for key in ("version", "notes", "git_commit")
    )
    if same_as_latest:
        history[0] = entry
    else:
        history.insert(0, entry)

    with open(release_history_path(release_dir), "w", encoding="utf-8") as f:
        json.dump(history, f, indent=2)
        f.write("\n")

    with open(os.path.join(release_dir, "CHANGELOG.md"), "w", encoding="utf-8") as f:
        f.write(render_changelog(history))

    return history
