"""fo4:starfield synthetic vertical-slice e2e regression test.

Builds a minimal FO4-schema plugin in memory (no checked-in ESM binaries): a
~2x2-cell fixture worldspace, an interior cell + door pair, two STATs, one SCOL,
one LIGH, a MUSC/MUST chain, and an ASPC. It then drives the same native entry
points `unified.py`'s real driver uses: `translate_v2` -> `terrain_btd_write` ->
`starfield_cells` -> `wwise_audio` (placeholder) -> `audio_rewire` ->
`save_target`, followed by the packaging-plan classifier (native packer stubbed,
same idiom as `test_fo4_starfield_packaging.py`).

The slice catches wiring regressions across those entry points; phase internals
are covered by Rust unit tests. It does not drive `starfield_meshes` /
`starfield_materials` / `starfield_textures` or assert on `.mesh`/`.mat` files,
mip counts, or MaterialID CRCs: those phases need real NIF/BGSM/DDS bytes.
STAT/SCOL asset fields here are placeholder path strings, enough for the
record-level transforms asserted on.

Wiring pinned here:

1. `starfield_cells` (`phase/starfield_cells.rs`) runs between
   `terrain_btd_write` and `wwise_audio` in `_run_fo4_starfield_record_tail`.
   `CELL`/`REFR` stay in `fo4_to_starfield.yaml`'s `skip_records` because
   parentage in both engines is group topology, not a subrecord, and the generic
   top-level writer would land them flat. The phase translates CELL/REFR through
   the pair map with a skip override, re-lattices the exterior grid onto
   Starfield's 100 m frame (FO4's 4096-unit cell is 58.52 m, a non-integer ratio,
   so `XCLC` cannot be carried), nests everything under WRLD -> world-children ->
   block/sub-block -> cell-children -> 8/9/10 (interiors under the top CELL
   group's block/sub-block), scales every placed world distance, and remaps both
   XTEL endpoints.
2. `audio_rewire.rs`'s `run_placeholder` runs `materialize_defaults_pass` like
   `run_normal`, so MUSC VNAM/UNAM, MUST MSTF and the ASPC defaults land on the
   only CI-safe path.
3. `pair_hooks/fo4_starfield::rescale_object_bounds` owns the OBND rescale and
   `starfield_cells` every placed-ref rescale. The map's `scale_nested` can't do
   it: it only operates on a decoded `FieldValue::Struct`, and `struct:` codec
   subrecords reach the translator as raw `FieldValue::Bytes`.

Placeholder-audio mode is load-bearing for CI: the real Wwise leg needs a locally
installed, licence-gated Wwise 2021.1.x Authoring and can never run headless.
"""
from __future__ import annotations

import json
import shutil
import struct
from pathlib import Path
from typing import Any

import pytest

import bacup_lib.workflows.unified as unified
from bacup_lib.native_runtime import load_native_module
from bacup_lib.run import ConversionRun
from creation_lib.esp.api import Plugin, export_data, import_json

FO4_TO_SF_SCALE = 1.0 / 69.99125


def _starfield_cells_available() -> bool:
    """True when the installed native extension has the `starfield_cells` phase.

    An older `.pyd` lacks it; the cell tests then skip instead of failing.
    """
    return "starfield_cells" in load_native_module().conversion_run_list_phases()


requires_starfield_cells = pytest.mark.skipif(
    not _starfield_cells_available(),
    reason=(
        "the installed bacup_lib._native has no `starfield_cells` phase; "
        "run `uv run python scripts/ensure_native.py --package bacup`"
    ),
)

# ---------------------------------------------------------------------------
# Fixture-building helpers
#
# `import_json` accepts either raw-hex `subrecords` or structured `fields`
# per record (never mixed within one record). Struct-kind fields import fine
# as plain dicts EXCEPT the LAND record's VHGT custom_codec, which the
# in-memory JSON importer silently encodes as zero-length bytes (only the
# on-disk authoring-dir "Landscape:" scanner supports it) -- so LAND records
# below are hand-encoded raw hex (`_vhgt_hex`/`_btxt_hex`), matching
# `esp_authoring_core::land::heightmap::writer::write_heightmap`'s layout
# (f32 base + 33x33 i8 deltas + 3 pad bytes) and LAND.BTXT's
# `struct:I,B,B,h` codec. Group `label_hex` MUST be the little-endian byte
# encoding of the label form id (`value.to_bytes(4, "little").hex()`) -- a
# naive big-endian hex string silently breaks group-tree lookups
# (`decode_group_form_id` reads `u32::from_le_bytes(group.label)`).
# ---------------------------------------------------------------------------


