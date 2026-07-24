from ui.editor.panels import collision_info


class _Block:
    def __init__(self, block_id, type_name, fields):
        self.block_id = block_id
        self.type_name = type_name
        self._fields = fields

    def get_field(self, name):
        return self._fields.get(name)


class _Nif:
    def __init__(self, blocks):
        self._blocks = blocks

    def get_block(self, block_id):
        return self._blocks.get(block_id)


def test_physics_system_summary_reports_multi_body_shapes(monkeypatch):
    monkeypatch.setattr(
        collision_info,
        "_parse_packfile_summary",
        lambda _blob, _block_id=-1: {
            "shape_kind": "multi_body_mixed",
            "objects": [],
            "bodies": [
                {
                    "body_id": 0,
                    "shape_class": "hknpCompressedMeshShape",
                    "layer": 1,
                    "material_crc": 4146539321,
                },
                {"body_id": 1, "shape_class": "hknpConvexPolytopeShape", "layer": 2},
            ],
            "n_subshapes": 2,
            "blob_size": 2,
        },
    )
    block = _Block(31, "bhkPhysicsSystem", {"Binary Data": {"Data": [0, 1]}})

    suffix, lines = collision_info.summarize_collision_block(None, block)

    assert suffix == "(2 bodies: compressed mesh + convex polytope)"
    assert "Body 0: compressed mesh, STATIC (1), WeaponPistol (4146539321)" in lines
    assert "Body 0 Material Type: MaterialWeaponPistol" in lines
    assert "Body 1: convex polytope, ANIMSTATIC (2)" in lines


def test_np_collision_object_summary_reports_body_layer(monkeypatch):
    monkeypatch.setattr(
        collision_info,
        "_parse_packfile_summary",
        lambda _blob, _block_id=-1: {
            "shape_kind": "multi_body_mixed",
            "objects": [],
            "bodies": [
                {
                    "body_id": 0,
                    "shape_class": "hknpCompressedMeshShape",
                    "layer": 1,
                    "material_crc": 4146539321,
                },
            ],
            "n_subshapes": 1,
            "blob_size": 2,
        },
    )
    phys = _Block(31, "bhkPhysicsSystem", {"Binary Data": {"Data": [0, 1]}})
    coll = _Block(30, "bhkNPCollisionObject", {"Data": 31, "Body ID": 0})
    nif = _Nif({31: phys})

    assert (
        collision_info.summarize_np_body_shape(nif, coll)
        == "compressed mesh, STATIC (1), WeaponPistol (4146539321)"
    )


def test_physics_system_summary_reports_material_type(monkeypatch):
    monkeypatch.setattr(
        collision_info,
        "_parse_packfile_summary",
        lambda _blob, _block_id=-1: {
            "shape_kind": "compressed_mesh",
            "objects": [],
            "bodies": [
                {
                    "body_id": 0,
                    "shape_class": "hknpCompressedMeshShape",
                    "layer": 1,
                    "material_crc": 4146539321,
                },
            ],
            "n_subshapes": None,
            "blob_size": 2,
        },
    )
    block = _Block(31, "bhkPhysicsSystem", {"Binary Data": {"Data": [0, 1]}})

    _suffix, lines = collision_info.summarize_collision_block(None, block)

    assert "Material: WeaponPistol (4146539321)" in lines
    assert "Material Type: MaterialWeaponPistol" in lines


def test_physics_system_summary_labels_empty_polytope_as_unsupported(monkeypatch):
    monkeypatch.setattr(
        collision_info,
        "_parse_packfile_summary",
        lambda _blob, _block_id=-1: {
            "shape_kind": "convex_polytope",
            "geometry_status": "empty_polytope",
            "objects": [
                {
                    "class_name": "hknpConvexPolytopeShape",
                    "n_vertices": 0,
                    "n_faces": 0,
                    "n_planes": 0,
                    "n_instances": None,
                },
            ],
            "bodies": [],
            "n_subshapes": None,
            "blob_size": 2,
        },
    )
    block = _Block(31, "bhkPhysicsSystem", {"Binary Data": {"Data": [0, 1]}})

    suffix, lines = collision_info.summarize_collision_block(None, block)

    assert suffix == "(Unsupported/empty polytope)"
    assert "Shape: hknpConvexPolytopeShape (no decoded geometry)" in lines


def test_physics_system_shapes_expose_selectable_compound_children(monkeypatch):
    monkeypatch.setattr(
        collision_info,
        "_parse_packfile_summary",
        lambda _blob, _block_id=-1: {
            "shape_kind": "compound_polytope",
            "objects": [
                {"class_name": "hknpDynamicCompoundShape"},
                {
                    "class_name": "hknpConvexPolytopeShape",
                    "n_vertices": 12,
                    "n_faces": 8,
                },
                {"class_name": "hknpCapsuleShape"},
            ],
            "bodies": [
                {
                    "body_id": 3,
                    "shape_class": "hknpDynamicCompoundShape",
                    "layer": 4,
                    "material_crc": 4146539321,
                    "bs_materials": [],
                }
            ],
            "n_subshapes": 2,
        },
    )
    monkeypatch.setattr(
        collision_info,
        "_parse_packfile_previews",
        lambda _blob, _block_id, _body_id: [
            {
                "shape_type": "convex_hull",
                "mesh": {"vertices": [{}] * 20, "triangles": [{}] * 16},
            },
            {
                "shape_type": "capsule",
                "mesh": {"vertices": [{}] * 24, "triangles": [{}] * 32},
            },
        ],
    )
    block = _Block(31, "bhkPhysicsSystem", {"Binary Data": {"Data": [0, 1]}})

    bodies = collision_info.inspect_physics_system_shapes(block)

    assert len(bodies) == 1
    body = bodies[0]
    assert body.display_type == "Dynamic Compound"
    assert body.layer == "CLUTTER (4)"
    assert body.materials == ("WeaponPistol (4146539321)",)
    assert [shape.display_type for shape in body.sub_shapes] == [
        "Convex Polytope",
        "Capsule",
    ]
    assert body.sub_shapes[0].vertex_count == 12
    assert body.sub_shapes[0].triangle_count == 8
    assert body.sub_shapes[1].vertex_count == 24
    assert body.sub_shapes[1].triangle_count == 32


def test_find_physics_system_shape_returns_body_or_sub_shape(monkeypatch):
    child = collision_info.HavokShapeInfo(
        body_id=2,
        shape_index=0,
        class_name="hknpCompressedMeshShape",
        display_type="Compressed Mesh",
        layer=None,
        materials=(),
        material_types=(),
    )
    body = collision_info.HavokShapeInfo(
        body_id=2,
        shape_index=None,
        class_name="hknpDynamicCompoundShape",
        display_type="Dynamic Compound",
        layer=None,
        materials=(),
        material_types=(),
        sub_shapes=(child,),
    )
    monkeypatch.setattr(
        collision_info,
        "inspect_physics_system_shapes",
        lambda _block: (body,),
    )

    assert collision_info.find_physics_system_shape(object(), 2, None) is body
    assert collision_info.find_physics_system_shape(object(), 2, 0) is child
