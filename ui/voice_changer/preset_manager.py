"""Preset manager — load/save/reset/delete voice changer presets."""
from __future__ import annotations

import json
import logging
import os
import shutil
from typing import Any

_log = logging.getLogger("toolkit.voice_changer.presets")


class PresetManager:
    """Manages built-in and user presets stored as JSON files.

    Built-in presets live in a version-controlled directory (read-only at runtime).
    User presets live in a separate directory and can override built-ins.
    Loading checks user dir first, then falls back to built-in.
    """

    def __init__(self, builtin_dir: str, user_dir: str):
        self._builtin_dir = builtin_dir
        self._user_dir = user_dir
        os.makedirs(self._user_dir, exist_ok=True)

    def list_presets(self) -> list[dict[str, Any]]:
        """List all available presets (built-in + user), with metadata.

        Returns list of dicts with keys: slug, name, description, builtin.
        User presets that override built-ins show as builtin=True (since they
        shadow a built-in). Pure user presets show as builtin=False.
        """
        seen: dict[str, dict] = {}

        # Built-ins first
        for slug in self._scan_dir(self._builtin_dir):
            data = self._read_json(os.path.join(self._builtin_dir, f"{slug}.json"))
            if data:
                seen[slug] = {
                    "slug": slug,
                    "name": data.get("name", slug),
                    "description": data.get("description", ""),
                    "builtin": True,
                }

        # User presets (override or new)
        for slug in self._scan_dir(self._user_dir):
            data = self._read_json(os.path.join(self._user_dir, f"{slug}.json"))
            if data:
                if slug not in seen:
                    seen[slug] = {
                        "slug": slug,
                        "name": data.get("name", slug),
                        "description": data.get("description", ""),
                        "builtin": False,
                    }
                else:
                    # User override of built-in — update name/desc but keep builtin=True
                    seen[slug]["name"] = data.get("name", slug)
                    seen[slug]["description"] = data.get("description", "")

        return sorted(seen.values(), key=lambda p: p["name"])

    def load_preset(self, slug: str) -> dict[str, Any] | None:
        """Load a preset by slug. User dir takes priority over built-in."""
        user_path = os.path.join(self._user_dir, f"{slug}.json")
        if os.path.isfile(user_path):
            return self._read_json(user_path)
        builtin_path = os.path.join(self._builtin_dir, f"{slug}.json")
        if os.path.isfile(builtin_path):
            return self._read_json(builtin_path)
        return None

    def save_preset(
        self, slug: str, name: str, description: str, chain: list[dict]
    ) -> None:
        """Save a preset to the user directory."""
        data = {"name": name, "description": description, "chain": chain}
        path = os.path.join(self._user_dir, f"{slug}.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)
        _log.info("Saved preset: %s -> %s", slug, path)

    def reset_preset(self, slug: str) -> None:
        """Reset a built-in preset by removing the user override."""
        user_path = os.path.join(self._user_dir, f"{slug}.json")
        if os.path.isfile(user_path):
            os.remove(user_path)
            _log.info("Reset preset to default: %s", slug)

    def delete_preset(self, slug: str) -> None:
        """Delete a user preset. Built-in presets cannot be deleted."""
        user_path = os.path.join(self._user_dir, f"{slug}.json")
        if os.path.isfile(user_path):
            os.remove(user_path)
            _log.info("Deleted user preset: %s", slug)

    def is_builtin(self, slug: str) -> bool:
        """Check if a preset exists as a built-in."""
        return os.path.isfile(os.path.join(self._builtin_dir, f"{slug}.json"))

    def _scan_dir(self, directory: str) -> list[str]:
        """Return slugs (filenames without .json) from a directory."""
        if not os.path.isdir(directory):
            return []
        return [
            f[:-5] for f in os.listdir(directory)
            if f.endswith(".json") and os.path.isfile(os.path.join(directory, f))
        ]

    def _read_json(self, path: str) -> dict[str, Any] | None:
        try:
            with open(path, "r", encoding="utf-8") as f:
                return json.load(f)
        except (json.JSONDecodeError, OSError):
            _log.warning("Failed to read preset: %s", path)
            return None
