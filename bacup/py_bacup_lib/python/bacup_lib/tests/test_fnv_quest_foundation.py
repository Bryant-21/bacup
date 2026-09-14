from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Sequence

import pytest

from bacup_lib.fnv_quest_foundation import (
    FOUNDATION_INTEGRATION_STATE,
    FnvQuestRecord,
    _canonical_form_key,
    _fnv_quest_start_game_enabled,
    audit_fnv_quest_readers,
)


@dataclass
class FakeReader:
    plugin_name: str
    corpus: tuple[FnvQuestRecord, ...]
    forward: dict[str, tuple[str, ...]]
    reverse: dict[str, tuple[str, ...]]
    autostart: frozenset[str]
    dialogue_infos: dict[str, tuple[str, ...]]

    def records(self, signatures: Sequence[str] | None = None):
        if signatures is None:
            return self.corpus
        wanted = {signature.upper() for signature in signatures}
        return tuple(record for record in self.corpus if record.signature in wanted)

    def references(self, form_key: str):
        return self.forward.get(form_key, ())

    def references_by_subrecord(self, form_key: str, signature: str):
        return ()

    def referencing(self, form_key: str):
        return self.reverse.get(form_key, ())

    def quest_start_game_enabled(self, form_key: str):
        return form_key in self.autostart

    def dialogue_info_form_keys(self, dialogue_form_key: str):
        return self.dialogue_infos.get(dialogue_form_key, ())


def record(form_key: str, signature: str, editor_id: str | None = None):
    return FnvQuestRecord(form_key, editor_id, signature)


def test_inventory_classifies_all_startup_paths_and_dialogue_topology():
    autostart = record("FalloutNV.esm:000100", "QUST", "AutoQuest")
    scripted = record("FalloutNV.esm:000101", "QUST", "ScriptQuest")
    event_quest = record("FalloutNV.esm:000102", "QUST", "EventQuest")
    unresolved = record("FalloutNV.esm:000103", "QUST", "ManualQuest")
    script = record("FalloutNV.esm:000200", "SCPT", "StartScript")
    activator = record("FalloutNV.esm:000201", "ACTI", "QuestTrigger")
    topic = record("FalloutNV.esm:000300", "DIAL", "QuestTopic")
    info = record("FalloutNV.esm:000301", "INFO")
    reader = FakeReader(
        "FalloutNV.esm",
        (autostart, scripted, event_quest, unresolved, script, activator, topic, info),
        forward={
            script.form_key: (scripted.form_key,),
            activator.form_key: (event_quest.form_key,),
            topic.form_key: (scripted.form_key,),
        },
        reverse={
            scripted.form_key: (script.form_key, topic.form_key),
            event_quest.form_key: (activator.form_key,),
        },
        autostart=frozenset({autostart.form_key}),
        dialogue_infos={topic.form_key: (info.form_key,)},
    )

    report = audit_fnv_quest_readers([reader])
    plans = {plan.quest.editor_id: plan for plan in report.quests}

    assert report.integration_state == FOUNDATION_INTEGRATION_STATE
    assert plans["AutoQuest"].target_startup_strategy == "preserve_quest_autostart"
    assert plans["AutoQuest"].story_manager_strategy == "not_required"
    assert plans["ScriptQuest"].startup_mechanisms == (
        "legacy_script_callsite",
        "dialogue_result_script",
    )
    assert plans["ScriptQuest"].target_startup_strategy == "translate_direct_papyrus_callsite"
    assert plans["ScriptQuest"].dialogue_topics == (topic,)
    assert plans["ScriptQuest"].dialogue_infos == (info,)
    assert plans["EventQuest"].target_startup_strategy == "translate_event_producer"
    assert (
        plans["EventQuest"].story_manager_strategy
        == "candidate_blocked_pending_event_proof"
    )
    assert plans["ManualQuest"].startup_mechanisms == ("unresolved",)
    assert (
        plans["ManualQuest"].story_manager_strategy
        == "blocked_no_source_event_proof"
    )


def test_inventory_reports_story_manager_records_without_treating_them_as_routes():
    quest = record("FalloutNV.esm:000100", "QUST")
    story_node = record("Unexpected.esp:000200", "SMQN")
    reader = FakeReader(
        "Unexpected.esp",
        (quest, story_node),
        forward={},
        reverse={},
        autostart=frozenset(),
        dialogue_infos={},
    )

    report = audit_fnv_quest_readers([reader])

    assert report.source_story_manager_counts == {"SMBN": 0, "SMEN": 0, "SMQN": 1}
    assert report.quests[0].story_manager_strategy == "blocked_no_source_event_proof"


def test_dependency_closure_traverses_quest_graph_and_reports_external_refs():
    quest = record("FalloutNV.esm:000100", "QUST")
    script = record("FalloutNV.esm:000200", "SCPT")
    item = record("FalloutNV.esm:000300", "MISC")
    missing = "Missing.esm:000400"
    reader = FakeReader(
        "FalloutNV.esm",
        (quest, script, item),
        forward={
            quest.form_key: (item.form_key,),
            script.form_key: (quest.form_key,),
            item.form_key: (missing,),
        },
        reverse={quest.form_key: (script.form_key,)},
        autostart=frozenset(),
        dialogue_infos={},
    )

    report = audit_fnv_quest_readers([reader])

    assert {entry.form_key for entry in report.dependency_closure} == {
        quest.form_key,
        script.form_key,
        item.form_key,
    }
    assert report.dependency_signature_counts == {"MISC": 1, "QUST": 1, "SCPT": 1}
    assert report.unresolved_form_keys == (missing,)
    assert not report.closure_truncated


def test_dependency_closure_cap_is_explicit_and_json_is_deterministic():
    quest = record("FalloutNV.esm:000100", "QUST")
    dependency = record("FalloutNV.esm:000200", "MISC")
    reader = FakeReader(
        "FalloutNV.esm",
        (dependency, quest),
        forward={quest.form_key: (dependency.form_key,)},
        reverse={},
        autostart=frozenset(),
        dialogue_infos={},
    )

    report = audit_fnv_quest_readers([reader], max_closure_records=1)

    assert report.closure_truncated
    assert json.loads(report.to_json()) == report.to_dict()


def test_inventory_requires_a_reader_and_positive_cap():
    with pytest.raises(ValueError, match="at least one"):
        audit_fnv_quest_readers([])
    reader = FakeReader("FalloutNV.esm", (), {}, {}, frozenset(), {})
    with pytest.raises(ValueError, match="positive"):
        audit_fnv_quest_readers([reader], max_closure_records=0)


def test_fnv_autostart_reads_legacy_qust_data_flags():
    assert _fnv_quest_start_game_enabled((("DATA", b"\x01\x64\x00\x00", None),))
    assert not _fnv_quest_start_game_enabled((("DATA", b"\x04\x64\x00\x00", None),))
    assert not _fnv_quest_start_game_enabled((("DNAM", b"\x01\x00", None),))


def test_dialogue_topology_preserves_master_owned_form_keys_for_overrides():
    raw_form_id = 0x01001234
    assert _canonical_form_key(
        "DeadMoney.esm", raw_form_id, {raw_form_id: "FalloutNV.esm:001234"}
    ) == "FalloutNV.esm:001234"
    assert _canonical_form_key("DeadMoney.esm", 0x02005678, {}) == (
        "DeadMoney.esm:005678"
    )
