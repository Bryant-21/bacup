"""Separate explicit review/acknowledgement for recoverable F4SE bootstrap placement."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button


class RootSetupUI:
    def __init__(self, workspace):
        self.workspace, self.ack, self.review_key = workspace, False, None

    def sync(self):
        plan = self.workspace.manager.root_setup_review
        if plan is not self.review_key:
            self.review_key, self.ack = plan, False

    def details(self, selected):
        if not selected:
            return
        self.sync()
        m = self.workspace.manager
        plan = m.root_setup_review
        if plan:
            imgui.text_unformatted("Restore original F4SE bootstrap" if plan.restore else "Review F4SE game-root bootstrap")
            imgui.text_wrapped(selected.game_root)
            if not plan.restore and any(old for _, old, _ in plan.files):
                imgui.text_wrapped("Original bytes and basic metadata are retained. Full NTFS permissions/alternate streams are not preserved by this backup contract.")
            if not plan.files:
                imgui.text_wrapped("Already installed: applying verifies the same setup without replacement.")
            for name, old, new in plan.files:
                description = ("restore original" if new else "remove created file") if plan.restore else ("replace; original backed up" if old else "create")
                imgui.text_unformatted(name + " - " + description)
            for name, _ in plan.temporaries:
                imgui.text_wrapped(name + " - remove reviewed retained temporary")
            imgui.text_wrapped("Scripts stay private. No game starts. Close all FO4/F4SE processes first.")
            if imgui.collapsing_header("Hashes and recovery details"):
                for name, old, new in plan.files:
                    imgui.text_unformatted(name)
                    if old:
                        imgui.text_wrapped("Current SHA-256: " + old["sha256"])
                    if new:
                        imgui.text_wrapped("Result SHA-256: " + new["sha256"])
                for name, info in plan.temporaries:
                    imgui.text_wrapped(name + "\nCurrent SHA-256: " + info["sha256"])
                imgui.text_wrapped("Only the listed bootstrap files and transaction temporaries/lock are written in the game root. Originals are retained in target-owned history. Restore refuses unrelated changes. Mod payloads and game Data are not copied.")
            return
        imgui.text_unformatted("Recoverable game-root bootstrap")
        imgui.text_wrapped("Optional standard placement for F4SE loader/runtime DLLs. Existing components are backed up. Scripts stay private and are mapped by USVFS; source mods and game Data are not copied. Real FO4/F4SE boot remains unverified.")
        if action_button("Review root restore/recovery"):
            self.workspace._action(lambda: m.review_target_root_setup(restore=True))
        if selected.f4se.get("managed") and not selected.f4se.get("root_setup"):
            imgui.same_line()
            if action_button("Prepare matching F4SE again"):
                nxm = self.workspace.targets_ui.nxm
                self.workspace.targets_ui.nxm = ""
                self.workspace._action(lambda: m.prepare_target_f4se(archive=self.workspace.targets_ui.archive, nxm=nxm))

    def footer(self):
        self.sync()
        m = self.workspace.manager
        plan = m.root_setup_review
        if not plan:
            return
        _, self.ack = imgui.checkbox("Allow the listed root-file writes with recoverable backups", self.ack)
        if action_button("Apply root restore" if plan.restore else "Apply root setup", primary=True, enabled=self.ack):
            self.workspace._action(lambda: m.apply_target_root_setup(accept_root_writes=self.ack))
        imgui.same_line()
        if action_button("Cancel root review"):
            m.root_setup_review, self.ack = None, False
