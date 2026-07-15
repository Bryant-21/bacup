from __future__ import annotations

from pathlib import Path

from creation_lib.nif import native_runtime as nif_native_runtime

FIXTURE_DIR = Path(__file__).resolve().parent / "fixtures" / "nif" / "fnv" / "weapons"


def test_weapon_block_diff_raw_returns_attachment_block_ids() -> None:
    diff_ids = nif_native_runtime.weapon_block_diff_raw(
        str(FIXTURE_DIR / "m2_min_base.nif"),
        str(FIXTURE_DIR / "m2_min_with_attachment.nif"),
    )
    assert diff_ids
