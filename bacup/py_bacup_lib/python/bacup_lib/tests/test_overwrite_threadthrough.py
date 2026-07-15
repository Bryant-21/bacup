import dataclasses

from bacup_lib.models import ConversionContext, ConversionSummary, PluginPortOptions
from bacup_lib.pipeline._shim import build_orchestrator_shim


def _ctx(overwrite, *, pbr_carry=False, texture_landscape_mip_flooding=False):
    return ConversionContext(
        source_game="fo76",
        target_game="fo4",
        mod_path="x",
        output_plugin_name="X.esp",
        target_extracted_dir=None,
        target_data_dir=None,
        formkey_mapper=None,
        fixups=None,
        summary=ConversionSummary(mod_path="x"),
        overwrite_existing=overwrite,
        pbr_carry=pbr_carry,
        texture_landscape_mip_flooding=texture_landscape_mip_flooding,
    )


def test_shim_reads_overwrite_from_context():
    assert build_orchestrator_shim([], _ctx(True)).overwrite_existing is True
    assert build_orchestrator_shim([], _ctx(False)).overwrite_existing is False


def test_shim_reads_pbr_carry_from_context_and_defaults_off():
    assert build_orchestrator_shim([], _ctx(False, pbr_carry=True)).pbr_carry is True
    assert build_orchestrator_shim([], _ctx(False)).pbr_carry is False


def test_shim_reads_landscape_mip_flooding_from_context():
    enabled = _ctx(False, texture_landscape_mip_flooding=True)
    assert build_orchestrator_shim([], enabled).texture_landscape_mip_flooding is True
    assert build_orchestrator_shim([], _ctx(False)).texture_landscape_mip_flooding is False


def test_plugin_port_options_accepts_overwrite_true():
    options = PluginPortOptions(
        overwrite_existing=True,
    )
    assert options.overwrite_existing is True
    assert dataclasses.asdict(options)["overwrite_existing"] is True


def test_plugin_port_options_has_overwrite_default_false():
    assert PluginPortOptions().overwrite_existing is False
