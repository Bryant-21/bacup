"""Account actions with a review body and fixed confirmation footer."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import heading, action_button
from creation_lib.mod_manager import nexus_account


class AccountUI:
    def __init__(self, workspace):
        self.workspace, self.plan, self.ack = workspace, None, False

    def open(self, mod_id, action):
        self.plan = nexus_account.review(self.workspace.manager, mod_id, action)
        self.ack, self.workspace._dialog = False, "Nexus account request"
        self.workspace._tab = "Nexus account request"

    def send(self):
        if not self.ack:
            from creation_lib.mod_manager.nexus import NexusError
            raise NexusError("Confirm the reviewed account request before sending.")
        nexus_account.validate(self.workspace.manager, self.plan)
        self.workspace.manager.nexus_account_action(self.plan.mod_id, self.plan.action)
        self.plan, self.workspace._dialog = None, ""

    def draw(self):
        w, m, plan = self.workspace, self.workspace.manager, self.plan
        heading("Review Nexus account request")
        if not plan:
            return
        imgui.begin_child("nexus_account_review_body", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            imgui.text_wrapped(f"{plan.action.capitalize()}: {plan.name}\nAccount: {plan.account_name}\n{plan.game} / mod {plan.nexus_mod_id} / installed file {plan.file_id or 'Unknown'}\nInstalled version: {plan.version or 'Unknown'}")
            imgui.text_wrapped("This sends a request to the connected Nexus account. Eligibility is decided by Nexus. A timeout, failure or cancellation may leave the remote outcome unknown; refresh account state before retrying.")
            imgui.text_wrapped("Unendorse requests the Nexus abstained state. It does not delete this mod or download another file.")
            imgui.text_wrapped("The version is installed metadata supplied locally. Check that it corresponds to a published version on this mod's Nexus page.")
        finally:
            imgui.end_child()
        _, self.ack = imgui.checkbox("Send this request to the shown Nexus account", self.ack)
        if action_button("Send reviewed account request", primary=True, enabled=self.ack and not m.busy):
            w._action(self.send)
        imgui.same_line()
        if action_button("Cancel account request", enabled=not m.busy):
            self.plan, w._dialog = None, ""
