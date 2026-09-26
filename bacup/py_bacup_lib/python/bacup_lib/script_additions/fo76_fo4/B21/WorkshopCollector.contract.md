# B21:WorkshopCollector

FO76 C.A.M.P. collectors (beehives, extractors, Amm-O-Matics, food/drink makers)
are `CONT` records carrying the keyword `WorkshopCollectorObject` and the
server-side `WorkshopCollectorScript`. That script's client `.pex` is fully
stripped — 221 bytes, no functions and no properties — and its VMAD binding
carries no properties either, so nothing on the FO4 side describes what a
collector makes or how fast.

The production data lives in FO76's `RESO` ("Resource") record, which has no FO4
representation and is therefore dropped during conversion (178 in the source, 0
in the output):

| `RESO` subrecord | Target | Meaning |
|---|---|---|
| `NAM1` | `AVIF` (`Type = Resource`) | resource identity, e.g. "Honey" |
| `NAM2` | `LVLI` | what the collector produces |
| `NAM4` | `GLOB` | production interval, in hours |

The `attach_fo76_camp_collectors` fixup rebuilds that join before the record is
written: it indexes every source `RESO` by its resource `AVIF`, finds each
converted collector `CONT` by keyword, matches the `PRPS` row naming a resource
`AVIF`, and attaches this script with `Produce` and `IntervalHours` bound to the
mapped `LVLI` and `GLOB`. Collectors whose resource AV has no `RESO`, or whose
`LVLI`/`GLOB` did not survive conversion, are left untouched and counted in the
fixup's diagnostics rather than bound to a half-filled script.

## Behavior

Production is accrued from elapsed game time, so a collector the player has not
visited still fills while its cell is unloaded — the FO76 symptom this repairs
is a container that is still empty after a day. While loaded, a game-time timer
also accrues every interval.

- `OnInit` stamps `fLastProduced`, so a freshly built collector does not
  immediately dump a full load.
- `OnLoad`, `OnActivate`, the production timer and the power events evaluate
  `Accrue()`.
- `Accrue()` grants one `AddItem(Produce, ...)` roll per whole elapsed interval
  while `GetItemCount(None)` is under `MaxStoredItems`. Whole intervals are
  consumed even while at capacity or unpowered, so an emptied or re-powered
  collector does not burst-fill from time it could not produce.
- **Power.** A collector carrying `WorkshopCanBePowered` (`Fallout4.esm:03037E`
  — every FO76 extractor; beehives and other passive collectors do not) only
  produces while powered. Power is only observable while loaded, so `bPowered`
  is refreshed on `OnLoad`, `OnActivate`, the timer and
  `OnPowerOn`/`OnPowerOff`, and unloaded time is credited according to the last
  state seen. The converter keeps the FO76 `PowerRequired` (`000330`) `PRPS`
  row so extractors actually draw power.

## Capacity

FO76 stores no per-collector cap on the resource: every resource `AVIF` ships
`DURL = "No Limit"`. The cap is the collector's `PRPS` `CarryWeight` read as
pounds of stored items, which matches every published fill time (Oil 0.5 lb of
0.1 lb oil = 5 items × 3 min = 15 min; Acid 10 items = 30 min; Steel 20 × 1.8
min = 36 min; Concrete/Wood 40 × 1.8 min = 72 min).

Papyrus without F4SE cannot read inventory weight, so the fixup converts that
weight into an item count — `CarryWeight / average weight of the Produce
list's items` — and binds it as `MaxStoredItems`. When the weight inputs are
missing the property is left unbound and the script default of 50 applies.
