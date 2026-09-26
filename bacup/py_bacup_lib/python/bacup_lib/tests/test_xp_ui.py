import struct

from bacup_lib import xp_ui as ui


def contract(meter="HUDMenu_fla.XPMeter_420", level="HUDMenu_fla.LevelUpAnimation_443"):
    return ui.XPSourceContract(meter_class=meter, level_up_class=level)


def movie(dependencies=(), code=b"original"):
    tags = [(71, name.encode() + b"\0\1\0\0\0") for name in dependencies]
    tags += [(2, b"source XP art"), (82, code), (1, b""), (0, b"")]
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(
        struct.pack("<HI", (tag << 6) | 63, len(data)) + data for tag, data in tags
    )
    return b"FWS\x11" + struct.pack("<I", len(body) + 8) + body


def source(tmp_path, dependencies=("fonts_en.swf",)):
    root = tmp_path / "source"
    interface = root / "interface"
    interface.mkdir(parents=True)
    (interface / ui.SOURCE).write_bytes(movie(dependencies))
    (interface / "fonts_en.swf").write_bytes(movie())
    lines = [f"{key}\t{key.removeprefix('$')}" for key in sorted(ui.TRANSLATION_KEYS)]
    (interface / "translate_en.txt").write_text("\n".join(lines) + "\n", encoding="utf-16")
    for path in ui.SOUNDS:
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(b"source " + path.name.encode())
    return root


def test_source_art_audio_and_contract_manifest_are_repeatable(tmp_path, monkeypatch):
    root = source(tmp_path)
    source_contract = contract(
        "HUDMenu_fla.XPMeter_777", "HUDMenu_fla.LevelUpAnimation_888"
    )
    monkeypatch.setattr(ui, "verify_source_contract", lambda data: source_contract)
    output = tmp_path / "converted/data"
    first = ui.convert_xp_ui(root, output)
    second = ui.convert_xp_ui(root, output)
    assert first == second
    assert first["source_layout"] == ui.SOURCE_LAYOUT
    assert first["source_sound_descriptors"]["text"] == "057B44:SeventySix.esm"
    assert first["files"] == []
    assert first["presentation_owner"] == "StatusHUD"
    assert first["menu"].endswith("StatusHUD/statushud.swf")
    assert not list((output / ui.OUTPUT).glob("*.swf"))
    for path in ui.SOUNDS:
        assert (output / path).read_bytes() == (root / path).read_bytes()
    translations = (ui.translation_path(output)).read_text(encoding="utf-16")
    assert "$LEVELUP\tLEVELUP" in translations
