from __future__ import annotations

import json
import struct

from bacup_lib.legendary_perks_ui import remap_menu_fonts, swf_tags
from bacup_lib.quest_area_ui import closure, symbol_ids, tag
from bacup_lib.status_hud_source import (inject_art, placement, replace_tags, select_children, sprite_tags, states,
                                         transform)
from bacup_lib.ui_contract import resolve_numbered_name
from creation_lib.swf import native_runtime
from creation_lib.swf.tags import PlaceObject2Tag
from creation_lib.swf.types import MATRIX


def source_layout(movie: bytes, root: str) -> dict:
    tags = swf_tags(movie)
    cid = symbol_ids(tags)[root]
    entries = next(sprite_tags(p) for c, p in tags if c == 39 and struct.unpack_from('<H', p)[0] == cid) if cid else tags
    return states(entries)[0]


def import_art(source: bytes, host: bytes, wanted: dict[int, str], prefix: str,
               children: dict[int, set[str]] | None = None) -> bytes:
    tags = swf_tags(source)
    children = children or {}
    tags = [(c, select_children(p, children[struct.unpack_from('<H', p)[0]]))
            if c == 39 and struct.unpack_from('<H', p)[0] in children else (c, p) for c, p in tags]
    selected = set()
    for cid, name in wanted.items():
        selected.update(closure(tags, cid, name))
    art = [entry for entry in tags if entry in selected]
    classes = dict(wanted)
    for c, p in art:
        if c == 39:
            cid = struct.unpack_from('<H', p)[0]
            classes.setdefault(cid, prefix + str(cid))
    exports = struct.pack('<H', len(classes)) + b''.join(
        struct.pack('<H', cid) + name.encode() + b'\0' for cid, name in classes.items())
    library = remap_menu_fonts(replace_tags(source, [(69, struct.pack('<I', 8)), *art, (76, exports), (1, b''), (0, b'')]))
    result = inject_art(library, host, [(name, name) for name in classes.values()])
    scripts = []
    for cid, name in classes.items():
        # Keep source selection tweens and their terminal stops; other art stays on its chosen frame.
        stops = [0, 8, 9, 17] if name == 'B21_MainLarge' else [0, 8] if name == 'B21_MainImage' else [0]
        frame_scripts = ','.join(f'{frame},halt' for frame in stops)
        scripts.append(f'package {{ import flash.display.MovieClip; public dynamic class {name} extends MovieClip {{'
                       f'public function {name}() {{ addFrameScript({frame_scripts}); }} '
                       'private function halt():void { stop(); } } }')
    entries = swf_tags(result)
    at = next(i for i, (c, _) in enumerate(entries) if c == 76)
    entries.insert(at, (82, native_runtime.compile_as3_do_abc(scripts)))
    return replace_tags(result, entries)


