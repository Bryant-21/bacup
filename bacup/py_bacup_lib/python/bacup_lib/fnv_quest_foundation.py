"""Inventory the source semantics required before FNV quests can be admitted to FO4."""

from __future__ import annotations

import json
from collections import Counter, deque
from dataclasses import asdict, dataclass
from typing import Protocol, Sequence


FOUNDATION_SCHEMA_VERSION = 1
FOUNDATION_INTEGRATION_STATE = "inventory_only_not_wired_to_mvp"
STORY_MANAGER_SIGNATURES = frozenset({"SMBN", "SMEN", "SMQN"})

_DIRECT_SCRIPT_SIGNATURES = frozenset({"SCPT"})
_DIALOGUE_SIGNATURES = frozenset({"DIAL", "INFO"})
_EVENT_PRODUCER_SIGNATURES = frozenset(
    {
        "ACHR",
        "ACRE",
        "ACTI",
        "CONT",
        "DOOR",
        "PACK",
        "REFR",
        "TERM",
    }
)


@dataclass(frozen=True, slots=True)
class FnvQuestRecord:
    form_key: str
    editor_id: str | None
    signature: str


@dataclass(frozen=True, slots=True)
class FnvQuestPlan:
    quest: FnvQuestRecord
    start_game_enabled: bool
    startup_mechanisms: tuple[str, ...]
    target_startup_strategy: str
    story_manager_strategy: str
    direct_dependencies: tuple[FnvQuestRecord, ...]
    inbound_producers: tuple[FnvQuestRecord, ...]
    dialogue_topics: tuple[FnvQuestRecord, ...]
    dialogue_infos: tuple[FnvQuestRecord, ...]


@dataclass(frozen=True, slots=True)
class FnvQuestFoundationReport:
    schema_version: int
    integration_state: str
    source_plugins: tuple[str, ...]
    source_story_manager_counts: dict[str, int]
    quests: tuple[FnvQuestPlan, ...]
    dependency_closure: tuple[FnvQuestRecord, ...]
    dependency_signature_counts: dict[str, int]
    unresolved_form_keys: tuple[str, ...]
    closure_truncated: bool

    def to_dict(self) -> dict:
        return json.loads(json.dumps(asdict(self)))

    def to_json(self, *, indent: int | None = 2) -> str:
        return json.dumps(self.to_dict(), indent=indent, sort_keys=True)


class FnvQuestReader(Protocol):
    @property
    def plugin_name(self) -> str: ...

    def records(self, signatures: Sequence[str] | None = None) -> Sequence[FnvQuestRecord]: ...

    def references(self, form_key: str) -> Sequence[str]: ...

    def references_by_subrecord(self, form_key: str, signature: str) -> Sequence[str]: ...

    def referencing(self, form_key: str) -> Sequence[str]: ...

    def quest_start_game_enabled(self, form_key: str) -> bool: ...

    def dialogue_info_form_keys(self, dialogue_form_key: str) -> Sequence[str]: ...


class NativeFnvQuestReader:
    def __init__(self, plugin) -> None:
        from creation_lib.esp import native_runtime

        handle = getattr(plugin, "_rust_handle", None)
        if handle is None:
            raise RuntimeError("FNV quest inventory requires a native-backed Plugin")
        self._plugin = plugin
        self._handle = handle
        self._native_runtime = native_runtime
        self._rows = {
            form_key.casefold(): (form_key, editor_id or None, signature.upper(), raw_form_id)
            for form_key, editor_id, signature, _object_id, raw_form_id
            in plugin.record_index_rows()
        }
        self._form_keys_by_raw_id = {
            raw_form_id & 0xFFFF_FFFF: form_key
            for form_key, _editor_id, _signature, raw_form_id in self._rows.values()
        }
        self._dialogue_infos = self._load_dialogue_infos()

    @property
    def plugin_name(self) -> str:
        return self._plugin.plugin_name

    def records(self, signatures: Sequence[str] | None = None) -> Sequence[FnvQuestRecord]:
        wanted = None if signatures is None else {signature.upper() for signature in signatures}
        return tuple(
            FnvQuestRecord(form_key, editor_id, signature)
            for form_key, editor_id, signature, _raw_form_id in self._rows.values()
            if wanted is None or signature in wanted
        )

    def references(self, form_key: str) -> Sequence[str]:
        return self._plugin.get_referenced_form_keys(form_key)

    def references_by_subrecord(self, form_key: str, signature: str) -> Sequence[str]:
        return self._plugin.get_referenced_form_keys_by_subrecord(form_key, signature)

    def referencing(self, form_key: str) -> Sequence[str]:
        return self._plugin.get_referencing_form_keys(form_key)

    def quest_start_game_enabled(self, form_key: str) -> bool:
        row = self._rows.get(form_key.casefold())
        if row is None:
            return False
        subrecords = self._native_runtime.plugin_handle_record_subrecords(
            self._handle, row[3]
        )
        if subrecords is None:
            return False
        return _fnv_quest_start_game_enabled(subrecords)

    def dialogue_info_form_keys(self, dialogue_form_key: str) -> Sequence[str]:
        return self._dialogue_infos.get(dialogue_form_key.casefold(), ())

    def _load_dialogue_infos(self) -> dict[str, tuple[str, ...]]:
        payload = json.loads(
            self._native_runtime.plugin_handle_extract_dialogue_text(
                self._handle, "json"
            )
        )
        mapping: dict[str, tuple[str, ...]] = {}
        for topic in payload:
            dialogue_form_id = int(topic["dial_form_id"], 16) & 0xFFFF_FFFF
            dialogue_key = _canonical_form_key(
                self.plugin_name, dialogue_form_id, self._form_keys_by_raw_id
            )
            info_keys = []
            for info in topic.get("infos", ()):
                info_form_id = int(info["form_id"], 16) & 0xFFFF_FFFF
                info_keys.append(
                    _canonical_form_key(
                        self.plugin_name, info_form_id, self._form_keys_by_raw_id
                    )
                )
            mapping[dialogue_key.casefold()] = tuple(sorted(info_keys, key=str.casefold))
        return mapping


