"""Catalog editor model: master discovery, layers, merge provenance, editing, validation, save, all schemas."""
from __future__ import annotations

import json
import struct
from pathlib import Path

import jsonschema
import pytest

from creation_lib.catalog.plugin_catalog import catalog_path
from ui.tools.catalog_editor import model
from ui.tools.catalog_editor.model import CatalogSaveError, CatalogSession, load_schemas, resolve_ref

SCHEMAS = load_schemas()
FORM = "SeventySix.esm:000801"


def subrecord(tag: bytes, data: bytes) -> bytes:
    return tag + struct.pack("<H", len(data)) + data


def write_plugin(path: Path, masters: tuple[str, ...] = ()) -> Path:
    payload = subrecord(b"HEDR", struct.pack("<fII", 1.0, 0, 0x800))
    for master in masters:
        payload += subrecord(b"MAST", master.encode() + b"\0") + subrecord(b"DATA", bytes(8))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"TES4" + struct.pack("<IIIIHH", len(payload), 0, 0, 0, 131, 0) + payload)
    return path


def write_catalog(directory: Path, feature: str, plugin: str, doc: dict) -> Path:
    path = directory / catalog_path(feature, plugin)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc), encoding="utf-8")
    return path


class FakeForms:
    def __init__(self, known: set[str]):
        self.known = known

    def search(self, text, signatures=None, limit=200):
        return [(key, key) for key in sorted(self.known) if text in key][:limit]

    def describe(self, key):
        return key if key in self.known else None

    def resolves(self, key):
        return key in self.known


def pet(actor: str, species: str = "dog") -> dict:
    return {"actor": actor, "home": "Fallout4.esm:000100", "species": species}


@pytest.fixture
def world(tmp_path: Path):
    """data/ holds Base.esm (no master) and Mid.esm (masters Base.esm); mod/ holds My.esp (masters Base.esm, Mid.esm)."""
    data = tmp_path / "data"
    write_plugin(data / "Base.esm")
    write_plugin(data / "Mid.esm", ("Base.esm",))
    mod = tmp_path / "mod"
    plugin = write_plugin(mod / "My.esp", ("Base.esm", "Mid.esm"))
    return tmp_path, data, mod, plugin


def open_session(world, **kwargs) -> CatalogSession:
    _, data, _, plugin = world
    return CatalogSession.open(plugin, data_dir=data, **kwargs)


def test_masters_come_from_the_header_in_order_and_missing_ones_are_listed(world):
    _, data, _, plugin = world
    session = CatalogSession.open(plugin)
    assert [m.name for m in session.masters] == ["Base.esm", "Mid.esm"]
    assert session.missing_masters == ["Base.esm", "Mid.esm"]
    session = CatalogSession.open(plugin, data_dir=data)
    assert session.missing_masters == []
    assert session.masters[0].path == data / "Base.esm"


def test_master_search_order_first_hit_wins_case_insensitive(world, tmp_path):
    root, data, mod, plugin = world
    in_plugin_folder = write_plugin(mod / "base.ESM")
    bacup = root / "bacup"
    write_plugin(bacup / "Base.esm")
    session = CatalogSession.open(plugin, data_dir=data, bacup_output_dir=bacup)
    assert session.masters[0].path == in_plugin_folder
    in_plugin_folder.unlink()
    session = CatalogSession.open(plugin, data_dir=data, bacup_output_dir=bacup)
    assert session.masters[0].path == data / "Base.esm"
    (data / "Base.esm").unlink()
    session = CatalogSession.open(plugin, data_dir=data, bacup_output_dir=bacup)
    assert session.masters[0].path == bacup / "Base.esm"
    extra = root / "extra"
    write_plugin(extra / "Mid.esm")
    session = CatalogSession.open(plugin, data_dir=data, bacup_output_dir=bacup)
    assert session.missing_masters == ["Mid.esm"] or session.masters[1].path == data / "Mid.esm"


