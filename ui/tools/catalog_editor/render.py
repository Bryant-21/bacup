"""Schema -> imgui widgets for the catalog editor.

``field_kind``, ``default_value``, ``summary`` and ``walk_fields`` are pure; ``draw_value`` is the only
imgui entry point. Values are never mutated in place: every draw returns ``(changed, new_value)``.

Schema vocabulary beyond JSON Schema: ``title``/``description`` (label and (?) help), ``x-asset``,
``x-enum-labels``, ``x-flags``, ``x-columns``, ``x-key-title``, ``x-key-form-type``, ``x-widget``,
``x-form-type`` and ``x-ref``. Every shape has a real control; a schema that says nothing about a value
gets a structured tree editor, never a JSON text box.
"""

from __future__ import annotations

import re
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from functools import lru_cache
from typing import Any

import json

from creation_lib.catalog.plugin_catalog import SCHEMAS_DIR
from ui.tools.catalog_editor.model import resolve_ref

PLACEHOLDER_FORM = "Plugin.esp:000001"
_FORMKEY_PATTERN_MARK = ":[0-9A-Fa-f]"
_SCALARS = ("string", "int", "number", "bool", "enum", "formkey", "ref", "asset", "flags")
_INLINE_HELP = (*_SCALARS, "variant", "tuple")
_MAX_TUPLE = 8
_SLIDER_SPAN = 1_000_000


# --- schema helpers (pure) ---------------------------------------------------------------------


def humanize(key: str) -> str:
    """``max_hp`` / ``maxHp`` -> ``Max hp``."""
    words = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", str(key)).replace("_", " ").replace("-", " ").strip()
    return words[:1].upper() + words[1:]


def label_of(name: str, node: dict) -> str:
    return node.get("title") or humanize(name)


def _resolve(schema: dict, node: Any) -> dict:
    return resolve_ref(schema, node) if isinstance(node, dict) else {}


def _is_null(node: dict) -> bool:
    return node.get("type") == "null" or node.get("const") == ""


def _is_remove_marker(node: dict) -> bool:
    return node.get("required") == ["remove"]


def _type_of(node: dict) -> str | None:
    kind = node.get("type")
    if isinstance(kind, list):
        kind = [k for k in kind if k != "null"]
        kind = kind[0] if len(kind) == 1 else None
    if kind is None:
        if "properties" in node:
            return "object"
        if "items" in node or "prefixItems" in node:
            return "array"
    return kind


def _simplify(schema: dict, node: Any) -> dict:
    """Resolve refs and collapse ``oneOf/anyOf`` with one real branch (the rest null or remove markers)."""
    n = _resolve(schema, node)
    if "allOf" in n and all(isinstance(p, dict) for p in n["allOf"]):
        merged: dict = {k: v for k, v in n.items() if k != "allOf"}
        for part in n["allOf"]:
            for k, v in _resolve(schema, part).items():
                if k in ("properties", "required") and k in merged:
                    merged[k] = {**merged[k], **v} if k == "properties" else [*merged[k], *v]
                else:
                    merged.setdefault(k, v)
        return _simplify(schema, merged)
    for key in ("oneOf", "anyOf"):
        if key not in n:
            continue
        keep = [b for b in (_resolve(schema, b) for b in n[key]) if not _is_null(b) and not _is_remove_marker(b)]
        rest = {k: v for k, v in n.items() if k != key}
        if len(keep) == 1:
            return _simplify(schema, {**keep[0], **rest})
        if keep and all("const" in b or "enum" in b for b in keep):
            values = [v for b in keep for v in (b["enum"] if "enum" in b else [b["const"]])]
            return {**rest, "enum": values}
        return n
    return n


def _variants(schema: dict, n: dict) -> list[dict]:
    """The selectable branches of a ``oneOf/anyOf`` that did not collapse (null branches included)."""
    for key in ("oneOf", "anyOf"):
        if key in n:
            branches = [_resolve(schema, b) for b in n[key]]
            branches = [b for b in branches if not _is_remove_marker(b)]
            return branches if len(branches) > 1 else []
    return []


def _tuple_items(schema: dict, n: dict) -> list[dict] | None:
    """Per-position item nodes of a fixed-length array, else None."""
    if _type_of(n) != "array":
        return None
    prefix = n.get("prefixItems")
    if isinstance(prefix, list) and prefix:
        return [_child(schema, n, p) for p in prefix]
    items = n.get("items")
    if isinstance(items, list) and items:
        return [_child(schema, n, p) for p in items]
    low = n.get("minItems")
    if isinstance(items, dict) and low is not None and low == n.get("maxItems") and 1 <= low <= _MAX_TUPLE:
        return [_child(schema, n, items)] * int(low)
    return None


_NO_EMPTY = object()


def _empty_value(schema: dict, node: Any) -> Any:
    """What "no value" is for this field: None for a null branch, "" for a const-"" branch, else _NO_EMPTY."""
    n = _resolve(schema, node)
    if isinstance(n.get("type"), list) and "null" in n["type"]:
        return None
    for key in ("oneOf", "anyOf"):
        for branch in n.get(key, []):
            branch = _resolve(schema, branch)
            if branch.get("type") == "null":
                return None
            if branch.get("const") == "":
                return ""
    return _NO_EMPTY


