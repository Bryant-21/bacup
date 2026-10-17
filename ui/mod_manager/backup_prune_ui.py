"""Fixed-footer review of one owned retained save/overwrite bundle."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button, heading


def open_review(workspace, kind, identifier):
    workspace.manager.review_retained_backup(kind, identifier)
    workspace._retained_origin = "Save backups" if kind == "save" else "Overwrite"
    workspace._tab = "Backup deletion"


def draw(workspace):
    m, plan = workspace.manager, workspace.manager.retained_review
    heading("Permanently delete one retained backup", large=True)
    token = (plan.kind, plan.profile_id, plan.backup_id, plan.token) if plan else None
    if token != workspace._retained_key:
        workspace._retained_key, workspace._retained_ack = token, False
    imgui.begin_child("retained_delete_details", (0, -imgui.get_frame_height_with_spacing() * 3))
    try:
        if plan:
            imgui.text_wrapped(f"{plan.name}\n{plan.kind.title()} backup: {plan.files:,} files, {plan.bytes:,} bytes, including its ownership receipt.")
            imgui.text_wrapped("This permanently removes the selected retained bundle. Current saves, runtime output and other backups remain intact. Cancellation before commit restores the bundle; interrupted committed cleanup resumes on reopening the library and preserves changed or unexpected content.")
            imgui.text_disabled("Backup " + plan.backup_id)
        elif m.busy:
            imgui.text_wrapped("Checking the complete owned bundle in the background.")
        else:
            imgui.text_wrapped(m.status)
        if m.error:
            imgui.text_wrapped(m.error)
    finally:
        imgui.end_child()
    imgui.begin_disabled(m.busy or m.runtime_active)
    try:
        _, workspace._retained_ack = imgui.checkbox("Permanently delete this reviewed retained backup", workspace._retained_ack)
        if action_button("Delete reviewed retained backup", enabled=bool(plan and workspace._retained_ack), primary=True):
            workspace._action(m.remove_retained_backup)
            workspace._retained_ack = False
        imgui.same_line()
        if action_button("Cancel retained backup deletion"):
            m.retained_review, workspace._retained_ack = None, False
            workspace._dialog = workspace._retained_origin
    finally:
        imgui.end_disabled()
