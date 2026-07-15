"""Texture conversion compression choices."""
from __future__ import annotations

ROLE_COMPRESSION: dict[str, str] = {
    "normal": "BC5_UNORM",
    "specular": "BC5_UNORM",
}
_DEFAULT_COMPRESSION = "BC7_UNORM"


def compression_for_role(role: str | None) -> str:
    return ROLE_COMPRESSION.get(role or "", _DEFAULT_COMPRESSION)
