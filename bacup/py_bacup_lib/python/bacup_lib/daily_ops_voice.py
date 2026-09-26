from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

RECIPE = Path(__file__).with_name("resources") / "daily_ops/radio_topics.json"
OUTPUT = Path("Sound/FX/B21/TalesFromAppalachia/DailyOps")
CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/daily_ops_voice.json")
SPEAKER = "bs01_npcm_dailyops_dodge"


def fuz_audio(data: bytes) -> bytes:
    if len(data) < 12 or data[:4] != b"FUZE":
        raise ValueError("Invalid Daily Ops FUZ header")
    version, lip_size = struct.unpack_from("<II", data, 4)
    audio = data[12 + lip_size:]
    if version != 1 or len(audio) < 12 or audio[:4] != b"RIFF" or audio[8:12] not in (b"WAVE", b"XWMA"):
        raise ValueError("Daily Ops FUZ has no valid XWM payload")
    if struct.unpack_from("<I", audio, 4)[0] + 8 != len(audio):
        raise ValueError("Daily Ops XWM payload is truncated")
    return audio


def convert_daily_ops_voice(source_root: Path, output_mod: Path) -> dict:
    recipe = json.loads(RECIPE.read_text(encoding="utf-8"))
    topics, files = {}, {}
    for topic, variants in recipe.items():
        topics[topic] = []
        for variant in variants:
            clips = []
            for number in variant["responses"]:
                name = f"{variant['info']:08x}_{number}"
                source = source_root / f"sound/voice/seventysix.esm/{SPEAKER}/{name}.fuz"
                raw = source.read_bytes()
                path = (OUTPUT / f"{name}.xwm").as_posix()
                audio = fuz_audio(raw)
                files[path] = audio
                clips.append({"path": path, "source_sha256": hashlib.sha256(raw).hexdigest(),
                              "sha256": hashlib.sha256(audio).hexdigest()})
            topics[topic].append({"info": variant["info"], "clips": clips})
    manifest = {"version": 1, "speaker": SPEAKER, "topics": topics,
                "recipe_sha256": hashlib.sha256(RECIPE.read_bytes()).hexdigest()}
    for path, data in files.items():
        target = output_mod / "data" / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    target = output_mod / CATALOG
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest
