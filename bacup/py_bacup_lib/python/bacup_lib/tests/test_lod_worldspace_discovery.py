from __future__ import annotations

import json
from types import SimpleNamespace

import pytest

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import RegenOptions


def _fo76_settings() -> dict:
    return {
        "global": {
            "worldspaces": ["APPALACHIA"],
            "stride": 512,
            "southwest_cell": [-190, -221],
            "bounds": {"w": -190, "s": -221, "e": 252, "n": 135},
            "generate_trees": False,
        },
        "objects": {"source": "fo76_bto_atlas"},
        "trees": {"trees_3d": False},
    }


@pytest.mark.parametrize("pair_id", ["fnvfo3:fo4", "skyrimse:fo4"])
def test_cross_game_lod_settings_discover_and_scrub_fo76_fields(pair_id):
    prepared = regen_pipeline._prepare_lod_generation_settings(
        RegenOptions(lod_mode="hybrid-atlas"),
        _fo76_settings(),
        None,
        6,
        pair_id=pair_id,
    )

    assert prepared is not None
    worldspaces, settings, uses_fo76_bto, discover = prepared
    assert worldspaces == []
    assert discover is True
    assert uses_fo76_bto is False
    assert settings["global"]["worldspaces"] == []
    assert settings["global"]["stride"] is None
    assert settings["global"]["southwest_cell"] is None
    assert settings["global"]["bounds"] is None
    assert settings["global"]["generate_trees"] is True
    assert settings["objects"]["source"] == "records"
    assert settings["objects"]["fo76_bto_atlas_pages"] is False
    assert settings["trees"]["trees_3d"] is True


def test_fo76_default_discovers_worldspaces_but_keeps_tuned_profile():
    prepared = regen_pipeline._prepare_lod_generation_settings(
        RegenOptions(lod_mode="hybrid-atlas"),
        _fo76_settings(),
        None,
        8,
        pair_id="fo76:fo4",
    )

    assert prepared is not None
    worldspaces, settings, uses_fo76_bto, discover = prepared
    assert worldspaces == []
    assert discover is True
    assert uses_fo76_bto is True
    assert settings["global"]["stride"] == 512
    assert settings["global"]["southwest_cell"] == [-190, -221]
    assert settings["objects"]["source"] == "fo76_bto_atlas"


def test_explicit_lod_worldspaces_override_discovery():
    prepared = regen_pipeline._prepare_lod_generation_settings(
        RegenOptions(lod_mode="generate"),
        _fo76_settings(),
        ["Mojave", "DCWorld"],
        4,
        pair_id="fnvfo3:fo4",
    )

    assert prepared is not None
    worldspaces, settings, uses_fo76_bto, discover = prepared
    assert worldspaces == ["Mojave", "DCWorld"]
    assert settings["global"]["worldspaces"] == ["Mojave", "DCWorld"]
    assert discover is False
    assert uses_fo76_bto is False


def test_discovery_expands_all_eligible_worldspaces(monkeypatch, tmp_path):
    plugin = tmp_path / "Skyrim_Merged.esm"
    logs = []
    monkeypatch.setattr(
        "creation_lib.lod.native_runtime.discover_worldspaces",
        lambda plugin_path, game: (
            "Tamriel",
            "Blackreach",
            "tamriel",
            "DLC01SoulCairn",
        ),
    )

    result = regen_pipeline._resolve_lod_worldspaces(
        [],
        discover_from_plugin=True,
        working_esm=plugin,
        runner_log=lambda level, message: logs.append((level, message)),
    )

    assert result == ["Tamriel", "Blackreach", "DLC01SoulCairn"]
    assert logs == [
        (
            "INFO",
            "lodgen: discovered 3 worldspace(s): Tamriel, Blackreach, DLC01SoulCairn",
        )
    ]


def test_explicit_worldspaces_do_not_call_discovery(monkeypatch, tmp_path):
    monkeypatch.setattr(
        "creation_lib.lod.native_runtime.discover_worldspaces",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(
            AssertionError("explicit override must not discover")
        ),
    )

    assert regen_pipeline._resolve_lod_worldspaces(
        ["Mojave"],
        discover_from_plugin=False,
        working_esm=tmp_path / "FNV_FO3_Merged.esm",
        runner_log=lambda *_args: None,
    ) == ["Mojave"]


def test_empty_discovery_fails_clearly(monkeypatch, tmp_path):
    plugin = tmp_path / "Empty.esm"
    monkeypatch.setattr(
        "creation_lib.lod.native_runtime.discover_worldspaces",
        lambda *_args, **_kwargs: (),
    )

    with pytest.raises(RuntimeError, match="no eligible worldspaces") as exc_info:
        regen_pipeline._resolve_lod_worldspaces(
            [],
            discover_from_plugin=True,
            working_esm=plugin,
            runner_log=lambda *_args: None,
        )
    assert str(plugin) in str(exc_info.value)


