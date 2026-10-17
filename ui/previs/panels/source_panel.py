"""Load-order source, options and the Check button."""
from __future__ import annotations

from imgui_bundle import imgui

from creation_lib.ui.widgets.pick_folder import pick_file, pick_folder
from ui.previs.state import SOURCE_KINDS, SOURCE_LABELS


def draw(app) -> None:
    state = app.state
    imgui.begin_disabled(state.running)
    imgui.text("Load order")
    for kind in SOURCE_KINDS:
        if imgui.radio_button(f"{SOURCE_LABELS[kind]}##previs_src_{kind}", state.source_kind == kind):
            state.source_kind = kind
            if kind == "modbox" and not state.modbox_profiles:
                app.refresh_modbox_profiles()
    imgui.spacing()
    if state.source_kind == "mo2":
        _path_row("Profile folder", "mo2", state.mo2_profile_dir,
                  lambda: pick_folder("MO2 profile (profiles\\<name>)", state.mo2_profile_dir),
                  lambda v: setattr(state, "mo2_profile_dir", v))
        imgui.text_disabled("The Fallout 4 folder comes from Settings > Paths,\nor a portable instance's ModOrganizer.ini.")
    elif state.source_kind == "modbox":
        _path_row("Library", "modbox", state.modbox_library or app.default_library(),
                  lambda: pick_folder("ModBox21 library", state.modbox_library),
                  lambda v: (setattr(state, "modbox_library", v), app.refresh_modbox_profiles()))
        names = [name for _, name in state.modbox_profiles]
        ids = [pid for pid, _ in state.modbox_profiles]
        current = ids.index(state.modbox_profile_id) if state.modbox_profile_id in ids else 0
        if names:
            changed, current = imgui.combo("Profile##previs_modbox_profile", current, names)
            if changed:
                state.modbox_profile_id = ids[current]
        if imgui.button("Refresh profiles##previs_modbox_refresh"):
            app.refresh_modbox_profiles()
    else:
        _path_row("Plugin", "plugin", state.plugin_path,
                  lambda: pick_file("Plugin", [("Plugins", "*.esm *.esp *.esl")], state.plugin_path),
                  lambda v: setattr(state, "plugin_path", v))
        _draw_against(state)

    imgui.separator()
    imgui.text("Options")
    _, state.include_base_conflicts = imgui.checkbox("Include base-game conflicts##previs_base", state.include_base_conflicts)
    _tooltip("Also report inconsistencies between Bethesda plugins alone, the way a full PRP-style rebuild would.")
    _, state.restore_disabled = imgui.checkbox("Restore disabled previsbines##previs_restore", state.restore_disabled)
    _tooltip("Treat previs or precombines that a mod deliberately disabled as broken.")
    _, state.unprecombined_enabled = imgui.checkbox("Rebuild for performance##previs_perf", state.unprecombined_enabled)
    _tooltip("Rebuild precombined cells where mods placed at least this many precombinable objects outside the precombines.")
    if state.unprecombined_enabled:
        _, state.unprecombined_threshold = imgui.slider_int("Min references##previs_perf_n", state.unprecombined_threshold, 1, 50)
    imgui.text("Rebuild")
    if imgui.radio_button("Broken cells##previs_scope_broken", state.scope == "broken"):
        state.scope = "broken"
    _tooltip("Only cells whose precombines or previs the load order broke. With PPF.esm loaded, that is what your mods changed.")
    if state.source_kind == "plugin":
        if imgui.radio_button("All cells of this plugin##previs_scope_plugin", state.scope == "plugin"):
            state.scope = "plugin"
        _tooltip("Every cell the plugin defines, overrides or places objects in, not only the broken ones.\n"
                 "Use it to give a mod its own precombines and previs.")
    elif state.scope == "plugin":
        state.scope = "broken"
    if imgui.radio_button("All cells of the load order##previs_scope_all", state.scope == "all"):
        state.scope = "all"
    _tooltip("Every cell, broken or not: a full base such as PPF.esm, built over Fallout4.esm, the DLC and UFO4P.\n"
             "Takes as long as a full regeneration.")
    imgui.end_disabled()

    imgui.separator()
    imgui.begin_disabled(state.running)
    if imgui.button("Check load order##previs_check", (-1, 0)):
        app.start_check()
    imgui.end_disabled()
    if state.running or state.progress_msg:
        imgui.progress_bar(state.progress_frac, (-1, 0), state.progress_msg)
    if state.error_message:
        imgui.push_style_color(imgui.Col_.text, (1.0, 0.4, 0.4, 1.0))
        imgui.text_wrapped(state.error_message)
        imgui.pop_style_color()
    if state.report is not None:
        _draw_generate(app)
    if state.load_order_label:
        imgui.separator()
        imgui.text_wrapped(state.load_order_label)
        if state.plugins and imgui.tree_node(f"{len(state.plugins)} plugins##previs_plugins"):
            for index, name in enumerate(state.plugins):
                imgui.text_unformatted(f"{index:3}  {name}")
            imgui.tree_pop()


