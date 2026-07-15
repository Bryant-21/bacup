"""Havok behavior graph conversion.

Only used for games with Havok behavior graphs (FO4, FO76, Skyrim, Starfield).
animation/ has no dependency on this module."""
from bacup_lib.behavior.translator import translate_behavior
from bacup_lib.behavior.driver_synth import (
    load_driver_config,
    BehaviorDriverSpec,
    DriverConfig,
)
from bacup_lib.behavior.deps import extract_behavior_refs, expand_behavior_bundle
from bacup_lib.behavior.phases import phase_scaffold

__all__ = [
    "translate_behavior",
    "load_driver_config",
    "BehaviorDriverSpec",
    "DriverConfig",
    "extract_behavior_refs",
    "expand_behavior_bundle",
    "phase_scaffold",
]
