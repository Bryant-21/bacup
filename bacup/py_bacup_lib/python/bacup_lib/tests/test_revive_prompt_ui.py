import struct
from pathlib import Path

import pytest
from creation_lib.swf import native_runtime

from bacup_lib import revive_prompt_ui as ui
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.tests.test_legendary_perks_ui import swf
from bacup_lib.tests.test_quest_area_ui import short_tag, symbols, text


def named_sprite(character, *rows):
    tags = b"".join(
        short_tag(26, bytes([0x22 if name else 0x02]) + struct.pack("<HH", depth, child) +
                  (name.encode() + b"\0" if name else b""))
        for depth, child, name in rows)
    return struct.pack("<HH", character, 1) + tags + short_tag(1) + short_tag(0)


def source(tmp_path, keys="$CRITICALLY INJURED\tCRITICALLY INJURED\n$GIVEUP\tGIVE UP\n$Other\tNo\n",
           parts=("reviveTimer", "reviveTitle")):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text(keys, encoding="utf-16")
    rows = [(depth, child, name) for depth, child, name in ((1, 12, "reviveTimer"), (2, 14, "reviveTitle"))
            if name in parts] + [(3, 16, "ButtonHintBar_mc")]
    tags = [(69, b"\x08\0\0\0"),
            (37, text(11)), (39, named_sprite(12, (1, 11, "reviveTime_tf"))),
            (37, text(13)), (39, named_sprite(14, (1, 13, None))),
            (39, named_sprite(16)),
            (39, named_sprite(20, *rows)),
            (82, b"FO76 HUD code"), (76, symbols((20, ui.SYMBOL_STEM + "334"), (0, "HUDMenu"))), (1, b"")]
    (interface / ui.SOURCE).write_bytes(swf(tags))
    return interface.parent


def test_named_children_reads_place_object_names():
    assert ui.named_children(named_sprite(20, (1, 12, "reviveTimer"), (2, 16, None))) == {"reviveTimer": 12}


def test_prompt_movie_keeps_only_the_timer_and_title_art(tmp_path):
    output = tmp_path / "output"
    manifest = ui.convert_revive_prompt_ui(source(tmp_path), output)
    data = (output / ui.OUTPUT / ui.MOVIE).read_bytes()
    tags = swf_tags(data)
    defined = sorted(struct.unpack_from("<H", payload)[0] for code, payload in tags if code in (37, 39))
    assert defined == [11, 12, 13, 14]
    assert ui.symbol_ids(tags) == {"B21_ReviveTimerArt": 12, "B21_ReviveTitleArt": 14, "B21_RevivePrompt": 0}
    assert native_runtime.unbacked_symbol_classes(data) == []
    assert manifest["symbol"] == ui.SYMBOL_STEM + "334"
    table = ui.translation_path(output).read_bytes().decode("utf-16")
    assert "$CRITICALLY INJURED\tCRITICALLY INJURED" in table
    assert "$GIVEUP\tGIVE UP" in table
    assert "$Other" not in table


def test_missing_part_fails(tmp_path):
    with pytest.raises(ValueError, match="reviveTitle"):
        ui.convert_revive_prompt_ui(source(tmp_path, parts=("reviveTimer",)), tmp_path / "output")


def test_real_hud_prompt_converts(tmp_path):
    root = Path(__file__).resolve().parents[5] / "extracted/fo76"
    if not (root / "interface" / ui.SOURCE).is_file():
        pytest.skip("FO76 source data unavailable")
    manifest = ui.convert_revive_prompt_ui(root, tmp_path)
    movie = (tmp_path / ui.OUTPUT / ui.MOVIE).read_bytes()
    assert manifest["symbol"].startswith(ui.SYMBOL_STEM)
    assert native_runtime.unbacked_symbol_classes(movie) == []
    outline = native_runtime.abc_class_outline(movie, ui.ROOT_CLASS)
    assert "B21SetRevive" in {row["name"] for row in outline["instance_traits"]}