def test_add_master_folder_resolves_missing_masters_and_layers(world, tmp_path):
    root, _, mod, plugin = world
    extra = root / "extra"
    write_plugin(extra / "Base.esm")
    write_plugin(extra / "Mid.esm")
    write_catalog(extra, "Pets", "Base.esm", {"version": 1, "pets": [pet("Base.esm:000801")]})
    session = CatalogSession.open(plugin)
    document = session.feature("Pets")
    assert document.rows("pets") == []
    session.add_master_folder(extra)
    assert session.missing_masters == []
    assert [r.key for r in document.rows("pets")] == ["Base.esm:000801"]


def test_layers_merge_in_header_order_with_provenance(world):
    _, data, mod, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 1, "pets": [pet("Base.esm:000801"), pet("Base.esm:000802")],
                                              "settings": {"xp_per_second": 1.0, "xp_thresholds": [1, 2]}})
    write_catalog(data, "Pets", "Mid.esm", {"version": 1, "pets": [pet("Base.esm:000801", "cat"), pet("Mid.esm:000803"),
                                                                    {"actor": "Base.esm:000802", "remove": True}],
                                             "settings": {"xp_per_second": 2.0}})
    document = open_session(world).feature("Pets")
    rows = {row.key: row for row in document.rows("pets")}
    assert list(rows) == ["Base.esm:000801", "Mid.esm:000803"]
    assert rows["Base.esm:000801"].entry["species"] == "cat"
    assert rows["Base.esm:000801"].source == "Mid.esm"
    assert rows["Base.esm:000801"].status == "inherited"
    settings = {name: (value, source, mine) for name, value, source, mine in document.settings()}
    assert settings["xp_per_second"] == (2.0, "Mid.esm", False)
    assert settings["xp_thresholds"] == ([1, 2], "Base.esm", False)
    assert settings["progression"] == (None, "", False)


def test_unknown_version_skips_the_layer_with_a_non_blocking_problem(world):
    _, data, _, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 99, "pets": [pet("Base.esm:000801")]})
    write_catalog(data, "Pets", "Mid.esm", {"version": 1, "pets": [pet("Mid.esm:000803")]})
    document = open_session(world).feature("Pets")
    assert [r.key for r in document.rows("pets")] == ["Mid.esm:000803"]
    problems = document.validate(None)
    assert len(problems) == 1 and not problems[0].blocking and "99" in problems[0].message


def test_identity_ignores_formkey_case_and_leading_zeros_but_keeps_original_strings(world):
    _, data, _, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 1, "pets": [pet("SeventySix.esm:00ABCD")]})
    document = open_session(world).feature("Pets")
    assert document.rows("pets")[0].key == "SeventySix.esm:00ABCD"
    document.set_entry("pets", pet("seventysix.esm:abcd", "cat"))
    rows = document.rows("pets")
    assert [(r.key, r.status) for r in rows] == [("SeventySix.esm:00ABCD", "overridden")]
    assert rows[0].entry["actor"] == "seventysix.esm:abcd"
    assert rows[0].inherited["species"] == "dog"


def test_override_remove_revert_and_added_status(world):
    _, data, _, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 1, "pets": [pet("Base.esm:000801"), pet("Base.esm:000802")]})
    document = open_session(world).feature("Pets")
    assert not document.dirty and document.edit_count == 0
    key = document.set_entry("pets", pet("Base.esm:000801", "cat"))
    assert key == "Base.esm:000801"
    document.remove("pets", "Base.esm:000802")
    added = document.set_entry("pets", pet("My.esp:000800"))
    status = {r.key: r.status for r in document.rows("pets")}
    assert status == {"Base.esm:000801": "overridden", "Base.esm:000802": "removed", "My.esp:000800": "added"}
    removed = next(r for r in document.rows("pets") if r.status == "removed")
    assert removed.entry == removed.inherited and removed.source == "My.esp"
    assert document.section_keys("pets") == ["Base.esm:000801", "My.esp:000800"]
    assert document.dirty and document.edit_count == 3
    document.revert("pets", "Base.esm:000801")
    document.revert("pets", "Base.esm:000802")
    document.remove("pets", added)
    assert [r.status for r in document.rows("pets")] == ["inherited", "inherited"]
    assert not document.dirty and document.edit_count == 0
    with pytest.raises(KeyError):
        document.remove("pets", "Nothing.esm:000001")


