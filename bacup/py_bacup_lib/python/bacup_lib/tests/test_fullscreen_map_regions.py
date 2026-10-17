from bacup_lib.fullscreen_map_regions import build_region_catalog


def ref(object_id):
    return {"reference": {"plugin": "World.esm", "object_id": object_id}}


def region(eid, location="000002", world="000001"):
    return {"eid": eid, "fields": [
        {"WorldspaceReference": ref(world)}, {"Location": ref(location)},
        {"Points": [{"PointsX": 0, "PointsY": 0}, {"PointsX": 200, "PointsY": 0}, {"PointsX": 0, "PointsY": 200}]},
    ]}


def test_only_major_region_locations_supply_labels():
    locations = {"world.esm:000002": {"Name": "The Forest", "Keywords": [ref("000003")]},
                 "world.esm:000004": {"Name": "Local site", "Keywords": []}}
    records = [region("ForestObjectRegion"), region("ForestSubRegion01"), region("ForestBorder"),
               region("ForestWeather"), region("ForestRegionNudge01"), region("WorkshopForest"),
               region("ForestDistantObjectRegion"), region("TestForestRegion"), region("Site", "000004"),
               region("DifferentWorld", world="000099")]
    deleted = region("DeletedForest")
    deleted["flags"] = "00000020"
    records.append(deleted)
    result = build_region_catalog(records, locations, "world.esm:000003", "world.esm:000001", "World")
    assert result["worldspace"] == "World"
    assert len(result["regions"]) == 1
    assert result["regions"][0]["name"] == "The Forest"
    assert len(result["regions"][0]["polygons"]) == 1


def test_regions_sharing_a_major_location_merge_and_keep_disconnected_polygons():
    records = [region("ForestObjectRegion"), region("ForestAddition")]
    records[0]["fields"].append(records[0]["fields"][-1])
    locations = {"world.esm:000002": {"Name": {"Values": [
        {"Language": "English", "String": "The Forest"}, {"Language": "French", "String": "La Forêt"}]},
        "Keywords": [ref("000003")]}}
    result = build_region_catalog(records, locations, "world.esm:000003", "world.esm:000001", "World", "French")
    assert len(result["regions"]) == 1
    assert result["regions"][0]["name"] == "La Forêt"
    assert len(result["regions"][0]["polygons"]) == 3


def test_unnamed_and_missing_locations_do_not_become_editor_id_labels():
    records = [region("ForestObjectRegion"), region("Missing", "000099")]
    locations = {"world.esm:000002": {"Keywords": [ref("000003")]}}
    assert build_region_catalog(records, locations, "world.esm:000003", "world.esm:000001", "World")["regions"] == []
