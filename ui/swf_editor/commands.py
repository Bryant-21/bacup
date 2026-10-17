"""SWF workspace commands, revision checks and unsaved-edit protection."""
from __future__ import annotations

import copy
from pathlib import Path
import shutil
import subprocess
import tempfile

from imgui_bundle import imgui

from ui.swf_editor.document import RevisionJob, SwfEditingDocument, atomic_write


class DocumentActions:
    def _init_document(self):
        self.document = SwfEditingDocument()
        self.jobs = RevisionJob()
        self.compiled_bytes = None
        self.compiled_key = None
        self._pending_action = None
        self._pending_for_exit = False
        self._pending_draw_frame = -1
        self._saved_document = copy.deepcopy(self.document)
        self._exit_cancelled = False
        self._exit_ready = False
        self._exit_ready_state = None
        self._preview_directory = None
        self._preview_process = None
        self._preview_log = None
        self._preview_failure_reported = False
        self.preview_player = shutil.which("ruffle") or ""
        self._captured_tool = None
        self._fit_on_next_viewport = True
        self._gesture_snapshot = None
        self._gesture_undo_count = 0

    @property
    def scene(self):
        return self.document.scene

    @scene.setter
    def scene(self, value):
        self.document.scene = value

    def report_error(self, error):
        self.status_text = f"Error: {error}"
        self.document.diagnostics = str(error)

    def request_action(self, action):
        if self._pending_action is not None:
            return
        if self.document.dirty or self.jobs.busy:
            self._pending_action = action
        else:
            action()

    def _replace_document(self, document):
        self.jobs.cancel()
        self.document = document
        self._saved_document = copy.deepcopy(document)
        self.compiled_bytes = self.compiled_key = None
        self.undo_stack.clear()
        self._captured_tool = None
        if self._tools_initialized:
            self._init_tools()
        self.camera.canvas_width = self.scene.canvas_width
        self.camera.canvas_height = self.scene.canvas_height
        self.camera.fit_canvas()
        self._fit_on_next_viewport = True
        if self.renderer:
            self.renderer.clear_cache()
        if getattr(self, "document_panel", None):
            self.document_panel.reset()

    def new_document(self, menu=False):
        def create():
            self._replace_document(SwfEditingDocument.menu() if menu else SwfEditingDocument())
            self.status_text = "New AS3 menu" if menu else "New vector artwork"
        self.request_action(create)

    def open_document(self, path):
        def load():
            try:
                document = SwfEditingDocument.open(path)
                self._replace_document(document)
                self.status_text = f"Opened {Path(path).name} ({document.mode})"
            except Exception as exc:
                self.report_error(exc)
        self.request_action(load)

    def choose_open(self):
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file
            path = pick_file("Open SWF or project", [("SWF / projects", "*.swf;*.swfproj"), ("All files", "*.*")])
            if path:
                self.open_document(path)
        except Exception as exc:
            self.report_error(exc)

    def save_document(self, save_as=False):
        try:
            target = self.document.path
            if save_as or target is None:
                from creation_lib.ui.widgets.pick_folder import pick_save_file
                target = pick_save_file("Save SWF editing project", [("SWF project", "*.swfproj")], default_ext=".swfproj")
            if not target:
                return False
            self.document.save(target)
            self._saved_document = copy.deepcopy(self.document)
            self.status_text = f"Saved {Path(target).name}"
            return True
        except Exception as exc:
            self.report_error(exc)
            return False

    def start_build(self, *, export_path=None, preview=False):
        try:
            if export_path:
                self.document.check_export_target(export_path)
            kind = "preview" if preview else (f"export:{export_path}" if export_path else "build")
            self.jobs.start(self.document, lambda snapshot, cancel: snapshot.build(cancel), kind, cancellable=True)
            self.status_text = "Compiling current revision..."
        except Exception as exc:
            self.report_error(exc)

    def choose_export(self):
        try:
            from creation_lib.ui.widgets.pick_folder import pick_save_file
            path = pick_save_file("Export compiled SWF", [("SWF movie", "*.swf")], default_ext=".swf")
            if path:
                self.start_build(export_path=path)
        except Exception as exc:
            self.report_error(exc)

    def start_binary_edit(self, operation, label):
        try:
            self.jobs.start(self.document, operation, f"edit:{label}")
            self.status_text = f"{label}..."
        except Exception as exc:
            self.report_error(exc)

    def poll(self):
        if (self._preview_process is not None and self._preview_process.poll() not in (None, 0)
                and not self._preview_failure_reported):
            self._preview_failure_reported = True
            self.report_error(f"Ruffle exited with code {self._preview_process.returncode}. {self.preview_output()}")
        result = self.jobs.poll(self.document)
        if not result:
            return
        kind, value = result
        if kind == "error":
            self.report_error(value)
            return
        if kind == "stale":
            self.status_text = value
            return
        try:
            if kind == 'native_method_source':
                name = f"{value['class']}::{value['method']}"
                if name in self.document.sources:
                    raise ValueError('This method already has a source buffer; select it in AS3 Sources')
                self.push_undo('Recover existing method AS3')
                self.document.sources[name] = value['body']
                self.document.manifest.setdefault('native_method_sources',{})[name] = {
                    'class':value['class'],'method':value['method'],'baseline':value['body'],
                    'parameters':value['parameters'],'returns':value['returns']}
                if self.document_panel:
                    self.document_panel.source_name = name
                    self.document_panel._editor_source = None
                    self.document_panel._select_source_tab = True
                    self.document_panel._focus_next = True
                self.status_text = 'Existing method recovered; edit its body and Build / Export to compile that method'
            elif kind.startswith("edit:"):
                from ui.swf_editor.movie import Movie
                Movie(value)
                self.push_undo(kind[5:])
                self.document.movie = value
                self.compiled_bytes = self.compiled_key = None
                self.status_text = f"{kind[5:]} complete; save the project or export a new SWF"
                if getattr(self, "document_panel", None):
                    self.document_panel.refresh_structure()
            else:
                self.compiled_bytes = value
                self.compiled_key = self.document.state_key()
                from creation_lib.swf import native_runtime
                missing = native_runtime.unbacked_symbol_classes(value)
                self.document.diagnostics = (f"Unbacked SymbolClass bindings: {missing}" if missing else
                                             "Build completed. All SymbolClass exports are backed by AS3 classes.\n"
                                             "Scaleform acceptance still requires testing in game.")
                if kind.startswith("export:"):
                    atomic_write(Path(kind[7:]), value)
                    self.status_text = f"Exported {Path(kind[7:]).name} ({len(value):,} bytes)"
                elif kind == "preview":
                    self.launch_preview(value)
                else:
                    self.status_text = f"Built {len(value):,} bytes"
                if getattr(self, "document_panel", None):
                    self.document_panel.refresh_structure()
        except Exception as exc:
            self.report_error(exc)

    def launch_preview(self, data):
        if not self.preview_player or not Path(self.preview_player).is_file():
            raise ValueError("Choose a Ruffle desktop executable in Build & Preview")
        self.close_preview()
        self._preview_directory = tempfile.TemporaryDirectory(prefix="modbox-swf-preview-")
        directory = Path(self._preview_directory.name)
        movie = directory / "preview.swf"
        movie.write_bytes(data)
        self._preview_log = directory / "ruffle.log"
        self._preview_failure_reported = False
        # This is an explicitly requested interactive player window. Keep all
        # incidental config/cache/storage in the disposable preview directory.
        with self._preview_log.open("wb") as log:
            self._preview_process = subprocess.Popen(
                [self.preview_player, "--storage", "memory", "--config", str(directory / "config"),
                 "--cache-directory", str(directory / "cache"), "--filesystem-access-mode", "deny",
                 "--tcp-connections", "deny", "--open-url-mode", "deny", str(movie)],
                cwd=directory, stdout=log, stderr=log)
        self.status_text = "Ruffle preview opened for the compiled revision"

    def preview_output(self):
        if self._preview_log is not None and self._preview_log.is_file():
            with self._preview_log.open("rb") as handle:
                handle.seek(max(self._preview_log.stat().st_size - 8192, 0))
                return handle.read().decode("utf-8", errors="replace")
        return ""

    def close_preview(self):
        if self._preview_process is not None and self._preview_process.poll() is None:
            self._preview_process.terminate()
            try:
                self._preview_process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self._preview_process.kill()
                self._preview_process.wait(timeout=2)
        self._preview_process = None
        self._preview_log = None
        if self._preview_directory is not None:
            self._preview_directory.cleanup()
            self._preview_directory = None

    def exit_confirmation_state(self):
        self.poll()
        return (id(self.document), self.document.state_key(), self.document.saved_state,
                str(self.document.path), self.jobs.busy,
                id(self.jobs.future) if self.jobs.future is not None else None)

    def reset_exit_confirmation(self):
        if self._pending_for_exit:
            opened, _ = imgui.begin_popup_modal("Unsaved SWF edits##swf", flags=imgui.WindowFlags_.always_auto_resize)
            if opened:
                imgui.close_current_popup()
                imgui.end_popup()
            self._pending_action = None
            self._pending_for_exit = False
            self._pending_draw_frame = -1
        self._exit_cancelled = self._exit_ready = False
        self._exit_ready_state = None

    def confirm_exit(self):
        if self._exit_cancelled:
            self._exit_cancelled = False
            return False
        if self._exit_ready:
            self._exit_ready = False
            approved = self._exit_ready_state
            self._exit_ready_state = None
            if approved == self.exit_confirmation_state():
                return True
        if not self.document.dirty and not self.jobs.busy:
            return True
        if self._pending_action is None:
            if imgui.is_popup_open("", imgui.PopupFlags_.any_popup_id | imgui.PopupFlags_.any_popup_level):
                return None
            self._pending_for_exit = True
            self._pending_action = lambda: setattr(self, "_exit_ready", True)
        self.draw_pending_dialog()
        return None

    def resolve_pending(self, choice):
        if self._pending_action is None:
            return False
        if choice == "cancel":
            self._pending_action = None
            self._exit_cancelled = self._pending_for_exit
            self._pending_for_exit = False
            return True
        if choice == "save" and not self.save_document():
            return False
        if choice not in {"save", "discard"}:
            return False
        action, self._pending_action = self._pending_action, None
        for_exit = self._pending_for_exit
        self._pending_for_exit = False
        self.jobs.cancel()
        if for_exit:
            # Quit-time discard is approval, not an immediate document reset:
            # a later workspace can still cancel the overall transaction.
            self._exit_ready_state = self.exit_confirmation_state()
        action()
        return True

    def draw_pending_dialog(self):
        if self._pending_action is None:
            return
        frame = imgui.get_frame_count()
        if self._pending_draw_frame == frame:
            return
        if (not imgui.is_popup_open("Unsaved SWF edits##swf")
                and imgui.is_popup_open("", imgui.PopupFlags_.any_popup_id | imgui.PopupFlags_.any_popup_level)):
            return
        self._pending_draw_frame = frame
        imgui.open_popup("Unsaved SWF edits##swf")
        opened, _ = imgui.begin_popup_modal("Unsaved SWF edits##swf", flags=imgui.WindowFlags_.always_auto_resize)
        if opened:
            imgui.text_wrapped("Save this editing project before continuing? Exporting a SWF does not save the source project.")
            if self.jobs.busy:
                imgui.text_wrapped("A build is running. Continuing will discard its result.")
            for label, choice in (("Save project", "save"), ("Discard edits", "discard"), ("Cancel", "cancel")):
                if imgui.button(label) and self.resolve_pending(choice):
                    imgui.close_current_popup()
                imgui.same_line()
            imgui.new_line()
            if self.document.diagnostics.startswith("Error") or self.status_text.startswith("Error"):
                imgui.text_wrapped(self.status_text)
            imgui.end_popup()

    def import_art(self, path, threshold=128):
        if self.document.mode != "visual":
            raise ValueError("Use Replace vector shape in Structure for binary movies; source projects name SVG assets in their manifest")
        from creation_lib.swf.svg_io import svg_to_shapes
        from creation_lib.swf.trace import TraceSettings, trace_image_file
        from ui.swf_editor.swf_scene import EditorDisplayEntry, Keyframe
        shapes = (svg_to_shapes(Path(path).read_text(encoding="utf-8")) if Path(path).suffix.lower() == ".svg"
                  else trace_image_file(str(path), TraceSettings(threshold=threshold)))
        if not shapes:
            raise ValueError("Import produced no drawable paths")
        layer = self.scene.layers[self.scene.active_layer_index]
        if layer.locked:
            raise ValueError("The active layer is locked")
        self.push_undo("Import artwork")
        keyframe = layer.keyframe_at(self.scene.current_frame)
        if keyframe is None:
            keyframe = Keyframe(frame=self.scene.current_frame)
            layer.keyframes.append(keyframe)
            layer.keyframes.sort(key=lambda k: k.frame)
        for shape in shapes:
            sid = max(self.scene.library_symbols, default=0) + 1
            shape.shape_id = sid
            self.scene.library_symbols[sid] = shape
            keyframe.display_list.append(EditorDisplayEntry(shape_id=sid))
        self.status_text = f"Imported {len(shapes)} vector shapes"

    def choose_import(self):
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file
            path = pick_file("Import SVG or trace raster", [("Artwork", "*.svg;*.png;*.jpg;*.jpeg;*.bmp;*.dds")])
            if path:
                self.import_art(path, getattr(self.document_panel, "trace_threshold", 128))
        except Exception as exc:
            self.report_error(exc)

    def export_svg(self):
        try:
            from creation_lib.swf.svg_io import shape_to_svg
            from creation_lib.ui.widgets.pick_folder import pick_save_file
            if len(self.scene.selection) != 1:
                raise ValueError("Select one vector shape to export as SVG")
            li, ei = self.scene.selection[0]
            entry = self.scene.layers[li].keyframe_at(self.scene.current_frame).display_list[ei]
            path = pick_save_file("Export selected vector shape", [("SVG", "*.svg")], default_ext=".svg")
            if path:
                atomic_write(Path(path), shape_to_svg(self.scene.library_symbols[entry.shape_id], background=False).encode("utf-8"))
                self.status_text = f"Exported {Path(path).name}"
        except Exception as exc:
            self.report_error(exc)

    def snapshot_document(self):
        return copy.deepcopy(self.document)

    def cancel_gesture(self):
        if self._captured_tool is not None and self._gesture_snapshot is not None:
            self._restore_snapshot(self._gesture_snapshot)
            del self.undo_stack._undo[self._gesture_undo_count:]
        self._captured_tool = self._gesture_snapshot = None
        if self._tools_initialized:
            self._init_tools()

    def select_tool(self, name):
        if self.active_tool != name:
            self.cancel_gesture()
            self.active_tool = name

    def outline_stroke(self):
        import pyclipper
        from creation_lib.swf.svg_io import _bucket_edges, _stitch, _compute_bounds
        from creation_lib.swf.project import _merge_shapes
        from creation_lib.swf.shapes import ShapeDef, StyleChange, StraightEdge, EndShape
        from creation_lib.swf.types import FillStyle
        from ui.swf_editor.swf_renderer import _subdivide_quadratic
        replacements = []
        for li, ei in self.scene.selection:
            layer = self.scene.layers[li]
            if layer.locked:
                raise ValueError("A selected layer is locked")
            entry = layer.keyframe_at(self.scene.current_frame).display_list[ei]
            shape = self.scene.library_symbols[entry.shape_id]
            if not shape.line_styles:
                continue
            fill_shape = copy.deepcopy(shape)
            fill_shape.line_styles = []
            for record in fill_shape.records:
                if isinstance(record, StyleChange):
                    record.line = 0
                    if record.new_line_styles:
                        raise ValueError("Outline Stroke does not support mid-shape line-style tables")
            parts = [fill_shape]
            _, strokes = _bucket_edges(shape)
            for style, segments in strokes.values():
                if style.color is None:
                    raise ValueError("Outline Stroke supports solid stroke colors")
                offset = pyclipper.PyclipperOffset()
                for contour in _stitch(segments):
                    path = [(contour[0][0] / 20, contour[0][1] / 20)]
                    for sx, sy, ex, ey, cx, cy in contour:
                        path.extend([(ex / 20, ey / 20)] if cx is None else
                                    _subdivide_quadratic(sx / 20, sy / 20, cx / 20, cy / 20, ex / 20, ey / 20)[1:])
                    if len(path) >= 2:
                        closed = path[0] == path[-1]
                        offset.AddPath(pyclipper.scale_to_clipper(path[:-1] if closed else path), pyclipper.JT_ROUND,
                                       pyclipper.ET_CLOSEDLINE if closed else pyclipper.ET_OPENROUND)
                records = []
                for path in pyclipper.scale_from_clipper(offset.Execute(pyclipper.scale_to_clipper(max(style.width / 20, 1) / 2))):
                    points = [(round(x * 20), round(y * 20)) for x, y in path]
                    if len(points) < 3:
                        continue
                    records.append(StyleChange(move_x=points[0][0], move_y=points[0][1], fill1=1))
                    records.extend(StraightEdge(dx=b[0] - a[0], dy=b[1] - a[1])
                                   for a, b in zip(points, points[1:] + points[:1]))
                if records:
                    records.append(EndShape())
                    parts.append(ShapeDef(shape_id=1, bounds=_compute_bounds(records),
                                          records=records, fill_styles=[FillStyle(fill_type=0, color=copy.deepcopy(style.color))], line_styles=[]))
            replacements.append((entry, _merge_shapes(parts, 1)))
        if not replacements:
            raise ValueError("Select a shape with a stroke to outline")
        self.push_undo("Outline stroke")
        for entry, shape in replacements:
            shape.shape_id = max(self.scene.library_symbols, default=0) + 1
            self.scene.library_symbols[shape.shape_id] = shape
            entry.shape_id = shape.shape_id