def test_x_order_keeps_first_appearance_and_unordered_sections_sort(world):
    _, data, _, _ = world
    assert document_order(world, data, "Pets", "pets", ["Base.esm:000803", "Base.esm:000801"]) == [
        "Base.esm:000803", "Base.esm:000801"]
    assert document_order(world, data, "FoodSpoilage", "foods", ["Base.esm:000803", "Base.esm:000801"]) == [
        "Base.esm:000801", "Base.esm:000803"]


def document_order(world, data, feature, section, keys):
    schema = SCHEMAS[feature]
    entry = minimal(schema, schema["properties"][section]["items"], 0)
    field = schema["properties"][section]["x-key"][0]
    write_catalog(data, feature, "Base.esm", {"version": 1, section: [{**entry, field: k} for k in keys]})
    return [r.key for r in open_session(world).feature(feature).rows(section)]


def test_map_sections_and_remove_marker(world):
    _, data, _, _ = world
    write_catalog(data, "LegendaryPerks", "Base.esm", {"version": 1, "bindings": {"B21_A": FORM, "B21_B": FORM}})
    document = open_session(world).feature("LegendaryPerks")
    assert document.section_kind("bindings") == "map"
    document.set_entry("bindings", "SeventySix.esm:000802", map_key="B21_A")
    document.remove("bindings", "B21_B")
    assert document.author_document() == {
        "version": 1, "bindings": {"B21_A": "SeventySix.esm:000802", "B21_B": {"remove": True}}}
    with pytest.raises(ValueError):
        document.set_entry("bindings", FORM)


def test_composite_keys_join_the_fields(world):
    document = open_session(world).feature("ObjectiveAreas")
    entry = {"quest": "A.esm:000001", "objective": 10, "target": "A.esm:000002"}
    key = document.set_entry("areas", {**entry, "x": 1})
    assert key == "A.esm:000001 | 10 | A.esm:000002"
    document.remove("areas", key)
    assert document.edit_count == 0


def test_settings_override_revert_and_minimal_document(world):
    _, data, _, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 1, "settings": {"xp_per_second": 1.0}})
    document = open_session(world).feature("Pets")
    assert document.author_document() == {"version": 1}
    document.set_setting("xp_per_second", 5.0)
    values = {name: (value, source, mine) for name, value, source, mine in document.settings()}
    assert values["xp_per_second"] == (5.0, "My.esp", True)
    assert document.author_document() == {"version": 1, "settings": {"xp_per_second": 5.0}}
    assert document.dirty and document.edit_count == 1
    document.revert_setting("xp_per_second")
    assert not document.dirty


def test_author_document_holds_only_author_entries_and_nested_edits_do_not_alias(world):
    _, data, _, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 1, "pets": [pet("Base.esm:000801")]})
    document = open_session(world).feature("Pets")
    value = pet("Base.esm:000801", "cat")
    document.set_entry("pets", value)
    value["species"] = "dog"
    assert document.author_document() == {"version": 1, "pets": [pet("Base.esm:000801", "cat")]}


def test_opaque_sections_are_read_only_rows(world):
    _, data, _, _ = world
    write_catalog(data, "LeveledLists", "Base.esm", {"version": 1, "ess": {"a": 1, "b": 2}})
    write_catalog(data, "LeveledLists", "Mid.esm", {"version": 1, "ess": {"c": 3}})
    document = open_session(world).feature("LeveledLists")
    assert document.section_kind("ess") == "opaque"
    assert [(r.key, r.entry, r.source) for r in document.rows("ess")] == [("c", 3, "Mid.esm")]
    for call in (lambda: document.set_entry("ess", {"x": 1}, map_key="x"), lambda: document.remove("ess", "c")):
        with pytest.raises(ValueError):
            call()