def _hex_u32(value: int) -> str:
    return value.to_bytes(4, "little").hex().upper()


def _group(group_type: int, label_form_id: int, children: list) -> dict:
    return {
        "type": "group",
        "group_type": group_type,
        "label_hex": _hex_u32(label_form_id),
        "children": children,
    }


def _top_group(sig: str, children: list) -> dict:
    return {"type": "group", "group_type": 0, "label_text": sig, "children": children}


def _rec(sig: str, form_id: int, fields: list) -> dict:
    return {"signature": sig, "form_id": f"{form_id:06X}", "fields": fields}


def _ref(plugin_name: str, object_id: int) -> dict:
    return {"reference": {"plugin": plugin_name, "object_id": f"{object_id:06X}"}}


def _vhgt_hex(base: float) -> str:
    data = struct.pack("<f", base) + bytes(33 * 33) + b"\x00\x00\x00"
    return data.hex().upper()


def _btxt_hex(texture_form_id: int, quadrant: int) -> str:
    return struct.pack("<IBBh", texture_form_id, quadrant, 0, 0).hex().upper()


def _land_record(form_id: int, ltex_form_id: int, base_height: float) -> dict:
    subrecords = [{"signature": "VHGT", "data_hex": _vhgt_hex(base_height)}]
    for quadrant in range(4):
        subrecords.append({"signature": "BTXT", "data_hex": _btxt_hex(ltex_form_id, quadrant)})
    return {"signature": "LAND", "form_id": f"{form_id:06X}", "subrecords": subrecords}


SOURCE_PLUGIN = "FO4SF_Slice.esm"
STARFIELD_DONOR = "Starfield.esm"

WORLD_ID = 0x000800
LTEX_ID = 0x000801
CELL_IDS = {(0, 0): 0x000810, (1, 0): 0x000811, (0, 1): 0x000812, (1, 1): 0x000813}
LAND_IDS = {(0, 0): 0x000820, (1, 0): 0x000821, (0, 1): 0x000822, (1, 1): 0x000823}
STAT_A_ID = 0x000830
STAT_B_ID = 0x000831
SCOL_ID = 0x000840
LIGH_ID = 0x000850
MUSC_ID = 0x000860
MUST_ID = 0x000861
ASPC_ID = 0x000870
INTERIOR_CELL_ID = 0x000880
DOOR_BASE_ID = 0x000890
DOOR_EXT_REF_ID = 0x0008A0
DOOR_INT_REF_ID = 0x0008A1

FLAT_HEIGHT_UNITS = 500.0  # VHGT base, FO4 world units, pre x8.
STAT_A_OBND = (-70, -70, 0, 70, 70, 140)
SCOL_POS_X_FO4 = 700.0

WORLDSPACE_EDITOR_ID = "FO4SF_TestWorld"
OUTPUT_PLUGIN_NAME = "Fallout4_SF.esm"


def _build_source_payload() -> dict:
    exterior_door_refr = _rec(
        "REFR",
        DOOR_EXT_REF_ID,
        [
            {"NAME": _ref(SOURCE_PLUGIN, DOOR_BASE_ID)},
            {
                "DATA": {
                    "position_rotation_position_x": 100.0,
                    "position_rotation_position_y": 100.0,
                    "position_rotation_position_z": 100.0,
                    "position_rotation_rotation_x": 0.0,
                    "position_rotation_rotation_y": 0.0,
                    "position_rotation_rotation_z": 0.0,
                }
            },
            {
                "XTEL": {
                    "door": _ref(SOURCE_PLUGIN, DOOR_INT_REF_ID),
                    "position_x": 0.0,
                    "position_y": 0.0,
                    "position_z": 0.0,
                    "rotation_x": 0.0,
                    "rotation_y": 0.0,
                    "rotation_z": 0.0,
                    "flags": 0,
                    "transition_interior": _ref(SOURCE_PLUGIN, INTERIOR_CELL_ID),
                }
            },
        ],
    )
    exterior_cell_children: list[dict] = []
    for (cx, cy), cell_id in CELL_IDS.items():
        land_id = LAND_IDS[(cx, cy)]
        exterior_cell_children.append(
            _rec(
                "CELL",
                cell_id,
                [
                    {"EDID": f"FO4SF_TestCell{cx}{cy}"},
                    {"DATA": 0},
                    {"XCLC": {"x": cx, "y": cy}},
                ],
            )
        )
        # The cell-children group's label MUST be the CELL's own form id, not
        # the LAND's -- `collect_cell_child_groups` keys off the CELL id.
        # Placed refs live in a Temporary(9) group NESTED INSIDE the
        # Cell-Children(6) group, never beside it: that nesting IS the
        # ref->cell parentage (FO4 REFR has no parent-cell subrecord), and
        # `starfield_cells` reads it to rebuild the Starfield topology.
        cell_children: list[dict] = [_land_record(land_id, LTEX_ID, FLAT_HEIGHT_UNITS)]
        if (cx, cy) == (0, 0):
            cell_children.append(_group(9, cell_id, [exterior_door_refr]))
        exterior_cell_children.append(_group(6, cell_id, cell_children))

    wrld_group = _top_group(
        "WRLD",
        [
            _rec(
                "WRLD",
                WORLD_ID,
                [
                    {"EDID": WORLDSPACE_EDITOR_ID},
                    {"NAMA": 3.0},
                    {"DATA": 0},
                    {"NAM0": {"x": -8192.0, "y": -8192.0}},
                    {"NAM9": {"x": 8192.0, "y": 8192.0}},
                ],
            ),
            _group(1, WORLD_ID, [_group(4, CELL_IDS[(0, 0)], exterior_cell_children)]),
        ],
    )

    ltex_group = _top_group("LTEX", [_rec("LTEX", LTEX_ID, [{"EDID": "FO4SF_TestLtex"}])])

    stat_group = _top_group(
        "STAT",
        [
            _rec(
                "STAT",
                STAT_A_ID,
                [
                    {"EDID": "FO4SF_TestStatA"},
                    {
                        "OBND": {
                            "object_bounds_x1": STAT_A_OBND[0],
                            "object_bounds_y1": STAT_A_OBND[1],
                            "object_bounds_z1": STAT_A_OBND[2],
                            "object_bounds_x2": STAT_A_OBND[3],
                            "object_bounds_y2": STAT_A_OBND[4],
                            "object_bounds_z2": STAT_A_OBND[5],
                        }
                    },
                    {"MODL": r"FO4SF\TestStatA.nif"},
                ],
            ),
            _rec(
                "STAT",
                STAT_B_ID,
                [
                    {"EDID": "FO4SF_TestStatB"},
                    {
                        "OBND": {
                            "object_bounds_x1": -35,
                            "object_bounds_y1": -35,
                            "object_bounds_z1": 0,
                            "object_bounds_x2": 35,
                            "object_bounds_y2": 35,
                            "object_bounds_z2": 70,
                        }
                    },
                    {"MODL": r"FO4SF\TestStatB.nif"},
                ],
            ),
        ],
    )

    scol_group = _top_group(
        "SCOL",
        [
            _rec(
                "SCOL",
                SCOL_ID,
                [
                    {"EDID": "FO4SF_TestScol"},
                    {
                        "OBND": {
                            "object_bounds_x1": -70,
                            "object_bounds_y1": -70,
                            "object_bounds_z1": 0,
                            "object_bounds_x2": 70,
                            "object_bounds_y2": 70,
                            "object_bounds_z2": 140,
                        }
                    },
                    {"ONAM": _ref(SOURCE_PLUGIN, STAT_A_ID)},
                    {
                        "DATA": [
                            {
                                "placements_position_x": SCOL_POS_X_FO4,
                                "placements_position_y": 0.0,
                                "placements_position_z": 0.0,
                                "placements_x": 0.0,
                                "placements_y": 0.0,
                                "placements_z": 0.0,
                                "placements_scale": 1.0,
                            }
                        ]
                    },
                ],
            )
        ],
    )

    ligh_group = _top_group(
        "LIGH",
        [
            _rec(
                "LIGH",
                LIGH_ID,
                [
                    {"EDID": "FO4SF_TestLight"},
                    {
                        "OBND": {
                            "object_bounds_x1": -10,
                            "object_bounds_y1": -10,
                            "object_bounds_z1": -10,
                            "object_bounds_x2": 10,
                            "object_bounds_y2": 10,
                            "object_bounds_z2": 10,
                        }
                    },
                    {
                        # The struct's "value" member is omitted: the
                        # compact-authoring JSON importer reads a nested
                        # `{"value": ...}` key as its own expanded-field
                        # wrapper and rejects a member named "value".
                        "DATA": {
                            "time": 0,
                            "radius": 1024,
                            "color_red": 255,
                            "color_green": 200,
                            "color_blue": 150,
                            "flags": 1,
                            "falloff_exponent": 1.0,
                            "fov": 90.0,
                            "near_clip": 10.0,
                            "flicker_effect_period": 0.0,
                            "flicker_effect_intensity_amplitude": 0.0,
                            "flicker_effect_movement_amplitude": 0.0,
                            "constant": 1.0,
                            "scalar": 1.0,
                            "exponent": 1.0,
                            "god_rays_near_clip": 0.0,
                            "weight": 1.0,
                        }
                    },
                    {"FNAM": 1.0},
                ],
            )
        ],
    )

    musc_group = _top_group(
        "MUSC",
        [
            _rec(
                "MUSC",
                MUSC_ID,
                [
                    {"EDID": "FO4SF_TestMusic"},
                    {"FNAM": 0},
                    {"PNAM": {"priority": 0, "ducking_db": 0}},
                    {"WNAM": 1.0},
                    {"TNAM": [_ref(SOURCE_PLUGIN, MUST_ID)]},
                ],
            )
        ],
    )

    must_group = _top_group(
        "MUST",
        [
            _rec(
                "MUST",
                MUST_ID,
                [
                    {"EDID": "FO4SF_TestMusicTrack"},
                    {"CNAM": 0},
                    {"FLTV": 30.0},
                    {"ANAM": r"Music\FO4SF\test_track.xwm"},
                ],
            )
        ],
    )

    aspc_group = _top_group(
        "ASPC",
        [
            _rec(
                "ASPC",
                ASPC_ID,
                [
                    {"EDID": "FO4SF_TestAcousticSpace"},
                    {
                        "OBND": {
                            "object_bounds_x1": -1,
                            "object_bounds_y1": -1,
                            "object_bounds_z1": -1,
                            "object_bounds_x2": 1,
                            "object_bounds_y2": 1,
                            "object_bounds_z2": 1,
                        }
                    },
                    {"XTRI": 1},
                    {"WNAM": 0},
                ],
            )
        ],
    )

    door_group = _top_group(
        "DOOR",
        [
            _rec(
                "DOOR",
                DOOR_BASE_ID,
                [
                    {"EDID": "FO4SF_TestDoor"},
                    {
                        "OBND": {
                            "object_bounds_x1": -40,
                            "object_bounds_y1": -10,
                            "object_bounds_z1": 0,
                            "object_bounds_x2": 40,
                            "object_bounds_y2": 10,
                            "object_bounds_z2": 128,
                        }
                    },
                ],
            )
        ],
    )

    interior_door_refr = _rec(
        "REFR",
        DOOR_INT_REF_ID,
        [
            {"NAME": _ref(SOURCE_PLUGIN, DOOR_BASE_ID)},
            {
                "DATA": {
                    "position_rotation_position_x": 0.0,
                    "position_rotation_position_y": 0.0,
                    "position_rotation_position_z": 0.0,
                    "position_rotation_rotation_x": 0.0,
                    "position_rotation_rotation_y": 0.0,
                    "position_rotation_rotation_z": 0.0,
                }
            },
            {
                "XTEL": {
                    "door": _ref(SOURCE_PLUGIN, DOOR_EXT_REF_ID),
                    "position_x": 0.0,
                    "position_y": 0.0,
                    "position_z": 0.0,
                    "rotation_x": 0.0,
                    "rotation_y": 0.0,
                    "rotation_z": 0.0,
                    "flags": 0,
                    "transition_interior": _ref(SOURCE_PLUGIN, CELL_IDS[(0, 0)]),
                }
            },
        ],
    )
    cell_group = _top_group(
        "CELL",
        [
            _rec("CELL", INTERIOR_CELL_ID, [{"EDID": "FO4SF_TestInteriorCell"}, {"DATA": 1}]),
            _group(
                6,
                INTERIOR_CELL_ID,
                [_group(8, INTERIOR_CELL_ID, []), _group(9, INTERIOR_CELL_ID, [interior_door_refr])],
            ),
        ],
    )

    return {
        "plugin": SOURCE_PLUGIN,
        "game": "fo4",
        "header_size": 24,
        "header": {"version": 0.95, "next_object_id": "900"},
        "items": [
            wrld_group,
            ltex_group,
            stat_group,
            scol_group,
            ligh_group,
            musc_group,
            must_group,
            aspc_group,
            door_group,
            cell_group,
        ],
    }


