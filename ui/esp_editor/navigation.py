"""Pure presentation helpers; file IDs remain the edit/native API identity."""
from __future__ import annotations

from dataclasses import dataclass


SEARCH_FIELDS = ("All indexed fields", "Editor ID", "FormID", "Name", "Record type")
RECORD_SORTS = ("FormID", "Editor ID", "Record type")


def matches(record, text: str, signature: str, search_field: str, type_labels=None) -> bool:
    if signature and record.signature.upper() != signature:
        return False
    if not text:
        return True
    editor_id = (record.editor_id or "").casefold()
    form_id = f"{record.form_id:08X}".casefold()
    type_text = f"{record.signature} {(type_labels or {}).get(record.signature, '')}".casefold()
    if search_field == "FormID":
        return text.removeprefix("0x") in form_id
    if search_field == "Editor ID":
        return text in editor_id
    if search_field == "Record type":
        return text in type_text
    return text in editor_id or text.removeprefix("0x") in form_id or text in type_text


def sort_records(records, mode: str, descending: bool):
    def key(record):
        stable = (int(record.form_id), record.signature, (record.editor_id or "").casefold())
        if mode == "Editor ID":
            return ((record.editor_id or "").casefold(), *stable)
        if mode == "Record type":
            return (record.signature, *stable)
        return stable
    return sorted(records, key=key, reverse=descending)


@dataclass(frozen=True)
class SessionSlot:
    label: str
    base: int
    object_mask: int


def session_slots(plugins, flags) -> dict[int, SessionSlot]:
    """Compute engine-style slots for this session's order, including ESL/SF medium.

    This is a projection of the editor order, not evidence of the game's active
    Plugins.txt order. Header flags, not extension, determine light/medium status.
    """
    counters = {}
    slots = {}
    for plugin in sorted(plugins, key=lambda p: p.load_order_index):
        value = flags.get(plugin.handle, 0)
        full, light, medium = counters.get(plugin.game, (0, 0, 0))
        light_flag = 0x100 if plugin.game == "starfield" else 0x200
        if plugin.game in ("fo4", "skyrimse", "starfield") and value & light_flag:
            if light < 0x1000:
                slots[plugin.handle] = SessionSlot(f"FE:{light:03X}", 0xFE000000 | light << 12, 0xFFF)
            light += 1
        elif plugin.game == "starfield" and value & 0x400:
            if medium < 0x100:
                slots[plugin.handle] = SessionSlot(f"FD:{medium:02X}", 0xFD000000 | medium << 16, 0xFFFF)
            medium += 1
        else:
            maximum = 0xFD if plugin.game == "starfield" else 0xFE if plugin.game in ("fo4", "skyrimse") else 0xFF
            if full < maximum:
                slots[plugin.handle] = SessionSlot(f"{full:02X}", full << 24, 0xFFFFFF)
            full += 1
        counters[plugin.game] = (full, light, medium)
    return slots


@dataclass(frozen=True)
class RecordIdentity:
    file_id: str
    owner: str | None
    object_id: int
    session_id: str | None


def record_identity(plugin, form_id: int, masters, plugins, slots) -> RecordIdentity:
    index = (int(form_id) >> 24) & 255
    owner = plugin.plugin_name if index in (len(masters), 255) else masters[index] if index < len(masters) else None
    object_id = int(form_id) & 0xFFFFFF
    origin = next((p for p in plugins if owner and p.plugin_name.casefold() == owner.casefold() and p.game == plugin.game), None)
    slot = slots.get(origin.handle) if origin is not None else None
    resolved = f"{slot.base | object_id:08X}" if slot and object_id <= slot.object_mask else None
    return RecordIdentity(f"{form_id:08X}", owner, object_id, resolved)
