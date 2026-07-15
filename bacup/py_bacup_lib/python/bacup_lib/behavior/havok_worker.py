"""Worker function for parallel HKX conversion (used by ProcessPoolExecutor)."""
from __future__ import annotations


def convert_hkx_file(src_path: str, dst_path: str, target_version: int) -> tuple[bool, str | None]:
    """Convert a single HKX file in a worker process.

    Creates its own HavokConverter so patch loading is amortized across
    the files handled by each worker process.

    Returns (success, error_message).
    """
    try:
        from creation_lib.havok_convert.converter import HavokConverter
        converter = HavokConverter()
        converter.convert_file(src_path, dst_path, target_version)
        return True, None
    except Exception as e:
        return False, str(e)
