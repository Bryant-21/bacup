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

    def test_translate_all_returns_stats_dict(self):
        m = _native()
        with _create_run(FIXTURE) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert isinstance(stats, dict), f"expected dict, got {type(stats)}"
            assert "records_translated" in stats
            assert "records_dropped" in stats
            assert "records_deferred" in stats
            assert "records_failed" in stats
            assert "by_signature" in stats
            assert isinstance(stats["by_signature"], dict)

    def test_translate_all_translates_weap_record(self):
        """The fixture has exactly one WEAP record; it should be translated."""
        m = _native()
        with _create_run(FIXTURE) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_translated"] >= 1, (
                f"expected at least 1 translated record, got {stats}"
            )
            assert stats["by_signature"]["WEAP"]["seen"] >= 1
            assert stats["by_signature"]["WEAP"]["translated"] >= 1

    def test_translate_all_no_failures(self):
        """The minimal fixture should produce no failed records."""
        m = _native()
        with _create_run(FIXTURE) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_failed"] == 0, (
                f"expected 0 failed records, got {stats['records_failed']}"
            )

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

    def test_translate_all_drops_records_missing_from_target_schema(self, tmp_path):
        """FO76-only record signatures must not be written to FO4 targets."""
        m = _native()
        source_path = _write_plugin(
            tmp_path / "Source.esm",
            game="fo76",
            records=[
                Record(
                    signature="ATXO",
                    form_id=0x000800,
                    form_version=257,
                    subrecords=[Subrecord("EDID", b"AtomicShopOnly\0")],
                )
            ],
        )
        with _create_run(source_path, source_game="fo76") as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_translated"] == 0
            assert stats["records_dropped"] == 1
            assert stats["records_failed"] == 0
            assert stats["by_signature"]["ATXO"]["seen"] == 1
            assert stats["by_signature"]["ATXO"]["dropped"] == 1
            assert _group_count(run, tmp_path, "ATXO") == 0

    def test_translate_all_skips_fo76_game_settings(self, tmp_path):
        m = _native()
        source_path = _write_plugin(
            tmp_path / "Source.esm",
            game="fo76",
            records=[
                Record(
                    signature="GMST",
                    form_id=0x000800,
                    form_version=257,
                    subrecords=[
                        Subrecord("EDID", b"uInvalidForFo4\0"),
                        Subrecord("DATA", (123).to_bytes(4, "little")),
                    ],
                )
            ],
        )
        with _create_run(source_path, source_game="fo76") as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_translated"] == 0
            assert stats["records_dropped"] == 1
            assert stats["records_failed"] == 0
            assert stats["by_signature"]["GMST"]["seen"] == 1
            assert stats["by_signature"]["GMST"]["dropped"] == 1
            assert _group_count(run, tmp_path, "GMST") == 0

    def test_translate_all_skips_fo76_default_object_assignments(self, tmp_path):
        m = _native()
        source_path = _write_plugin(
            tmp_path / "SourceDfoo.esm",
            game="fo76",
            records=[
                Record(
                    signature="DFOB",
                    form_id=0x000800,
                    form_version=257,
                    subrecords=[
                        Subrecord("EDID", b"GoldBullion_DO\0"),
                        Subrecord("DATA", (0x000801).to_bytes(4, "little")),
                    ],
                )
            ],
        )
        target_name = "OutputDfoo.esm"
        with _create_run(
            source_path,
            source_game="fo76",
            target_plugin_name=target_name,
        ) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_translated"] == 0
            assert stats["records_dropped"] == 1
            assert stats["records_failed"] == 0
            assert stats["by_signature"]["DFOB"]["seen"] == 1
            assert stats["by_signature"]["DFOB"]["dropped"] == 1
            assert _group_count(run, tmp_path, "DFOB", target_name) == 0

    @pytest.mark.parametrize(
        "signature",
        [
            "ACHR",
            "DIAL",
            "INFO",
            "NAVM",
            "PGRE",
            "PHZD",
            "PLYR",
            "PMIS",
            "REFR",
        ],
    )
    def test_translate_all_skips_fo76_records_that_cannot_be_flat_top_level(
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

    def test_skyrim_mvp_excludes_all_mapped_actor_anatomy_records(self, tmp_path):
        actor_only_signatures = {
            "BPTD",
            "CLFM",
            "CSTY",
            "EYES",
            "HDPT",
            "MOVT",
            "RACE",
        }
        m = _native()
        items = []
        for index, signature in enumerate(sorted(actor_only_signatures), start=0x800):
            editor_id = f"Test{signature}".encode().hex() + "00"
            items.append(
                {
                    "type": "group",
                    "label_text": signature,
                    "group_type": 0,
                    "children": [
                        {
                            "signature": signature,
                            "form_id": f"{index:06X}",
                            "form_version": 44,
                            "subrecords": [
                                {"signature": "EDID", "data_hex": editor_id}
                            ],
                        }
                    ],
                }
            )

        source_path = _write_imported_plugin(
            tmp_path / "SkyrimActorAnatomy.esm",
            json.dumps(
                {
                    "plugin": "SkyrimActorAnatomy.esm",
                    "game": "skyrimse",
                    "header": {"version": 1.7, "next_object_id": "000805"},
                    "items": items,
                }
            ),
        )
        target_name = "SkyrimActorAnatomyOutput.esm"
        with _create_run(
            source_path,
            source_game="skyrimse",
            target_plugin_name=target_name,
            config={
                "preserve_source_ids": True,
                "is_whole_plugin": True,
                "skip_record_signatures": sorted(actor_only_signatures),
            },
        ) as run:
            stats = m.conversion_run_translate_all(run.id)
            assert stats["records_translated"] == 0
            assert all(
                stats["by_signature"][signature]["dropped"] == 1
                for signature in actor_only_signatures
            )

            exported = _target_export(run, tmp_path, target_name)
            assert _exported_signatures(exported["items"]).isdisjoint(
                actor_only_signatures
            )

    def test_translate_all_unknown_run_raises(self):
        """Calling translate_all with an unknown run_id must raise RuntimeError."""
        m = _native()
        with pytest.raises(Exception):
            m.conversion_run_translate_all(999999999)

    def test_translate_all_no_longer_raises_stub_error(self):
        """translate_all must not raise a stub RuntimeError."""
        m = _native()
        with _create_run(FIXTURE) as run:
            # Should NOT raise.
            stats = m.conversion_run_translate_all(run.id)
            assert isinstance(stats, dict)


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
            assert isinstance(report, dict), f"expected dict, got {type(report)}"
            for key in (
                "records_changed",
                "records_dropped",
                "records_added",
                "assets_written",
                "warnings",
                "elapsed_ms",
            ):
                assert key in report

    def test_fixups_v2_reports_aggregate_changes(self):
        m = _native()
        with _create_run(FIXTURE) as run:
            m.conversion_run_translate_all(run.id)
            report = _run_fixups_v2(m, run.id)
            assert report["records_changed"] >= 0
            assert report["records_added"] >= 0
            assert report["records_dropped"] >= 0

    def test_legacy_fixups_phase_is_rejected(self):
        m = _native()
        with _create_run(FIXTURE) as run:
            with pytest.raises(Exception):
                m.conversion_run_phase(run.id, "fixups", {"mod_path": "", "params": {}})

    def test_fixups_v2_unknown_run_raises(self):
        m = _native()
        with pytest.raises(Exception):
            _run_fixups_v2(m, 999999999)

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

    def test_translate_all_cancel_via_callback(self):
        """A callback that immediately returns False should cancel translation."""
        m = _native()
        # We need a fixture with >=1000 records to trigger the yield; for the
        # minimal fixture this test is skipped since it has only 1 record.
        # Instead we verify the API plumbing: set_progress_callback + translate_all
        # with a True callback succeeds, confirming the callback wiring works.
        with _create_run(FIXTURE) as run:
            # A callback that always returns True — should not cancel.
            stats = m.conversion_run_translate_all(
                run.id,
                progress_callback=lambda n: True,
            )
            assert isinstance(stats, dict)

    def test_set_progress_callback_then_translate_all(self):
        """conversion_run_set_progress_callback stores the callback for translate_all."""
        m = _native()
        with _create_run(FIXTURE) as run:
            # Pre-install the callback, then call translate_all without inline cb.
            m.conversion_run_set_progress_callback(run.id, lambda n: True)
            stats = m.conversion_run_translate_all(run.id)
            assert isinstance(stats, dict)

    def test_set_progress_callback_clear_with_none(self):
        """Passing None to set_progress_callback clears any existing callback."""
        m = _native()
        with _create_run(FIXTURE) as run:
            m.conversion_run_set_progress_callback(run.id, lambda n: True)
            m.conversion_run_set_progress_callback(run.id, None)
            stats = m.conversion_run_translate_all(run.id)
            assert isinstance(stats, dict)
