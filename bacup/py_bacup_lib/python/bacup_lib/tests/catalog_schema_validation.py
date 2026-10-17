"""Validates an emitted catalog document against creation_lib/resources/catalog_schemas/<Feature>.schema.json."""

from __future__ import annotations

import json

import jsonschema
from creation_lib.catalog.plugin_catalog import SCHEMAS_DIR
from referencing import Registry, Resource

SCHEMAS = SCHEMAS_DIR


def validate_catalog(feature: str, document: dict) -> None:
    contents = [json.loads(path.read_text(encoding="utf-8")) for path in SCHEMAS.glob("*.schema.json")]
    resources = [(item["$id"], Resource.from_contents(item)) for item in contents]
    registry = Registry().with_resources(resources)
    schema = json.loads((SCHEMAS / f"{feature}.schema.json").read_text(encoding="utf-8"))
    jsonschema.Draft202012Validator.check_schema(schema)
    jsonschema.Draft202012Validator(schema, registry=registry).validate(document)


def hex_key(plugin: str, name: str) -> str:
    """Deterministic valid FormKey for a fake resolver's (plugin, editor id)."""
    import zlib

    return f"{plugin}:{0x100000 + zlib.crc32(name.encode()) % 0xEFFFFF:06X}"