def _is_formkey(n: dict) -> bool:
    return n.get("type") == "string" and _FORMKEY_PATTERN_MARK in str(n.get("pattern", ""))


def _child(schema: dict, parent: dict, node: Any) -> dict:
    """A child node; a form-key child without its own ``x-form-type`` inherits the parent's."""
    n = _simplify(schema, node)
    if "x-form-type" in parent and "x-form-type" not in n and _is_formkey(n):
        n = {**n, "x-form-type": parent["x-form-type"]}
    return n


def _map_value_node(schema: dict, n: dict) -> dict:
    extra = n.get("additionalProperties")
    if isinstance(extra, dict) and extra:
        return _child(schema, n, extra)
    patterns = n.get("patternProperties")
    if isinstance(patterns, dict) and patterns:
        return _child(schema, n, next(iter(patterns.values())))
    return {}


def _is_map(n: dict) -> bool:
    extra = n.get("additionalProperties")
    return bool((isinstance(extra, dict) and extra) or n.get("patternProperties") or n.get("propertyNames")
                or extra is True)


def field_kind(node: dict, schema: dict) -> str:
    """Never ``raw``: a shape the schema does not describe is a ``tree`` (structured editor)."""
    n = _simplify(schema, node)
    if "x-widget" in n:
        return f"widget:{n['x-widget']}"
    if _variants(schema, n):
        return "variant"
    if n.get("x-opaque"):
        return "tree"
    if "enum" in n or "const" in n:
        return "enum"
    kind = _type_of(n)
    if kind == "string":
        if n.get("x-asset"):
            return "asset"
        if n.get("x-ref"):
            return "ref"
        return "formkey" if _is_formkey(n) else "string"
    if kind == "integer":
        return "flags" if n.get("x-flags") else "int"
    if kind == "number":
        return "number"
    if kind == "boolean":
        return "bool"
    if kind == "object":
        if "properties" in n:
            return "object"
        return "map" if _is_map(n) else "tree"
    if kind == "array":
        if _tuple_items(schema, n) is not None:
            return "tuple"
        if not isinstance(n.get("items"), dict) or not n["items"]:
            return "tree"
        item = field_kind(_child(schema, n, n["items"]), schema)
        if item == "formkey":
            return "formkey_list"
        if item in ("string", "int", "number", "bool", "enum", "asset", "ref", "flags"):
            return "scalar_list"
        if item == "tuple":
            return "tuple_list"
        return "object_list" if item == "object" else "list"
    return "tree"


def _required(n: dict) -> list[str]:
    names = list(n.get("required", []))
    if n.get("if") == {"not": {"required": ["remove"]}}:
        names += n.get("then", {}).get("required", [])
    for part in n.get("allOf", []):
        if isinstance(part, dict):
            names += _required(part)
    return list(dict.fromkeys(names))


def _default_curve(schema: dict, n: dict) -> list:
    items = _simplify(schema, n.get("items", {}))
    count = max(2, int(n.get("minItems", 0)))
    if _type_of(items) == "object":
        low = _simplify(schema, items.get("properties", {}).get("y", {})).get("minimum", 0)
        return [{"x": float(i), "y": float(low)} for i in range(count)]
    return [[float(i), 0.0] for i in range(count)]


def _string_default(n: dict) -> str:
    text = "x" * int(n.get("minLength", 0))
    pattern = n.get("pattern")
    if not pattern or re.search(pattern, text):
        return text
    guess = re.search(r"\[\^?(.)[^\]]*\]\{(\d+)", pattern)
    candidates = [c * int(guess.group(2)) for c in [guess.group(1)]] if guess else []
    candidates += ["0", "00000000", "a", "A", "Plugin.esp", "Plugin.esp:000001"]
    return next((c for c in candidates if re.search(pattern, c)), text)


def _bounds(n: dict) -> tuple[float | None, float | None]:
    lows = [n[k] + (1 if k == "exclusiveMinimum" else 0) for k in ("minimum", "exclusiveMinimum") if k in n]
    highs = [n[k] - (1 if k == "exclusiveMaximum" else 0) for k in ("maximum", "exclusiveMaximum") if k in n]
    return (max(lows) if lows else None), (min(highs) if highs else None)


def _variant_default(schema: dict, branch: dict) -> Any:
    value = default_value(branch, schema)
    if isinstance(value, dict):
        for name, child in branch.get("properties", {}).items():
            resolved = _resolve(schema, child)
            if "const" in resolved:
                value[name] = resolved["const"]
    return value


