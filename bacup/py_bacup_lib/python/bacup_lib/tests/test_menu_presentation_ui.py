from bacup_lib import menu_presentation_ui as ui


def source(tmp_path, music="Data/Music/Special/MUS_Special_MainMenu_Takeovers.xwm"):
    extracted = tmp_path / "extracted"
    installed = tmp_path / "installed"
    (extracted / "interface").mkdir(parents=True)
    (extracted / "music/special").mkdir(parents=True)
    (installed / "Data/Video").mkdir(parents=True)
    (installed / "Fallout76.ini").write_text("[General]\nsMainMenuMusic = " + music + "\n")
    for name in ui.MENUS:
        (extracted / 'interface' / name).write_bytes(name.encode())
    (extracted / "interface/translate_en.txt").write_text(
        "$MainMenuPhotoGallery\tPHOTO GALLERY\n$Loading\tLoading\n$Unrelated\tNo\n")
    (extracted / "music/special/mus_special_mainmenu_takeovers.xwm").write_bytes(
        b"RIFF" + bytes(4) + b"XWMA" + bytes(24))
    for video in ui.VIDEOS:
        (installed / "Data/Video" / video).write_bytes(b"KB2j" + bytes(32))
    (installed / "bink2w64.dll").write_bytes(b"MZ" + bytes(62))
    backgrounds = extracted / "textures/interface/loadingmenubackgrounds"
    backgrounds.mkdir(parents=True)
    for name in ("LS_Field.dds", "nw_morgantown.dds"):
        (backgrounds / name).write_bytes(b"DDS ")
    return extracted, installed


def test_converts_only_source_owned_menu_and_media_closure(tmp_path, monkeypatch):
    extracted, installed = source(tmp_path)
    monkeypatch.setattr(ui.native_runtime, "abc_class_names",
                        lambda data: ['SeventySixMenu', 'MainMenuEntry', 'LoadingMenu'])
    output = tmp_path / "output/data"
    result = ui.convert_menu_presentation(extracted, installed, output)
    assert len(result["media_pairs"]) == 1
    assert result["media_pairs"][0]["music"].lower().endswith("mus_special_mainmenu_takeovers.xwm")
    assert len(result["files"]) == 4
    assert result["loading_backgrounds"] == ["Textures/interface/loadingmenubackgrounds/ls_field.dds"]
    assert (output.parent / ui.LOOSE_MEDIA / "Video/Intro.bk2").read_bytes().startswith(b"KB2")
    assert not any(output.rglob('*.swf'))
    assert set(result['sources']) == set(ui.MENUS)
    assert (output.parent / result["media_pairs"][0]["loop"]).read_bytes().startswith(b"KB2")
    assert (output.parent / ui.LOOSE_MEDIA / "Video/bink2w64_fo76.dll").read_bytes().startswith(b"MZ")
    assert (output.parent / ui.LOOSE_MEDIA.parent / "menu-presentation.json").is_file()
    table = (output.parent / "F4SE/Plugins/B21_TalesFromAppalachia_en.txt").read_text(encoding="utf-16")
    assert "$MainMenuPhotoGallery\tPHOTO GALLERY" in table
    assert "$Unrelated" not in table


def test_deleted_intro_video_skips_only_that_file(tmp_path, monkeypatch):
    extracted, installed = source(tmp_path)
    (installed / "Data/Video/Intro.bk2").unlink()
    monkeypatch.setattr(ui.native_runtime, "abc_class_names",
                        lambda data: ['SeventySixMenu', 'MainMenuEntry', 'LoadingMenu'])
    output = tmp_path / "output/data"

    result = ui.convert_menu_presentation(extracted, installed, output)

    assert result["missing_media"] == [(installed / "Data/Video/Intro.bk2").as_posix()]
    assert not (output.parent / ui.LOOSE_MEDIA / "Video/Intro.bk2").exists()
    assert (output.parent / ui.LOOSE_MEDIA / "Video/MainMenuLoop.bk2").is_file()
    assert len(result["media_pairs"]) == 1
    assert len(result["files"]) == 3


def test_stock_list_rows_take_fo76_highlight_art():
    import struct
    from pathlib import Path
    import pytest
    from bacup_lib.legendary_perks_ui import swf_tags
    from bacup_lib.menu_presentation_art import LIST_ENTRIES, OPTION_ENTRIES, _frame_children, entry_art
    from bacup_lib.quest_area_ui import symbol_ids
    source, target = Path('extracted/fo76/interface'), Path('extracted/fo4/interface/mainmenu.swf')
    if not (source / 'seventysixmenu.swf').is_file() or not target.is_file():
        pytest.skip('needs extracted FO76 and FO4 interface files')
    movie, _ = entry_art((source / "seventysixmenu.swf").read_bytes(), (source / "menulistcomponent.swf").read_bytes(),
                      target.read_bytes())
    tags = swf_tags(movie)
    ids = symbol_ids(tags)
    order = [struct.unpack_from('<H', p)[0] for c, p in tags if c == 39]
    for name in LIST_ENTRIES + OPTION_ENTRIES:
        children = {item.name: item.character_id for item in _frame_children(tags, ids[name])}
        banner = 'B21_OptionBanner' if name in OPTION_ENTRIES else 'B21_ListBanner'
        assert children['border'] == ids[banner] and order.index(ids[name]) > order.index(ids[banner])
        assert (ids['B21_OptionRowBG'] in children.values()) == (name in OPTION_ENTRIES)
    assert not ui.native_runtime.unbacked_symbol_classes(movie)
