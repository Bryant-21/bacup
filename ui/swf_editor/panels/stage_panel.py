"""Compiled HUD stage, display-list hierarchy and placement properties."""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
import hashlib
import threading
import time

from imgui_bundle import imgui

from ui.swf_editor.stage_runtime import render_stage
from ui.swf_editor.stage import CompiledStage, flatten


class StagePanel:
    def __init__(self, app):
        self.app = app
        self.owner, self.frame = 0, 1
        self.selection = None
        self.model = None
        self.nodes = []
        self._model_key = None
        self._nodes_key = None
        self._image_key = None
        self._requested_key = None
        self._requested_at = 0
        self._texture = None
        self._size = (1,1)
        self._executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix='swf-stage')
        self._future = None
        self._cancel = threading.Event()
        self._drag = None
        self._last_viewport_frame = -1
        self.zoom = 1.
        self.pan = (0.,0.)
        self.message = ''
        self._control_rects = {}

    def remember(self, name):
        lo, hi = imgui.get_item_rect_min(), imgui.get_item_rect_max()
        self._control_rects[name] = (lo.x,lo.y,hi.x,hi.y)

    def sync(self):
        document = self.app.document
        if document.mode != 'binary':
            self._cancel.set()
            return
        key = hashlib.sha256(document.movie).hexdigest()
        if key != self._model_key:
            self.model = CompiledStage(document.movie)
            self._model_key = key
            self._nodes_key = None
            if self.owner not in self.model.timelines:
                self.owner = 0
            self.frame = min(max(self.frame,1), max(self.model.timelines[self.owner][2],1))
        nodes_key = (key,self.owner,self.frame)
        if self._nodes_key != nodes_key:
            self.nodes = self.model.nodes(self.owner,self.frame)
            self._nodes_key = nodes_key
        settings = self.app._toolkit_settings
        font_key = (settings.active_game,settings.get_game_paths(settings.active_game).get('extracted_dir','')) if settings else None
        return (key,self.owner,self.frame,font_key)

    def selected(self):
        return next((node for node in flatten(self.nodes) if node.key == self.selection), None)

    def tick(self):
        key = self.sync()
        if self._drag and (self._drag['key'] != key or self.app.document is not self._drag['document']
                           or imgui.get_frame_count()-self._last_viewport_frame > 1):
            self._drag = None
        if self._future and self._future.done():
            future, self._future = self._future, None
            if self._running_key == key:
                try:
                    size, pixels, bounds, warnings = future.result()
                    texture = self.app.ctx.texture(size,4,pixels)
                    if self._texture:
                        self._texture.release()
                    self._texture, self._size = texture,size
                    self._render_bounds = bounds
                    self._image_key = key
                    self.message = '\n'.join(warnings[:6])
                except Exception as exc:
                    self.message = str(exc)
        if key is None:
            self._drag = None
            return
        if key != self._requested_key:
            self._cancel.set()
            self._requested_key, self._requested_at = key,time.monotonic()
            self.message = ''
        if key != self._image_key and not self._future and not self.message and time.monotonic()-self._requested_at >= .15:
            self._cancel = threading.Event()
            self._running_key = key
            self._future = self._executor.submit(render_stage,self.app.document.movie,self.frame,self.owner,
                                                self.app._toolkit_settings,self._cancel)

    def retry(self):
        self.message = ''
        self._image_key = None
        self._requested_at = 0

    def commit_matrix(self, node, matrix, *, undo=True):
        data = self.model.set_matrix(node.address,matrix)
        if data != self.app.document.movie:
            if undo:
                self.app.push_undo('Move compiled placement')
            self.app.document.movie = data
            self.app.compiled_bytes = self.app.compiled_key = None
            self.app.status_text = 'Placement changed; save project or export a new SWF'
            self.sync()

    def draw_hierarchy(self):
        self.sync()
        imgui.text('Display list')
        imgui.text_wrapped('Select an instance; double-click a sprite to inspect its timeline.')
        def draw(nodes):
            for node in nodes:
                flags = imgui.TreeNodeFlags_.open_on_arrow | imgui.TreeNodeFlags_.span_avail_width
                if node.key == self.selection:
                    flags |= imgui.TreeNodeFlags_.selected
                if not node.children:
                    flags |= imgui.TreeNodeFlags_.leaf | imgui.TreeNodeFlags_.no_tree_push_on_open
                name = node.name or self.model.symbols.get(node.character) or f'Character {node.character}'
                if node.clip_depth is not None:
                    name += f' [mask to {node.clip_depth}]'
                opened = imgui.tree_node_ex(f'{node.depth}  {name}##stage_{node.key}',flags)
                if imgui.is_item_clicked():
                    self.selection = node.key
                if imgui.is_item_hovered() and imgui.is_mouse_double_clicked(0) and node.character in self.model.timelines:
                    self.owner,self.frame,self.selection = node.character,1,None
                if node.name:
                    self.remember('node:'+node.name)
                if opened and node.children:
                    draw(node.children)
                    imgui.tree_pop()
        draw(self.nodes)
        for warning in self.model.warnings:
            imgui.text_wrapped(warning)

    def draw_timeline(self):
        self.sync()
        owners = [0]+sorted(self.model.timelines.keys()-{0})
        labels = ['Stage']+[f'{value}: {self.model.symbols.get(value,"Sprite")}' for value in owners[1:]]
        changed, index = imgui.combo('Timeline##compiled',owners.index(self.owner),labels)
        self.remember('timeline')
        if changed:
            self.owner,self.frame,self.selection = owners[index],1,None
        count = max(self.model.timelines[self.owner][2],1)
        changed, self.frame = imgui.slider_int('Frame##compiled',min(self.frame,count),1,count)
        self.remember('frame')
        if changed:
            self._drag = None
        if imgui.button('Stage root'):
            self.owner,self.frame,self.selection = 0,1,None
        imgui.text_wrapped('Authored frames are shown without executing AS3. Scripts and game callbacks can change the live HUD.')

    def draw_properties(self):
        self.sync()
        node = self.selected()
        if node is None:
            imgui.text_wrapped('Select an existing HUD element in the display list or stage. Drag it on the stage, or type coordinates with Ctrl+click on the sliders.')
            return
        imgui.text_wrapped(node.name or self.model.symbols.get(node.character) or f'Character {node.character}')
        imgui.text(f'Depth {node.depth} | Character {node.character} | Timeline {node.owner}')
        imgui.text(f'Placement begins at frame {node.placed_frame}')
        class_name = self.model.symbols.get(node.character)
        document_panel = self.app.document_panel
        if class_name in document_panel.classes and imgui.button('Inspect bound AS3'):
            document_panel.class_name = class_name
            document_panel.method = ''
            document_panel._select_inspector_tab = True
            document_panel._focus_next = True
        if node.owner:
            imgui.text_wrapped('This edits a shared sprite definition and affects all its instances. Move its parent to reposition only one instance.')
        imgui.text_wrapped('Changes start at this placement key and persist until the next matrix key or removal.')
        matrix = node.matrix
        for label,attribute,factor,low,high in [('X','translate_x',20,-8192,8192),('Y','translate_y',20,-8192,8192),
                     ('Scale X','scale_x',1,-8,8),('Scale Y','scale_y',1,-8,8),
                     ('Skew Y','rotate_skew_0',1,-8,8),('Skew X','rotate_skew_1',1,-8,8)]:
            changed,value = imgui.slider_float(label+'##compiled',getattr(matrix,attribute)/factor,low,high)
            self.remember(label)
            if imgui.is_item_activated():
                self.app.push_undo('Transform compiled placement')
            if changed:
                matrix = replace(matrix,**{attribute:value*factor})
                try:
                    self.commit_matrix(node,matrix,undo=False)
                except Exception as exc:
                    self.app.report_error(exc)

    def draw_viewport(self):
        self.sync()
        self._last_viewport_frame = imgui.get_frame_count()
        if imgui.button('Refresh stage'):
            self.retry()
        self.remember('refresh')
        imgui.same_line()
        if imgui.button('Fit'):
            self.zoom,self.pan = 1.,(0.,0.)
        imgui.text_wrapped('Authored layout | Drag to move | Alt-click selects deeper content | Wheel zoom | Middle drag pan')
        if self.message:
            imgui.text_wrapped(self.message)
        elif self._image_key != self._requested_key:
            imgui.text_disabled('Rendering compiled stage...')
        if not self._texture or self._image_key != self._requested_key:
            if self._texture and self._image_key and self._image_key[1:3] == self._requested_key[1:3]:
                imgui.text_disabled('Previous revision shown while updating')
            else:
                imgui.text_wrapped('The hierarchy and coordinates are available in Layers, Properties and Timeline.')
                return
        origin = imgui.get_cursor_screen_pos()
        size = imgui.get_content_region_avail()
        stage_rect = self._render_bounds
        if stage_rect is None:
            stage_rect = (0,0,self._size[0],self._size[1])
        x0,y0,x1,y1 = stage_rect
        scale = min(max(size.x,1)/max(x1-x0,1),max(size.y,1)/max(y1-y0,1))*self.zoom
        offset = (origin.x+self.pan[0]+(size.x-(x1-x0)*scale)/2,
                  origin.y+self.pan[1]+(size.y-(y1-y0)*scale)/2)
        self._stage_geometry = (offset,scale,stage_rect)
        imgui.invisible_button('Compiled stage##swf',size)
        self.remember('stage')
        draw = imgui.get_window_draw_list()
        draw.push_clip_rect(origin,imgui.ImVec2(origin.x+size.x,origin.y+size.y),True)
        draw.add_image(imgui.ImTextureRef(self._texture.glo),imgui.ImVec2(*offset),
                       imgui.ImVec2(offset[0]+(x1-x0)*scale,offset[1]+(y1-y0)*scale))
        selected = self.selected()
        if selected and selected.bounds:
            a,b,c,d = selected.bounds
            dx,dy = self._drag['delta'] if self._drag else (0,0)
            draw.add_rect(imgui.ImVec2(offset[0]+(a-x0+dx)*scale,offset[1]+(b-y0+dy)*scale),
                          imgui.ImVec2(offset[0]+(c-x0+dx)*scale,offset[1]+(d-y0+dy)*scale),0xFF40C7FF,thickness=2.)
        draw.pop_clip_rect()
        io = imgui.get_io()
        hovered = imgui.is_item_hovered()
        if hovered and io.mouse_wheel:
            self.zoom = max(.05,min(20,self.zoom*1.15**io.mouse_wheel))
        if hovered and io.mouse_down[2]:
            self.pan = (self.pan[0]+io.mouse_delta.x,self.pan[1]+io.mouse_delta.y)
        wx,wy = (io.mouse_pos.x-offset[0])/scale+x0,(io.mouse_pos.y-offset[1])/scale+y0
        if hovered and imgui.is_mouse_clicked(0) and self._image_key == self._requested_key:
            candidates = [n for n in flatten(self.nodes) if n.bounds and n.bounds[0] <= wx <= n.bounds[2]
                          and n.bounds[1] <= wy <= n.bounds[3] and n.clip_depth is None]
            hit = next((n for n in candidates if n.key == self.selection),None)
            if hit is None or io.key_alt:
                hit = min(candidates,key=lambda n:(-len(n.key) if io.key_alt else len(n.key),-n.depth),default=None)
            self.selection = hit.key if hit else None
            if hit:
                self._drag = {'node':hit,'start':(wx,wy),'delta':(0,0),'document':self.app.document,
                              'key':self.sync()}
        if self._drag:
            drag = self._drag
            drag['delta'] = (wx-drag['start'][0],wy-drag['start'][1])
            if imgui.is_key_pressed(imgui.Key.escape) or self.app.document is not drag['document'] or self.sync() != drag['key']:
                self._drag = None
            elif imgui.is_mouse_released(0):
                self._drag = None
                dx,dy = drag['delta']
                if abs(dx)+abs(dy) > .05:
                    node = drag['node']
                    parent = node.parent_matrix
                    det = parent.scale_x*parent.scale_y-parent.rotate_skew_0*parent.rotate_skew_1
                    try:
                        if abs(det) < 1e-8:
                            raise ValueError('Cannot move through a singular parent transform')
                        local_x = (parent.scale_y*dx-parent.rotate_skew_1*dy)/det
                        local_y = (-parent.rotate_skew_0*dx+parent.scale_x*dy)/det
                        self.commit_matrix(node,replace(node.matrix,translate_x=node.matrix.translate_x+local_x*20,
                                                       translate_y=node.matrix.translate_y+local_y*20))
                    except Exception as exc:
                        self.app.report_error(exc)

    def close(self):
        self._cancel.set()
        self._executor.shutdown(wait=False,cancel_futures=True)
        if self._texture:
            self._texture.release()
            self._texture = None
