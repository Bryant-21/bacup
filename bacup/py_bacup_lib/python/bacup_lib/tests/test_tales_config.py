from __future__ import annotations

from bacup_lib import tales_config as tc


def test_synced_template_matches_schema_defaults():
    ini = tc.parse_ini(tc.template_text())
    schema = tc.load_schema()
    assert tc.layout_version(tc.template_text()) == schema.layout_version
    for setting in schema.settings():
        if setting.kind == "action":
            continue
        assert tc.is_default(ini, setting), setting.id


def test_set_value_replaces_in_place_and_keeps_padding_and_crlf():
    text = "; top\r\n[HUD]\r\nbA = 1\r\n; note\r\nbB=0\r\n"
    assert tc.set_value(text, "hud", "BA", "0") == "; top\r\n[HUD]\r\nbA = 0\r\n; note\r\nbB=0\r\n"


def test_set_value_inserts_after_last_setting_or_appends_section():
    text = "[HUD]\nbA=1\n; trailing comment\n\n[Menus]\nbX=1\n"
    assert tc.set_value(text, "HUD", "bNew", "1") == "[HUD]\nbA=1\nbNew=1\n; trailing comment\n\n[Menus]\nbX=1\n"
    assert tc.set_value("[HUD]\nbA=1", "Raids", "bEnabled", "0") == "[HUD]\nbA=1\n\n[Raids]\nbEnabled=0\n"


def test_migrate_v1_renames_values_and_copies_tables():
    v1 = (
        "[QuickBoy]\niEnabled=false\n"
        "[Respawn]\niEnabled=0\n"
        "[EquipmentConditionTiers]\nDefault=1,2\nMine=3,4\n"
    )
    out = tc.parse_ini(tc.migrate_v1(v1, tc.template_text()))
    assert out["general"]["iversion"] == "2"
    assert out["pipboy"]["bquickboy"] == "0"
    assert out["world"]["irespawnmode"] == "0"
    assert out["conditiontiers"] == {"default": "1,2", "mine": "3,4"}


def test_carry_forward_keeps_deployed_values_on_the_new_template():
    deployed = (
        "[General]\niVersion=2\n"
        "[HUD]\nbDamageNumbers=1\n"
        "[ConditionTiers]\nDefault=10,20\n"
        "[Legendary]\nsFallbackAbilities=abc\n"
    )
    text = tc.carry_forward(deployed)
    ini = tc.parse_ini(text)
    assert text.startswith("; Tales from Appalachia settings.")
    assert ini["hud"]["bdamagenumbers"] == "1"
    assert ini["hud"]["bhealthstatus"] == "1"
    assert ini["conditiontiers"] == {"default": "10,20"}
    assert ini["legendary"]["sfallbackabilities"] == "abc"
    assert tc.carry_forward(None) == tc.template_text()


def test_apply_edits_writes_known_settings_only(tmp_path):
    deployed = "[General]\niVersion=2\n[HUD]\nbCrosshair=0\n"
    (tmp_path / tc.INI_RELATIVE_PATH).parent.mkdir(parents=True)
    (tmp_path / tc.INI_RELATIVE_PATH).write_text(deployed, encoding="utf-8")
    edits = {"HUD/bDamageNumbers": "1", "Nope/bMissing": "1"}

    path = tc.write_install_ini(tmp_path, tc.read_deployed(tmp_path), edits)

    ini = tc.parse_ini(path.read_text(encoding="utf-8"))
    assert ini["hud"]["bcrosshair"] == "0"
    assert ini["hud"]["bdamagenumbers"] == "1"
    assert "nope" not in ini
    assert tc.parse_ini(tc.effective_text(tmp_path, {})) == ini


def test_value_formatting_matches_tales():
    assert tc.format_number(1.0, 0.05) == "1.0"
    assert tc.format_number(7.5, 0.5) == "7.5"
    assert tc.format_number(20, 1) == "20"
    assert tc.format_number(0.25, 0.25) == "0.25"
    assert tc.format_bool("true", False) == "false"
    assert tc.format_bool("1", True) == "1"
    assert tc.key_code("72,0") == 72
    assert tc.key_code("junk") == 0
    assert tc.format_key(0) == "0,0"


def test_respawn_mode_falls_back_to_the_old_world_key():
    setting = next(s for s in tc.load_schema().settings() if s.key == "iRespawnMode")
    assert tc.read(tc.parse_ini("[World]\nbRespawn=0\n"), setting) == "0"
    assert tc.read(tc.parse_ini(""), setting) == "1"


def test_quick_boy_own_color_is_one_row_over_three_channel_keys():
    schema = tc.load_schema()
    own = next(s for s in schema.settings() if s.id == "PipBoy/iColor")
    mode = next(s for s in schema.settings() if s.id == "PipBoy/iQuickBoyColor")
    assert own.kind == "color" and own.default == "7BAAD9" and own.selects == "iQuickBoyColor=1"
    assert mode.kind == "choice" and mode.default == "1"
    assert {c.value: c.label for c in mode.choices} == {"0": "Match Pip-Boy", "2": "Match HUD", "1": "Own Color"}

    assert tc.read_color({}, own) == 0x7BAAD9
    assert tc.read_color(tc.parse_ini("[PipBoy]\niColorR=300\niColorG=-4\niColorB=junk\n"), own) == 0xFF00D9
    assert tc.parse_color("#7baad9") == 0x7BAAD9 and tc.parse_color("7BAAD") == 0
    assert tc.format_color(0x0A0B0C) == "0A0B0C"

    text = tc.apply_edits("[PipBoy]\nbQuickBoy=1\n", {"PipBoy/iColor": "E4E6A4", "PipBoy/iQuickBoyColor": "1"}, schema)
    ini = tc.parse_ini(text)
    assert [ini["pipboy"][k] for k in ("icolorr", "icolorg", "icolorb", "iquickboycolor")] == ["228", "230", "164", "1"]
    assert tc.read_color(ini, own) == 0xE4E6A4 and not tc.is_default(ini, own)
    assert tc.is_default(tc.parse_ini(tc.set_color("", own, 0x7BAAD9)), own)


def _search(query: str) -> set[str]:
    return {s.id for page in tc.load_schema().pages for group in page.groups for s in group.settings
            if tc.matches_search(page, group, s, query)}


def test_search_matches_the_tales_config_menu():
    area_loot = _search("area loot")
    assert {f"AreaLoot/{key}" for key in ("bEnabled", "bQuickLoot", "bHighlight", "bPlayerKillsOnly", "fDistance",
                                          "fPollSeconds")} <= area_loot
    assert "HUD/bQuickLoot" in _search("looting")
    assert _search("fdistance") == {"AreaLoot/fDistance"}
    assert _search("atlantic") == {"Menus/sMenuMusic"}
    assert _search("AREA  Loot") == area_loot
    assert not _search("quick zzz")
    assert "World/iRespawnMode" not in _search("world")


def test_carry_forward_keeps_hand_written_companion_cards():
    existing = tc.template_text().replace("079249:Fallout4.esm=Chem Fiend", "001002:ellen.esp=random")
    out = tc.carry_forward(existing)
    assert "001002:ellen.esp=random" in out
    assert "079249:Fallout4.esm=Chem Fiend" not in out
