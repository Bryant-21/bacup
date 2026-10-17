"""Transactional SWF editing independent of ImGui and OpenGL.

Visual projects and AS3 authoring projects use the CLI's .swfproj schema.
Binary sessions additionally retain the actual movie, without rebuilding its
unknown tags. Builds never write the source .as files or the input SWF.
"""
from __future__ import annotations

import base64
import copy
from dataclasses import dataclass, field
import hashlib
import json
import os
from pathlib import Path
import tempfile

from creation_lib.swf import native_runtime
from creation_lib.swf.parser import parse_swf
from creation_lib.swf.project import build_document
from creation_lib.swf.writer import write_swf
from ui.swf_editor.movie import Movie, artwork_metadata_supported
from ui.swf_editor.swf_scene import SwfScene


def binary_scene(movie: Movie) -> SwfScene:
    """Read stage metadata without asking the artwork parser to decode assets."""
    from creation_lib.swf.types import BitReader, RECT, RGBA
    reader = BitReader(movie.header)
    rect = RECT.parse(reader)
    reader.align()
    scene = SwfScene(canvas_width=rect.width_px, canvas_height=rect.height_px,
                     fps=reader.read_ui16() / 256)
    scene.timeline_length = reader.read_ui16()
    for tag in movie.tags:
        if tag.code == 9 and len(tag.payload) == 3:
            scene.background = RGBA(*tag.payload, 255)
    return scene


def atomic_write(path: Path, data: bytes) -> None:
    """Replace only after the full output was written; retain original on failure."""
    path = path.resolve()
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f".{path.name}.", suffix=".tmp", delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


