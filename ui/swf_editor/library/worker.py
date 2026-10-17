"""Isolated CPU previews: no GL context, source mutation or AS3 execution."""
from __future__ import annotations

import hashlib
import io
import json
from pathlib import Path
import queue

from PIL import Image

from creation_lib.swf.library_assets import decode_geometry, encoded_geometry, validate_geometry
from ui.swf_editor.document import atomic_write
from ui.swf_editor.library.catalog import Asset, payload

PREVIEW_VERSION = "native-vector-preview-4"


def canonical_asset(asset: Asset):
    blob, svg = payload(asset)
    shape, warning = decode_geometry(blob, svg)
    canonical = json.dumps(encoded_geometry(shape)).encode()
    return shape, canonical, warning, hashlib.sha256(PREVIEW_VERSION.encode() + blob + svg.encode()).hexdigest()


def preview(asset: Asset, size: int, cache_dir: Path):
    shape, _, warning, digest = canonical_asset(asset)
    key = hashlib.sha256(f"{digest}:{size}".encode()).hexdigest()
    path = cache_dir / f"{key}.png"
    if path.is_file():
        try:
            with Image.open(path) as cached:
                if cached.size != (size, size) or cached.mode != "RGBA":
                    raise ValueError("Cached preview dimensions changed")
                pixels = cached.tobytes()
            return pixels, warning, True
        except (OSError, ValueError):
            path.unlink(missing_ok=True)
    from creation_lib.swf import native_runtime
    from creation_lib.swf.parser import SwfDocument, SwfHeader
    from creation_lib.swf.tags import DefineShapeTag, SymbolClassTag, ShowFrameTag, EndTag
    from creation_lib.swf.types import RECT
    from creation_lib.swf.writer import write_swf
    from creation_lib.swf.library_assets import SHAPE_CODES
    shape.shape_id = 1
    document = SwfDocument(header=SwfHeader('FWS', 10, 0, RECT(0, size * 20, 0, size * 20), 24, 1),
                           tags=[DefineShapeTag(shape, SHAPE_CODES[shape.shape_version]),
                                 SymbolClassTag([(1, "B21_LibraryPreview")]), ShowFrameTag(), EndTag()])
    w, h = max((shape.bounds[2] - shape.bounds[0]) / 20, 1), max((shape.bounds[3] - shape.bounds[1]) / 20, 1)
    png = native_runtime.render_symbol_png(write_swf(document), "B21_LibraryPreview", scale=min((size - 8) / w, (size - 8) / h))
    with Image.open(io.BytesIO(png)) as image:
        image = image.convert("RGBA")
        image.thumbnail((size - 8, size - 8), Image.Resampling.LANCZOS)
        tile = Image.new("RGBA", (size, size))
        tile.alpha_composite(image, ((size - image.width) // 2, (size - image.height) // 2))
        pixels = tile.tobytes()
    try:
        cache_dir.mkdir(parents=True, exist_ok=True)
        buffer = io.BytesIO()
        tile.save(buffer, format="PNG")
        atomic_write(path, buffer.getvalue())
    except OSError:
        warning = (warning + " Preview disk cache is unavailable.").strip()
    return pixels, warning, False


def import_file(filename: str, threshold: int):
    path = Path(filename)
    if path.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("Library import supports files up to 64 MB")
    entries, errors, total_bytes = [], [], 0
    if path.suffix.lower() == ".swf":
        from ui.swf_editor.movie import Movie
        from creation_lib.swf.tags import DefineShapeTag
        for tag in Movie(path.read_bytes()).tags:
            if tag.code not in {2, 22, 32, 83}:
                continue
            try:
                shape = DefineShapeTag.parse(tag.payload, tag.code).shape
                validate_geometry(shape)
                geometry = json.dumps(encoded_geometry(shape)).encode()
                total_bytes += len(geometry)
                if total_bytes > 64 * 1024 * 1024:
                    raise ValueError('Copied vector geometry exceeds the 64 MB import limit')
                entries.append((f"{path.stem} #{shape.shape_id}", geometry, path.name, "Imported"))
            except Exception as exc:
                errors.append(f"Character {tag.character_id}: {exc}")
            if total_bytes > 64 * 1024 * 1024:
                raise ValueError('Copied vector geometry exceeds the 64 MB import limit')
            if len(entries) > 4096:
                raise ValueError("Import supports up to 4096 vector definitions per file")
    else:
        from creation_lib.swf.svg_io import svg_to_shapes
        from creation_lib.swf.trace import trace_image_file, TraceSettings
        from creation_lib.swf.project import _merge_shapes
        if path.suffix.lower() == '.svg':
            import xml.etree.ElementTree as ET
            svg = path.read_text(encoding='utf-8-sig')
            for element in ET.fromstring(svg).iter():
                kind = element.tag.rsplit('}', 1)[-1]
                if (kind not in {'svg', 'g', 'path', 'rect', 'circle', 'ellipse', 'title', 'desc'}
                        or any(element.get(a) for a in ('transform', 'style', 'class', 'clip-path', 'mask', 'filter'))
                        or element.get('stroke', 'none') != 'none'
                        or element.get('fill', 'white') not in {'none', 'white', 'black'} and not element.get('fill', '').startswith('#')
                        or kind == 'g' and any(element.get(a) for a in ('fill', 'opacity', 'fill-opacity'))):
                    raise ValueError('SVG library import supports flat filled paths/rectangles/circles/ellipses. Expand transforms, strokes, CSS, text and paints to filled paths first.')
            shapes = svg_to_shapes(svg)
        else:
            shapes = trace_image_file(str(path), TraceSettings(threshold=threshold))
        if shapes:
            shape = _merge_shapes(shapes, 1)
            validate_geometry(shape)
            entries.append((path.stem, json.dumps(encoded_geometry(shape)).encode(), path.name, "Imported"))
    if not entries:
        raise ValueError("No supported vector geometry found. " + "; ".join(errors[:3]))
    return entries, errors


def run(requests, results, stopped, epochs):
    """One bounded helper process; the parent owns and terminates it on close."""
    while not stopped.is_set():
        try:
            request = requests.get(timeout=.1)
        except queue.Empty:
            continue
        if request is None:
            break
        result = {"id": request["id"], "kind": request["kind"], "key": request.get("key"), "size": request.get("size")}
        kind = request["kind"]
        stale = request["epoch"] != epochs[kind].value
        try:
            if stale:
                result["stale"] = True
            elif kind == "import":
                result["entries"], result["warnings"] = import_file(request["filename"], request["threshold"])
            elif kind == "content":
                result["shape"], result["geometry"], result["warning"], _ = canonical_asset(request["asset"])
            else:
                result["pixels"], result["warning"], result["disk_hit"] = preview(request["asset"], request["size"], Path(request["cache"]))
        except Exception as exc:
            result["error"] = str(exc)
        while not stopped.is_set():
            try:
                results.put(result, timeout=.1)
                break
            except queue.Full:
                continue
