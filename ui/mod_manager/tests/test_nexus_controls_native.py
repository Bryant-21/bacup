"""Actual ImGui request review and download routing; transport is always mocked."""
import pytest
from imgui_bundle import imgui

from creation_lib.mod_manager.nexus import NexusClient
from creation_lib.mod_manager.nexus_state import inspect
from . import test_widget_ids as widgets
from .test_nexus_state import Transport
from .test_download_install import owned_download

native, workspace = widgets.native, widgets.workspace


def mapped(workspace):
    m = workspace.manager
    row = next(row for row in m.rows if row.kind == "external")
    with m.library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1,version='1.2' WHERE id=?", (row.id,))
    m.refresh()
    m.nexus_client = NexusClient("fixture-session", transport=Transport())
    m.nexus_client.user = {"user_id": 42, "name": "Fixture account", "premium": False}
    return row.id


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.0), ((1280, 800), 1.5)])
def test_account_ack_cancel_and_unendorse_actual_input(native, workspace, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    row = mapped(workspace)
    m, draw = workspace.manager, widgets.panels(workspace)
    workspace.nexus_account.open(row, "unendorse")
    native.warm(draw)
    send = native.find("Send reviewed account request")
    assert send.rect[3] < size[1]
    native.click(send, draw)
    assert not m.nexus_client._transport.requests
    native.click(native.find("Cancel account request"), draw)
    assert not m.nexus_client._transport.requests and not workspace._dialog
    workspace.nexus_account.open(row, "unendorse")
    native.warm(draw)
    native.click(native.find("Send this request to the shown Nexus account"), draw)
    native.click(native.find("Send reviewed account request"), draw)
    widgets.finish_job(workspace, native, draw)
    assert len(m.nexus_client._transport.requests) == 1
    assert m.nexus_client._transport.requests[0].full_url.endswith("/abstain.json")
    assert not m.nexus_states and "verify" in m.status


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.0), ((1280, 800), 1.5)])
def test_download_destination_additive_review_and_cancel_actual_input(native, workspace, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    row = mapped(workspace)
    m, draw = workspace.manager, widgets.panels(workspace)
    path = owned_download(m.library)
    workspace.download_installer.open({"path": str(path)})
    widgets.finish_job(workspace, native, draw)
    native.warm(draw)
    assert native.find("Cancel download install").rect[3] < size[1]
    candidate = next(i for i in m.download_review.candidates if i.id == row)
    items = [i for i in native.items if i.label == "##download_destination"]
    assert items and candidate.overlay_eligible
    native.click(items[0], draw)
    native.click(native.find("Review additive update"), draw)
    widgets.finish_job(workspace, native, draw)
    assert m.prepared and m.prepared.overlay_review.base_id == row
    assert not any(r.kind == "owned" for r in m.rows)
    m.discard_install()
    workspace._dialog = ""


def test_free_replacement_browser_action_never_queues_or_sends_credentials(native, workspace, monkeypatch):
    row = mapped(workspace)
    m, urls = workspace.manager, []
    m.selected_id = row
    m.nexus_states[("fallout4", 123)] = inspect(m.nexus_client, "fallout4", 123)
    m.nexus_client._transport.requests.clear()
    monkeypatch.setattr("creation_lib.mod_manager.nexus_links.browse", urls.append)
    def draw():
        imgui.set_next_window_pos((0, 0))
        imgui.set_next_window_size((1000, 1000))
        imgui.begin("Offline Information controls")
        workspace._draw_details(embedded=True)
        imgui.end()
    native.warm(draw)
    buttons = [item for item in native.items if item.label == "Open replacement file on Nexus"]
    assert len(buttons) == 2
    native.click(buttons[0], draw)
    assert urls == ["https://www.nexusmods.com/fallout4/mods/123?tab=files&file_id=3"]
    assert not m.pending_links and not m.nexus_client._transport.requests