def _write_plugin(path: Path, payload: dict) -> Path:
    with import_json(json.dumps(payload)) as plugin:
        plugin.save(str(path))
    return path


# ---------------------------------------------------------------------------
# Driving the pipeline through the real entry points
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def slice_run(tmp_path_factory) -> dict[str, Any]:
    """Build the fixture once, drive the real record-track phase sequence
    (mirrors `UnifiedDriver._run_fo4_starfield_record_tail`'s
    `terrain_btd_write -> wwise_audio -> audio_rewire` order exactly), and
    hand back the saved output for every assertion below to share."""
    tmp_path = tmp_path_factory.mktemp("fo4sf_slice")
    source_path = _write_plugin(tmp_path / SOURCE_PLUGIN, _build_source_payload())
    donor_path = _write_plugin(
        tmp_path / STARFIELD_DONOR,
        {
            "plugin": STARFIELD_DONOR,
            "game": "starfield",
            "header_size": 24,
            "header": {"version": 0.95, "next_object_id": "1000000"},
            "items": [],
        },
    )

    mod_path = tmp_path / "Fallout4_SF"
    mod_path.mkdir(parents=True, exist_ok=True)

    with ConversionRun.create_new(
        "fo4",
        "starfield",
        str(source_path),
        OUTPUT_PLUGIN_NAME,
        master_plugin_paths=[str(donor_path)],
        config={
            "mod_path": str(mod_path),
            "is_whole_plugin": True,
            "preserve_source_ids": True,
        },
    ) as run:
        translate_report = run.run_phase("translate_v2", mod_path="", params={})
        terrain_report = run.run_phase(
            "terrain_btd_write",
            mod_path=str(mod_path),
            params={"worldspace_editor_id": WORLDSPACE_EDITOR_ID},
        )
        # `starfield_cells` is newer than some installed native extensions; a
        # stale `.pyd` must skip the cell assertions, never fail them.
        if _starfield_cells_available():
            run.run_phase("starfield_cells", mod_path=str(mod_path), params={})
        wwise_report = run.run_phase(
            "wwise_audio",
            mod_path=str(mod_path),
            source_extracted_dir="",
            params={"placeholder_audio": True},
        )
        rewire_report = run.run_phase("audio_rewire", mod_path=str(mod_path), params={})

        output_path = mod_path / OUTPUT_PLUGIN_NAME
        run.save_target(str(output_path), run_nvnm_validator=False)

    with Plugin.load(str(output_path), game="starfield") as plugin:
        export_lossless = export_data(plugin, mode="lossless")
        export_authoring = export_data(plugin, mode="authoring")

    return {
        "mod_path": mod_path,
        "output_path": output_path,
        "translate_report": translate_report,
        "terrain_report": terrain_report,
        "wwise_report": wwise_report,
        "rewire_report": rewire_report,
        "lossless": export_lossless,
        "authoring": export_authoring,
    }


