from pathlib import Path
import struct
import zlib

import pytest

from bacup_lib import camp_build_ui as ui
from bacup_lib.legendary_perks_ui import imports, swf_tags


def movie(import_name: str | None = None) -> bytes:
    header = b"\x08\x00\x00\x1e\x01\x00"
    payload = b"" if import_name is None else import_name.encode() + b"\x00\x01\x00\x00\x00"
    tags = b"" if import_name is None else struct.pack("<H", 71 << 6 | len(payload)) + payload
    body = header + tags + b"\x40\x00\x00\x00"
    return b"FWS\x0a" + struct.pack("<I", len(body) + 8) + body


def test_missing_source_does_not_write(tmp_path):
    with pytest.raises(FileNotFoundError):
        ui.convert_camp_build_ui(tmp_path / "source", tmp_path / "out")
    assert not (tmp_path / "out").exists()


@pytest.mark.parametrize('compressed', [False, True])
def test_modify_header_repairs_only_the_expanded_frame_in_the_named_sprite(compressed):
    from bacup_lib.status_hud_source import sprite_tags, tag
    wrong = b'modifyModeTwoLined_Minimized\0'
    correct = b'modifyModeTwoLine_Expanded\0'
    symbols = struct.pack('<HH', 1, 9) + b'WorkshopNewLibrary_fla.LeftPanel_Title_Entry_40\0'
    timeline = tag(43, wrong) + tag(1, b'') + tag(0, b'')
    source = ui.rebuild(movie(), [(76, symbols),
        (39, struct.pack('<HH', 9, 1) + timeline),
        (39, struct.pack('<HH', 10, 1) + timeline), *swf_tags(movie())])
    if compressed:
        source = b'CWS' + source[3:8] + zlib.compress(source[8:])
    result = swf_tags(ui.repair_mode_header(source))
    sprites = {struct.unpack_from('<H', body)[0]: list(sprite_tags(body))
               for kind, body in result if kind == 39}
    assert sprites[9] == [(43, correct), (1, b''), (0, b'')]
    assert sprites[10] == [(43, wrong), (1, b''), (0, b'')]
    assert ui.repair_mode_header(ui.repair_mode_header(source)) == ui.repair_mode_header(source)


def test_import_closure_follows_cycles_without_duplicate_files(tmp_path):
    interface = tmp_path / "interface"
    interface.mkdir()
    (interface / ui.MENU).write_bytes(movie("WorkshopNewLibrary.swf"))
    (interface / "workshopnewlibrary.swf").write_bytes(movie(ui.MENU))
    closure = ui.source_closure(interface)
    assert set(closure) == {ui.MENU, "workshopnewlibrary.swf"}


@pytest.mark.parametrize("name", ["../other.swf", "C:/other.swf", "other\\file.swf", "https://other.swf", "other.txt"])
def test_import_closure_rejects_paths_outside_the_interface_folder(tmp_path, name):
    (tmp_path / ui.MENU).write_bytes(movie(name))
    with pytest.raises(ValueError, match="Unsupported"):
        ui.source_closure(tmp_path)


def test_missing_dependency_fails_before_output_is_created(tmp_path):
    source = tmp_path / "source"
    (source / "interface").mkdir(parents=True)
    (source / "interface" / ui.MENU).write_bytes(movie("missing.swf"))
    with pytest.raises(FileNotFoundError):
        ui.convert_camp_build_ui(source, tmp_path / "out")
    assert not (tmp_path / "out").exists()


@pytest.mark.parametrize("compressed", [False, True])
def test_imports_resolve_only_to_the_private_menu_directory(compressed):
    source = movie("WorkshopNewLibrary.swf")
    if compressed:
        source = b"CWS" + source[3:8] + zlib.compress(source[8:])
    converted = ui.private_imports(source)
    # FO4 GFx resolves an import against the importing movie's own folder, and one missing import fails the whole load.
    assert imports(converted) == ["workshopnewlibrary.swf"]
    assert ui.OUTPUT / ui.MENU != Path("Interface/Workshop.swf")


def test_translation_keys_include_bridge_strings_and_whole_field_text():
    from creation_lib.swf import native_runtime as native
    code = native.compile_as3_do_abc(['package { public class Menu {'
        'public function hint():String { return "$BUILD"; } } }'])
    # DefineEditText: id, empty bounds, HasText, HTML, empty variable name, initial text.
    field = struct.pack('<H', 3) + b'\x00\x80\x02\0' + b'<p align="left"><font size="27">$BLUEPRINT BUDGET</font></p>\0'
    data = ui.rebuild(movie(), [(82, code), (37, field), *swf_tags(movie())])
    assert {"$BUILD", "$BLUEPRINT BUDGET"} <= ui.translation_keys(data)


