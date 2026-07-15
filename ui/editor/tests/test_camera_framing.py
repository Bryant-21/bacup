from creation_lib.renderer.camera import OrbitCamera


def test_frame_on_bounds_expands_far_plane_for_large_bounds():
    camera = OrbitCamera()

    camera.frame_on_bounds((0.0, 0.0, 0.0), 5203.5)

    assert camera.distance == 13008.75
    assert camera.far > camera.distance + 5203.5


def test_zoom_out_keeps_far_plane_ahead_of_framed_scene():
    camera = OrbitCamera()

    camera.frame_on_bounds((0.0, 0.0, 0.0), 5203.5)

    for _ in range(5):
        camera.zoom(-1.0)

    assert camera.distance > 20814.0
    assert camera.far > camera.distance + 5203.5
