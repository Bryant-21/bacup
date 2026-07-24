from creation_lib.renderer.scene_renderer import SceneRenderer
from ui.editor.selection import CollisionShapeSelection, SelectionManager


def test_virtual_collision_shape_selection_tracks_body_and_sub_shape():
    manager = SelectionManager()
    notifications = []
    manager.on_selection_changed(
        lambda nif_id, block_id: notifications.append((nif_id, block_id))
    )

    manager.select_collision_shape("main", 31, body_id=2, shape_index=4)

    assert manager.selected is None
    assert manager.selected_nif_id == "main"
    assert manager.selected_block_id == 31
    assert manager.selected_collision_shape == CollisionShapeSelection(
        nif_id="main",
        block_id=31,
        body_id=2,
        shape_index=4,
    )
    assert notifications == [("main", 31)]


def test_renderer_selects_one_virtual_collision_mesh_or_whole_body():
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer._collision_shape_meshes = [
        {
            "nif_id": "main",
            "source_block_id": 31,
            "body_id": 2,
            "shape_index": 0,
        },
        {
            "nif_id": "main",
            "source_block_id": 31,
            "body_id": 2,
            "shape_index": 1,
        },
    ]
    renderer.selection_mgr = SelectionManager()

    renderer.selection_mgr.select_collision_shape("main", 31, 2, 1)
    token, selected = renderer._selected_collision_meshes()
    assert token == ("main", 31, 2, 1)
    assert [shape["shape_index"] for shape in selected] == [1]

    renderer.selection_mgr.select_collision_shape("main", 31, 2, None)
    _token, selected = renderer._selected_collision_meshes()
    assert [shape["shape_index"] for shape in selected] == [0, 1]


def test_selecting_real_collision_block_matches_legacy_shape_mesh():
    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer._collision_shape_meshes = [
        {
            "nif_id": "main",
            "source_block_id": 8,
            "body_id": None,
            "shape_index": None,
        },
        {
            "nif_id": "main",
            "source_block_id": 9,
            "body_id": None,
            "shape_index": None,
        },
    ]
    renderer.selection_mgr = SelectionManager()

    renderer.selection_mgr.select_by_id("main", 9)

    _token, selected = renderer._selected_collision_meshes()
    assert [shape["source_block_id"] for shape in selected] == [9]
