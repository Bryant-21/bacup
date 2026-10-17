from pathlib import Path

import pytest

from bacup_lib import death_screen_ui as ui
from bacup_lib.tests.test_legendary_perks_ui import swf


def import_tag(url):
    return (71, url.encode() + b"\0\x01\x00\x00\x00")


def source(tmp_path, keys="$RESPAWN\tRESPAWN\n$Other\tNo\n"):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text(keys, encoding="utf-16")
    (interface / ui.MENU).write_bytes(swf([import_tag("BSButtonHintBar.swf"), (1, b"")]))
    (interface / "BSButtonHintBar.swf").write_bytes(swf([(1, b"")]))
    return interface.parent


def test_menu_and_imports_are_written_with_the_tales_bridge(tmp_path, monkeypatch):
    calls = []
    bridged = swf([(82, b"B21 death screen bridge"), (1, b"")])

    def replace(movie, replacements, dependencies):
        calls.append((set(replacements), len(list(dependencies))))
        return bridged

    monkeypatch.setattr(ui, "replace_classes", replace)
    output = tmp_path / "output"
    manifest = ui.convert_death_screen_ui(source(tmp_path), output)

    assert calls == [({"DeathRespawnMenu"}, 2)]
    assert (output / ui.OUTPUT / ui.MENU).read_bytes() == bridged
    assert (output / ui.OUTPUT / "bsbuttonhintbar.swf").is_file()
    assert manifest["code_object"] == "root1.DeathRespawnMenu"
    assert manifest["menu"] == (ui.OUTPUT / ui.MENU).as_posix()
    assert [row["path"] for row in manifest["files"]] == [
        (ui.OUTPUT / name).as_posix() for name in ("bsbuttonhintbar.swf", ui.MENU)]
    table = ui.translation_path(output).read_bytes().decode("utf-16")
    assert "$RESPAWN\tRESPAWN" in table
    assert "$Other" not in table


def test_missing_respawn_text_fails(tmp_path):
    with pytest.raises(ValueError, match=r"\$RESPAWN"):
        ui.convert_death_screen_ui(source(tmp_path, keys="$Other\tNo\n"), tmp_path / "output")


def test_real_menu_bridge_exposes_the_native_contract(tmp_path):
    root = Path(__file__).resolve().parents[5] / "extracted/fo76"
    if not (root / "interface" / ui.MENU).is_file():
        pytest.skip("FO76 source data unavailable")
    from creation_lib.swf import native_runtime

    ui.convert_death_screen_ui(root, tmp_path)
    movie = (tmp_path / ui.OUTPUT / ui.MENU).read_bytes()
    traits = {row["name"] for row in native_runtime.abc_class_outline(movie, "DeathRespawnMenu")["instance_traits"]}
    assert {"RespawnSecondsField", "ButtonHintBar_mc", "ModalSetBounty_mc", "Leaderboard_mc",
            "BGSCodeObj", "B21SetDeathData"} <= traits


def class_only_placements(movie):
    import struct
    import zlib

    from bacup_lib.legendary_perks_ui import swf_tags

    if movie[:3] == b"CWS":
        movie = b"FWS" + movie[3:8] + zlib.decompress(movie[8:])
    found = []

    def scan(tags, owner):
        for code, body in tags:
            if code == 70 and body[1] & 8 and not body[0] & 2:
                found.append((owner, body[4:body.index(b"\0", 4)].decode()))
            elif code == 39:
                offset, nested = 4, []
                while offset < len(body):
                    header, = struct.unpack_from("<H", body, offset)
                    offset += 2
                    size = header & 63
                    if size == 63:
                        size, = struct.unpack_from("<I", body, offset)
                        offset += 4
                    nested.append((header >> 6, body[offset:offset + size]))
                    offset += size
                scan(nested, int.from_bytes(body[:2], "little"))

    scan(swf_tags(movie), 0)
    return found


def test_real_menu_places_the_respawn_hint_bar_by_character(tmp_path):
    # FO4's Scaleform does not instantiate FO76's class-only placements: the RESPAWN hint never drew in
    # game (2026-10-04) because ButtonHintBar_mc stayed null.
    root = Path(__file__).resolve().parents[5] / "extracted/fo76"
    if not (root / "interface" / ui.MENU).is_file():
        pytest.skip("FO76 source data unavailable")
    from creation_lib.swf import native_runtime

    ui.convert_death_screen_ui(root, tmp_path)
    movie = (tmp_path / ui.OUTPUT / ui.MENU).read_bytes()
    symbols = {name for _, name in native_runtime.list_symbols(movie)}
    assert {"Shared.AS3.BSButtonHintBar", "Shared.AS3.BSButtonHint"} <= symbols
    assert not [name for _, name in class_only_placements(movie) if name == "Shared.AS3.BSButtonHintBar"]