@dataclass
class SwfEditingDocument:
    mode: str = "visual"
    scene: SwfScene = field(default_factory=SwfScene)
    path: Path | None = None
    base_dir: Path = field(default_factory=Path.cwd)
    manifest: dict = field(default_factory=dict)
    sources: dict[str, str] = field(default_factory=dict)
    movie: bytes | None = None
    saved_state: str = ""
    diagnostics: str = ""
    manifest_buffer: str = ""
    original_scene: str = ""
    _source_signatures: dict = field(default_factory=dict, init=False, repr=False)

    def __post_init__(self):
        if self.mode == "source" and not self.manifest_buffer:
            self.manifest_buffer = json.dumps({k: v for k, v in self.manifest.items()
                                               if k not in {"script_sources", "editor_manifest_buffer"}}, indent=2)
        if not self.saved_state:
            self.mark_saved()

    @classmethod
    def menu(cls):
        from creation_lib.swf.types import RGBA
        source = """package {
    import flash.display.MovieClip;
    import flash.text.TextField;
    import flash.text.TextFormat;
    public class DocumentMenu extends MovieClip {
        public function DocumentMenu() {
            var title:TextField = new TextField();
            title.defaultTextFormat = new TextFormat("$MAIN_Font", 28, 0xFFFFFF);
            title.width = 800;
            title.text = "New Scaleform menu";
            title.x = 32;
            title.y = 32;
            title.selectable = false;
            addChild(title);
        }
    }
}
"""
        return cls(mode="source", scene=SwfScene(canvas_width=1280, canvas_height=720,
                                                background=RGBA(0, 0, 0, 255)),
                   manifest={"canvas": [1280, 720], "version": 14, "fps": 30,
                             "background": "#000000", "scripts": ["DocumentMenu.as"],
                             "exports": [{"character": 0, "class": "DocumentMenu"}]},
                   sources={"DocumentMenu.as": source})

    @classmethod
    def open(cls, path: str | Path):
        path = Path(path).resolve()
        if path.suffix.lower() == ".swf":
            data = path.read_bytes()
            movie = Movie(data)
            # Only flat, vector timelines can be freely authored. Everything
            # else stays opaque in the binary workspace, including ABC/text.
            visual = all(tag.code in {0, 1, 2, 9, 22, 26, 28, 32, 43, 69, 83} for tag in movie.tags)
            visual = visual and artwork_metadata_supported(movie)
            visual = visual and not any(tag.code == 69 and len(tag.payload) >= 4 and (tag.payload[0] & 8) for tag in movie.tags)
            if visual:
                parsed = parse_swf(data)
                from ui.swf_editor.movie import simple_alpha
                for tag in parsed.tags:
                    if hasattr(tag, "color_transform"):
                        try:
                            simple_alpha(tag.color_transform)
                        except (ValueError, IndexError):
                            visual = False
                        visual = visual and tag.clip_depth is None and tag.clip_actions is None and tag.ratio is None
            if visual:
                scene = SwfScene.from_swf_document(parse_swf(data))
            else:
                scene = binary_scene(movie)
            result = cls(mode="visual" if visual else "binary", scene=scene, movie=data,
                         base_dir=path.parent, manifest={"version": movie.version})
            # Binary originals are never the implicit Save target.
            result.scene.file_path = str(path)
            result.original_scene = result.state_key()
            result.mark_saved()
            return result
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        if not isinstance(data, dict):
            raise ValueError("A SWF project must be a JSON object")
        if data.get("editor_mode") == "binary":
            movie = base64.b64decode(data["base_movie"], validate=True)
            scene = binary_scene(Movie(movie))
            scene.file_path = data.get("original_path", "")
            return cls(mode="binary", path=path, base_dir=path.parent, movie=movie, scene=scene,
                       sources=data.get("script_sources", {}), manifest=data.get("binary_options", {}))
        if "editor_scene" in data or "layers" in data:
            scene = SwfScene.from_project(data)
            scene.file_path = data.get("original_path", "")
            return cls(mode="visual", scene=scene, path=path, base_dir=path.parent, manifest=data,
                       sources=dict(data.get("script_sources", {})))
        # Opening invalid AS3 is allowed: it must remain editable before Build.
        sources = {}
        warnings = []
        for name in data.get("scripts", []):
            if name in data.get("script_sources", {}):
                sources[name] = data["script_sources"][name]
            else:
                try:
                    sources[name] = (path.parent / name).read_text(encoding="utf-8-sig")
                except OSError as exc:
                    warnings.append(f"{name}: {exc}")
        from creation_lib.swf.types import RGBA
        scene = SwfScene(canvas_width=data.get("canvas", [550, 400])[0],
                         canvas_height=data.get("canvas", [550, 400])[1], fps=data.get("fps", 30),
                         background=RGBA.from_hex(data.get("background", "#333333")))
        return cls(mode="source", path=path, base_dir=path.parent, manifest=data,
                   sources=sources, diagnostics="\n".join(warnings),
                   manifest_buffer=data.get("editor_manifest_buffer", ""))

    def project(self) -> dict:
        if self.mode == "binary":
            return {"editor_mode": "binary", "editor_version": 2,
                    "base_movie": base64.b64encode(self.movie).decode("ascii"),
                    "original_path": self.scene.file_path,
                    "script_sources": dict(self.sources), "binary_options": copy.deepcopy(self.manifest)}
        if self.mode == "visual":
            project = self.scene.to_authoring_project(self.manifest.get("version", 17))
            project["scripts"] = list(self.sources)
            project["exports"] = copy.deepcopy(self.manifest.get("exports", []))
            project["original_path"] = self.scene.file_path
        else:
            project = json.loads(self.manifest_buffer)
            if not isinstance(project, dict):
                raise ValueError("Project manifest must be a JSON object")
        project["script_sources"] = dict(self.sources)
        return project

    def state_key(self) -> str:
        # Editor metadata can be saved even when a tween cannot be exported.
        scene = self.scene.to_project() if self.mode == "visual" else None
        # Large recovered menus contain hundreds of files. Hash a buffer only
        # when its string changes; don't serialize all their text every frame.
        def signatures(values, group):
            result = {}
            for name, value in values.items():
                previous = self._source_signatures.get((group,name))
                if previous is None or previous[0] is not value:
                    previous = (value,hashlib.sha256(value.encode('utf-8')).hexdigest())
                    self._source_signatures[group,name] = previous
                result[name] = previous[1]
            return result
        manifest = {key:value for key,value in self.manifest.items() if key != 'embedded_sources'}
        baseline = signatures(self.manifest.get('embedded_sources',{}),'embedded')
        sources = signatures(self.sources,'sources')
        self._source_signatures = {key:value for key,value in self._source_signatures.items()
                                   if key[1] in (self.sources if key[0] == 'sources' else self.manifest.get('embedded_sources',{}))}
        return json.dumps([self.mode, scene, manifest, baseline, self.manifest_buffer, sources,
                           hashlib.sha256(self.movie).hexdigest() if self.mode == "binary" and self.movie else None],
                          sort_keys=True)

    @property
    def dirty(self) -> bool:
        return self.state_key() != self.saved_state

    def mark_saved(self) -> None:
        self.saved_state = self.state_key()
        self.scene.dirty = False

    def save(self, path: str | Path) -> None:
        target = Path(path).resolve()
        if target.suffix.lower() != ".swfproj":
            raise ValueError("Save an editing project as .swfproj; use Export for .swf")
        try:
            project = self.project()
        except json.JSONDecodeError:
            if self.mode != "source":
                raise
            project = copy.deepcopy(self.manifest)
            project["script_sources"] = dict(self.sources)
            project["editor_manifest_buffer"] = self.manifest_buffer
        # Preserve asset lookup after Save As without changing external assets.
        if self.mode == "source":
            for shape in project.get("shapes", []):
                if "svg" in shape:
                    shape["svg"] = str((self.base_dir / shape["svg"]).resolve())
            for name in project.get("scripts", []):
                if name not in self.sources:
                    raise ValueError(f"Missing source {name}; add it before saving")
        atomic_write(target, json.dumps(project, indent=2, ensure_ascii=False).encode("utf-8"))
        self.path = target
        if self.mode == "source":
            self.manifest = project
        self.base_dir = target.parent
        self.mark_saved()

    def build(self, cancel=None) -> bytes:
        if self.mode == "binary":
            Movie(self.movie)
            baseline = self.manifest.get("embedded_sources", {})
            if baseline:
                if any(self.sources.get(name) != text for name,text in baseline.items()):
                    raise ValueError('External recovered-source compilation is unavailable; use native method editing')
            result = bytes(self.movie)
            for name, method in self.manifest.get('native_method_sources',{}).items():
                if name not in self.sources:
                    raise ValueError(f'Missing method source {name}')
                if cancel is not None and cancel.is_set():
                    raise RuntimeError('Method compilation cancelled')
                if self.sources[name] != method['baseline']:
                    result = native_runtime.replace_as3_method_source(result,method['class'],method['method'],self.sources[name])
            return result
        project = self.project()
        if self.mode == "visual" and self.movie and self.state_key() == self.original_scene:
            return bytes(self.movie)
        return write_swf(build_document(project, self.base_dir))

    def export(self, path: str | Path) -> bytes:
        self.check_export_target(path)
        data = self.build()
        Movie(data)
        atomic_write(Path(path), data)
        return data

    def check_export_target(self, path):
        if self.scene.file_path and Path(path).resolve() == Path(self.scene.file_path).resolve():
            raise ValueError("Choose a new SWF output path to preserve the opened original")

    def apply_sources(self, targets: dict[str, str], *, augment: bool = False,
                      dependencies: list[bytes] | None = None) -> bytes:
        if self.mode != "binary" or not self.movie:
            raise ValueError("Class replacement applies to an opened binary SWF")
        if not targets:
            raise ValueError("Select a class and attach its AS3 source first")
        operation = native_runtime.augment_as3_classes if augment else native_runtime.replace_as3_classes
        result = operation(self.movie, targets, dependencies or [])
        Movie(result)
        self.movie = result
        return result

    def inspect_class(self, name: str, view: str = "outline", method: str | None = None):
        data = self.build()
        if view == "disassembly":
            return native_runtime.abc_disassemble(data, name, method or None)
        if view == "dependencies":
            return native_runtime.abc_class_references(data, name, True)
        return native_runtime.abc_class_outline(data, name)


