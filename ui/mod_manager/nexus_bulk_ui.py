"""Bounded per-item bulk review with a fixed confirmation/result footer."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import heading, action_button, scaled

from creation_lib.mod_manager import nexus_bulk


class BulkUI:
    def __init__(self, workspace):
        self.workspace, self.plan, self.result = workspace, None, None
        self.selected, self.ack, self.action = set(), False, 2
        self.mods = ()

    def open_account(self, ids):
        self.mods, self.action = tuple(ids), 2
        self.plan = nexus_bulk.account_review(self.workspace.manager, self.mods, "track")
        self._open()

    def open_downloads(self):
        self.plan = nexus_bulk.download_review(self.workspace.manager)
        self._open()

    def _open(self):
        self.selected = {item.id for item in self.plan.items if not item.issue}
        self.ack, self.result, self.workspace._tab = False, None, "Nexus batch review"

    def apply(self):
        if not self.ack:
            raise nexus_bulk.ManagerError("Confirm the shown batch before applying it.")
        m, plan = self.workspace.manager, self.plan
        if plan.kind == "downloads":
            self.result = nexus_bulk.queue_downloads(m, plan, self.selected)
            m.status = f"Reviewed download batch: {len(self.result.rows)} outcomes. No archive was installed."
            return
        items = nexus_bulk.validate(m, plan, self.selected)
        for item in items:
            m.nexus_states.pop(item.identity, None)
        m._status_revision += 1
        def complete(result):
            self.result = result
            if result.retry_after:
                import time
                m._rate_blocked_until = time.monotonic() + result.retry_after
            m.status = f"Account batch {'stopped with partial outcomes' if result.stopped else 'finished'}: {len(result.rows)} reviewed items. Remote state requires refresh."
        m.submit("Sending reviewed Nexus account batch", lambda cancel, progress: nexus_bulk.account_run(m, plan, items, cancel, progress), complete)
        m._cancel_status = "Stopping account batch between requests. Requests already sent cannot be undone; remote outcomes require refresh."

    def draw(self):
        w, m, plan = self.workspace, self.workspace.manager, self.plan
        heading("Review Nexus batch")
        if not plan:
            return
        imgui.begin_child("nexus_batch_body", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            if plan.kind == "account" and not self.result:
                imgui.begin_disabled(m.busy)
                imgui.set_next_item_width(min(scaled(240), imgui.get_content_region_avail().x * .4))
                changed, self.action = imgui.combo("Account action", self.action, ["Endorse", "Unendorse", "Track", "Untrack"])
                imgui.end_disabled()
                imgui.same_line()
                if changed:
                    self.plan = nexus_bulk.account_review(m, self.mods, ("endorse", "unendorse", "track", "untrack")[self.action])
                    self.selected = {item.id for item in self.plan.items if not item.issue}
                    self.ack = False
                    plan = self.plan
            imgui.text_wrapped("Account: " + plan.account)
            imgui.text_wrapped(f"Profile: {plan.destination['profile_name'] or m.profile_id} | Game: {plan.destination['game'] or m.nexus_game} | Target: {plan.destination['target_name'] or 'None'}")
            results = dict(self.result.rows) if self.result else {}
            jobs = {(j["game"], j["mod_id"], j["file_id"]): j for j in m.download_jobs} if plan.kind == "downloads" and self.result else {}
            imgui.begin_disabled(m.busy or self.result is not None)
            for item in plan.items:
                imgui.push_id(str(item.id))
                imgui.begin_disabled(bool(item.issue))
                changed, selected = imgui.checkbox(item.name + "###nexus_batch_item", item.id in self.selected)
                imgui.end_disabled()
                if changed:
                    self.selected.add(item.id) if selected else self.selected.discard(item.id)
                    self.ack = False
                imgui.text_wrapped(f"{item.identity[0]} / mod {item.identity[1]} / file {item.identity[2] if len(item.identity) == 3 else item.file_id or 'Unknown'} | Version: {item.version or 'Unknown'}")
                if item.issue or item.id in results:
                    imgui.text_wrapped(item.issue or results[item.id])
                if self.result and plan.kind == "downloads":
                    job = jobs.get(item.identity)
                    if job:
                        imgui.text_wrapped("Queue state: " + job["state"])
                imgui.pop_id()
            imgui.end_disabled()
            imgui.separator()
            imgui.text_wrapped(plan.destination["library_root"])
            imgui.text_wrapped("Versions are local declared metadata. Nexus decides eligibility. Failed or cancelled writes may have an unconfirmed remote outcome; refresh before retrying. Requests already sent cannot be undone." if plan.kind == "account" else "Downloads use current entitlements and the existing bounded queue. Each archive requires separate installer review. Payload compatibility across game builds is unverified.")
        finally:
            imgui.end_child()
        if m.busy:
            if action_button("Stop account batch"):
                m.cancel()
        elif not self.result:
            _, self.ack = imgui.checkbox(f"Confirm {len(self.selected)} selected requests", self.ack)
            if action_button("Apply reviewed batch", primary=True, enabled=self.ack and bool(self.selected) and not m.busy and not m.runtime_active):
                w._action(self.apply)
        elif plan.kind == "downloads":
            if action_button("Pause this download batch"):
                for identifier, _ in self.result.rows:
                    m.pause_download(identifier)
        if action_button("Close batch results" if self.result else "Cancel batch review", enabled=not m.busy):
            self.plan, self.result, w._dialog = None, None, ""
