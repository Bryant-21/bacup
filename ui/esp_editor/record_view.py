"""Small, file-relative record snapshots for the editor (never a plugin dump)."""
from __future__ import annotations

import math
import struct
from dataclasses import dataclass, field
from types import SimpleNamespace

from creation_lib.esp.editor.fields import Field, FieldKind, decode_record, _NUMERIC_KIND
from creation_lib.esp.schema import get_schema
from creation_lib.esp.native_runtime import (
    plugin_handle_get, plugin_handle_record_flags,
    plugin_handle_record_subrecords, plugin_handle_record_summary,
)


@dataclass
class RecordDocument:
    handle: int
    plugin_name: str
    game: str
    summary: object
    subrecords: list[tuple[str, bytes, str | None]]
    fields: list[Field]
    masters: tuple[str, ...]
    flags: int = 0
    edit_errors: list[str] = field(default_factory=list)
    localized_values: dict[int, str] = field(default_factory=dict)
    is_localized: bool = False
    plugin_flags: int = 0

    @property
    def is_partial(self) -> bool:
        spec = get_schema(self.game).records.get(self.summary.signature)
        flags = spec.record_flags if spec else None
        return bool(flags and any(
            bit.name.casefold() == "partial form" and self.flags & (1 << bit.bit)
            for bit in flags.bits
        ))

    def form_key(self, value: int) -> str:
        if not value:
            return "<null>"
        index = (value >> 24) & 255
        if index in (len(self.masters), 255):
            origin = self.plugin_name
        elif index < len(self.masters):
            origin = self.masters[index]
        else:
            return f"<unresolved {value:08X}>"
        return f"{value & 0xFFFFFF:06X}:{origin.casefold()}"


def load_document(plugin, summary) -> RecordDocument:
    payload = plugin_handle_record_subrecords(plugin.handle, summary.form_id)
    if payload is None:
        raise ValueError(f"Record {summary.form_id:08X} is no longer available")
    is_localized = bool(plugin_handle_get(plugin.handle, "is_localized"))
    fields = decode_record(
        SimpleNamespace(signature=summary.signature, subrecords=[
            SimpleNamespace(signature=sig, data=data) for sig, data, _ in payload
        ]),
        game=plugin.game,
        is_localized=is_localized,
    )
    document = RecordDocument(
        plugin.handle, plugin.plugin_name, plugin.game, summary, payload, fields,
        tuple(plugin_handle_get(plugin.handle, "masters") or []),
        int(plugin_handle_record_flags(plugin.handle, summary.form_id) or 0),
    )
    document.is_localized = is_localized
    document.plugin_flags = int(plugin_handle_get(plugin.handle, "header_flags") or 0)
    document.edit_errors = [edit_reason(fld, is_localized=is_localized) for fld in fields]
    from creation_lib.esp.native_runtime import plugin_handle_call
    for fld in fields:
        if fld.kind == FieldKind.LSTRING:
            value = int(fld.value or 0)
            document.localized_values[value] = str(plugin_handle_call(plugin.handle, "resolve_string", value, "") or "")
    return document


def load_comparison(plugins, selected_plugin, summary) -> list[RecordDocument]:
    selected = load_document(selected_plugin, summary)
    key = selected.form_key(summary.form_id)
    origin = key.partition(":")[2]
    if not origin:
        return [selected]
    documents = []
    for plugin in sorted(plugins, key=lambda p: p.load_order_index):
        if plugin.game != selected.game:
            continue
        if plugin.handle == selected.handle:
            documents.append(selected)
            continue
        names = [*(plugin_handle_get(plugin.handle, "masters") or []), plugin.plugin_name]
        index = next((i for i, name in enumerate(names) if name.casefold() == origin), None)
        if index is None:
            continue
        local_id = (index << 24) | (summary.form_id & 0xFFFFFF)
        other = plugin_handle_record_summary(plugin.handle, local_id)
        if (other is not None and other.signature == summary.signature
                and other.form_id == local_id):
            documents.append(load_document(plugin, other))
    return documents


def encode_edit(fld: Field) -> bytes:
    """Strict encoding: preserve opaque tails, reject overflow and lossy text.

    Unlike the permissive inspection codecs, this must never silently keep the
    old bytes on an invalid input, truncate a string, or skip an invalid row.
    """
    spec = fld.spec
    codec = (spec.codec or "").lower() if spec else ""
    if fld.kind in (FieldKind.INT, FieldKind.ENUM, FieldKind.FLAGSET, FieldKind.FORMID):
        fmt = "<" + _NUMERIC_KIND.get(codec, (None, "I"))[1]
        size = struct.calcsize(fmt)
        return struct.pack(fmt, int(fld.value)) + fld.raw[size:]
    if fld.kind == FieldKind.FLOAT:
        value = float(fld.value)
        if not math.isfinite(value):
            raise ValueError("Enter a finite number")
        fmt = "<d" if codec in ("float64", "f64") else "<f"
        return struct.pack(fmt, value) + fld.raw[struct.calcsize(fmt):]
    if fld.kind == FieldKind.STRING:
        text = str(fld.value)
        if "\x00" in text:
            raise ValueError("Text cannot contain a null character")
        data = text.encode("cp1252", errors="strict")
        if codec == "lenstring16":
            return len(data).to_bytes(2, "little") + data
        if codec.startswith("fixed_string:"):
            width = int(codec.partition(":")[2])
            if len(data) > width:
                raise ValueError(f"Text must fit in {width} bytes")
            return data.ljust(width, b"\x00") + fld.raw[width:]
        return data + b"\x00"
    if fld.kind == FieldKind.STRUCT:
        values = [c.value for c in fld.components] if fld.components else list(fld.value)
        if any(isinstance(v, float) and not math.isfinite(v) for v in values):
            raise ValueError("Enter finite numbers")
        packed = struct.pack(fld.struct_layout, *values)
        return packed + fld.raw[len(packed):]
    if fld.kind == FieldKind.ARRAY:
        layout = fld.element_layout
        if not layout:
            raise ValueError("This array has no supported element layout")
        size = struct.calcsize(layout)
        if len(fld.raw) % size:
            raise ValueError("Array contains an opaque prefix or incomplete element")
        values = [[c.value for c in row] for row in fld.rows]
        if any(isinstance(v, float) and not math.isfinite(v) for row in values for v in row):
            raise ValueError("Enter finite numbers")
        return b"".join(struct.pack(layout, *row) for row in values)
    if fld.kind == FieldKind.BYTES:
        return bytes(fld.value)
    if fld.kind == FieldKind.EMPTY:
        return b""
    raise ValueError("This field requires a dedicated editor")


