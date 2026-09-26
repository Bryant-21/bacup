"""Integration tests for native translation and canonical fixups.

translate_all reads every record from the source plugin and writes translated
records into the target plugin, returning a TranslateStats dict.

fixups_v2 runs registered post-translation fixups.

Requires fo4_minimal_weap.esm fixture (built by scripts/build_a2_fixture.py).
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.run import ConversionRun
from creation_lib.esp import Plugin
from creation_lib.esp.api import export_data, import_json
from creation_lib.esp.model import Record, Subrecord
from creation_lib.esp.native_runtime import (
    plugin_handle_close,
    plugin_handle_load,
    plugin_handle_record_subrecords,
)

# ---------------------------------------------------------------------------
# Fixture path — walk up to repo root
# ---------------------------------------------------------------------------


def _find_fixture() -> Path:
    here = Path(__file__).resolve()
    for parent in [here, *here.parents]:
        candidate = (
            parent
            / "bacup"
            / "py_bacup_lib"
            / "native"
            / "conversion"
            / "src"
            / "test_fixtures"
            / "fo4_minimal_weap.esm"
        )
        if candidate.exists():
            return candidate
    return (
        Path(__file__).resolve().parents[5]
        / "bacup"
        / "py_bacup_lib"
        / "native"
        / "conversion"
        / "src"
        / "test_fixtures"
        / "fo4_minimal_weap.esm"
    )


FIXTURE = _find_fixture()

pytestmark = pytest.mark.skipif(
    not FIXTURE.exists(),
    reason=f"fo4 fixture not present at {FIXTURE} — run scripts/build_a2_fixture.py",
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _native():
    from bacup_lib.native_runtime import load_native_module

    return load_native_module()


def _create_run(
    source_plugin_path: Path,
    *,
    source_game: str = "fo4",
    target_game: str = "fo4",
    target_plugin_name: str = "Output.esm",
    config: dict | None = None,
) -> ConversionRun:
    run_config = {"output_plugin_name": target_plugin_name, **(config or {})}
    return ConversionRun.create_new(
        source_game,
        target_game,
        str(source_plugin_path),
        target_plugin_name,
        config=run_config,
    )


def _write_plugin(
    path: Path,
    *,
    game: str,
    records: list[Record],
) -> Path:
    with Plugin.new(path.name, game=game) as plugin:
        for record in records:
            plugin.add_record(record)
        plugin.save(path)
    return path


def _write_imported_plugin(path: Path, source_json: str) -> Path:
    with import_json(source_json) as plugin:
        plugin.save(path)
    return path


def _run_fixups_v2(m, run_id: int) -> dict:
    return m.conversion_run_phase(run_id, "fixups_v2", {"mod_path": "", "params": {}})


def _save_target(
    run: ConversionRun,
    tmp_path: Path,
    target_plugin_name: str = "Output.esm",
) -> Path:
    target_path = tmp_path / target_plugin_name
    run.save_target(str(target_path), run_nvnm_validator=False)
    return target_path


def _group_count(
    run: ConversionRun,
    tmp_path: Path,
    signature: str,
    target_plugin_name: str = "Output.esm",
) -> int:
    target_path = _save_target(run, tmp_path, target_plugin_name)
    with Plugin.load(target_path, game="fo4") as plugin:
        return dict(plugin.group_signatures or []).get(signature, 0)


def _target_export(
    run: ConversionRun,
    tmp_path: Path,
    target_plugin_name: str,
) -> dict:
    target_path = _save_target(run, tmp_path, target_plugin_name)
    with Plugin.load(target_path, game="fo4") as plugin:
        return export_data(plugin)


def _fo76_dialogue_source_json(plugin_name: str) -> str:
    return f"""
{{
  "plugin": "{plugin_name}",
  "game": "fo76",
  "header": {{"version": 1.0, "next_object_id": "000803"}},
  "items": [
    {{
      "type": "group",
      "label_text": "QUST",
      "group_type": 0,
      "children": [
        {{
          "signature": "QUST",
          "form_id": "000800",
          "form_version": 257,
          "subrecords": [
            {{"signature": "EDID", "data_hex": "506172656E74517565737400"}}
          ]
        }}
      ]
    }},
    {{
      "type": "group",
      "label_text": "DIAL",
      "group_type": 0,
      "children": [
        {{
          "signature": "DIAL",
          "form_id": "000801",
          "form_version": 257,
          "subrecords": [
            {{"signature": "EDID", "data_hex": "506172656E74546F70696300"}},
            {{"signature": "PNAM", "data_hex": "0000803F"}},
            {{"signature": "QNAM", "data_hex": "00080000"}},
            {{"signature": "DATA", "data_hex": "00000000"}},
            {{"signature": "TIFC", "data_hex": "01000000"}},
            {{"signature": "INFO", "data_hex": "02080000"}}
          ]
        }},
        {{
          "type": "group",
          "label_hex": "01080000",
          "group_type": 7,
          "children": [
            {{
              "signature": "INFO",
              "form_id": "000802",
              "form_version": 257,
              "subrecords": [
                {{"signature": "EDID", "data_hex": "546F706963496E666F00"}}
              ]
            }}
          ]
        }}
      ]
    }}
  ]
}}
"""


def _fo76_combined_scene_dialogue_source_json(plugin_name: str) -> str:
    def text(value: str) -> str:
        return (value + "\0").encode().hex()

    quest = {
        "signature": "QUST",
        "form_id": "000800",
        "form_version": 257,
        "subrecords": [
            {"signature": "EDID", "data_hex": text("ParentQuest")},
        ],
    }
    player_dial = {
        "signature": "DIAL",
        "form_id": "000801",
        "form_version": 257,
        "subrecords": [
            {"signature": "EDID", "data_hex": text("PlayerTopic")},
            {"signature": "PNAM", "data_hex": "0000803F"},
            {"signature": "QNAM", "data_hex": "00080000"},
            {"signature": "DATA", "data_hex": "00000000"},
            {"signature": "TIFC", "data_hex": "01000000"},
            {"signature": "INFO", "data_hex": "03080000"},
        ],
    }
    npc_dial = {
        "signature": "DIAL",
        "form_id": "000802",
        "form_version": 257,
        "subrecords": [
            {"signature": "EDID", "data_hex": text("NpcTopic")},
            {"signature": "PNAM", "data_hex": "0000803F"},
            {"signature": "QNAM", "data_hex": "00080000"},
            {"signature": "DATA", "data_hex": "00000000"},
            {"signature": "TIFC", "data_hex": "00000000"},
        ],
    }
    combined_info = {
        "signature": "INFO",
        "form_id": "000803",
        "form_version": 257,
        "subrecords": [
            {"signature": "ENAM", "data_hex": "00000000"},
            {
                "signature": "TRDA",
                "data_hex": "000000000100000000000000FFFFFFFFFFFFFFFF",
            },
            {
                "signature": "NAM1",
                "data_hex": text("God damn it. *sigh*"),
            },
            {"signature": "NAM2", "data_hex": "00"},
            {"signature": "NAM3", "data_hex": "00"},
            {"signature": "NAM4", "data_hex": "00"},
            {
                "signature": "RNAM",
                "data_hex": text("The door's sealed tight. No one's getting in."),
            },
            {"signature": "TSCE", "data_hex": "04080000"},
        ],
    }
    scene = {
        "signature": "SCEN",
        "form_id": "000804",
        "form_version": 257,
        "subrecords": [
            {"signature": "EDID", "data_hex": text("CombinedDialogueScene")},
            {"signature": "PNAM", "data_hex": "00080000"},
            {"signature": "ANAM", "data_hex": "03000000"},
            {"signature": "INAM", "data_hex": "38000000"},
            {"signature": "ESCE", "data_hex": "02080000"},
            {"signature": "ESCS", "data_hex": "01080000"},
            {"signature": "DTGT", "data_hex": "00000000"},
        ],
    }
    return json.dumps(
        {
            "plugin": plugin_name,
            "game": "fo76",
            "header": {"version": 1.0, "next_object_id": "000805"},
            "items": [
                {
                    "type": "group",
                    "label_text": "QUST",
                    "group_type": 0,
                    "children": [quest],
                },
                {
                    "type": "group",
                    "label_text": "DIAL",
                    "group_type": 0,
                    "children": [
                        player_dial,
                        {
                            "type": "group",
                            "label_hex": "01080000",
                            "group_type": 7,
                            "children": [combined_info],
                        },
                        npc_dial,
                    ],
                },
                {
                    "type": "group",
                    "label_text": "SCEN",
                    "group_type": 0,
                    "children": [scene],
                },
            ],
        }
    )


def _exported_signatures(items) -> set[str]:
    signatures: set[str] = set()
    for item in items:
        signature = item.get("signature")
        if signature:
            signatures.add(signature)
        signatures.update(_exported_signatures(item.get("children", [])))
    return signatures


# ---------------------------------------------------------------------------
# translate_all
# ---------------------------------------------------------------------------


class TestTranslateAll:
    """conversion_run_translate_all returns stats and writes records to target."""

    def test_translate_all_translates_fixture_weap_without_failures(self):
        m = _native()
        with _create_run(FIXTURE) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert "records_dropped" in stats
            assert "records_deferred" in stats
            assert stats["records_translated"] >= 1
            assert stats["records_failed"] == 0
            assert stats["by_signature"]["WEAP"]["translated"] >= 1

        with pytest.raises(Exception):
            m.conversion_run_translate_all(999999999)

    def test_translate_all_preserves_short_fnv_inline_lstring(self, tmp_path):
        source_path = _write_plugin(
            tmp_path / "FalloutNV.esm",
            game="fnv",
            records=[
                Record(
                    signature="MISC",
                    form_id=0x000800,
                    form_version=15,
                    subrecords=[
                        Subrecord("EDID", b"DrinkingGlass01\0"),
                        Subrecord("FULL", b"Cup\0"),
                    ],
                )
            ],
        )
        with _create_run(source_path, source_game="fnv") as run:
            stats = _native().conversion_run_translate_all(run.id)
            assert stats["records_translated"] == 1
            target_path = _save_target(run, tmp_path)

        handle = plugin_handle_load(str(target_path), game="fo4")
        try:
            subrecords = plugin_handle_record_subrecords(handle, 0x000800) or []
            full = next(data for signature, data, _ in subrecords if signature == "FULL")
            assert full == b"Cup\0"
        finally:
            plugin_handle_close(handle)

    def test_translate_all_honors_zero_records_limit(self):
        m = _native()
        with _create_run(FIXTURE, config={"records_limit": 0}) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_translated"] == 0
            assert stats["records_dropped"] == 0
            assert stats["records_deferred"] == 0
            assert stats["records_failed"] == 0
            assert stats["by_signature"] == {}

    def test_translate_records_accepts_plugin_first_form_key(self):
        """Native root discovery returns Plugin.esm:XXXXXX FormKeys."""
        m = _native()
        with _create_run(FIXTURE) as run:
            stats = m.conversion_run_translate_records(
                run.id,
                ["fo4_minimal_weap.esm:000800"],
            )
            warnings = m.conversion_run_drain_warnings(run.id)
            assert stats["records_translated"] >= 1
            assert stats["records_failed"] == 0
            assert stats["by_signature"]["WEAP"]["seen"] >= 1
            assert not any("bad_form_key" in warning for warning in warnings)

    @pytest.mark.parametrize(
        "signature",
        ["ATXO", "GMST", "DFOB", "DIAL", "NAVM", "REFR"],
    )
    def test_translate_all_drops_fo76_records_fo4_cannot_hold_top_level(
        self,
        signature,
        tmp_path,
    ):
        m = _native()
        source_path = _write_plugin(
            tmp_path / f"Source{signature}.esm",
            game="fo76",
            records=[
                Record(
                    signature=signature,
                    form_id=0x000800,
                    form_version=257,
                    subrecords=[
                        Subrecord("EDID", f"Skip{signature}\0".encode("ascii"))
                    ],
                )
            ],
        )
        target_name = f"Output{signature}.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
        ) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_translated"] == 0
            assert stats["records_dropped"] == 1
            assert stats["records_failed"] == 0
            assert stats["by_signature"][signature]["seen"] == 1
            assert stats["by_signature"][signature]["dropped"] == 1
            assert _group_count(run, tmp_path, signature, target_name) == 0

    def test_whole_plugin_translate_all_emits_fo76_scene(self, tmp_path):
        m = _native()
        source_path = _write_plugin(
            tmp_path / "SourceScene.esm",
            game="fo76",
            records=[
                Record(
                    signature="QUST",
                    form_id=0x000800,
                    form_version=257,
                    subrecords=[Subrecord("EDID", b"ParentQuest\0")],
                ),
                Record(
                    signature="SCEN",
                    form_id=0x000801,
                    form_version=257,
                    subrecords=[
                        Subrecord("EDID", b"ChildScene\0"),
                        Subrecord("PNAM", (0x000800).to_bytes(4, "little")),
                        Subrecord("INAM", (0).to_bytes(4, "little")),
                        Subrecord("VNAM", (0).to_bytes(16, "little")),
                    ],
                ),
            ],
        )
        target_name = "OutputScene.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
            config={
                "preserve_source_ids": True,
                "is_whole_plugin": True,
            },
        ) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["by_signature"]["SCEN"]["seen"] == 1
            assert stats["by_signature"]["SCEN"]["translated"] == 1
            assert stats["by_signature"]["SCEN"]["dropped"] == 0
            exported = _target_export(run, tmp_path, target_name)
            assert "SCEN" in _exported_signatures(exported["items"])

    def test_whole_plugin_translate_all_places_fo76_dialogue_and_info_under_quest(
        self,
        tmp_path,
    ):
        m = _native()
        source_path = _write_imported_plugin(
            tmp_path / "SourceDialogue.esm",
            _fo76_dialogue_source_json("SourceDialogue.esm"),
        )
        target_name = "OutputDialogue.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
            config={
                "preserve_source_ids": True,
                "is_whole_plugin": True,
            },
        ) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["by_signature"]["DIAL"]["translated"] == 1
            assert stats["by_signature"]["INFO"]["translated"] == 1
            target_path = _save_target(run, tmp_path, target_name)
            with Plugin.load(target_path, game="fo4") as target:
                top_groups = {label for label, _count in target.group_signatures}
            assert "DIAL" not in top_groups
            assert "INFO" not in top_groups

            exported = _target_export(run, tmp_path, target_name)
            quest_group = next(
                item for item in exported["items"] if item.get("label_text") == "QUST"
            )
            quest_child_groups = [
                item
                for item in quest_group["children"]
                if item.get("type") == "group"
                and item.get("group_type") == 10
                and item.get("label_hex") == "00080000"
            ]
            assert len(quest_child_groups) == 1
            quest_child = quest_child_groups[0]
            assert any(
                child.get("signature") == "DIAL" for child in quest_child["children"]
            )
            topic_child_groups = [
                child
                for child in quest_child["children"]
                if child.get("type") == "group"
                and child.get("group_type") == 7
                and child.get("label_hex") == "01080000"
            ]
            assert len(topic_child_groups) == 1
            assert any(
                child.get("signature") == "INFO"
                for child in topic_child_groups[0]["children"]
            )

    def test_whole_plugin_splits_fo76_combined_player_prompt_and_npc_response(
        self,
        tmp_path,
    ):
        m = _native()
        source_path = _write_imported_plugin(
            tmp_path / "SourceCombinedDialogue.esm",
            _fo76_combined_scene_dialogue_source_json("SourceCombinedDialogue.esm"),
        )
        target_name = "OutputCombinedDialogue.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
            config={
                "preserve_source_ids": True,
                "is_whole_plugin": True,
            },
        ) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["by_signature"]["INFO"]["translated"] == 1
            exported = _target_export(run, tmp_path, target_name)

        quest_group = next(
            item for item in exported["items"] if item.get("label_text") == "QUST"
        )
        quest_child = next(
            item
            for item in quest_group["children"]
            if item.get("type") == "group"
            and item.get("group_type") == 10
            and item.get("label_hex") == "00080000"
        )
        topic_groups = {
            item["label_hex"]: item
            for item in quest_child["children"]
            if item.get("type") == "group" and item.get("group_type") == 7
        }
        player_info = next(
            child
            for child in topic_groups["01080000"]["children"]
            if child.get("signature") == "INFO"
        )
        npc_info = next(
            child
            for child in topic_groups["02080000"]["children"]
            if child.get("signature") == "INFO"
        )

        def fields(record: dict) -> dict[str, str]:
            return {
                field["signature"]: field["data_hex"]
                for field in record["subrecords"]
            }

        player_fields = fields(player_info)
        npc_fields = fields(npc_info)
        assert bytes.fromhex(player_fields["NAM1"]).rstrip(b"\0").decode() == (
            "The door's sealed tight. No one's getting in."
        )
        assert bytes.fromhex(npc_fields["NAM1"]).rstrip(b"\0").decode() == (
            "God damn it. *sigh*"
        )
        assert "RNAM" in player_fields
        assert "RNAM" not in npc_fields
        assert "TSCE" not in player_fields
        assert npc_fields["TSCE"] == "04080000"

    def test_whole_plugin_translate_all_excluding_quest_suppresses_dialogue_tail(
        self,
        tmp_path,
    ):
        m = _native()
        source_path = _write_imported_plugin(
            tmp_path / "SourceDialogueSkipQuest.esm",
            _fo76_dialogue_source_json("SourceDialogueSkipQuest.esm"),
        )
        target_name = "OutputDialogueSkipQuest.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
            config={
                "preserve_source_ids": True,
                "is_whole_plugin": True,
                "skip_record_signatures": ["QUST"],
            },
        ) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["by_signature"]["QUST"]["dropped"] == 1
            assert stats["records_translated"] == 0

            exported = _target_export(run, tmp_path, target_name)
            assert _exported_signatures(exported["items"]).isdisjoint(
                {"QUST", "DIAL", "INFO"}
            )

# ---------------------------------------------------------------------------
# fixups_v2
# ---------------------------------------------------------------------------


class TestFixupsV2:
    """fixups_v2 is the canonical post-translation path."""

    def test_fixups_v2_returns_phase_report(self):
        m = _native()
        with _create_run(FIXTURE) as run:
            m.conversion_run_translate_all(run.id)
            report = _run_fixups_v2(m, run.id)
            for key in (
                "records_changed",
                "records_dropped",
                "records_added",
                "assets_written",
                "warnings",
                "elapsed_ms",
            ):
                assert key in report
            with pytest.raises(Exception):
                m.conversion_run_phase(run.id, "fixups", {"mod_path": "", "params": {}})

    def test_fixups_v2_preserves_packin_storage_cell(self, tmp_path):
        m = _native()
        source_path = _write_plugin(
            tmp_path / "SeventySix.esm",
            game="fo76",
            records=[
                Record(
                    signature="CELL",
                    form_id=0x21CA70,
                    form_version=257,
                    subrecords=[Subrecord("DATA", (0x0401).to_bytes(2, "little"))],
                ),
                Record(
                    signature="PKIN",
                    form_id=0x21DA21,
                    form_version=257,
                    subrecords=[
                        Subrecord("EDID", b"SupermutantClutter11\0"),
                        Subrecord("CNAM", (0x21CA70).to_bytes(4, "little")),
                    ],
                ),
            ],
        )
        target_name = "SeventySix.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
            config={
                "is_whole_plugin": True,
                "preserve_source_ids": True,
            },
        ) as run:
            stats = m.conversion_run_translate_records(
                run.id,
                ["SeventySix.esm:21CA70", "SeventySix.esm:21DA21"],
            )
            assert stats["by_signature"]["CELL"]["dropped"] == 1
            assert stats["by_signature"]["PKIN"]["translated"] == 1

            report = _run_fixups_v2(m, run.id)
            assert report["records_added"] == 1

            assert _group_count(run, tmp_path, "CELL", target_name) == 1
            target_path = _save_target(run, tmp_path, target_name)
            with Plugin.load(target_path, game="fo4") as target:
                refs = target.get_referenced_form_keys_by_subrecord(
                    "SeventySix.esm:21DA21",
                    "CNAM",
                )
            assert refs == ["SeventySix.esm:21CA70"]

    def test_fixups_v2_preserves_duplicate_packin_storage_cell_once(self, tmp_path):
        m = _native()
        records = [
            Record(
                signature="CELL",
                form_id=0x21CA70,
                form_version=257,
                subrecords=[Subrecord("DATA", (0x0401).to_bytes(2, "little"))],
            )
        ]
        for form_id, editor_id in [
            (0x21DA21, b"PackinA\0"),
            (0x21DA22, b"PackinB\0"),
        ]:
            records.append(
                Record(
                    signature="PKIN",
                    form_id=form_id,
                    form_version=257,
                    subrecords=[
                        Subrecord("EDID", editor_id),
                        Subrecord("CNAM", (0x21CA70).to_bytes(4, "little")),
                    ],
                )
            )
        source_path = _write_plugin(
            tmp_path / "SeventySix.esm",
            game="fo76",
            records=records,
        )
        target_name = "SeventySix.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
            config={
                "is_whole_plugin": True,
                "preserve_source_ids": True,
            },
        ) as run:
            m.conversion_run_translate_records(
                run.id,
                [
                    "SeventySix.esm:21CA70",
                    "SeventySix.esm:21DA21",
                    "SeventySix.esm:21DA22",
                ],
            )
            report = _run_fixups_v2(m, run.id)
            assert report["records_added"] == 1

            assert _group_count(run, tmp_path, "CELL", target_name) == 1


# ---------------------------------------------------------------------------
# progress callback + cancellation
# ---------------------------------------------------------------------------


class TestProgressCallback:
    """conversion_run_translate_all accepts a progress_callback parameter."""

    def test_translate_all_callback_not_called_for_small_fixture(self):
        """With <1000 records, the callback should never be called."""
        m = _native()
        call_log: list = []
        with _create_run(FIXTURE) as run:
            stats = m.conversion_run_translate_all(
                run.id,
                progress_callback=lambda n: (call_log.append(n), True)[1],
            )
            assert isinstance(stats, dict), f"expected dict, got {type(stats)}"
            # Int record-count calls are gated to every 1000 records; the 1-WEAP
            # fixture must not trigger any. Free-text setup status strings
            # (mapper-state build + form-key enumeration) flow through the same
            # callback and are expected regardless of record count.
            int_calls = [n for n in call_log if isinstance(n, int)]
            assert int_calls == [], (
                f"record-count callback should not fire for <1000 records, got: {int_calls}"
            )
            assert any(isinstance(n, str) for n in call_log), (
                "expected translate_all setup status strings via the callback"
            )

    def test_set_progress_callback_then_clear_with_none(self):
        m = _native()
        calls: list = []
        with _create_run(FIXTURE) as run:
            m.conversion_run_set_progress_callback(run.id, lambda n: calls.append(n) or True)
            m.conversion_run_translate_all(run.id)
            assert calls
        calls.clear()
        with _create_run(FIXTURE) as run:
            m.conversion_run_set_progress_callback(run.id, lambda n: calls.append(n) or True)
            m.conversion_run_set_progress_callback(run.id, None)
            assert isinstance(m.conversion_run_translate_all(run.id), dict)
            assert calls == []
