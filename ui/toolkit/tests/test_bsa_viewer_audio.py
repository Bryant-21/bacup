from ui.bsa_viewer.workspace import _format_time, _is_audio_member, _is_texture_member


def test_bsa_viewer_audio_member_detection() -> None:
    assert _is_audio_member("sound/fx/weapon_fire.xwm")
    assert _is_audio_member("sound/voice/line_01.FUZ")
    assert _is_audio_member("sound/voice/line_01.wav")
    assert not _is_audio_member("textures/armor/body_d.dds")


def test_bsa_viewer_audio_time_formatting() -> None:
    assert _format_time(0.0) == "00:00"
    assert _format_time(65.9) == "01:05"
    assert _format_time(600.0) == "10:00"


def test_bsa_viewer_texture_member_detection() -> None:
    assert _is_texture_member("textures/armor/body_d.dds")
    assert not _is_texture_member("textures/armor/body_d.png")
