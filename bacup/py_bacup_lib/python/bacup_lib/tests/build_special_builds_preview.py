from pathlib import Path
import argparse
import shutil
import json
import subprocess

from bacup_lib.special_builds_ui import MENU, OUTPUT, RESOURCES, convert_special_builds_ui
from creation_lib.swf.native_runtime import augment_as3_classes, patch_as3_method


def build(source: Path, output: Path) -> Path:
    manifest = convert_special_builds_ui(source, output / "data")
    bundle = output / "data" / OUTPUT
    name = "SpecialBuilds.SpecialBuildsMenu"
    movie = augment_as3_classes((bundle / MENU).read_bytes(), {
        name: (RESOURCES / "Preview.as").read_text(),
        "SpecialBuilds.EditSpecialModal": (RESOURCES / "EditPreview.as").read_text()})
    movie = patch_as3_method(movie, name, "$constructor", [["callpropvoid", "B21Initialize", 0]],
        [["keep", 0], ["getlocal", 0], ["callpropvoid", "PreviewInit", 0]])
    for item in manifest["files"]:
        asset = output / "data" / item["path"]
        shutil.copyfile(asset, output / asset.name)
    (output / "preview.swf").write_bytes(movie)
    project = output / "loader.swfproj"
    project.write_text(json.dumps({"canvas": [1920, 1080], "version": 14, "fps": 30,
        "scripts": [str((RESOURCES / "PreviewLoader.as").resolve())],
        "exports": [{"character": 0, "class": "PreviewLoader"}]}))
    subprocess.run(["modkit.exe", "swf", "pack", str(project), "-o", str(output / "loader.swf")], check=True)
    (output / "preview.html").write_text('''<!doctype html><html><style>
body{margin:0;background:#16191a;color:#eee}ruffle-player{width:1280px;height:720px;display:block}
#report{white-space:pre-wrap;font:14px monospace}</style><body><div id="report">Loading</div><script>
window.result=null;function report(value){window.result=value;document.getElementById('report').textContent=value}
window.RufflePlayer={config:{autoplay:'on',unmuteOverlay:'hidden',allowScriptAccess:true,scale:'exactFit'}};
window.addEventListener('DOMContentLoaded',()=>{window.player=RufflePlayer.newest().createPlayer();document.body.prepend(player);
player.ruffle().load({url:'loader.swf',allowScriptAccess:true})});</script>
<div id="controls"></div><script>
for (const action of ['Read','Up','Down','Left','Right','XButton','YButton','Accept','Cancel']) {
  const b=document.createElement('button'); b.textContent=action;
  b.onclick=()=>report(player.command(action)); document.getElementById('controls').append(b);
}
</script><script src="/tmp/currency-layout/ruffle/package/ruffle.js"></script></body></html>''')
    return output / "preview.html"


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    print(build(args.source, args.output))
