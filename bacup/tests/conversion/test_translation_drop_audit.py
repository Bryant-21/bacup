from pathlib import Path

import yaml

from tools.schema_forge.schema_diff import load_generated_rust_schema
from tools.schema_forge.translation_drop_audit import build_translation_drop_audit


ROOT = Path(__file__).resolve().parents[3]
MAP_PATH = (
    ROOT
    / "bacup/py_bacup_lib/native/conversion/src/embedded/translation_maps/fo76_to_fo4.yaml"
)
SCHEMA_DIR = ROOT / "py_creation_lib/native/esp/generated"
JUSTIFICATIONS_PATH = (
    ROOT / "tools/schema_forge/data/fo76_to_fo4_drop_justifications.yaml"
)


def test_fo76_to_fo4_shared_exact_drops_are_explicitly_justified() -> None:
    translation_map = yaml.safe_load(MAP_PATH.read_text(encoding="utf-8")) or {}
    justifications = (
        yaml.safe_load(JUSTIFICATIONS_PATH.read_text(encoding="utf-8")) or {}
    )
    report = build_translation_drop_audit(
        translation_map,
        load_generated_rust_schema(SCHEMA_DIR / "fo76.rs"),
        load_generated_rust_schema(SCHEMA_DIR / "fo4.rs"),
        justifications=justifications,
    )
    actual = {
        f"{row['record']}.{row['drop']}"
        for row in report["drops"]
        if row["category"] == "shared_exact_contract"
    }

    assert actual == set(justifications)
    assert report["summary"]["unjustified_shared_exact"] == 0
