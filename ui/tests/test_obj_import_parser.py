from __future__ import annotations

import numpy as np

from creation_lib.skinning.importers import import_obj
from ui.editor.exporters.obj_export import import_obj_to_shape


class _Block:
    def __init__(self) -> None:
        self.fields: dict[str, object] = {}

    def set_field(self, name: str, value: object) -> None:
        self.fields[name] = value


class _Nif:
    def __init__(self, block: _Block) -> None:
        self._block = block

    def get_block(self, block_id: int) -> _Block | None:
        return self._block if block_id == 7 else None


def test_obj_importers_expand_face_corner_uv_and_normal_indices(tmp_path):
    obj_path = tmp_path / "split_corner.obj"
    obj_path.write_text(
        "\n".join(
            [
                "v 0 0 0",
                "v 1 0 0",
                "v 1 1 0",
                "v 0 1 0",
                "vt 0 0",
                "vt 1 0",
                "vt 1 1",
                "vt 0 1",
                "vt 0 0.5",
                "vt 1 0.5",
                "vn 0 0 1",
                "vn 0 0 -1",
                "f 1/1/1 2/2/1 3/3/1",
                "f 1/5/2 3/6/2 4/4/2",
            ]
        ),
        encoding="utf-8",
    )

    skin = import_obj(obj_path)
    assert skin.num_vertices == 6
    np.testing.assert_array_equal(
        skin.triangles,
        np.array([[0, 1, 2], [3, 4, 5]], dtype=np.uint32),
    )
    np.testing.assert_allclose(skin.normals[0], [0, 0, 1])
    np.testing.assert_allclose(skin.normals[3], [0, 0, -1])

    block = _Block()
    count = import_obj_to_shape(_Nif(block), 7, str(obj_path))

    assert count == 6
    assert block.fields["Num Vertices"] == 6
    assert block.fields["Triangles"] == [
        {"v1": 0, "v2": 1, "v3": 2},
        {"v1": 3, "v2": 4, "v3": 5},
    ]
    vertex_data = block.fields["Vertex Data"]
    assert vertex_data[0]["Normal"] == {"x": 0.0, "y": 0.0, "z": 1.0}
    assert vertex_data[3]["Normal"] == {"x": 0.0, "y": 0.0, "z": -1.0}
    assert vertex_data[0]["UV"] == {"u": 0.0, "v": 1.0}
    assert vertex_data[3]["UV"] == {"u": 0.0, "v": 0.5}
