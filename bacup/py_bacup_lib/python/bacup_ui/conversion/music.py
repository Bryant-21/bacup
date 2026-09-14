from __future__ import annotations

import logging
import os
import subprocess
import threading
import wave
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path

_log = logging.getLogger("toolkit.conversion_music")
_FIRST_THEME_STEM = "mus_maintheme"
_FRAMES_PER_BLOCK = 4096
_BACKGROUND_VOLUME = 0.12
_THEME_EXTENSIONS = {".wav", ".xwm"}
_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0


def _theme_sort_key(path: Path) -> tuple[int, str]:
    stem = path.stem.casefold()
    priority = 0 if stem == _FIRST_THEME_STEM else 1 if stem == "maintitle" else 2
    return priority, path.name.casefold()


def prioritize_music_tracks(tracks: Iterable[Path]) -> tuple[Path, ...]:
    unique = tuple(dict.fromkeys(tracks))
    main_theme = [
        track for track in unique if track.stem.casefold() == _FIRST_THEME_STEM
    ]
    main_title = [track for track in unique if track.stem.casefold() == "maintitle"]
    priority_tracks = set(main_theme) | set(main_title)
    return tuple(
        (
            *main_theme,
            *main_title,
            *(track for track in unique if track not in priority_tracks),
        )
    )


def ordered_theme_tracks(theme_dir: Path) -> tuple[Path, ...]:
    if not theme_dir.is_dir():
        return ()
    return tuple(
        sorted(
            (
                path
                for path in theme_dir.iterdir()
                if path.is_file() and path.suffix.casefold() == ".wav"
            ),
            key=_theme_sort_key,
        )
    )


def _ordered_theme_sources(theme_dir: Path) -> tuple[Path, ...]:
    if not theme_dir.is_dir():
        return ()
    by_stem: dict[str, Path] = {}
    for path in sorted(theme_dir.iterdir(), key=lambda item: item.name.casefold()):
        if not path.is_file() or path.suffix.casefold() not in _THEME_EXTENSIONS:
            continue
        stem = path.stem.casefold()
        existing = by_stem.get(stem)
        if existing is None or path.suffix.casefold() == ".wav":
            by_stem[stem] = path
    return tuple(sorted(by_stem.values(), key=_theme_sort_key))


def discover_theme_tracks(theme_dirs: Iterable[Path]) -> tuple[Path, ...]:
    for theme_dir in theme_dirs:
        tracks = ordered_theme_tracks(theme_dir)
        if tracks:
            return tracks
    return ()


def _decode_xwm(source: Path, destination: Path) -> Path:
    from creation_lib.paths import get_resource_dir

    tool = get_resource_dir() / "xWMAEncode.exe"
    if not tool.is_file():
        raise FileNotFoundError(f"xWMAEncode.exe not found: {tool}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(
        f".{destination.stem}.{threading.get_ident()}.tmp.wav"
    )
    try:
        result = subprocess.run(
            [str(tool), str(source), str(temporary)],
            capture_output=True,
            check=False,
            creationflags=_CREATE_NO_WINDOW,
        )
        if result.returncode != 0 or not temporary.is_file():
            detail = (
                result.stderr.decode(errors="replace").strip() or "XWM decode failed"
            )
            raise RuntimeError(detail)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    return destination


def prepare_theme_tracks(
    theme_dirs: Iterable[Path],
    cache_dir: Path,
    *,
    decoder: Callable[[Path, Path], Path] = _decode_xwm,
) -> tuple[Path, ...]:
    candidates = tuple(dict.fromkeys(theme_dirs))
    ready = discover_theme_tracks(candidates)
    if ready:
        return ready

    for theme_dir in candidates:
        sources = _ordered_theme_sources(theme_dir)
        if not sources:
            continue
        prepared: list[Path] = []
        for source in sources:
            if source.suffix.casefold() == ".wav":
                prepared.append(source)
                continue
            destination = cache_dir / f"{source.stem}.wav"
            try:
                current = (
                    destination.is_file()
                    and destination.stat().st_size > 44
                    and destination.stat().st_mtime >= source.stat().st_mtime
                )
                prepared.append(
                    destination if current else decoder(source, destination)
                )
            except Exception as exc:
                _log.warning("Unable to decode conversion theme %s: %s", source, exc)
        return tuple(prepared)
    return ()


