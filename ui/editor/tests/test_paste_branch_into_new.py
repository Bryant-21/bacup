from unittest.mock import MagicMock

from creation_lib.nif.nif_file import NifFile
from ui.editor.block_ops import BlockOperations
from ui.editor.nif_session import NifRegistry, NifSession


def _make_source_nif():
    nif = NifFile.new("fo4")
    root = nif.get_block(0)
    branch = nif.add_block("NiNode", {"Name": "Branch", "Children": [], "Num Children": 0})
    root.set_field("Children", [branch.block_id])
    root.set_field("Num Children", 1)
    return nif, branch.block_id


def test_paste_branch_into_new_creates_new_root_and_child_branch():
    source_nif, branch_id = _make_source_nif()
    target_nif = NifFile.new("fo4")
    target_root = target_nif.get_block(0)
    target_root.type_name = "NiNode"
    target_root.set_field("Children", [])
    target_root.set_field("Num Children", 0)
    target_nif.header.block_type_names = ["NiNode"]
    target_nif.header.block_type_index = [0]

    app = MagicMock()
    app.nif_file = source_nif
    app.registry = NifRegistry()
    source_session = NifSession(
        nif_id="main",
        nif=source_nif,
        file_path="source.nif",
        scene_root=MagicMock(),
        anim_manager=MagicMock(),
    )
    app.registry.add_session(source_session)
    target_session = NifSession(
        nif_id="main",
        nif=target_nif,
        file_path="untitled.nif",
        scene_root=MagicMock(),
        anim_manager=MagicMock(),
    )
    app._default_new_nif_game_id.return_value = "fo4"
    app.new_blank_nif.return_value = target_session
    app.queue_paste_branch_into_new = None

    ops = BlockOperations(app)

    ops.paste_branch_into_new(branch_id)

    assert target_nif.get_block(0).type_name == "NiNode"
    assert target_nif.get_block(0).get_field("Children") == [1]
    assert target_nif.get_block(1).get_field("Name") == "Branch"
    assert target_session.dirty is True
    app.new_blank_nif.assert_called_once_with("fo4")
    app.rebuild_scene_from_nif.assert_called_once_with("main")


def test_paste_branch_into_new_queues_when_app_supports_loading_mask():
    source_nif, branch_id = _make_source_nif()
    app = MagicMock()
    app.nif_file = source_nif
    app._default_new_nif_game_id.return_value = "fo4"

    ops = BlockOperations(app)

    ops.paste_branch_into_new(branch_id)

    app.queue_paste_branch_into_new.assert_called_once()
    branch, source_block_id, game_id = app.queue_paste_branch_into_new.call_args[0]
    assert source_block_id == branch_id
    assert game_id == "fo4"
    assert branch[0].block_id == branch_id


def test_poll_branch_paste_executes_queued_operation():
    from ui.editor.app import NifEditorApp

    app = NifEditorApp.__new__(NifEditorApp)
    app.block_ops = MagicMock()
    app._branch_paste_pending = (["branch"], 42, "fo4")
    app._branch_paste_queued_at = 0.0
    app._branch_paste_busy = True
    app._branch_paste_label = "Pasting branch 42 into new NIF..."
    app.status_text = ""

    NifEditorApp._poll_branch_paste_into_new(app)

    app.block_ops.execute_paste_branch_into_new.assert_called_once_with(
        ["branch"],
        42,
        "fo4",
    )
    assert app._branch_paste_pending is None
    assert app._branch_paste_busy is False
    assert app._branch_paste_label == ""


def test_queue_paste_branch_into_new_enables_loading_mask_state():
    from ui.editor.app import NifEditorApp

    app = NifEditorApp.__new__(NifEditorApp)
    app.status_text = ""

    NifEditorApp.queue_paste_branch_into_new(app, ["branch"], 42, "fo4")

    assert app._branch_paste_pending == (["branch"], 42, "fo4")
    assert app._branch_paste_busy is True
    assert app._branch_paste_label == "Pasting branch 42 into new NIF..."
    assert app.status_text == app._branch_paste_label
