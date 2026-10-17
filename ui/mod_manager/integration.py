from __future__ import annotations

import queue

from creation_lib.mod_manager.nxm_instances import Inbox
from creation_lib.mod_manager.paths import ManagerError


def ensure_inbox(workspace):
    from creation_lib.mod_manager.nxm_routing import context
    if workspace._link_inbox is None and not workspace._inbox_failed:
        try:
            from ui.toolkit.app_paths import get_ini_dir
            workspace._link_inbox = Inbox(get_ini_dir() / "mod-manager", context=context(workspace.manager))
        except (OSError, ValueError):
            workspace._inbox_failed = True
            workspace.manager.error = "Browser-link inbox could not start. Pasted links remain available in Nexus setup."
    if workspace._link_inbox:
        workspace._link_inbox.publish(context(workspace.manager))


def poll_nexus_intake(app):
    workspace = app._ws_map.get("mod_manager")
    if workspace is None or not hasattr(workspace, "manager"):
        return
    ensure_inbox(workspace)
    if workspace._link_inbox:
        for _ in range(10):
            try:
                link = workspace._link_inbox.requests.get_nowait()
            except queue.Empty:
                break
            app._switch_workspace("mod_manager", remember=False)
            from creation_lib.mod_manager.nxm_routing import receive
            workspace._action(lambda: receive(workspace, link))
    workspace.manager.poll()
    try:
        workspace.manager.tick_updates()
    except ManagerError as exc:
        workspace.manager.error = str(exc)
