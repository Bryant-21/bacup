import argparse
import json
from pathlib import Path
import shutil
import subprocess

from bacup_lib.workshop_ui import RESOURCES, convert_workshop_prototype


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("target_movie", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--ruffle-url", default="../native-as3-preview/package/ruffle.js")
    args = parser.parse_args()
    interface = args.output / "data/Interface"
    receipt = convert_workshop_prototype(args.source_root, args.target_movie, args.output / "data")
    shutil.copy2(args.target_movie.with_name("fonts_en.swf"), interface / "fonts_en.swf")
    project = args.output / "preview.swfproj"
    project.write_text(json.dumps({"canvas": [1280, 720], "fps": 30, "version": 15,
        "background": "#40545d", "scripts": [str((RESOURCES / "WorkshopPreview.as").resolve())],
        "exports": [{"character": 0, "class": "WorkshopPreview"}]}), encoding="utf-8")
    subprocess.run(["modkit.exe", "swf", "pack", str(project), "-o", str(interface / "preview.swf")], check=True)
    html = '''<!doctype html><html><head><meta charset="utf-8"><title>Workshop mixed-preview prototype</title>
<style>body{margin:0;background:#20282b;color:white;font:14px monospace}#player{width:min(100vw,1280px);height:min(56.25vw,720px)}ruffle-player{width:100%;height:100%}pre{margin:12px}</style></head>
<body><div id="player"></div><pre id="report">Loading...</pre><script>
window.RufflePlayer={config:{autoplay:"on",unmuteOverlay:"hidden",allowScriptAccess:true,logLevel:"warn"}};
function report(value){document.getElementById("report").textContent=value}
window.addEventListener("DOMContentLoaded",()=>{const p=window.RufflePlayer.newest().createPlayer();document.getElementById("player").appendChild(p);p.ruffle().load({url:"data/Interface/preview.swf",base:"data/Interface/",allowScriptAccess:true});});
</script><script src=RUFFLE_URL></script></body></html>'''
    (args.output / "index.html").write_text(html.replace("RUFFLE_URL", json.dumps(args.ruffle_url)), encoding="utf-8")
    print(json.dumps({"preview": str(args.output / "index.html"), "files": len(receipt["files"])}))


if __name__ == "__main__":
    main()
