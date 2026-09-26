from bacup_lib import translations


def test_ui_table_migrates_outside_archive_data_and_preserves_other_keys(tmp_path):
    data = tmp_path / "B21_Output" / "data"
    legacy = data / translations.LEGACY_TRANSLATIONS
    translations.write_table(legacy, ["$OtherUI\tKeep", "$Owned\tOld"])
    target = translations.translation_path(data)
    translations.write_table(target, ["$AlreadyMoved\tKeep", "$Owned\tNewer"])

    translations.merge_ui_translations(data, ["$ITEM STATS\tITEM STATS"])

    assert target == data.parent / "F4SE/Plugins/B21_TalesFromAppalachia_en.txt"
    assert not legacy.exists()
    assert not target.is_relative_to(data)
    assert translations.read_table(target) == [
        "$OtherUI\tKeep", "$AlreadyMoved\tKeep", "$Owned\tNewer", "$ITEM STATS\tITEM STATS",
    ]
    before = target.read_bytes()
    translations.merge_ui_translations(data, ["$ITEM STATS\tITEM STATS"])
    assert target.read_bytes() == before


def test_packaged_language_tables_merge_beside_english_table(tmp_path, monkeypatch):
    monkeypatch.setattr(translations, "packaged_overlay_languages", lambda: ["de"])
    monkeypatch.setattr(translations, "packaged_tales_lines",
                        lambda language="en": {"en": ["$Key\tCaps"], "de": ["$Key\tKronkorken"]}[language])
    accumulator = translations.translation_path(tmp_path / "data")
    german = translations.translation_path(tmp_path / "data", "de")
    translations.write_table(german, ["$FromFO76\tBehalten"])

    translations.merge_packaged(accumulator)

    assert german == accumulator.with_name("B21_TalesFromAppalachia_de.txt")
    assert translations.read_table(accumulator) == ["$Key\tCaps"]
    assert translations.read_table(german) == ["$FromFO76\tBehalten", "$Key\tKronkorken"]


def test_packaged_overlay_languages_exclude_english():
    assert "en" not in translations.packaged_overlay_languages()
