from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache
from typing import Any

from creation_lib.esp.schema import FieldSpec, RecordSpec, SubrecordSpec, get_schema

_AUTHORING_KEY_ALIASES = {
    "group_body_data": "BodyDatas",
    "group_biped_model": "BipedModels",
    "group_object_template": "ObjectTemplates",
    "group_quest_dialogue_conditions": "QuestDialogueConditions",
    "group_story_manager_conditions": "StoryManagerConditions",
}


def _authoring_group_display_key(group_key: str) -> str:
    """Mirror native authoring_group_display_key: strip group_/xedit_group_ prefix,
    PascalCase the rest, and pluralize. This is what the Rust ESP serializer
    emits for row_group authoring keys."""
    stem = group_key
    for prefix in ("xedit_group_", "group_"):
        if stem.startswith(prefix):
            stem = stem[len(prefix):]
            break
    stem = stem.strip("_")
    parts = [part for part in stem.split("_") if part]
    if not parts:
        return group_key
    out = "".join(part[0].upper() + part[1:] for part in parts)
    if out.endswith("y"):
        out = out[:-1] + "ies"
    elif not out.endswith("s"):
        out += "s"
    return out


@dataclass(frozen=True)
class SchemaDecision:
    record_type: str
    field: str
    action: str
    reason: str
    value: Any = None


class SchemaRecordError(ValueError):
    pass


def _to_camel(text: str) -> str:
    out: list[str] = []
    cap = True
    for ch in text:
        if ch in ("'", "`"):
            continue
        if ch.isascii() and ch.isalnum():
            out.append(ch.upper() if cap and ch.isalpha() else ch)
            cap = False
        else:
            cap = True
    return "".join(out)


def _authoring_key(subrecord: SubrecordSpec) -> str:
    if subrecord.authoring_key:
        return subrecord.authoring_key
    if subrecord.row_label:
        return _to_camel(subrecord.row_label)
    if subrecord.display_label and subrecord.fields and len(subrecord.fields) > 1:
        return _to_camel(subrecord.display_label)
    return subrecord.sig


def _authoring_aliases(record_type: str, subrecord: SubrecordSpec) -> list[str]:
    keys = [_authoring_key(subrecord), subrecord.sig]
    if subrecord.authoring_key in _AUTHORING_KEY_ALIASES:
        keys.append(_AUTHORING_KEY_ALIASES[subrecord.authoring_key])
    if (
        subrecord.authoring_layout == "row_group"
        and subrecord.authoring_key
        and subrecord.authoring_key.startswith(("group_", "xedit_group_"))
    ):
        keys.append(_authoring_group_display_key(subrecord.authoring_key))
    if subrecord.sig == "FULL" and subrecord.display_label:
        keys.append(_to_camel(subrecord.display_label))
    if subrecord.display_label:
        # Mirror native schema_subrecord_key: display_label always wins.
        keys.append(_to_camel(subrecord.display_label))
    if subrecord.sig != "FULL" and len(subrecord.fields or []) == 1:
        field = subrecord.fields[0]
        keys.append(_field_key(field))
        if subrecord.display_label:
            keys.append(_to_camel(subrecord.display_label))
    elif not subrecord.fields and subrecord.display_label != "Data":
        keys.append(_to_camel(subrecord.display_label or ""))
    elif record_type == "SNDR" and subrecord.sig == "BNAM":
        keys.append("Data")

    seen: set[str] = set()
    return [key for key in keys if key and not (key in seen or seen.add(key))]


def _field_key(field: FieldSpec) -> str:
    return _to_camel(field.authoring_label or field.name)


def _field_default(field: FieldSpec) -> Any:
    return field.default_value


_DROP = object()


def _enum_member_key(text: str) -> str:
    return "".join(ch.lower() for ch in text if ch.isascii() and ch.isalnum())


def _is_unknown_member(text: str) -> bool:
    return _enum_member_key(text).startswith("unknown")


