from __future__ import annotations

from dataclasses import dataclass, field

SOURCE_KINDS = ("mo2", "modbox", "plugin")
SOURCE_LABELS = {"mo2": "MO2 profile", "modbox": "ModBox21 profile", "plugin": "Single plugin + masters"}
ACTIONS = ("rebuild_precombines", "rebuild_previs", "fix", "warn", "info")
ACTION_LABELS = {
    "rebuild_precombines": "Rebuild precombines",
    "rebuild_previs": "Rebuild previs",
    "fix": "Fix",
    "warn": "Warning",
    "info": "Info",
}


@dataclass
class PrevisState:
    source_kind: str = "mo2"
    mo2_profile_dir: str = ""
    modbox_library: str = ""
    modbox_profile_id: str = ""
    modbox_profiles: list[tuple[str, str]] = field(default_factory=list)
    plugin_path: str = ""
    # Plugins the single plugin is checked against without mastering them (PPF.esm, another mod).
    against: list[str] = field(default_factory=list)
    include_base_conflicts: bool = False
    restore_disabled: bool = False
    unprecombined_enabled: bool = False
    unprecombined_threshold: int = 5
    # "broken", "plugin" (every cell of the single plugin) or "all" (the whole load order).
    scope: str = "broken"

    running: bool = False
    progress_msg: str = ""
    progress_frac: float = 0.0
    error_message: str = ""

    report: dict | None = None
    load_order_label: str = ""

    output_dir: str = ""
    patch_name: str = ""
    into_plugin: bool = False
    plugin_type: str = "esp"
    pack: bool = True
    small_model_cull: bool = False
    build_log: list[str] = field(default_factory=list)
    build_result: dict | None = None
    plugins: list[str] = field(default_factory=list)

    action_filter: set[str] = field(default_factory=lambda: {"rebuild_precombines", "rebuild_previs", "fix", "warn"})
    show_overridden_only: bool = False
    search: str = ""
    selected_cell: str = ""
    selected_cluster: str = ""

    def single_plugin(self) -> bool:
        return self.source_kind == "plugin" and bool(self.plugin_path)

    def against_paths(self) -> list[str]:
        return [p for p in self.against if p.strip()] if self.source_kind == "plugin" else []

    def default_patch_name(self, fallback: str) -> str:
        from pathlib import Path

        against = self.against_paths()
        if against and self.plugin_path:
            return f"{Path(self.plugin_path).stem} - {Path(against[0]).stem} Patch.{self.plugin_type}"
        return f"{Path(fallback).stem}.{self.plugin_type}"

    def stamps_plugin(self) -> bool:
        """The build writes into a copy of the chosen plugin instead of a new patch."""
        return self.single_plugin() and self.into_plugin

    def visible_cells(self) -> list[dict]:
        if not self.report:
            return []
        needle = self.search.strip().lower()
        out = []
        for cell in self.report["cells"]:
            action = cell.get("action")
            if action is None:
                if not self.show_overridden_only:
                    continue
            elif action not in self.action_filter:
                continue
            if needle and not _matches(cell, needle):
                continue
            out.append(cell)
        return out

    def visible_clusters(self) -> list[dict]:
        if not self.report:
            return []
        needle = self.search.strip().lower()
        return [
            c for c in self.report["clusters"]
            if c.get("action") in self.action_filter and (not needle or _matches(c, needle))
        ]


def _matches(item: dict, needle: str) -> bool:
    fields = [item.get("form_key"), item.get("editor_id"), item.get("world"), item.get("winning_plugin")]
    fields += [f.get("blamed") for f in item.get("findings", [])]
    fields += [f.get("code") for f in item.get("findings", [])]
    return any(needle in str(value).lower() for value in fields if value)