def test_font_alias_classes_keep_their_names_and_runtime_font_strings():
    from creation_lib.swf import native_runtime as native
    code = native.compile_as3_do_abc(['package { public class $MAIN_Font {} }',
        'package Shared.AS3 { public class BSButtonHint {'
        'public function GetExpectedFont():String { return "$MAIN_Font"; } } }'])
    data = ui.rebuild(movie(), [(82, code), *swf_tags(movie())])
    converted = ui.private_movie(data, ui.private_class_names(native.abc_class_names(data)))
    assert set(native.abc_class_names(converted)) == {"$MAIN_Font", "B21TFA_CampBuild.Shared.AS3.BSButtonHint"}
    body = native.abc_disassemble(converted, "B21TFA_CampBuild.Shared.AS3.BSButtonHint", "GetExpectedFont")[0]["code"]
    assert any('PushString "$MAIN_Font"' in line for line in body)


def test_workflow_routes_only_extracted_fo76_fo4_sources(monkeypatch, tmp_path):
    from types import SimpleNamespace
    from bacup_lib.workflows import unified
    calls = []
    monkeypatch.setattr(ui, "convert_camp_build_ui", lambda source, target: calls.append((source, target)) or {"files": [1, 2]})
    monkeypatch.setattr(unified, "_merge_packaged_tales_translations", lambda *args: None)
    ctx = SimpleNamespace(source_data_dir=tmp_path / "source", mod_path=tmp_path / "converted")
    request = SimpleNamespace(source_game="fo76", target_game="fo4")
    assert unified._convert_fo76_camp_build_ui(request, ctx) == 0
    interface = ctx.source_data_dir / "interface"
    interface.mkdir(parents=True)
    (interface / ui.MENU).touch()
    assert unified._convert_fo76_camp_build_ui(request, ctx) == 2
    assert calls == [(ctx.source_data_dir, ctx.mod_path / "data")]
    assert unified._convert_fo76_camp_build_ui(SimpleNamespace(source_game="skyrimse", target_game="fo4"), ctx) == 0
    assert len(calls) == 1


def test_dynamic_list_class_names_move_but_the_item_card_channel_does_not():
    from creation_lib.swf import native_runtime as native
    sources = [
        'package { public class ItemsListEntry {} }',
        'package { public class WorkshopItemCard {} }',
        'package { public class Menu {'
        'public function entry():String { return "ItemsListEntry"; }'
        'public function channel():String { return "WorkshopItemCard"; }'
        'public function card():WorkshopItemCard { return null; } } }',
    ]
    data = movie()
    data = ui.rebuild(data, [(82, native.compile_as3_do_abc(sources)), *swf_tags(data)])
    data = native.rename_as3_classes(data, 'B21')
    data = ui.private_names(data, {'ItemsListEntry': 'B21_ItemsListEntry', 'WorkshopItemCard': 'B21_WorkshopItemCard'})
    entry = native.abc_disassemble(data, 'B21_Menu', 'entry')[0]['code']
    channel = native.abc_disassemble(data, 'B21_Menu', 'channel')[0]['code']
    assert any('PushString "B21_ItemsListEntry"' in instruction for instruction in entry)
    assert any('PushString "WorkshopItemCard"' in instruction for instruction in channel)
    card = next(trait for trait in native.abc_class_outline(data, 'B21_Menu')['instance_traits'] if trait['name'] == 'card')
    assert card['signature']['returns'] == 'B21_WorkshopItemCard'


def test_card_resource_icon_lookups_name_the_renamed_icon_classes():
    from creation_lib.swf import native_runtime as native
    code = native.compile_as3_do_abc(['package { public class WS_icon_Food {} }',
        'package { public class Entry { public function icon(id:String):String { return "WS_icon_" + id; } } }'])
    data = ui.rebuild(movie(), [(82, code), *swf_tags(movie())])
    converted = ui.private_movie(data, ui.private_class_names(native.abc_class_names(data)))
    assert "B21TFA_CampBuild_WS_icon_Food" in native.abc_class_names(converted)
    body = native.abc_disassemble(converted, "B21TFA_CampBuild_Entry", "icon")[0]["code"]
    assert any('PushString "B21TFA_CampBuild_WS_icon_"' in line for line in body)


@pytest.mark.parametrize('compressed', [False, True])
def test_imported_bindings_and_nested_class_placements_move_together(compressed):
    from bacup_lib.quest_area_ui import tag
    binding = b'ItemsListEntry\0'
    imported = b'WorkshopNewLibrary.swf\0\1\0' + struct.pack('<HH', 1, 8) + binding
    placed = b'\0\x08' + struct.pack('<H', 1) + binding
    sprite = struct.pack('<HH', 9, 1) + tag(70, placed) + tag(0, b'')
    data = movie()
    data = ui.rebuild(data, [(71, imported), (70, placed), (39, sprite), *swf_tags(data)])
    if compressed:
        data = b'CWS' + data[3:8] + zlib.compress(data[8:])
    result = swf_tags(ui.private_names(data, {'ItemsListEntry': 'B21_ItemsListEntry'}))
    for code, payload in result:
        if code in (71, 70, 39):
            assert b'B21_ItemsListEntry\0' in payload
            assert b'\0ItemsListEntry\0' not in payload
