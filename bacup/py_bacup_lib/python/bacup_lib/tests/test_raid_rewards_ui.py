import hashlib
import json
import struct

from bacup_lib import raid_rewards_ui as ui
from bacup_lib.translations import translation_path
from creation_lib.swf import native_runtime

IMPORTS = ["fonts_en.swf", "BSButtonHintBar.swf", "ChallengeRewardIconLibrary.swf"]


def movie(imports=(), action=b"stock", artwork=b"artwork"):
    tags = []
    for name in imports:
        payload = name.encode() + b"\0\1\0\0\0"
        tags.append(struct.pack("<HI", (71 << 6) | 63, len(payload)) + payload)
    tags.append(struct.pack("<HI", (82 << 6) | 63, len(action)) + action)
    tags.append(struct.pack("<HI", (2 << 6) | 63, len(artwork)) + artwork)
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(tags) + b"\0\0"
    return b"FWS\x11" + struct.pack("<I", len(body) + 8) + body


def write_translations(interface, keys):
    (interface / "translate_en.txt").write_text(
        "\n".join(f"{key}\t{key} text" for key in sorted(keys)) + "\n$Unused\tNo\n", encoding="utf-16")


def setup_source(tmp_path):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / ui.MENU).write_bytes(movie(IMPORTS))
    for name in IMPORTS:
        (interface / name.lower()).write_bytes(movie())
    write_translations(interface, ui.MOVIE_TRANSLATION_KEYS)
    return interface


def test_converter_replaces_only_document_code_and_preserves_import_closure(tmp_path, monkeypatch):
    interface = setup_source(tmp_path)
    source = (interface / ui.MENU).read_bytes()

    def compile_bridge(data, sources, dependencies):
        assert set(sources) == {"UniversalRewardsMenu"}
        assert len(dependencies) == 4
        assert '"$RAIDS REWARDS":"$RAIDS REWARDS text"' in sources["UniversalRewardsMenu"]
        assert "__RAID_TRANSLATIONS__" not in sources["UniversalRewardsMenu"]
        return movie(IMPORTS, b"bridge")

    monkeypatch.setattr(native_runtime, "replace_as3_classes", compile_bridge)
    output = tmp_path / "converted/data"
    result = ui.convert_raid_rewards_ui(interface.parent, output)
    assert (interface / ui.MENU).read_bytes() == source
    assert result["code_object"] == "root1"
    assert result["menu"] == "Interface/B21/TalesFromAppalachia/Raids/universalrewardsmenu.swf"
    assert len(result["files"]) == 4
    assert json.loads((output / ui.OUTPUT / "conversion.json").read_text()) == result
    converted = output / ui.OUTPUT / ui.MENU
    entry = next(item for item in result["files"] if item["path"].endswith(ui.MENU))
    assert entry["sha256"] == hashlib.sha256(converted.read_bytes()).hexdigest()
    for name in IMPORTS:
        assert (output / ui.OUTPUT / name.lower()).is_file()
    keys = {line.split("\t", 1)[0] for line in translation_path(output).read_text(encoding="utf-16").splitlines()}
    assert ui.MERGED_TRANSLATION_KEYS <= keys
    assert "$wt" not in keys and "$Unused" not in keys