def audit_fnv_quest_plugins(
    plugins: Sequence,
    *,
    max_closure_records: int = 100_000,
) -> FnvQuestFoundationReport:
    return audit_fnv_quest_readers(
        [NativeFnvQuestReader(plugin) for plugin in plugins],
        max_closure_records=max_closure_records,
    )


def _fnv_quest_start_game_enabled(subrecords: Sequence[tuple[str, bytes, object]]) -> bool:
    for signature, data, _semantic_type in subrecords:
        if signature.upper() == "DATA" and data:
            return bool(data[0] & 0x01)
    return False


def _canonical_form_key(
    plugin_name: str, raw_form_id: int, form_keys_by_raw_id: dict[int, str]
) -> str:
    return form_keys_by_raw_id.get(
        raw_form_id & 0xFFFF_FFFF,
        f"{plugin_name}:{raw_form_id & 0x00FF_FFFF:06X}",
    )


def audit_fnv_quest_readers(
    readers: Sequence[FnvQuestReader],
    *,
    max_closure_records: int = 100_000,
) -> FnvQuestFoundationReport:
    if not readers:
        raise ValueError("at least one FNV source plugin is required")
    if max_closure_records < 1:
        raise ValueError("max_closure_records must be positive")

    record_index = _record_index(readers)
    quests = _records_for_signatures(readers, ("QUST",))
    story_counts = Counter(
        record.signature
        for record in _records_for_signatures(readers, tuple(STORY_MANAGER_SIGNATURES))
    )
    plans = tuple(
        _plan_quest(quest, readers, record_index)
        for quest in sorted(quests, key=lambda record: record.form_key.casefold())
    )
    closure, unresolved, truncated = _dependency_closure(
        [plan.quest.form_key for plan in plans],
        readers,
        record_index,
        max_records=max_closure_records,
    )
    signature_counts = Counter(record.signature for record in closure)
    return FnvQuestFoundationReport(
        schema_version=FOUNDATION_SCHEMA_VERSION,
        integration_state=FOUNDATION_INTEGRATION_STATE,
        source_plugins=tuple(reader.plugin_name for reader in readers),
        source_story_manager_counts={
            signature: story_counts.get(signature, 0)
            for signature in sorted(STORY_MANAGER_SIGNATURES)
        },
        quests=plans,
        dependency_closure=closure,
        dependency_signature_counts=dict(sorted(signature_counts.items())),
        unresolved_form_keys=unresolved,
        closure_truncated=truncated,
    )


def _record_index(readers: Sequence[FnvQuestReader]) -> dict[str, FnvQuestRecord]:
    result = {}
    for reader in readers:
        for record in reader.records():
            result[record.form_key.casefold()] = record
    return result


def _records_for_signatures(
    readers: Sequence[FnvQuestReader], signatures: Sequence[str]
) -> tuple[FnvQuestRecord, ...]:
    records = {
        record.form_key.casefold(): record
        for reader in readers
        for record in reader.records(signatures)
    }
    return tuple(records.values())


def _resolve_records(
    form_keys: Sequence[str], record_index: dict[str, FnvQuestRecord]
) -> tuple[FnvQuestRecord, ...]:
    records = {
        record.form_key.casefold(): record
        for form_key in form_keys
        if (record := record_index.get(form_key.casefold())) is not None
    }
    return tuple(sorted(records.values(), key=lambda record: record.form_key.casefold()))


