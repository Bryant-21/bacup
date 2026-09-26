import json
import struct
import zlib

from bacup_lib import perk_cards_ui as ui
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.translations import read_table, translation_path
from creation_lib.swf import native_runtime


def swf(tags):
    body = b"\x08\x00\x00\x1e\x01\x00"
    for code, payload in tags + [(0, b"")]:
        body += struct.pack("<HI", code << 6 | 63, len(payload)) + payload
    return b"CWS\x25" + struct.pack("<I", len(body) + 8) + zlib.compress(body)


def source(tmp_path):
    root = tmp_path / "source"
    interface = root / "interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text(
        "$Perks_Rank\tRank\n$EXIT\tExit\n$LEGENDARYPERKSBUTTON\tLegendary Perks\n"
        "$PickAPerkCount\tPick a Perk ({1})\n$UNEQUIP\tUnequip\n$Choose\tChoose\n"
        "$PERKJOKECOUNT\t1\n$PERKJOKE1\tA gum wrapper joke.\n", encoding="utf-16")
    for name in [ui.MENU, "perkcardpacks.swf"]:
        (interface / name).write_bytes(swf([(71, b"library.swf\0\1\0\0\0"), (82, b"script"), (2, b"art")]))
    (interface / "library.swf").write_bytes(swf([(2, b"library art")]))
    (interface / "messageboxmenu.swf").write_bytes(swf([(82, b"script"), (2, b"prompt art")]))
    return root


def test_conversion_preserves_source_art_and_copies_complete_import_closure(tmp_path, monkeypatch):
    def augment(data, classes, dependencies):
        assert set(classes) == {"PerksMenu", "PerkCardRankConfirmation", "FilteredCarousel"}
        assert len(dependencies) == 4
        return swf([(code, b"bridge" if code == 82 else payload) for code, payload in swf_tags(data) if code])

    def patch(data, cls, method, pattern, replacement):
        assert cls in ("PerksMenu", "FilteredCarousel")
        if pattern == [["getproperty", "onLevelUpButtonPressed"]]:
            assert replacement == [["callproperty", "B21PerksHandler", 0]]
        elif pattern == [["getproperty", "onBoostPress"]]:
            assert replacement == [["callproperty", "B21SPECIALHandler", 0]]
        elif method == "ProcessUserEvent":
            assert replacement in ([["callpropvoid", "B21OpenSPECIAL", 0]], [["callpropvoid", "B21OpenPerks", 0]])
        elif method == "$constructor":
            assert replacement[1] == ["callpropvoid", "B21Initialize", 0]
        elif method == "onRespecPicked":
            assert pattern == [["callpropvoid", "dispatchEvent", 1]]
            assert replacement[-1] == ["callpropvoid", "B21FreeRespec", 0]
        elif method == "SetButtons":
            assert replacement == [["callproperty", "B21SetButtons", 0]]
        elif method == "GetFilterText":
            assert replacement[-1] == ["callproperty", "B21FilterText", 1]
        elif pattern == [["getproperty", "shouldShowPickSpecial"]]:
            assert replacement == [["callproperty", "B21PickPerkOnly", 0]]
        elif method == "onSpecialPicked":
            assert replacement in ([["callproperty", "B21AllocationValues", 0]], [["callproperty", "B21PicksAfterSpecial", 0]])
        else:
            assert method in ("onPerkPickAnimFinished", "CanCardBeRankedUp", "onRankUpPress", "ShowRankUpForAquiringCard")
            assert replacement in ([["callproperty", "B21RankCandidates", 1]], [["callpropvoid", "B21SetData", 2]])
        return data

    monkeypatch.setattr(native_runtime, "augment_as3_classes", augment)
    monkeypatch.setattr(native_runtime, "abc_string_pools", lambda data: [])
    monkeypatch.setattr(native_runtime, "patch_as3_method", patch)
    monkeypatch.setattr(ui, "pack_prompt_movie", lambda data: data)
    root = source(tmp_path)
    output = tmp_path / "out"
    original = (root / "interface" / ui.MENU).read_bytes()
    report = ui.convert_perk_card_ui(root, output)
    assert len(report["files"]) == 4
    assert (root / "interface" / ui.MENU).read_bytes() == original
    assert (output / ui.OUTPUT / "library.swf").read_bytes() == (root / "interface/library.swf").read_bytes()
    assert (output / ui.OUTPUT / "perkcardpacks.swf").read_bytes() == (root / "interface/perkcardpacks.swf").read_bytes()
    assert (output / ui.OUTPUT / "packprompt.swf").read_bytes() == (root / "interface/messageboxmenu.swf").read_bytes()
    assert swf_tags((output / ui.OUTPUT / ui.MENU).read_bytes())[2] == (2, b"art")
    assert json.loads((output / ui.OUTPUT / "conversion.json").read_text()) == report
    table = dict(line.split("\t", 1) for line in read_table(translation_path(output)))
    assert table["$LEGENDARYPERKSBUTTON"] == "Legendary Perks"
    assert table["$PickAPerkCount"] == "Pick a Perk ({1})"
    assert table["$UNEQUIP"] == "Unequip"
    assert table["$Choose"] == "Choose"
    assert table["$PERKJOKECOUNT"] == "1"
    assert table["$PERKJOKE1"] == "A gum wrapper joke."
