import json
import struct

import pytest

from bacup_lib import daily_ops_voice as voice
from bacup_lib.daily_ops_ui import empty_hud_timeline
from bacup_lib.legendary_perks_ui import swf_tags


def fuz(kind=b"XWMA"):
    audio = b"RIFF" + struct.pack("<I", 8) + kind + b"data"
    return b"FUZE" + struct.pack("<II", 1, 3) + b"lip" + audio


@pytest.mark.parametrize("kind", [b"XWMA", b"WAVE"])
def test_retains_voice_payload_exactly(kind):
    assert voice.fuz_audio(fuz(kind)) == fuz(kind)[15:]


@pytest.mark.parametrize("data", [b"", fuz()[:14], fuz()[:-1], fuz(b"FAIL")])
def test_rejects_bad_voice(data):
    with pytest.raises(ValueError):
        voice.fuz_audio(data)


def test_catalog_is_loose_and_multiline_audio_is_packed(tmp_path, monkeypatch):
    recipe = tmp_path / "recipe.json"
    recipe.write_text(json.dumps({"10": [{"info": 20, "responses": [1, 2]}]}))
    monkeypatch.setattr(voice, "RECIPE", recipe)
    source = tmp_path / "source"
    directory = source / f"sound/voice/seventysix.esm/{voice.SPEAKER}"
    directory.mkdir(parents=True)
    for n in (1, 2):
        (directory / f"00000014_{n}.fuz").write_bytes(fuz())
    output = tmp_path / "mod"
    catalog = voice.convert_daily_ops_voice(source, output)
    clips = catalog["topics"]["10"][0]["clips"]
    assert len(clips) == 2
    assert (output / voice.CATALOG).is_file()
    for clip in clips:
        assert (output / "data" / clip["path"]).read_bytes() == fuz()[15:]
    assert not (output / "Sound").exists()


def test_missing_voice_fails_before_writes(tmp_path, monkeypatch):
    recipe = tmp_path / "recipe.json"
    recipe.write_text(json.dumps({"10": [{"info": 20, "responses": [1]}]}))
    monkeypatch.setattr(voice, "RECIPE", recipe)
    with pytest.raises(FileNotFoundError):
        voice.convert_daily_ops_voice(tmp_path / "source", tmp_path / "output")
    assert not (tmp_path / "output").exists()


def test_hud_does_not_construct_source_game_root_children():
    def tag(code, body):
        return struct.pack("<HI", code << 6 | 63, len(body)) + body
    sprite = struct.pack("<HH", 42, 1) + tag(26, b"inside") + tag(1, b"") + tag(0, b"")
    body = b"\x08\x00\x00\x1e\x02\x00" + tag(39, sprite) + tag(26, b"root") + tag(1, b"") + tag(1, b"") + tag(0, b"")
    data = b"FWS\x11" + struct.pack("<I", len(body) + 8) + body
    result = empty_hud_timeline(data)
    tags = swf_tags(result)
    assert (39, sprite) in tags
    assert not any(code == 26 for code, _ in tags)
    assert sum(code == 1 for code, _ in tags) == 1
