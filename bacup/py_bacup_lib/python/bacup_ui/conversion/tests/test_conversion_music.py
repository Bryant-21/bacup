import time
from pathlib import Path

from bacup_ui.conversion.music import ConversionMusicPlayer, prepare_theme_tracks


def test_prepare_theme_tracks_decodes_xwm_once_and_keeps_main_theme_first(tmp_path):
    source_dir = tmp_path / "skyrimse" / "music" / "special"
    source_dir.mkdir(parents=True)
    (source_dir / "mus_special_wordofpower_01.xwm").write_bytes(b"xwm")
    (source_dir / "mus_maintheme.xwm").write_bytes(b"xwm")
    decoded = []

    def decode(source: Path, destination: Path) -> Path:
        decoded.append(source.name)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(b"R" * 64)
        return destination

    cache_dir = tmp_path / "cache"
    first = prepare_theme_tracks((source_dir,), cache_dir, decoder=decode)
    second = prepare_theme_tracks((source_dir,), cache_dir, decoder=decode)

    assert [track.name for track in first] == [
        "mus_maintheme.wav",
        "mus_special_wordofpower_01.wav",
    ]
    assert second == first
    assert decoded == ["mus_maintheme.xwm", "mus_special_wordofpower_01.xwm"]


def test_player_returns_when_every_track_fails():
    def fail_track(
        _track: Path,
        _player: ConversionMusicPlayer,
    ) -> None:
        raise RuntimeError("no audio device")

    player = ConversionMusicPlayer(track_player=fail_track)
    player.start((Path("first.wav"), Path("second.wav")))

    deadline = time.monotonic() + 1.0
    while player.playing and time.monotonic() < deadline:
        time.sleep(0.01)

    assert player.playing is False
