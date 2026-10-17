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
        "$MainMenuPhotoGallery\tPHOTO GALLERY\n$Loading\tLoading\n$LOADING\tLOADING\n$Unrelated\tNo\n")
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
    # loadingmenu.swf authors its LoadingArea_tf as the upper-case key, and FO4's lookup is case-sensitive.
    assert "$LOADING\tLOADING" in table
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
    from bacup_lib.menu_presentation_art import LIST_ENTRIES, OPTION_ENTRIES, _frame_children, entry_art, source_layout
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
    reference = source_layout(movie, 'MainMenu')['ColorReference_mc'].character_id
    children = {item.name: item.character_id for item in _frame_children(tags, reference)}
    assert children['ColorSwatch_mc'] == ids['B21_ColorFace']
    assert 'ColorText_tf' in children and ids['B21_ColorBG'] in children.values()
    assert order.index(reference) > order.index(ids['B21_ColorFace'])
    slider = {item.name: item.character_id for item in _frame_children(tags, ids['Option_Scrollbar'])}
    assert slider['Track_mc'] == ids['B21_SliderTrack'] and slider['Thumb_mc'] == ids['B21_SliderThumb']
    assert {'LeftCatcher_mc', 'RightCatcher_mc', 'BarCatcher_mc', 'LeftArrow_mc', 'RightArrow_mc'} <= slider.keys()
    assert all(tag in tags for tag in swf_tags(target.read_bytes()) if tag[0] == 82)
    assert not ui.native_runtime.unbacked_symbol_classes(movie)


def test_loading_movie_preserves_the_full_fo76_timeline_and_accepts_fo4_callbacks():
    from pathlib import Path
    import struct
    import pytest
    from bacup_lib.legendary_perks_ui import remap_menu_fonts, swf_tags

    path = Path('extracted/fo76/interface/loadingmenu.swf')
    if not path.is_file():
        pytest.skip('needs extracted FO76 loading movie')
    source = path.read_bytes()
    movie, layout = ui.build_loading_host(path.parent)
    artwork = lambda data: [entry for entry in swf_tags(data) if entry[0] not in (72, 76, 82)]
    changed = [(a, b) for a, b in zip(artwork(movie), artwork(remap_menu_fonts(source))) if a != b]
    assert len(artwork(movie)) == len(artwork(source))
    assert len(changed) == 1
    assert changed[0][0][0] == changed[0][1][0]
    assert changed[0][0][0] in (2, 22, 32, 83)
    assert struct.unpack_from('<H', changed[0][0][1])[0] == layout['vault_number_shape']
    assert layout['source'] == 'loadingmenu.swf'
    outline = ui.native_runtime.abc_class_outline(movie, 'B21_Loading_LoadingMenu')
    traits = {entry['name']: entry for entry in outline['instance_traits']}
    assert {'Box_mc', 'Info_mc', 'Vault51Logo_mc', 'BGSCodeObj', 'B21LoadingReport'} <= traits.keys()
    assert traits['SetLoadingText']['signature']['params'] == ['uint', 'String', 'String']
    assert traits['SetMinimalMode']['signature']['params'] == []
    assert traits['SetLevel']['signature']['params'] == ['uint', 'Number']
    assert traits['SetPlatform']['signature']['params'] == ['uint', 'Boolean']
    assert not ui.native_runtime.unbacked_symbol_classes(movie)


def test_main_art_carries_the_bitmaps_its_shapes_fill_with():
    from pathlib import Path
    import pytest
    from bacup_lib.menu_presentation_art import main_art

    source, target = Path('extracted/fo76/interface'), Path('extracted/fo4/interface/mainmenu.swf')
    if not (source / 'seventysixmenu.swf').is_file() or not target.is_file():
        pytest.skip('needs extracted FO76 and FO4 interface files')
    movie, _ = main_art((source / 'seventysixmenu.swf').read_bytes(), (source / 'menulistcomponent.swf').read_bytes(),
                        target.read_bytes())
    # Scaleform dereferences a fill's missing image when the shape first draws.
    assert ui.native_runtime.dangling_bitmap_fills(movie) == []


def test_extra_main_menu_hosts_build_their_own_runtime_family():
    import os
    from pathlib import Path
    import pytest

    source, target = Path('extracted/fo76/interface'), Path('extracted/fo4/interface')
    og = Path(os.environ.get('FO4_OG_DIR', '').strip('"')) / 'Data/Interface/MainMenu.swf'
    if not (source / 'seventysixmenu.swf').is_file() or not (target / 'mainmenu.swf').is_file() or not og.is_file():
        pytest.skip('needs extracted FO76/FO4 interface files and FO4_OG_DIR with a loose OG MainMenu.swf')
    outputs, layout = ui.build_hosts(source, target, extra_hosts=[og])
    names = {path.name for path in outputs}
    # One shared converted mod serves both installs; each runtime loads only its own family.
    assert {'B21_TFAMainMenu_AE.swf', 'B21_TFAMainMenu_OG.swf'} <= names
    assert set(layout['target_hosts']) == {'MainMenu_AE', 'MainMenu_OG'}
    for path, movie in outputs.items():
        assert ui.native_runtime.dangling_bitmap_fills(movie) == [], path
