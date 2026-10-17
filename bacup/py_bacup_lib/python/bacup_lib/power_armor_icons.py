from __future__ import annotations

import hashlib
import json
import logging
from pathlib import Path

import numpy as np

MODELS = Path(__file__).with_name("data") / "power_armor_icon_models.json"
CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/power_armor_icons.json")
SIZE = 128
# Tales' kLineAlpha: detail lines are cut into an opaque silhouette rather than drawn over a faint fill.
LINE_ALPHA = 40
_log = logging.getLogger(__name__)


def model_triangles(path: Path) -> np.ndarray:
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile.load(str(path))
    result = []
    visited = set()

    def visit(block_id, parent):
        if block_id in visited or block_id < 0:
            return
        visited.add(block_id)
        block = nif.get_block(block_id)
        rotation = block.get_field("Rotation")
        transform = np.eye(4)
        if rotation:
            # NiMatrix3::LoadBinary keeps the named matrix and FO4 rotates row vectors (p·M), so a column-vector
            # transform is its transpose. The untransposed reading turned rotated parts (T-60 shoulder) backwards.
            transform[:3, :3] = [[rotation[f"m{col}{row}"] for col in range(1, 4)] for row in range(1, 4)]
        transform[:3, :3] *= block.get_field("Scale") or 1
        translation = block.get_field("Translation") or {}
        transform[:3, 3] = [translation.get(axis, 0) for axis in "xyz"]
        transform = parent @ transform
        vertices = block.get_field("Vertex Data") or []
        faces = block.get_field("Triangles") or []
        if vertices and faces:
            points = np.array([[v["Vertex"][axis] for axis in "xyz"] + [1] for v in vertices])
            points = (points @ transform.T)[:, :3]
            indices = np.array([[f[axis] for axis in ("v1", "v2", "v3")] for f in faces])
            result.extend(points[indices])
        for child in block.get_field("Children") or []:
            visit(child, transform)

    visit(0, np.eye(4))
    return np.asarray(result)


def turned(triangles: np.ndarray, degrees: float) -> np.ndarray:
    angle = np.radians(degrees)
    rotation = np.array([[np.cos(angle), -np.sin(angle), 0], [np.sin(angle), np.cos(angle), 0], [0, 0, 1]])
    return triangles @ rotation.T


def pitched(triangles: np.ndarray, degrees: float) -> np.ndarray:
    angle = np.radians(degrees)
    rotation = np.array([[1, 0, 0], [0, np.cos(angle), -np.sin(angle)], [0, np.sin(angle), np.cos(angle)]])
    return triangles @ rotation.T


def facing_front(triangles: np.ndarray) -> np.ndarray:
    """A camera in front of a piece (+y) sees the wearer's right (+x) on its left; front_view draws +x rightward,
    which drew every left piece as a right one."""
    return triangles * (-1, 1, 1)


