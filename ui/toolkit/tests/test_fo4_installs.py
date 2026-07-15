from ui.toolkit.settings import ToolkitSettings


def _mk(tmp_path):
    return ToolkitSettings(
        shared_path=tmp_path / "shared_settings.json",
        variant_path=tmp_path / "variant.json",
        editor_settings_path=tmp_path / "missing_editor_settings.json",
    )


def test_extra_installs_default_empty(tmp_path):
    assert _mk(tmp_path).get_fo4_extra_installs() == []


def test_set_and_get_extra_installs_roundtrip_through_disk(tmp_path):
    s = _mk(tmp_path)
    s.set_fo4_extra_installs(
        [{"label": "NextGen 1.10.984", "root_dir": r"D:\Games\Fallout4_NG"}]
    )
    assert s.get_fo4_extra_installs() == [
        {"label": "NextGen 1.10.984", "root_dir": r"D:\Games\Fallout4_NG"}
    ]

    reloaded = _mk(tmp_path)
    assert reloaded.get_fo4_extra_installs() == [
        {"label": "NextGen 1.10.984", "root_dir": r"D:\Games\Fallout4_NG"}
    ]


def test_set_extra_installs_drops_empty_root_and_fills_blank_label(tmp_path):
    s = _mk(tmp_path)
    s.set_fo4_extra_installs(
        [
            {"label": "  ", "root_dir": r"D:\Games\Fallout4_NG", "junk": 1},
            {"label": "Dropped", "root_dir": "   "},
        ]
    )
    assert s.get_fo4_extra_installs() == [
        {"label": "Fallout4_NG", "root_dir": r"D:\Games\Fallout4_NG"}
    ]


def test_install_choices_primary_first_then_extras(tmp_path):
    s = _mk(tmp_path)
    s._paths["fo4"]["root_dir"] = r"N:\Steam\Fallout 4"
    s.set_fo4_extra_installs([{"label": "NextGen", "root_dir": r"D:\FO4NG"}])

    choices = s.get_fo4_install_choices()

    assert len(choices) == 2
    assert choices[0]["primary"] is True
    assert choices[0]["root_dir"] == r"N:\Steam\Fallout 4"
    assert choices[1] == {"label": "NextGen", "root_dir": r"D:\FO4NG", "primary": False}


def test_install_choices_primary_only_when_no_extras(tmp_path):
    s = _mk(tmp_path)
    s._paths["fo4"]["root_dir"] = r"N:\Steam\Fallout 4"

    choices = s.get_fo4_install_choices()

    assert len(choices) == 1
    assert choices[0]["primary"] is True
    assert choices[0]["root_dir"] == r"N:\Steam\Fallout 4"
