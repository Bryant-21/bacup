"""WEAP animation metadata synthesis for FNV/FO3 weapon conversion."""
from __future__ import annotations


def wire_weap_animation(
    weap: dict,
    family_subgraph: str,
    overlay_relpath: str | None,
    rest_pose_relpath: str | None,
) -> dict:
    """Append AnimSubgraph-related fields to a translated WEAP record."""
    fields = list(weap.get("fields", []))
    fields.append({"AnimSubgraph": family_subgraph})
    if overlay_relpath:
        fields.append({"AdditiveAnimationOverlay": overlay_relpath})
    if rest_pose_relpath:
        fields.append({"AnimRestPose": rest_pose_relpath})
    return {**weap, "fields": fields}
