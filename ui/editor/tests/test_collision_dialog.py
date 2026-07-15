from types import SimpleNamespace

from creation_lib.nif.nif_file import NifBlock, NifFile


def test_collision_dialog_forwards_source_block_ids(monkeypatch):
    from ui.editor.panels.collision_dialog import CollisionDialog

    calls = []

    def fake_generate_collision(nif, **kwargs):
        calls.append((nif, kwargs))
        return SimpleNamespace(success=True, description="ok")

    monkeypatch.setattr(
        "creation_lib.nif.operations.collision.generate_collision",
        fake_generate_collision,
    )

    app = SimpleNamespace(
        nif_file=object(),
        registry=SimpleNamespace(active_session=SimpleNamespace(game_profile=object())),
    )
    dialog = CollisionDialog(app)
    dialog.open(1, source_block_ids=[3])

    dialog._apply()

    assert calls
    assert calls[0][1]["node_block_id"] == 1
    assert calls[0][1]["source_block_ids"] == [3]
    assert calls[0][1]["include_child_nodes"] is True
    assert calls[0][1]["profile"] is app.registry.active_session.game_profile
    assert calls[0][1]["material"] == 186875565


def test_collision_dialog_requires_game_profile(monkeypatch):
    from ui.editor.panels.collision_dialog import CollisionDialog

    calls = []

    def fake_generate_collision(nif, **kwargs):
        calls.append((nif, kwargs))
        return SimpleNamespace(success=True, description="ok")

    monkeypatch.setattr(
        "creation_lib.nif.operations.collision.generate_collision",
        fake_generate_collision,
    )

    app = SimpleNamespace(nif_file=object())
    dialog = CollisionDialog(app)

    dialog._apply()

    assert calls == []
    assert dialog._log_is_error is True
    assert "game profile" in dialog._log_msg.lower()


def test_collision_dialog_exposes_auto_convex_hull_option():
    from ui.editor.panels import collision_dialog

    assert "Auto Convex Hull" in collision_dialog._SHAPE_LABELS
    idx = collision_dialog._SHAPE_LABELS.index("Auto Convex Hull")
    assert collision_dialog._SHAPE_KEYS[idx] == "convex_fit"


def test_collision_dialog_defaults_to_auto_convex_hull():
    from ui.editor.panels import collision_dialog
    from ui.editor.panels.collision_dialog import CollisionDialog

    dialog = CollisionDialog(object())

    assert collision_dialog._SHAPE_KEYS[dialog._shape_type] == "convex_fit"


def test_collision_dialog_forwards_child_ninode_mesh_toggle(monkeypatch):
    from ui.editor.panels.collision_dialog import CollisionDialog

    calls = []

    def fake_generate_collision(nif, **kwargs):
        calls.append((nif, kwargs))
        return SimpleNamespace(success=True, description="ok")

    monkeypatch.setattr(
        "creation_lib.nif.operations.collision.generate_collision",
        fake_generate_collision,
    )

    app = SimpleNamespace(
        nif_file=object(),
        registry=SimpleNamespace(active_session=SimpleNamespace(game_profile=object())),
    )
    dialog = CollisionDialog(app)
    dialog.open(1)
    dialog._include_child_nodes = False

    dialog._apply()

    assert calls
    assert calls[0][1]["source_block_ids"] is None
    assert calls[0][1]["include_child_nodes"] is False


def test_batch_collision_requires_game_profile(monkeypatch):
    from ui.editor.panels.batch_operations import BatchOperationsPanel

    calls = []

    def fake_generate_collision(nif, **kwargs):
        calls.append((nif, kwargs))
        return SimpleNamespace(success=True, description="ok")

    monkeypatch.setattr(
        "creation_lib.nif.operations.collision.generate_collision",
        fake_generate_collision,
    )

    panel = BatchOperationsPanel(SimpleNamespace())

    panel._generate_collision(object())

    assert calls == []
    assert any("game profile" in line.lower() for line in panel._log)


def test_batch_collision_forwards_child_ninode_mesh_toggle(monkeypatch):
    from ui.editor.panels.batch_operations import BatchOperationsPanel

    calls = []

    def fake_generate_collision(nif, **kwargs):
        calls.append((nif, kwargs))
        return SimpleNamespace(success=True, description="ok")

    monkeypatch.setattr(
        "creation_lib.nif.operations.collision.generate_collision",
        fake_generate_collision,
    )

    app = SimpleNamespace(
        nif_file=None,
        registry=SimpleNamespace(active_session=SimpleNamespace(game_profile=object())),
        renderer=None,
    )
    panel = BatchOperationsPanel(app)
    panel._include_child_nodes = False

    panel._generate_collision(object())

    assert calls
    assert calls[0][1]["node_block_id"] == 0
    assert calls[0][1]["include_child_nodes"] is False


def test_batch_collision_defaults_to_auto_convex_hull():
    from ui.editor.panels.batch_operations import BatchOperationsPanel

    panel = BatchOperationsPanel(SimpleNamespace())

    assert panel._collision_type == 1


def test_shape_collision_parent_id_uses_parent_ninode():
    from ui.editor.panels.scene_tree import _shape_collision_parent_id

    nif = NifFile()
    parent = NifBlock(block_id=1, type_name="NiNode")
    shape = NifBlock(block_id=2, type_name="BSTriShape")
    nif.blocks = [NifBlock(block_id=0, type_name="NiNode"), parent, shape]

    block_ops = SimpleNamespace(_find_parent=lambda _nif, _block_id: parent)

    assert _shape_collision_parent_id(nif, block_ops, 2) == 1


def test_shape_collision_parent_id_none_without_parent():
    from ui.editor.panels.scene_tree import _shape_collision_parent_id

    nif = NifFile()
    block_ops = SimpleNamespace(_find_parent=lambda _nif, _block_id: None)

    assert _shape_collision_parent_id(nif, block_ops, 2) is None
