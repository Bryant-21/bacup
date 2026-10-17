"""Compiled timelines and surgical placement edits, independent of rendering.

Definitions, ABC and unknown tags are never passed through the artwork writer.
Editing a nested timeline edits its shared symbol definition, like Flash does.
"""
from __future__ import annotations

from dataclasses import dataclass, field, replace
import math
import struct

from creation_lib.swf.types import BitReader, BitWriter, MATRIX, RECT
from ui.swf_editor.movie import Movie, MovieTag, encode_tag, split_tags


def compose(parent: MATRIX, child: MATRIX) -> MATRIX:
    a, b, c, d = parent.scale_x, parent.rotate_skew_0, parent.rotate_skew_1, parent.scale_y
    e, f, g, h = child.scale_x, child.rotate_skew_0, child.rotate_skew_1, child.scale_y
    return MATRIX(a*e+c*f, b*g+d*h, b*e+d*f, a*g+c*h,
                  a*child.translate_x+c*child.translate_y+parent.translate_x,
                  b*child.translate_x+d*child.translate_y+parent.translate_y)


def transform_bounds(bounds, matrix):
    if bounds is None:
        return None
    x0, y0, x1, y1 = bounds
    points = [(matrix.scale_x*x+matrix.rotate_skew_1*y+matrix.translate_x/20,
               matrix.rotate_skew_0*x+matrix.scale_y*y+matrix.translate_y/20)
              for x, y in ((x0,y0),(x1,y0),(x1,y1),(x0,y1))]
    return (min(p[0] for p in points), min(p[1] for p in points),
            max(p[0] for p in points), max(p[1] for p in points))


def union_bounds(bounds):
    values = [b for b in bounds if b is not None]
    return (min(b[0] for b in values), min(b[1] for b in values),
            max(b[2] for b in values), max(b[3] for b in values)) if values else None


@dataclass(frozen=True)
class Placement:
    depth: int
    character: int | None
    matrix: MATRIX | None
    matrix_span: tuple[int, int]
    name: str | None = None
    clip_depth: int | None = None
    move: bool = False


def read_placement(tag: MovieTag) -> Placement:
    """Read only the placement prefix. Filters/actions/AMF remain opaque bytes."""
    if tag.code not in {4, 26, 70, 94}:
        raise ValueError("Select a PlaceObject placement")
    reader = BitReader(tag.payload)
    if tag.code == 4:
        character, depth = reader.read_ui16(), reader.read_ui16()
        start = reader.tell()
        matrix = MATRIX.parse(reader)
        return Placement(depth, character, matrix, (start, reader.tell()))
    flags = reader.read_ui8()
    flags2 = reader.read_ui8() if tag.code in {70, 94} else 0
    depth = reader.read_ui16()
    # PlaceObject3 ClassName precedes CharacterId and the matrix.
    if flags2 & 8 or flags2 & 16 and flags & 2:
        reader.read_string()
    character = reader.read_ui16() if flags & 2 else None
    start = reader.tell()
    matrix = MATRIX.parse(reader) if flags & 4 else None
    span = (start, reader.tell())
    if flags & 8:
        add, mult, bits = reader.read_ubits(1), reader.read_ubits(1), reader.read_ubits(4)
        for _ in range(4 * (add+mult)):
            reader.read_sbits(bits)
        reader.align()
    if flags & 16:
        reader.read_ui16()
    name = reader.read_string() if flags & 32 else None
    clip_depth = reader.read_ui16() if flags & 64 else None
    return Placement(depth, character, matrix, span, name, clip_depth, bool(flags & 1))


@dataclass
class StageNode:
    key: tuple[int, ...]
    address: tuple[int, ...]
    owner: int
    depth: int
    character: int
    name: str
    matrix: MATRIX
    parent_matrix: MATRIX
    world_matrix: MATRIX
    frame: int
    placed_frame: int
    clip_depth: int | None
    bounds: tuple[float, float, float, float] | None = None
    children: list[StageNode] = field(default_factory=list)


