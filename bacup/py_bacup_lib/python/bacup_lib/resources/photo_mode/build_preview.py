from pathlib import Path
import argparse
import shutil
import re
import struct
import zlib
import json

from bacup_lib.photo_mode_ui import BRIDGE, MENU, OUTPUT, convert_photo_mode_ui
from creation_lib.swf import native_runtime
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.quest_area_ui import symbol_ids
from bacup_lib.status_hud_source import inject_art, replace_tags, resolve_class_placements
from bacup_lib.translations import read_table, merge_packaged, translation_path
from bacup_lib.resources.menu_presentation.preview_fonts import embed_game_fonts


def preview_compatible(movie: bytes) -> bytes:
    if movie[:3] != b"CWS":
        raise ValueError("Expected compressed converted PhotoMode movie")
    body = zlib.decompress(movie[8:])
    rect_bits = body[0] >> 3
    start = (5 + rect_bits * 4 + 7) // 8 + 4

    def tags(data: bytes) -> bytes:
        result = bytearray()
        cursor = 0
        while cursor < len(data):
            header = struct.unpack_from("<H", data, cursor)[0]
            cursor += 2
            kind, length = header >> 6, header & 63
            if length == 63:
                length = struct.unpack_from("<I", data, cursor)[0]
                cursor += 4
            payload = data[cursor:cursor + length]
            cursor += length
            if kind == 39:
                payload = payload[:4] + tags(payload[4:])
            elif kind == 70 and len(payload) > 4 and payload[1] == 8 and not payload[0] & 2:
                continue
            elif kind == 70 and len(payload) > 6 and payload[1] == 1:
                names = list(re.finditer(rb"[A-Za-z][A-Za-z0-9_]{2,}\x00", payload[6:]))
                if not names:
                    raise ValueError("PhotoMode preview could not locate filtered instance name")
                end = 6 + names[0].end()
                payload = payload[:1] + b"\x00" + payload[2:end]
            result.extend(struct.pack("<H", kind << 6 | min(len(payload), 63)))
            if len(payload) >= 63:
                result.extend(struct.pack("<I", len(payload)))
            result.extend(payload)
            if kind == 0:
                break
        return bytes(result)

    result = body[:start] + tags(body[start:])
    return movie[:4] + struct.pack("<I", len(result) + 8) + zlib.compress(result)


def build_preview(source: Path, output: Path, ruffle_package: Path):
    manifest = convert_photo_mode_ui(source, output / "data")
    merge_packaged(translation_path(output / 'data'))
    movie = (output / "data" / OUTPUT / MENU).read_bytes()
    hints = (output / 'data' / OUTPUT / 'bsbuttonhintbar.swf').read_bytes()
    movie = inject_art(hints, movie, [('Shared.AS3.BSButtonHintBar', 'Shared.AS3.BSButtonHintBar'),
                                    ('Shared.AS3.BSButtonHint', 'Shared.AS3.BSButtonHint')])
    symbols = symbol_ids(swf_tags(movie))
    movie = replace_tags(movie, [(c, resolve_class_placements(p, symbols) if c == 39 else p)
                                for c, p in swf_tags(movie)])
    strings = dict(line.split('\t', 1) for line in read_table(output / 'F4SE/Plugins/B21_TalesFromAppalachia_en.txt') if '\t' in line)
    options = {name: ['$OFF', '$ON'] for name in ('Show Player', 'Depth of Field', 'Freeze Time')}
    options.update({'Pose': ['$NONE', '$B21_PM_Salute'], 'Pose Category': ['$B21_PM_FO4Poses'],
                    'Frame': ['$NONE', '$B21_PM_Vintage', '$B21_PM_Keepsake'], 'Frame Category': ['$B21_PM_PhotoFrames'],
                    'Filter': ['$NONE'] + ['$B21_PM_' + key for key in (
                        'Saturated', 'BlackAndWhite', 'Sepia', 'PhotoFlash', 'Cool', 'Dandy', 'Daytripper', 'Enhance',
                        'Gamma', 'Nuke', 'Tintype', 'Vintage', 'Warm', 'Acid', 'Electric', 'Jet', '3dViz', 'OnlyRed',
                        'Feral', 'SugarBomb', 'VaultBlue')]})
    script = ('package { public class B21_PhotoStrings { public static var values:Object = ' + json.dumps(strings) +
              '; public static var options:Object = ' + json.dumps(options) + '; } }')
    tags = swf_tags(movie)
    tags.insert(next(i for i, (c, _) in enumerate(tags) if c == 76), (82, native_runtime.compile_as3_do_abc([script])))
    movie = replace_tags(movie, tags)
    movie = native_runtime.augment_as3_classes(movie, {
        'Panel': BRIDGE.with_name('PanelPreview.as').read_text(),
        "SelfieMenu": BRIDGE.with_name("Preview.as").read_text()})
    for name, clip in (('OptionStepperWithLabel', 'Stepper_mc.RightArrow_mc'),
                       ('OptionSliderWithLabel', 'Slider_mc.Slider_Selected_mc.Slider_Range_Selected_mc')):
        movie = native_runtime.augment_as3_classes(movie, {name:
            'package { import flash.display.MovieClip; import flash.display.DisplayObject; public class ' + name +
            ' extends MovieClip { public function B21PreviewBounds(root:DisplayObject):Object {'
            'var b:Object=getBounds(root); try { b=' + clip + '.getBounds(root); } catch (error:Error) {} '
            'return {x:b.x,y:b.y,width:b.width,height:b.height}; } } }'})
    movie = native_runtime.patch_as3_method(movie, "SelfieMenu", "$constructor",
        [["callproperty", "B21RegisterHandler", 0], ["callpropvoid", "addEventListener", 2]],
        [["keep", 0], ["keep", 1], ["getlocal", 0], ["callpropvoid", "B21PreviewInit", 0]])
    # Byte-exact helpers may return FWS; normalize before the preview-only filter adaptation.
    if movie[:3] == b'FWS':
        movie = b'CWS' + movie[3:8] + zlib.compress(movie[8:])
    movie = embed_game_fonts(preview_compatible(movie), (source.parent / 'fo4/interface/fonts_en.swf').read_bytes())
    (output / "preview.swf").write_bytes(movie)
    shutil.copytree(ruffle_package, output / "ruffle", dirs_exist_ok=True)
    for item in manifest["files"]:
        path = output / "data" / item["path"]
        if path.suffix == ".swf":
            shutil.copyfile(path, output / path.name)
    (output / "index.html").write_text('''<!doctype html><html><style>
body{margin:0;background:linear-gradient(130deg,#697d86,#202b31);color:white;font:16px sans-serif}
ruffle-player{width:100vw;height:calc(100vh - 40px);display:block}#report{white-space:pre-wrap}
</style><body><div id="report">Loading converted FO76 PhotoMode</div><script>
window.result=null; window.actions=[];
function report(v){window.result=v; document.querySelector('#report').textContent=JSON.stringify(v)}
function action(v){window.actions.push(v)}
window.RufflePlayer={config:{autoplay:'on',unmuteOverlay:'hidden',allowScriptAccess:true,scale:'showAll'}};
window.addEventListener('DOMContentLoaded',()=>{window.player=RufflePlayer.newest().createPlayer();
document.body.prepend(player);player.ruffle().load({url:'preview.swf',allowScriptAccess:true})});
</script><script src="ruffle/ruffle.js"></script></body></html>''', encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("ruffle_package", type=Path)
    args = parser.parse_args()
    build_preview(args.source, args.output, args.ruffle_package)
