"""SubGraph Maker tool — create subgraph overlay files for animation mods."""

from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool

_log = logging.getLogger("tools.subgraph_maker")

RESOURCE_FILES = {
    "Human": "SubGraphData_HumanRaceSubGraphData.txt",
    "PowerArmor": "SubGraphData_PowerArmorRace.txt",
    "SuperMutant": "SubGraphData_SuperMutantRace.txt",
}


def _split_csv(s: str) -> list[str]:
    if not s:
        return []
    return [p.strip() for p in s.split(",") if p.strip()]


def _join_csv(items: list[str]) -> str:
    return ", ".join(items)


def _process_line(
    line: str,
    target_anim: str,
    new_anim: str,
    target_folder: str,
    new_folder: str,
    prepend_mode: bool,
) -> tuple[str, bool]:
    """Process a single TSV line. Returns (possibly modified line, matched)."""
    if not line.strip():
        return line, False

    newline = ""
    if line.endswith("\r\n"):
        newline = "\r\n"
        core = line[:-2]
    elif line.endswith("\n"):
        newline = "\n"
        core = line[:-1]
    else:
        core = line

    cols = core.split("\t")
    if len(cols) < 6:
        return line, False

    anim_tokens = _split_csv(cols[3])
    anim_targets = _split_csv(target_anim)
    if not anim_targets:
        return line, False

    has_anim = any(tok in anim_targets for tok in anim_tokens)
    if not has_anim:
        return line, False

    folder_tokens = _split_csv(cols[5])
    folder_targets = _split_csv(target_folder)

    match_indices: list[tuple[int, int]] = []
    for i, entry in enumerate(folder_tokens):
        parts = entry.replace("/", "\\").split("\\")
        for s_idx, seg in enumerate(parts):
            if seg in folder_targets:
                match_indices.append((i, s_idx))
                break

    if not match_indices:
        return line, False

    # Replace animation tokens
    new_anim_tokens = [new_anim if tok in anim_targets else tok for tok in anim_tokens]
    cols[3] = _join_csv(new_anim_tokens)

    if prepend_mode:
        offset = 0
        for idx, seg_idx in match_indices:
            real_idx = idx + offset
            entry = folder_tokens[real_idx]
            parts = entry.replace("/", "\\").split("\\")
            parts[seg_idx] = new_folder
            new_entry = "\\".join(parts)
            if real_idx - 1 >= 0 and folder_tokens[real_idx - 1] == new_entry:
                pass
            else:
                folder_tokens.insert(real_idx, new_entry)
                offset += 1
    else:
        for idx, seg_idx in match_indices:
            entry = folder_tokens[idx]
            parts = entry.replace("/", "\\").split("\\")
            parts[seg_idx] = new_folder
            folder_tokens[idx] = "\\".join(parts)

    cols[5] = _join_csv(folder_tokens)
    return "\t".join(cols) + newline, True


