import threading
import time
from pathlib import Path

from bacup_ui.conversion.music import (
    ConversionMusicPlayer,
    discover_theme_tracks,
    prepare_theme_tracks,
    prioritize_music_tracks,
    ordered_theme_tracks,
)


def test_ordered_theme_tracks_places_original_main_theme_first(tmp_path):
    theme_dir = tmp_path / "music" / "special"
    theme_dir.mkdir(parents=True)
    for name in (
        "mus_special_wastelanders_maintheme.wav",
        "mus_maintheme.wav",
        "mus_76_special_mainmenu_storm.wav",
    ):
        (theme_dir / name).write_bytes(b"wav")
    (theme_dir / "notes.txt").write_text("ignore", encoding="utf-8")

    tracks = ordered_theme_tracks(theme_dir)

    assert [track.name for track in tracks] == [
        "mus_maintheme.wav",
        "mus_76_special_mainmenu_storm.wav",
        "mus_special_wastelanders_maintheme.wav",
    ]


def test_discover_theme_tracks_uses_first_populated_directory(tmp_path):
    empty = tmp_path / "empty"
    empty.mkdir()
    populated = tmp_path / "themes"
    populated.mkdir()
    expected = populated / "mus_maintheme.wav"
    expected.write_bytes(b"wav")

    assert discover_theme_tracks((tmp_path / "missing", empty, populated)) == (
        expected,
    )


def test_prioritize_music_tracks_places_main_theme_then_installed_main_title():
    tracks = prioritize_music_tracks(
        (
            Path("radio/song.wav"),
            Path("Fallout New Vegas/MainTitle.wav"),
            Path("music/mus_maintheme.wav"),
            Path("special/intro.wav"),
            Path("Fallout 3/MainTitle.wav"),
        )
    )

    assert tracks == (
        Path("music/mus_maintheme.wav"),
        Path("Fallout New Vegas/MainTitle.wav"),
        Path("Fallout 3/MainTitle.wav"),
        Path("radio/song.wav"),
        Path("special/intro.wav"),
    )


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


def test_player_repeats_playlist_and_stops():
    played: list[str] = []
    repeated = threading.Event()

    def play_track(track: Path, player: ConversionMusicPlayer) -> None:
        played.append(track.name)
        if len(played) == 3:
            repeated.set()
            player.stop()

    player = ConversionMusicPlayer(track_player=play_track)
    player.start((Path("first.wav"), Path("second.wav")))

    assert repeated.wait(timeout=1.0)
    player.stop()

    assert played == ["first.wav", "second.wav", "first.wav"]
    assert player.playing is False


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


def test_player_supports_next_previous_pause_volume_and_seek():
    played: list[str] = []
    first_started = threading.Event()
    second_started = threading.Event()
    previous_started = threading.Event()
    seek_applied = threading.Event()

    def play_track(track: Path, player: ConversionMusicPlayer) -> None:
        played.append(track.name)
        player._set_track_timing(120.0)
        if len(played) == 1:
            first_started.set()
        elif len(played) == 2:
            second_started.set()
        elif len(played) == 3:
            previous_started.set()
        while (
            not player._stop_event.is_set() and not player._track_change_event.is_set()
        ):
            progress = player._take_pending_seek()
            if progress is not None:
                player._set_position(progress * 120.0)
                seek_applied.set()
            player._wake_event.wait(timeout=0.01)
            player._wake_event.clear()

    player = ConversionMusicPlayer(track_player=play_track)
    player.start((Path("first.wav"), Path("second.wav")))
    assert first_started.wait(timeout=1.0)

    player.next()
    assert second_started.wait(timeout=1.0)
    player.previous()
    assert previous_started.wait(timeout=1.0)

    player.pause()
    player.set_volume(0.42)
    player.seek(0.5)
    assert seek_applied.wait(timeout=1.0)
    state = player.snapshot()

    assert played[:3] == ["first.wav", "second.wav", "first.wav"]
    assert state.paused is True
    assert state.volume == 0.42
    assert state.position_seconds == 60.0
    assert state.progress == 0.5

    player.resume()
    assert player.snapshot().paused is False
    player.stop()
    assert player.active is False