def test_fo76_world_settings_use_bto_when_available_and_records_otherwise(
    monkeypatch, tmp_path
):
    generated = []
    logs = []

    def fake_generate(world, settings, **kwargs):
        generated.append((world, settings, kwargs))
        return SimpleNamespace(
            btr=1,
            bto=1,
            dds=2,
            lod_written=True,
            warnings=(),
        )

    monkeypatch.setattr("creation_lib.lod.native_runtime.generate_lod", fake_generate)
    monkeypatch.setattr(
        "creation_lib.lod.native_runtime.count_fo76_bto_tiles",
        lambda _root, world: 12 if world == "APPALACHIA" else 0,
    )

    regen_pipeline._run_generate_lod(
        mod_root=tmp_path / "mod",
        worldspaces=["APPALACHIA", "EXM1PittWorldspace"],
        working_esm=tmp_path / "mod" / "SeventySix.esm",
        asset_dirs=[],
        settings=_fo76_settings(),
        runner_log=lambda level, message: logs.append((level, message)),
        source_data_dir=tmp_path / "extracted" / "fo76",
        fo76_profile=True,
    )

    appalachia = generated[0][1]
    pitt = generated[1][1]
    assert appalachia["global"]["stride"] == 512
    assert appalachia["global"]["bounds"] is not None
    assert appalachia["global"]["generate_trees"] is False
    assert appalachia["objects"]["source"] == "fo76_bto_atlas"
    assert appalachia["trees"]["trees_3d"] is False
    assert pitt["global"]["stride"] is None
    assert pitt["global"]["southwest_cell"] is None
    assert pitt["global"]["bounds"] is None
    assert pitt["global"]["generate_trees"] is True
    assert pitt["objects"]["source"] == "records"
    assert pitt["trees"]["trees_3d"] is True
    assert any(
        level == "WARN"
        and "EXM1PittWorldspace: no matching extracted source BTOs" in message
        for level, message in logs
    )


def test_generate_lod_forwards_object_lod_overlay(monkeypatch, tmp_path):
    generated = []

    def fake_generate(world, settings, **kwargs):
        generated.append((world, settings, kwargs))
        return SimpleNamespace(
            btr=1,
            bto=1,
            dds=1,
            lod_written=True,
            warnings=(),
        )

    monkeypatch.setattr("creation_lib.lod.native_runtime.generate_lod", fake_generate)
    overlay = tmp_path / "mod" / ".modkit" / "object_lod_overlay.v1.json"

    regen_pipeline._run_generate_lod(
        mod_root=tmp_path / "mod",
        worldspaces=["Mojave"],
        working_esm=tmp_path / "mod" / "Output.esm",
        asset_dirs=[],
        settings={"global": {}},
        runner_log=lambda *_args: None,
        object_lod_overlay=overlay,
    )

    assert generated[0][2]["object_lod_overlay"] == str(overlay)


def test_generate_lod_without_overlay_preserves_default(monkeypatch, tmp_path):
    generated = []

    def fake_generate(_world, _settings, **kwargs):
        generated.append(kwargs)
        return SimpleNamespace(
            btr=1,
            bto=1,
            dds=1,
            lod_written=True,
            warnings=(),
        )

    monkeypatch.setattr("creation_lib.lod.native_runtime.generate_lod", fake_generate)
    regen_pipeline._run_generate_lod(
        mod_root=tmp_path / "mod",
        worldspaces=["Mojave"],
        working_esm=tmp_path / "mod" / "Output.esm",
        asset_dirs=[],
        settings={"global": {}},
        runner_log=lambda *_args: None,
    )

    assert generated[0]["object_lod_overlay"] is None


def test_object_lod_overlay_scope_cleans_success_and_stale_file(tmp_path):
    overlay = tmp_path / ".modkit" / "object_lod_overlay.v1.json"
    overlay.parent.mkdir(parents=True)
    overlay.write_text("stale", encoding="utf-8")

    with regen_pipeline._object_lod_overlay_scope(overlay):
        assert not overlay.exists()
        overlay.write_text("current", encoding="utf-8")

    assert not overlay.exists()


def test_object_lod_overlay_scope_cleans_exception(tmp_path):
    overlay = tmp_path / ".modkit" / "object_lod_overlay.v1.json"
    overlay.parent.mkdir(parents=True)

    with pytest.raises(RuntimeError, match="native lodgen failed"):
        with regen_pipeline._object_lod_overlay_scope(overlay):
            overlay.write_text("current", encoding="utf-8")
            raise RuntimeError("native lodgen failed")

    assert not overlay.exists()


def test_object_lod_overlay_binding_uses_saved_plugin_fingerprint(tmp_path):
    plugin = tmp_path / "Output.esm"
    plugin.write_bytes(b"TES4 saved output")
    overlay = tmp_path / "object_lod_overlay.v1.json"
    overlay.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "plugin_name": plugin.name,
                "plugin_size": 0,
                "plugin_mtime_ns": 0,
                "entries": [],
            }
        ),
        encoding="utf-8",
    )

    regen_pipeline._bind_object_lod_overlay_to_plugin(overlay, plugin)

    document = json.loads(overlay.read_text(encoding="utf-8"))
    metadata = plugin.stat()
    assert document["plugin_size"] == metadata.st_size
    assert document["plugin_mtime_ns"] == metadata.st_mtime_ns
    assert not overlay.with_suffix(overlay.suffix + ".tmp").exists()


@pytest.mark.parametrize(
    ("result", "message"),
    [
        (
            SimpleNamespace(btr=0, bto=0, dds=0, lod_written=True, warnings=()),
            "generated no BTR files",
        ),
        (
            SimpleNamespace(btr=1, bto=0, dds=2, lod_written=False, warnings=()),
            "no .lod file was written",
        ),
    ],
)
def test_lod_generation_rejects_missing_required_outputs(
    monkeypatch, tmp_path, result, message
):
    monkeypatch.setattr(
        "creation_lib.lod.native_runtime.generate_lod",
        lambda *_args, **_kwargs: result,
    )

    with pytest.raises(RuntimeError, match=message):
        regen_pipeline._run_generate_lod(
            mod_root=tmp_path / "mod",
            worldspaces=["Mojave"],
            working_esm=tmp_path / "mod" / "FNV_FO3_Merged.esm",
            asset_dirs=[],
            settings={"global": {"generate_terrain": True, "write_lodsettings": True}},
            runner_log=lambda *_args: None,
        )