def main_art(source_menu: bytes, source_list: bytes, host: bytes) -> tuple[bytes, dict]:
    symbols = symbol_ids(swf_tags(source_list))
    large = resolve_numbered_name(symbols, 'MenuListComponent_fla.MainMenuEntryLarge_', 'main menu large row')
    small = resolve_numbered_name(symbols, 'MenuListComponent_fla.MainMenuEntrySmall_', 'main menu small row')
    image = resolve_numbered_name(symbols, 'MenuListComponent_fla.MainMenuImageBody_', 'main menu Vault Boy tween')
    wanted = {symbols[large]: 'B21_MainLarge', symbols[small]: 'B21_MainSmall', symbols[image]: 'B21_MainImage'}
    host = import_art(source_list, host, wanted, 'B21_MainArt', {
        symbols[large]: {'Sizer_mc', 'HitArea_mc', 'Spinner_mc', 'Text_mc', 'Image_mc', 'Backer_mc'},
    })
    main = source_layout(source_menu, 'SeventySixMenu')['MainList']
    component = source_layout(source_menu, 'SeventySixMenuMain')['MenuList_mc']
    entry = source_layout(source_list, 'MainMenuEntry')['EntryInternal_mc']
    tags = swf_tags(source_list)
    sprite = next(p for c, p in tags if c == 39 and struct.unpack_from('<H', p)[0] == symbols['MainMenuEntry'])
    small_entry = states(sprite_tags(sprite))[1]['EntryInternal_mc']
    root = source_layout(source_menu, 'SeventySixMenu')
    sub_list = source_layout(source_menu, resolve_numbered_name(
        symbol_ids(swf_tags(source_menu)), 'SeventySixMenu_fla.SubListAnimation_', 'settings category list'))['SubList']
    secondary = source_layout(source_menu, resolve_numbered_name(
        symbol_ids(swf_tags(source_menu)), 'SeventySixMenu_fla.SecondarySubListAnimation_',
        'secondary list'))['SecondarySubList']
    panels = {'categories': [transform(root['SubListAnimation'])[i] + transform(sub_list)[i] for i in (4, 5)],
              'options': transform(root['SettingsList_mc'])[4:], 'controls': transform(root['InputMappingList_mc'])[4:],
              'hints': transform(root['ButtonHintBar_mc'])[4:],
              'saves': [transform(root['SecondarySubListAnimation'])[i] + transform(secondary)[i] for i in (4, 5)]}
    layout = {'x': transform(main)[4] + transform(component)[4],
              'y': transform(main)[5] + transform(component)[5],
              'large': transform(entry), 'small': transform(small_entry),
              'panels': panels,
              'source': {'menu': 'seventysixmenu.swf', 'rows': 'menulistcomponent.swf',
                         'large': large, 'small': small, 'image': image}}
    host, panels['rowRatio'] = entry_art(source_menu, source_list, host)
    return host, layout


LIST_ENTRIES = ('MainMenuListEntry', 'SaveLoadListEntry', 'SettingsCategoryListEntry', 'MainMenuHelpListEntry',
                'DLCListEntry', 'MainMenuInstalledContentListEntry')
OPTION_ENTRIES = ('SettingsOptionItem', 'InputMappingListEntry')
# Share of MenuListEntryHover's width taken by its slanted end (measured in game: 95 of 641 px).
BANNER_TAPER = 0.15
DEFINITIONS = {2, 22, 32, 83, 39, 37, 46, 84, 10, 48, 75, 91, 11, 33}


def _definition(tags, cid: int) -> tuple[int, bytes]:
    return next((c, p) for c, p in tags if c in DEFINITIONS and struct.unpack_from('<H', p)[0] == cid)


