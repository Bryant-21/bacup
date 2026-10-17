"""Visible same-package source repair and retry; never rewrites declarations."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button

from creation_lib.mod_manager import installer_repair
from creation_lib.mod_manager.paths import ManagerError
from .toolbar_ui import _next_button


class InstallerRepairUI:
    def __init__(self, workspace):
        self.workspace, self.cached, self.issues = workspace, None, ()
        self.original, self.replacement = "", ""

    def draw(self, evaluation):
        w, m = self.workspace, self.workspace.manager
        prepared = m.prepared
        if evaluation is not self.cached:
            self.cached = evaluation
            self.issues = installer_repair.missing(prepared.package, evaluation) if evaluation else ()
            if not any(issue.source == self.original for issue in self.issues):
                self.original = self.issues[0].source if self.issues else ""
                self.replacement = ""
        if not self.issues and not m.install_repairs:
            return
        if not imgui.collapsing_header("Missing source repair", imgui.TreeNodeFlags_.default_open):
            return
        imgui.text_wrapped("Bind a missing declared file/folder to an existing payload inside this package. Authored destination names stay intact. Required-choice conflicts need corrected choices, explicit dependency input, or manual layout; no requirement is dropped automatically.")
        for issue in self.issues[:20]:
            imgui.text_wrapped(("Folder: " if issue.folder else "File: ") + issue.source +
                               (" -> " + m.install_repairs[issue.source.casefold()] if issue.source.casefold() in m.install_repairs else " (unresolved)"))
        if len(self.issues) > 20:
            imgui.text_disabled(f"{len(self.issues) - 20} additional missing declarations; enter their exact package path below.")
        label_width = max(imgui.calc_text_size(label).x for label in ("Missing declared source", "Existing package payload"))
        for label, identifier, attribute in (("Missing declared source", "##missing_declared_source", "original"),
                                             ("Existing package payload", "##existing_package_payload", "replacement")):
            start = imgui.get_cursor_pos_x()
            imgui.text_unformatted(label)
            imgui.same_line(start + label_width + imgui.get_style().item_inner_spacing.x)
            imgui.set_next_item_width(-1)
            _, value = imgui.input_text(identifier, getattr(self, attribute))
            setattr(self, attribute, value)
        if action_button("Bind missing source", enabled=bool(self.original and self.replacement) and not m.busy):
            bindings = {**m.install_repairs, self.original.casefold(): self.replacement}
            expected = m._install_review_state(w._content_root)
            evaluation_snapshot = evaluation
            def work(cancel, progress):
                prepared.validate(m.context_library, m.profile_id, cancel)
                return installer_repair.validate(prepared.package, evaluation_snapshot, bindings, cancel)
            def complete(result):
                if m.prepared is not prepared or expected != m._install_review_state(w._content_root):
                    raise ManagerError("Installer choices changed during source repair. Review again.")
                m.install_repairs = result
                w._invalidate_install()
                m.status = "Explicit package source binding recorded. Review every destination before installing."
            m.submit("Reviewing explicit package source binding", work, complete)
        _next_button("Clear source repairs")
        if action_button("Clear source repairs", enabled=bool(m.install_repairs) and not m.busy):
            m.install_repairs = {}
            w._invalidate_install()
