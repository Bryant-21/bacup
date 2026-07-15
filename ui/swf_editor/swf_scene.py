"""SWF editor document model.

Bridges py_creation_lib/python/creation_lib/swf's parsed SwfDocument and the editor's mutable state.
Supports layers, keyframes, tweens, selection, and undo snapshots.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

from creation_lib.swf.types import RGBA, MATRIX
from creation_lib.swf.shapes import ShapeDef
from creation_lib.swf.parser import SwfDocument
from creation_lib.swf.timeline import Timeline, Frame, DisplayEntry


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

    def to_matrix(self) -> MATRIX:
        import math
        rad = math.radians(self.rotation)
        cos_r = math.cos(rad)
        sin_r = math.sin(rad)
        return MATRIX(
            scale_x=self.scale_x * cos_r,
            scale_y=self.scale_y * cos_r,
            rotate_skew_0=sin_r,
            rotate_skew_1=-sin_r,
            translate_x=int(self.x * 20),
            translate_y=int(self.y * 20),
        )

    @classmethod
    def from_matrix(cls, m: MATRIX) -> "AffineTransform":
        import math
        rotation = math.degrees(math.atan2(m.rotate_skew_0, m.scale_x))
        sx = (m.scale_x ** 2 + m.rotate_skew_0 ** 2) ** 0.5
        sy = (m.scale_y ** 2 + m.rotate_skew_1 ** 2) ** 0.5
        return cls(
            x=m.translate_x / 20.0,
            y=m.translate_y / 20.0,
            scale_x=sx,
            scale_y=sy,
            rotation=rotation,
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
        )
        scene.library_symbols = dict(doc.shapes)

        # Also add sprites' shapes to the library so nested characters resolve
        for sprite in doc.sprites.values():
            for frame in sprite.timeline.frames:
                for depth, entry in frame.placements.items():
                    if entry.character_id in doc.shapes:
                        scene.library_symbols.setdefault(
                            entry.character_id, doc.shapes[entry.character_id]
                        )

        tl = doc.main_timeline
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

                if same and not label:
                    # Extend current keyframe duration
                    continue

                # Flush previous keyframe
                if prev_entry is not None and kf_start is not None:
                    kf = Keyframe(
                        frame=kf_start,
                        duration=fi - kf_start,
                        display_list=[
                            EditorDisplayEntry(
                                shape_id=prev_entry.character_id,
                                transform=AffineTransform.from_matrix(prev_entry.matrix),
                                instance_name=prev_entry.name or "",
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
                    duration=len(frame_dls) - kf_start,
                    display_list=[
                        EditorDisplayEntry(
                            shape_id=prev_entry.character_id,
                            transform=AffineTransform.from_matrix(prev_entry.matrix),
                            instance_name=prev_entry.name or "",
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
        return max((l.total_frames for l in self.layers), default=1)

    def add_layer(self, name: str | None = None) -> Layer:
        name = name or f"Layer {len(self.layers) + 1}"
        layer = Layer(name=name)
        self.layers.insert(self.active_layer_index, layer)
        self.dirty = True
        return layer

    def remove_layer(self, index: int) -> None:
        if len(self.layers) <= 1:
            return
        self.layers.pop(index)
        if self.active_layer_index >= len(self.layers):
            self.active_layer_index = len(self.layers) - 1
        self.dirty = True

    # ── Project file save/load (.swfproj) ──────────────────────────

    def to_project(self) -> dict:
        """Serialize to .swfproj JSON structure."""
        return {
            "version": 1,
            "canvas": [self.canvas_width, self.canvas_height],
            "fps": self.fps,
            "background": self.background.to_hex(),
            "layers": [self._serialize_layer(l) for l in self.layers],
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
        for layer_data in data.get("layers", []):
            scene.layers.append(cls._deserialize_layer(layer_data))
        if not scene.layers:
            scene.layers.append(Layer(name="Layer 1"))
        return scene

    def save_project(self, path: str) -> None:
        """Save to .swfproj file."""
        import json
        Path(path).write_text(json.dumps(self.to_project(), indent=2), encoding="utf-8")
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
                    ),
                    color_alpha=de.get("alpha", 255),
                    instance_name=de.get("name", ""),
                )
                for de in data.get("display_list", [])
            ],
        )

    def _serialize_shape_ref(self, shape: ShapeDef) -> dict:
        return {
            "shape_id": shape.shape_id,
            "bounds": list(shape.bounds_px),
            "fill_count": len(shape.fill_styles),
            "record_count": len(shape.records),
        }
