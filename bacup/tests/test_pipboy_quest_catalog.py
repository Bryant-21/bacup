from __future__ import annotations

import pytest

from bacup_lib.pipboy_quest_catalog import OUTPUT, build_catalog, classify, emit_quest_catalog


def source(form: str, editor_id: str, quest_type: str | None, *, title: bool = True,
           lead_target: str | None = None) -> dict:
    fields = {"DATA": [{"QuestType": quest_type}] if quest_type else [],
              "Name": [{"Values": [{"Language": "English", "String": "Source title"}]}]
              if title else []}
    if lead_target:
        fields["VirtualMachineAdapter"] = [{"Script Fragments": {"Script": {"Properties": [
            {"Type": "Object", "Value": {"FormID": {"reference": {"plugin": "SeventySix.esm",
                                                                "object_id": lead_target}}}}
        ]}}}]
    return {"form_id": form, "eid": editor_id, "fields": fields}


def test_source_types_and_explicit_breadcrumb_bind_to_converted_forms() -> None:
    records = [
        source("002315", "FS01_MQ_Warn", "Primary"),
        source("056F63", "MTR01_Intro", "SideQuest"),
        source("0040CE", "POST_GQ_PlayerKiller", "Miscellaneous", title=False),
        source("699466", "Storm_MQ01_Breadcrumb_Radio", "Miscellaneous", lead_target="72A2A8"),
        source("00123F", "GQ_Horde", "Event"),
        source("0364D0", "MTNS06_Uranium", "PublicEvent"),
        source("3E271D", "E05_NukeBoss", "PublicEvent"),
        source("583D14", "NukeEvent", "PublicEvent"),
        source("2AF217", "76CharGenQuest", "Primary", title=False),
    ]
    converted = [(f"SeventySix.esm:{row['form_id']}", row["eid"] + "fo76") for row in records]
    catalog = build_catalog(records, converted, "SeventySix.esm", "SeventySix.esm",
                            {"available": True})
    by_source = {row["source"].split(":")[1]: row for row in catalog["quests"]}
    assert [by_source[form]["category"] for form in
            ("002315", "056F63", "0040CE", "699466", "00123F", "0364D0", "3E271D", "583D14",
             "2AF217")] == [
        "MAIN", "SIDE", "MISC", "LEADS", "EVENTS", "EVENTS", "EVENTS", "EVENTS", "MAIN"]
    assert all(row["form"] == f"SeventySix.esm:{form}" for form, row in by_source.items())


@pytest.mark.parametrize("form,target", [("5E9591", "5EAD3C"),
                                         ("699466", "72A2A8"),
                                         ("78DE65", "78DE64")])
def test_three_source_radio_leads_use_verified_quest_bindings(form: str, target: str) -> None:
    record = source(form, "SourceRadio", "Miscellaneous", lead_target=target)
    catalog = build_catalog([record], [(f"SeventySix.esm:{form}", "SourceRadio")],
                            "SeventySix.esm", "SeventySix.esm", {"available": True})
    assert catalog["quests"][0]["category"] == "LEADS"


def test_source_lead_binding_must_still_point_to_its_breadcrumb_quest() -> None:
    record = source("699466", "Storm_MQ01_Breadcrumb_Radio", "Miscellaneous", lead_target="001234")
    converted = [("SeventySix.esm:699466", record["eid"])]
    try:
        build_catalog([record], converted, "SeventySix.esm", "SeventySix.esm", {"available": True})
    except ValueError as error:
        assert "lead binding changed" in str(error)
    else:
        raise AssertionError("A changed source lead binding must disable the catalog")


def test_unmapped_source_quest_is_reported_and_never_guessed() -> None:
    records = [source("002315", "FS01_MQ_Warn", "Primary"),
               source("FFFFFF", "Unconverted", "Secondary")]
    catalog = build_catalog(records, [("SeventySix.esm:002315", "FS01_MQ_Warn")],
                            "SeventySix.esm", "SeventySix.esm", {"available": True})
    assert catalog["missing_converted"] == ["FFFFFF"]
    assert len(catalog["quests"]) == 1
    assert classify(14) == "MISC"


def test_missing_source_movie_removes_a_stale_catalog_before_fallback(tmp_path) -> None:
    destination = tmp_path / OUTPUT
    destination.parent.mkdir(parents=True)
    destination.write_text("stale", encoding="utf-8")
    with pytest.raises(FileNotFoundError):
        emit_quest_catalog(tmp_path / "SeventySix.esm", tmp_path / "converted.esm",
                           tmp_path / "source", tmp_path)
    assert not destination.exists()
