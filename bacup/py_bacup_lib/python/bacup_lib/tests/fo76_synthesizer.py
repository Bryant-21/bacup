"""Synthesize FO76 source records from FO4 fan ESP records.

Reads the translation map YAML and mechanically reverses each transform
to produce synthetic FO76 input. This lets us test the translator's
accuracy by doing: FO4 -> synthesize FO76 -> translate -> compare to FO4.
"""
from __future__ import annotations

import copy

import yaml

from bacup_lib.native_maps import native_translation_maps_dir

# Dummy values for dropped fields and added-back subfields
_DUMMY_INT = 0
_DUMMY_STR = "FO76_PLACEHOLDER"
_DUMMY_HEX = "0x00000000"
_CURVETABLE_FK = "AAAAAA:SeventySix.esm"


class FO76Synthesizer:
    """Reverse translation map transforms to produce synthetic FO76 input."""

    def __init__(self):
        self._maps: dict[str, dict] = {}
        self._source_esm: str = ""
        self._target_esm: str = ""

    def load_map(self, source_game: str, target_game: str) -> None:
        filename = f"{source_game}_to_{target_game}.yaml"
        path = native_translation_maps_dir() / filename
        if not path.is_file():
            self._maps = {}
            return
        with open(path, encoding="utf-8") as f:
            self._maps = yaml.safe_load(f) or {}

    def synthesize(self, fo4_record: dict, record_type: str) -> dict:
        """Produce a synthetic FO76 record from an FO4 fan ESP record."""
        type_map = self._maps.get(record_type)
        if type_map is None:
            return dict(fo4_record)

        result = copy.deepcopy(fo4_record)
        transforms = type_map.get("transforms", {})
        drop_list = type_map.get("drop", [])
        defaults = type_map.get("defaults", {})

        # 0. Remove fields that only exist because of FO4-side defaults injection.
        #    Must happen before transform reversal so scale_nested doesn't alter
        #    default values and break the equality check.
        for key, default_val in defaults.items():
            if key in result and result[key] == default_val:
                del result[key]

        # 1. Reverse transforms on existing fields
        for field_name, transform in transforms.items():
            if field_name not in result:
                continue
            t = transform.get("type", "")

            if t == "remap_formkey":
                source_esm = transform.get("source_esm", "")
                target_esm = transform.get("target_esm", "")
                result[field_name] = self._reverse_remap(
                    result[field_name], source_esm, target_esm,
                )

            elif t == "strip_subfields":
                if isinstance(result[field_name], dict):
                    removed = transform.get("remove", [])
                    for subfield in removed:
                        result[field_name][subfield] = _DUMMY_HEX

            elif t == "trim_languages":
                if isinstance(result[field_name], dict) and "Values" in result[field_name]:
                    val = result[field_name]
                    eng = next(
                        (v for v in val["Values"] if v.get("Language") == "English"),
                        None,
                    )
                    text = eng["String"] if eng else ""
                    val["Values"].append(
                        {"Language": "ChineseSimplified", "String": text}
                    )

            elif t == "scale":
                factor = transform.get("factor", 1.0)
                if isinstance(result[field_name], (int, float)) and factor != 0:
                    result[field_name] = result[field_name] / factor

            elif t == "clamp_max":
                # Reverse: cannot recover original value above the cap; leave as-is.
                pass

            elif t == "scale_nested":
                subfield_factors: dict[str, float] = transform.get("subfields", {})
                if isinstance(result[field_name], dict) and subfield_factors:
                    for sub, factor in subfield_factors.items():
                        if sub in result[field_name] and factor != 0:
                            try:
                                result[field_name][sub] = float(result[field_name][sub]) / float(factor)
                            except (TypeError, ValueError):
                                pass

            elif t == "flatten_curvetable":
                result[field_name] = _CURVETABLE_FK

            elif t == "enum_map":
                mapping = transform.get("map", {})
                reverse = {v: k for k, v in mapping.items()}
                val = result[field_name]
                str_val = str(val)
                if str_val in reverse:
                    result[field_name] = reverse[str_val]

        # 2. Add back dropped fields with dummies
        for field_name in drop_list:
            if field_name not in result:
                result[field_name] = _DUMMY_INT

        return result

    @staticmethod
    def _reverse_remap(value, source_esm: str, target_esm: str):
        """Recursively swap target_esm back to source_esm in FormKey strings."""
        if isinstance(value, str):
            return value.replace(target_esm, source_esm)
        if isinstance(value, list):
            return [FO76Synthesizer._reverse_remap(v, source_esm, target_esm) for v in value]
        if isinstance(value, dict):
            return {k: FO76Synthesizer._reverse_remap(v, source_esm, target_esm) for k, v in value.items()}
        return value