def default_value(node: dict, schema: dict, keys: Any = ()) -> Any:
    """Smallest value that validates against ``node``; ``keys`` are extra object fields to include."""
    n = _simplify(schema, node)
    kind = field_kind(node, schema)
    if kind == "widget:conditions":
        return []
    if kind == "widget:curve":
        return _default_curve(schema, n)
    if kind == "variant":
        for branch in _variants(schema, n):
            if not _is_null(branch):
                return _variant_default(schema, branch)
        return None
    if kind == "enum":
        return n["enum"][0] if "enum" in n else n["const"]
    if kind in ("string", "asset"):
        return _string_default(n)
    if kind in ("int", "number", "flags"):
        low, _ = _bounds(n)
        if low is None:
            return 0
        return low if kind == "number" else int(low)
    if kind == "bool":
        return False
    if kind in ("formkey", "ref"):
        empty = _empty_value(schema, node)
        if empty is not _NO_EMPTY:
            return empty
        return PLACEHOLDER_FORM if _is_formkey(n) else ""
    if kind == "object":
        out: dict[str, Any] = {}
        props = n.get("properties", {})
        for name in [*_required(n), *[k for k in keys if isinstance(k, str)]]:
            if name in props and name not in out:
                out[name] = default_value(_child(schema, n, props[name]), schema)
        return out
    if kind == "map":
        return {}
    if kind == "tuple":
        return [default_value(item, schema) for item in _tuple_items(schema, n) or []]
    if kind in ("scalar_list", "formkey_list", "object_list", "list", "tuple_list"):
        return [default_value(_child(schema, n, n["items"]), schema) for _ in range(int(n.get("minItems", 0)))]
    if kind == "tree" and _type_of(n) == "object":
        return {name: "" for name in _required(n)}
    return {"object": {}, "array": [], "string": "", "integer": 0, "number": 0.0, "boolean": False,
            "null": None}.get(_type_of(n) or "", "")


def walk_fields(node: dict, schema: dict, path: str = "", _depth: int = 0) -> Iterator[tuple[str, dict, str]]:
    """Every (path, resolved node, kind) below ``node``, descending exactly as the renderer does."""
    n = _simplify(schema, node)
    kind = field_kind(node, schema)
    yield path, n, kind
    if _depth > 12:
        return
    if kind == "object":
        for name, child in n["properties"].items():
            yield from walk_fields(_child(schema, n, child), schema, f"{path}.{name}" if path else name, _depth + 1)
    elif kind in ("object_list", "list", "scalar_list", "formkey_list", "tuple_list"):
        yield from walk_fields(_child(schema, n, n["items"]), schema, f"{path}[]", _depth + 1)
    elif kind == "tuple":
        for index, item in enumerate(_tuple_items(schema, n) or []):
            yield from walk_fields(item, schema, f"{path}[{index}]", _depth + 1)
    elif kind == "map":
        yield from walk_fields(_map_value_node(schema, n), schema, f"{path}{{}}", _depth + 1)
    elif kind == "variant":
        for index, branch in enumerate(_variants(schema, n)):
            yield from walk_fields(branch, schema, f"{path}|{index}", _depth + 1)


@lru_cache(maxsize=1)
def _registry():
    from referencing import Registry, Resource

    resources = []
    for path in SCHEMAS_DIR.glob("*.schema.json"):
        doc = json.loads(path.read_text(encoding="utf-8"))
        resources.append((doc["$id"], Resource.from_contents(doc)))
    return Registry().with_resources(resources)


def validation_errors(node: dict, schema: dict, value: Any) -> list[str]:
    """jsonschema errors of ``value`` against ``node`` (refs resolved against ``schema``'s file)."""
    import jsonschema

    wrapper = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": schema.get("$id", "https://modbox21.local/catalog_schemas/_node.schema.json"),
        "$defs": schema.get("$defs", {}),
        **_resolve(schema, node),
    }
    validator = jsonschema.Draft202012Validator(wrapper, registry=_registry())
    return [e.message for e in validator.iter_errors(value)]


def summary(node: dict, entry: Any, ctx_schema: dict, limit: int = 3) -> str:
    if not isinstance(entry, dict):
        return "" if entry is None else str(entry)[:80]
    if entry.get("remove"):
        return "(removed)"
    n = _simplify(ctx_schema, node)
    parts: list[str] = []
    for name, child in n.get("properties", {}).items():
        if name == "remove" or name not in entry or len(parts) >= limit:
            continue
        kind = field_kind(_child(ctx_schema, n, child), ctx_schema)
        if kind in _SCALARS and entry[name] not in (None, ""):
            parts.append(f"{name}={entry[name]}")
    if not parts:
        for name, value in entry.items():
            if isinstance(value, list) and name != "remove":
                parts.append(f"{name} ({len(value)})")
                break
    return "  ".join(parts)


def _preview(value: Any) -> str:
    if isinstance(value, list):
        return f"{len(value)} item(s)"
    if isinstance(value, dict):
        parts = [f"{k}={v}" for k, v in value.items() if not isinstance(v, (dict, list))][:3]
        return "  ".join(parts) or f"{len(value)} field(s)"
    return "" if value is None else str(value)[:60]


# --- variants and maps (pure) ------------------------------------------------------------------

_TYPE_LABELS = {"string": "Text", "integer": "Whole number", "number": "Number", "boolean": "Yes / No",
                "array": "List", "object": "Group", "null": "None"}


def variant_label(schema: dict, branch: dict) -> str:
    if branch.get("title"):
        return branch["title"]
    if _is_null(branch):
        return "None"
    if "const" in branch:
        return str(branch["const"])
    for child in branch.get("properties", {}).values():
        resolved = _resolve(schema, child)
        if "const" in resolved:
            return str(resolved["const"])
    if _is_formkey(branch):
        return "Form"
    kind = _type_of(branch) or ("object" if "properties" in branch else "")
    return _TYPE_LABELS.get(kind, kind or "Value")


def _type_matches(kind: Any, value: Any) -> bool:
    if isinstance(kind, list):
        return any(_type_matches(k, value) for k in kind)
    return {
        "null": value is None, "string": isinstance(value, str), "boolean": isinstance(value, bool),
        "integer": isinstance(value, int) and not isinstance(value, bool),
        "number": isinstance(value, (int, float)) and not isinstance(value, bool),
        "array": isinstance(value, list), "object": isinstance(value, dict),
    }.get(kind, True)


def branch_matches(schema: dict, branch: dict, value: Any) -> bool:
    if branch.get("type") == "null":
        return value is None
    if "const" in branch:
        return value == branch["const"] and isinstance(value, bool) == isinstance(branch["const"], bool)
    if "enum" in branch:
        return value in branch["enum"]
    kind = branch.get("type") or ("object" if "properties" in branch else "array" if "items" in branch else None)
    if not _type_matches(kind, value):
        return False
    pattern = branch.get("pattern")
    if isinstance(value, str) and pattern and not re.search(pattern, value):
        return False
    if isinstance(value, dict):
        if any(name not in value for name in branch.get("required", [])):
            return False
        for name, child in branch.get("properties", {}).items():
            resolved = _resolve(schema, child)
            if "const" in resolved and name in value and value[name] != resolved["const"]:
                return False
    return True


def variant_index(schema: dict, branches: list[dict], value: Any) -> int:
    for index, branch in enumerate(branches):
        if branch_matches(schema, branch, value):
            return index
    return 0


def rename_key(mapping: dict, old: str, new: str) -> dict:
    """``mapping`` with ``old`` renamed to ``new``, keeping its position."""
    return {(new if key == old else key): value for key, value in mapping.items()}


def flag_bits(n: dict) -> list[tuple[int, str]]:
    return sorted((int(bit), label) for bit, label in n.get("x-flags", {}).items())


def toggle_flag(value: int, bit: int, on: bool) -> int:
    return (value | bit) if on else (value & ~bit)


# --- drawing -----------------------------------------------------------------------------------


@dataclass
class RenderContext:
    schema: dict
    forms: Any
    picker: Any
    section_keys: Callable[[str], list[str]]
    read_only: bool = False
    assets: Any = None


_state: dict[str, Any] = {}
_TREE_TYPES = ["string", "integer", "number", "boolean", "null", "object", "array"]
_TREE_DEFAULTS = {"string": "", "integer": 0, "number": 0.0, "boolean": False, "null": None}


def _wrapped_tooltip(text: str) -> None:
    from imgui_bundle import imgui

    if imgui.begin_tooltip():
        imgui.push_text_wrap_pos(imgui.get_font_size() * 32)
        imgui.text_unformatted(text)
        imgui.pop_text_wrap_pos()
        imgui.end_tooltip()


def _help(n: dict) -> None:
    """A "(?)" after the previous item whose tooltip is the node's description."""
    from imgui_bundle import imgui

    text = n.get("description")
    if not text:
        return
    imgui.same_line()
    imgui.text_disabled("(?)")
    if imgui.is_item_hovered():
        _wrapped_tooltip(text)


def draw_value(label: str, node: dict, value: Any, ctx: RenderContext, id_path: Any) -> tuple[bool, Any]:
    from imgui_bundle import imgui

    path = str(id_path)
    n = _simplify(ctx.schema, node)
    kind = field_kind(node, ctx.schema)
    empty = _empty_value(ctx.schema, node)
    imgui.push_id(path or label or "root")
    if ctx.read_only:
        imgui.begin_disabled(True)
    try:
        if empty is None and kind not in ("formkey", "ref", "enum", "variant") and not kind.startswith("widget:"):
            result = _draw_nullable(label, n, value, ctx, path, kind, node)
        else:
            result = _draw_kind(label, n, value, ctx, path, kind, empty)
        if kind in _INLINE_HELP and value is not None:
            _help(n)
    finally:
        if ctx.read_only:
            imgui.end_disabled()
        imgui.pop_id()
    return result


def _draw_kind(label, n, value, ctx, path, kind, empty):
    if kind.startswith("widget:"):
        from ui.tools.catalog_editor.widgets import REGISTRY

        draw = REGISTRY.get(kind.split(":", 1)[1])
        return draw(label, n, value, ctx, path) if draw else _draw_tree(label, n, value, ctx, path)
    return _DRAWERS[kind](label, n, value, ctx, path, empty)


def _draw_nullable(label, n, value, ctx, path, kind, node):
    """A field that may be absent (null): a checkbox turns it on with a default or off to null."""
    from imgui_bundle import imgui

    toggled, has = imgui.checkbox("##has", value is not None)
    imgui.same_line()
    if toggled:
        return True, (default_value(node, ctx.schema) if has else None)
    if value is None:
        imgui.text_disabled(f"{label}: (not set)" if label else "(not set)")
        _help(n)
        return False, value
    return _draw_kind(label, n, value, ctx, path, kind, _NO_EMPTY)


