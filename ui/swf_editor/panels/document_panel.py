"""Source editing, binary structure, diagnostics and compiled preview tools."""
from __future__ import annotations

import json
from pathlib import Path
import re

from imgui_bundle import imgui, imgui_color_text_edit

from creation_lib.swf import native_runtime
from ui.swf_editor.document import atomic_write, binary_scene
from ui.swf_editor.movie import Movie, split_tags

TAG_NAMES = {0: "End", 1: "ShowFrame", 2: "DefineShape", 4: "PlaceObject", 6: "DefineBits",
             9: "Background", 10: "DefineFont", 11: "DefineText", 12: "DoAction (AS1/2)",
             14: "DefineSound", 20: "DefineBitsLossless", 21: "DefineBitsJPEG2", 22: "DefineShape2",
             26: "PlaceObject2", 28: "RemoveObject2", 32: "DefineShape3", 33: "DefineText2",
             35: "DefineBitsJPEG3", 36: "DefineBitsLossless2", 37: "DefineEditText", 39: "DefineSprite",
             43: "FrameLabel", 48: "DefineFont2", 69: "FileAttributes", 70: "PlaceObject3",
             72: "DoABC", 75: "DefineFont3", 76: "SymbolClass", 77: "Metadata", 82: "DoABC",
             83: "DefineShape4", 87: "DefineBinaryData", 91: "DefineFont4"}


