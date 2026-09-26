import struct

from bacup_lib import reputation_ui as ui
from bacup_lib.legendary_perks_ui import swf_tags


def movie(dependencies=(), code=b"original", text_fields=()):
    tags = [(71, name.encode() + b"\0\1\0\0\0") for name in dependencies]
    tags += [(37, data) for data in text_fields]
    tags += [(2, b"source reputation art"), (82, code), (1, b""), (0, b"")]
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(
        struct.pack("<HI", (tag << 6) | 63, len(data)) + data for tag, data in tags
    )
    return b"FWS\x11" + struct.pack("<I", len(body) + 8) + body


def translations(interface):
    lines = [f"{key}\t{key.removeprefix('$')}" for key in sorted(ui.TRANSLATION_KEYS)]
    lines.append("$Unused\tUnused")
    (interface / "translate_en.txt").write_text("\n".join(lines) + "\n", encoding="utf-16")


def source(tmp_path, dependencies=("fonts_en.swf",)):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / ui.SOURCE).write_bytes(movie(dependencies))
    (interface / "fonts_en.swf").write_bytes(movie())
    (interface / "reputationlibrary.swf").write_bytes(movie())
    (interface / "socialreputationwidget.swf").write_bytes(movie(("reputationlibrary.swf",)))
    translations(interface)
    return interface


def test_live_hud_contract_excludes_social_list_and_is_repeatable(tmp_path, monkeypatch):
    interface = source(tmp_path)
    seen = []

    def replace(data, dependencies):
        assert ui.BRIDGE.is_file()
        assert len(list(dependencies)) == 2
        seen.append(True)
        return data.replace(b"original", b"bridge__")

    monkeypatch.setattr(ui, "build_movie", replace)
    output = tmp_path / "converted/data"
    first = ui.convert_reputation_ui(interface.parent, output)
    second = ui.convert_reputation_ui(interface.parent, output)
    assert first == second
    assert len(seen) == 2
    assert [entry["source"] for entry in first["files"]] == ["fonts_en.swf", ui.SOURCE]
    assert first["excluded_source_movies"] == ["reputationlibrary.swf", "socialreputationwidget.swf"]
    assert not (output / ui.OUTPUT / "socialreputationwidget.swf").exists()
    assert not (output / ui.OUTPUT / "reputationlibrary.swf").exists()
    converted = (output / ui.OUTPUT / ui.OUTPUT_MOVIE).read_bytes()
    assert (2, b"source reputation art") in swf_tags(converted)
    assert not (output / ui.OUTPUT / ui.SOURCE).exists()
    translation_text = (ui.translation_path(output)).read_text(encoding="utf-16")
    assert "$CraterReputation\t" in translation_text
    assert "$ReputationStatus6\t" in translation_text
    assert "$Unused\t" not in translation_text
