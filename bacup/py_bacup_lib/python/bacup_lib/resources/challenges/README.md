# Converted FO76 challenge screen

`challenge_ui.py` converts the user's `seventysixmenu.swf` into
`data/Interface/B21/TalesFromAppalachia/Challenges/`. It retains FO76's challenge
screen, scrolling lists, category artwork, checkmarks and reward library. Native
class replacement supplies the local Tales snapshot and tracking contract.

The category list combines Daily, Weekly and the original lifetime categories.
Daily/weekly headings include completed/total counts. The UTC reset countdown and
list state refresh once per second without losing the selected parent or child.
Completed entries are hidden until toggled. Mouse row clicks and Enter/A select
children or toggle tracking; T/Y tracks, X toggles completed entries, Q/E or LB/RB
switches categories, and Tab/Escape/B closes.

The source's class-only category placement uses a Scaleform extension that the
native renderer and Ruffle cannot parse. Conversion imports the original
MenuListComponent symbol closure and supplies an explicit character binding.
The online SCORE widget placement is removed. Caps and XP source artwork is
embedded so FO76's external icon loading contract is unnecessary. Remaining
source imports are copied recursively, and conversion.json records all hashes.

Inspection gap: `modkit swf inspect` does not report class-only PlaceObject3
bindings or their external symbol owners. A focused placement report listing
sprite, depth, class, instance name and owning library would avoid raw tag
inspection for future conversions.

```powershell
uv run --no-sync python -m bacup_lib.challenge_ui extracted/fo76 mods/SeventySix/data
uv run --no-sync python bacup/py_bacup_lib/python/bacup_lib/resources/challenges/build_preview.py extracted/fo76 tmp/challenges-preview
uv run --no-sync python -m http.server 8879 --bind 127.0.0.1
```

Open `/tmp/challenges-preview/preview.html` on that server. The fixture augments
the compiled production movie and displays its assertions below the player. It
uses the existing local Ruffle installation at
`tmp/currency-layout/ruffle/package/`. No browser runtime or source game asset is
shipped in Tales. The fixture leaves the movie interactive for real mouse and
keyboard checks.

The preview checks daily/weekly/lifetime entries, completed filtering, caps/XP
and optional pack rewards, track/close callbacks, parent children and selected
child preservation across refresh. The native menu receives controller events;
browser checks do not prove Fallout 4 controller, focus, cursor or save/load
behavior. The matching movie and DLL still require in-game acceptance.
