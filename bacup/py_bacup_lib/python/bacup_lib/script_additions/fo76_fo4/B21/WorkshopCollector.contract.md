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

Production is accrued lazily rather than driven by a timer, so a collector the
player has not visited still fills while its cell is unloaded — the FO76 symptom
this repairs is a container that is still empty after a day.

- `OnInit` stamps `fLastProduced`, so a freshly built collector does not
  immediately dump a full load.
- `OnLoad` and `OnActivate` evaluate `Accrue()`.
- `Accrue()` grants one `AddItem(Produce, ...)` roll per whole elapsed interval,
  clamped to the remaining room under `MaxStored`. Whole intervals are consumed
  even while at capacity, so an emptied collector does not immediately refill
  from time that passed while it was full.
- `OnItemRemoved` decrements the stored counter. The counter is tracked in the
  script rather than read back with `GetItemCount`, because `Produce` is a
  leveled list and not the concrete item that lands in the container.

## Assumptions

`MaxStored` is **not** derived from the source data and defaults to 10. FO76
stores no per-collector capacity in the client plugin: every resource `AVIF`
ships `DURL = "No Limit"` with ±FLT_MAX bounds, so the real cap was server-side.
The `PRPS` `CarryWeight` row is left alone — on an FO4 container that value is
native workshop-storage capacity, and there is no evidence it meant a collector
cap in FO76.

The produced count assumes one item per leveled-list roll, which holds for the
converted collector lists (single `LVLO` entry, count 1, flagged
`CalculateForEachItemInCount`). A list that yields more per roll would leave
`iStored` slightly under the true contents, which only makes the collector
generous rather than stuck.
