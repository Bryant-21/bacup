from bacup_lib.models import ConversionContext, ConversionSummary
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


def test_shim_reads_options_from_context():
    assert build_orchestrator_shim([], _ctx(True)).overwrite_existing is True
    assert build_orchestrator_shim([], _ctx(False, pbr_carry=True)).pbr_carry is True
    enabled = _ctx(False, texture_landscape_mip_flooding=True)
    assert build_orchestrator_shim([], enabled).texture_landscape_mip_flooding is True

    disabled = build_orchestrator_shim([], _ctx(False))
    assert disabled.overwrite_existing is False
    assert disabled.pbr_carry is False
    assert disabled.texture_landscape_mip_flooding is False