def format_music_time(seconds: float) -> str:
    minutes, secs = divmod(max(0, int(seconds)), 60)
    return f"{minutes:02d}:{secs:02d}"


@dataclass(frozen=True)
class MusicPlayerState:
    active: bool
    paused: bool
    current_track: Path | None
    track_index: int
    track_count: int
    position_seconds: float
    duration_seconds: float
    volume: float

    @property
    def progress(self) -> float:
        if self.duration_seconds <= 0.0:
            return 0.0
        return max(0.0, min(self.position_seconds / self.duration_seconds, 1.0))


def _play_pcm_wav(track: Path, player: ConversionMusicPlayer) -> None:
    import numpy as np
    import sounddevice as sd

    with wave.open(str(track), "rb") as source:
        if source.getsampwidth() != 2 or source.getcomptype() != "NONE":
            raise ValueError(f"unsupported WAV encoding: {track}")
        sample_rate = source.getframerate()
        total_frames = source.getnframes()
        player._set_track_timing(total_frames / sample_rate)
        with sd.RawOutputStream(
            samplerate=sample_rate,
            channels=source.getnchannels(),
            dtype="float32",
        ) as stream:
            stream_running = True
            while not player._stop_event.is_set():
                if player._track_change_event.is_set():
                    return
                seek_fraction = player._take_pending_seek()
                if seek_fraction is not None:
                    frame = min(int(seek_fraction * total_frames), total_frames)
                    source.setpos(frame)
                    player._set_position(frame / sample_rate)
                if player.paused:
                    if stream_running:
                        stream.stop()
                        stream_running = False
                    player._wake_event.wait(timeout=0.05)
                    player._wake_event.clear()
                    continue
                if not stream_running:
                    stream.start()
                    stream_running = True
                pcm = source.readframes(_FRAMES_PER_BLOCK)
                if not pcm:
                    player._set_position(total_frames / sample_rate)
                    return
                samples = np.frombuffer(pcm, dtype="<i2").astype(np.float32)
                samples *= player.volume / 32768.0
                stream.write(samples.tobytes())
                player._set_position(source.tell() / sample_rate)


