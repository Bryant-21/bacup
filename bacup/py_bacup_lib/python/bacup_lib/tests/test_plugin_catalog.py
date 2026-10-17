import json

import jsonschema
import pytest

from bacup_lib import plugin_catalog as pc

SCHEMAS = pc.SCHEMAS_DIR
CASES = json.loads((SCHEMAS / "plugin_catalog_cases.json").read_text(encoding="utf-8"))


@pytest.mark.parametrize("case", CASES["form_key_accept"], ids=lambda c: c["text"])
def test_form_key_accepts(case):
    assert pc.parse_form_key(case["text"]) == (case["plugin"], case["id"])


@pytest.mark.parametrize("text", CASES["form_key_reject"])
def test_form_key_rejects(text):
    assert pc.parse_form_key(text) is None


@pytest.mark.parametrize("case", CASES["form_key_format"], ids=lambda c: c["text"])
def test_form_key_format(case):
    assert pc.form_key(case["plugin"], case["id"]) == case["text"]


@pytest.mark.parametrize("case", CASES["order"], ids=lambda c: c["name"])
def test_order_files(case):
    assert pc.order_files(case["files"], case["active"]) == case["expected"]


@pytest.mark.parametrize("case", CASES["merge"], ids=lambda c: c["name"])
def test_merge_section(case):
    field = case["key_field"]

    def key_of(entry, map_key):
        if field is None:
            return map_key
        value = entry.get(field)
        return value if isinstance(value, str) else None

    merged = {}
    for section in case["docs"]:
        pc.merge_section(merged, section, key_of)
    assert merged == case["expected"]


@pytest.mark.parametrize("case", CASES["settings"], ids=lambda c: c["name"])
def test_merge_settings(case):
    settings = {}
    for doc in case["docs"]:
        pc.merge_settings(settings, doc)
    assert settings == case["expected"]


def test_catalog_path():
    assert pc.catalog_path("Fishing", "SeventySix.esm").as_posix() == "F4SE/Plugins/B21_TalesFromAppalachia/Fishing/SeventySix.esm.json"
    assert pc.ROOT.as_posix() == "F4SE/Plugins/B21_TalesFromAppalachia"


def test_write_document_is_deterministic_and_envelope_ordered(tmp_path):
    doc = {"records": [{"form": "SeventySix.esm:35ADBA", "name": "Zoë"}], "settings": {"b": 1, "a": 2}}
    path = pc.write_document("Fishing", "SeventySix.esm", doc, tmp_path)
    first = path.read_bytes()
    assert path == tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/Fishing/SeventySix.esm.json"
    assert b"\r" not in first and first.endswith(b"}\n")
    assert "Zoë".encode("utf-8") in first
    loaded = json.loads(first)
    assert list(loaded) == ["version", "settings", "records"]
    assert loaded["version"] == 1
    pc.write_document("Fishing", "SeventySix.esm", {"settings": doc["settings"], "records": doc["records"]}, tmp_path)
    assert path.read_bytes() == first


def test_envelope_schema_accepts_envelope_and_checks_form_keys():
    schema = json.loads((SCHEMAS / "envelope.schema.json").read_text(encoding="utf-8"))
    jsonschema.Draft202012Validator.check_schema(schema)
    validator = jsonschema.Draft202012Validator(schema)
    assert not list(validator.iter_errors({"version": 1, "settings": {}, "x": []}))
    assert list(validator.iter_errors({"settings": {}}))
    form_key = jsonschema.Draft202012Validator({"$ref": "#/$defs/formKey", "$defs": schema["$defs"]})
    for case in CASES["form_key_accept"]:
        assert not list(form_key.iter_errors(case["text"]))
    for text in ("35ADBA:SeventySix.esm", "35ADBA", "3517882", "SeventySix.esm:"):
        assert list(form_key.iter_errors(text))
