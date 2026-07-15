from __future__ import annotations

from pathlib import Path

from creation_lib.nif import native_runtime as nif_native_runtime

FIXTURE_BASE = (
    Path(__file__).resolve().parent / "fixtures" / "nif" / "fnv" / "weapons" / "m2_min_base.nif"
)


def _root_payload_name(payload: dict) -> str:
    header = payload.get("header") or {}
    root_ids = [root_id for root_id in header.get("footer_roots", []) if root_id >= 0]
    root_id = root_ids[0] if root_ids else 0
    return str(((payload["blocks"][root_id]["fields"]) or {}).get("Name") or "")


def test_native_convert_honors_gun_weapon_role(tmp_path) -> None:
    dst = tmp_path / "gun.nif"
    nif_native_runtime.convert_nif_file_raw(
        str(FIXTURE_BASE),
        str(dst),
        "fnv",
        "fo4",
        None,
        {"weapon_role": "gun", "asset_prefix": "fnv"},
    )
    payload = nif_native_runtime.load_nif_raw(str(dst))
    assert _root_payload_name(payload) == "Weapon"

