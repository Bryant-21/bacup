import json

from bacup_lib.pipboy_quest_catalog import OUTPUT, to_document
from bacup_lib.plugin_catalog import write_document
from bacup_lib.tests.catalog_schema_validation import validate_catalog


def test_document_keeps_quests_as_the_keyed_section_and_validates(tmp_path):
    native = {"schema_version": 1, "categories": ["MAIN", "SIDE", "MISC", "LEADS", "EVENTS"],
              "presentation": {"available": True, "component_owner": "vanilla_fo4"},
              "quests": [{"form": "SeventySix.esm:002315", "category": "MAIN"}],
              "missing_converted": ["QUST:Foo"], "source_sha256": "ab"}
    document = to_document(native)
    assert document["quests"] == native["quests"] and "quests" not in document["settings"]
    path = write_document("QuestCategories", "SeventySix.esm", document, tmp_path)
    assert path.relative_to(tmp_path) == OUTPUT
    validate_catalog("QuestCategories", json.loads(path.read_text(encoding="utf-8")))