class SubGraphMakerTool(BaseTool):
    name = "SubGraph Maker"
    tool_id = "subgraph_maker"
    description = "Create subgraph overlay files"
    category = "Mod Tools"

    def __init__(self):
        super().__init__()
        self._target_anim = ""
        self._new_anim = ""
        self._target_folder = ""
        self._new_folder = ""
        self._prepend_mode = True
        self._human = True
        self._power_armor = False
        self._super_mutant = False
        self._resource_dir = ""

    def _find_resource_dir(self) -> str:
        """Locate the resource/ directory with SubGraphData files."""
        if self._resource_dir and os.path.isdir(self._resource_dir):
            return self._resource_dir
        from ui.toolkit.app_paths import get_resource_dir
        res = str(get_resource_dir())
        test = os.path.join(res, RESOURCE_FILES["Human"])
        return res if os.path.isfile(test) else ""

    def draw_content(self) -> None:
        imgui.text("Target Animation Keyword")
        imgui.set_next_item_width(-1)
        _, self._target_anim = imgui.input_text("##target_anim", self._target_anim)

        imgui.text("My Animation Keyword")
        imgui.set_next_item_width(-1)
        _, self._new_anim = imgui.input_text("##new_anim", self._new_anim)

        imgui.text("Target Animation Path(s) (CSV)")
        imgui.set_next_item_width(-1)
        _, self._target_folder = imgui.input_text("##target_folder", self._target_folder)

        imgui.text("My Mod Animation Path")
        imgui.set_next_item_width(-1)
        _, self._new_folder = imgui.input_text("##new_folder", self._new_folder)

        imgui.separator()

        _, self._prepend_mode = imgui.checkbox("Add paths (on) or Replace (off)", self._prepend_mode)

        imgui.spacing()
        imgui.text("Races:")
        _, self._human = imgui.checkbox("Human", self._human)
        imgui.same_line()
        _, self._power_armor = imgui.checkbox("PowerArmor", self._power_armor)
        imgui.same_line()
        _, self._super_mutant = imgui.checkbox("SuperMutant", self._super_mutant)

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                self._validate_and_run()
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _validate_and_run(self):
        races = []
        if self._human:
            races.append("Human")
        if self._power_armor:
            races.append("PowerArmor")
        if self._super_mutant:
            races.append("SuperMutant")

        if not races:
            self._error_msg = "Please select at least one race."
            return
        if not self._target_anim.strip():
            self._error_msg = "Please enter Target Animation Keyword."
            return
        if not self._new_anim.strip():
            self._error_msg = "Please enter My Animation Keyword."
            return
        if not self._target_folder.strip():
            self._error_msg = "Please enter Target Animation Path(s)."
            return
        if not self._new_folder.strip():
            self._error_msg = "Please enter My Mod Animation Path."
            return

        self._start_batch(self._run_subgraph, races)

    def _run_subgraph(self, races: list[str]):
        resource_dir = self._find_resource_dir()
        if not resource_dir:
            self._error_msg = "Could not find resource/ directory with SubGraphData files."
            return

        # Output dir
        out_dir = os.path.join(os.path.dirname(resource_dir), "output", "subgraph")
        os.makedirs(out_dir, exist_ok=True)

        total = len(races)
        results = []

        for idx, race in enumerate(races):
            if self._cancel_requested:
                break

            filename = RESOURCE_FILES.get(race)
            if not filename:
                continue

            in_path = os.path.join(resource_dir, filename)
            if not os.path.isfile(in_path):
                self._on_progress(idx + 1, total, f"Missing: {filename}")
                results.append(f"{filename}: MISSING")
                continue

            name_no_ext = os.path.splitext(filename)[0]
            nf_label = self._new_folder.strip().replace(",", "+").replace(" ", "") or "new"
            out_name = f"{name_no_ext}_{nf_label}_additive.txt"
            out_path = os.path.join(out_dir, out_name)

            matches = []
            header_line = None
            try:
                with open(in_path, "r", encoding="utf-8") as f:
                    for line_idx, raw in enumerate(f):
                        if self._cancel_requested:
                            return
                        if line_idx == 0:
                            header_line = raw
                            continue
                        new_line, matched = _process_line(
                            raw,
                            self._target_anim.strip(),
                            self._new_anim.strip(),
                            self._target_folder.strip(),
                            self._new_folder.strip(),
                            self._prepend_mode,
                        )
                        if matched:
                            matches.append(new_line)
            except UnicodeDecodeError:
                with open(in_path, "r", encoding="cp1252", errors="ignore") as f:
                    for line_idx, raw in enumerate(f):
                        if line_idx == 0:
                            header_line = raw
                            continue
                        new_line, matched = _process_line(
                            raw,
                            self._target_anim.strip(),
                            self._new_anim.strip(),
                            self._target_folder.strip(),
                            self._new_folder.strip(),
                            self._prepend_mode,
                        )
                        if matched:
                            matches.append(new_line)

            if matches:
                with open(out_path, "w", encoding="utf-8", newline="") as out_f:
                    if header_line is not None:
                        if not header_line.endswith("\n"):
                            header_line += "\n"
                        out_f.write(header_line)
                    for m in matches:
                        if not m.endswith("\n"):
                            m += "\n"
                        out_f.write(m)
                results.append(f"{out_name}: {len(matches)} entries")
            else:
                results.append(f"{filename}: no matching rows")

            self._on_progress(idx + 1, total, f"Processed: {race}")

        self._result_msg = "Results:\n" + "\n".join(results) + f"\nOutput: {out_dir}"
