from __future__ import annotations


def test_record_extractors_expose_only_native_asset_metadata():
    from bacup_lib.record import extractors
    from bacup_lib.record.extractors import (
        ASSET_CONTRIBUTIONS,
        signatures_for_asset_kind,
    )

    assert ASSET_CONTRIBUTIONS
    assert signatures_for_asset_kind("nif") == frozenset()
    assert signatures_for_asset_kind("material") == frozenset({"MSWP"})
    assert not hasattr(extractors, "get_extractor")
    assert not hasattr(extractors, "register")