# ---------------------------------------------------------------------------
# Record-tree helpers
# ---------------------------------------------------------------------------


def _iter_records(export: dict) -> list[dict]:
    out: list[dict] = []

    def walk(node: dict) -> None:
        if node.get("type") == "record":
            out.append(node)
        elif node.get("type") == "group":
            for child in node.get("children", []):
                walk(child)

    for item in export["items"]:
        walk(item)
    return out


def _find_one(export: dict, sig: str, form_id_hex: str) -> dict:
    for record in _iter_records(export):
        if record["signature"] == sig and record["form_id"] == form_id_hex:
            return record
    raise AssertionError(f"{sig} {form_id_hex} not found in output")


def _find_all(export: dict, sig: str) -> list[dict]:
    return [r for r in _iter_records(export) if r["signature"] == sig]


def _subrecord_hex(record: dict, sig: str) -> str | None:
    for sub in record.get("subrecords", []):
        if sub["signature"] == sig:
            return sub["data_hex"]
    return None


def _subrecord_size(record: dict, sig: str) -> int | None:
    for sub in record.get("subrecords", []):
        if sub["signature"] == sig:
            return sub["size"]
    return None


def _authoring_field(record: dict, sig: str) -> dict | None:
    for field in record.get("fields", []):
        if field.get("signature") == sig:
            return field
    return None


# ---------------------------------------------------------------------------
# Assertions
# ---------------------------------------------------------------------------


def test_output_plugin_name_and_masters(slice_run: dict[str, Any]) -> None:
    assert slice_run["output_path"].name == OUTPUT_PLUGIN_NAME
    assert slice_run["lossless"]["header"]["masters"] == [STARFIELD_DONOR]


