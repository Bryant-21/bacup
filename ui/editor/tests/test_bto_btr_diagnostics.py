from creation_lib.nif.nif_file import NifBlock, NifFile
from ui.editor.panels.properties_header import _collect_bto_btr_diagnostics


def _block(block_id: int, type_name: str, fields: dict, remainder: bytes = b""):
    return NifBlock(
        block_id=block_id,
        type_name=type_name,
        fields=list(fields.items()),
        _remainder=remainder,
    )


def test_collect_bto_diagnostics_reports_lod_tile_fields_and_remainders():
    nif = NifFile()
    nif.blocks = [
        _block(
            0,
            "BSSubIndexTriShape",
            {
                "Name": "obj",
                "Num Vertices": 12,
                "Num Triangles": 5,
                "Num Segments": 2,
                "Segment": [{}, {}],
                "Vertex Desc": 0x1B00000430205,
                "Translation": {"x": 1.0, "y": 2.0, "z": 3.0},
                "Scale": 1.0,
                "Shader Property": 1,
                "Alpha Property": -1,
            },
        ),
        _block(
            1,
            "BSLightingShaderProperty",
            {"Shader Type": 0, "Shader Flags 1:FO4": 0x80400201},
            remainder=b"\xAA\xBB",
        ),
        _block(
            2,
            "BSShaderTextureSet",
            {"Textures": [r"Data\textures\terrain\w\objects\w.objects.dds"]},
        ),
    ]

    diagnostics = _collect_bto_btr_diagnostics(
        nif,
        r"Meshes\Terrain\W\Objects\W.16.-9.5.Winter.bto",
    )

    assert diagnostics is not None
    assert diagnostics["kind"] == "Object LOD (.bto)"
    assert diagnostics["tile"] == {
        "world": "W",
        "level": 16,
        "x": -9,
        "y": 5,
        "season": ".Winter",
    }
    assert diagnostics["totals"] == {"vertices": 12, "triangles": 5, "segments": 2}
    assert diagnostics["remainders"] == [
        {"block_id": 1, "type": "BSLightingShaderProperty", "bytes": 2}
    ]
    assert diagnostics["texture_paths"] == [
        r"Data\textures\terrain\w\objects\w.objects.dds"
    ]
    assert "Shader Flags 1:FO4" in dict(diagnostics["fields_by_type"])[
        "BSLightingShaderProperty"
    ]
    assert "segments=2" in diagnostics["shape_lines"][0]
    assert "vertex_desc=0x1B00000430205" in diagnostics["shape_lines"][0]


def test_collect_btr_diagnostics_uses_array_lengths_when_counts_are_missing():
    nif = NifFile()
    nif.blocks = [
        _block(
            0,
            "BSTriShape",
            {
                "Name": "Land",
                "Vertex Data": [{}, {}, {}],
                "Triangles": [{}, {}],
                "Vertex Desc": 0x300000000203,
                "Translation": {"x": 0.0, "y": 0.0, "z": 128.0},
                "Scale": 16.0,
                "Shader Property": 1,
                "Alpha Property": -1,
            },
        ),
        _block(1, "BSLightingShaderProperty", {"Shader Type": 18}),
    ]

    diagnostics = _collect_bto_btr_diagnostics(
        nif,
        r"Meshes\Terrain\Commonwealth\Commonwealth.16.-16.-16.btr",
    )

    assert diagnostics is not None
    assert diagnostics["kind"] == "Terrain LOD (.btr)"
    assert diagnostics["tile"] == {
        "world": "Commonwealth",
        "level": 16,
        "x": -16,
        "y": -16,
        "season": "",
    }
    assert diagnostics["totals"] == {"vertices": 3, "triangles": 2, "segments": 0}
    assert diagnostics["shape_lines"] == [
        "[0] BSTriShape 'Land': verts=3, tris=2, "
        "vertex_desc=0x300000000203, translation=(0.00, 0.00, 128.00), "
        "scale=16.0, shader=1"
    ]


def test_collect_bto_btr_diagnostics_ignores_plain_nif():
    assert _collect_bto_btr_diagnostics(NifFile(), "weapon.nif") is None
