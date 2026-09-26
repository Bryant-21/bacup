from __future__ import annotations

import argparse
import shutil
from pathlib import Path

from bacup_lib.challenge_ui import CLASS_PREFIX, MENU, OUTPUT, RESOURCES, convert_challenge_ui
from creation_lib.swf.native_runtime import augment_as3_classes, patch_as3_method


def build_preview(source: Path, destination: Path) -> Path:
    manifest = convert_challenge_ui(source, destination / "data")
    bundle = destination / "data" / OUTPUT
    root_class = f"{CLASS_PREFIX}_SeventySixMenu"
    movie = (bundle / MENU).read_bytes()
    movie = augment_as3_classes(movie, {root_class: (RESOURCES / "Preview.as").read_text(encoding="utf-8")})
    movie = patch_as3_method(movie, root_class, "$constructor",
                             [["getlocal", 0], ["getproperty", "Added"], ["callpropvoid", "addEventListener", 2]],
                             [["keep", 0], ["keep", 1], ["keep", 2], ["getlocal", 0], ["callpropvoid", "B21PreviewInit", 0]])
    (destination / "preview.swf").write_bytes(movie)
    for item in manifest["files"]:
        asset = destination / "data" / item["path"]
        shutil.copyfile(asset, destination / asset.name)
    (destination / "preview.html").write_text('''<!doctype html><html><style>
body{margin:0;background:#101921;color:#eee}ruffle-player{width:1280px;height:720px;display:block}
#report{white-space:pre-wrap;font:14px monospace}</style><body><div id="report">Loading</div><script>
window.result=null;function report(value){window.result=value;document.getElementById('report').textContent=value}
window.RufflePlayer={config:{autoplay:'on',unmuteOverlay:'hidden',allowScriptAccess:true,scale:'exactFit'}};
window.addEventListener('DOMContentLoaded',()=>{window.player=RufflePlayer.newest().createPlayer();document.body.prepend(player);
player.ruffle().load({url:'preview.swf',allowScriptAccess:true})});</script>
<script src="/tmp/currency-layout/ruffle/package/ruffle.js"></script></body></html>''', encoding="utf-8")
    return destination / "preview.html"


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    print(build_preview(args.source, args.destination))
