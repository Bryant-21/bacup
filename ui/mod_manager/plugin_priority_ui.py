"""A compact priority editor; final profile persistence remains in the Plugins pane."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import heading, action_button

from creation_lib.mod_manager.plugin_priority import begin, apply


class PluginPriorityUI:
    def __init__(self, workspace):
        self.workspace, self.plan, self.value = workspace, None, 0

    def open(self, name):
        w = self.workspace
        self.plan = begin(w.panes, name)
        self.value = self.plan.priority
        w._tab = "Edit plugin priority"

    def draw(self):
        w, m = self.workspace, self.workspace.manager
        heading("Edit plugin draft priority", large=True)
        imgui.text_wrapped("Choose the plugin's position in the complete draft, including filtered rows. Details must be inspected and master/lock constraints must pass. After editing, review and apply from Plugins to persist this local profile.")
        if not self.plan:
            w._dialog = ""
            return
        imgui.text_unformatted(self.plan.name)
        imgui.text_disabled(f"Current priority: {self.plan.priority}; {len(self.plan.order)} draft entries")
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.set_next_item_width(-1)
        _, self.value = imgui.slider_int("##plugin_priority_value", self.value, 0, len(self.plan.order) - 1)
        imgui.set_item_tooltip("Ctrl+click to type an exact priority.")
        if action_button("Use reviewed draft priority", primary=True):
            def save():
                apply(w.panes, self.plan, str(self.value))
                self.plan, w._dialog = None, ""
                w._right_pane = "Plugins"
                m.status = "Plugin draft priority changed. Review and apply from Plugins to save this local profile."
            w._action(save)
        imgui.same_line()
        if action_button("Cancel plugin priority edit"):
            self.plan, w._dialog = None, ""
        imgui.end_disabled()