def _draw_generate(app) -> None:
    from creation_lib.previs import DEFAULT_PATCH_NAME

    state = app.state
    imgui.separator()
    imgui.text("Rebuild")
    precombine, seed = app.rebuild_counts()
    if not seed:
        imgui.text_disabled("Nothing to rebuild.")
        return
    imgui.text_wrapped(f"{precombine} cell(s) get new precombines; {seed} cell(s) get new previs, whole clusters included.")
    imgui.begin_disabled(state.running)
    if state.single_plugin():
        if imgui.radio_button("New patch##previs_mode_patch", not state.into_plugin):
            state.into_plugin = False
        _tooltip("Write a separate patch plugin loaded after everything else.")
        imgui.same_line()
        if imgui.radio_button("Into the plugin##previs_mode_plugin", state.into_plugin):
            state.into_plugin = True
        _tooltip("Write a copy of the plugin with its own precombines and previs: the cells keep their data and\n"
                 "take the new stamps; its CSG, CDX, meshes and tomes are named after it. The original is not touched.")
    stamping = state.stamps_plugin()
    _path_row("Output folder", "output", state.output_dir,
              lambda: pick_folder("Output folder", state.output_dir),
              lambda v: setattr(state, "output_dir", v))
    _tooltip("The plugin copy and its files go here. Replace the mod's plugin with it."
             if stamping else "The patch, its BA2, CSG and CDX go here. Install the folder as a mod and load the patch last.")
    if not stamping:
        if imgui.radio_button("ESP##previs_type_esp", state.plugin_type == "esp"):
            state.plugin_type = "esp"
        _tooltip("A plain plugin, loaded where you put it after the plugins it patches.")
        imgui.same_line()
        if imgui.radio_button("ESM##previs_type_esm", state.plugin_type == "esm"):
            state.plugin_type = "esm"
        _tooltip("A master, loaded ahead of every plain ESP. A patch that masters a plain ESP\n"
                 "is written as an ESP anyway, since as an ESM it would load before that plugin.")
        imgui.text("Patch name")
        imgui.set_next_item_width(-1)
        _, state.patch_name = imgui.input_text_with_hint("##previs_patch_name", state.default_patch_name(DEFAULT_PATCH_NAME), state.patch_name)
    _, state.pack = imgui.checkbox("Pack meshes and tomes into a BA2##previs_pack", state.pack)
    _tooltip("Off: leave them loose. A plugin that already ships <Name> - Main.ba2 always gets loose files to add to it.")
    _, state.small_model_cull = imgui.checkbox("CK small-object previs cull##previs_small_model_cull", state.small_model_cull)
    _tooltip("Keep the Creation Kit's 4096-unit previs cull distance on models smaller than 100 units. Off: small precombined parts of larger objects stay visible past 4096 units.")
    if imgui.button(("Stamp plugin" if stamping else "Build patch") + "##previs_build", (-1, 0)):
        app.start_build()
    imgui.end_disabled()
    result = state.build_result
    if result:
        if result["patch"]:
            imgui.text_wrapped(f"Wrote {result['patch']} with {len(result['cells'])} cell(s).")
        if result.get("added"):
            imgui.text_wrapped(f"It now also overrides {len(result['added'])} cell(s) its previs clusters need: "
                               + ", ".join(result["added"]))
        if result["unchanged"]:
            imgui.push_style_color(imgui.Col_.text, (0.95, 0.8, 0.45, 1.0))
            imgui.text_wrapped(f"{len(result['unchanged'])} cell(s) could not be rebuilt and kept what they had.")
            imgui.pop_style_color()
        after = result.get("after")
        if after:
            parts = [f"{action}: {count}" for action, count in after.get("actions", [])]
            imgui.text_wrapped("With the patch: " + (", ".join(parts) if parts else "no cells need work"))
    if state.build_log and imgui.tree_node(f"Build log ({len(state.build_log)})##previs_build_log"):
        for line in state.build_log:
            imgui.text_wrapped(line)
        imgui.tree_pop()


def _draw_against(state) -> None:
    imgui.text("Against (optional)")
    _tooltip("Plugins the mod is checked against without mastering them, loaded before it: PPF.esm, or another mod.\n"
             "A patch then fixes what the mod breaks in them and masters both.")
    for index, path in enumerate(list(state.against)):
        _path_row(f"Against {index + 1}", f"against_{index}", path,
                  lambda path=path: pick_file("Plugin to check against", [("Plugins", "*.esm *.esp")], path),
                  lambda v, index=index: state.against.__setitem__(index, v))
        imgui.same_line()
        if imgui.button(f"Remove##previs_against_remove_{index}"):
            state.against.pop(index)
            break
    if imgui.button("Add plugin to check against##previs_against_add"):
        chosen = pick_file("Plugin to check against", [("Plugins", "*.esm *.esp")], "")
        if chosen:
            state.against.append(chosen)


def _path_row(label: str, key: str, value: str, choose, assign) -> None:
    imgui.text(label)
    imgui.push_item_width(-imgui.calc_text_size("Browse").x - imgui.get_style().frame_padding.x * 4)
    changed, new_value = imgui.input_text(f"##previs_path_{key}", value)
    imgui.pop_item_width()
    if changed:
        assign(new_value)
    imgui.same_line()
    if imgui.button(f"Browse##previs_browse_{key}"):
        chosen = choose()
        if chosen:
            assign(chosen)


def _tooltip(text: str) -> None:
    if imgui.is_item_hovered():
        imgui.set_tooltip(text)