class CompiledStage:
    def __init__(self, data: bytes):
        self.movie = Movie(data)
        reader = BitReader(self.movie.header)
        rect = RECT.parse(reader)
        self.rect = (rect.xmin/20, rect.ymin/20, rect.xmax/20, rect.ymax/20)
        self.fps = reader.read_ui16()/256
        self.frames = reader.read_ui16()
        self.definitions = {}
        self.timelines = {0: (self.movie.tags, (), self.frames)}
        self.symbols = {}
        self.warnings = []
        self._index(self.movie.tags, ())

    def _index(self, tags, path):
        for index, tag in enumerate(tags):
            address = path + (index,)
            if tag.character_id is not None:
                self.definitions[tag.character_id] = (tag, address)
            if tag.code == 39:
                if len(tag.payload) < 4:
                    raise ValueError("Truncated sprite definition")
                character, frames = struct.unpack_from('<HH', tag.payload)
                nested = split_tags(tag.payload[4:])
                self.timelines[character] = (nested, address, frames)
                self._index(nested, address)
            elif tag.code in {56, 76}:
                reader = BitReader(tag.payload)
                for _ in range(reader.read_ui16()):
                    character = reader.read_ui16()
                    self.symbols[character] = reader.read_string()

    def display_list(self, owner=0, frame=1):
        tags, path, frames = self.timelines[owner]
        if not 1 <= frame <= max(frames, 1):
            raise ValueError(f"Frame must be between 1 and {max(frames,1)}")
        current, live = 1, {}
        for index, tag in enumerate(tags):
            if tag.code == 1:
                if current == frame:
                    break
                current += 1
            elif tag.code in {4, 26, 70, 94}:
                p = read_placement(tag)
                old = live.get(p.depth) if p.move else None
                if p.character is None and old is None:
                    self.warnings.append(f"Timeline {owner}: modification of absent depth {p.depth}")
                    continue
                character = p.character if p.character is not None else old.character
                matrix = p.matrix or (old.matrix if old else MATRIX())
                live[p.depth] = StageNode((), path+(index,), owner, p.depth, character,
                    p.name if p.name is not None else old.name if old else '', matrix,
                    MATRIX(), matrix, frame, old.placed_frame if old else current,
                    p.clip_depth if p.clip_depth is not None else old.clip_depth if old else None)
            elif tag.code in {5, 28}:
                depth = struct.unpack_from('<H', tag.payload, 2 if tag.code == 5 else 0)[0]
                live.pop(depth, None)
        return list(live.values())

    def _bounds(self, character):
        value = self.definitions.get(character)
        if value is None:
            return None
        tag = value[0]
        if tag.code in {2, 22, 32, 83, 11, 33, 37, 46, 84}:
            reader = BitReader(tag.payload[2:])
            rect = RECT.parse(reader)
            bounds = (rect.xmin/20, rect.ymin/20, rect.xmax/20, rect.ymax/20)
            if tag.code in {46, 84}:
                end = RECT.parse(reader)
                bounds = union_bounds([bounds, (end.xmin/20,end.ymin/20,end.xmax/20,end.ymax/20)])
            return bounds
        if tag.code in {20, 36} and len(tag.payload) >= 7:
            w, h = struct.unpack_from('<HH', tag.payload, 3)
            return (0,0,w,h)
        return None

    def nodes(self, owner=0, frame=1):
        self.warnings.clear()
        budget = [4096]
        def walk(owner, frame, parent, key, ancestors):
            if len(ancestors) > 32 or owner in ancestors:
                self.warnings.append("Recursive/deep sprite; hierarchy truncated")
                return []
            result = []
            for node in self.display_list(owner, frame):
                budget[0] -= 1
                if budget[0] < 0:
                    self.warnings.append("Hierarchy exceeds 4096 placements; select a sprite timeline")
                    break
                node.key = key+(node.depth,)
                node.parent_matrix = parent
                node.world_matrix = compose(parent, node.matrix)
                if node.character in self.timelines:
                    count = max(self.timelines[node.character][2], 1)
                    child_frame = (frame-node.placed_frame) % count + 1
                    node.children = walk(node.character, child_frame, node.world_matrix, node.key, ancestors+(owner,))
                    node.bounds = union_bounds(child.bounds for child in node.children)
                else:
                    node.bounds = transform_bounds(self._bounds(node.character), node.world_matrix)
                result.append(node)
            return result
        return walk(owner, frame, MATRIX(), (), ())

    def set_matrix(self, address: tuple[int,...], matrix: MATRIX) -> bytes:
        values = (matrix.scale_x, matrix.scale_y, matrix.rotate_skew_0, matrix.rotate_skew_1,
                  matrix.translate_x, matrix.translate_y)
        if not all(math.isfinite(value) for value in values):
            raise ValueError("Placement values must be finite")
        if max(abs(value) for value in values[:4]) >= 16384 or max(abs(value) for value in values[4:]) >= 2**30:
            raise ValueError("Placement exceeds SWF MATRIX precision/range")
        matrix = replace(matrix, translate_x=round(matrix.translate_x), translate_y=round(matrix.translate_y))
        writer = BitWriter()
        matrix.write(writer)
        encoded_matrix = writer.getvalue()
        def patch(tags, path):
            index = path[0]
            tag = tags[index]
            if len(path) > 1:
                if tag.code != 39:
                    raise ValueError("Invalid nested placement address")
                payload = tag.payload[:4] + b''.join(item.encoded for item in patch(split_tags(tag.payload[4:]), path[1:]))
            else:
                p = read_placement(tag)
                start, end = p.matrix_span
                prefix = bytearray(tag.payload[:start])
                if tag.code != 4:
                    prefix[0] |= 4
                payload = bytes(prefix) + encoded_matrix + tag.payload[end:]
            tags = list(tags)
            tags[index] = MovieTag(tag.code, payload, encode_tag(tag.code, payload))
            return tags
        result = self.movie.assemble(patch(self.movie.tags, address))
        CompiledStage(result)
        return result


def flatten(nodes):
    for node in nodes:
        yield node
        yield from flatten(node.children)
