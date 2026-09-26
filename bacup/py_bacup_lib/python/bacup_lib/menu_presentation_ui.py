from __future__ import annotations

import hashlib
import json
import re
import shutil
from pathlib import Path

from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.menu_presentation_art import main_art, loading_art, layout_script
from bacup_lib.menu_presentation_deploy import write_always_loose
from bacup_lib.status_hud_source import replace_tags
from bacup_lib.translations import merge_ui_translations, read_table
from creation_lib.swf import native_runtime


OUTPUT = Path("Interface/B21/TalesFromAppalachia/MenuPresentation")
LOOSE_MEDIA = Path("F4SE/Plugins/B21_TalesFromAppalachia/menu-media")
MENUS = ("seventysixmenu.swf", "menulistcomponent.swf", "loadingmenu.swf")
VIDEOS = ("MainMenuLoop.bk2", "Intro.bk2")
MUSIC_SETTING = re.compile(r"^\s*sMainMenuMusic\s*=\s*(.+?)\s*$", re.IGNORECASE | re.MULTILINE)
RESOURCES = Path(__file__).with_name("resources") / "menu_presentation"


def _read_music_path(ini: Path) -> Path:
    match = MUSIC_SETTING.search(ini.read_text(encoding="utf-8-sig"))
    if not match:
        raise ValueError("FO76 source does not declare sMainMenuMusic")
    value = match.group(1).replace("\\", "/")
    relative = Path(value)
    if relative.is_absolute() or not value.lower().startswith("data/music/") or ".." in relative.parts:
        raise ValueError("FO76 main-menu music path escapes Data/Music")
    return Path(*relative.parts[1:])


def _checked_file(path: Path, header: bytes) -> Path:
    with path.open("rb") as stream:
        data = stream.read(32)
    if len(data) < 32 or not data.startswith(header) or (header == b"RIFF" and data[8:12] != b"XWMA"):
        raise ValueError(f"Invalid FO76 media: {path}")
    return path


def _present_media(path: Path, header: bytes, missing: list[str]) -> Path | None:
    # Players delete FO76 videos and music to save space; each missing file only drops itself.
    if not path.is_file():
        missing.append(path.as_posix())
        return None
    return _checked_file(path, header)


def _hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def host_file(target_interface: Path, name: str, target_data_dir: Path | None = None) -> Path:
    loose = Path(target_data_dir) / 'Interface' / name if target_data_dir else None
    return loose if loose and loose.is_file() else target_interface / name


def main_host_family(movie: bytes) -> str:
    traits = {t['name'] for t in native_runtime.abc_class_outline(movie, 'MainMenu')['instance_traits']}
    return 'AE' if 'INSTALLED_CONTENT_INDEX' in traits else 'NG' if 'PS5TRANSFERSAVE_INDEX' in traits else 'OG'


def build_hosts(source_interface: Path, target_interface: Path,
                target_data_dir: Path | None = None) -> tuple[dict[Path, bytes], dict]:
    hosts = {}
    for path in (target_interface / 'mainmenu.swf', host_file(target_interface, 'mainmenu.swf', target_data_dir)):
        movie = path.read_bytes()
        hosts['MainMenu_' + main_host_family(movie)] = movie
    hosts['LoadingMenu'] = host_file(target_interface, 'loadingmenu.swf', target_data_dir).read_bytes()
    loading, loading_layout = loading_art((source_interface / 'loadingmenu.swf').read_bytes(),
                                         hosts['LoadingMenu'])
    outputs = {}
    main_layout = {}
    for host_name, original in hosts.items():
        name = host_name.split('_')[0]
        movie = loading
        if name == 'MainMenu':
            movie, main_layout = main_art((source_interface / 'seventysixmenu.swf').read_bytes(),
                (source_interface / 'menulistcomponent.swf').read_bytes(), original)
        scripts = [layout_script(main_layout, loading_layout)]
        if name == 'MainMenu':
            scripts.append((RESOURCES / 'B21_MainRows.as').read_text(encoding='utf-8'))
        entries = swf_tags(movie)
        at = next(i for i, (c, _) in enumerate(entries) if c == 76)
        entries.insert(at, (82, native_runtime.compile_as3_do_abc(scripts)))
        movie = replace_tags(movie, entries)
        bridge = (RESOURCES / (name + '.as')).read_text(encoding='utf-8')
        bridge = bridge.replace('__B21_HOST_FAMILY__', host_name.removeprefix('MainMenu_'))
        movie = native_runtime.augment_as3_classes(movie, {name: bridge})
        if name == 'MainMenu':
            movie = native_runtime.patch_as3_method(movie, name, 'InitList', [['callpropvoid', 'InvalidateData', 0]],
                [['keep', 0], ['getlocal', 0], ['callpropvoid', 'B21InstallPhotoEntry', 0]])
            movie = native_runtime.patch_as3_method(movie, name, 'onListItemPress', [['callpropvoid', 'onMainListItemPress', 0]],
                [['callpropvoid', 'B21MainPress', 0]])
            movie = native_runtime.patch_as3_method(movie, name, 'RequestOptions',
                [['callpropvoid', 'RequestDisplayOptions', 1]],
                [['keep', 0], ['getlocal', 0], ['getlocal', 2], ['callpropvoid', 'B21AppendDisplayOptions', 1]])
        if name == 'LoadingMenu':
            movie = native_runtime.patch_as3_method(movie, name, 'SetLevel', [['callpropvoid', 'SetMeter', 3]],
                [['keep', 0], ['getlocal', 0], ['getlocal', 2], ['callpropvoid', 'B21LevelChanged', 1]])
        if native_runtime.unbacked_symbol_classes(movie):
            raise ValueError(f'Unbacked converted {name} classes')
        outputs[Path('Interface/B21_TFA' + host_name + '.swf')] = movie
    return outputs, {'main': main_layout, 'loading': loading_layout,
                     'target_hosts': {name: hashlib.sha256(movie).hexdigest() for name, movie in hosts.items()}}


