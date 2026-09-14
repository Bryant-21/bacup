from __future__ import annotations

from types import MappingProxyType

from bacup_lib.tests.fixtures.skyrim_jzargo_slice import (
    SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS,
    skyrim_jzargo_slice_record_form_ids,
)


def test_jzargo_oracle_is_an_exact_immutable_42_record_fixture() -> None:
    assert isinstance(SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS, MappingProxyType)
    assert sum(map(len, SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS.values())) == 42
    assert SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS["DLVW"] == (0x0958B4,)
    assert SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS["DLBR"] == (
        0x0958B5,
        0x096201,
        0x0967E6,
        0x0F1B2A,
    )
    assert SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS["QUST"] == (0x0958B3,)
    assert 0x0C0416 not in SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS["QUST"]
    assert 0x0C04F0 not in SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS["INFO"]


def test_jzargo_oracle_returns_an_isolated_native_config_copy() -> None:
    record_form_ids = skyrim_jzargo_slice_record_form_ids()
    record_form_ids["QUST"].append(0xFFFFFF)

    assert SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS["QUST"] == (0x0958B3,)
