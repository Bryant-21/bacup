"""Focused recipe transfer form with clipped mapping and fixed review actions."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.forms import pick_file
from creation_lib.ui.widgets.modern import action_button, heading, scaled


class ProfileTransfersUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.path, self.name, self.query = "", "", ""
        self.config_name = ""
        self.mapping = {}
        self.seen, self.reviewed = None, None
        self.ack = False
        self._candidate_key, self._candidates = None, ()
        self._available_key, self._available = None, {}
        self.payloads, self.payload_ids, self.positions = (), set(), {}
        self._mapping_tuple, self._missing = (), 0

    def _mapping_changed(self):
        self._mapping_tuple = tuple(sorted(self.mapping.items()))
        self._missing = len(self.payload_ids - self.mapping.keys())

    def draw(self):
        w, m = self.workspace, self.workspace.manager
        heading("Portable profile recipes", large=True)
        imgui.text_wrapped("Transfer activation, priority, groups and local configuration to a new profile. Map payloads already present in the destination library. Mod payloads, saves, executable definitions, credential stores and source-folder metadata stay separate; allowlisted configuration text is retained.")
        footer = imgui.get_frame_height_with_spacing() * 4
        imgui.begin_child("recipe_details", (0, max(scaled(100), imgui.get_content_region_avail().y - footer)))
        try:
            imgui.begin_disabled(m.busy or m.runtime_active)
            try:
                if action_button("Export current profile recipe"):
                    w._action(m.export_recipe)
                if m.recipe_export:
                    imgui.set_next_item_width(-1)
                    imgui.input_text("##recipe_export_path", m.recipe_export, imgui.InputTextFlags_.read_only)
                imgui.separator()
                imgui.set_next_item_width(-1)
                _, self.path = imgui.input_text_with_hint("##recipe_path", "A .modbox-profile.json recipe", self.path)
                if action_button("Choose recipe"):
                    path = pick_file("Choose a profile recipe", [("Profile recipe", "*.modbox-profile.json *.json")])
                    if path:
                        self.path = str(path)
                imgui.same_line()
                if action_button("Read recipe", enabled=bool(self.path.strip())):
                    w._action(lambda: m.inspect_recipe(self.path))
                recipe = m.recipe
                if not recipe:
                    return
                if self.seen is not recipe:
                    self.seen = recipe
                    self.mapping, self.name, self.ack = dict(m.recipe_suggestions), recipe.name[:185] + " (transferred)", False
                    self.query = ""
                    self.config_name = recipe.files[0][0] if recipe.files else ""
                    self.payloads = tuple(entry for entry in recipe.entries if entry.kind != "separator")
                    self.payload_ids = {entry.id for entry in self.payloads}
                    self.positions = {entry.id: index for index, entry in enumerate(recipe.entries)}
                    self._mapping_changed()
                payloads, missing = self.payloads, self._missing
                imgui.text_wrapped(f"{recipe.name} | {recipe.game}\n{len(payloads):,} payload references, {len(recipe.entries) - len(payloads):,} local separators, {len(recipe.files)} configuration files\n{missing:,} unmapped; names are never guessed. Separators will be created as local metadata.")
                imgui.set_next_item_width(-1)
                _, self.name = imgui.input_text("New profile name", self.name)
                if imgui.collapsing_header("Map recipe mods", imgui.TreeNodeFlags_.default_open if missing else 0):
                    imgui.set_next_item_width(-1)
                    _, self.query = imgui.input_text_with_hint("##recipe_candidate_query", "Filter available local mods", self.query)
                    key = (id(m.recipe_candidates), self.query.casefold())
                    if key != self._candidate_key:
                        self._candidate_key = key
                        self._candidates = tuple(row for row in m.recipe_candidates if self.query.casefold() in row.name.casefold())
                    if self._available_key is not m.recipe_candidates:
                        self._available_key = m.recipe_candidates
                        self._available = {row.id: row for row in m.recipe_candidates}
                    available = self._available
                    height = imgui.get_frame_height_with_spacing() + imgui.get_text_line_height_with_spacing()
                    imgui.begin_child("recipe_mappings", (0, scaled(180)), imgui.ChildFlags_.borders)
                    try:
                        clipper = imgui.ListClipper()
                        clipper.begin(len(payloads), height)
                        while clipper.step():
                            for index in range(clipper.display_start, clipper.display_end):
                                entry = payloads[index]
                                imgui.push_id(entry.id)
                                try:
                                    imgui.text_unformatted(entry.name)
                                    imgui.set_item_tooltip(f"Recipe identity: {entry.id}\nVersion: {entry.version or 'Unspecified'}\nPriority: {self.positions[entry.id]} | Enabled: {entry.enabled}\nGroup: {entry.group or 'None'}")
                                    target = available.get(self.mapping.get(entry.id))
                                    imgui.set_next_item_width(-1)
                                    if imgui.begin_combo("##recipe_mapping", target.name if target else "Unmapped"):
                                        try:
                                            if imgui.selectable("Unmapped", target is None)[0]:
                                                self.mapping.pop(entry.id, None)
                                                self._mapping_changed()
                                            choices = imgui.ListClipper()
                                            choices.begin(len(self._candidates))
                                            while choices.step():
                                                for candidate_index in range(choices.display_start, choices.display_end):
                                                    candidate = self._candidates[candidate_index]
                                                    imgui.push_id(candidate.id)
                                                    try:
                                                        if imgui.selectable("##map_local_mod", candidate.id == self.mapping.get(entry.id))[0]:
                                                            self.mapping[entry.id] = candidate.id
                                                            self._mapping_changed()
                                                        imgui.get_window_draw_list().add_text(imgui.get_item_rect_min(), imgui.get_color_u32(imgui.Col_.text), candidate.name)
                                                        imgui.set_item_tooltip(f"{candidate.kind} | {candidate.source or 'Unavailable folder'}\nIdentity: {candidate.id}")
                                                    finally:
                                                        imgui.pop_id()
                                        finally:
                                            imgui.end_combo()
                                finally:
                                    imgui.pop_id()
                    finally:
                        imgui.end_child()
                if imgui.collapsing_header("Configuration preview"):
                    names = [name for name, _ in recipe.files]
                    if names:
                        changed, index = imgui.combo("Recipe configuration", names.index(self.config_name), names)
                        if changed:
                            self.config_name = names[index]
                        imgui.input_text_multiline("##recipe_config_preview", dict(recipe.files)[self.config_name], (-1, scaled(150)), imgui.InputTextFlags_.read_only)
                    imgui.text_wrapped("Plugin headers/order and runtime INIs must be reviewed again before native launch. Imported recipes do not carry a validation cache or source game path.")
            finally:
                imgui.end_disabled()
        finally:
            imgui.end_child()
        # Fixed actions do not scroll away with long metadata/mapping inventories.
        recipe, plan = m.recipe, m.recipe_plan
        allowed = not m.busy and not m.runtime_active
        mapped = recipe is not None and not self._missing
        same = bool(plan and plan.recipe is recipe and plan.template == m.profile_id and plan.name == self.name.strip()
                    and plan.mapping == self._mapping_tuple)
        if action_button("Review profile transfer", enabled=allowed and mapped and bool(self.name.strip())):
            self.ack = False
            w._action(lambda: m.review_recipe(self.mapping, self.name))
        if plan and same:
            if self.reviewed is not plan:
                self.reviewed, self.ack = plan, False
            imgui.text_disabled(("Existing: " if plan.existing else "New: ") + plan.name[:32] + f" | {len(plan.recipe.entries):,} entries")
            imgui.set_item_tooltip(f"{plan.name}\n{len(plan.recipe.files)} allowlisted configuration files\n" + ("Repeat import preserves all existing local edits." if plan.existing else "New metadata references the reviewed mapped mods."))
            _, self.ack = imgui.checkbox("Keep original profiles and import this reviewed mapping", self.ack)
        else:
            self.ack = False
        if action_button("Use existing transferred profile" if plan and plan.existing else "Import reviewed profile", primary=True, enabled=allowed and same and self.ack):
            w._action(m.apply_recipe)
        imgui.same_line()
        if action_button("Cancel transfer", enabled=allowed):
            m.recipe_plan, self.ack = None, False
            m.status = "Profile transfer cancelled; no profile changed."
