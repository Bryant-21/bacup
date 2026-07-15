"""NIF Collision Generator — bulk tool for per-part collision generation."""

from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_run_cancel_buttons, pick_file

_log = logging.getLogger("tools.collision_gen")

SHAPE_TYPES = ["capsule", "cylinder", "sphere", "box", "convex_hull", "auto", "optimized", "mopp", "compressed_mesh"]
SHAPE_TYPE_LABELS = {s: s.replace("_", " ").title() for s in SHAPE_TYPES}

LAYER_NAMES = [
    "STATIC", "ANIMSTATIC", "TRANSPARENT", "CLUTTER", "WEAPON",
    "PROJECTILE", "NPC", "TERRAIN", "BIPED", "TREES", "DEADBIP", "CHARCONTROLLER",
]

PRESET_NAMES = ["Weapon", "Custom"]


class CollisionGeneratorTool(BaseTool):
    name = "NIF Collision Generator"
    tool_id = "nif_collision_gen"
    description = "Generate per-part collision for NIF meshes"
    category = "NIF"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._output_dir = ""
        self._include_subdirs = True
        self._overwrite = True

        # Grouping
        self._group_by_node = True  # True=node, False=shape

        # Part mappings table
        self._preset_idx = 0
        self._mappings: list[list[str]] = []  # [[pattern, shape_type], ...]
        self._load_preset("Weapon")

        # Physics
        self._layer_idx = LAYER_NAMES.index("WEAPON")
        self._mass = 0.0
        self._friction = 0.5
        self._restitution = 0.4
        self._radius = 0.05
        self._replace_existing = True

        # Preview state
        self._preview_open = False
        self._preview_parts: list[dict] = []
        self._preview_unmatched: list[str] = []

    def _load_preset(self, name: str) -> None:
        if name == "Weapon":
            from creation_lib.nif.operations.collision_parts import WEAPON_PRESETS
            self._mappings = [[m.pattern, m.shape_type] for m in WEAPON_PRESETS]
        else:
            self._mappings = []

    def _get_part_mappings(self):
        from creation_lib.nif.operations.collision_parts import PartMapping
        return [PartMapping(m[0], m[1]) for m in self._mappings if m[0].strip()]

    def draw_content(self) -> None:
        # --- Input/Output ---
        imgui.separator_text("Input / Output")

        pick_input = pick_output = False
        if begin_form("io##colgen"):
            _, pick_input = draw_path_row("Input", self._input_path)
            _, pick_output = draw_path_row("Output", self._output_dir, "Browse...##out")
            end_form()

        if pick_input:
            path = pick_file("Select NIF file or folder", [("NIF", "*.nif"), ("All", "*.*")])
            if path:
                self._input_path = path
        if pick_output:
            folder = pick_folder("Output directory (optional)")
            if folder:
                self._output_dir = folder

        _, self._include_subdirs = imgui.checkbox("Include subdirectories", self._include_subdirs)
        imgui.same_line()
        _, self._overwrite = imgui.checkbox("Overwrite originals", self._overwrite)

        imgui.spacing()

        # --- Grouping ---
        imgui.separator_text("Grouping")
        if imgui.radio_button("By parent NiNode name", self._group_by_node):
            self._group_by_node = True
        imgui.same_line()
        if imgui.radio_button("By BSTriShape name", not self._group_by_node):
            self._group_by_node = False

        imgui.spacing()

        # --- Part Mappings ---
        imgui.separator_text("Part Mappings")

        # Preset dropdown
        imgui.text("Preset:")
        imgui.same_line()
        imgui.set_next_item_width(150)
        changed, self._preset_idx = imgui.combo("##preset", self._preset_idx, PRESET_NAMES)
        if changed:
            self._load_preset(PRESET_NAMES[self._preset_idx])

        # Mappings table
        flags = (imgui.TableFlags_.borders_inner_h
                 | imgui.TableFlags_.row_bg
                 | imgui.TableFlags_.sizing_stretch_prop)
        if imgui.begin_table("mappings", 3, flags):
            imgui.table_setup_column("Pattern", imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("Shape Type", imgui.TableColumnFlags_.width_fixed, 150)
            imgui.table_setup_column("", imgui.TableColumnFlags_.width_fixed, 30)
            imgui.table_headers_row()

            to_remove = -1
            for i, mapping in enumerate(self._mappings):
                imgui.table_next_row()

                imgui.table_set_column_index(0)
                imgui.set_next_item_width(-1)
                changed, mapping[0] = imgui.input_text(f"##pat_{i}", mapping[0])

                imgui.table_set_column_index(1)
                imgui.set_next_item_width(-1)
                cur_idx = SHAPE_TYPES.index(mapping[1]) if mapping[1] in SHAPE_TYPES else 0
                changed, new_idx = imgui.combo(f"##st_{i}", cur_idx, SHAPE_TYPES)
                if changed:
                    mapping[1] = SHAPE_TYPES[new_idx]

                imgui.table_set_column_index(2)
                if imgui.small_button(f"-##rm_{i}"):
                    to_remove = i

            imgui.end_table()

            if to_remove >= 0:
                self._mappings.pop(to_remove)

        if imgui.button("+ Add Row"):
            self._mappings.append(["", "convex_hull"])

        imgui.spacing()

        # --- Physics ---
        imgui.separator_text("Physics")

        imgui.text("Layer:")
        imgui.same_line()
        imgui.set_next_item_width(150)
        _, self._layer_idx = imgui.combo("##layer", self._layer_idx, LAYER_NAMES)

        imgui.same_line(spacing=20)
        imgui.text("Mass:")
        imgui.same_line()
        imgui.set_next_item_width(80)
        _, self._mass = imgui.input_float("##mass", self._mass, 0.0, 0.0, "%.1f")

        imgui.text("Friction:")
        imgui.same_line()
        imgui.set_next_item_width(80)
        _, self._friction = imgui.input_float("##friction", self._friction, 0.0, 0.0, "%.2f")

        imgui.same_line(spacing=20)
        imgui.text("Restitution:")
        imgui.same_line()
        imgui.set_next_item_width(80)
        _, self._restitution = imgui.input_float("##restitution", self._restitution, 0.0, 0.0, "%.2f")

        imgui.text("Radius:")
        imgui.same_line()
        imgui.set_next_item_width(80)
        _, self._radius = imgui.input_float("##radius", self._radius, 0.0, 0.0, "%.3f")

        imgui.same_line(spacing=20)
        _, self._replace_existing = imgui.checkbox("Replace existing collision", self._replace_existing)

        imgui.spacing()

        # --- Preview ---
        imgui.separator_text("Preview")
        _, self._preview_open = imgui.collapsing_header("Preview Results", self._preview_open)
        if self._preview_open:
            if imgui.button("Preview Single NIF"):
                self._run_preview()

            if self._preview_parts:
                imgui.text(f"Detected {len(self._preview_parts)} part(s):")
                for p in self._preview_parts:
                    imgui.bullet_text(
                        f"{p['name']} -> {p['pattern']} ({p['shape']}) "
                        f"[{p['meshes']} mesh(es), {p['verts']} verts]"
                    )
                if self._preview_unmatched:
                    imgui.spacing()
                    imgui.text_colored(imgui.ImVec4(0.7, 0.7, 0.3, 1.0), "Unmatched nodes:")
                    for name in self._preview_unmatched:
                        imgui.bullet_text(name)
            elif self._preview_parts is not None and len(self._preview_parts) == 0 and self._input_path:
                imgui.text_disabled("No parts detected. Check mappings and grouping mode.")

        imgui.spacing()
        imgui.separator()

        # --- Run/Cancel ---
        can_run = bool(self._input_path) and bool(self._mappings)
        run_clicked, cancel_clicked = draw_run_cancel_buttons(self._running, can_run)
        if run_clicked:
            self._start_batch(self._run_batch)
        if cancel_clicked:
            self._cancel_requested = True

    def _run_preview(self) -> None:
        """Preview part detection on a single NIF without modifying it."""
        self._preview_parts = []
        self._preview_unmatched = []

        path = self._input_path
        if not path or not os.path.isfile(path):
            return

        try:
            from creation_lib.nif.nif_file import NifFile
            from creation_lib.nif.io import read_nif
            from creation_lib.nif.operations.collision_parts import identify_parts

            nif = read_nif(path)
            mappings = self._get_part_mappings()
            group_by = "node" if self._group_by_node else "shape"
            parts = identify_parts(nif, 0, mappings, group_by=group_by)

            matched_ids = set()
            for p in parts:
                self._preview_parts.append({
                    "name": p.node_name,
                    "pattern": p.matched_pattern,
                    "shape": p.shape_type,
                    "meshes": len(p.mesh_block_ids),
                    "verts": p.vertex_count,
                })
                matched_ids.add(p.node_block_id)

            # Show unmatched nodes
            for block in nif.blocks:
                if block.block_id in matched_ids:
                    continue
                if block.type_name in ("NiNode", "BSFadeNode", "BSLeafAnimNode"):
                    name = block.get_field("Name") or "(unnamed)"
                    self._preview_unmatched.append(f"[{block.type_name}] {name}")

        except Exception as e:
            self._error_msg = f"Preview failed: {e}"
            _log.exception("Preview failed")

    def _collect_nif_files(self) -> list[str]:
        """Collect .nif files from input path."""
        path = self._input_path
        if os.path.isfile(path):
            return [path] if path.lower().endswith(".nif") else []

        if os.path.isdir(path):
            files = []
            if self._include_subdirs:
                for dirpath, _, filenames in os.walk(path):
                    for f in filenames:
                        if f.lower().endswith(".nif"):
                            files.append(os.path.join(dirpath, f))
            else:
                for f in os.listdir(path):
                    if f.lower().endswith(".nif"):
                        files.append(os.path.join(path, f))
            return sorted(files)
        return []

    def _run_batch(self) -> None:
        """Background worker: generate collision for all matching NIFs."""
        from creation_lib.nif.io import read_nif, write_nif
        from creation_lib.nif.operations.collision_parts import identify_parts, generate_per_part_collision

        files = self._collect_nif_files()
        if not files:
            self._error_msg = "No .nif files found"
            return

        mappings = self._get_part_mappings()
        group_by = "node" if self._group_by_node else "shape"
        layer = LAYER_NAMES[self._layer_idx]

        processed = 0
        skipped = 0
        errors = 0

        for i, fpath in enumerate(files):
            if self._cancel_requested:
                break

            self._on_progress(i, len(files), os.path.basename(fpath))

            try:
                nif = read_nif(fpath)
                parts = identify_parts(nif, 0, mappings, group_by=group_by)

                if not parts:
                    skipped += 1
                    continue

                result = generate_per_part_collision(
                    nif, 0, parts,
                    layer=layer,
                    mass=self._mass,
                    friction=self._friction,
                    restitution=self._restitution,
                    radius=self._radius,
                    replace=self._replace_existing,
                )

                if not result.success:
                    _log.warning("Collision gen failed for %s: %s", fpath, result.description)
                    errors += 1
                    continue

                # Determine output path
                if self._overwrite:
                    out_path = fpath
                elif self._output_dir:
                    rel = os.path.relpath(fpath, os.path.dirname(self._input_path))
                    out_path = os.path.join(self._output_dir, rel)
                    os.makedirs(os.path.dirname(out_path), exist_ok=True)
                else:
                    base, ext = os.path.splitext(fpath)
                    out_path = f"{base}_collision{ext}"

                write_nif(nif, out_path)
                processed += 1

            except Exception as e:
                _log.exception("Error processing %s", fpath)
                errors += 1

        self._on_progress(len(files), len(files), "Done")
        self._result_msg = (
            f"Processed {processed} file(s), skipped {skipped}, errors {errors} "
            f"(out of {len(files)} total)"
        )
