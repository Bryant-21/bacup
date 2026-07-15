from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.asset_phases import (
    _normalize_material_source_override,
    _resolve_material_source_override,
)


def test_absolute_material_source_override_is_rejected(tmp_path) -> None:
    assert (
        _normalize_material_source_override(str(tmp_path / "materials" / "foo.bgsm"))
        == ""
    )


def test_material_source_override_resolves_from_asset_root(tmp_path) -> None:
    root = tmp_path / "extracted" / "fo76"
    source = root / "materials" / "landscape" / "ground" / "temp_groundtexture01.bgsm"
    replacement = root / "materials" / "landscape" / "ground" / "forestrocks01.bgsm"
    source.parent.mkdir(parents=True)
    source.write_bytes(b"source")
    replacement.write_bytes(b"replacement")

    resolved = _resolve_material_source_override(
        "materials/landscape/ground/temp_groundtexture01.bgsm",
        str(source),
        "materials/landscape/ground/forestrocks01.bgsm",
    )

    assert Path(resolved) == replacement
