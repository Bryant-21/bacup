from __future__ import annotations

from bacup_lib.nif import lookup as nif_lookup


class FakeConnection:
    def __init__(self, results):
        self.results = list(results)
        self.queries = []
        self.closed = False

    def query_all(self, sql, params=None):
        self.queries.append((sql, params))
        return self.results.pop(0)

    def close(self):
        self.closed = True


def test_nif_lookup_queries_indexed_nif_id(monkeypatch, tmp_path):
    conn = FakeConnection([[{"v": "textures/foo.dds"}]])
    db_path = tmp_path / "fo76_nifs.db"
    db_path.write_bytes(b"")
    monkeypatch.setattr(nif_lookup.Database, "open", staticmethod(lambda *_args: conn))

    lookup = nif_lookup.NifIndexLookup(str(db_path), "fo76")

    assert lookup.get_textures("Landscape/Grass/Foo.NIF") == ["textures/foo.dds"]
    sql, params = conn.queries[0]
    assert "LOWER(" not in sql
    assert "JOIN nifs" not in sql
    assert "WHERE nif_id = ?" in sql
    assert params == ["fo76/landscape/grass/foo.nif"]


def test_nif_lookup_path_fallback_avoids_lower_expression(monkeypatch, tmp_path):
    conn = FakeConnection([[], [{"v": "materials/foo.bgsm"}]])
    db_path = tmp_path / "fo76_nifs.db"
    db_path.write_bytes(b"")
    monkeypatch.setattr(nif_lookup.Database, "open", staticmethod(lambda *_args: conn))

    lookup = nif_lookup.NifIndexLookup(str(db_path), "fo76")

    assert lookup.get_materials("Landscape/Grass/Foo.NIF") == ["materials/foo.bgsm"]
    sql, params = conn.queries[1]
    assert "LOWER(" not in sql
    assert "WHERE n.path = ?" in sql
    assert params == ["landscape/grass/foo.nif"]


def test_nif_lookup_normalizes_secondary_asset_paths(monkeypatch, tmp_path):
    db_path = tmp_path / "fo76_nifs.db"
    db_path.write_bytes(b"")
    lookup = nif_lookup.NifIndexLookup(str(db_path), "fo76")
    monkeypatch.setattr(
        lookup,
        "get_textures",
        lambda _nif_path: ["data/textures/Vehicles/Truck_d.dds"],
    )
    monkeypatch.setattr(
        lookup,
        "get_materials",
        lambda _nif_path: [
            "C:/Projects/76/Build/PC/Materials/Landscape/Ground/TEMP_GroundTexture01Decal.BGSM"
        ],
    )
    monkeypatch.setattr(lookup, "get_behaviors", lambda _nif_path: [])

    assets = lookup.get_secondary_assets("Landscape/Grass/Foo.NIF")

    assert [(asset.asset_type, asset.source_path) for asset in assets] == [
        ("texture", "textures/Vehicles/Truck_d.dds"),
        ("material", "Materials/Landscape/Ground/TEMP_GroundTexture01Decal.BGSM"),
    ]