def test_validate_reports_schema_errors_unresolved_forms_and_duplicates(world):
    _, _, mod, _ = world
    write_catalog(mod, "Pets", "My.esp", {"version": 1, "pets": [pet("My.esp:000800"), pet("my.esp:800")]})
    known = {"Fallout4.esm:000100", "My.esp:000800", "my.esp:800"}
    document = open_session(world, forms=FakeForms(known)).feature("Pets")
    assert [p.message for p in document.validate(document._forms())] == ["duplicate key"]
    document.set_entry("pets", pet("My.esp:000800"))
    assert document.validate(document._forms()) == []

    document.set_entry("pets", {"actor": "My.esp:000800", "home": "Fallout4.esm:000100", "species": "dragon"})
    problems = document.validate(None)
    assert len(problems) == 1 and problems[0].section == "pets" and problems[0].key == "My.esp:000800"
    assert "dragon" in problems[0].message

    document.set_entry("pets", pet("My.esp:000800"))
    document.set_entry("pets", {**pet("My.esp:000802"), "home": "Gone.esm:000001"})
    problems = document.validate(document._forms())
    assert [(p.key, p.message) for p in problems] == [
        ("My.esp:000802", "FormKey My.esp:000802 does not resolve"),
        ("My.esp:000802", "FormKey Gone.esm:000001 does not resolve")]
    assert not any(p.blocking for p in problems)


def test_validate_checks_settings_and_ignores_remove_markers(world):
    _, data, _, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 1, "pets": [pet("Base.esm:000801")]})
    document = open_session(world, forms=FakeForms(set())).feature("Pets")
    document.remove("pets", "Base.esm:000801")
    assert document.validate(document._forms()) == []
    document.set_setting("xp_per_second", "fast")
    assert any(p.section == "settings" for p in document.validate(None))


def test_save_writes_only_the_author_layer_and_round_trips(world):
    _, data, mod, _ = world
    write_catalog(data, "Pets", "Base.esm", {"version": 1, "pets": [pet("Base.esm:000801"), pet("Base.esm:000802")],
                                              "settings": {"xp_per_second": 1.0}})
    session = open_session(world)
    document = session.feature("Pets")
    document.set_entry("pets", pet("Base.esm:000801", "cat"))
    document.remove("pets", "Base.esm:000802")
    document.set_entry("pets", pet("My.esp:000800"))
    document.set_setting("xp_per_second", 3.0)
    assert session.dirty_features() == ["Pets"]
    path = document.save()
    assert path == mod / catalog_path("Pets", "My.esp")
    assert not document.dirty and session.dirty_features() == []
    written = json.loads(path.read_text(encoding="utf-8"))
    assert written["pets"][1] == {"actor": "Base.esm:000802", "remove": True}
    assert list(written) == ["version", "settings", "pets"]
    assert not list(path.parent.glob("*.tmp"))

    again = open_session(world).feature("Pets")
    assert not again.dirty and again.edit_count == 4
    assert [(r.key, r.status) for r in again.rows("pets")] == [
        ("Base.esm:000801", "overridden"), ("Base.esm:000802", "removed"), ("My.esp:000800", "added")]


def test_save_preserves_unknown_fields_and_extra_sections(world):
    _, _, mod, _ = world
    write_catalog(mod, "Pets", "My.esp", {"version": 1, "future": {"a": 1},
                                           "pets": [{**pet("My.esp:000800"), "extra_field": [1, 2]}]})
    document = open_session(world).feature("Pets")
    document.set_setting("xp_per_second", 1.0)
    document.save()
    written = json.loads((mod / catalog_path("Pets", "My.esp")).read_text(encoding="utf-8"))
    assert written["future"] == {"a": 1}
    assert written["pets"][0]["extra_field"] == [1, 2]


