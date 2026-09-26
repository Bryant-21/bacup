import json
from pathlib import Path


CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/menu-presentation.json")
MUSIC = "Music/B21/TalesFromAppalachia/Menu/"
LEGACY_MUSIC = "F4SE/Plugins/B21_TalesFromAppalachia/menu-media/Music/"


def startup_assets(mod_dir: Path) -> dict[Path, Path]:
    catalog = mod_dir / CATALOG
    if not catalog.is_file():
        return {}
    data = json.loads(catalog.read_text(encoding="utf-8"))
    paths = {}
    for family in ("OG", "NG", "AE"):
        name = "B21_TFAMainMenu_" + family
        expected = "Interface/" + name + ".swf"
        if data.get("menus", {}).get(name) == expected:
            paths[Path(expected)] = mod_dir / "data" / expected
    for pair in data.get("media_pairs", []):
        music = pair.get("music", "")
        legacy = music.startswith(LEGACY_MUSIC)
        if not legacy and not music.startswith(MUSIC):
            continue
        filename = music.removeprefix(LEGACY_MUSIC if legacy else MUSIC)
        if (filename.endswith(".xwm") and not any(token in filename for token in ("..", ":", "\\", "/"))):
            paths[Path(MUSIC + filename)] = mod_dir / music if legacy else mod_dir / "data" / music
    for relative, source in paths.items():
        if not source.resolve().is_relative_to(mod_dir.resolve()) or not source.is_file():
            raise FileNotFoundError(f"Converted startup asset unavailable: {relative}")
    return dict(sorted(paths.items()))


def write_always_loose(mod_dir: Path) -> int:
    from creation_lib.build.deployer import ALWAYS_LOOSE_NAME

    assets = startup_assets(mod_dir)
    listing = mod_dir / ALWAYS_LOOSE_NAME
    if not assets:
        listing.unlink(missing_ok=True)
        return 0
    files = {relative.as_posix(): source.resolve().relative_to(mod_dir.resolve()).as_posix()
             for relative, source in assets.items()}
    listing.write_text(json.dumps({"files": files}, indent=2) + "\n", encoding="utf-8")
    return len(files)
