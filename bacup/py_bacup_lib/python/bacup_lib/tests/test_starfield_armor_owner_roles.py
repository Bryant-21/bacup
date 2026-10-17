from __future__ import annotations

from types import SimpleNamespace

import pytest

from bacup_lib.workflows.asset_phases import _params_for_convert_nifs
from bacup_lib.workflows.unified import _UnifiedRecordRuntime


@pytest.mark.parametrize(
    ("primary_signature", "primary_field", "claims", "expected_count", "has_first_person"),
    [
        ("ARMA", "MOD2", [("ARMA", "MOD2"), ("ARMA", "MOD4")], 2, True),
        ("STAT", "MODL", [("STAT", "MODL"), ("ARMA", "MOD4")], 2, True),
        ("ARMA", "MOD5", [("ARMA", "MOD5")], 1, True),
        ("STAT", "MODL", [("STAT", "MOD4")], 1, False),
    ],
)
def test_native_collection_owner_roles_reach_starfield_nif_entries(
    tmp_path, primary_signature, primary_field, claims, expected_count, has_first_person
):
    skeleton = tmp_path / "meshes/actors/character/_1stperson/characterassets/skeleton.nif"
    skeleton.parent.mkdir(parents=True)
    skeleton.write_bytes(b"")
    source_path = "Clothes/ChronoWatch/Watch_M.nif"
    owner_claims = [
        {
            "source_form_key": "Starfield.esm:3A8DE7",
            "source_record_signature": signature,
            "source_subrecord_sig": field,
            "idle_topology": [],
        }
        for signature, field in claims
    ]
    companion_claims = [
        {"consumer_field": "MOD2", "resolved_path": "C:/source/body_3p.nif"},
        {"consumer_field": "MOD4", "resolved_path": "C:/source/body_1p.nif"},
        {"consumer_field": "MOD5", "resolved_path": "c:\\source\\BODY_1P.NIF"},
    ]
    asset = _UnifiedRecordRuntime._asset_ref_from_collection_item(
        {
            "asset_type": "nif",
            "source_path": source_path,
            "resolved_path": str(tmp_path / "source.nif"),
            "resolution_error": None,
            "provenance": {
                "added_by_record_fk": "Starfield.esm:3A8DE7",
                "added_by_record_eid": "",
                "added_by_record_sig": primary_signature,
                "added_by_field": primary_field,
                "walk_depth": 0,
                "walker_pass": "native_asset_collect",
            },
            "owner_claims": owner_claims,
            "cloth_companion_claims": companion_claims,
            "workshop_wire_point": None,
        }
    )
    orchestrator = SimpleNamespace(
        source_game="starfield",
        target_game="fo4",
        target_extracted_dir=str(tmp_path),
        base_asset_namespace="Starfield",
        _source_profile=None,
        _addon_index_map={},
        graph=SimpleNamespace(all_assets=[asset], all_records=[]),
        _forced_asset_output_subpath=lambda _asset: f"Meshes/Starfield/{source_path}",
    )

    entries = _params_for_convert_nifs(orchestrator)["nif_paths"]

    assert asset.owner_claims == owner_claims
    assert asset.cloth_companion_claims == companion_claims
    assert len(entries) == expected_count
    first_person = [entry for entry in entries if "target_skeleton" in entry]
    assert len(first_person) == int(has_first_person)
    if has_first_person:
        assert first_person[0]["source_path"] == source_path
        assert first_person[0]["target_skeleton"] == str(skeleton)
        assert first_person[0]["output_subpath"] == (
            "Meshes/Starfield/FirstPerson/Clothes/ChronoWatch/Watch_M.nif"
        )
        assert first_person[0]["cloth_companions"] == ["C:/source/body_1p.nif"]
    if expected_count == 2:
        assert entries[0]["output_subpath"] == f"Meshes/Starfield/{source_path}"
        assert "target_skeleton" not in entries[0]
        assert entries[0]["cloth_companions"] == ["C:/source/body_3p.nif"]
