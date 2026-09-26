from __future__ import annotations

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import threading

from playwright.sync_api import sync_playwright

from bacup_lib.reputation_ui import OUTPUT, convert_reputation_ui
from bacup_lib.legendary_perks_ui import swf_tags

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("--source-root", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--ruffle", type=Path, required=True)
parser.add_argument("--browser", type=Path, required=True)
args = parser.parse_args()
output = args.output.resolve()
manifest = convert_reputation_ui(args.source_root, output / "data")
bundle = output / "data" / OUTPUT
art = {2, 6, 20, 21, 22, 32, 35, 36, 46, 83, 84, 90}
original = swf_tags((args.source_root / "interface/hudreputationmeter.swf").read_bytes())
converted = swf_tags((bundle / "reputationhud.swf").read_bytes())
assert [(c, p) for c, p in original if c in art] == [(c, p) for c, p in converted if c in art]
fixture = ROOT / "bacup/py_bacup_lib/python/bacup_lib/tests/fixtures/reputation_preview.as"
project = output / "preview.swfproj"
project.write_text(json.dumps({"canvas": [1920, 1080], "fps": 30, "version": 17,
    "scripts": [str(fixture)], "exports": [{"character": 0, "class": "B21_ReputationPreview"}]}))
subprocess.run(["modkit.exe", "swf", "pack", str(project), "-o", str(bundle / "preview.swf")], check=True)
ruffle = "/" + args.ruffle.resolve().relative_to(ROOT).as_posix()
(bundle / "preview.html").write_text('''<!doctype html><style>
body{margin:0;background:#293933}ruffle-player{width:1280px;height:720px;display:block}
</style><script>
window.RufflePlayer={config:{autoplay:"on",unmuteOverlay:"hidden",allowScriptAccess:true}};
window.addEventListener("DOMContentLoaded",function(){window.player=RufflePlayer.newest().createPlayer();
document.body.appendChild(player);player.ruffle().load({url:"preview.swf",allowScriptAccess:true});});
</script><script src="''' + ruffle + '"></script>', encoding="utf-8")


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(ROOT)))
threading.Thread(target=server.serve_forever, daemon=True).start()
messages, results = [], []
try:
    with sync_playwright() as browser_api:
        browser = browser_api.chromium.launch(executable_path=str(args.browser), headless=True)
        page = browser.new_page(viewport={"width": 1280, "height": 720})
        page.on("console", lambda message: messages.append(message.text))
        page.goto(f"http://127.0.0.1:{server.server_port}/" + (bundle / "preview.html").relative_to(ROOT).as_posix())
        page.wait_for_function('player.ruffle().callExternalInterface("state")?.ready', timeout=30000)
        for sequence, faction, tier, before, after, level_up in (
            (1, "Crater", 2, .2, .4, False), (2, "Foundation", 3, .7, .5, False),
            (3, "Crater", 4, 1., 0., True), (4, "Foundation", 6, 1., 1., False),
        ):
            sample = {"visible": True, "sequence": sequence, "factionCode": faction,
                      "factionName": "$" + faction, "tierStart": tier - int(level_up), "tierEnd": tier,
                      "percentStart": before, "percentEnd": after, "levelUp": level_up}
            error = page.evaluate('data=>player.ruffle().callExternalInterface("sample",data)', sample)
            assert error == "", error
            page.wait_for_timeout(900)
            assert page.evaluate('player.ruffle().callExternalInterface("state").visible')
            page.screenshot(path=str(output / f"reputation-{sequence}.png"))
            assert page.evaluate('data=>player.ruffle().callExternalInterface("sample",data)', sample) == ""
            page.wait_for_function('n=>player.ruffle().callExternalInterface("state").done===n', arg=sequence, timeout=12000)
            state = page.evaluate('player.ruffle().callExternalInterface("state")')
            assert state["completions"] == sequence and state["reported"] == sequence and not state["visible"], state
            if not level_up:
                assert abs(state["meter"] - after) < .01, state
            if tier == 6:
                assert state["left"] == state["right"] == "ally", state
            results.append(state)
        browser.close()
finally:
    server.shutdown()
    (output / "console.log").write_text("\n".join(messages), encoding="utf-8")
(output / "preview-results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
print(f"Production reputation SWF: {len(results)} cases completed exactly once; {manifest['document_class']} bound")