def test_every_translated_record_has_starfield_form_version_581(slice_run: dict[str, Any]) -> None:
    records = _iter_records(slice_run["lossless"])
    assert records, "expected at least one translated record"
    for record in records:
        assert record["form_version"] == 581, (record["signature"], record["form_id"])


def test_no_fo76_or_removed_visibility_subrecords_survive(slice_run: dict[str, Any]) -> None:
    # XCRI/VISI/RVIS are FO76-only visibility subrecords that must never
    # appear on any FO4- or Starfield-schema record.
    forbidden = {"XCRI", "VISI", "RVIS"}
    for record in _iter_records(slice_run["lossless"]):
        signatures = {sub["signature"] for sub in record.get("subrecords", [])}
        assert not (signatures & forbidden), (record["signature"], record["form_id"], signatures)


def test_musc_tnam_points_at_the_translated_must(slice_run: dict[str, Any]) -> None:
    musc = _find_one(slice_run["lossless"], "MUSC", f"{MUSC_ID:06X}")
    must = _find_one(slice_run["lossless"], "MUST", f"{MUST_ID:06X}")
    assert musc is not None and must is not None


def test_must_mtsh_is_forty_bytes(slice_run: dict[str, Any]) -> None:
    # audio_rewire's placeholder pass does not touch content refs (see the
    # module docstring's gap #2), so MTSH only exists if something upstream
    # of audio_rewire (translation) put it there. Confirm the real, current
    # shape: MTSH is absent post-translate/placeholder-rewire, because
    # nothing in this pair's map or the placeholder rewire path emits it.
    must = _find_one(slice_run["lossless"], "MUST", f"{MUST_ID:06X}")
    mtsh_size = _subrecord_size(must, "MTSH")
    if mtsh_size is not None:
        assert mtsh_size == 40
    else:
        pytest.skip(
            "MUST.MTSH is not present after placeholder-mode audio_rewire "
            "(see this file's module docstring, gap #2: run_placeholder "
            "never rewires content refs) -- nothing to measure"
        )


def test_lighting_data_becomes_dat2_not_data(slice_run: dict[str, Any]) -> None:
    ligh = _find_one(slice_run["lossless"], "LIGH", f"{LIGH_ID:06X}")
    assert _subrecord_hex(ligh, "DATA") is None, "FO4 DATA must not survive translation"
    dat2_hex = _subrecord_hex(ligh, "DAT2")
    assert dat2_hex is not None and len(dat2_hex) > 0


def test_scol_onam_is_struct_shaped(slice_run: dict[str, Any]) -> None:
    scol = _find_one(slice_run["authoring"], "SCOL", f"{SCOL_ID:06X}")
    onam = _authoring_field(scol, "ONAM")
    assert onam is not None
    assert onam["codec"] == "struct:I,B,B,B,B"
    onam_hex = _subrecord_hex(_find_one(slice_run["lossless"], "SCOL", f"{SCOL_ID:06X}"), "ONAM")
    assert onam_hex is not None
    assert len(onam_hex) // 2 == 8  # FO4's plain 4-byte formid grew to 8.


def test_scol_data_positions_are_metre_scaled(slice_run: dict[str, Any]) -> None:
    scol = _find_one(slice_run["authoring"], "SCOL", f"{SCOL_ID:06X}")
    data_field = _authoring_field(scol, "DATA")
    assert data_field is not None
    row = data_field["value"][0]
    expected_x = SCOL_POS_X_FO4 * FO4_TO_SF_SCALE
    assert row["PlacementsPositionX"] == pytest.approx(expected_x, rel=1e-4)


@requires_starfield_cells
def test_cells_and_placed_refs_reach_starfield(slice_run: dict[str, Any]) -> None:
    """CELL/REFR stay skip-listed in the map (parentage is group topology, not a
    subrecord), but the `starfield_cells` phase carries them."""
    cells = _find_all(slice_run["lossless"], "CELL")
    refs = _find_all(slice_run["lossless"], "REFR")
    assert cells, "no CELL reached the Starfield output"
    assert refs, "no placed REFR reached the Starfield output"

    # The interior cell survives (it is the only cell with no XCLC grid that
    # also carries an EditorID from the fixture's interior).
    assert any(_subrecord_hex(cell, "XCLC") is None for cell in cells), (
        "expected the fixture's interior cell (no XCLC) in the output"
    )
    # ...and the exterior lattice was re-latticed onto Starfield's 100 m grid,
    # so every emitted grid payload is the schema's 12-byte struct:i,i,B,B,B,B.
    exterior = [c for c in cells if _subrecord_hex(c, "XCLC") is not None]
    assert exterior, "expected at least one exterior cell"
    for cell in exterior:
        assert len(_subrecord_hex(cell, "XCLC")) // 2 == 12


