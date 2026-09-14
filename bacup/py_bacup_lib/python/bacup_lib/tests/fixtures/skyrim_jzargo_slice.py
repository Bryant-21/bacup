from __future__ import annotations

from collections.abc import Mapping
from types import MappingProxyType

from bacup_lib.source_pairs import SKYRIM_MVP_EXCLUDE_SIGNATURES


SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS: Mapping[str, tuple[int, ...]] = (
    MappingProxyType(
        {
            "QUST": (0x0958B3,),
            "NPC_": (0x01C195,),
            "ACHR": (0x01C1A3,),
            "DLVW": (0x0958B4,),
            "DLBR": (0x0958B5, 0x096201, 0x0967E6, 0x0F1B2A),
            "DIAL": (
                0x0958B0,
                0x0958AF,
                0x0961FE,
                0x0967E1,
                0x0C0418,
                0x0C04D2,
                0x0C04D3,
                0x0C04E8,
                0x0C04E9,
                0x0F1B28,
            ),
            "INFO": (
                0x0958B1,
                0x0C041B,
                0x0C041C,
                0x0958B2,
                0x096200,
                0x0967E2,
                0x0C041A,
                0x0C0502,
                0x0C0506,
                0x0C04FD,
                0x0C0503,
                0x0F1B29,
            ),
            "GLOB": (0x097EE5, 0x097EE6),
            "SCRL": (0x0967E3,),
            "SPEL": (0x097EDF, 0x097EE0),
            "MGEF": (0x097EE2, 0x097EE1),
            "MUSC": (0x07F810,),
            "MUST": (0x02482C, 0x07AE08, 0x07AE0A, 0x07AE09),
        }
    )
)

SKYRIM_JZARGO_SLICE_EXCLUDE_SIGNATURES = SKYRIM_MVP_EXCLUDE_SIGNATURES


def skyrim_jzargo_slice_record_form_ids() -> dict[str, list[int]]:
    return {
        signature: list(form_ids)
        for signature, form_ids in SKYRIM_JZARGO_SLICE_RECORD_FORM_IDS.items()
    }
