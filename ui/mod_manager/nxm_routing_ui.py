"""Explicit browser destination selection with a fixed send/cancel footer."""
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace

from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import heading, action_button

from creation_lib.mod_manager import nxm_instances
from creation_lib.mod_manager.process_identity import process_state


class RoutingUI:
    def __init__(self, workspace):
        self.workspace, self.request = workspace, None
        self.destinations, self.selected, self.message = (), "", ""
        self.future = None
        self.dismissed = False
        self.waiting = []
        self.pool = ThreadPoolExecutor(max_workers=1, thread_name_prefix="nxm-destination")

    def open(self, request):
        if self.request is not None:
            if request.id != self.request.id and all(item.id != request.id for item in self.waiting):
                if len(self.waiting) >= 100:
                    from creation_lib.mod_manager.paths import ManagerError
                    raise ManagerError("Browser routing review queue is full. Review or cancel pending links first.")
                self.waiting.append(request)
            return
        self.request, self.message = request, ""
        self.dismissed = False
        self.selected = request.destination.instance_id if request.uncertain and request.destination else ""
        self.workspace._tab = "Browser download destination"
        try:
            self.refresh()
        except (OSError, ValueError):
            self.destinations, self.message = (), "Destination discovery is unavailable. Refresh after opening a matching profile."

    def _next(self):
        self.request = None
        if self.waiting:
            self.open(self.waiting.pop(0))
        else:
            self.workspace._dialog = ""

    def refresh(self):
        from .integration import ensure_inbox
        ensure_inbox(self.workspace)
        inbox = self.workspace._link_inbox
        self.destinations = nxm_instances.discover(inbox.directory, self.request.link.game) if inbox else ()
        if self.request.uncertain and self.request.destination and process_state(self.request.destination.process) == "alive":
            self.destinations = (self.request.destination,)
            self.message = "Delivery outcome is unknown. Check that instance's receipt before choosing another destination."
        elif self.request.uncertain:
            self.request = replace(self.request, uncertain=False, destination=None)
            self.message = "Previous destination closed. Choose a live destination; cached downloads remain deduplicated within their library."
        if self.selected not in {d.instance_id for d in self.destinations}:
            self.selected = ""

    def send(self):
        destination = next(d for d in self.destinations if d.instance_id == self.selected)
        request = replace(self.request, destination=destination)
        self.request = request
        self.future = self.pool.submit(nxm_instances.send, destination, request, status_only=request.uncertain)

    def poll(self):
        if not self.future or not self.future.done():
            return
        result, self.future = self.future.result(), None
        state = result.get("state", "uncertain")
        if self.dismissed:
            self.request, self.dismissed = None, False
            self.workspace.manager.status = f"Browser routing review closed. Delivery state: {state}; routing requested no download."
            if self.waiting:
                self.open(self.waiting.pop(0))
            return
        if state in {"accepted", "delivered"}:
            target = result.get("context", self.request.destination.context)
            self.workspace.manager.status = f"Browser request delivered to {target['profile_name']} in {target['library_root']}. Download still requires an explicit action there."
            self._next()
        else:
            self.request = replace(self.request, uncertain=state == "uncertain")
            self.message = {"uncertain": "Delivery outcome remains unknown. Check the same destination's receipt; no fallback download was started.",
                            "unknown": "That instance has no receipt. Sending again uses the same delivery ID.",
                            "delivered_closed": "This request was already delivered to a now-closed instance. Open that destination library to review its queue; routing will not duplicate it into another library.",
                            "stale": "Destination changed. Refresh and review it again.",
                            "full": "Destination inbox is full. Review its pending requests before retrying."}.get(state, "Destination rejected the request. Refresh and review again.")

    def draw(self):
        w, request = self.workspace, self.request
        heading("Choose a destination")
        if not request:
            return
        imgui.begin_child("nxm_destination_body", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            imgui.text_wrapped(request.link.label)
            if self.message:
                imgui.text_wrapped(self.message)
            if not self.destinations:
                imgui.text_wrapped("No matching live destination. Open a profile with a known game, then refresh. Closing this review discards the in-memory browser key.")
            imgui.begin_disabled(self.future is not None)
            for destination in self.destinations:
                imgui.push_id(destination.instance_id)
                here = w._link_inbox and destination.instance_id == w._link_inbox.instance_id
                label = ("This window: " if here else f"Window {destination.process['pid']}: ") + destination.label
                if imgui.selectable(label + "###nxm_destination", destination.instance_id == self.selected)[0]:
                    self.selected = destination.instance_id
                imgui.text_disabled(destination.context["library_root"])
                imgui.set_item_tooltip("Library: " + destination.context["library_root"] + "\nGame: " + destination.context["game"] + "\nTarget folder: " + (destination.context["target_root"] or "None"))
                imgui.pop_id()
            imgui.end_disabled()
            imgui.text_wrapped("Choose the library, profile and game target that should receive this request. This queues a link for review; it does not download or install anything. Payload compatibility across game builds is unverified.")
        finally:
            imgui.end_child()
        if action_button("Refresh destinations", enabled=self.future is None):
            w._action(self.refresh)
        if action_button("Check delivery receipt" if request.uncertain else "Send to selected destination", primary=True, enabled=bool(self.selected) and self.future is None):
            w._action(self.send)
        imgui.same_line()
        if action_button("Cancel browser routing", enabled=self.future is None):
            self._next()
            w.manager.status = "Browser routing cancelled locally. An earlier uncertain delivery may already be queued; no download was requested by routing."

    def close(self):
        self.request = None
        self.waiting.clear()
        self.pool.shutdown(wait=True, cancel_futures=True)

    def dismiss(self):
        self.waiting.clear()
        self.dismissed = self.future is not None
        if not self.dismissed:
            self.request = None
        self.workspace.manager.status = "Browser routing review closed; its in-memory link was discarded. An earlier delivery may already be queued; no download was requested by routing."