def test_save_refuses_a_problem_a_master_filename_and_a_master_catalog(world):
    _, data, mod, plugin = world
    document = open_session(world).feature("Pets")
    document.set_entry("pets", {"actor": "My.esp:000800", "home": "Fallout4.esm:000100", "species": "dragon"})
    with pytest.raises(CatalogSaveError) as caught:
        document.save()
    assert caught.value.problems and not (mod / catalog_path("Pets", "My.esp")).exists()

    same_name = write_plugin(data / "Mid.esm", ("Base.esm", "Mid.esm"))
    document = CatalogSession.open(same_name, data_dir=data).feature("Pets")
    document.set_entry("pets", pet("Mid.esm:000800"))
    with pytest.raises(CatalogSaveError, match="masters"):
        document.save()

    write_plugin(data / "Mid.esm", ("Base.esm",))
    write_plugin(mod / "Base.esm")
    write_catalog(mod, "Pets", "Base.esm", {"version": 1})
    session = CatalogSession.open(plugin, data_dir=mod)
    document = session.feature("Pets")
    document.author_path = mod / catalog_path("Pets", "Base.esm")
    document.set_entry("pets", pet("My.esp:000800"))
    with pytest.raises(CatalogSaveError, match="catalog of master"):
        document.save()


def test_unreadable_author_file_blocks_save(world):
    _, _, mod, _ = world
    path = write_catalog(mod, "Pets", "My.esp", {"version": 7})
    document = open_session(world).feature("Pets")
    document.set_entry("pets", pet("My.esp:000800"))
    with pytest.raises(CatalogSaveError):
        document.save()
    assert json.loads(path.read_text(encoding="utf-8")) == {"version": 7}


def test_resolve_ref_follows_envelope_and_local_refs():
    schema = SCHEMAS["Pets"]
    node = resolve_ref(schema, schema["properties"]["pets"]["items"]["properties"]["actor"])
    assert node["type"] == "string" and node["x-form-type"] == ["NPC_"]
    assert resolve_ref(schema, {"$ref": "#/$defs/optionalForm"})["oneOf"]


def test_entry_schema_for_arrays_and_maps(world):
    assert open_session(world).feature("Pets").entry_schema("pets")["properties"]["species"]["enum"]
    assert open_session(world).feature("LegendaryPerks").entry_schema("bindings") is not None


# -- every feature --


STRING_CANDIDATES = (
    lambda v: f"SeventySix.esm:{0x801 + v:06X}",
    lambda v: f"{0x801 + v:08X}",
    lambda v: f"{0x801 + v:02x}",
    lambda v: f"B21_Legendary_Foo_{v + 1}",
    lambda v: f"P-{v}",
    lambda v: f"v{v}",
    lambda v: f"SG{v:02d}",
)


def sample_string(schema: dict, node: dict, variant: int, registry) -> str:
    validator = jsonschema.Draft202012Validator(node, registry=registry)
    for candidate in STRING_CANDIDATES:
        if validator.is_valid(candidate(variant)):
            return candidate(variant)
    raise AssertionError(f"no sample string for {node}")


def minimal(schema: dict, node: dict, variant: int, *, force_items: bool = False) -> object:
    """Smallest valid value for a schema node; `variant` shifts keys so entries stay distinct."""
    registry = model._schema_registry(SCHEMAS)
    node = resolve_ref(schema, node)
    if "const" in node:
        return node["const"]
    if "enum" in node:
        return node["enum"][0]
    for combiner in ("oneOf", "anyOf"):
        for branch in node.get(combiner, []):
            if resolve_ref(schema, branch).get("type") != "null":
                return minimal(schema, branch, variant, force_items=force_items)
    if "allOf" in node:
        merged: dict = {}
        for branch in node["allOf"]:
            merged.update(minimal(schema, branch, variant))
        return merged
    kind = node.get("type")
    if isinstance(kind, list):
        kind = next(k for k in kind if k != "null")
    if kind == "string" or (kind is None and ("pattern" in node or node.get("x-form-type"))):
        return sample_string(schema, {**node, "type": "string"}, variant, registry)
    if kind in ("integer", "number"):
        low = node.get("minimum", node.get("exclusiveMinimum", -1) + 1 if "exclusiveMinimum" in node else 0)
        return int(low) + variant
    if kind == "boolean":
        return False
    if kind == "array":
        count = max(node.get("minItems", 0), 1 if force_items else 0)
        return [minimal(schema, node.get("items", {}), variant) for _ in range(count)]
    if kind == "object" or "properties" in node:
        required = list(node.get("required", [])) + list(node.get("then", {}).get("required", []))
        required += list(node.get("x-key", [])) if isinstance(node.get("x-key"), list) else []
        properties = node.get("properties", {})
        return {name: minimal(schema, properties.get(name, {}), variant) for name in dict.fromkeys(required)
                if name != "remove"}
    return f"v{variant}"


