import hashlib
import json
import struct

from bacup_lib import expedition_results_ui as ui
from creation_lib.swf import native_runtime


def movie(imports=(), action=b"stock", artwork=b"artwork"):
    tags = []
    for name in imports:
        payload = name.encode() + b"\0\1\0\0\0"
        tags.append(struct.pack("<HI", (71 << 6) | 63, len(payload)) + payload)
    tags.append(struct.pack("<HI", (82 << 6) | 63, len(action)) + action)
    tags.append(struct.pack("<HI", (2 << 6) | 63, len(artwork)) + artwork)
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(tags) + b"\0\0"
    return b"FWS\x11" + struct.pack("<I", len(body) + 8) + body


def setup_source(tmp_path):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / ui.MENU).write_bytes(movie(["BSButtonHintBar.swf", "fonts_en.swf"]))
    (interface / "bsbuttonhintbar.swf").write_bytes(movie())
    (interface / "fonts_en.swf").write_bytes(movie())
    (interface / "translate_en.txt").write_text(
        "\n".join(f"{key}\t{key} text" for key in sorted(ui.RESULT_TRANSLATION_KEYS)) + "\n$Unused\tNo\n",
        encoding="utf-8",
    )
    return interface


def test_converter_replaces_only_document_code_and_preserves_import_closure(tmp_path, monkeypatch):
    interface = setup_source(tmp_path)
    source = (interface / ui.MENU).read_bytes()

    def compile_bridge(data, sources, dependencies):
        assert set(sources) == {"ExpeditionsPostMatchMenu"}
        assert dependencies
        assert '"$XPD_AC01_MissionTitle":"$XPD_AC01_MissionTitle text"' in sources["ExpeditionsPostMatchMenu"]
        return movie(["BSButtonHintBar.swf", "fonts_en.swf"], b"bridge")

    monkeypatch.setattr(native_runtime, "replace_as3_classes", compile_bridge)
    output = tmp_path / "converted/data"
    result = ui.convert_expedition_results_ui(interface.parent, output)
    assert (interface / ui.MENU).read_bytes() == source
    assert result["code_object"] == "root1"
    assert len(result["files"]) == 3
    assert json.loads((output / ui.OUTPUT / "conversion.json").read_text()) == result
    converted = output / ui.OUTPUT / ui.MENU
    assert result["files"][1]["sha256"] == hashlib.sha256(converted.read_bytes()).hexdigest()
    assert (output / ui.OUTPUT / "bsbuttonhintbar.swf").is_file()
    text = (ui.translation_path(output)).read_text(encoding="utf-16")
    assert "$Unused" not in text
    assert ui.RESULT_TRANSLATION_KEYS <= {line.split("\t", 1)[0] for line in text.splitlines()}
