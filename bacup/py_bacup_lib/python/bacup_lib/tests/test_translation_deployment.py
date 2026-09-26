import pytest

from bacup_lib import regen_pipeline, translations


@pytest.mark.parametrize("mode", ["packed", "loose", "selective"])
def test_bacup_deploys_updated_runtime_translations_to_mo2(tmp_path, mode):
    mod = tmp_path / "mods/SeventySix"
    mod.mkdir(parents=True)
    (mod / "SeventySix.esm").write_bytes(b"plugin")
    translations.merge_ui_translations(mod / "data", ["$ITEM STATS\tITEM STATS"])
    german = translations.translation_path(mod / "data", "de")
    translations.write_table(german, ["$ITEM STATS\tGEGENSTANDSWERTE"])
    target = tmp_path / "MO2/SeventySix"
    old = target / translations.TRANSLATIONS
    translations.write_table(old, ["$Old\tStale"])

    regen_pipeline._deploy_output_mods(
        "SeventySix", plugin_names=["SeventySix.esm"], project_root=tmp_path,
        game_data_dir=target, resource_dir=tmp_path / "resource",
        plugin_only=mode == "selective", deploy_loose=mode == "loose",
    )

    assert old.read_bytes() == (mod / translations.TRANSLATIONS).read_bytes()
    assert (target / german.relative_to(mod)).read_bytes() == german.read_bytes()
    assert not (target / translations.LEGACY_TRANSLATIONS).exists()