def edit_reason(fld: Field, *, is_localized: bool = False) -> str:
    """Gate ambiguous layouts rather than enable a misleading generic editor."""
    if fld.kind == FieldKind.LSTRING or (is_localized and fld.spec and fld.spec.localized):
        return "Localized string ID; edit the string table with a dedicated editor."
    spec = fld.spec
    if spec and (spec.union_selector or spec.union_variants or spec.presence_conditions):
        return "Conditional layout; shown as read only to preserve its payload."
    if spec and spec.fields and any(
        fs.nested_fields or fs.union_selector or fs.union_variants or fs.presence_conditions or fs.array
        for fs in spec.fields
    ):
        return "Nested variable layout; requires a dedicated editor."
    if spec and fld.kind == FieldKind.STRUCT and len(spec.fields or ()) > len(fld.components):
        return "Schema includes variable members; requires a dedicated editor."
    if spec and spec.array and (spec.array.count_codec or spec.array.count_field):
        return "Counted layout; requires a dedicated array editor."
    try:
        if encode_edit(fld) != fld.raw:
            return "Schema does not fully describe these bytes; shown as read only."
    except (ValueError, TypeError, struct.error, OverflowError):
        return "Unsupported layout; shown as read only."
    return ""


def winning_document(documents: list[RecordDocument]) -> RecordDocument:
    # xEdit GetWinningOverride skips partial forms. A deleted full record still
    # wins; deletion is not permission to fall back to an earlier payload.
    return next((d for d in reversed(documents) if not d.is_partial), documents[0])


def field_text(fld: Field, document: RecordDocument) -> str:
    if fld.kind == FieldKind.FORMID:
        return document.form_key(int(fld.value or 0))
    if fld.kind == FieldKind.LSTRING:
        value = int(fld.value or 0)
        return document.localized_values.get(value) or f"<unresolved string #{value}>"
    if fld.kind in (FieldKind.ENUM, FieldKind.FLAGSET):
        options = fld.enum_options or fld.flag_options
        current = int(fld.value or 0)
        if fld.kind == FieldKind.ENUM:
            label = next((label for value, label in options if value == current), "Unknown")
            return f"{label} ({current})"
        labels = [label for bit, label in options if bit and current & bit == bit]
        return f"{', '.join(labels) or 'None'} (0x{current:X})"
    if fld.components:
        return "; ".join(f"{c.name}: {field_text(c, document)}" for c in fld.components)
    if fld.kind == FieldKind.ARRAY:
        return f"{len(fld.rows)} entries"
    if fld.kind == FieldKind.BYTES:
        return fld.raw[:32].hex() + (f"... ({len(fld.raw)} bytes)" if len(fld.raw) > 32 else "")
    return str(fld.value)


def comparison_rows(documents: list[RecordDocument]):
    """Align repeated subrecords and named members without merging occurrences."""
    columns = []
    labels = {}
    for doc in documents:
        values = {("header",): f"0x{doc.flags:08X}"}
        labels[("header",)] = "Record flags"
        occurrences = {}
        for fld in doc.fields:
            occurrence = occurrences.get(fld.signature, 0)
            occurrences[fld.signature] = occurrence + 1
            key = (fld.signature, occurrence)
            label = f"{fld.name} ({fld.signature})" + (f" [{occurrence + 1}]" if occurrence else "")
            values[key] = field_text(fld, doc)
            labels.setdefault(key, label)
            # Include raw bytes in equality keys for opaque/array data; summaries
            # such as '2 entries' must not hide differing values.
            if fld.rows:
                for ri, row in enumerate(fld.rows):
                    for ci, comp in enumerate(row):
                        member_key = (*key, ri, ci)
                        labels.setdefault(member_key, f"  [{ri}] {comp.name}")
                        values[member_key] = field_text(comp, doc)
            elif fld.components:
                for ci, comp in enumerate(fld.components):
                    member_key = (*key, ci)
                    labels.setdefault(member_key, f"  {comp.name}")
                    values[member_key] = field_text(comp, doc)
            if fld.kind == FieldKind.BYTES:
                values[key] = fld.raw.hex()
        columns.append(values)
    return [(key, label, [c.get(key) for c in columns]) for key, label in labels.items()]