class DocumentPanel:
    def __init__(self, app):
        self.app = app
        self.editor = imgui_color_text_edit.TextEditor()
        # The bundled widget has no AS3 lexer; C++ provides familiar syntax
        # coloring while compilation remains the native AS3 compiler's job.
        self.editor.set_language(imgui_color_text_edit.TextEditor.Language.cpp())
        # The widget pairs each character independently in a queued input burst,
        # which can leave an extra ')' when several characters arrive together.
        self.editor.set_complete_paired_glyphs(False)
        self.manifest_editor = imgui_color_text_edit.TextEditor()
        self.manifest_editor.set_language(imgui_color_text_edit.TextEditor.Language.json())
        self.trace_threshold = 128
        self.reset()

    def reset(self):
        self.source_name = ""
        self.new_source_name = "B21_NewClass.as"
        self._editor_source = None
        self._select_source_tab = False
        self._select_inspector_tab = False
        self._focus_next = False
        self._method_choices_key = None
        self._method_choices = []
        self._manifest_key = None
        self.classes = []
        self.symbols = []
        self.tags = []
        self.movie_info = None
        self.class_name = ""
        self.method = ""
        self.inspection = "Build a source project or open a binary SWF to inspect compiled classes."
        self.tag_index = 0
        self.patch_pattern = '[["getlex", "Example"]]'
        self.patch_replacement = '[["getlex", "Replacement"]]'
        self.prefix = ""
        self.dependencies = []
        self.preview_symbol = ""
        self.preview_frame = 1
        self._png_texture = None
        self._png_size = (1, 1)
        self._donor_bytes = None
        self._donor_names = []
        self._donor_selected = set()
        self.refresh_structure()

    def available_movie(self):
        if self.app.compiled_key == self.app.document.state_key() and self.app.compiled_bytes:
            return self.app.compiled_bytes
        if self.app.document.mode == "binary":
            return self.app.document.movie
        if self.app.compiled_key == self.app.document.state_key():
            return self.app.compiled_bytes
        return None

    def refresh_structure(self):
        data = self.available_movie()
        self.classes, self.symbols, self.tags = [], [], []
        self.movie_info = None
        if data:
            try:
                movie = Movie(data)
                self.tags = movie.tags
                self.movie_info = (movie.signature.decode(), movie.version, binary_scene(movie))
                self.classes = native_runtime.abc_class_names(data)
                self.symbols = native_runtime.list_symbols(data)
                if self.class_name not in self.classes:
                    self.class_name = self.classes[0] if self.classes else ""
            except Exception as exc:
                self.app.report_error(exc)

    def attempt(self, action):
        try:
            action()
        except Exception as exc:
            self.app.report_error(exc)

    def draw(self):
        if self._focus_next:
            imgui.set_next_window_focus()
            self._focus_next = False
        visible, _ = imgui.begin("Document##swf")
        if visible:
            doc = self.app.document
            name = doc.path.name if doc.path else Path(doc.scene.file_path).name if doc.scene.file_path else "Untitled"
            imgui.text(f"{name}{' *' if doc.dirty else ''}  |  {doc.mode.title()}")
            imgui.text_wrapped(self.app.status_text)
            if imgui.begin_tab_bar("document_tabs##swf"):
                for title, draw in (("AS3 Sources", self.draw_sources), ("Structure", self.draw_structure),
                                    ("ABC Inspector", self.draw_inspector), ("Build & Preview", self.draw_build)):
                    wanted = (title == 'AS3 Sources' and self._select_source_tab or
                              title == 'ABC Inspector' and self._select_inspector_tab)
                    flags = imgui.TabItemFlags_.set_selected if wanted else imgui.TabItemFlags_.none
                    opened, _ = imgui.begin_tab_item(title, flags=flags)
                    if opened:
                        if title == 'AS3 Sources':
                            self._select_source_tab = False
                        if title == 'ABC Inspector':
                            self._select_inspector_tab = False
                        draw()
                        imgui.end_tab_item()
                imgui.end_tab_bar()
        imgui.end()

    def attach_source(self):
        from creation_lib.ui.widgets.pick_folder import pick_file
        path = pick_file("Attach ActionScript source", [("AS3 source", "*.as")])
        if not path:
            return
        text = Path(path).read_text(encoding="utf-8-sig")
        name = Path(path).name
        self.add_source(name, text)

    def add_source(self, name, text):
        doc = self.app.document
        if name in doc.sources:
            raise ValueError(f"Source {name} is already attached; edit it or remove it first")
        if doc.mode == "source":
            project = json.loads(doc.manifest_buffer)
            project.setdefault("scripts", []).append(name)
        self.app.push_undo("Add AS3 source")
        if doc.mode == "source":
            doc.manifest_buffer = json.dumps(project, indent=2)
        doc.sources[name] = text
        self.source_name = name
        self._editor_source = None

    def new_source(self):
        name = self.new_source_name.strip()
        if not re.fullmatch(r"[A-Za-z_$][A-Za-z0-9_$]*\.as", name):
            raise ValueError("Use a class filename such as B21_NewClass.as")
        classname = name[:-3]
        self.add_source(name, f"package {{\n    public class {classname} {{\n"
                        f"        public function {classname}() {{}}\n    }}\n}}\n")

    def remove_source(self):
        doc = self.app.document
        if not self.source_name:
            return
        if self.source_name in doc.manifest.get('embedded_sources', {}):
            raise ValueError('Removing embedded classes is not supported; revert the source buffer to undo its edits')
        if doc.mode == "source":
            project = json.loads(doc.manifest_buffer)
            project["scripts"] = [s for s in project.get("scripts", []) if s != self.source_name]
        self.app.push_undo("Remove AS3 source")
        if doc.mode == "source":
            doc.manifest_buffer = json.dumps(project, indent=2)
        doc.sources.pop(self.source_name, None)
        doc.manifest.get('native_method_sources',{}).pop(self.source_name,None)
        self.source_name = ""
        self._editor_source = None

    def draw_sources(self):
        doc = self.app.document
        if doc.mode == "binary":
            recovered = doc.manifest.get('embedded_sources', {})
            imgui.text_wrapped('Inspect existing embedded AS3 in ABC Inspector. Native method editing and supplied-source replacement preserve the rest of the movie. General class-source decompilation is not available.')
            if recovered:
                edited = sum(doc.sources.get(name) != text for name,text in recovered.items())
                imgui.text(f'{len(recovered)} recovered files | {edited} changed')
        if imgui.button("Attach .as source"):
            self.attempt(self.attach_source)
        imgui.same_line()
        if imgui.button("Remove source"):
            self.attempt(self.remove_source)
        imgui.same_line()
        if imgui.button("New class source"):
            self.attempt(self.new_source)
        imgui.same_line()
        imgui.set_next_item_width(220)
        _, self.new_source_name = imgui.input_text("Filename##new_as3", self.new_source_name)
        names = list(doc.sources)
        if names:
            if self.source_name not in names:
                self.source_name = names[0]
                self._editor_source = None
            changed, index = imgui.combo("Source##swf", names.index(self.source_name), names)
            if changed:
                self.source_name = names[index]
                self._editor_source = None
            text = doc.sources[self.source_name]
            method = doc.manifest.get('native_method_sources',{}).get(self.source_name)
            if method:
                imgui.text_wrapped(f"{method['class']} | {method['method']}({', '.join(method['parameters'])}):{method['returns']}")
                imgui.text_wrapped('Edit this method body. Build / Export compiles it with the original signature and preserves the other methods. Native recovery supports a subset of AVM2; inspect bytecode for unsupported methods.')
            if self._editor_source != (self.source_name, text):
                self.editor.set_text(text)
            before = self.editor.get_text()
            self.editor.render("AS3##swf", imgui.ImVec2(-1, max(imgui.get_content_region_avail().y * 0.55, 100)))
            value = self.editor.get_text()
            if value != before:
                doc.sources[self.source_name] = value
            self._editor_source = (self.source_name, doc.sources[self.source_name])
        else:
            imgui.text_wrapped("Attach source files to edit and compile AS3. New AS3 menu starts with a document class and game font setup.")
        if doc.mode == "source":
            imgui.separator()
            imgui.text("Project manifest: stage, sprites, art, scripts and SymbolClass bindings")
            if self._manifest_key != doc.manifest_buffer:
                self.manifest_editor.set_text(doc.manifest_buffer)
            before = self.manifest_editor.get_text()
            self.manifest_editor.render("Manifest##swf", imgui.ImVec2(-1, max(imgui.get_content_region_avail().y, 80)))
            value = self.manifest_editor.get_text()
            if value != before:
                doc.manifest_buffer = value
            self._manifest_key = doc.manifest_buffer
        elif doc.mode == "visual":
            imgui.text_wrapped("Visual projects compile every attached source. Set class bindings below as JSON, for example [{\"character\":0,\"class\":\"MyMenu\"}].")
            text = json.dumps(doc.manifest.get("exports", []))
            changed, value = imgui.input_text("Class bindings##swf", text)
            if changed:
                def apply():
                    bindings = json.loads(value)
                    if not isinstance(bindings, list):
                        raise ValueError("Class bindings must be a JSON array")
                    doc.manifest["exports"] = bindings
                self.attempt(apply)

    def draw_structure(self):
        data = self.available_movie()
        if data is None:
            imgui.text_wrapped("Build the current revision to view its compiled structure.")
            return
        imgui.text(f"{len(data):,} bytes | {len(self.tags)} top-level tags | {len(self.classes)} AS3 classes")
        if self.movie_info:
            signature, version, stage = self.movie_info
            imgui.text(f"{signature} v{version} | {stage.canvas_width} x {stage.canvas_height} | {stage.fps:g} FPS | {stage.total_frames} frames")
        if self.symbols and imgui.collapsing_header("Exported symbols / class bindings"):
            for character, name in self.symbols:
                clicked, _ = imgui.selectable(f"{character}: {name}", False)
                if clicked:
                    self.class_name = name
                    if character != 0:
                        self.preview_symbol = name
        if imgui.button("Refresh structure"):
            self.refresh_structure()
        imgui.begin_child("tags##swf", imgui.ImVec2(-1, 190), child_flags=imgui.ChildFlags_.borders)
        for index, tag in enumerate(self.tags):
            label = f"{index:04}  {TAG_NAMES.get(tag.code, 'Unknown')} ({tag.code})  {len(tag.payload):,} bytes"
            if tag.character_id is not None:
                label += f"  character {tag.character_id}"
            clicked, _ = imgui.selectable(label, self.tag_index == index)
            if clicked:
                self.tag_index = index
        imgui.end_child()
        self.tag_index = min(self.tag_index, max(len(self.tags) - 1, 0))
        if not self.tags:
            return
        tag = self.tags[self.tag_index]
        if tag.code == 39:
            self.attempt(lambda: self._show_sprite(tag))
        if imgui.button("Export tag payload"):
            self.attempt(self.export_tag)
        if self.app.document.mode == "binary":
            imgui.same_line()
            if imgui.button("Replace tag payload..."):
                self.attempt(self.replace_tag)
            if tag.code in {2, 22, 32, 83}:
                if imgui.button("Export vector SVG"):
                    self.attempt(self.export_shape)
                imgui.same_line()
                if imgui.button("Replace vector from SVG..."):
                    self.attempt(self.replace_shape)
            imgui.text_wrapped("Raw replacement keeps the tag code and character ID. Payloads must use that tag's binary format. Text/font/bitmap/sound have raw editing only.")
            if imgui.button("Inject exported symbols from donor SWF..."):
                self.attempt(self.inject_symbols)
            if self._donor_bytes:
                for name in self._donor_names:
                    changed, selected = imgui.checkbox(f"{name}##donor", name in self._donor_selected)
                    if changed:
                        if selected:
                            self._donor_selected.add(name)
                        else:
                            self._donor_selected.discard(name)
                if imgui.button("Inject selected symbols"):
                    data, names = self._donor_bytes, sorted(self._donor_selected)
                    if names:
                        self.app.start_binary_edit(lambda doc: native_runtime.inject_symbols(data, doc.movie, names), "Inject donor symbols")

    def _show_sprite(self, tag):
        children = split_tags(tag.payload[4:])
        imgui.text_wrapped(f"Sprite timeline: {len(children)} nested tags; " + ", ".join(str(t.code) for t in children[:100]))

    def export_tag(self):
        from creation_lib.ui.widgets.pick_folder import pick_save_file
        path = pick_save_file("Export tag payload", [("Binary payload", "*.bin")], default_ext=".bin")
        if path:
            atomic_write(Path(path), self.tags[self.tag_index].payload)

    def replace_tag(self):
        from creation_lib.ui.widgets.pick_folder import pick_file
        path = pick_file("Replace selected tag payload", [("Binary payload", "*.bin")])
        if path:
            payload, index = Path(path).read_bytes(), self.tag_index
            self.app.start_binary_edit(lambda doc: Movie(doc.movie).replace_tag(index, payload), "Replace tag")

    def selected_shape(self):
        from creation_lib.swf.tags import DefineShapeTag
        tag = self.tags[self.tag_index]
        return DefineShapeTag.parse(tag.payload, tag.code).shape

    def export_shape(self):
        from creation_lib.swf.svg_io import shape_to_svg
        from creation_lib.ui.widgets.pick_folder import pick_save_file
        path = pick_save_file("Export shape SVG", [("SVG", "*.svg")], default_ext=".svg")
        if path:
            atomic_write(Path(path), shape_to_svg(self.selected_shape(), background=False).encode("utf-8"))

    def replace_shape(self):
        from creation_lib.ui.widgets.pick_folder import pick_file
        from creation_lib.swf.svg_io import svg_to_shapes
        from creation_lib.swf.project import _merge_shapes
        path = pick_file("Replace vector shape", [("SVG", "*.svg")])
        if path:
            shapes = svg_to_shapes(Path(path).read_text(encoding="utf-8"))
            if not shapes:
                raise ValueError("SVG contains no vector paths")
            sid = self.tags[self.tag_index].character_id
            shape = _merge_shapes(shapes, sid)
            self.app.start_binary_edit(lambda doc: Movie(doc.movie).replace_shape(sid, shape), "Replace vector shape")

    def inject_symbols(self):
        from creation_lib.ui.widgets.pick_folder import pick_file
        path = pick_file("Choose symbol donor", [("SWF", "*.swf")])
        if path:
            data = Path(path).read_bytes()
            names = [name for _, name in native_runtime.list_symbols(data)]
            if not names:
                raise ValueError("Donor has no SymbolClass exports")
            self._donor_bytes, self._donor_names = data, names
            self._donor_selected = set()

    def draw_inspector(self):
        if not self.available_movie():
            imgui.text_wrapped("Build the current revision first.")
            return
        if self.classes:
            if self.class_name not in self.classes:
                self.class_name = self.classes[0]
            changed, index = imgui.combo("Class##swf", self.classes.index(self.class_name), self.classes)
            if changed:
                self.class_name = self.classes[index]
                self.method = ''
            _, self.method = imgui.input_text("Method filter##swf", self.method)
            if self.app.document.mode == 'binary':
                key = (id(self.available_movie()),self.class_name)
                if self._method_choices_key != key:
                    outline = native_runtime.abc_class_outline(self.available_movie(),self.class_name)
                    self._method_choices = [('get ' if t['kind'] == 'getter' else 'set ' if t['kind'] == 'setter' else '')+t['name']
                                            for t in outline['instance_traits'] if t['kind'] in {'method','getter','setter'}]
                    self._method_choices_key = key
                names = ['Select an existing method']+self._method_choices
                current = names.index(self.method) if self.method in names else 0
                changed,selected = imgui.combo('Existing method##swf',current,names)
                if changed:
                    self.method = names[selected] if selected else ''
                imgui.begin_disabled(self.app.jobs.busy or not self.method.strip())
                if imgui.button('Edit existing method as AS3'):
                    self.attempt(self.recover_method)
                if self.app.stage_panel:
                    self.app.stage_panel.remember('recover_method')
                imgui.end_disabled()
            for title, view in (("Outline", "outline"), ("Disassemble", "disassembly"), ("Dependencies", "dependencies")):
                if imgui.button(title):
                    self.attempt(lambda v=view: self.inspect(v))
                imgui.same_line()
            imgui.new_line()
            if (self.app.document.mode == "binary" and self.source_name in self.app.document.sources
                    and self.source_name not in self.app.document.manifest.get('native_method_sources',{})):
                imgui.text_wrapped(f"Apply {self.source_name} to {self.class_name}. Replace requires a complete class; merge keeps existing members absent from the source.")
                if imgui.button("Replace class from source"):
                    self.apply_class(False)
                imgui.same_line()
                if imgui.button("Merge supplied members"):
                    self.apply_class(True)
                if imgui.button("Add dependency SWF..."):
                    self.attempt(self.add_dependency)
                imgui.same_line()
                if imgui.button("Clear dependencies"):
                    self.dependencies.clear()
                imgui.text(f"{len(self.dependencies)} compile dependency movies (not injected)")
        else:
            imgui.text("This movie defines no AS3 classes.")
        imgui.input_text_multiline("Inspection##swf", self.inspection, imgui.ImVec2(-1, 220), imgui.InputTextFlags_.read_only)
        if self.app.document.mode == "binary" and imgui.collapsing_header("Advanced ABC operations"):
            _, self.prefix = imgui.input_text("Class name prefix", self.prefix)
            if imgui.button("Rename movie classes"):
                if self.prefix.strip():
                    prefix = self.prefix.strip()
                    self.app.start_binary_edit(lambda doc: native_runtime.rename_as3_classes(doc.movie, prefix), "Rename AS3 classes")
            _, self.patch_pattern = imgui.input_text("Opcode pattern JSON", self.patch_pattern)
            _, self.patch_replacement = imgui.input_text("Opcode replacement JSON", self.patch_replacement)
            if imgui.button("Patch selected method (exactly one match)"):
                self.attempt(self.patch_method)

    def recover_method(self):
        classname, method = self.class_name, self.method.strip()
        name = f'{classname}::{method}'
        if name in self.app.document.sources:
            self.source_name = name
            self._select_source_tab = self._focus_next = True
            self.app.status_text = 'Opened the existing method source buffer'
            return
        self.app.jobs.start(self.app.document,lambda doc: native_runtime.recover_as3_method_source(doc.movie,classname,method),
                            'native_method_source')
        self.app.status_text = 'Recovering existing method AS3 with the native backend...'

    def inspect(self, view):
        data = self.available_movie()
        if view == "disassembly":
            report = native_runtime.abc_disassemble(data, self.class_name, self.method or None)
        elif view == "dependencies":
            report = native_runtime.abc_class_references(data, self.class_name, True)
        else:
            report = native_runtime.abc_class_outline(data, self.class_name)
        self.inspection = json.dumps(report, indent=2)

    def apply_class(self, augment):
        name, text = self.class_name, self.app.document.sources[self.source_name]
        dependencies = list(self.dependencies)
        self.app.start_binary_edit(lambda doc: doc.apply_sources({name: text}, augment=augment, dependencies=dependencies),
                                   "Merge AS3 members" if augment else "Replace AS3 class")

    def add_dependency(self):
        from creation_lib.ui.widgets.pick_folder import pick_file
        path = pick_file("Compiler dependency movie", [("SWF", "*.swf")])
        if path:
            data = Path(path).read_bytes()
            Movie(data)
            self.dependencies.append(data)

    def patch_method(self):
        pattern, replacement = json.loads(self.patch_pattern), json.loads(self.patch_replacement)
        name, method = self.class_name, self.method
        if not name or not method:
            raise ValueError("Choose a class and a specific method for patching")
        self.app.start_binary_edit(lambda doc: native_runtime.patch_as3_method(doc.movie, name, method, pattern, replacement,
                                                                             expected_matches=1), "Patch AS3 method")

    def draw_build(self):
        imgui.begin_disabled(self.app.jobs.busy)
        if imgui.button("Build / validate current revision"):
            self.app.start_build()
        imgui.same_line()
        if imgui.button("Export SWF..."):
            self.app.choose_export()
        imgui.end_disabled()
        if self.app.jobs.busy:
            if imgui.button("Cancel operation"):
                self.app.jobs.cancel()
                self.app.status_text = "Operation cancelled; its result will be discarded"
        _, self.app.preview_player = imgui.input_text("Ruffle executable", self.app.preview_player)
        if imgui.button("Choose Ruffle..."):
            def choose():
                from creation_lib.ui.widgets.pick_folder import pick_file
                path = pick_file("Ruffle desktop player", [("Executable", "*.exe")])
                if path:
                    self.app.preview_player = path
            self.attempt(choose)
        imgui.same_line()
        if imgui.button("Compile & play in Ruffle"):
            self.app.start_build(preview=True)
        imgui.same_line()
        if imgui.button("Close preview"):
            self.attempt(self.app.close_preview)
        imgui.text_wrapped("Ruffle runs a disposable compiled copy. Game fonts, Scaleform input, native callbacks and item delivery require in-game verification. The canvas shows vector artwork and does not execute AS3.")
        if self.app._preview_log is not None and imgui.collapsing_header("Player output"):
            imgui.text_wrapped(self.app.preview_output() or "Player is running; no output yet.")
        imgui.separator()
        imgui.text_wrapped(self.app.document.diagnostics or "Compiler diagnostics will appear here. Builds use source buffers; external .as files remain untouched.")
        imgui.separator()
        _, self.preview_symbol = imgui.input_text("Exported artwork symbol", self.preview_symbol)
        _, self.preview_frame = imgui.slider_int("Symbol frame", self.preview_frame, 1, 1000)
        if imgui.button("Render symbol PNG..."):
            self.attempt(self.export_symbol_png)
        imgui.text_wrapped("Symbol rendering supports vector/sprite art, gradients and supported bitmaps. Filters, masks, text and script-generated drawing may be unsupported; renderer errors are shown above.")
        _, self.trace_threshold = imgui.slider_int("Raster trace threshold", self.trace_threshold, 0, 255)
        if imgui.button("Import SVG / trace raster..."):
            self.app.choose_import()
        imgui.same_line()
        if imgui.button("Export selected SVG..."):
            self.app.export_svg()

    def export_symbol_png(self):
        from creation_lib.ui.widgets.pick_folder import pick_save_file
        data = self.available_movie()
        if data is None:
            raise ValueError("Build the current revision before rendering a symbol")
        if not self.preview_symbol or not any(sid != 0 and name == self.preview_symbol for sid, name in self.symbols):
            raise ValueError("Choose an exported artwork character (document class 0 has no static symbol preview)")
        png = native_runtime.render_symbol_png(data, self.preview_symbol, self.preview_frame)
        path = pick_save_file("Export rendered symbol", [("PNG", "*.png")], default_ext=".png")
        if path:
            atomic_write(Path(path), png)
            self.app.status_text = f"Rendered {Path(path).name}"