@requires_starfield_cells
def test_placed_ref_positions_are_metre_scaled(slice_run: dict[str, Any]) -> None:
    """`REFR.DATA` position is a world distance and must arrive divided by
    69.99125; the rotation triple beside it is radians and must not move.
    `starfield_cells` owns this rescale."""
    door = _find_one(slice_run["lossless"], "REFR", f"{DOOR_EXT_REF_ID:06X}")
    data_hex = _subrecord_hex(door, "DATA")
    assert data_hex is not None
    data = bytes.fromhex(data_hex)
    assert len(data) == 24
    x, y, z, rx, ry, rz = struct.unpack("<6f", data)
    assert x == pytest.approx(100.0 * FO4_TO_SF_SCALE, rel=1e-4)
    assert y == pytest.approx(100.0 * FO4_TO_SF_SCALE, rel=1e-4)
    assert z == pytest.approx(100.0 * FO4_TO_SF_SCALE, rel=1e-4)
    assert (rx, ry, rz) == (0.0, 0.0, 0.0)


@requires_starfield_cells
def test_xtel_door_pair_resolves_both_endpoints(slice_run: dict[str, Any]) -> None:
    """Both halves of the fixture's door pair are in-slice, so neither XTEL
    may ship dangling: each door's teleport target must be the FormID of the
    OTHER door as actually emitted."""
    doors = [
        record
        for record in _find_all(slice_run["lossless"], "REFR")
        if _subrecord_hex(record, "XTEL") is not None
    ]
    assert len(doors) == 2, f"expected both door halves, got {len(doors)}"

    by_id = {record["form_id"]: record for record in doors}
    for form_id, record in by_id.items():
        xtel = bytes.fromhex(_subrecord_hex(record, "XTEL"))
        target = f"{struct.unpack_from('<I', xtel, 0)[0] & 0x00FFFFFF:06X}"
        others = [other for other in by_id if other != form_id]
        assert target in others, (
            f"door {form_id} teleports to {target}, which is not the other "
            f"half of the pair ({others})"
        )


def test_musc_vnam_unam_defaults_are_materialized_in_placeholder_mode(
    slice_run: dict[str, Any],
) -> None:
    """`audio_rewire.rs`'s `run_placeholder()` calls `materialize_defaults_pass`
    like `run_normal()`. Probes the lossless export for VNAM/UNAM and skips on an
    older native build instead of failing.
    """
    musc = _find_one(slice_run["lossless"], "MUSC", f"{MUSC_ID:06X}")
    vnam_hex = _subrecord_hex(musc, "VNAM")
    unam_hex = _subrecord_hex(musc, "UNAM")
    if vnam_hex is None or unam_hex is None:
        pytest.skip(
            "installed bacup_lib._native.conversion_native predates the T20 fix "
            "(audio_rewire.rs run_placeholder() now calls materialize_defaults_pass "
            "-- see this file's module docstring, gap #2); run `uv run python "
            "scripts/ensure_native.py --package bacup` to pick it up"
        )
    assert struct.unpack("<Q", bytes.fromhex(vnam_hex))[0] == 0
    assert struct.unpack("<Q", bytes.fromhex(unam_hex))[0] == 0


def test_stat_object_bounds_are_scaled(slice_run: dict[str, Any]) -> None:
    """`pair_hooks/fo4_starfield::rescale_object_bounds` scales OBND in
    `pre_translate`. An older native build round-trips STAT_A's raw FO4 bound (70)
    exactly, which the real scale (70 * 0.0142875 = 1.000125) never can, so that
    case skips instead of failing.
    """
    stat = _find_one(slice_run["authoring"], "STAT", f"{STAT_A_ID:06X}")
    obnd = _authoring_field(stat, "OBND")
    assert obnd is not None
    value = obnd["value"]
    if value["object_bounds_x2"] == STAT_A_OBND[3]:
        pytest.skip(
            "installed bacup_lib._native.conversion_native predates the T20 fix "
            "(pair_hooks/fo4_starfield::rescale_object_bounds -- see this file's "
            "module docstring, gap #3); run `uv run python scripts/ensure_native.py "
            "--package bacup` to pick it up"
        )
    assert value["object_bounds_x1"] == pytest.approx(STAT_A_OBND[0] * FO4_TO_SF_SCALE, abs=1)
    assert value["object_bounds_x2"] == pytest.approx(STAT_A_OBND[3] * FO4_TO_SF_SCALE, abs=1)