def front_view(triangles: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    from bacup_lib.native_runtime import load_native_module

    size = SIZE * 2
    covered, depth, normals = load_native_module().render_icon_front(np.asarray(triangles, dtype="<f8").tobytes())
    return (np.frombuffer(covered, dtype=np.bool_).reshape(size, size),
            np.frombuffer(depth, dtype="<f8").reshape(size, size),
            np.frombuffer(normals, dtype="<f8").reshape(size, size, 3))


def _halved(alpha: np.ndarray) -> np.ndarray:
    return np.rint(alpha.reshape(SIZE, 2, SIZE, 2).mean(axis=(1, 3))).astype(np.uint8)


def front_icon(triangles: np.ndarray) -> np.ndarray:
    """The Favorites wheel's detailed icon: an opaque silhouette with creases and steps cut in as lines."""
    return _front_icon(*front_view(triangles))


def _front_icon(covered: np.ndarray, depth: np.ndarray, normals: np.ndarray) -> np.ndarray:
    lines = np.zeros_like(covered)
    for dx, dy in ((2, 0), (-2, 0), (0, 2), (0, -2)):
        neighbor = np.roll(covered, (-dy, -dx), axis=(0, 1))
        normal = np.roll(normals, (-dy, -dx), axis=(0, 1))
        difference = np.roll(depth, (-dy, -dx), axis=(0, 1)) - depth
        planes = (np.abs(difference*normals[..., 2] + dx*normals[..., 0] + dy*normals[..., 1]) > 4) & \
                 (np.abs(difference*normal[..., 2] + dx*normal[..., 0] + dy*normal[..., 1]) > 4)
        lines |= neighbor & (((normals*normal).sum(axis=2) < .85) | planes)
    return _halved(np.where(covered, np.where(lines, LINE_ALPHA, 255), 0))


def _shifted(mask: np.ndarray, dy: int, dx: int, fill=False) -> np.ndarray:
    result = np.full_like(mask, fill)
    h, w = mask.shape
    result[max(dy, 0):h+min(dy, 0), max(dx, 0):w+min(dx, 0)] = mask[max(-dy, 0):h-max(dy, 0), max(-dx, 0):w-max(dx, 0)]
    return result


def _dilated(mask: np.ndarray, radius: int) -> np.ndarray:
    result = mask.copy()
    for dy in range(-radius, radius+1):
        for dx in range(-radius, radius+1):
            if dx*dx + dy*dy <= radius*radius:
                result |= _shifted(mask, dy, dx)
    return result


def _eroded(mask: np.ndarray, radius: int) -> np.ndarray:
    return ~_dilated(~mask, radius)


def _blurred(mask: np.ndarray, sigma: float) -> np.ndarray:
    reach = int(3*sigma)
    weights = np.exp(-np.arange(-reach, reach+1)**2 / (2*sigma*sigma))
    weights /= weights.sum()
    result = mask.astype(float)
    for axis in (0, 1):
        padded = np.pad(result, [(reach, reach) if a == axis else (0, 0) for a in (0, 1)], mode="edge")
        result = sum(w*np.take(padded, range(i, i+mask.shape[axis]), axis=axis) for i, w in enumerate(weights))
    return result


def _labels(mask: np.ndarray) -> np.ndarray:
    """4-connected component labels (the smallest pixel index in each); background is -1."""
    h, w = mask.shape
    empty = h*w
    labels = np.where(mask, np.arange(empty).reshape(h, w), empty)
    while True:
        smallest = labels
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            smallest = np.minimum(smallest, _shifted(labels, dy, dx, empty))
        flat = np.where(mask, smallest, empty).ravel()
        inside = flat < empty
        flat[inside] = flat[flat[inside]]
        updated = flat.reshape(h, w)
        if np.array_equal(updated, labels):
            return np.where(mask, labels, -1)
        labels = updated


def _without_small(mask: np.ndarray, area: int, keep_border: bool) -> np.ndarray:
    labels = _labels(mask)
    names, counts = np.unique(labels[mask], return_counts=True)
    border = set(np.unique(np.r_[labels[0], labels[-1], labels[:, 0], labels[:, -1]])) if keep_border else set()
    small = [name for name, count in zip(names, counts) if count < area and name not in border]
    return mask & ~np.isin(labels, small)


def _polygon(outline: np.ndarray, shape: tuple[int, int]) -> np.ndarray:
    y, x = np.mgrid[0:shape[0], 0:shape[1]] + .5
    inside = np.zeros(shape, dtype=bool)
    for (x0, y0), (x1, y1) in zip(outline, np.roll(outline, -1, axis=0)):
        if y0 == y1:
            continue
        crossing = (y0 > y) != (y1 > y)
        inside ^= crossing & (x < x0 + (y - y0) * (x1 - x0) / (y1 - y0))
    return inside


def list_icon(triangles: np.ndarray, eyes: list | None = None) -> np.ndarray:
    """Inventory rows: a smoothed solid silhouette, with a helmet's eyes cut out large enough to read at text height.

    Eye outlines are fractions of the drawn bounds, picked once per helmet model in the BACUP table."""
    return _list_icon(front_view(triangles)[0], eyes)


def _list_icon(covered: np.ndarray, eyes: list | None) -> np.ndarray:
    solid = _eroded(_dilated(covered, 2), 2)
    solid |= ~_without_small(~solid, 196, keep_border=True)
    solid = _without_small(_blurred(solid, 1.6) > .5, 100, keep_border=False)
    ys, xs = np.nonzero(covered)
    if eyes and xs.size:
        left, top = xs.min(), ys.min()
        width, height = xs.max() + 1 - left, ys.max() + 1 - top
        holes = np.zeros_like(solid)
        for outline in eyes:
            eye = _polygon(np.array(outline, dtype=float) * (width, height) + (left, top), solid.shape)
            thickness, core = 0, eye
            while core.any():
                thickness += 2
                core = _eroded(core, 1)
            holes |= _dilated(eye, max(0, round((.11 * max(width, height) - thickness) / 2)))
        # A rim keeps the helmet outline closed around visors that reach its edge.
        solid &= ~(holes & _eroded(solid, max(1, round(.05 * height))))
    return _halved(np.where(solid, 255, 0))


def runs(alpha: np.ndarray) -> list[list[int]]:
    result = []
    for y, row in enumerate(alpha):
        changes = np.flatnonzero(np.diff(np.r_[0, row, 0]))
        for x, end in zip(changes, changes[1:]):
            if row[x]:
                result.append([int(x), y, int(end-x), int(row[x])])
    return result


def generate_power_armor_icons(source_root: Path, mod_root: Path, preview_dir: Path | None = None) -> Path:
    icons, missing = [], []
    fronts: dict[tuple[Path, float], np.ndarray] = {}
    for entry in json.loads(MODELS.read_text()):
        name = f"{entry['keyword']}:{entry['piece']}"
        model = source_root / "Meshes" / entry["model"]
        if not model.is_file():
            missing.append(name)
            continue
        # A ground model authored facing sideways names the turn about its vertical axis that faces it forward;
        # one lying down also names the tip about its left-right axis that stands it up.
        view = (model, entry.get("yaw", 0), entry.get("pitch", 0), json.dumps(entry.get("eyes")))
        if view not in fronts:
            triangles = facing_front(pitched(turned(model_triangles(model), view[1]), view[2]))
            covered, depth, normals = front_view(triangles)
            pair = (_front_icon(covered, depth, normals), _list_icon(covered, entry.get("eyes")))
            # Ground models lie on their side; limbs read better standing. Clockwise puts the shoulder or hip end
            # of the table's ground models up.
            if entry["piece"] != "Helmet" and entry["piece"] != "Torso":
                ys, xs = np.nonzero(pair[0])
                if xs.size and np.ptp(xs) > np.ptp(ys):
                    pair = tuple(np.rot90(image, -1) for image in pair)
            fronts[view] = pair
        # Some families ship one ground mesh for both sides; the other side is its mirror image.
        alpha, listed = (np.flip(image, axis=1) for image in fronts[view]) if entry.get("mirror") else fronts[view]
        icon = {key: value for key, value in entry.items() if key != "eyes"}
        icons.append({**icon, "source_sha256": hashlib.sha256(model.read_bytes()).hexdigest(),
                      "runs": runs(alpha), "list_runs": runs(listed)})
        if preview_dir:
            from PIL import Image
            preview_dir.mkdir(parents=True, exist_ok=True)
            image = Image.new("RGBA", (SIZE, SIZE), (255, 255, 255, 0))
            image.putalpha(Image.fromarray(alpha))
            image.save(preview_dir / f"{entry['keyword']}_{entry['piece']}.png")
            image.putalpha(Image.fromarray(listed))
            image.save(preview_dir / f"{entry['keyword']}_{entry['piece']}_list.png")
    output = mod_root / CATALOG
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"width": SIZE, "height": SIZE, "view": "front", "icons": icons, "missing": missing}, separators=(",", ":")))
    _log.info("Generated %d offline power armor pieces; missing %s", len(icons), missing)
    return output


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("mod", type=Path)
    parser.add_argument("--preview-dir", type=Path)
    args = parser.parse_args()
    print(generate_power_armor_icons(args.source, args.mod, args.preview_dir))
