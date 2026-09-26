import hashlib
import json

from bacup_lib import respawn_ui as ui
from bacup_lib.reputation_ui import TRANSLATION_KEYS
from bacup_lib.translations import translation_path


def fixtures(tmp_path):
    source = tmp_path / "source"
    interface = source / "interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text(
        "\n".join(f"{key}\t{key[1:]}" for key in sorted(TRANSLATION_KEYS)),
        encoding="utf-16",
    )
    mesh = source / "Meshes" / ui.BAG_MODEL
    mesh.parent.mkdir(parents=True)
    mesh.write_bytes(b"installed FO76 source mesh")
    output = tmp_path / "converted"
    for asset in ui.BAG_ASSETS:
        path = output / "data" / asset
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(f"converted {asset}".encode())
    return source, output


def test_conversion_emits_versioned_bridge_and_only_references_native_assets(tmp_path, monkeypatch):
    source, output = fixtures(tmp_path)
    monkeypatch.setattr(ui, "bag_record", lambda *_: {
        "source": "SeventySix.esm:365BCC",
        "converted": "SeventySix.esm:365BCC",
        "model": ui.BAG_MODEL.replace("/", "\\"),
    })
    first = ui.convert_respawn_ui(
        source, output, source_plugin=tmp_path / "source.esm",
        converted_plugin=tmp_path / "converted.esm",
    )
    second = ui.convert_respawn_ui(
        source, output, source_plugin=tmp_path / "source.esm",
        converted_plugin=tmp_path / "converted.esm",
    )
    assert first == second
    assert first["version"] == 1
    assert [entry["path"] for entry in first["files"]] == [ui.CATALOG.as_posix()]
    assert first["source_mesh_sha256"] == hashlib.sha256(
        (source / "Meshes" / ui.BAG_MODEL).read_bytes()
    ).hexdigest()
    catalog = json.loads((output / ui.CATALOG).read_text(encoding="utf-8"))
    assert catalog["assets"] == list(ui.BAG_ASSETS)
    assert catalog["converted"] == "SeventySix.esm:365BCC"
    assert catalog["labels"]["$B21_RespawnTitle"] == "Respawn"
    assert catalog["map_contract"] == {"version": 1, "transport": "F4SE", "message_type": "42325444"}
    assert not (output / "PrismaUI_F4").exists()
    for asset in first["assets"]:
        assert asset["sha256"] == hashlib.sha256((output / "data" / asset["path"]).read_bytes()).hexdigest()
    assert "$B21_RespawnTitle\t" in translation_path(output / "data").read_text(encoding="utf-16")
    assert not (tmp_path / "tales" / "Meshes").exists()
