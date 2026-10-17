"""Actual input for synthetic browser selection and reviewed bulk actions."""
from dataclasses import replace
import uuid
import threading

from imgui_bundle import imgui
import pytest

from creation_lib.mod_manager.nexus import parse_nxm
from creation_lib.mod_manager.nxm_instances import Inbox, Request
from creation_lib.mod_manager.nxm_routing import context
from . import test_widget_ids as widgets
from .test_nexus_bulk import Account
from .test_nexus import LINK

native, workspace = widgets.native, widgets.workspace


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.0), ((1280, 800), 1.5)])
def test_multiple_destinations_cancel_and_select_remote_actual_input(native, workspace, tmp_path, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    m, draw = workspace.manager, widgets.panels(workspace)
    from ui.mod_manager.integration import ensure_inbox
    ensure_inbox(workspace)
    remote_root = tmp_path / "remote library"
    remote_root.mkdir()
    remote_context = {**context(m), "library_id": "remote-fixture", "library_root": str(remote_root), "profile_name": "Remote profile"}
    remote = Inbox(workspace._link_inbox.directory, context=remote_context)
    try:
        workspace.handle_nxm(LINK)
        native.warm(draw)
        assert len(workspace.nxm_router.destinations) == 2 and not workspace.nxm_router.selected
        assert native.find("Cancel browser routing").rect[3] < size[1]
        native.click(native.find("Cancel browser routing"), draw)
        assert not workspace.nxm_router.request and remote.requests.empty() and not m.pending_links
        workspace.handle_nxm(LINK)
        native.warm(draw)
        item = next(i for i in native.items if "Remote profile" in i.label and i.label.endswith("###nxm_destination"))
        native.click(item, draw)
        native.click(native.find("Send to selected destination"), draw)
        workspace.nxm_router.future.result(timeout=5)
        native.warm(draw)
        delivered = remote.requests.get(timeout=5)
        assert delivered.identity == ("fallout4", 123, 456)
        assert not m.pending_links and not m.library.downloads()
        assert "explicit action" in m.status
    finally:
        remote.close()


def test_uncertain_route_offers_receipt_check_and_keeps_same_delivery_id(native, workspace):
    from ui.mod_manager.integration import ensure_inbox
    ensure_inbox(workspace)
    inbox, draw = workspace._link_inbox, widgets.panels(workspace)
    request = Request(uuid.uuid4().hex, parse_nxm(LINK), inbox.destination, True)
    workspace.nxm_router.open(request)
    native.warm(draw)
    assert native.find("Check delivery receipt") and not any(i.label == "Send to selected destination" for i in native.items)
    native.click(native.find("Check delivery receipt"), draw)
    workspace.nxm_router.future.result(timeout=5)
    native.warm(draw)
    assert workspace.nxm_router.request.id == request.id and not workspace.nxm_router.request.uncertain
    assert not workspace.manager.pending_links


@pytest.mark.parametrize("close", ["back", "escape"])
def test_navigation_closing_browser_review_discards_pending_secret(native, workspace, close):
    workspace.handle_nxm(LINK)
    draw = widgets.panels(workspace)
    native.warm(draw)
    assert workspace.nxm_router.request
    if close == "back":
        native.click(native.find("Back to Library"), draw)
    else:
        native.click(native.find("Refresh destinations"), draw)
        native.key(imgui.Key.escape, draw)
    assert not workspace._dialog and not workspace.nxm_router.request
    assert not workspace.manager.pending_links


def test_new_browser_link_survives_close_while_previous_delivery_finishes(native, workspace):
    from concurrent.futures import Future
    workspace.handle_nxm(LINK)
    draw = widgets.panels(workspace)
    native.warm(draw)
    future = Future()
    workspace.nxm_router.future = future
    native.click(native.find("Back to Library"), draw)
    newer = replace(parse_nxm(LINK), file_id=457)
    workspace.nxm_router.open(Request(uuid.uuid4().hex, newer))
    assert len(workspace.nxm_router.waiting) == 1
    future.set_result({"state": "uncertain"})
    native.warm(draw)
    assert workspace._dialog == "Browser download destination"
    assert workspace.nxm_router.request.link.identity == newer.identity
    assert not workspace.nxm_router.waiting and not workspace.manager.pending_links


def mapped(workspace):
    m = workspace.manager
    ids = [r.id for r in m.rows if r.kind == "external"][:2]
    with m.library._db() as db:
        for index, identifier in enumerate(ids):
            db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=?,nexus_file_id=1,version='1.2' WHERE id=?", (123 + index, identifier))
    m.refresh()
    m.nexus_client = Account()
    return ids


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.0), ((1280, 800), 1.5)])
def test_bulk_account_review_ack_cancel_apply_and_results_actual_input(native, workspace, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    ids = mapped(workspace)
    m, draw = workspace.manager, widgets.panels(workspace)
    workspace.nexus_bulk.open_account(ids)
    native.warm(draw)
    assert native.find("Cancel batch review").rect[3] < size[1]
    native.click(native.find("Apply reviewed batch"), draw)
    assert not m.nexus_client.calls
    native.click(native.find("Cancel batch review"), draw)
    assert not m.nexus_client.calls
    workspace.nexus_bulk.open_account(ids)
    native.warm(draw)
    native.click(native.find("Confirm 2 selected requests"), draw)
    native.click(native.find("Apply reviewed batch"), draw)
    widgets.finish_job(workspace, native, draw)
    assert len(m.nexus_client.calls) == 2 and workspace.nexus_bulk.result
    assert not m.nexus_states and "requires refresh" in m.status
    native.click(native.find("Close batch results"), draw)
    assert not workspace._dialog


def test_account_batch_cancel_after_first_reports_partial_sent_outcomes(native, workspace):
    ids = mapped(workspace)
    m, draw = workspace.manager, widgets.panels(workspace)
    entered, release = threading.Event(), threading.Event()
    def waiting():
        entered.set()
        assert release.wait(5)
    m.nexus_client.callback = waiting
    workspace.nexus_bulk.open_account(ids)
    native.warm(draw)
    native.click(native.find("Confirm 2 selected requests"), draw)
    native.click(native.find("Apply reviewed batch"), draw)
    assert entered.wait(5)
    native.warm(draw)
    native.click(native.find("Stop account batch"), draw)
    release.set()
    widgets.finish_job(workspace, native, draw)
    assert len(m.nexus_client.calls) == 1 and workspace.nexus_bulk.result.stopped
    assert "Not sent" in workspace.nexus_bulk.result.rows[1][1]


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.0), ((1280, 800), 1.5)])
def test_bulk_download_cancel_review_has_no_transfer_actual_input(native, workspace, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    mapped(workspace)
    m, draw = workspace.manager, widgets.panels(workspace)
    m.accept_nxm(LINK)
    m.accept_nxm(replace(parse_nxm(LINK), file_id=457))
    workspace.nexus_bulk.open_downloads()
    native.warm(draw)
    assert native.find("Cancel batch review").rect[3] < size[1]
    native.click(native.find("Cancel batch review"), draw)
    assert m._download_scheduler is None and len(m.pending_links) == 2


def test_reviewed_download_batch_actual_apply_and_pause_keep_owned_checkpoints(native, workspace):
    from .test_download_scheduler import Wire, link
    from creation_lib.mod_manager.download_queue import DownloadQueue
    m, draw, wire = workspace.manager, widgets.panels(workspace), Wire()
    mapped(workspace)
    m.nexus_client = wire.client()
    for index in range(2):
        m.accept_nxm(link(index))
    workspace.nexus_bulk.open_downloads()
    try:
        native.warm(draw)
        native.click(native.find("Confirm 2 selected requests"), draw)
        native.click(native.find("Apply reviewed batch"), draw)
        assert wire.entered[0].wait(5)
        native.warm(draw)
        native.click(native.find("Pause this download batch"), draw)
        wire.release.set()
        for transfer in list(m._download_scheduler.transfers.values()):
            transfer.future.exception(timeout=15)
        m._poll_downloads()
        jobs = DownloadQueue(m.library).records()
        assert len(jobs) == 2 and not any(job["state"] == "complete" for job in jobs)
        assert len(workspace.nexus_bulk.result.rows) == 2 and not m.prepared
        assert not any(row.kind == "owned" for row in m.rows)
    finally:
        wire.release.set()
