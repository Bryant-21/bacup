import json

from bacup_lib.combat_perk_catalog import CARDS, ITEMS, build_catalog, to_document
from bacup_lib.plugin_catalog import write_document
from bacup_lib.tests.catalog_schema_validation import hex_key, validate_catalog


class FakeReader:
    def __init__(self, missing=()):
        self.missing = set(missing)

    def lookup(self, signature, editor_id):
        if editor_id in self.missing:
            raise KeyError(editor_id)
        return 0x100000 + len(editor_id), {}


class FakeResolver:
    def resolve(self, signature, editor_id, source_id):
        return hex_key("SeventySix.esm", editor_id)


def catalog(missing=()):
    return build_catalog(FakeReader(missing), FakeResolver(), "SeventySix.esm", "SeventySix.esm", "abc")


def test_document_is_keyed_by_slug_and_validates(tmp_path):
    document = to_document(catalog())
    assert set(document["cards"]) == {key for key, _, _ in CARDS}
    assert set(document["items"]) == {key for key, _, _ in ITEMS}
    assert document["settings"]["available"] is True
    path = write_document("CombatPerks", "SeventySix.esm", document, tmp_path)
    assert path.as_posix().endswith("F4SE/Plugins/B21_TalesFromAppalachia/CombatPerks/SeventySix.esm.json")
    validate_catalog("CombatPerks", json.loads(path.read_text(encoding="utf-8")))


def test_missing_source_keeps_a_null_unavailable_entry():
    document = to_document(catalog({"mod_Custom_FinalWord"}))
    assert document["items"]["final_word"]["form"] is None and document["items"]["final_word"]["available"] is False
    assert document["settings"]["available"] is False
    validate_catalog("CombatPerks", document)