def _bounds(tags, cid: int) -> tuple[float, float, float, float]:
    code, payload = _definition(tags, cid)
    if code != 39:
        bits = payload[2] >> 3
        value = int.from_bytes(payload[2:2 + (5 + 4 * bits + 7) // 8], 'big')
        shift = (5 + 4 * bits + 7) // 8 * 8 - 5
        box = []
        for _ in range(4):
            shift -= bits
            item = value >> shift & (1 << bits) - 1
            box.append((item - (1 << bits) if item >> bits - 1 else item) / 20)
        return box[0], box[1], box[2], box[3]
    boxes = []
    for item in _frame_children(tags, cid):
        x0, x1, y0, y1 = _bounds(tags, item.character_id)
        m = item.matrix
        sx, sy, tx, ty = (m.scale_x, m.scale_y, m.translate_x / 20, m.translate_y / 20) if m else (1, 1, 0, 0)
        xs, ys = (x0 * sx + tx, x1 * sx + tx), (y0 * sy + ty, y1 * sy + ty)
        boxes.append((min(xs), max(xs), min(ys), max(ys)))
    return (min(b[0] for b in boxes), max(b[1] for b in boxes),
            min(b[2] for b in boxes), max(b[3] for b in boxes))


def _frame_children(tags, cid: int) -> list:
    code, payload = _definition(tags, cid)
    items = []
    if code != 39:
        return items
    for c, body in sprite_tags(payload):
        if c == 1:
            break
        item = placement(c, body)
        if item is not None and item.character_id is not None:
            items.append(item)
    return items


def _placed_box(tags, item) -> tuple[float, float, float, float]:
    x0, x1, y0, y1 = _bounds(tags, item.character_id)
    m = item.matrix
    xs = (x0 * m.scale_x + m.translate_x / 20, x1 * m.scale_x + m.translate_x / 20)
    ys = (y0 * m.scale_y + m.translate_y / 20, y1 * m.scale_y + m.translate_y / 20)
    return min(xs), max(xs), min(ys), max(ys)


def _fit(art: tuple[float, float, float, float], box: tuple[float, float, float, float]) -> MATRIX:
    sx = (box[1] - box[0]) / (art[1] - art[0])
    sy = (box[3] - box[2]) / (art[3] - art[2])
    return MATRIX(scale_x=sx, scale_y=sy, translate_x=round((box[0] - art[0] * sx) * 20),
                  translate_y=round((box[2] - art[2] * sy) * 20))


def entry_art(source_menu: bytes, source_list: bytes, host: bytes) -> tuple[bytes, float]:
    """Give every stock main-menu list row FO76's highlight: the yellow menu banner for lists, and the
    option-row bar over a dark row background for settings and controls. The stock entry classes keep
    driving it, since they already show `border` only on the selected row and turn its text black."""
    list_tags, menu_tags = swf_tags(source_list), swf_tags(source_menu)
    hover = symbol_ids(list_tags)[resolve_numbered_name(
        symbol_ids(list_tags), 'MenuListComponent_fla.MenuListEntryHover_', 'menu list highlight')]
    children = _frame_children(menu_tags, symbol_ids(menu_tags)['SettingsOptionItem'])
    row = {item.name: item.character_id for item in children if item.name == 'border'}
    backing = [item.character_id for item in children
               if any(child.name == 'EntryBG_mc' for child in _frame_children(menu_tags, item.character_id))]
    if 'border' not in row or len(backing) != 1:
        raise ValueError('FO76 settings row art changed')
    art = {'B21_ListBanner': _bounds(list_tags, hover), 'B21_OptionBanner': _bounds(menu_tags, row['border']),
           'B21_OptionRowBG': _bounds(menu_tags, backing[0])}
    host = import_art(source_list, host, {hover: 'B21_ListBanner'}, 'B21_ListArt')
    host = import_art(source_menu, host, {row['border']: 'B21_OptionBanner', backing[0]: 'B21_OptionRowBG'},
                      'B21_OptionArt')
    tags = swf_tags(host)
    ids = symbol_ids(tags)
    entries = {ids[name]: name for name in LIST_ENTRIES + OPTION_ENTRIES}
    for c, p in tags:
        if c == 39 and struct.unpack_from('<H', p)[0] not in entries:
            for item in _frame_children(tags, struct.unpack_from('<H', p)[0]):
                if item.character_id in entries:
                    raise ValueError(f'{entries[item.character_id]} is placed on a timeline')
    rebuilt, widths, heights = {}, {}, {}
    for cid, name in entries.items():
        payload = next(p for c, p in tags if c == 39 and struct.unpack_from('<H', p)[0] == cid)
        border = next(item for item in _frame_children(tags, cid) if item.name == 'border')
        depths = {item.depth for item in _frame_children(tags, cid)}
        box = _placed_box(tags, border)
        heights[name] = box[3] - box[2]
        if name in OPTION_ENTRIES:
            # FO76's option rows are much wider. The stock control stays at the fixed x the class gives it,
            # so the row grows to the left, taking its label along.
            art_box = art['B21_OptionBanner']
            widths[name] = (box[3] - box[2]) * (art_box[1] - art_box[0]) / (art_box[3] - art_box[2]) - (box[1] - box[0])
            box = (box[0] - widths[name], box[1], box[2], box[3])
        elif name == 'SaveLoadListEntry':
            # The play time is right-aligned to the stock row, and FO76's banner tapers over its last
            # part, cutting the text; extend it until the full-height part reaches the row's end.
            box = (box[0], box[1] + (box[1] - box[0]) * BANNER_TAPER / (1 - BANNER_TAPER), box[2], box[3])
        output = []
        for code, body in sprite_tags(payload):
            item = placement(code, body)
            if item is not None and item.name == 'textField' and name in OPTION_ENTRIES:
                if code != 26 or item.matrix is None:
                    raise ValueError(f'Unexpected {name} label placement')
                item.matrix.translate_x -= round(widths[name] * 20)
                output.append(tag(26, item.to_bytes()))
                continue
            if item is None or item.name != 'border':
                output.append(tag(code, body))
                continue
            if code != 26 or item.matrix is None or item.depth + 1 in depths and name in OPTION_ENTRIES:
                raise ValueError(f'Unexpected {name} border placement')
            banner = 'B21_OptionBanner' if name in OPTION_ENTRIES else 'B21_ListBanner'
            if name in OPTION_ENTRIES:
                output.append(tag(26, PlaceObject2Tag(depth=item.depth, character_id=ids['B21_OptionRowBG'],
                                                      matrix=_fit(art['B21_OptionRowBG'], box)).to_bytes()))
                item.depth += 1
            item.character_id, item.matrix = ids[banner], _fit(art[banner], box)
            output.append(tag(26, item.to_bytes()))
        rebuilt[cid] = payload[:4] + b''.join(output)
    lists = {}
    for list_name, entry in (('OptionsList', 'SettingsOptionItem'), ('InputMappingList', 'InputMappingListEntry')):
        cid = ids[list_name]
        payload = next(p for c, p in tags if c == 39 and struct.unpack_from('<H', p)[0] == cid)
        output = []
        for code, body in sprite_tags(payload):
            item = placement(code, body)
            if item is not None and item.name in ('border', 'ScrollUp', 'ScrollDown'):
                if code != 26 or item.matrix is None:
                    raise ValueError(f'Unexpected {list_name} {item.name} placement')
                if item.name == 'border':
                    box = _placed_box(tags, item)
                    item.matrix = _fit(_bounds(tags, item.character_id),
                                       (box[0] - widths[entry], box[1], box[2], box[3]))
                else:
                    item.matrix.translate_x -= round(widths[entry] * 20)
                output.append(tag(26, item.to_bytes()))
            else:
                output.append(tag(code, body))
        lists[cid] = payload[:4] + b''.join(output)
    tags = [(c, lists.get(struct.unpack_from('<H', p)[0], p)) if c == 39 else (c, p) for c, p in tags]
    # The rows now place art defined after them; definitions must precede use.
    kept = [(c, p) for c, p in tags if not (c == 39 and struct.unpack_from('<H', p)[0] in rebuilt)]
    at = max(i for i, (c, p) in enumerate(kept) if c in DEFINITIONS) + 1
    kept[at:at] = [(39, payload) for payload in rebuilt.values()]
    row = art['B21_OptionBanner']
    # Panels scale so a stock row is as tall as FO76's, which is measured in its 1920x1080 layout.
    return replace_tags(host, kept), (row[3] - row[2]) / heights['SettingsOptionItem']


def loading_art(source: bytes, host: bytes) -> tuple[bytes, dict]:
    root = source_layout(source, 'LoadingMenu')
    names = {'Box_mc': 'B21_LoadingBox', 'LeftText_mc': 'B21_LoadingText'}
    # The event-details panel has no equivalent FO4 loading data. Keep the actual tip/level presentation.
    left = root['LeftText_mc'].character_id
    host = import_art(source, host, {root[n].character_id: cls for n, cls in names.items()}, 'B21_LoadingArt', {
        left: {'LoadScreenText_tf', 'LevelText_tf', 'PlayerLevelMeter_mc', 'LoadingArea_tf', 'Dots_mc'},
    })
    return host, {n: transform(root[n]) for n in names}


def layout_script(main: dict, loading: dict) -> str:
    return ('package { public class B21_MenuLayout { public static var main:Object = ' + json.dumps(main) +
            '; public static var loading:Object = ' + json.dumps(loading) + '; } }')
