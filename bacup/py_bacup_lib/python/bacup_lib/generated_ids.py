"""Generated FormID allocation policy shared by conversion workflows."""

MAX_LOCAL_OBJECT_ID = 0x00FFFFFF
FO76_GENERATED_FORM_ID_HEADROOM = 0x00100000
FO76_GENERATED_FORM_ID_ALIGNMENT = 0x00100000


def _align_object_id_up(value: int, alignment: int) -> int:
    if alignment <= 0:
        return value
    return ((value + alignment - 1) // alignment) * alignment


def generated_object_id_floor(source_game: str, source_max_object_id: int) -> int:
    if source_game.lower() != "fo76":
        return 0
    if source_max_object_id < 0 or source_max_object_id > MAX_LOCAL_OBJECT_ID:
        raise RuntimeError(
            f"FO76 source max object ID is outside the local FormID range: "
            f"{source_max_object_id:06X}"
        )
    if source_max_object_id == 0:
        return 0

    requested_floor = source_max_object_id + FO76_GENERATED_FORM_ID_HEADROOM
    if requested_floor <= MAX_LOCAL_OBJECT_ID:
        aligned_floor = _align_object_id_up(
            requested_floor,
            FO76_GENERATED_FORM_ID_ALIGNMENT,
        )
        if aligned_floor <= MAX_LOCAL_OBJECT_ID:
            return aligned_floor

    fallback_floor = source_max_object_id + 1
    if fallback_floor <= MAX_LOCAL_OBJECT_ID:
        return fallback_floor
    raise RuntimeError(
        "FO76 source plugin has no remaining local FormID space for generated records"
    )