def convert_menu_presentation(source_root: Path, source_game: Path, output_data: Path,
                              target_root: Path | None = None, target_data_dir: Path | None = None) -> dict:
    source_root, source_game, output_data = map(Path, (source_root, source_game, output_data))
    source_interface = source_root / "interface"
    closure = {name: (source_interface / name).read_bytes() for name in MENUS}
    for name, class_name in (("seventysixmenu.swf", "SeventySixMenu"),
                             ("menulistcomponent.swf", "MainMenuEntry"), ("loadingmenu.swf", "LoadingMenu")):
        if class_name not in native_runtime.abc_class_names(closure[name]):
            raise ValueError(f"FO76 {name} document class is missing")

    music = _read_music_path(source_game / "Fallout76.ini")
    source_music = source_root / Path(*[part.lower() for part in music.parts])
    if not source_music.is_file():
        source_music = source_game / "Data" / music
    music_sources = {source_music}
    music_sources.update(path for path in (source_root / "music/special").glob("*.xwm")
                         if "main" in path.stem.lower())
    media: dict[Path, Path] = {}
    missing_media: list[str] = []
    tracks: list[tuple[str, Path]] = []
    for track in sorted(music_sources, key=lambda path: path.name.lower()):
        target = LOOSE_MEDIA / "Music" / track.name
        if _present_media(track, b"RIFF", missing_media) is None:
            continue
        media[target] = track
        tracks.append((track.stem.lower(), target))
    for video in VIDEOS:
        source_video = _present_media(source_game / "Data" / "Video" / video, b"KB2", missing_media)
        if source_video is not None:
            media[LOOSE_MEDIA / "Video" / video] = source_video
    # FO76 videos are KB2j, which FO4's older Bink runtime cannot decode; Tales plays them with this one.
    bink = _present_media(source_game / "bink2w64.dll", b"MZ", missing_media)
    if bink is not None:
        media[LOOSE_MEDIA / "Video" / "bink2w64_fo76.dll"] = bink

    outputs, layout = {}, {}
    if target_root is not None:
        outputs, layout = build_hosts(source_interface, Path(target_root) / 'interface', target_data_dir)
    pairs = [{
        "id": name,
        "loop": (LOOSE_MEDIA / "Video/MainMenuLoop.bk2").as_posix(),
        "music": path.as_posix(),
        "source_music": "Data/Music/Special/" + path.name,
    } for name, path in tracks]
    manifest = {
        "schema_version": 1,
        "host_version": 2,
        "menus": {path.stem: path.as_posix() for path in outputs},
        "source_layout": layout,
        "sources": {name: hashlib.sha256(data).hexdigest() for name, data in closure.items()},
        "media_pairs": pairs,
        "missing_media": missing_media,
        # Appalachia art only: expedition (expd_) and Nuclear Winter (nw_) screens are left out.
        "loading_backgrounds": sorted(f"Textures/interface/loadingmenubackgrounds/{path.name.lower()}" for path in
                                      (source_root / "textures/interface/loadingmenubackgrounds").glob("ls_*.dds")),
        "files": [{"path": (Path("data") / path).as_posix(), "sha256": hashlib.sha256(data).hexdigest()}
                  for path, data in sorted(outputs.items())] +
                 [{"path": path.as_posix(), "sha256": _hash_file(source)}
                  for path, source in sorted(media.items())],
    }
    translations = [line for line in read_table(source_interface / "translate_en.txt")
                    if line.split("\t", 1)[0].startswith(("$MainMenu", "$Loading", "$VDSG"))]
    previous_manifest = output_data / OUTPUT / 'conversion.json'
    if previous_manifest.is_file():
        previous = json.loads(previous_manifest.read_text(encoding='utf-8'))
        for entry in previous.get('files', []):
            relative = Path(entry['path'])
            if relative.parts[:1] == ('data',):
                relative = Path(*relative.parts[1:])
            target = (output_data / relative).resolve()
            if (relative not in outputs and relative.suffix.lower() == '.swf' and
                    (target.is_relative_to((output_data / OUTPUT).resolve()) or
                     target == (output_data / 'Interface/B21_TFAMainMenu.swf').resolve())):
                target.unlink(missing_ok=True)
    for path, data in outputs.items():
        target = output_data / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    for path, source in media.items():
        target = output_data.parent / path
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    (output_data / OUTPUT).mkdir(parents=True, exist_ok=True)
    (output_data / OUTPUT / "conversion.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    loose_manifest = output_data.parent / LOOSE_MEDIA.parent / "menu-presentation.json"
    loose_manifest.parent.mkdir(parents=True, exist_ok=True)
    loose_manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    write_always_loose(output_data.parent)
    return manifest
