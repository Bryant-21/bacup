"""SWF editor document model.

Bridges py_creation_lib/python/creation_lib/swf's parsed SwfDocument and the editor's mutable state.
Supports layers, keyframes, tweens, selection, and undo snapshots.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path

from creation_lib.swf.types import RGBA, MATRIX
from creation_lib.swf.shapes import ShapeDef
from creation_lib.swf.parser import SwfDocument
from creation_lib.swf.timeline import DisplayEntry


class TweenType(Enum):
    NONE = "none"
    MOTION = "motion"
    SHAPE = "shape"


class LayerType(Enum):
    NORMAL = "normal"
    GUIDE = "guide"


@dataclass
class AffineTransform:
    """Editor-friendly transform (decomposed from MATRIX)."""
    x: float = 0.0
    y: float = 0.0
    scale_x: float = 1.0
    scale_y: float = 1.0
    rotation: float = 0.0  # degrees
    skew_x: float = 0.0
    skew_y: float = 0.0

    def apply(self, x, y):
        m = self.to_matrix()
        return (m.scale_x * x + m.rotate_skew_1 * y + self.x,
                m.rotate_skew_0 * x + m.scale_y * y + self.y)

    def inverse(self, x, y):
        m = self.to_matrix()
        determinant = m.scale_x * m.scale_y - m.rotate_skew_0 * m.rotate_skew_1
        if abs(determinant) < 1e-10:
            return float("inf"), float("inf")
        x, y = x - self.x, y - self.y
        return ((m.scale_y * x - m.rotate_skew_1 * y) / determinant,
                (m.scale_x * y - m.rotate_skew_0 * x) / determinant)

    def to_matrix(self) -> MATRIX:
        import math
        rad = math.radians(self.rotation)
        cos_r = math.cos(rad)
        sin_r = math.sin(rad)
        return MATRIX(
            scale_x=self.scale_x * cos_r,
            scale_y=self.scale_y * (cos_r + sin_r * self.skew_x),
            rotate_skew_0=self.scale_x * sin_r,
            rotate_skew_1=self.scale_y * (cos_r * self.skew_x - sin_r),
            # skew_x is the shear of the local y axis; decomposition retains
            # reflections and nonuniform rotation without changing placement.
            translate_x=int(self.x * 20),
            translate_y=int(self.y * 20),
        )

    @classmethod
    def from_matrix(cls, m: MATRIX) -> "AffineTransform":
        import math
        rotation = math.degrees(math.atan2(m.rotate_skew_0, m.scale_x))
        sx = (m.scale_x ** 2 + m.rotate_skew_0 ** 2) ** 0.5
        sy = ((m.scale_x * m.scale_y - m.rotate_skew_0 * m.rotate_skew_1) / sx
              if sx else (m.scale_y ** 2 + m.rotate_skew_1 ** 2) ** 0.5)
        shear = ((m.scale_x * m.rotate_skew_1 + m.rotate_skew_0 * m.scale_y)
                 / (sx * sy) if sx and sy else 0.0)
        return cls(
            x=m.translate_x / 20.0,
            y=m.translate_y / 20.0,
            scale_x=sx,
            scale_y=sy,
            rotation=rotation,
            skew_x=shear,
        )


@dataclass
class EditorDisplayEntry:
    """One shape instance on the canvas at a specific frame."""
    shape_id: int
    transform: AffineTransform = field(default_factory=AffineTransform)
    color_alpha: int = 255
    instance_name: str = ""


@dataclass
class Keyframe:
    """Single keyframe within a layer."""
    frame: int
    duration: int = 1
    display_list: list[EditorDisplayEntry] = field(default_factory=list)
    tween: TweenType = TweenType.NONE
    label: str = ""


@dataclass
class Layer:
    """Editor layer with keyframes."""
    name: str
    visible: bool = True
    locked: bool = False
    opacity: float = 1.0
    layer_type: LayerType = LayerType.NORMAL
    keyframes: list[Keyframe] = field(default_factory=list)

    def keyframe_at(self, frame: int) -> Keyframe | None:
        """Get the active keyframe at a given frame index."""
        result = None
        for kf in self.keyframes:
            if kf.frame <= frame < kf.frame + kf.duration:
                result = kf
                break
            if kf.frame > frame:
                break
        return result

    @property
    def total_frames(self) -> int:
        if not self.keyframes:
            return 0
        last = self.keyframes[-1]
        return last.frame + last.duration


@dataclass
class SwfScene:
    """Mutable editor document state."""
    canvas_width: int = 550
    canvas_height: int = 400
    fps: int = 30
    background: RGBA = field(default_factory=lambda: RGBA(0x33, 0x33, 0x33, 255))

    layers: list[Layer] = field(default_factory=list)
    library_symbols: dict[int, ShapeDef] = field(default_factory=dict)

    # Editor state (not saved to project)
    active_layer_index: int = 0
    current_frame: int = 0
    selection: list[tuple[int, int]] = field(default_factory=list)  # (layer_idx, entry_idx)
    playing: bool = False
    file_path: str = ""
    dirty: bool = False
    timeline_length: int = 1
    frame_labels: dict[str, str] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if not self.layers:
            self.layers = [Layer(name="Layer 1", keyframes=[Keyframe(frame=0)])]

    @classmethod
    def from_swf_document(cls, doc: SwfDocument) -> "SwfScene":
        """Import a parsed SwfDocument into the editor model.

        Uses cumulative display list semantics: shapes placed at a depth
        persist across frames until explicitly removed or replaced.
        Consecutive frames with identical display state at a given depth
        are merged into a single keyframe with the appropriate duration.
        """
        scene = cls(
            canvas_width=doc.header.frame_size.width_px,
            canvas_height=doc.header.frame_size.height_px,
            fps=int(doc.header.fps),
            background=doc.background_color,
            timeline_length=doc.header.frame_count,
        )
        from ui.swf_editor.movie import simple_alpha
        scene.library_symbols = dict(doc.shapes)
        scene.layers.clear()

        # Also add sprites' shapes to the library so nested characters resolve
        for sprite in doc.sprites.values():
            for frame in sprite.timeline.frames:
                for depth, entry in frame.placements.items():
                    if entry.character_id in doc.shapes:
                        scene.library_symbols.setdefault(
                            entry.character_id, doc.shapes[entry.character_id]
                        )

        tl = doc.main_timeline
        scene.frame_labels = {str(fi): frame.label for fi, frame in enumerate(tl.frames) if frame.label}
        if not tl.frames:
            scene.layers.append(Layer(name="Layer 1"))
            return scene

        # Compute cumulative display list at every frame
        frame_dls: list[dict[int, DisplayEntry]] = []
        for fi in range(len(tl.frames)):
            frame_dls.append(tl.display_list_at(fi))

        # Collect all depths used across all frames
        all_depths: set[int] = set()
        for dl in frame_dls:
            all_depths.update(dl.keys())

        # One layer per depth
        depth_to_layer: dict[int, int] = {}
        for i, depth in enumerate(sorted(all_depths)):
            layer = Layer(name=f"Layer {i + 1}")
            depth_to_layer[depth] = i
            scene.layers.append(layer)

        # Build keyframes by merging consecutive identical states per depth
        for depth in sorted(all_depths):
            layer = scene.layers[depth_to_layer[depth]]
            kf_start: int | None = None
            prev_entry: DisplayEntry | None = None

            for fi in range(len(frame_dls)):
                entry = frame_dls[fi].get(depth)
                label = tl.frames[fi].label or ""

                # Check if state changed from previous frame
                same = (
                    entry is not None
                    and prev_entry is not None
                    and entry.character_id == prev_entry.character_id
                    and entry.matrix == prev_entry.matrix
                )

                same = (same and entry.color_transform == prev_entry.color_transform
                        and entry.name == prev_entry.name)
                if same and not label:
                    # Extend current keyframe duration
                    continue

                # Flush previous keyframe
                if prev_entry is not None and kf_start is not None:
                    kf = Keyframe(
                        frame=kf_start,
                        label=tl.frames[kf_start].label or "",
                        duration=fi - kf_start,
                        display_list=[
                            EditorDisplayEntry(
                                shape_id=prev_entry.character_id,
                                transform=AffineTransform.from_matrix(prev_entry.matrix),
                                instance_name=prev_entry.name or "",
                                color_alpha=simple_alpha(prev_entry.color_transform),
                            )
                        ],
                    )
                    layer.keyframes.append(kf)

                # Start new keyframe (or gap)
                if entry is not None:
                    kf_start = fi
                    prev_entry = entry
                else:
                    kf_start = None
                    prev_entry = None

            # Flush final keyframe
            if prev_entry is not None and kf_start is not None:
                kf = Keyframe(
                    frame=kf_start,
                    label=tl.frames[kf_start].label or "",
                    duration=len(frame_dls) - kf_start,
                    display_list=[
                        EditorDisplayEntry(
                            shape_id=prev_entry.character_id,
                            transform=AffineTransform.from_matrix(prev_entry.matrix),
                            instance_name=prev_entry.name or "",
                            color_alpha=simple_alpha(prev_entry.color_transform),
                        )
                    ],
                )
                layer.keyframes.append(kf)

        # Ensure at least one layer
        if not scene.layers:
            scene.layers.append(Layer(name="Layer 1"))

        return scene

    def get_visible_entries(self) -> list[tuple[ShapeDef, AffineTransform]]:
        """Get all visible shape instances on the current frame."""
        entries: list[tuple[ShapeDef, AffineTransform]] = []
        for layer in self.layers:
            if not layer.visible or layer.layer_type == LayerType.GUIDE:
                continue
            kf = layer.keyframe_at(self.current_frame)
            if not kf:
                continue
            for de in kf.display_list:
                shape = self.library_symbols.get(de.shape_id)
                if shape:
                    entries.append((shape, de.transform))
        return entries

    @property
    def total_frames(self) -> int:
        return max(self.timeline_length, max((layer.total_frames for layer in self.layers), default=1))

    def get_render_entries(self):
        return [(self.library_symbols[entry.shape_id], entry.transform,
                 layer.opacity * entry.color_alpha / 255)
                for layer in self.layers if layer.visible and layer.layer_type != LayerType.GUIDE
                for kf in [layer.keyframe_at(self.current_frame)] if kf
                for entry in kf.display_list if entry.shape_id in self.library_symbols]

    def add_layer(self, name: str | None = None) -> Layer:
        name = name or f"Layer {len(self.layers) + 1}"
        layer = Layer(name=name, keyframes=[Keyframe(frame=0, duration=max(self.total_frames, 1))])
        self.layers.insert(self.active_layer_index, layer)
        self.dirty = True
        return layer

    def remove_layer(self, index: int) -> None:
        if len(self.layers) <= 1:
            return
        self.layers.pop(index)
        self.selection.clear()
        if self.active_layer_index >= len(self.layers):
            self.active_layer_index = len(self.layers) - 1
        self.dirty = True

    def insert_keyframe(self, frame: int, blank=False):
        import copy
        layer = self.layers[self.active_layer_index]
        if layer.locked:
            raise ValueError("The active layer is locked")
        previous = layer.keyframe_at(frame)
        if previous and previous.frame == frame:
            if blank:
                previous.display_list.clear()
            return previous
        if previous is None:
            previous = layer.keyframe_at(max(frame - 1, 0))
        entries = copy.deepcopy(previous.display_list) if previous and not blank else []
        duration = previous.frame + previous.duration - frame if previous else 1
        if previous:
            previous.duration = frame - previous.frame
        keyframe = Keyframe(frame=frame, duration=max(duration, 1), display_list=entries)
        layer.keyframes.append(keyframe)
        layer.keyframes.sort(key=lambda k: k.frame)
        self.selection.clear()
        self.dirty = True
        return keyframe

    # ── Project file save/load (.swfproj) ──────────────────────────

    def to_project(self) -> dict:
        """Serialize to .swfproj JSON structure."""
        return {
            "editor_version": 2,
            "canvas": [self.canvas_width, self.canvas_height],
            "fps": self.fps,
            "background": self.background.to_hex(),
            "timeline_length": self.timeline_length,
            "frame_labels": self.frame_labels,
            "layers": [self._serialize_layer(layer) for layer in self.layers],
            "symbols": {str(k): self._serialize_shape_ref(v)
                        for k, v in self.library_symbols.items()},
            "metadata": {
                "author": "B21",
            },
        }

    @classmethod
    def from_project(cls, data: dict) -> "SwfScene":
        """Deserialize from .swfproj JSON."""
        scene = cls(
            canvas_width=data.get("canvas", [550, 400])[0],
            canvas_height=data.get("canvas", [550, 400])[1],
            fps=data.get("fps", 30),
            background=RGBA.from_hex(data.get("background", "#333333")),
        )
        scene.layers.clear()
        if "editor_scene" in data:
            data = data["editor_scene"]
        scene.timeline_length = data.get("timeline_length", 1)
        scene.frame_labels = dict(data.get("frame_labels", {}))
        for layer_data in data.get("layers", []):
            scene.layers.append(cls._deserialize_layer(layer_data))
        if not scene.layers:
            scene.layers.append(Layer(name="Layer 1", keyframes=[Keyframe(frame=0)]))
        import base64
        from creation_lib.swf.tags import DefineShapeTag
        for sid, shape in data.get("symbols", {}).items():
            if "tag_data" not in shape:
                raise ValueError("Legacy project contains shape references without geometry; reopen its source SWF")
            tag = DefineShapeTag.parse(base64.b64decode(shape["tag_data"], validate=True), shape["tag_code"])
            scene.library_symbols[int(sid)] = tag.shape
        return scene

    def save_project(self, path: str) -> None:
        """Save to .swfproj file."""
        import json
        from ui.swf_editor.document import atomic_write
        atomic_write(Path(path), json.dumps(self.to_authoring_project(), indent=2).encode("utf-8"))
        self.file_path = path
        self.dirty = False

    @classmethod
    def load_project(cls, path: str) -> "SwfScene":
        """Load from .swfproj file."""
        import json
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        scene = cls.from_project(data)
        scene.file_path = path
        return scene

    def _serialize_layer(self, layer: Layer) -> dict:
        return {
            "name": layer.name,
            "visible": layer.visible,
            "locked": layer.locked,
            "opacity": layer.opacity,
            "type": layer.layer_type.value,
            "keyframes": [self._serialize_keyframe(kf) for kf in layer.keyframes],
        }

    @classmethod
    def _deserialize_layer(cls, data: dict) -> Layer:
        return Layer(
            name=data.get("name", "Layer"),
            visible=data.get("visible", True),
            locked=data.get("locked", False),
            opacity=data.get("opacity", 1.0),
            layer_type=LayerType(data.get("type", "normal")),
            keyframes=[cls._deserialize_keyframe(kf) for kf in data.get("keyframes", [])],
        )

    def _serialize_keyframe(self, kf: Keyframe) -> dict:
        return {
            "frame": kf.frame,
            "duration": kf.duration,
            "tween": kf.tween.value,
            "label": kf.label,
            "display_list": [
                {
                    "shape_id": de.shape_id,
                    "x": de.transform.x, "y": de.transform.y,
                    "scale_x": de.transform.scale_x, "scale_y": de.transform.scale_y,
                    "rotation": de.transform.rotation,
                    "skew_x": de.transform.skew_x,
                    "alpha": de.color_alpha,
                    "name": de.instance_name,
                }
                for de in kf.display_list
            ],
        }

    @classmethod
    def _deserialize_keyframe(cls, data: dict) -> Keyframe:
        return Keyframe(
            frame=data.get("frame", 0),
            duration=data.get("duration", 1),
            tween=TweenType(data.get("tween", "none")),
            label=data.get("label", ""),
            display_list=[
                EditorDisplayEntry(
                    shape_id=de.get("shape_id", 0),
                    transform=AffineTransform(
                        x=de.get("x", 0), y=de.get("y", 0),
                        scale_x=de.get("scale_x", 1), scale_y=de.get("scale_y", 1),
                        rotation=de.get("rotation", 0),
                        skew_x=de.get("skew_x", 0),
                    ),
                    color_alpha=de.get("alpha", 255),
                    instance_name=de.get("name", ""),
                )
                for de in data.get("display_list", [])
            ],
        )

    def _serialize_shape_ref(self, shape: ShapeDef) -> dict:
        import base64
        from creation_lib.swf.tags import DefineShapeTag
        code = {1: 2, 2: 22, 3: 32, 4: 83}[shape.shape_version]
        return {
            "shape_id": shape.shape_id,
            "tag_code": code,
            "tag_data": base64.b64encode(DefineShapeTag(shape, code).to_bytes()).decode("ascii"),
        }

    def to_authoring_project(self, version: int = 17) -> dict:
        """A CLI-packable project plus exact editor layers/shape geometry."""
        from dataclasses import asdict
        import copy
        from creation_lib.swf.types import BitWriter
        stage = []
        previous = set()
        slots = [(li, ei) for li, layer in enumerate(self.layers)
                 for ei in range(max((len(k.display_list) for k in layer.keyframes), default=0))]
        for frame in range(max(self.total_frames, 1)):
            places = []
            label = self.frame_labels.get(str(frame), "")
            for depth, (li, ei) in enumerate(slots, 1):
                layer = self.layers[li]
                if not layer.visible or layer.layer_type == LayerType.GUIDE:
                    continue
                kf = layer.keyframe_at(frame)
                if not kf or ei >= len(kf.display_list):
                    continue
                if kf.frame == frame and kf.label:
                    label = kf.label
                entry = kf.display_list[ei]
                if kf.tween != TweenType.NONE:
                    raise ValueError("Tween export is not implemented; use explicit keyframes")
                if entry.shape_id not in self.library_symbols:
                    raise ValueError(f"Missing vector character {entry.shape_id}; use the binary workspace for nested movies")
                alpha = round(entry.color_alpha * layer.opacity)
                # CXFORMWITHALPHA: multiplier terms, no additions, 9 bits.
                writer = BitWriter()
                writer.write_ubits(1, 0)
                writer.write_ubits(1, 1)
                writer.write_ubits(4, 10)
                for value in (256, 256, 256, round(alpha * 256 / 255)):
                    writer.write_sbits(10, value)
                writer.align()
                places.append({"depth": depth, "character": entry.shape_id,
                               "matrix": asdict(entry.transform.to_matrix()),
                               "name": entry.instance_name or None,
                               **({"color_transform": writer.getvalue().hex()} if alpha != 255 else {})})
            occupied = {p["depth"] for p in places}
            stage.append({"place": places, "remove": sorted(previous - occupied), "label": label})
            previous = occupied
        return {
            "version": version, "canvas": [self.canvas_width, self.canvas_height],
            "fps": self.fps, "background": self.background.to_hex(),
            "shapes": [{"id": sid, **self._serialize_shape_ref(copy.deepcopy(shape))}
                       for sid, shape in self.library_symbols.items()],
            "stage": stage, "editor_scene": self.to_project(),
        }