class RevisionJob:
    """One build/edit at a time; cancelled or stale results cannot commit."""
    def __init__(self):
        from concurrent.futures import ThreadPoolExecutor
        import threading
        self.executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="swf-build")
        self.future = None
        self.key = None
        self.kind = ""
        self.cancel_event = threading.Event()

    @property
    def busy(self):
        return self.future is not None

    def start(self, document: SwfEditingDocument, operation, kind: str = "build", *, cancellable=False):
        if self.busy:
            raise RuntimeError("A SWF operation is already running")
        snapshot = copy.deepcopy(document)
        import threading
        self.cancel_event = threading.Event()
        self.key = document.state_key()
        self.kind = kind
        self.future = self.executor.submit(operation, snapshot, self.cancel_event) if cancellable else self.executor.submit(operation, snapshot)

    def cancel(self):
        self.cancel_event.set()
        if self.future is not None:
            self.future.cancel()
            self.future = None

    def poll(self, document: SwfEditingDocument):
        if not self.future or not self.future.done():
            return None
        future, self.future = self.future, None
        if self.key != document.state_key():
            return ("stale", "Document changed; discarded the previous result")
        try:
            return (self.kind, future.result())
        except Exception as exc:
            return ("error", str(exc))

    def close(self):
        self.cancel()
        self.executor.shutdown(wait=False, cancel_futures=True)
