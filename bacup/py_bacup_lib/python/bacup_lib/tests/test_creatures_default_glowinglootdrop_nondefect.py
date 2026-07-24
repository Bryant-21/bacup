from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source
from creation_lib.esp import Plugin


REPO_ROOT = Path(__file__).resolve().parents[5]
TARGET_PLUGIN = REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm"
SCRIPT_NAME = "Creatures:_Default:GlowingLootDrop"
FO4_GLOWING_LOOT_DROP = {"plugin": "Fallout4.esm", "object_id": "21C007"}
SOURCE_CARRIER_IDS = (0x21C007, 0x21C008)
BASE_DEDUPED_CONSUMERS = {
    0x020BCC: 0x020BCC,
    0x59F61A: 0x11669F,
    0x11669F: 0x11669F,
    0x16CA42: 0x16CA42,
    0x1BBEAA: 0x1BBEAA,
}


def _fo4_data_dir() -> Path:
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if not configured:
        for line in (REPO_ROOT / ".env").read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                configured = line.split("=", 1)[1].strip().strip('"')
                break
    data_dir = Path(configured) / "Data"
    assert (data_dir / "Fallout4.esm").is_file(), data_dir
    return data_dir


def _source_consumers() -> list[dict[str, str]]:
    result = subprocess.run(
        [
            "modkit.exe",
            "--game",
            "fo76",
            "--format",
            "compact",
            "data",
            "refs",
            "21C007:SeventySix.esm",
            "-n",
            "10000",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def _actor_effects(record: dict) -> list[dict[str, str]]:
    return [
        field["ActorEffect"]["reference"]
        for field in record["fields"]
        if "ActorEffect" in field
    ]


def test_glowing_loot_carrier_dedupes_to_the_working_fo4_chain():
    consumers = _source_consumers()
    assert len(consumers) == 200
    assert {consumer["record_type"] for consumer in consumers} == {"NPC_"}

    target = Plugin.load(TARGET_PLUGIN, game="fo4", lazy_index=True)
    base = Plugin.load(_fo4_data_dir() / "Fallout4.esm", game="fo4", lazy_index=True)
    try:
        assert _script_patch_source(SCRIPT_NAME) is None
        assert all(target.read_authoring_record(form_id) is None for form_id in SOURCE_CARRIER_IDS)

        retained = 0
        deduped: dict[int, int] = {}
        for consumer in consumers:
            source_id = int(consumer["form_key"].split(":", 1)[0], 16)
            record = target.read_authoring_record(source_id)
            if record is None:
                deduped[source_id] = BASE_DEDUPED_CONSUMERS[source_id]
            else:
                retained += 1
                assert FO4_GLOWING_LOOT_DROP in _actor_effects(record)

        assert retained == 195
        assert deduped == BASE_DEDUPED_CONSUMERS
        for base_id in deduped.values():
            record = base.read_authoring_record(base_id)
            assert record is not None
            assert FO4_GLOWING_LOOT_DROP in _actor_effects(record)
    finally:
        target.close()
        base.close()