class ConversionMusicPlayer:
    def __init__(
        self,
        *,
        volume: float = _BACKGROUND_VOLUME,
        track_player: Callable[[Path, ConversionMusicPlayer], None] = _play_pcm_wav,
    ) -> None:
        self._volume = max(0.0, min(float(volume), 1.0))
        self._track_player = track_player
        self._stop_event = threading.Event()
        self._track_change_event = threading.Event()
        self._wake_event = threading.Event()
        self._lock = threading.Lock()
        self._thread: threading.Thread | None = None
        self._playlist: tuple[Path, ...] = ()
        self._current_track: Path | None = None
        self._track_index = 0
        self._position_seconds = 0.0
        self._duration_seconds = 0.0
        self._paused = False
        self._pending_track_delta = 0
        self._pending_seek_fraction: float | None = None

    @property
    def active(self) -> bool:
        with self._lock:
            return self._thread is not None and self._thread.is_alive()

    @property
    def playing(self) -> bool:
        with self._lock:
            return (
                self._thread is not None
                and self._thread.is_alive()
                and not self._paused
            )

    @property
    def paused(self) -> bool:
        with self._lock:
            return self._paused

    @property
    def current_track(self) -> Path | None:
        with self._lock:
            return self._current_track

    @property
    def volume(self) -> float:
        with self._lock:
            return self._volume

    def snapshot(self) -> MusicPlayerState:
        with self._lock:
            thread = self._thread
            return MusicPlayerState(
                active=thread is not None and thread.is_alive(),
                paused=self._paused,
                current_track=self._current_track,
                track_index=self._track_index,
                track_count=len(self._playlist),
                position_seconds=self._position_seconds,
                duration_seconds=self._duration_seconds,
                volume=self._volume,
            )

    def set_volume(self, volume: float) -> None:
        with self._lock:
            self._volume = max(0.0, min(float(volume), 1.0))

    def start(self, tracks: Iterable[Path]) -> None:
        playlist = tuple(dict.fromkeys(tracks))
        if not playlist:
            return
        with self._lock:
            if self._thread is not None and self._thread.is_alive():
                return
            self._playlist = playlist
            self._track_index = 0
            self._current_track = None
            self._position_seconds = 0.0
            self._duration_seconds = 0.0
            self._paused = False
            self._pending_track_delta = 0
            self._pending_seek_fraction = None
            self._stop_event.clear()
            self._track_change_event.clear()
            self._wake_event.clear()
            self._thread = threading.Thread(
                target=self._run,
                name="bacup-conversion-music",
                daemon=True,
            )
            self._thread.start()

    def stop(self) -> None:
        self._stop_event.set()
        self._wake_event.set()
        with self._lock:
            thread = self._thread
        if thread is not None and thread is not threading.current_thread():
            thread.join(timeout=1.0)
        with self._lock:
            if self._thread is thread and (thread is None or not thread.is_alive()):
                self._reset_stopped_state()

    def pause(self) -> None:
        with self._lock:
            if self._thread is not None and self._thread.is_alive():
                self._paused = True
        self._wake_event.set()

    def resume(self) -> None:
        with self._lock:
            if self._thread is not None and self._thread.is_alive():
                self._paused = False
        self._wake_event.set()

    def previous(self) -> None:
        self._change_track(-1)

    def next(self) -> None:
        self._change_track(1)

    def seek(self, progress: float) -> None:
        with self._lock:
            if self._thread is None or not self._thread.is_alive():
                return
            self._pending_seek_fraction = max(0.0, min(float(progress), 1.0))
        self._wake_event.set()

    def _change_track(self, delta: int) -> None:
        with self._lock:
            if self._thread is None or not self._thread.is_alive():
                return
            self._pending_track_delta = -1 if delta < 0 else 1
            self._pending_seek_fraction = None
            self._track_change_event.set()
        self._wake_event.set()

    def _take_pending_seek(self) -> float | None:
        with self._lock:
            progress = self._pending_seek_fraction
            self._pending_seek_fraction = None
            return progress

    def _set_track_timing(self, duration_seconds: float) -> None:
        with self._lock:
            self._duration_seconds = max(0.0, float(duration_seconds))
            self._position_seconds = 0.0

    def _set_position(self, position_seconds: float) -> None:
        with self._lock:
            self._position_seconds = max(
                0.0,
                min(float(position_seconds), self._duration_seconds),
            )

    def _reset_stopped_state(self) -> None:
        self._thread = None
        self._playlist = ()
        self._current_track = None
        self._track_index = 0
        self._position_seconds = 0.0
        self._duration_seconds = 0.0
        self._paused = False
        self._pending_track_delta = 0
        self._pending_seek_fraction = None

    def _run(self) -> None:
        consecutive_failures = 0
        try:
            while not self._stop_event.is_set():
                with self._lock:
                    playlist = self._playlist
                    if not playlist:
                        return
                    track = playlist[self._track_index]
                    self._current_track = track
                    self._position_seconds = 0.0
                    self._duration_seconds = 0.0
                    self._pending_seek_fraction = None
                try:
                    self._track_player(track, self)
                except Exception as exc:
                    consecutive_failures += 1
                    _log.warning("Unable to play conversion music %s: %s", track, exc)
                else:
                    consecutive_failures = 0
                if self._stop_event.is_set():
                    return
                with self._lock:
                    delta = self._pending_track_delta
                    self._pending_track_delta = 0
                    self._track_change_event.clear()
                    self._track_index = (self._track_index + (delta or 1)) % len(
                        self._playlist
                    )
                if consecutive_failures >= len(playlist):
                    return
        finally:
            with self._lock:
                if self._thread is threading.current_thread():
                    self._reset_stopped_state()