def sample_entry(schema: dict, document: model.FeatureDocument, section: str, variant: int):
    info = document._section(section)
    entry_node = document.entry_schema(section)
    if info.kind == "array":
        entry = minimal(schema, entry_node, variant)
        properties = entry_node.get("properties", {})
        for name in info.key_fields:
            entry[name] = minimal(schema, properties.get(name, {}), variant, force_items=True)
        return entry, None
    names = info.node.get("propertyNames", {})
    key_node = resolve_ref(schema, names)
    key = key_node["enum"][variant] if "enum" in key_node else sample_string(schema, key_node, variant, model._schema_registry(SCHEMAS))
    return minimal(schema, entry_node, variant), key


@pytest.mark.parametrize("feature", sorted(SCHEMAS))
def test_every_feature_round_trips(feature, tmp_path):
    schema = SCHEMAS[feature]
    data = tmp_path / "data"
    write_plugin(data / "SeventySix.esm")
    plugin = write_plugin(tmp_path / "mod" / "My.esp", ("SeventySix.esm",))
    session = CatalogSession.open(plugin, data_dir=data)
    document = session.feature(feature)
    assert document.sections() == [n for n in schema["properties"] if n not in ("version", "settings")]
    editable = [s for s in document.sections() if document.section_kind(s) != "opaque"]

    base_doc: dict = {"version": document.version}
    for section in editable:
        entries = [sample_entry(schema, document, section, variant) for variant in (0, 1)]
        if document.section_kind(section) == "array":
            base_doc[section] = [entry for entry, _ in entries]
        else:
            base_doc[section] = {key: entry for entry, key in entries}
    jsonschema.Draft202012Validator({k: v for k, v in schema.items() if k != "required"},
                                    registry=model._schema_registry(SCHEMAS)).validate(base_doc)
    write_catalog(data, feature, "SeventySix.esm", base_doc)

    session = CatalogSession.open(plugin, data_dir=data)
    document = session.feature(feature)
    expected: dict[str, dict[str, tuple[str, object]]] = {}
    for section in editable:
        rows = document.rows(section)
        assert len(rows) == 2 and {r.status for r in rows} == {"inherited"} and {r.source for r in rows} == {"SeventySix.esm"}
        first, second = sorted(rows, key=lambda r: r.key) if document._section(section).ordered is False else rows
        overridden, _ = sample_entry(schema, document, section, 0)
        added, added_key = sample_entry(schema, document, section, 2)
        override_key = document.set_entry(section, overridden, map_key=first.key if document.section_kind(section) == "map" else None)
        document.remove(section, second.key)
        new_key = document.set_entry(section, added, map_key=added_key)
        expected[section] = {
            override_key: ("overridden", overridden),
            second.key: ("removed", second.entry),
            new_key: ("added", added),
        }
    document.save()

    reopened = CatalogSession.open(plugin, data_dir=data).feature(feature)
    assert not reopened.dirty
    for section in editable:
        actual = {r.key: (r.status, r.entry) for r in reopened.rows(section)}
        assert actual == expected[section], section
        assert reopened.validate(None) == []