class SchemaSurface:
    def __init__(self, game: str) -> None:
        self.game = game
        self.schema = get_schema(game)

    def record(self, record_type: str) -> RecordSpec:
        spec = self.schema.records.get(record_type)
        if spec is None:
            raise SchemaRecordError(
                f"{self.game} schema does not contain record {record_type}"
            )
        return spec

    def allowed_keys(self, record_type: str) -> set[str]:
        return {
            key
            for subrecord in self.record(record_type).subrecords
            for key in _authoring_aliases(record_type, subrecord)
        }

    def ordered_keys(self, record_type: str) -> list[str]:
        spec = self.record(record_type)
        keys = [
            key
            for subrecord in spec.subrecords
            for key in _authoring_aliases(record_type, subrecord)
        ]
        seen: set[str] = set()
        return [key for key in keys if not (key in seen or seen.add(key))]

    def _metadata_keys(self, record_type: str) -> set[str]:
        return {
            "form_id",
            "flags",
            "version_control",
            "form_version",
            "version2",
            "eid",
            "type",
            "record_type",
            record_type,
        }

    def normalize_record(
        self,
        record: dict[str, Any],
        record_type: str,
    ) -> tuple[dict[str, Any], list[SchemaDecision]]:
        allowed = self.allowed_keys(record_type)
        ordered = self.ordered_keys(record_type)
        fields = record.get("fields")
        from_flat_record = not isinstance(fields, list)
        if not isinstance(fields, list):
            fields = [
                {key: value}
                for key, value in record.items()
                if key not in self._metadata_keys(record_type)
            ]

        buckets: dict[str, list[Any]] = {}
        invalid: list[str] = []
        for index, entry in enumerate(fields):
            if not isinstance(entry, dict):
                raise SchemaRecordError(
                    f"{record_type}.fields[{index}] must be a single-key dict"
                )
            if len(entry) != 1:
                keys = ", ".join(str(key) for key in entry)
                raise SchemaRecordError(
                    f"{record_type}.fields[{index}] must contain one field key; "
                    f"got: {keys}"
                )
            key, value = next(iter(entry.items()))
            if key not in allowed:
                invalid.append(f"{record_type}.{key}")
                continue
            buckets.setdefault(key, []).append(value)

        if invalid:
            raise SchemaRecordError(
                f"Invalid target authoring field(s): {', '.join(sorted(invalid))}"
            )

        out_fields: list[dict[str, Any]] = []
        emitted: set[str] = set()
        for key in ordered:
            for value in buckets.get(key, []):
                out_fields.append({key: value})
            emitted.add(key)
        for key, values in buckets.items():
            if key in emitted:
                continue
            for value in values:
                out_fields.append({key: value})

        if from_flat_record:
            out = {
                key: value
                for key, value in record.items()
                if key in self._metadata_keys(record_type)
            }
        else:
            out = {key: value for key, value in record.items() if key != "fields"}
        out["fields"] = out_fields
        return out, []

    def sanitize_record_values(
        self,
        record: dict[str, Any],
        record_type: str,
        *,
        drop_unknown_flags: bool = True,
    ) -> tuple[dict[str, Any], list[SchemaDecision]]:
        decisions: list[SchemaDecision] = []
        out: dict[str, Any] = {}
        for key, value in record.items():
            subrecords = self._subrecords_for_key(record_type, key)
            if not subrecords:
                out[key] = value
                continue
            if self._is_row_group_key(subrecords):
                cleaned = self._sanitize_row_group_value(
                    record_type,
                    key,
                    subrecords,
                    value,
                    decisions,
                    drop_unknown_flags=drop_unknown_flags,
                )
            else:
                cleaned = self._sanitize_subrecord_value(
                    record_type,
                    key,
                    subrecords[0],
                    value,
                    decisions,
                    drop_unknown_flags=drop_unknown_flags,
                )
            if cleaned is _DROP:
                continue
            out[key] = cleaned
        return out, decisions

    def _subrecords_for_key(
        self,
        record_type: str,
        key: str,
    ) -> list[SubrecordSpec]:
        return [
            subrecord
            for subrecord in self.record(record_type).subrecords
            if key in _authoring_aliases(record_type, subrecord)
        ]

    def _subrecord_for_key(
        self,
        record_type: str,
        key: str,
    ) -> SubrecordSpec | None:
        subrecords = self._subrecords_for_key(record_type, key)
        return subrecords[0] if subrecords else None

    @staticmethod
    def _is_row_group_key(subrecords: list[SubrecordSpec]) -> bool:
        return len(subrecords) > 1 or any(
            subrecord.authoring_layout == "row_group"
            for subrecord in subrecords
        )

    def _sanitize_row_group_value(
        self,
        record_type: str,
        field_key: str,
        subrecords: list[SubrecordSpec],
        value: Any,
        decisions: list[SchemaDecision],
        *,
        drop_unknown_flags: bool,
    ) -> Any:
        if isinstance(value, list):
            return [
                self._sanitize_row_group_row(
                    record_type,
                    field_key,
                    subrecords,
                    item,
                    decisions,
                    drop_unknown_flags=drop_unknown_flags,
                )
                if isinstance(item, dict)
                else item
                for item in value
            ]
        if isinstance(value, dict):
            return self._sanitize_row_group_row(
                record_type,
                field_key,
                subrecords,
                value,
                decisions,
                drop_unknown_flags=drop_unknown_flags,
            )
        return value

    def _sanitize_row_group_row(
        self,
        record_type: str,
        field_key: str,
        subrecords: list[SubrecordSpec],
        row: dict[str, Any],
        decisions: list[SchemaDecision],
        *,
        drop_unknown_flags: bool,
    ) -> dict[str, Any]:
        out: dict[str, Any] = {}
        for nested_key, nested_value in row.items():
            subrecord = self._row_group_subrecord_for_key(
                record_type,
                field_key,
                subrecords,
                nested_key,
            )
            if subrecord is None:
                decisions.append(
                    SchemaDecision(
                        record_type,
                        f"{field_key}.{nested_key}",
                        "drop",
                        "invalid_target_nested_field",
                        nested_value,
                    )
                )
                continue
            field = self._field_map(subrecord).get(nested_key)
            if field is not None:
                cleaned = self._sanitize_field_value(
                    record_type,
                    f"{field_key}.{nested_key}",
                    field,
                    nested_value,
                    decisions,
                    drop_unknown_flags=drop_unknown_flags,
                )
            else:
                cleaned = self._sanitize_subrecord_value(
                    record_type,
                    f"{field_key}.{nested_key}",
                    subrecord,
                    nested_value,
                    decisions,
                    drop_unknown_flags=drop_unknown_flags,
                )
            if cleaned is _DROP:
                continue
            out[nested_key] = cleaned
        return out

    def _row_group_subrecord_for_key(
        self,
        record_type: str,
        field_key: str,
        subrecords: list[SubrecordSpec],
        nested_key: str,
    ) -> SubrecordSpec | None:
        for subrecord in subrecords:
            if (
                nested_key == "Name"
                and subrecord.sig == "FULL"
                and subrecord.authoring_key == "group_object_template"
            ):
                return subrecord
            aliases = set(_authoring_aliases(record_type, subrecord))
            aliases.discard(field_key)
            if subrecord.authoring_key:
                aliases.discard(subrecord.authoring_key)
                alias = _AUTHORING_KEY_ALIASES.get(subrecord.authoring_key)
                if alias:
                    aliases.discard(alias)
            aliases.update(self._field_map(subrecord))
            if nested_key in aliases:
                return subrecord
        return None

    def _field_map(self, subrecord: SubrecordSpec) -> dict[str, FieldSpec]:
        return self._field_map_from_fields(subrecord.fields or ())

    def _field_map_from_fields(
        self,
        fields: tuple[FieldSpec, ...],
    ) -> dict[str, FieldSpec]:
        mapping: dict[str, FieldSpec] = {}
        for field in fields:
            mapping[_field_key(field)] = field
            mapping[field.name] = field
        return mapping

    def _nested_field_map(self, field: FieldSpec) -> dict[str, FieldSpec]:
        nested: list[FieldSpec] = list(field.nested_fields)
        for variant in field.union_variants:
            nested.extend(variant.fields)
        return self._field_map_from_fields(tuple(nested))

    def _sanitize_subrecord_value(
        self,
        record_type: str,
        field_key: str,
        subrecord: SubrecordSpec,
        value: Any,
        decisions: list[SchemaDecision],
        *,
        drop_unknown_flags: bool,
    ) -> Any:
        if subrecord.enum_ref:
            return self._sanitize_enum_value(
                record_type,
                field_key,
                subrecord.enum_ref,
                value,
                decisions,
                drop_unknown_flags=drop_unknown_flags,
            )

        if not subrecord.fields:
            return value

        field_map = self._field_map(subrecord)
        if len(subrecord.fields) == 1:
            field = subrecord.fields[0]
            field_keys = {_field_key(field), field.name}
            if not self._value_contains_field_key(value, field_keys):
                return self._sanitize_field_value(
                    record_type,
                    f"{field_key}.{_field_key(field)}",
                    field,
                    value,
                    decisions,
                    drop_unknown_flags=drop_unknown_flags,
                )
        if isinstance(value, dict):
            return self._sanitize_struct_value(
                record_type,
                field_key,
                value,
                field_map,
                decisions,
                drop_unknown_flags=drop_unknown_flags,
            )
        if isinstance(value, list):
            cleaned_items: list[Any] = []
            for item in value:
                if isinstance(item, dict):
                    cleaned = self._sanitize_struct_value(
                        record_type,
                        field_key,
                        item,
                        field_map,
                        decisions,
                        drop_unknown_flags=drop_unknown_flags,
                    )
                    cleaned_items.append(cleaned)
                else:
                    cleaned_items.append(item)
            return cleaned_items

        if len(subrecord.fields) == 1:
            field = subrecord.fields[0]
            return self._sanitize_field_value(
                record_type,
                f"{field_key}.{_field_key(field)}",
                field,
                value,
                decisions,
                drop_unknown_flags=drop_unknown_flags,
            )
        return value

    @staticmethod
    def _value_contains_field_key(value: Any, field_keys: set[str]) -> bool:
        if isinstance(value, dict):
            return any(key in field_keys for key in value)
        if isinstance(value, list):
            return any(
                isinstance(item, dict)
                and any(key in field_keys for key in item)
                for item in value
            )
        return False

    def _sanitize_struct_value(
        self,
        record_type: str,
        field_key: str,
        value: dict[str, Any],
        field_map: dict[str, FieldSpec],
        decisions: list[SchemaDecision],
        *,
        drop_unknown_flags: bool,
    ) -> dict[str, Any]:
        out: dict[str, Any] = {}
        for nested_key, nested_value in value.items():
            field = field_map.get(nested_key)
            if field is None:
                decisions.append(
                    SchemaDecision(
                        record_type,
                        f"{field_key}.{nested_key}",
                        "drop",
                        "invalid_target_nested_field",
                        nested_value,
                    )
                )
                continue
            cleaned = self._sanitize_field_value(
                record_type,
                f"{field_key}.{nested_key}",
                field,
                nested_value,
                decisions,
                drop_unknown_flags=drop_unknown_flags,
            )
            if cleaned is _DROP:
                continue
            out[nested_key] = cleaned
        return out

    def _sanitize_field_value(
        self,
        record_type: str,
        field_path: str,
        field: FieldSpec,
        value: Any,
        decisions: list[SchemaDecision],
        *,
        drop_unknown_flags: bool,
    ) -> Any:
        if field.enum_ref:
            return self._sanitize_enum_value(
                record_type,
                field_path,
                field.enum_ref,
                value,
                decisions,
                drop_unknown_flags=drop_unknown_flags,
            )
        if field.union_variants and isinstance(value, dict) and (
            "variant" in value or "value" in value
        ):
            return value
        nested_map = self._nested_field_map(field)
        if nested_map and isinstance(value, dict):
            return self._sanitize_struct_value(
                record_type,
                field_path,
                value,
                nested_map,
                decisions,
                drop_unknown_flags=drop_unknown_flags,
            )
        if nested_map and isinstance(value, list):
            cleaned_items: list[Any] = []
            for item in value:
                if isinstance(item, dict):
                    cleaned_items.append(
                        self._sanitize_struct_value(
                            record_type,
                            field_path,
                            item,
                            nested_map,
                            decisions,
                            drop_unknown_flags=drop_unknown_flags,
                        )
                    )
                else:
                    cleaned_items.append(item)
            return cleaned_items
        return value

    def _sanitize_enum_value(
        self,
        record_type: str,
        field_path: str,
        enum_ref: str,
        value: Any,
        decisions: list[SchemaDecision],
        *,
        drop_unknown_flags: bool,
    ) -> Any:
        enum_def = self.schema.enums.get(enum_ref)
        if enum_def is None or enum_def.storage_kind != "flags":
            return value

        allowed = self._allowed_flag_members(enum_ref, drop_unknown_flags)
        allowed_values = self._allowed_flag_values(enum_ref, drop_unknown_flags)
        if isinstance(value, list):
            cleaned: list[Any] = []
            dropped: list[Any] = []
            for item in value:
                if isinstance(item, int) and not isinstance(item, bool):
                    if item in allowed_values:
                        cleaned.append(item)
                    else:
                        dropped.append(item)
                    continue
                if not isinstance(item, str):
                    dropped.append(item)
                    continue
                if item.startswith("0x") or _enum_member_key(item) not in allowed:
                    dropped.append(item)
                    continue
                cleaned.append(item)
            if dropped:
                decisions.append(
                    SchemaDecision(
                        record_type,
                        field_path,
                        "drop",
                        "invalid_target_flag_member",
                        dropped,
                    )
                )
            return cleaned if cleaned else _DROP

        if isinstance(value, str):
            if value.startswith("0x") or _enum_member_key(value) not in allowed:
                decisions.append(
                    SchemaDecision(
                        record_type,
                        field_path,
                        "drop",
                        "invalid_target_flag_member",
                        value,
                    )
                )
                return _DROP
        if isinstance(value, int) and not isinstance(value, bool):
            allowed_mask = 0
            for flag_value in allowed_values:
                allowed_mask |= flag_value
            cleaned = value & allowed_mask
            if cleaned != value:
                decisions.append(
                    SchemaDecision(
                        record_type,
                        field_path,
                        "drop",
                        "invalid_target_flag_member",
                        value & ~allowed_mask,
                    )
                )
            return cleaned if cleaned else _DROP
        return value

    def _allowed_flag_members(
        self,
        enum_ref: str,
        drop_unknown_flags: bool,
    ) -> set[str]:
        enum_def = self.schema.enums.get(enum_ref)
        if enum_def is None:
            return set()
        allowed: set[str] = set()

        def add(label: str) -> None:
            if drop_unknown_flags and _is_unknown_member(label):
                return
            allowed.add(_enum_member_key(label))

        for _value, token in enum_def.values:
            add(token)
        for _value, label in enum_def.labels:
            add(label)
        for legacy, canonical in enum_def.aliases:
            if _enum_member_key(canonical) in allowed:
                add(legacy)
        return allowed

    def _allowed_flag_values(
        self,
        enum_ref: str,
        drop_unknown_flags: bool,
    ) -> set[int]:
        enum_def = self.schema.enums.get(enum_ref)
        if enum_def is None:
            return set()
        values: set[int] = set()
        for value, token in enum_def.values:
            if drop_unknown_flags and _is_unknown_member(token):
                continue
            label = enum_def.label_for_value(value) or token
            if drop_unknown_flags and _is_unknown_member(label):
                continue
            values.add(value)
        return values

    def complete_struct_defaults(
        self,
        record_type: str,
        field_key: str,
        value: dict[str, Any],
    ) -> tuple[dict[str, Any], list[SchemaDecision]]:
        subrecord = self._subrecord_for_key(record_type, field_key)
        if subrecord is None or not subrecord.fields:
            return dict(value), []

        completed = dict(value)
        decisions: list[SchemaDecision] = []
        for field in subrecord.fields:
            key = _field_key(field)
            if key in completed:
                continue
            default = _field_default(field)
            if default is None:
                continue
            completed[key] = default
            decisions.append(
                SchemaDecision(
                    record_type,
                    field_key,
                    "default",
                    "schema_default",
                    {key: default},
                )
            )
        return completed, decisions


@lru_cache(maxsize=None)
def get_schema_surface(game: str) -> SchemaSurface:
    return SchemaSurface(game)
