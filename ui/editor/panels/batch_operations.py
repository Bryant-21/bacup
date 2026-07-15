"""Batch operations panel — NIF "spells" for common mesh operations.

Provides one-click batch operations on the loaded NIF file:
fix normals, strip unused blocks, rename shapes, mirror mesh,
set two-sided, remove alpha properties.
"""

from imgui_bundle import imgui
import numpy as np

from creation_lib.nif.nif_file import NifFile


class BatchOperationsPanel:
    """imgui panel with batch NIF operations (spells)."""

    def __init__(self, app):
        self.app = app
        self._visible = False
        self.window_name = "Batch Operations"
        self._rename_prefix = ""
        self._collision_type = 1
        self._collision_layer = 0
        self._include_child_nodes = True
        self._log: list[str] = []

    def _log_msg(self, msg: str):
        self._log.append(msg)
        if len(self._log) > 30:
            self._log = self._log[-30:]

    def _reload_scene(self):
        """Rebuild 3D view from in-memory NIF (no disk round-trip)."""
        if self.app.nif_file:
            try:
                self.app.rebuild_scene_from_nif()
            except Exception as e:
                self._log_msg(f"Reload error: {e}")

    def draw(self):
        """Draw the batch operations panel."""
        if not self._visible:
            return

        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return

        nif = self.app.nif_file
        if not nif:
            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), "No NIF loaded")
            imgui.end()
            return

        imgui.text("Spells")
        imgui.separator()

        # Fix Normals
        if imgui.button("Fix Normals", imgui.ImVec2(180, 0)):
            self._fix_normals(nif)

        imgui.same_line()
        if imgui.button("Strip Unused", imgui.ImVec2(180, 0)):
            self._strip_unused(nif)

        # Set All Two-Sided
        if imgui.button("Set All Two-Sided", imgui.ImVec2(180, 0)):
            self._set_two_sided(nif)

        imgui.same_line()
        if imgui.button("Remove Alpha", imgui.ImVec2(180, 0)):
            self._remove_alpha(nif)

        # Mirror Mesh (X)
        if imgui.button("Mirror Mesh (X)", imgui.ImVec2(180, 0)):
            self._mirror_x(nif)

        imgui.same_line()
        if imgui.button("Flip Faces", imgui.ImVec2(180, 0)):
            self._flip_faces(nif)

        if imgui.button("Flip Normals", imgui.ImVec2(180, 0)):
            self._flip_normals(nif)

        imgui.same_line()
        if imgui.button("Normalize Normals", imgui.ImVec2(180, 0)):
            self._normalize_normals(nif)

        if imgui.button("Update All Bounds", imgui.ImVec2(180, 0)):
            self._update_bounds(nif)

        imgui.same_line()
        if imgui.button("Prune Degen Tris", imgui.ImVec2(180, 0)):
            self._prune_degenerate_tris(nif)

        if imgui.button("Remove Dup Verts", imgui.ImVec2(180, 0)):
            self._remove_duplicate_vertices(nif)

        imgui.same_line()
        if imgui.button("Flip UVs (V)", imgui.ImVec2(180, 0)):
            self._flip_uvs_v(nif)

        imgui.separator()

        # Optimization spells
        imgui.text("Optimization")
        imgui.separator()

        if imgui.button("Combine Dup Props", imgui.ImVec2(180, 0)):
            self._combine_duplicate_properties(nif)

        imgui.same_line()
        if imgui.button("Remove Bogus Nodes", imgui.ImVec2(180, 0)):
            self._remove_bogus_nodes(nif)

        if imgui.button("Sanitize Links", imgui.ImVec2(180, 0)):
            self._sanitize_links(nif)

        imgui.separator()

        # Rename Shapes — needs prefix input
        imgui.text("Rename Shapes:")
        imgui.push_item_width(130)
        _, self._rename_prefix = imgui.input_text(
            "Prefix##rename", self._rename_prefix
        )
        imgui.pop_item_width()
        imgui.same_line()
        if imgui.button("Apply##rename"):
            self._rename_shapes(nif)

        imgui.separator()

        # Collision
        imgui.text("Collision")
        imgui.separator()

        imgui.push_item_width(130)
        _, self._collision_type = imgui.combo(
            "Shape##coll", self._collision_type,
            # TODO: MOPP for legacy engines
            ["Convex Hull", "Auto Convex Hull", "Box", "Sphere", "Capsule", "Cylinder",
             "Auto Best-Fit", "Compound (List)", "Auto Compressed Mesh", "Compressed Mesh"]
        )
        imgui.pop_item_width()

        imgui.push_item_width(130)
        _, self._collision_layer = imgui.combo(
            "Layer##coll", self._collision_layer,
            ["STATIC", "ANIMSTATIC", "CLUTTER", "WEAPON", "PROJECTILE", "TERRAIN"]
        )
        imgui.pop_item_width()

        _, self._include_child_nodes = imgui.checkbox(
            "Use child NiNode meshes##batch_coll",
            self._include_child_nodes,
        )

        if imgui.button("Generate Collision", imgui.ImVec2(180, 0)):
            self._generate_collision(nif)

        imgui.same_line()
        if imgui.button("Remove Collision", imgui.ImVec2(180, 0)):
            self._remove_collision(nif)

        if hasattr(self.app, 'renderer') and self.app.renderer:
            _, self.app.renderer._show_collision = imgui.checkbox(
                "Show Collision", self.app.renderer._show_collision
            )

        imgui.separator()

        # Log
        if self._log:
            imgui.begin_child("batch_log", imgui.ImVec2(0, 0), imgui.ChildFlags_.borders.value)
            for entry in self._log:
                imgui.text_wrapped(entry)
            if imgui.get_scroll_y() >= imgui.get_scroll_max_y() - 10:
                imgui.set_scroll_here_y(1.0)
            imgui.end_child()

        imgui.end()

    def _fix_normals(self, nif: NifFile):
        """Recompute smooth normals for all BSTriShape blocks."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0

        for shape in shapes:
            vertex_data = shape.get_field("Vertex Data") or []
            triangles = shape.get_field("Triangles") or []
            if not vertex_data or not triangles:
                continue

            n_verts = len(vertex_data)
            verts = np.zeros((n_verts, 3), dtype=np.float32)
            for i, vd in enumerate(vertex_data):
                v = vd.get("Vertex", {})
                verts[i] = [
                    float(v.get("x", 0)),
                    float(v.get("y", 0)),
                    float(v.get("z", 0)),
                ]

            tris = np.array(
                [
                    [int(t.get("v1", 0)), int(t.get("v2", 0)), int(t.get("v3", 0))]
                    for t in triangles
                ],
                dtype=np.uint32,
            )

            # Compute smooth normals (area-weighted)
            normals = np.zeros_like(verts)
            if len(tris) > 0:
                v0, v1, v2 = verts[tris[:, 0]], verts[tris[:, 1]], verts[tris[:, 2]]
                face_normals = np.cross(v1 - v0, v2 - v0)
                for col in range(3):
                    np.add.at(normals, tris[:, col], face_normals)
                lengths = np.linalg.norm(normals, axis=1, keepdims=True)
                lengths[lengths < 1e-8] = 1.0
                normals = normals / lengths

            # Write back
            for i, vd in enumerate(vertex_data):
                vd["Normal"] = {
                    "x": float(normals[i, 0]),
                    "y": float(normals[i, 1]),
                    "z": float(normals[i, 2]),
                }

            shape.set_field("Vertex Data", vertex_data)
            count += 1

        self._log_msg(f"Fixed normals on {count} shape(s)")
        self._reload_scene()

    def _strip_unused(self, nif: NifFile):
        """Remove blocks not referenced by any other block (except root)."""
        from ui.editor.mcp_client import get_nif_ops

        ops = get_nif_ops()
        unreferenced = ops.find_unreferenced_blocks(nif)
        if not unreferenced:
            self._log_msg("No unused blocks found")
            return

        nif.remove_blocks(unreferenced)
        self._log_msg(f"Stripped {len(unreferenced)} unused block(s)")
        self._reload_scene()

    def _rename_shapes(self, nif: NifFile):
        """Prefix all shape names with the given string."""
        prefix = self._rename_prefix.strip()
        if not prefix:
            self._log_msg("Enter a prefix first")
            return

        shapes = nif.find_blocks("BSTriShape")
        count = 0
        for shape in shapes:
            old_name = shape.get_field("Name") or ""
            if isinstance(old_name, list):
                old_name = "".join(str(c) for c in old_name)
            if not old_name.startswith(prefix):
                shape.set_field("Name", f"{prefix}{old_name}")
                count += 1

        self._log_msg(f"Renamed {count} shape(s) with prefix '{prefix}'")
        self._reload_scene()

    def _mirror_x(self, nif: NifFile):
        """Mirror all shape geometry across the X axis."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0

        for shape in shapes:
            vertex_data = shape.get_field("Vertex Data") or []
            triangles = shape.get_field("Triangles") or []
            if not vertex_data:
                continue

            # Negate X for vertices and normals
            for vd in vertex_data:
                v = vd.get("Vertex")
                if v:
                    v["x"] = -float(v.get("x", 0))
                n = vd.get("Normal")
                if n:
                    n["x"] = -float(n.get("x", 0))

            # Flip triangle winding order to maintain correct face orientation
            for t in triangles:
                v1, v2 = t.get("v1", 0), t.get("v2", 0)
                t["v1"] = v2
                t["v2"] = v1

            shape.set_field("Vertex Data", vertex_data)
            shape.set_field("Triangles", triangles)
            count += 1

        self._log_msg(f"Mirrored {count} shape(s) across X axis")
        self._reload_scene()

    def _set_two_sided(self, nif: NifFile):
        """Enable double-sided rendering on all BSLightingShaderProperty blocks."""
        shaders = nif.find_blocks("BSLightingShaderProperty")
        count = 0

        for shader in shaders:
            flags1 = shader.get_field("Shader Flags 1") or 0
            # Bit 12 (0x1000) = SLSF1_Two_Sided in FO4
            if not (flags1 & 0x1000):
                flags1 |= 0x1000
                shader.set_field("Shader Flags 1", flags1)
                count += 1

        if count == 0:
            # Also try BSEffectShaderProperty
            effects = nif.find_blocks("BSEffectShaderProperty")
            for effect in effects:
                flags1 = effect.get_field("Shader Flags 1") or 0
                if not (flags1 & 0x1000):
                    flags1 |= 0x1000
                    effect.set_field("Shader Flags 1", flags1)
                    count += 1

        self._log_msg(f"Set two-sided on {count} shader(s)")
        self._reload_scene()

    def _remove_alpha(self, nif: NifFile):
        """Remove NiAlphaProperty references from all shapes."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0

        for shape in shapes:
            alpha_ref = shape.get_field("Alpha Property")
            if alpha_ref is not None:
                ref_id = alpha_ref
                if isinstance(ref_id, dict):
                    ref_id = ref_id.get("value", ref_id.get("Value", -1))
                if isinstance(ref_id, (int, float)) and int(ref_id) >= 0:
                    shape.set_field("Alpha Property", -1)
                    count += 1

        # Collect and remove orphaned NiAlphaProperty blocks
        if count > 0:
            from ui.editor.mcp_client import get_nif_ops

            ops = get_nif_ops()
            unused = ops.find_unreferenced_blocks(nif)
            alpha_blocks = [
                bid for bid in unused
                if nif.get_block(bid)
                and nif.get_block(bid).type_name == "NiAlphaProperty"
            ]
            if alpha_blocks:
                nif.remove_blocks(alpha_blocks)

        self._log_msg(f"Removed alpha from {count} shape(s)")
        self._reload_scene()

    # -------------------------------------------------------------------
    # Expanded Mesh Operations
    # -------------------------------------------------------------------

    def _flip_faces(self, nif: NifFile):
        """Reverse triangle winding order on all shapes."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0
        for shape in shapes:
            triangles = shape.get_field("Triangles") or []
            if not triangles:
                continue
            for t in triangles:
                v1, v2 = t.get("v1", 0), t.get("v2", 0)
                t["v1"] = v2
                t["v2"] = v1
            shape.set_field("Triangles", triangles)
            count += 1
        self._log_msg(f"Flipped faces on {count} shape(s)")
        self._reload_scene()

    def _flip_normals(self, nif: NifFile):
        """Negate all vertex normals."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0
        for shape in shapes:
            vertex_data = shape.get_field("Vertex Data") or []
            if not vertex_data:
                continue
            for vd in vertex_data:
                n = vd.get("Normal")
                if n:
                    n["x"] = -float(n.get("x", 0))
                    n["y"] = -float(n.get("y", 0))
                    n["z"] = -float(n.get("z", 0))
            shape.set_field("Vertex Data", vertex_data)
            count += 1
        self._log_msg(f"Flipped normals on {count} shape(s)")
        self._reload_scene()

    def _normalize_normals(self, nif: NifFile):
        """Normalize all vertex normals to unit length."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0
        for shape in shapes:
            vertex_data = shape.get_field("Vertex Data") or []
            if not vertex_data:
                continue
            fixed = 0
            for vd in vertex_data:
                n = vd.get("Normal")
                if not n:
                    continue
                nx, ny, nz = float(n.get("x", 0)), float(n.get("y", 0)), float(n.get("z", 0))
                length = (nx * nx + ny * ny + nz * nz) ** 0.5
                if length > 1e-8 and abs(length - 1.0) > 1e-4:
                    n["x"] = nx / length
                    n["y"] = ny / length
                    n["z"] = nz / length
                    fixed += 1
            if fixed:
                shape.set_field("Vertex Data", vertex_data)
                count += 1
        self._log_msg(f"Normalized normals on {count} shape(s)")
        self._reload_scene()

    def _update_bounds(self, nif: NifFile):
        """Recompute bounding sphere from vertex positions."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0
        for shape in shapes:
            vertex_data = shape.get_field("Vertex Data") or []
            if not vertex_data:
                continue

            # Compute center
            xs, ys, zs = [], [], []
            for vd in vertex_data:
                v = vd.get("Vertex", {})
                xs.append(float(v.get("x", 0)))
                ys.append(float(v.get("y", 0)))
                zs.append(float(v.get("z", 0)))

            cx = sum(xs) / len(xs)
            cy = sum(ys) / len(ys)
            cz = sum(zs) / len(zs)

            # Compute radius
            radius = 0.0
            for x, y, z in zip(xs, ys, zs):
                d = ((x - cx) ** 2 + (y - cy) ** 2 + (z - cz) ** 2) ** 0.5
                radius = max(radius, d)

            shape.set_field("Center", {"x": cx, "y": cy, "z": cz})
            shape.set_field("Radius", radius)
            count += 1
        self._log_msg(f"Updated bounds on {count} shape(s)")

    def _prune_degenerate_tris(self, nif: NifFile):
        """Remove zero-area and duplicate-index triangles."""
        shapes = nif.find_blocks("BSTriShape")
        total_removed = 0
        for shape in shapes:
            triangles = shape.get_field("Triangles") or []
            if not triangles:
                continue
            clean = []
            for t in triangles:
                v1, v2, v3 = int(t.get("v1", 0)), int(t.get("v2", 0)), int(t.get("v3", 0))
                if v1 != v2 and v2 != v3 and v1 != v3:
                    clean.append(t)
            removed = len(triangles) - len(clean)
            if removed:
                shape.set_field("Triangles", clean)
                shape.set_field("Num Triangles", len(clean))
                total_removed += removed
        self._log_msg(f"Pruned {total_removed} degenerate triangle(s)")
        if total_removed:
            self._reload_scene()

    def _remove_duplicate_vertices(self, nif: NifFile):
        """Merge vertices with identical positions, remapping triangle indices."""
        shapes = nif.find_blocks("BSTriShape")
        total_removed = 0

        for shape in shapes:
            vertex_data = shape.get_field("Vertex Data") or []
            triangles = shape.get_field("Triangles") or []
            if not vertex_data or not triangles:
                continue

            # Hash vertex positions
            pos_map = {}  # (x,y,z) -> first index
            remap = {}    # old index -> canonical index
            for i, vd in enumerate(vertex_data):
                v = vd.get("Vertex", {})
                key = (
                    round(float(v.get("x", 0)), 5),
                    round(float(v.get("y", 0)), 5),
                    round(float(v.get("z", 0)), 5),
                )
                if key in pos_map:
                    remap[i] = pos_map[key]
                else:
                    pos_map[key] = i
                    remap[i] = i

            dupes = sum(1 for old, new in remap.items() if old != new)
            if not dupes:
                continue

            # Remap triangles
            for t in triangles:
                t["v1"] = remap.get(int(t["v1"]), int(t["v1"]))
                t["v2"] = remap.get(int(t["v2"]), int(t["v2"]))
                t["v3"] = remap.get(int(t["v3"]), int(t["v3"]))

            shape.set_field("Triangles", triangles)
            total_removed += dupes

        self._log_msg(f"Found {total_removed} duplicate vertex position(s)")
        if total_removed:
            self._reload_scene()

    def _flip_uvs_v(self, nif: NifFile):
        """Flip UV V coordinate (1-v) on all shapes."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0
        for shape in shapes:
            vertex_data = shape.get_field("Vertex Data") or []
            if not vertex_data:
                continue
            for vd in vertex_data:
                uv = vd.get("UV")
                if uv:
                    uv["v"] = 1.0 - float(uv.get("v", 0))
            shape.set_field("Vertex Data", vertex_data)
            count += 1
        self._log_msg(f"Flipped UV V on {count} shape(s)")
        self._reload_scene()

    # -------------------------------------------------------------------
    # Optimization Spells
    # -------------------------------------------------------------------

    def _combine_duplicate_properties(self, nif: NifFile):
        """Merge identical shader property blocks."""
        shaders = nif.find_blocks("BSLightingShaderProperty")
        if len(shaders) < 2:
            self._log_msg("Nothing to combine")
            return

        # Group by field content hash
        groups = {}
        for shader in shaders:
            key = str(sorted(shader.fields))
            if key not in groups:
                groups[key] = []
            groups[key].append(shader.block_id)

        merged = 0
        for key, ids in groups.items():
            if len(ids) < 2:
                continue
            canonical = ids[0]
            for dup_id in ids[1:]:
                # Remap all references to dup_id → canonical
                for block in nif.blocks:
                    for i, (name, value) in enumerate(block.fields):
                        if isinstance(value, int) and value == dup_id:
                            block.set_field(name, canonical)
                merged += 1

        self._log_msg(f"Merged {merged} duplicate shader properties")
        if merged:
            self._reload_scene()

    def _remove_bogus_nodes(self, nif: NifFile):
        """Remove empty NiNodes with no children and no extra data."""
        to_remove = []
        for block in nif.blocks:
            if block.block_id == 0:  # Never remove root
                continue
            if not nif.schema.is_subtype_of(block.type_name, "NiNode"):
                continue
            children = block.get_field("Children") or []
            extra = block.get_field("Extra Data List") or []
            # Check for valid children
            valid_children = [r for r in children if _get_ref_id(r) >= 0]
            valid_extra = [r for r in extra if _get_ref_id(r) >= 0]
            if not valid_children and not valid_extra:
                to_remove.append(block.block_id)

        if to_remove:
            nif.remove_blocks(to_remove)
            self._log_msg(f"Removed {len(to_remove)} empty node(s)")
            self._reload_scene()
        else:
            self._log_msg("No bogus nodes found")

    def _sanitize_links(self, nif: NifFile):
        """Set invalid Ref/Ptr values to -1."""
        from creation_lib.nif.schema import build_field_def_map
        num_blocks = len(nif.blocks)
        fixed = 0
        schema = nif.schema

        for block in nif.blocks:
            fdef_map = build_field_def_map(schema, block.type_name)

            for name, value in block.fields:
                fdef = fdef_map.get(name)
                if fdef and fdef.type in ("Ref", "Ptr"):
                    if isinstance(value, int) and value >= num_blocks:
                        block.set_field(name, -1)
                        fixed += 1

        self._log_msg(f"Sanitized {fixed} invalid link(s)")

    def _game_profile(self):
        try:
            session = self.app.registry.active_session
            return getattr(session, "game_profile", None)
        except (AttributeError, KeyError):
            return None

    def _generate_collision(self, nif: NifFile):
        """Generate collision on root node."""
        from creation_lib.nif.operations.collision import generate_collision

        type_map = {
            0: "convex_hull", 1: "convex_fit", 2: "box", 3: "sphere", 4: "capsule",
            5: "cylinder", 6: "auto", 7: "list", 8: "auto_compressed_mesh",
            9: "compressed_mesh",
        }
        layer_map = {0: "STATIC", 1: "ANIMSTATIC", 2: "CLUTTER", 3: "WEAPON", 4: "PROJECTILE", 5: "TERRAIN"}

        shape_type = type_map.get(self._collision_type, "convex_hull")
        layer = layer_map.get(self._collision_layer, "STATIC")
        profile = self._game_profile()
        if profile is None:
            self._log_msg("Error: no game profile resolved for collision generation")
            return

        result = generate_collision(
            nif,
            node_block_id=0,
            shape_type=shape_type,
            layer=layer,
            profile=profile,
            include_child_nodes=self._include_child_nodes,
        )
        if result.success:
            self._log_msg(result.description)
            self._reload_scene()
            # Auto-enable collision overlay so user sees what was generated
            if hasattr(self.app, 'renderer') and self.app.renderer:
                self.app.renderer._show_collision = True
                self.app.renderer._collision_dirty = True
        else:
            self._log_msg(f"Error: {result.description}")
            if result.warnings:
                for w in result.warnings:
                    self._log_msg(f"  Warning: {w}")

    def _remove_collision(self, nif: NifFile):
        """Remove collision from root node."""
        from creation_lib.nif.operations.collision import remove_collision

        result = remove_collision(nif, node_block_id=0)
        if result.success:
            self._log_msg(result.description)
            self._reload_scene()
        else:
            self._log_msg(f"Error: {result.description}")


def _get_ref_id(ref) -> int:
    if isinstance(ref, (int, float)):
        return int(ref)
    if isinstance(ref, dict):
        return int(ref.get("value", ref.get("Value", -1)))
    return -1