def _all_references(readers: Sequence[FnvQuestReader], form_key: str) -> tuple[str, ...]:
    return tuple(
        sorted(
            {
                reference.casefold(): reference
                for reader in readers
                for reference in reader.references(form_key)
            }.values(),
            key=str.casefold,
        )
    )


def _all_referencing(readers: Sequence[FnvQuestReader], form_key: str) -> tuple[str, ...]:
    return tuple(
        sorted(
            {
                reference.casefold(): reference
                for reader in readers
                for reference in reader.referencing(form_key)
            }.values(),
            key=str.casefold,
        )
    )


def _plan_quest(
    quest: FnvQuestRecord,
    readers: Sequence[FnvQuestReader],
    record_index: dict[str, FnvQuestRecord],
) -> FnvQuestPlan:
    start_game_enabled = any(
        reader.quest_start_game_enabled(quest.form_key) for reader in readers
    )
    direct_dependencies = _resolve_records(
        _all_references(readers, quest.form_key), record_index
    )
    inbound_producers = _resolve_records(
        _all_referencing(readers, quest.form_key), record_index
    )
    topics = _resolve_records(
        tuple(
            form_key
            for reader in readers
            for form_key in reader.referencing(quest.form_key)
            if (record := record_index.get(form_key.casefold())) is not None
            and record.signature == "DIAL"
        ),
        record_index,
    )
    info_keys = tuple(
        form_key
        for topic in topics
        for reader in readers
        for form_key in reader.dialogue_info_form_keys(topic.form_key)
    )
    infos = _resolve_records(info_keys, record_index)
    mechanisms = _startup_mechanisms(start_game_enabled, inbound_producers)
    target_strategy, story_strategy = _target_startup_strategy(
        start_game_enabled, inbound_producers
    )
    return FnvQuestPlan(
        quest=quest,
        start_game_enabled=start_game_enabled,
        startup_mechanisms=mechanisms,
        target_startup_strategy=target_strategy,
        story_manager_strategy=story_strategy,
        direct_dependencies=direct_dependencies,
        inbound_producers=inbound_producers,
        dialogue_topics=topics,
        dialogue_infos=infos,
    )


def _startup_mechanisms(
    start_game_enabled: bool, producers: Sequence[FnvQuestRecord]
) -> tuple[str, ...]:
    mechanisms = []
    signatures = {producer.signature for producer in producers}
    if start_game_enabled:
        mechanisms.append("start_game_enabled")
    if signatures & _DIRECT_SCRIPT_SIGNATURES:
        mechanisms.append("legacy_script_callsite")
    if signatures & _DIALOGUE_SIGNATURES:
        mechanisms.append("dialogue_result_script")
    if signatures & _EVENT_PRODUCER_SIGNATURES:
        mechanisms.append("object_or_actor_event")
    if not mechanisms:
        mechanisms.append("unresolved")
    return tuple(mechanisms)


def _target_startup_strategy(
    start_game_enabled: bool, producers: Sequence[FnvQuestRecord]
) -> tuple[str, str]:
    signatures = {producer.signature for producer in producers}
    if start_game_enabled:
        return "preserve_quest_autostart", "not_required"
    if signatures & _DIRECT_SCRIPT_SIGNATURES:
        return "translate_direct_papyrus_callsite", "not_required"
    if signatures & _DIALOGUE_SIGNATURES:
        return "translate_dialogue_fragment", "not_required"
    if signatures & _EVENT_PRODUCER_SIGNATURES:
        return "translate_event_producer", "candidate_blocked_pending_event_proof"
    return "manual_startup_reconstruction_required", "blocked_no_source_event_proof"


def _dependency_closure(
    roots: Sequence[str],
    readers: Sequence[FnvQuestReader],
    record_index: dict[str, FnvQuestRecord],
    *,
    max_records: int,
) -> tuple[tuple[FnvQuestRecord, ...], tuple[str, ...], bool]:
    pending = deque(sorted(roots, key=str.casefold))
    visited: set[str] = set()
    unresolved: dict[str, str] = {}
    records: dict[str, FnvQuestRecord] = {}
    truncated = False
    while pending:
        form_key = pending.popleft()
        normalized = form_key.casefold()
        if normalized in visited:
            continue
        if len(visited) >= max_records:
            truncated = True
            break
        visited.add(normalized)
        record = record_index.get(normalized)
        if record is None:
            unresolved[normalized] = form_key
            continue
        records[normalized] = record
        neighbors = set(_all_references(readers, record.form_key))
        if record.signature == "QUST":
            neighbors.update(_all_referencing(readers, record.form_key))
        if record.signature == "DIAL":
            for reader in readers:
                neighbors.update(reader.dialogue_info_form_keys(record.form_key))
        pending.extend(sorted(neighbors, key=str.casefold))
    return (
        tuple(sorted(records.values(), key=lambda record: record.form_key.casefold())),
        tuple(sorted(unresolved.values(), key=str.casefold)),
        truncated,
    )
