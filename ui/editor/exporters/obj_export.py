"""OBJ import/export for BSTriShape blocks.

Export: writes vertices, normals, UVs, and face indices to .obj format.
Import: reads .obj vertex data back into an existing BSTriShape.
"""

import logging

from creation_lib.geometry.obj_import import load_obj_geometry

_log = logging.getLogger("nif_editor.obj_export")


def export_shape_to_obj(nif, block_id: int, filepath: str) -> int:
    """Export a BSTriShape to .OBJ file.

    Returns the number of triangles written, or -1 on error.
    """
    block = nif.get_block(block_id)
    if not block:
        _log.error("Block %d not found", block_id)
        return -1

    vertex_data = block.get_field("Vertex Data") or []
    triangles = block.get_field("Triangles") or []
    if not vertex_data:
        _log.error("Block %d has no vertex data", block_id)
        return -1

    name = block.get_field("Name") or f"Shape_{block_id}"
    if isinstance(name, list):
        name = "".join(str(c) for c in name)

    with open(filepath, "w") as f:
        f.write("# Exported from NIF Editor\n")
        f.write(f"# Shape: {name} (block {block_id})\n")
        f.write(f"o {name}\n\n")

        # Vertices
        for vd in vertex_data:
            v = vd.get("Vertex", {})
            x = float(v.get("x", 0))
            y = float(v.get("y", 0))
            z = float(v.get("z", 0))
            f.write(f"v {x:.6f} {y:.6f} {z:.6f}\n")

        f.write("\n")

        # Normals
        has_normals = False
        for vd in vertex_data:
            n = vd.get("Normal")
            if n:
                has_normals = True
                nx = float(n.get("x", 0))
                ny = float(n.get("y", 0))
                nz = float(n.get("z", 0))
                f.write(f"vn {nx:.6f} {ny:.6f} {nz:.6f}\n")
            else:
                f.write("vn 0.000000 0.000000 1.000000\n")

        f.write("\n")

        # UVs
        has_uvs = False
        for vd in vertex_data:
            uv = vd.get("UV")
            if uv:
                has_uvs = True
                u = float(uv.get("u", 0))
                v_coord = float(uv.get("v", 0))
                f.write(f"vt {u:.6f} {1.0 - v_coord:.6f}\n")  # Flip V for OBJ convention
            else:
                f.write("vt 0.000000 0.000000\n")

        f.write("\n")

        # Faces (OBJ uses 1-based indices)
        for t in triangles:
            v1 = int(t.get("v1", 0)) + 1
            v2 = int(t.get("v2", 0)) + 1
            v3 = int(t.get("v3", 0)) + 1
            if has_uvs and has_normals:
                f.write(f"f {v1}/{v1}/{v1} {v2}/{v2}/{v2} {v3}/{v3}/{v3}\n")
            elif has_normals:
                f.write(f"f {v1}//{v1} {v2}//{v2} {v3}//{v3}\n")
            else:
                f.write(f"f {v1} {v2} {v3}\n")

    _log.info("Exported %d verts, %d tris to %s", len(vertex_data), len(triangles), filepath)
    return len(triangles)


def import_obj_to_shape(nif, block_id: int, filepath: str) -> int:
    """Import .OBJ vertex data into an existing BSTriShape.

    Replaces vertices, normals, UVs, and triangles.
    Returns number of vertices imported, or -1 on error.
    """
    block = nif.get_block(block_id)
    if not block:
        _log.error("Block %d not found", block_id)
        return -1

    try:
        geometry = load_obj_geometry(filepath, flip_v=True)
    except (OSError, ValueError) as exc:
        _log.error("Failed to import OBJ %s: %s", filepath, exc)
        return -1

    vertex_data = []
    for i, (x, y, z) in enumerate(geometry.vertices):
        vd = {"Vertex": {"x": x, "y": y, "z": z}}
        if geometry.has_normals:
            nx, ny, nz = geometry.normals[i]
            vd["Normal"] = {"x": nx, "y": ny, "z": nz}
        if geometry.has_uvs:
            u, v = geometry.uvs[i]
            vd["UV"] = {"u": u, "v": v}
        vertex_data.append(vd)

    triangles = [
        {"v1": v1, "v2": v2, "v3": v3}
        for v1, v2, v3 in geometry.triangles
    ]

    block.set_field("Vertex Data", vertex_data)
    block.set_field("Num Vertices", len(vertex_data))
    block.set_field("Triangles", triangles)
    block.set_field("Num Triangles", len(triangles))

    _log.info("Imported %d verts, %d tris from %s", len(vertex_data), len(triangles), filepath)
    return len(vertex_data)
