from __future__ import annotations

import numpy as np

from creation_lib.skinning.brushes import mirror_weights, paint_weight
from creation_lib.skinning.normalization import normalize_weights
from creation_lib.skinning import partitions
from creation_lib.skinning.skin_data import SegmentInfo, SkinData, SubSegmentInfo
from ui.weight_painter.weight_painter_app import WeightPainterApp


def _skin_data(
    *,
    segment_ids: np.ndarray | None = None,
    segments: list[SegmentInfo] | None = None,
) -> SkinData:
    vertices = np.array(
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
        ],
        dtype=np.float32,
    )
    triangles = np.array([[0, 1, 2], [3, 4, 5]], dtype=np.uint32)
    return SkinData(
        vertices=vertices,
        triangles=triangles,
        normals=np.zeros_like(vertices),
        uvs=np.zeros((len(vertices), 2), dtype=np.float32),
        bone_names=["Root"],
        weights=np.ones((len(vertices), 1), dtype=np.float32),
        bone_indices=np.zeros((len(vertices), 1), dtype=np.int32),
        segment_ids=(
            np.asarray(segment_ids, dtype=np.int32)
            if segment_ids is not None
            else np.full(len(triangles), -1, dtype=np.int32)
        ),
        segments=segments or [],
    )


def test_sync_fo4_segments_from_ids_preserves_segment_metadata():
    original_segments = [
        SegmentInfo(
            start_index=0,
            num_primitives=1,
            user_index=30,
            sub_segments=[
                SubSegmentInfo(start_index=0, num_primitives=1, user_index=30),
            ],
        ),
        SegmentInfo(
            start_index=1,
            num_primitives=1,
            user_index=34,
            sub_segments=[
                SubSegmentInfo(start_index=1, num_primitives=1, user_index=34),
            ],
        ),
    ]
    skin = _skin_data(
        segment_ids=np.array([1, 0], dtype=np.int32),
        segments=original_segments,
    )

    synced = partitions.sync_fo4_segments_from_ids(skin)

    assert [seg.user_index for seg in synced] == [34, 30]
    assert [seg.sub_segments[0].user_index for seg in synced] == [34, 30]
    assert [seg.start_index for seg in synced] == [0, 1]
    assert skin.segment_ids.tolist() == [0, 1]


def test_rebuild_fo4_segments_from_body_parts_returns_segment_indices():
    skin = _skin_data()
    body_part_ids = np.array([34, 30], dtype=np.int32)

    segments, segment_ids = partitions.rebuild_fo4_segments_from_body_parts(
        skin, body_part_ids,
    )

    assert [seg.user_index for seg in segments] == [34, 30]
    assert [seg.sub_segments[0].user_index for seg in segments] == [34, 30]
    assert segment_ids.tolist() == [0, 1]


def test_normalize_weights_preserves_locked_bone_values():
    weights = np.array([[0.60, 0.80]], dtype=np.float32)
    bone_indices = np.array([[1, 2]], dtype=np.int32)

    out_w, out_bi, _ = normalize_weights(
        weights,
        bone_indices,
        max_bones=2,
        locked_bones={1},
    )

    locked_slot = int(np.where(out_bi[0] == 1)[0][0])
    unlocked_slot = int(np.where(out_bi[0] == 2)[0][0])
    assert np.isclose(out_w[0, locked_slot], np.float32(0.60))
    assert np.isclose(out_w[0, unlocked_slot], np.float32(0.40))


def test_mirror_weights_can_copy_negative_side_to_positive_side():
    weights = np.array(
        [
            [0.25, 0.75],
            [0.90, 0.10],
        ],
        dtype=np.float32,
    )
    bone_indices = np.array(
        [
            [0, 1],
            [1, 0],
        ],
        dtype=np.int32,
    )
    vertices = np.array([[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]], dtype=np.float32)

    out_w, out_bi = mirror_weights(
        weights,
        bone_indices,
        bone_names=["LArm", "RArm"],
        vertex_positions=vertices,
        axis=0,
        source_side="negative",
    )

    assert out_bi[1].tolist() == [1, 0]
    assert np.allclose(out_w[1], [0.25, 0.75])


def test_paint_weight_respects_selection_mask():
    weights = np.array([[1.0], [1.0]], dtype=np.float32)
    bone_indices = np.array([[0], [0]], dtype=np.int32)
    vertices = np.array([[0.0, 0.0, 0.0], [0.1, 0.0, 0.0]], dtype=np.float32)
    selection_mask = np.array([True, False], dtype=bool)

    out_w, _ = paint_weight(
        weights,
        bone_indices,
        bone_idx=0,
        vertex_positions=vertices,
        brush_center=np.array([0.0, 0.0, 0.0], dtype=np.float32),
        brush_radius=1.0,
        brush_strength=0.5,
        mode="subtract",
        auto_normalize=False,
        selection_mask=selection_mask,
    )

    assert out_w[0, 0] < 1.0
    assert out_w[1, 0] == 1.0


def test_weight_painter_undo_restores_mask_values():
    app = WeightPainterApp()
    app.skin_data = _skin_data()
    app.mask = np.zeros(app.skin_data.num_vertices, dtype=np.float32)

    app.push_undo("mask")
    app.mask[:] = 1.0
    app.undo()

    assert np.all(app.mask == 0.0)
