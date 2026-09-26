from bacup_lib.holotape_programs import copy_fo76_holotape_programs


def test_copies_only_fo76_only_games_with_their_assets_and_config(tmp_path):
    programs = tmp_path / "source" / "programs"
    (programs / "xml").mkdir(parents=True)
    for name in (
        "nukatapper.swf",
        "NukaTapperAssets.swf",
        "wasteladmapassets.swf",
        "xml/nukatapperconfig.xml",
        "atomiccommand.swf",
        "fonts_programs.swf",
        "xml/atomiccommandconfig.xml",
    ):
        (programs / name).write_bytes(name.encode())

    output = tmp_path / "data"
    copied = copy_fo76_holotape_programs(tmp_path / "source", output)

    assert set(copied) == {
        "Programs/NukaTapperAssets.swf",
        "Programs/nukatapper.swf",
        "Programs/wasteladmapassets.swf",
        "Programs/xml/nukatapperconfig.xml",
    }
    assert (output / "Programs/xml/nukatapperconfig.xml").read_bytes() == b"xml/nukatapperconfig.xml"
    assert not (output / "Programs/atomiccommand.swf").exists()


def test_missing_source_programs_copy_nothing(tmp_path):
    assert copy_fo76_holotape_programs(tmp_path, tmp_path / "data") == []
