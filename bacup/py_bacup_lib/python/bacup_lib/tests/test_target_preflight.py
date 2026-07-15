from __future__ import annotations

from pathlib import Path


class FakePlugin:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.file_path = path
        self.closed = False

    def record_index_rows(self):
        return [("Fallout4.esm:01F276", "Ammo10mm", "AMMO", 0x01F276, 0x01F276)]

    def close(self) -> None:
        self.closed = True


def test_record_preflight_dedupes_by_official_master_order(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    fallout4 = data_dir / "Fallout4.esm"
    robot = data_dir / "DLCRobot.esm"
    fallout4.write_bytes(b"")
    robot.write_bytes(b"")
    opened: list[FakePlugin] = []

    def fake_load(path, *, game=None, lazy_index=False):
        plugin = FakePlugin(Path(path))
        opened.append(plugin)
        return plugin

    def fake_collect(plugin, game):
        if plugin.path.name == "Fallout4.esm":
            return [
                {
                    "editor_id": "Ammo10mm",
                    "signature": "AMMO",
                    "form_key": "01F276:Fallout4.esm",
                }
            ]
        return [
            {
                "editor_id": "ammo10mm",
                "signature": "AMMO",
                "form_key": "099999:DLCRobot.esm",
            }
        ]

    monkeypatch.setattr(target_preflight.Plugin, "load", fake_load)

    result = target_preflight.build_target_record_preflight(
        "fo4",
        target_data_dir=data_dir,
        collect_eid_rows=fake_collect,
    )

    rows = result.rows_for_native()
    assert result.master_names == ["Fallout4.esm", "DLCRobot.esm"]
    assert ("Ammo10mm", "AMMO", "01F276:Fallout4.esm") in rows
    assert ("ammo10mm", "AMMO", "099999:DLCRobot.esm") not in rows
    assert any("duplicate target EditorID/signature" in warning for warning in result.warnings)
    assert all(plugin.closed for plugin in opened)


def test_record_preflight_uses_public_record_index_rows(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    fallout4 = data_dir / "Fallout4.esm"
    fallout4.write_bytes(b"")
    opened: list[FakePlugin] = []

    def fake_load(path, *, game=None, lazy_index=False):
        assert game == "fo4"
        assert lazy_index is True
        plugin = FakePlugin(Path(path))
        opened.append(plugin)
        return plugin

    monkeypatch.setattr(target_preflight.Plugin, "load", fake_load)

    result = target_preflight.build_target_record_preflight(
        "fo4", target_data_dir=data_dir
    )

    assert ("Ammo10mm", "AMMO", "01F276:Fallout4.esm") in result.rows_for_native()
    assert opened and opened[0].closed


def test_record_preflight_adds_fo4_hardcoded_actor_values(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    fallout4 = data_dir / "Fallout4.esm"
    fallout4.write_bytes(b"")

    def fake_load(path, *, game=None, lazy_index=False):
        return FakePlugin(Path(path))

    monkeypatch.setattr(target_preflight.Plugin, "load", fake_load)

    result = target_preflight.build_target_record_preflight(
        "fo4",
        target_data_dir=data_dir,
        collect_eid_rows=lambda plugin, game: [],
    )

    rows = result.rows_for_native()
    assert ("PowerGenerated", "AVIF", "00032E:Fallout4.esm") in rows
    assert ("ReflectDamage", "AVIF", "00035F:Fallout4.esm") in rows
    assert ("PlayCredits", "GLOB", "000063:Fallout4.esm") in rows


def test_record_preflight_reuses_open_target_master_handles(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    fallout4 = data_dir / "Fallout4.esm"
    fallout4.write_bytes(b"")
    handle = FakePlugin(fallout4)

    def fail_load(path, *, game=None, lazy_index=False):
        raise AssertionError(f"unexpected load: {path}")

    monkeypatch.setattr(target_preflight.Plugin, "load", fail_load)

    result = target_preflight.build_target_record_preflight(
        "fo4",
        target_data_dir=data_dir,
        target_master_handles=[handle],
        collect_eid_rows=lambda plugin, game: [
            {
                "editor_id": "Ammo10mm",
                "signature": "AMMO",
                "form_key": "01F276:Fallout4.esm",
            }
        ],
    )

    assert ("Ammo10mm", "AMMO", "01F276:Fallout4.esm") in result.rows_for_native()
    assert not handle.closed


def test_record_preflight_ignores_exact_duplicate_rows(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    fallout4 = data_dir / "Fallout4.esm"
    fallout4.write_bytes(b"")

    def fake_load(path, *, game=None, lazy_index=False):
        return FakePlugin(Path(path))

    monkeypatch.setattr(target_preflight.Plugin, "load", fake_load)

    result = target_preflight.build_target_record_preflight(
        "fo4",
        target_data_dir=data_dir,
        collect_eid_rows=lambda plugin, game: [
            {
                "editor_id": "Ammo10mm",
                "signature": "AMMO",
                "form_key": "01F276:Fallout4.esm",
            },
            {
                "editor_id": "ammo10mm",
                "signature": "AMMO",
                "form_key": "01F276:Fallout4.esm",
            },
        ],
    )

    assert result.rows_for_native().count(("Ammo10mm", "AMMO", "01F276:Fallout4.esm")) == 1
    assert not result.warnings


def test_record_preflight_keeps_physical_actor_value_over_hardcoded_row(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    fallout4 = data_dir / "Fallout4.esm"
    fallout4.write_bytes(b"")

    def fake_load(path, *, game=None, lazy_index=False):
        return FakePlugin(Path(path))

    def fake_collect(plugin, game):
        return [
            {
                "editor_id": "PowerGenerated",
                "signature": "AVIF",
                "form_key": "123456:Fallout4.esm",
            }
        ]

    monkeypatch.setattr(target_preflight.Plugin, "load", fake_load)

    result = target_preflight.build_target_record_preflight(
        "fo4",
        target_data_dir=data_dir,
        collect_eid_rows=fake_collect,
    )

    assert result.records[("powergenerated", "AVIF")].form_key == "123456:Fallout4.esm"


def test_record_preflight_keeps_dlc_owner_when_base_missing(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    nuka = data_dir / "DLCNukaWorld.esm"
    nuka.write_bytes(b"")

    def fake_load(path, *, game=None, lazy_index=False):
        return FakePlugin(Path(path))

    def fake_collect(plugin, game):
        return [
            {
                "editor_id": "DLC04_Ammo_HandmadeRound",
                "signature": "AMMO",
                "form_key": "037897:DLCNukaWorld.esm",
            }
        ]

    monkeypatch.setattr(target_preflight.Plugin, "load", fake_load)

    result = target_preflight.build_target_record_preflight(
        "fo4",
        target_data_dir=data_dir,
        collect_eid_rows=fake_collect,
    )

    assert result.master_names == ["DLCNukaWorld.esm"]
    assert result.rows_for_native() == [
        ("DLC04_Ammo_HandmadeRound", "AMMO", "037897:DLCNukaWorld.esm")
    ]


def test_normalize_asset_key_matches_case_insensitive_data_paths():
    from bacup_lib.target_preflight import normalize_asset_key

    assert normalize_asset_key(
        r"X:\extracted\fo4\Meshes\DLC03\Landscape\Plants\Bramble01.nif",
        root=r"X:\extracted\fo4",
    ) == "meshes/dlc03/landscape/plants/bramble01.nif"
    assert normalize_asset_key(
        r"X:\extracted\fo76\meshes\dlc03\landscape\plants\bramble01.nif",
        root=r"X:\extracted\fo76",
    ) == "meshes/dlc03/landscape/plants/bramble01.nif"


def test_target_asset_index_matches_asset_ref(tmp_path):
    from types import SimpleNamespace
    from bacup_lib.target_preflight import build_target_asset_index

    target_file = tmp_path / "Meshes" / "DLC03" / "Landscape" / "Plants" / "Bramble01.nif"
    target_file.parent.mkdir(parents=True)
    target_file.write_bytes(b"nif")

    index = build_target_asset_index(tmp_path)

    assert index.has_asset(
        SimpleNamespace(
            asset_type="nif",
            source_path="meshes/dlc03/landscape/plants/bramble01.nif",
        )
    )


def test_target_asset_index_matches_absolute_source_extracted_path(tmp_path):
    from types import SimpleNamespace
    from bacup_lib.target_preflight import build_target_asset_index

    target_root = tmp_path / "extracted" / "fo4"
    source_root = tmp_path / "extracted" / "fo76"
    target_file = target_root / "Meshes" / "DLC03" / "Landscape" / "Plants" / "Bramble01.nif"
    target_file.parent.mkdir(parents=True)
    target_file.write_bytes(b"nif")

    index = build_target_asset_index(target_root)

    assert index.has_asset(
        SimpleNamespace(
            asset_type="nif",
            source_path=source_root / "meshes" / "dlc03" / "landscape" / "plants" / "bramble01.nif",
        )
    )


def test_target_asset_index_discovers_lowercase_target_dirs(tmp_path):
    from types import SimpleNamespace
    from bacup_lib.target_preflight import build_target_asset_index

    target_file = tmp_path / "meshes" / "DLC03" / "Landscape" / "Plants" / "Bramble01.nif"
    target_file.parent.mkdir(parents=True)
    target_file.write_bytes(b"nif")

    index = build_target_asset_index(tmp_path)

    assert index.has_asset(
        SimpleNamespace(
            asset_type="nif",
            source_path="Meshes/DLC03/Landscape/Plants/Bramble01.nif",
        )
    )


def test_target_asset_index_matches_stripped_nif_ref(tmp_path):
    from types import SimpleNamespace
    from bacup_lib.target_preflight import build_target_asset_index

    target_file = tmp_path / "Meshes" / "DLC03" / "Landscape" / "Plants" / "Bramble01.nif"
    target_file.parent.mkdir(parents=True)
    target_file.write_bytes(b"nif")

    index = build_target_asset_index(tmp_path)

    assert index.has_asset(
        SimpleNamespace(
            asset_type="nif",
            source_path="dlc03/landscape/plants/bramble01.nif",
        )
    )


def test_target_asset_index_matches_stripped_texture_ref(tmp_path):
    from types import SimpleNamespace
    from bacup_lib.target_preflight import build_target_asset_index

    target_file = tmp_path / "Textures" / "DLC03" / "Plants" / "Bramble01_d.dds"
    target_file.parent.mkdir(parents=True)
    target_file.write_bytes(b"dds")

    index = build_target_asset_index(tmp_path)

    assert index.has_asset(
        SimpleNamespace(
            asset_type="texture",
            source_path="dlc03/plants/bramble01_d.dds",
        )
    )


def test_target_asset_index_indexes_extracted_and_data_roots(tmp_path):
    from types import SimpleNamespace
    from bacup_lib.target_preflight import build_target_asset_index

    extracted = tmp_path / "extracted" / "fo4"
    data = tmp_path / "Fallout4" / "Data"
    extracted_texture = (
        extracted / "Textures" / "DLC05" / "Architecture" / "WarehouseGarageDoor01_d.DDS"
    )
    data_texture = data / "Textures" / "Custom" / "Panel_d.dds"
    extracted_texture.parent.mkdir(parents=True)
    data_texture.parent.mkdir(parents=True)
    extracted_texture.write_bytes(b"dds")
    data_texture.write_bytes(b"dds")

    index = build_target_asset_index([extracted, data])

    assert index.has_asset(
        SimpleNamespace(
            asset_type="texture",
            source_path="textures/dlc05/architecture/warehousegaragedoor01_d.dds",
        )
    )
    assert index.has_asset(
        SimpleNamespace(
            asset_type="texture",
            source_path="textures/custom/panel_d.dds",
        )
    )


def test_target_asset_index_uses_parallel_workers(tmp_path):
    from types import SimpleNamespace
    from bacup_lib.target_preflight import build_target_asset_index

    mesh = tmp_path / "Meshes" / "Actors" / "Robot" / "Protectron.nif"
    texture = tmp_path / "Textures" / "Actors" / "Robot" / "Protectron_d.dds"
    mesh.parent.mkdir(parents=True)
    texture.parent.mkdir(parents=True)
    mesh.write_bytes(b"nif")
    texture.write_bytes(b"dds")

    index = build_target_asset_index(tmp_path, workers=4)

    assert index.workers == 4
    assert index.files_scanned == 2
    assert index.has_asset(
        SimpleNamespace(
            asset_type="nif",
            source_path="meshes/actors/robot/protectron.nif",
        )
    )
    assert index.has_asset(
        SimpleNamespace(
            asset_type="texture",
            source_path="textures/actors/robot/protectron_d.dds",
        )
    )