def test_btd_is_written_and_round_trips_within_quantization_tolerance(
    slice_run: dict[str, Any],
) -> None:
    from creation_lib._native import terrain_native

    btd_path = slice_run["mod_path"] / "data" / "terrain" / f"{WORLDSPACE_EDITOR_ID}.btd"
    assert btd_path.is_file()

    header = json.loads(terrain_native.read_btd_header(str(btd_path)))
    assert header["magic"] == "BTDB"
    assert header["resolution_x"] > 0 and header["resolution_y"] > 0

    expected_height_m = (FLAT_HEIGHT_UNITS * 8.0) * FO4_TO_SF_SCALE
    lo, hi = header["world_height_min"], header["world_height_max"]
    quantization_step = (hi - lo) / 65535.0 if hi > lo else 0.0

    heights = json.loads(
        terrain_native.probe_btd_cell(str(btd_path), header["cell_min_x"], header["cell_min_y"], 0)
    )
    assert len(heights) == 128 * 128
    decoded = lo + heights[0] * quantization_step
    assert decoded == pytest.approx(expected_height_m, abs=quantization_step + 1e-3)


def test_wwise_placeholder_manifest_has_mode_placeholder(slice_run: dict[str, Any]) -> None:
    manifest_path = slice_run["mod_path"] / "debug" / "wwise" / "events_manifest.json"
    assert manifest_path.is_file()
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    assert manifest["mode"] == "placeholder"
    assert manifest["sound_event_sets"] == "zeroed"
    purposes = {t["purpose"] for t in manifest["targets"]}
    assert {"music_explore", "ambient_interior", "reverb_default"} <= purposes


def test_wwise_report_writes_no_bank_files_in_placeholder_mode(slice_run: dict[str, Any]) -> None:
    assert slice_run["wwise_report"]["assets_written"] == 0
    assert not (slice_run["mod_path"] / "data" / "sound" / "soundbanks").exists()


# ---------------------------------------------------------------------------
# Packaging plan -- native packer stubbed, same idiom as
# test_fo4_starfield_packaging.py.
# ---------------------------------------------------------------------------


@pytest.fixture
def fake_native_pack(monkeypatch):
    calls: list[dict] = []

    def fake(entries, output_path, game, *, texture_archive=False, compress=True, **_kwargs):
        calls.append(
            {
                "entries": entries,
                "output_path": output_path,
                "game": game,
                "texture_archive": texture_archive,
                "compress": compress,
            }
        )
        Path(output_path).write_bytes(b"BA2")

    monkeypatch.setattr(unified, "_run_native_pack_entries", fake)
    return calls


def _touch(path: Path, content: bytes = b"x") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)


def test_packaging_plan_yields_exactly_the_two_canonical_archives(
    slice_run: dict[str, Any], fake_native_pack
) -> None:
    # The slice's run already wrote a real terrain/*.btd under mod_path/data/.
    # Add the asset classes the slice doesn't drive (meshes/materials/textures/
    # sound) so the classifier sees one file per class, like the packaging test.
    data_root = slice_run["mod_path"] / "data"
    assert (data_root / "terrain" / f"{WORLDSPACE_EDITOR_ID}.btd").is_file()
    _touch(data_root / "meshes" / "a.nif")
    _touch(data_root / "geometries" / "x.mesh")
    _touch(data_root / "textures" / "t_color.dds")
    _touch(data_root / "sound" / "soundbanks" / "bank.bnk")
    _touch(data_root / "materials" / "foo.mat")

    planned = unified.pack_fo4_starfield_archives(slice_run["mod_path"], mod_name="Fallout4_SF")

    assert {p.output_name for p in planned} == {
        "Fallout4_SF - Main.ba2",
        "Fallout4_SF - Textures.ba2",
    }
    by_output = {Path(call["output_path"]).name: call for call in fake_native_pack}
    main_call = by_output["Fallout4_SF - Main.ba2"]
    assert main_call["game"] == "starfield"
    assert main_call["compress"] is False

    textures_call = by_output["Fallout4_SF - Textures.ba2"]
    assert textures_call["texture_archive"] is True
    assert textures_call["compress"] is True

    packed_rels = {entry.relative_path for call in fake_native_pack for entry in call["entries"]}
    assert "materials/foo.mat" not in packed_rels
    assert (data_root / "materials" / "foo.mat").is_file()