def _draw_string(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    imgui.set_next_item_width(-160 if label else -1)
    changed, text = imgui.input_text(label or "##v", value if isinstance(value, str) else "")
    return changed, (text if changed else value)


def _draw_int(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    try:
        current = int(value)
    except (TypeError, ValueError):
        current = 0
    low, high = _bounds(n)
    if low is not None and high is not None and high - low <= _SLIDER_SPAN:
        imgui.set_next_item_width(240)
        changed, number = imgui.slider_int(label or "##v", min(max(current, int(low)), int(high)), int(low), int(high))
        return changed, (number if changed else value)
    imgui.set_next_item_width(160)
    changed, number = imgui.input_int(label or "##v", current)
    if not changed:
        return False, value
    if low is not None:
        number = max(int(low), number)
    if high is not None:
        number = min(int(high), number)
    return True, number


def _draw_number(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    current = float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else 0.0
    low, high = _bounds(n)
    if low is not None and high is not None:
        imgui.set_next_item_width(240)
        changed, number = imgui.slider_float(label or "##v", min(max(current, float(low)), float(high)),
                                             float(low), float(high), "%.3f")
        return changed, (number if changed else value)
    imgui.set_next_item_width(160)
    changed, number = imgui.input_double(label or "##v", current, 0.0, 0.0, "%g")
    if not changed:
        return False, value
    if low is not None:
        number = max(float(low), number)
    if high is not None:
        number = min(float(high), number)
    return True, number


def _draw_bool(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    changed, flag = imgui.checkbox(label or "##v", bool(value))
    return changed, (flag if changed else value)


def _draw_flags(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    current = int(value) if isinstance(value, int) and not isinstance(value, bool) else 0
    bits = flag_bits(n)
    out, changed = current, False
    imgui.text(f"{label}  = {current}" if label else f"= {current}")
    imgui.indent()
    for bit, text in bits:
        did, on = imgui.checkbox(f"{text}##bit{bit}", (current & bit) == bit)
        if did:
            out, changed = toggle_flag(out, bit, on), True
    imgui.unindent()
    return changed, (out if changed else value)


def _draw_enum(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    if "enum" not in n:
        imgui.text_disabled(f"{label}: {n['const']}" if label else str(n["const"]))
        return False, value
    options = n["enum"]
    labels = n.get("x-enum-labels") or {}
    shown = [str(labels.get(str(o), o)) for o in options]
    index = options.index(value) if value in options else -1
    imgui.set_next_item_width(200)
    changed, index = imgui.combo(label or "##v", index, shown)
    return changed, (options[index] if changed else value)


def _pick_text(value: Any, ctx: RenderContext) -> str:
    if not value:
        return "(none)"
    described = ctx.forms.describe(value) if ctx.forms else None
    return described.split(" [", 1)[0] if described else f"{value} (unresolved)"


def _draw_formkey(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    changed, new = False, value
    landed = ctx.picker.take(path)
    if landed is not None:
        changed, new = True, (landed if landed else ("" if empty is _NO_EMPTY else empty))
    shown = new if isinstance(new, str) else ""
    if imgui.button(f"{label + ': ' if label else ''}{_pick_text(shown, ctx)}##pick"):
        ctx.picker.request(path, n.get("x-form-type"), shown)
    if shown:
        if ctx.forms:
            imgui.set_item_tooltip(ctx.forms.describe(shown) or shown)
        imgui.same_line()
        if imgui.small_button("x##clear"):
            changed, new = True, ("" if empty is _NO_EMPTY else empty)
    return changed, new


def _draw_asset(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    from ui.tools.catalog_editor.assets import to_stored_path

    config = n["x-asset"] if isinstance(n["x-asset"], dict) else {}
    changed, new = False, value
    landed = ctx.assets.take(path) if ctx.assets is not None else None
    if landed is not None:
        changed, new = True, to_stored_path(landed, config.get("root", ""), config.get("strip_root", False))
    spacing = imgui.get_style().item_spacing.x
    browse = imgui.calc_text_size("Browse...").x + imgui.get_style().frame_padding.x * 2
    label_w = imgui.calc_text_size(label).x + spacing if label else 0
    imgui.set_next_item_width(-(browse + spacing + label_w))
    edited, text = imgui.input_text("##asset", new if isinstance(new, str) else "")
    if edited:
        changed, new = True, text
    imgui.same_line()
    if ctx.assets is None:
        imgui.begin_disabled(True)
    if imgui.button("Browse...##asset") and ctx.assets is not None:
        ctx.assets.request(path, config, new if isinstance(new, str) else "")
    if ctx.assets is None:
        imgui.end_disabled()
    if label:
        imgui.same_line()
        imgui.text_unformatted(label)
    return changed, new


def _draw_ref(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    options = list(ctx.section_keys(n["x-ref"]))
    if not options:
        draw = _draw_formkey if n.get("x-form-type") else _draw_string
        return draw(label, n, value, ctx, path, empty)
    current = value if isinstance(value, str) else ""
    if current and current not in options:
        options.append(current)
    imgui.set_next_item_width(300)
    changed, index = imgui.combo(label or "##v", options.index(current) if current in options else -1, options)
    return changed, (options[index] if changed else value)


def _draw_object(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    current = value if isinstance(value, dict) else {}
    if label:
        opened = imgui.tree_node_ex(label, imgui.TreeNodeFlags_.default_open)
        _help(n)
        if not opened:
            return False, value
    required = set(_required(n))
    out, changed = dict(current), False
    for name, child in n["properties"].items():
        if name == "remove":
            continue
        child_node = _child(ctx.schema, n, child)
        present = name in current
        faded = not present and name not in required
        if faded:
            imgui.push_style_var(imgui.StyleVar_.alpha, 0.55)
        did, new = draw_value(label_of(name, child_node), child_node,
                              current[name] if present else default_value(child_node, ctx.schema),
                              ctx, f"{path}.{name}")
        if faded:
            imgui.pop_style_var()
        if did:
            out[name], changed = new, True
        elif present and name not in required and not ctx.read_only:
            imgui.same_line()
            if imgui.small_button(f"unset##{name}"):
                del out[name]
                changed = True
    if label:
        imgui.tree_pop()
    return changed, (out if changed else value)


def _header(label: str, fallback: str, count: int, n: dict, suffix: str) -> bool:
    from imgui_bundle import imgui

    opened = imgui.tree_node_ex(f"{label or fallback} ({count})###{suffix}", imgui.TreeNodeFlags_.default_open)
    _help(n)
    return opened


def _draw_list(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    item_node = _child(ctx.schema, n, n["items"])
    out, changed = list(value) if isinstance(value, list) else [], False
    landed = ctx.picker.take(f"{path}+")
    if landed:
        out.append(landed)
        changed = True
    if _header(label, "Items", len(out), n, "list"):
        for index in range(len(out)):
            imgui.push_id(index)
            did, new = draw_value("", item_node, out[index], ctx, f"{path}[{index}]")
            if did:
                out[index], changed = new, True
            imgui.same_line()
            if imgui.small_button("x"):
                del out[index]
                changed = True
                imgui.pop_id()
                break
            imgui.pop_id()
        if imgui.small_button("+ Add"):
            if field_kind(item_node, ctx.schema) == "formkey":
                ctx.picker.request(f"{path}+", item_node.get("x-form-type"))
            else:
                out.append(default_value(item_node, ctx.schema))
                changed = True
        imgui.tree_pop()
    return changed, (out if changed else value)


def _draw_rows(label, n, value, ctx, path, empty):
    """Object (and other non-scalar) list: one collapsible row per item, with reorder and delete."""
    from imgui_bundle import imgui

    item_node = _child(ctx.schema, n, n["items"])
    out, changed = list(value) if isinstance(value, list) else [], False
    if _header(label, "Items", len(out), n, "list"):
        for index in range(len(out)):
            imgui.push_id(index)
            if imgui.small_button("^") and index:
                out[index - 1], out[index] = out[index], out[index - 1]
                changed = True
            imgui.same_line()
            if imgui.small_button("v") and index + 1 < len(out):
                out[index + 1], out[index] = out[index], out[index + 1]
                changed = True
            imgui.same_line()
            if imgui.small_button("x"):
                del out[index]
                changed = True
                imgui.pop_id()
                break
            imgui.same_line()
            text = summary(item_node, out[index], ctx.schema) if isinstance(out[index], dict) else _preview(out[index])
            if imgui.tree_node(f"{index + 1}. {text or '(empty)'}###row"):
                did, new = draw_value("", item_node, out[index], ctx, f"{path}[{index}]")
                if did:
                    out[index], changed = new, True
                imgui.tree_pop()
            imgui.pop_id()
        if imgui.small_button("+ Add"):
            out.append(default_value(item_node, ctx.schema, n.get("x-key", ())))
            changed = True
        imgui.tree_pop()
    return changed, (out if changed else value)


def _draw_tuple(label, n, value, ctx, path, empty):
    items = _tuple_items(ctx.schema, n) or []
    current = list(value) if isinstance(value, list) else []
    current += [default_value(item, ctx.schema) for item in items[len(current):]]
    columns = n.get("x-columns") or []
    out, changed = list(current), False
    from imgui_bundle import imgui

    if label:
        imgui.text_unformatted(label)
        _help(n)
        imgui.indent()
    for index, item in enumerate(items):
        name = columns[index] if index < len(columns) else item.get("title") or f"#{index + 1}"
        did, new = draw_value(name, item, current[index], ctx, f"{path}[{index}]")
        if did:
            out[index], changed = new, True
    if label:
        imgui.unindent()
    return changed, (out if changed else value)


def _draw_tuple_list(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    row_node = _child(ctx.schema, n, n["items"])
    items = _tuple_items(ctx.schema, row_node) or []
    columns = n.get("x-columns") or row_node.get("x-columns") or []
    out, changed = list(value) if isinstance(value, list) else [], False
    if _header(label, "Rows", len(out), n, "table"):
        flags = (imgui.TableFlags_.borders_inner_h | imgui.TableFlags_.row_bg | imgui.TableFlags_.resizable
                 | imgui.TableFlags_.sizing_stretch_same)
        if out and imgui.begin_table("##tuples", len(items) + 1, flags):
            for index, item in enumerate(items):
                imgui.table_setup_column(columns[index] if index < len(columns) else item.get("title") or f"#{index + 1}",
                                         imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("##remove", imgui.TableColumnFlags_.width_fixed, 28)
            imgui.table_headers_row()
            for row in range(len(out)):
                imgui.table_next_row()
                cells = list(out[row]) if isinstance(out[row], list) else []
                cells += [default_value(item, ctx.schema) for item in items[len(cells):]]
                for index, item in enumerate(items):
                    imgui.table_next_column()
                    imgui.push_id(row * 64 + index)
                    did, new = draw_value("", item, cells[index], ctx, f"{path}[{row}][{index}]")
                    imgui.pop_id()
                    if did:
                        cells[index] = new
                        out[row], changed = cells, True
                imgui.table_next_column()
                imgui.push_id(row * 64 + 63)
                removed = imgui.small_button("x")
                imgui.pop_id()
                if removed:
                    del out[row]
                    changed = True
                    break
            imgui.end_table()
        if imgui.small_button("+ Add row"):
            out.append(default_value(row_node, ctx.schema))
            changed = True
        imgui.tree_pop()
    return changed, (out if changed else value)


def _key_input(path: str, key: str, existing, width: float = 220) -> str | None:
    """Editable key text; returns the new key when the author finishes editing to a free, non-empty one."""
    from imgui_bundle import imgui

    state_key = f"{path}#rename:{key}"
    imgui.set_next_item_width(width)
    _, text = imgui.input_text("##key", _state.get(state_key, key))
    _state[state_key] = text
    if imgui.is_item_deactivated_after_edit():
        _state.pop(state_key, None)
        new = text.strip()
        if new and new != key and new not in existing:
            return new
    return None


def _draw_map(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    value_node = _map_value_node(ctx.schema, n)
    value_kind = field_kind(value_node, ctx.schema)
    key_node = _simplify(ctx.schema, n.get("propertyNames", {}))
    form_types = n.get("x-key-form-type") or key_node.get("x-form-type")
    key_is_form = bool(n.get("x-key-form-type")) or _is_formkey(key_node)
    key_options = key_node.get("enum")
    key_title = n.get("x-key-title") or "Name"
    out, changed = dict(value) if isinstance(value, dict) else {}, False

    def add(key: str) -> bool:
        if key and key not in out:
            out[key] = default_value(value_node, ctx.schema)
            return True
        return False

    landed = ctx.picker.take(f"{path}+")
    if landed:
        changed = add(landed)
    if _header(label, "Entries", len(out), n, "map"):
        if out:
            imgui.text_disabled(f"{key_title}  ->  value")
        for key in list(out):
            imgui.push_id(key)
            if imgui.small_button("x"):
                del out[key]
                changed = True
                imgui.pop_id()
                continue
            imgui.same_line()
            new_key = None
            if key_is_form:
                picked = ctx.picker.take(f"{path}~{key}")
                if picked and picked not in out:
                    new_key = picked
                if imgui.button(f"{_pick_text(key, ctx).replace(' (unresolved)', '')}##key"):
                    ctx.picker.request(f"{path}~{key}", form_types, key)
            elif key_options:
                imgui.text_unformatted(key)
            else:
                new_key = _key_input(path, key, out)
            if new_key is not None:
                out = rename_key(out, key, new_key)
                changed = True
                imgui.pop_id()
                break
            if value_kind in _SCALARS:
                imgui.same_line()
                did, new = draw_value("", value_node, out[key], ctx, f"{path}.{key}")
            else:
                imgui.indent()
                did, new = draw_value("", value_node, out[key], ctx, f"{path}.{key}")
                imgui.unindent()
            if did:
                out[key], changed = new, True
            imgui.pop_id()
        if key_is_form:
            if imgui.small_button(f"+ Add {key_title.lower()}"):
                ctx.picker.request(f"{path}+", form_types)
        elif key_options:
            free = [o for o in key_options if o not in out]
            if free:
                choice = _state.get(f"{path}#choice", 0) % len(free)
                imgui.set_next_item_width(240)
                _, choice = imgui.combo("##newkey", choice, [str(o) for o in free])
                _state[f"{path}#choice"] = choice
                imgui.same_line()
                if imgui.small_button("+ Add"):
                    changed = add(free[choice]) or changed
        else:
            imgui.set_next_item_width(240)
            _, text = imgui.input_text_with_hint("##newkey", f"new {key_title.lower()}", _state.get(f"{path}#k", ""))
            _state[f"{path}#k"] = text
            imgui.same_line()
            if imgui.small_button("+ Add") and add(text.strip()):
                _state[f"{path}#k"] = ""
                changed = True
        imgui.tree_pop()
    return changed, (out if changed else value)


def _draw_variant(label, n, value, ctx, path, empty):
    from imgui_bundle import imgui

    branches = _variants(ctx.schema, n)
    labels = [variant_label(ctx.schema, b) for b in branches]
    index = variant_index(ctx.schema, branches, value)
    if label:
        imgui.text_unformatted(label)
        imgui.same_line()
    imgui.set_next_item_width(150)
    picked, new_index = imgui.combo("##variant", index, labels)
    if picked and new_index != index:
        return True, _variant_default(ctx.schema, branches[new_index])
    branch = branches[index]
    if _is_null(branch):
        return False, value
    branch_kind = field_kind(branch, ctx.schema)
    if branch_kind in _SCALARS:
        imgui.same_line()
        return draw_value("", branch, value, ctx, f"{path}|{index}")
    imgui.indent()
    result = draw_value("", branch, value, ctx, f"{path}|{index}")
    imgui.unindent()
    return result


def _tree_type(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    return "object"


def _tree_default(kind: str) -> Any:
    return {"object": {}, "array": []}.get(kind) if kind in ("object", "array") else _TREE_DEFAULTS[kind]


def _draw_tree(label, n, value, ctx, path, empty=_NO_EMPTY):
    """Structured editor for a value whose shape the schema does not describe: pick the type, edit it."""
    from imgui_bundle import imgui

    locked = _type_of(n) if n else None
    kind = locked if locked in _TREE_TYPES else _tree_type(value)
    if kind == "object" and not isinstance(value, dict):
        value = {}
    if kind == "array" and not isinstance(value, list):
        value = []
    if locked not in _TREE_TYPES:
        imgui.set_next_item_width(100)
        picked, index = imgui.combo("##type", _TREE_TYPES.index(kind), _TREE_TYPES)
        if picked:
            return True, _tree_default(_TREE_TYPES[index])
        imgui.same_line()
    if kind in ("object", "array"):
        count = len(value)
        opened = imgui.tree_node_ex(f"{label or kind} ({count})###tree", imgui.TreeNodeFlags_.default_open)
        _help(n)
        if not opened:
            return False, value
        result = _draw_tree_object(value, ctx, path) if kind == "object" else _draw_tree_array(value, ctx, path)
        imgui.tree_pop()
        return result
    if label:
        imgui.text_unformatted(label)
        imgui.same_line()
    if kind == "string":
        imgui.set_next_item_width(-1)
        changed, text = imgui.input_text("##v", value)
        return changed, (text if changed else value)
    if kind == "integer":
        imgui.set_next_item_width(160)
        changed, number = imgui.input_int("##v", int(value))
        return changed, (number if changed else value)
    if kind == "number":
        imgui.set_next_item_width(160)
        changed, number = imgui.input_double("##v", float(value), 0.0, 0.0, "%g")
        return changed, (number if changed else value)
    if kind == "boolean":
        changed, flag = imgui.checkbox("##v", bool(value))
        return changed, (flag if changed else value)
    imgui.text_disabled("(null)")
    return False, value


def _draw_tree_object(value: dict, ctx, path):
    from imgui_bundle import imgui

    out, changed = dict(value), False
    for key in list(out):
        imgui.push_id(key)
        if imgui.small_button("x"):
            del out[key]
            changed = True
            imgui.pop_id()
            continue
        imgui.same_line()
        new_key = _key_input(path, key, out, 160)
        if new_key is not None:
            out = rename_key(out, key, new_key)
            changed = True
            imgui.pop_id()
            break
        imgui.same_line()
        did, new = _draw_tree("", {}, out[key], ctx, f"{path}.{key}")
        if did:
            out[key], changed = new, True
        imgui.pop_id()
    imgui.set_next_item_width(160)
    _, text = imgui.input_text_with_hint("##newkey", "new key", _state.get(f"{path}#k", ""))
    _state[f"{path}#k"] = text
    imgui.same_line()
    if imgui.small_button("+ Add") and text.strip() and text.strip() not in out:
        out[text.strip()] = ""
        _state[f"{path}#k"] = ""
        changed = True
    return changed, (out if changed else value)


def _draw_tree_array(value: list, ctx, path):
    from imgui_bundle import imgui

    out, changed = list(value), False
    for index in range(len(out)):
        imgui.push_id(index)
        if imgui.small_button("x"):
            del out[index]
            changed = True
            imgui.pop_id()
            break
        imgui.same_line()
        did, new = _draw_tree("", {}, out[index], ctx, f"{path}[{index}]")
        if did:
            out[index], changed = new, True
        imgui.pop_id()
    if imgui.small_button("+ Add"):
        out.append("")
        changed = True
    return changed, (out if changed else value)


_DRAWERS: dict[str, Callable] = {
    "string": _draw_string, "int": _draw_int, "number": _draw_number, "bool": _draw_bool,
    "enum": _draw_enum, "formkey": _draw_formkey, "ref": _draw_ref, "asset": _draw_asset, "flags": _draw_flags,
    "object": _draw_object, "scalar_list": _draw_list, "formkey_list": _draw_list, "object_list": _draw_rows,
    "list": _draw_rows, "tuple": _draw_tuple, "tuple_list": _draw_tuple_list, "map": _draw_map,
    "variant": _draw_variant, "tree": _draw_tree,
}
