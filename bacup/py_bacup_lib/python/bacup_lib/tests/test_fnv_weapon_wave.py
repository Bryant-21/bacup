from types import SimpleNamespace

from bacup_lib.models import AssetRef
from bacup_lib.workflows.unified import AssetWaveBuilder, _wave_plan_for


def test_full_fnv_wave_keeps_gun_role_for_world_and_first_person_models(tmp_path):
    paths = [r"Weapons\1HandPistol\9mm.nif", r"Weapons\1HandPistol\1stPerson9mm.nif"]
    assets = []
    for index, path in enumerate(paths):
        resolved = tmp_path / f"weapon{index}.nif"
        resolved.write_bytes(b"asset")
        assets.append(AssetRef("nif", path, str(resolved)))
    shim = SimpleNamespace(
        source_game="fnv",
        target_game="fo4",
        mod_path=tmp_path / "FNV_FO3",
        target_extracted_dir="",
        _source_profile=SimpleNamespace(id="fnv", asset_prefix="fnv"),
        _addon_index_map={},
        graph=SimpleNamespace(all_assets=assets, all_records=[]),
        _summary=SimpleNamespace(nifs_total=0),
        _target_has_asset=lambda _asset: False,
        _weapon_metadata_index={
            "0E3778@FalloutNV.esm": {
                "base_model": paths[0],
                "first_person_model": paths[1],
                "weapon_role": "gun",
                "anim_type": "3",
            }
        },
    )
    builder = SimpleNamespace(
        driver=SimpleNamespace(
            ctx=SimpleNamespace(source_game="fnv", target_game="fo4"),
            _req=SimpleNamespace(options=SimpleNamespace(exclude_signatures=())),
        ),
        _wave_plan=_wave_plan_for("fnv", "fo4"),
        runner=SimpleNamespace(emit_log=lambda *_args: None),
        runs=SimpleNamespace(nifs=SimpleNamespace(id="test-weapons")),
    )

    stage = AssetWaveBuilder._build_nif_stage(builder, shim, ())

    assert stage.phase == "convert_gamebryo_nifs"
    assert [entry["weapon_role"] for entry in stage.params["nif_paths"]] == ["gun", "gun"]
    assert [entry["source_path"] for entry in stage.params["nif_paths"]] == paths
