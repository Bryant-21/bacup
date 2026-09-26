from types import SimpleNamespace

from bacup_lib import (
    casino_ui,
    combat_perk_catalog,
    combat_perk_ui,
    expedition_results_ui,
    photo_gallery_ui,
    reputation_catalog,
    reputation_ui,
    xp_ui,
)
from bacup_lib.workflows import unified


def context(tmp_path):
    source = tmp_path / "source"
    (source / "interface").mkdir(parents=True)
    mod = tmp_path / "mod"
    mod.mkdir()
    source_plugin = tmp_path / "SeventySix.esm"
    source_plugin.write_bytes(b"source")
    converted_plugin = mod / "SeventySix.esm"
    converted_plugin.write_bytes(b"converted")
    return SimpleNamespace(
        source_data_dir=source,
        source_plugin_path=source_plugin,
        target_data_dir=tmp_path / "target-data",
        mod_path=mod,
        output_plugin_name=converted_plugin.name,
    )


def request(source="fo76", target="fo4"):
    return SimpleNamespace(source_game=source, target_game=target, target_data_dir=None)


def test_expedition_and_casino_wrappers_are_guarded_and_merge_packaged_translations(tmp_path, monkeypatch):
    ctx = context(tmp_path)
    merged = []
    monkeypatch.setattr(unified, "_merge_packaged_tales_translations", lambda value, runner=None: merged.append(value))
    monkeypatch.setattr(expedition_results_ui, "convert_expedition_results_ui", lambda *args: {"files": [1, 2]})
    monkeypatch.setattr(casino_ui, "convert_casino_ui", lambda *args: {"files": [1, 2, 3, 4]})
    xp_calls = []
    monkeypatch.setattr(
        xp_ui,
        "convert_xp_ui",
        lambda *args: xp_calls.append(args) or {"files": [1, 2], "audio": [1, 2, 3, 4, 5]},
    )

    assert unified._convert_fo76_expedition_results(request(), ctx) == 0
    assert unified._convert_fo76_casino_ui(request(), ctx) == 0
    assert unified._convert_fo76_xp_ui(request(), ctx) == 0
    assert merged == []
    assert xp_calls == []

    (ctx.source_data_dir / "interface" / expedition_results_ui.MENU).write_bytes(b"menu")
    (ctx.source_data_dir / "interface" / casino_ui.MENU).write_bytes(b"menu")
    (ctx.source_data_dir / "interface" / xp_ui.SOURCE).write_bytes(b"hud")
    assert unified._convert_fo76_expedition_results(request(), ctx) == 2
    assert unified._convert_fo76_casino_ui(request(), ctx) == 4
    assert unified._convert_fo76_xp_ui(request(), ctx) == 7
    assert merged == [ctx, ctx]
    assert xp_calls == [(ctx.source_data_dir, ctx.mod_path / "data")]


def test_reputation_wrapper_emits_catalog_before_ui_and_merges_translations(tmp_path, monkeypatch):
    ctx = context(tmp_path)
    calls = []

    def emit(source, converted, output):
        calls.append(("catalog", source, converted, output))
        return {"factions": [1, 2], "missing_converted": ["SNDR:UIReputationIncrease"]}

    monkeypatch.setattr(reputation_catalog, "emit_reputation_catalog", emit)
    monkeypatch.setattr(reputation_ui, "convert_reputation_ui", lambda *args: calls.append(("ui", *args)) or {"files": [1, 2]})
    monkeypatch.setattr(unified, "_merge_packaged_tales_translations", lambda value, runner=None: calls.append(("translations", value)))

    assert unified._convert_fo76_reputation_presentation(request(), ctx) == 2
    assert calls == [
        ("catalog", ctx.source_plugin_path, ctx.mod_path / ctx.output_plugin_name, ctx.mod_path),
        ("ui", ctx.source_data_dir, ctx.mod_path / "data"),
        ("translations", ctx),
    ]


def test_photo_gallery_wrapper_is_guarded_and_targets_the_mod_root(tmp_path, monkeypatch):
    ctx = context(tmp_path)
    calls = []

    def convert(source_root, output_root):
        calls.append(("gallery", source_root, output_root))
        scaleform = output_root / photo_gallery_ui.SCALEFORM_OUTPUT / photo_gallery_ui.MENU
        map_presentation = output_root / photo_gallery_ui.MAP_OUTPUT / "presentation.json"
        scaleform.parent.mkdir(parents=True)
        map_presentation.parent.mkdir(parents=True)
        scaleform.write_bytes(b"movie")
        map_presentation.write_text("{}", encoding="utf-8")
        return {"menu": str(scaleform)}

    monkeypatch.setattr(
        photo_gallery_ui,
        "convert_photo_gallery_ui",
        convert,
    )
    monkeypatch.setattr(
        unified,
        "_merge_packaged_tales_translations",
        lambda value, runner=None: calls.append(("translations", value)),
    )

    assert unified._convert_fo76_photo_gallery_ui(request(), ctx) == 0
    assert calls == []

    (ctx.source_data_dir / "interface" / photo_gallery_ui.MENU).write_bytes(b"menu")
    assert unified._convert_fo76_photo_gallery_ui(request(), ctx) == 2
    assert calls == [
        ("gallery", ctx.source_data_dir, ctx.mod_path),
        ("translations", ctx),
    ]
    assert (ctx.mod_path / photo_gallery_ui.SCALEFORM_OUTPUT / photo_gallery_ui.MENU).is_file()
    assert (ctx.mod_path / photo_gallery_ui.MAP_OUTPUT / "presentation.json").is_file()
    assert not (ctx.mod_path / "data" / photo_gallery_ui.MAP_OUTPUT).exists()


def test_combat_perk_wrapper_emits_catalog_before_hud(tmp_path, monkeypatch):
    ctx = context(tmp_path)
    calls = []
    logs = []
    movie = "Interface/B21/TalesFromAppalachia/StatusHUD/statushud.swf"
    monkeypatch.setattr(unified, "_safe_emit_log", lambda runner, level, message: logs.append((level, message)))
    monkeypatch.setattr(
        combat_perk_catalog,
        "emit_combat_perk_catalog",
        lambda *args: calls.append(("catalog", *args)) or {
            "missing_converted": [],
            "missing_source": ["PERK:HeavyGunner01"],
        },
    )
    monkeypatch.setattr(
        combat_perk_ui,
        "convert_combat_perk_ui",
        lambda *args: calls.append(("ui", *args)) or {"movie": movie},
    )

    assert unified._convert_fo76_combat_perks(request(), ctx) == 2
    assert calls == [
        ("catalog", ctx.source_plugin_path, ctx.mod_path / ctx.output_plugin_name, ctx.mod_path),
        ("ui", ctx.source_data_dir, ctx.mod_path / "data"),
    ]
    assert logs == [
        (
            "WARN",
            "Combat perks are missing 1 source form(s): PERK:HeavyGunner01",
        ),
        ("INFO", f"Generated the combat perk catalog and HUD asset: {movie}"),
    ]


def test_new_ui_wrappers_ignore_other_conversion_pairs(tmp_path):
    ctx = context(tmp_path)
    other = request("skyrimse", "fo4")
    assert unified._convert_fo76_expedition_results(other, ctx) == 0
    assert unified._convert_fo76_reputation_presentation(other, ctx) == 0
    assert unified._convert_fo76_casino_ui(other, ctx) == 0
    assert unified._convert_fo76_photo_gallery_ui(other, ctx) == 0
    assert unified._convert_fo76_xp_ui(other, ctx) == 0
    assert unified._convert_fo76_combat_perks(other, ctx) == 0
