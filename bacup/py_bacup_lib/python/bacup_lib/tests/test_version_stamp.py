from bacup_lib.run import ConversionRun
from bacup_lib.version_stamp import read_plugin_snam, stamp_plugin_version


def test_read_snam_missing_returns_none(tmp_path):
    assert read_plugin_snam(tmp_path / "nope.esm") is None


def test_read_snam_from_stamped_esm(tmp_path):
    stamped_esm = tmp_path / "stamped.esm"
    with ConversionRun.create_new(
        "fo4",
        "fo4",
        None,
        stamped_esm.name,
        config={"mod_path": str(tmp_path)},
    ) as run:
        stamp_plugin_version(run, "alpha2")
        run.save_target(run_nvnm_validator=False)
    assert read_plugin_snam(stamped_esm) == "alpha2"
