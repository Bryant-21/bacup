"""Compact launcher and focused executable editor; no shared shell changes."""
from pathlib import Path

from imgui_bundle import imgui
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.modern import action_button, heading, scaled

from creation_lib.mod_manager.launcher import MODES, preferred_game, selected_executable
from creation_lib.mod_manager.runtime import NOTICE, REPOSITORY, RuntimePlan, WRITE_POLICY
from creation_lib.mod_manager import runtime_setup
from creation_lib.mod_manager import profile_state
from creation_lib.mod_manager.paths import ManagerError


class LauncherUI:
    def __init__(self, workspace, settings=None):
        self.workspace, self.settings = workspace, settings
        self.selected_id = ""
        self.edit_id, self.title, self.binary, self.arguments, self.cwd = "", "", "", "", ""
        self.mode, self.hidden, self.instance_id = "profile", False, ""
        self.game_root, self.bundle_root = "", ""
        self.source_writes_ack, self.remove_ack = False, False
        self.minimal_ini_ack, self.steam_app_id = False, ""
        self.experimental_ack = False
        self.disable_mcp = False
        self.isolate_documents = False
        self._documents_profile = None
        self.bundle_candidates, self.bundle_issues, self.bundle_notice = [], [], ""
        self.bundle_trusted = False
        self._bundle_trust_path = ""
        from .executable_editor_ui import ExecutableEditor
        self.editor = ExecutableEditor(self)

    @property
    def manager(self):
        return self.workspace.manager

    def preferred_game(self):
        return preferred_game(self.manager, self.game_root, self.settings)

    def selected(self):
        self.selected_id, executable = selected_executable(self.manager, self.selected_id)
        return executable

    def draw_controls(self):
        m = self.manager
        selected = self.selected()
        width = imgui.get_content_region_avail().x
        style = imgui.get_style()
        run_width = imgui.calc_text_size("Run").x + style.frame_padding.x * 2
        edit_width = imgui.calc_text_size("Launch options").x + style.frame_padding.x * 2
        selector_width = width - run_width - edit_width - style.item_spacing.x * 2
        one_row = selector_width >= scaled(120)
        if not one_row:
            selector_width = width - run_width - style.item_spacing.x
        imgui.begin_disabled(m.busy)
        imgui.set_next_item_width(max(scaled(80), selector_width))
        if imgui.begin_combo("##runtime_executable", selected.title if selected else "Choose executable"):
            for item in m.executables:
                if item.hidden:
                    continue
                imgui.push_id(item.id)
                if imgui.selectable("##choose_executable", item.id == self.selected_id and not m.target_id)[0]:
                    if m.target_id:
                        self.workspace._action(lambda: m.select_target(""))
                    self.selected_id = item.id
                    m.launch_review = None
                imgui.get_window_draw_list().add_text(imgui.get_item_rect_min(), imgui.get_color_u32(imgui.Col_.text), item.title)
                imgui.set_item_tooltip(f"{item.title}\n{item.binary}\n{MODES[item.mode]}")
                imgui.pop_id()
            imgui.end_combo()
        imgui.same_line()
        if action_button("Run", primary=True, enabled=bool(selected and m.library)):
            self.source_writes_ack, self.minimal_ini_ack = False, False
            self.experimental_ack = False
            self.workspace._action(lambda: m.prepare_launch(selected.id, self.preferred_game(), ""))
            self.workspace._dialog = "Launch"
        if one_row:
            imgui.same_line()
        if action_button("Launch options", enabled=bool(m.library)):
            self._documents_profile = None
            imgui.open_popup("launch_options")
        imgui.end_disabled()
        if imgui.begin_popup("launch_options"):
            if imgui.menu_item("Edit executables...", "", False, not m.busy and not m.runtime_active)[0]:
                self.workspace._dialog = "Executables"
                self.edit(selected)
            imgui.separator()
            _, self.disable_mcp = imgui.checkbox("Keep MCP disabled", self.disable_mcp)
            imgui.set_item_tooltip("Keep the MCP server off for launches until you turn this option off.")
            if m.library and m.profile_id:
                scope = (str(m.library.root), m.profile_id)
                if self._documents_profile != scope:
                    self.isolate_documents = profile_state.isolate_documents(m.library, m.profile_id)
                    self._documents_profile = scope
                imgui.begin_disabled(m.busy)
                changed, enabled = imgui.checkbox("Isolate Documents", self.isolate_documents)
                imgui.set_item_tooltip("Off: use normal Documents saves, logs and other game files. On: use private session folders. Saved for this profile in both CLI and UI.")
                if changed:
                    self.workspace._action(lambda: self._set_documents_mode(enabled))
                imgui.end_disabled()
            imgui.separator()
            self.workspace.targets_ui.switcher()
            imgui.end_popup()
        if selected:
            imgui.text_disabled(Path(selected.binary).parent.name + ("   /   MCP off" if self.disable_mcp else ""))
            imgui.set_item_tooltip(selected.binary + "\nEach launch gets its own virtual mod files and output. Documents are shared unless isolation is enabled.")
        if m.runtime_sessions and imgui.collapsing_header(f"Sessions ({len(m.runtime_sessions)})"):
            for session, status in m.runtime_sessions:
                endpoint = "MCP disabled" if status.get("mcp_disabled") else status.get("mcp_endpoint") or "MCP port pending"
                imgui.text_wrapped(f"{session.name[:8]}: {status.get('title', 'Game')} - {status.get('state', 'unknown')} - {endpoint}")
                imgui.text_wrapped("Documents: " + str(status.get("documents_root", session / "output" / "ini")))
                imgui.text_wrapped("Output: " + str(session / "output"))
                imgui.set_item_tooltip(str(session / "output"))

    def _set_documents_mode(self, enabled):
        self.isolate_documents = profile_state.isolate_documents(self.manager.library, self.manager.profile_id, enabled)
        self.manager.launch_review = None

    def edit(self, item=None):
        self.edit_id = item.id if item else ""
        self.title = item.title if item else ""
        self.binary = item.binary if item else ""
        self.arguments = item.arguments if item else ""
        self.cwd = item.working_directory if item else ""
        self.mode, self.hidden = (item.mode, item.hidden) if item else ("profile", False)
        self.instance_id = item.instance_id if item else next((p.files.get("_modbox_instance_id", "") for p in self.manager.profiles if p.id == self.manager.profile_id), "")
        self.remove_ack = False
        self.steam_app_id = item.steam_app_id if item else ""
        self.editor.open(item)

    def draw_editor(self):
        self.editor.draw()

    def draw_settings(self, *, setup=False):
        if not setup:
            heading("Modded runtime")
        if self.bundle_notice:
            imgui.text_wrapped(self.bundle_notice)
        imgui.text_wrapped("Run uses this project's native virtual filespace. It merges selected mod/game files with owned output and protects referenced sources by copying a file to output on its first write. Development builds it locally with MSVC; packaged apps include it. No upstream USVFS bundle is required. Native fixtures pass; real FO4/F4SE boot remains unverified.")
        imgui.text_unformatted("Fallout 4 game directory")
        imgui.set_next_item_width(-1)
        _, self.game_root = imgui.input_text_with_hint("##runtime_game", self.preferred_game() or "Game directory", self.game_root)
        if action_button("Browse game directory"):
            value = pick_folder("Choose Fallout 4 game directory", self.game_root)
            if value:
                self.game_root = str(value)
        if imgui.collapsing_header("Legacy USVFS comparison"):
            imgui.text_wrapped("Optional legacy integration for separate comparison configuration. Normal Run always uses the project's own virtual filespace.")
            imgui.set_next_item_width(-1)
            _, self.bundle_root = imgui.input_text("##runtime_bundle", self.bundle_root)
            if action_button("Browse USVFS bundle"):
                value = pick_folder("Choose optional USVFS override", self.bundle_root)
                if value:
                    self.bundle_root = str(value)
            if self.bundle_root != self._bundle_trust_path:
                self.bundle_trusted = False
            if action_button("Check existing bundles"):
                self.find_bundles()
            for candidate in self.bundle_candidates:
                imgui.push_id(candidate)
                if imgui.selectable("Use supported bundle###use_bundle", self.bundle_root == candidate)[0]:
                    self.bundle_root, self.bundle_trusted = candidate, False
                imgui.set_item_tooltip(candidate)
                imgui.text_wrapped(candidate)
                imgui.pop_id()
            imgui.text_wrapped(f"Relative override paths resolve from {runtime_setup.APP_ROOT}. Checks read metadata without loading candidate DLLs.")
            for message in self.bundle_issues:
                imgui.text_wrapped(message)
            if self.bundle_root:
                self._draw_trust()
            imgui.text_wrapped(WRITE_POLICY)
        if imgui.collapsing_header("USVFS license and attribution"):
            imgui.text_wrapped(NOTICE)
            imgui.text_wrapped(REPOSITORY)
            license_path = Path(__file__).with_name("USVFS_LICENSE.txt")
            imgui.input_text_multiline("##usvfs_license", license_path.read_text("utf-8"), (-1, scaled(140)), imgui.InputTextFlags_.read_only)

    def find_bundles(self):
        self.bundle_candidates, self.bundle_issues = runtime_setup.discover(self.manager.instance_configuration)
        if not self.bundle_candidates:
            self.bundle_notice = "No supported bundle found in the imported instance or known app locations. Browse to an existing trusted USVFS 0.5.7.2 folder containing both DLLs and both proxy executables. Nothing was downloaded or loaded."
        else:
            self.bundle_notice = "Supported candidates found. Select one and acknowledge its origin before continuing."

    def ready_bundle(self):
        if not self.bundle_root:
            return True
        try:
            if self.bundle_root != self._bundle_trust_path or not self.bundle_trusted:
                raise ManagerError("Select and check an existing supported bundle, then acknowledge that you trust its origin.")
            self.bundle_root = str(runtime_setup.supported_bundle(self.bundle_root))
            self._bundle_trust_path = self.bundle_root
            self.bundle_notice = "Supported bundle metadata verified; DLLs remain unloaded until reviewed Run."
            return True
        except (ManagerError, OSError, ValueError) as exc:
            self.bundle_notice = str(exc)
            return False

    def draw_setup(self):
        heading("Runtime setup")
        imgui.begin_child("runtime_setup_details", (0, -imgui.get_frame_height_with_spacing() * 2))
        try:
            self.draw_settings(setup=True)
        finally:
            imgui.end_child()
        if action_button("Continue to launch review", primary=True, enabled=not self.manager.busy):
            selected = self.selected()
            if selected and self.ready_bundle():
                self.source_writes_ack = self.minimal_ini_ack = self.experimental_ack = False
                self.workspace._action(lambda: self.manager.prepare_launch(selected.id, self.preferred_game(), self.bundle_root))
                self.workspace._dialog = "Launch"
        imgui.same_line()
        if action_button("Cancel runtime setup"):
            self.workspace._dialog = ""

    def _draw_trust(self):
        _, self.bundle_trusted = imgui.checkbox("I trust this existing bundle's origin", self.bundle_trusted)
        if self.bundle_trusted:
            self._bundle_trust_path = self.bundle_root

    def draw_review(self):
        m = self.manager
        heading("Review launch")
        plan = m.launch_review
        if not plan:
            imgui.text_wrapped("Launch preparation must complete successfully before a process can start. Configure executables and the modded runtime in Settings if needed.")
            if action_button("Open runtime setup"):
                self.find_bundles()
                self.workspace._dialog = "Runtime setup"
            if action_button("Cancel launch"):
                if m.busy:
                    m.cancel()
                m.launch_review, self.workspace._dialog = None, ""
            return
        launch = plan.launch if isinstance(plan, RuntimePlan) else plan
        footer_rows = 2 + (1 + bool(plan.minimal_ini) + bool(plan.experimental) if isinstance(plan, RuntimePlan) else 0)
        imgui.begin_child("launch_review_details", (0, -imgui.get_frame_height_with_spacing() * footer_rows))
        try:
            if isinstance(plan, RuntimePlan):
                imgui.text_wrapped("Private Documents: saves and logs stay in this session's output." if plan.isolate_documents else "Shared Documents: saves, logs and other game files use the normal Documents folder. Profile INI overrides stay in this session's output.")
            self._review_details(plan, launch)
        finally:
            imgui.end_child()
        if isinstance(plan, RuntimePlan):
            if plan.runtime != "modbox-vfs-fo4-x64":
                _, self.source_writes_ack = imgui.checkbox("I understand programs may modify existing source files", self.source_writes_ack)
            else:
                imgui.text_wrapped("Project filespace: existing mapped files and new writes use owned output; direct writes to protected sources are refused. Delete/rename and unsupported native paths fail closed. This is not a security sandbox or a verified game/tool runtime.")
            if plan.minimal_ini:
                _, self.minimal_ini_ack = imgui.checkbox("Use minimal local settings for missing INIs", self.minimal_ini_ack)
            if plan.experimental:
                _, self.experimental_ack = imgui.checkbox("Try the experimental managed F4SE launch route", self.experimental_ack)
        ready = not isinstance(plan, RuntimePlan) or (plan.runtime == "modbox-vfs-fo4-x64" or self.source_writes_ack) and (not plan.minimal_ini or self.minimal_ini_ack) and (not plan.experimental or self.experimental_ack)
        if action_button("Start reviewed executable", primary=True, enabled=not m.busy and ready):
            self.workspace._action(lambda: m.start_launch(accept_source_writes=self.source_writes_ack, accept_minimal_ini=self.minimal_ini_ack, accept_experimental=self.experimental_ack, disable_mcp=self.disable_mcp))
            self.workspace._dialog = ""
        imgui.same_line()
        if action_button("Cancel launch"):
            m.launch_review, self.workspace._dialog = None, ""

    def _review_details(self, plan, launch):
        imgui.text_wrapped(f"{launch.executable.title}\nExecutable: {launch.argv[0]}\nWorking directory: {launch.cwd}\nArguments: {launch.executable.arguments or '(none)'}")
        if isinstance(plan, RuntimePlan):
            output = Path(plan.library_root) / "runtime" / "sessions" / plan.session_id / "output"
            overwrite = output / "overwrite"
            imgui.text_wrapped("Associated Fallout 4 installation: " + plan.game_root)
            imgui.text_wrapped(f"Selected local profile: {launch.profile_name}\n{len(plan.files):,} installed file mappings. Existing runtime output has higher precedence.\nRuntime overwrite: {overwrite}")
            imgui.text_wrapped("Documents: " + str(output / "ini" if plan.isolate_documents else plan.ini_target))
            if plan.isolate_documents:
                imgui.text_wrapped(f"Saves: {output / 'saves'}\nF4SE logs: {output / 'f4se-logs'}")
            if plan.runtime != "modbox-vfs-fo4-x64":
                imgui.text_wrapped(WRITE_POLICY)
            for finding in plan.compatibility:
                imgui.text_wrapped(finding)
            if plan.experimental:
                imgui.text_wrapped("Experimental managed F4SE staging: virtual root files and explicit executable/DLL paths are implemented, but a real FO4/F4SE boot has not established this launch route.")
            if plan.minimal_ini:
                imgui.text_wrapped("These profile INIs use a minimal local baseline: " + ", ".join(plan.minimal_ini))
            imgui.text_wrapped("Cancel applies before process creation. A running game and its child processes continue if this UI closes.")
            imgui.text_wrapped("If the helper crashes or supervision fails, its Windows job terminates the supervised game and children.")
        else:
            imgui.text_wrapped("Direct launch: normal filesystem, without the selected mod profile.")
