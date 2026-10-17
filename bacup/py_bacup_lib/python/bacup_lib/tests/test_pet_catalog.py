import pytest

from bacup_lib.pet_catalog import bind_homes, sample_curve


def record(object_id, editor_id, keywords):
    return {"id": object_id, "editor_id": editor_id, "keywords": set(keywords)}


def test_cumulative_xp_curve_interpolates_levels_and_caps():
    assert sample_curve([{"x": 1, "y": 0}, {"x": 3, "y": 100}, {"x": 5, "y": 400}], 4) == [0, 50, 100, 250]


@pytest.mark.parametrize("points,cap", [([], 1), ([{"x": 1, "y": 10}], 1),
    ([{"x": 1, "y": 0}, {"x": 2, "y": float("nan")}], 2),
    ([{"x": 1, "y": 0}, {"x": 2, "y": 5}, {"x": 2, "y": 9}], 2),
    ([{"x": 1, "y": 0}, {"x": 3, "y": 10}], 4)])
def test_invalid_progression_is_rejected(points, cap):
    with pytest.raises(ValueError):
        sample_curve(points, cap)


def test_bind_by_variant_not_list_position():
    homes = [record(1, "CAMPPets_SpawnFurniture_Dog_A", [100]),
             record(2, "CAMPPets_SpawnFurniture_Cat_B", [200])]
    actors = [record(12, "CAMPPets_Actor_Cat_B", [200]),
              record(11, "CAMPPets_Actor_Dog_A", [100])]
    bindings, rejected = bind_homes(homes, actors, {100, 200})
    assert [(row["home"]["id"], row["actor"]["id"]) for row in bindings] == [(1, 11), (2, 12)]
    assert rejected == []


def test_accessories_templates_and_retired_records_do_not_become_homes():
    homes = [record(1, "CAMPPets_SpawnFurniture_Dog_A", [100]),
             record(2, "CAMPPets_IdleFurniture_Dog_Bowl", [100]),
             record(3, "zzzCAMPPets_SpawnFurniture_Dog_Old", [100])]
    actors = [record(11, "CAMPPets_Actor_Dog_A", [100]),
              record(12, "CAMPPets_Actor_Dog_Template", [100])]
    bindings, rejected = bind_homes(homes, actors, {100})
    assert len(bindings) == 1
    assert bindings[0]["actor"]["id"] == 11
    assert rejected == []


@pytest.mark.parametrize("actors", [[], [record(11, "CAMPPets_Actor_Dog_A", [100]),
                                           record(12, "CAMPPets_Actor_Dog_B", [100])]])
def test_missing_or_ambiguous_actor_is_not_guessed(actors):
    bindings, rejected = bind_homes([record(1, "CAMPPets_SpawnFurniture_Dog_A", [100])], actors, {100})
    assert bindings == []
    assert len(rejected) == 1


def test_common_nonidentity_keywords_do_not_match():
    homes = [record(1, "CAMPPets_SpawnFurniture_Dog_A", [100, 999])]
    actors = [record(11, "CAMPPets_Actor_Dog_A", [100, 999]),
              record(12, "CAMPPets_Actor_Cat_B", [200, 999])]
    bindings, _ = bind_homes(homes, actors, {100, 200})
    assert bindings[0]["actor"]["id"] == 11


def test_multiple_variants_of_one_species_remain_distinct():
    homes = [record(1, "CAMPPets_SpawnFurniture_Dog_A", [100]),
             record(2, "ATX_CAMPPets_SpawnFurniture_Dog_B", [101])]
    actors = [record(11, "CAMPPets_Actor_Dog_A", [100]),
              record(12, "ATX_CAMPPets_Actor_Dog_B", [101])]
    bindings, rejected = bind_homes(homes, actors, {100, 101})
    assert len(bindings) == 2
    assert rejected == []


def test_document_splits_pets_and_apparel_from_settings_and_validates(tmp_path):
    import json

    from bacup_lib.pet_catalog import to_document
    from bacup_lib.plugin_catalog import write_document
    from bacup_lib.tests.catalog_schema_validation import validate_catalog

    catalog = {"schema_version": 1, "source": {"plugin": "SeventySix.esm", "sha256": "ab"},
               "pets": [{"home": "SeventySix.esm:7A0001", "actor": "SeventySix.esm:7A0002", "species": "dog",
                         "home_editor_id": "H", "actor_editor_id": "A", "source_keyword": "8ABEF6", "available": True},
                        {"home": None, "actor": "SeventySix.esm:7A0003", "species": "cat", "home_editor_id": "H2",
                         "actor_editor_id": "A2", "source_keyword": "8ABEF7", "available": False}],
               "rejected": [], "missing_converted": ["H2"], "bindings": {"sandbox": "SeventySix.esm:77D823", "stimpak": "Fallout4.esm:023736"},
               "interactions": {"0": {"pet": "SeventySix.esm:7A833A", "speak": None, "sit": None}},
               "apparel": [{"form": "SeventySix.esm:7A0010", "species": 0, "slots": []}],
               "xp_thresholds": [0.0, 10.0], "xp_per_second": 1.0, "progression": {"available": False, "reason": "x"}}
    document = to_document(catalog, "SeventySix.esm")
    assert [row["actor"] for row in document["pets"]] == ["SeventySix.esm:7A0002"]
    assert document["pets"][0]["source_keyword"] == "SeventySix.esm:8ABEF6"
    assert "pets" not in document["settings"] and document["settings"]["xp_per_second"] == 1.0
    path = write_document("Pets", "SeventySix.esm", document, tmp_path)
    validate_catalog("Pets", json.loads(path.read_text(encoding="utf-8")))
