"""File-open/drop intake selects a package for review; it never installs it."""
from creation_lib.mod_manager.package import validate_intake
from creation_lib.mod_manager.paths import ManagerError


def accept(workspace, path):
    selected = validate_intake(path)
    if workspace._incoming_package:
        raise ManagerError("A package is already pending. Review or discard it before opening another.")
    workspace._incoming_package = selected
    workspace.manager.status = "Package selected for review. Create or open a local profile to continue." if not workspace.manager.profile_id else "Package selected for review; waiting for current work to finish."
    workspace._tab = "Library" if not workspace.manager.profile_id else "Install"


def poll(workspace):
    manager = workspace.manager
    if not workspace._incoming_package or manager.busy or manager.runtime_active or not manager.library or not manager.profile_id:
        return
    selected, workspace._incoming_package = workspace._incoming_package, ""
    workspace._prepare(selected)
