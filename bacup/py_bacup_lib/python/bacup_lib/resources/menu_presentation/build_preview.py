from pathlib import Path
import argparse
import json
import shutil
from PIL import Image

from bacup_lib.menu_presentation_ui import RESOURCES, host_file, main_host_family
from bacup_lib.legendary_perks_ui import imports, swf_tags
from bacup_lib.status_hud_source import replace_tags
from bacup_lib.translations import read_table
from bacup_lib.resources.menu_presentation.preview_fonts import embed_game_fonts
from creation_lib.swf import native_runtime as native


def build_preview(converted_data: Path, output: Path, ruffle_package: Path,
                  source: Path = Path('extracted/fo76'), target: Path = Path('extracted/fo4'),
                  target_data_dir: Path | None = None) -> None:
    output.mkdir(parents=True, exist_ok=True)
    strings = dict(line.split('\t', 1) for line in read_table(host_file(target / 'interface', 'translate_en.txt', target_data_dir)) if '\t' in line)
    strings.update(dict(line.split('\t', 1) for line in read_table(source / 'interface/translate_en.txt')
                        if line.split('\t', 1)[0] in ('$Loading', '$LEVEL')))
    strings['$B21_PM_Menu'] = 'PHOTO MODE'
    # Fallout4.esm LSCR 021E50 (PipBoy01), English Description. The fixture calls the live host setter.
    tip = 'The RobCo Pip-Boy is the ultimate in personal computing devices.'
    script = ('package { public class B21_PreviewStrings { public static var values:Object = ' + json.dumps(strings) +
              '; public static var tip:String = ' + json.dumps(tip) + '; } }')
    dependencies = {'main_loginhelper.swf'}
    for name, fixture, hook in (('MainMenu', 'MainPreview.as', '__setProp_CharacterSelectList_mc_MenuObj_CharacterSelectList_0'),
                                 ('LoadingMenu', 'LoadingPreview.as', 'start')):
        family = main_host_family(host_file(target / 'interface', 'mainmenu.swf', target_data_dir).read_bytes()) if name == 'MainMenu' else ''
        production = converted_data / ('Interface/B21_TFA' + name + ('_' + family if family else '') + '.swf')
        movie = production.read_bytes()
        shutil.copyfile(production, output / production.name)
        tags = swf_tags(movie)
        tags.insert(next(i for i, (c, _) in enumerate(tags) if c == 76), (82, native.compile_as3_do_abc([script])))
        fixture_source = (RESOURCES / fixture).read_text(encoding='utf-8')
        if name == 'MainMenu':
            init = next(t for t in native.abc_class_outline(movie, name)['instance_traits'] if t['name'] == 'InitList')
            arguments = ['true', 'true', 'false', 'false', 'true', 'true', 'true', 'false', 'false', 'false']
            fixture_source = fixture_source.replace('__B21_INIT_LIST_ARGS__', ','.join(arguments[:len(init['signature']['params'])]))
        movie = native.augment_as3_classes(replace_tags(movie, tags), {name: fixture_source})
        movie = native.patch_as3_method(movie, name, '$constructor', [['callpropvoid', hook, 0]],
            [['keep', 0], ['getlocal', 0], ['callpropvoid', 'B21PreviewBoot', 0]])
        movie = embed_game_fonts(movie, host_file(target / 'interface', 'fonts_en.swf', target_data_dir).read_bytes())
        (output / ('main-preview.swf' if name == 'MainMenu' else 'loading-preview.swf')).write_bytes(movie)
        dependencies.update(imports(movie))
    copied = set()
    while dependencies:
        name = dependencies.pop().lower()
        if name in copied:
            continue
        copied.add(name)
        data = host_file(target / 'interface', name, target_data_dir).read_bytes()
        (output / name).write_bytes(data)
        dependencies.update(imports(data))
    Image.open(source / 'textures/interface/loadingmenubackgrounds/ls_mischief_reloaded.dds').save(output / 'photo-background.png')
    shutil.copytree(ruffle_package, output / 'ruffle', dirs_exist_ok=True)
    (output / 'index.html').write_text('''<!doctype html><meta charset="utf-8"><style>
body{margin:0;background:#182024;color:#eee;font:14px sans-serif}header{padding:10px}button,select{margin-right:8px}
#screen{width:100vw;height:calc(100vh - 60px)}ruffle-player{width:100%;height:100%}
</style><header><button onclick="show('main')">Main menu</button><button onclick="show('loading')">Loading screen</button>
<span id="report">Loading</span><br>Compiled FO4 hosts with FO76 art. Engine callbacks are stubbed; the background is a source loading image, not Bink playback.</header>
<div id="screen"></div><script>window.result=null;function report(r){window.result=r;document.querySelector('#report').textContent=JSON.stringify(r);}
window.RufflePlayer={config:{autoplay:'on',unmuteOverlay:'hidden',allowScriptAccess:true}};
async function show(kind){window.result=null;window.player=RufflePlayer.newest().createPlayer();document.querySelector('#screen').replaceChildren(player);await player.ruffle().load({url:kind+'-preview.swf'});}
window.addEventListener('load',()=>show(location.hash.slice(1)||'main'));</script><script src="ruffle/ruffle.js"></script>''', encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('converted_data', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('ruffle_package', type=Path)
    parser.add_argument('--source', type=Path, default=Path('extracted/fo76'))
    parser.add_argument('--target', type=Path, default=Path('extracted/fo4'))
    args = parser.parse_args()
    build_preview(args.converted_data, args.output, args.ruffle_package, args.source, args.target)
