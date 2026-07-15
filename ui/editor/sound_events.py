from __future__ import annotations

import logging
import re
import hashlib
import sqlite3
import tempfile
from dataclasses import dataclass
from pathlib import Path

_log = logging.getLogger("nif_editor.sound_events")

_SOUND_TEXT_RE = re.compile(r"^\s*Sound\s*:\s*(?P<cue>.+?)\s*$", re.IGNORECASE)
_SOUND_LINE_RE = re.compile(r"^\s*-\s*Sound\s*:\s*(?P<path>.+?)\s*$", re.MULTILINE)
_ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"

_ACTIVE_AUDIO = None
_MAX_SOUND_RESOLUTION_CACHE = 256
_MAX_SOUND_WAV_PREVIEW_CACHE = 256
_SOUND_RESOLUTION_CACHE: dict[tuple, ResolvedSound] = {}
_SOUND_WAV_PREVIEW_CACHE: dict[tuple, Path] = {}


@dataclass(frozen=True)
class ResolvedSound:
    cue: str
    path: Path | None = None
    error: str = ""


def parse_sound_text_key(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    match = _SOUND_TEXT_RE.match(value)
    if not match:
        return None
    cue = match.group("cue").strip()
    return cue or None


def format_sound_text_key(cue: str) -> str:
    return f"Sound: {cue.strip()}"


def clear_sound_resolution_cache() -> None:
    _SOUND_RESOLUTION_CACHE.clear()
    _SOUND_WAV_PREVIEW_CACHE.clear()


def resolve_sound_cue(
    cue: str, app=None, *, game_id: str | None = None
) -> ResolvedSound:
    cue = cue.strip()
    if not cue:
        return ResolvedSound(cue=cue, error="Empty sound cue")

    resolved_game_id = _resolve_game_id(app, game_id)
    cache_key = _sound_resolution_cache_key(cue, app, resolved_game_id)
    cached = _get_cached_sound_resolution(cache_key)
    if cached is not None:
        return cached

    texture_dirs, user_archive_dirs, base_archive_dirs = _resolve_texture_style_paths(
        app, resolved_game_id
    )
    loose_dirs = texture_dirs or _resolve_audio_dirs(app, resolved_game_id)
    ba2_mgr = _resolve_archive_manager(
        app, resolved_game_id, user_archive_dirs, base_archive_dirs
    )

    sound_paths = _lookup_sndr_sound_paths(cue, game_id=resolved_game_id)
    candidates = sound_paths or [cue]

    for sound_path in candidates:
        resolved = _resolve_audio_path_in_dirs(loose_dirs, sound_path)
        if resolved is not None:
            return _cache_sound_resolution(
                cache_key, ResolvedSound(cue=cue, path=resolved)
            )

    if ba2_mgr is not None:
        for sound_path in candidates:
            resolved = _resolve_archive_audio_path(ba2_mgr, sound_path)
            if resolved is not None:
                return _cache_sound_resolution(
                    cache_key, ResolvedSound(cue=cue, path=resolved)
                )

    if not sound_paths:
        return _cache_sound_resolution(
            cache_key, ResolvedSound(cue=cue, error=f"No SNDR Sound paths found for {cue}")
        )
    return _cache_sound_resolution(
        cache_key, ResolvedSound(cue=cue, error=f"Sound asset not found for {cue}")
    )


def play_sound_cue(cue: str, app=None, *, game_id: str | None = None) -> ResolvedSound:
    resolved = resolve_sound_cue(cue, app, game_id=game_id)
    if resolved.path is None:
        return resolved

    try:
        wav_path = _to_wav_for_preview(resolved.path)
        import sounddevice as sd
        import soundfile as sf

        data, sample_rate = sf.read(str(wav_path), dtype="float32")
        global _ACTIVE_AUDIO
        _ACTIVE_AUDIO = data
        sd.play(_ACTIVE_AUDIO, sample_rate)
    except Exception as exc:
        _log.warning("Failed to play sound cue %s: %s", cue, exc)
        return ResolvedSound(cue=cue, path=resolved.path, error=str(exc))
    return resolved


def _lookup_sndr_sound_paths(editor_id: str, *, game_id: str) -> list[str]:
    db_path = _records_db_path(game_id)
    if db_path is None:
        return []

    conn = None
    try:
        conn = sqlite3.connect(str(db_path))
        try:
            row = conn.execute(
                "select yaml_path, content from records where lower(editor_id) = lower(?) and record_type = 'SNDR'",
                (editor_id,),
            ).fetchone()
        except sqlite3.OperationalError as exc:
            if "no such column: yaml_path" not in str(exc).lower():
                raise
            legacy_row = conn.execute(
                "select content from records where lower(editor_id) = lower(?) and record_type = 'SNDR'",
                (editor_id,),
            ).fetchone()
            row = None if legacy_row is None else (None, legacy_row[0])
    except sqlite3.Error as exc:
        _log.debug("SNDR lookup failed for %s: %s", editor_id, exc)
        return []
    finally:
        if conn is not None:
            try:
                conn.close()
            except Exception:
                pass

    if row is None:
        return []
    content = _read_record_yaml_content(row[0], row[1])
    if not content:
        return []
    result = []
    for match in _SOUND_LINE_RE.finditer(content):
        value = match.group("path").strip().strip('"').strip("'")
        if value:
            result.append(value)
    return result


def _read_record_yaml_content(yaml_path: object, content: object) -> str:
    path_text = str(yaml_path or "").strip()
    if path_text:
        path = Path(path_text)
        if path.is_file():
            try:
                return path.read_text(encoding="utf-8", errors="replace")
            except OSError as exc:
                _log.debug("SNDR YAML read failed for %s: %s", path, exc)

    if content is None:
        return ""
    if isinstance(content, str):
        return content
    if isinstance(content, (bytes, bytearray, memoryview)):
        data = bytes(content)
        if not data:
            return ""
        if data[:4] == _ZSTD_MAGIC:
            try:
                import zstandard as zstd

                return (
                    zstd.ZstdDecompressor()
                    .decompressobj()
                    .decompress(data)
                    .decode("utf-8", errors="replace")
                )
            except Exception as exc:
                _log.debug("SNDR YAML decompression failed: %s", exc)
                return ""
        return data.decode("utf-8", errors="replace")
    return str(content)


def _records_db_path(game_id: str) -> Path | None:
    try:
        from app.paths import get_db_dir

        path = get_db_dir() / f"{game_id}_records.db"
        if path.is_file():
            return path
    except Exception:
        pass

    fallback = Path(__file__).resolve().parents[2] / "data" / f"{game_id}_records.db"
    return fallback if fallback.is_file() else None


def _sound_resolution_cache_key(cue: str, app, game_id: str) -> tuple:
    return (
        cue.strip().lower(),
        game_id,
        _normalize_cache_path(_resolve_nif_path(app)),
        _settings_cache_key(app, game_id),
        _archive_cache_key(getattr(app, "ba2_manager", None)),
        _records_db_cache_key(game_id),
    )


def _get_cached_sound_resolution(cache_key: tuple) -> ResolvedSound | None:
    cached = _SOUND_RESOLUTION_CACHE.get(cache_key)
    if cached is None:
        return None
    if cached.path is not None and not cached.path.is_file():
        _SOUND_RESOLUTION_CACHE.pop(cache_key, None)
        return None
    _SOUND_RESOLUTION_CACHE.pop(cache_key, None)
    _SOUND_RESOLUTION_CACHE[cache_key] = cached
    return cached


def _cache_sound_resolution(cache_key: tuple, resolved: ResolvedSound) -> ResolvedSound:
    if cache_key in _SOUND_RESOLUTION_CACHE:
        _SOUND_RESOLUTION_CACHE.pop(cache_key, None)
    _SOUND_RESOLUTION_CACHE[cache_key] = resolved
    while len(_SOUND_RESOLUTION_CACHE) > _MAX_SOUND_RESOLUTION_CACHE:
        oldest = next(iter(_SOUND_RESOLUTION_CACHE))
        _SOUND_RESOLUTION_CACHE.pop(oldest, None)
    return resolved


def _settings_cache_key(app, game_id: str) -> tuple:
    settings = getattr(app, "_toolkit_settings", None)
    if settings is None:
        return ()
    try:
        paths = settings.get_game_paths(game_id)
    except Exception:
        return ()
    additional = tuple(
        _normalize_cache_path(path)
        for path in paths.get("additional_paths", []) or []
        if path
    )
    return (
        _normalize_cache_path(paths.get("root_dir")),
        _normalize_cache_path(paths.get("extracted_dir") or paths.get("extracted")),
        additional,
    )


def _archive_cache_key(ba2_mgr) -> tuple | None:
    if ba2_mgr is None:
        return None
    try:
        archive_count = ba2_mgr.archive_count
    except Exception:
        archive_count = None
    return (id(ba2_mgr), archive_count)


def _records_db_cache_key(game_id: str) -> tuple | None:
    db_path = _records_db_path(game_id)
    if db_path is None:
        return None
    try:
        stat = db_path.stat()
    except OSError:
        return (_normalize_cache_path(db_path), None, None)
    return (_normalize_cache_path(db_path), stat.st_mtime_ns, stat.st_size)


def _normalize_cache_path(path) -> str:
    if not path:
        return ""
    try:
        return str(Path(path).resolve(strict=False)).lower()
    except Exception:
        return str(path).replace("\\", "/").lower()


def _resolve_game_id(app, game_id: str | None) -> str:
    if game_id:
        return game_id
    profile = _resolve_game_profile(app, None)
    profile_id = getattr(profile, "id", None)
    if isinstance(profile_id, str) and profile_id:
        return profile_id
    settings = getattr(app, "_toolkit_settings", None)
    if settings is not None:
        try:
            active_game = settings.get_active_game()
            if active_game:
                return str(active_game)
        except Exception:
            pass
    return "fo4"


def _resolve_game_profile(app, game_id: str | None):
    try:
        session = getattr(getattr(app, "registry", None), "active_session", None)
        profile = getattr(session, "game_profile", None)
        if isinstance(getattr(profile, "id", None), str):
            return profile
    except Exception:
        pass
    if not game_id:
        return None
    try:
        from creation_lib.core.game_profiles import get_profile

        return get_profile(game_id)
    except Exception:
        return None


def _resolve_texture_style_paths(
    app, game_id: str
) -> tuple[list[Path], list[Path], list[Path]]:
    build_texture_dirs = getattr(app, "_build_texture_dirs", None)
    if build_texture_dirs is None:
        return [], [], []

    nif_path = _resolve_nif_path(app)
    profile = _resolve_game_profile(app, game_id)
    try:
        result = build_texture_dirs(nif_path, game_profile=profile)
    except TypeError:
        try:
            result = build_texture_dirs(nif_path)
        except Exception as exc:
            _log.debug("Sound path build failed: %s", exc)
            return [], [], []
    except Exception as exc:
        _log.debug("Sound path build failed: %s", exc)
        return [], [], []

    try:
        texture_dirs, user_archive_dirs, base_archive_dirs = result
    except Exception:
        return [], [], []
    return (
        _unique_existing_paths(texture_dirs),
        _unique_existing_paths(user_archive_dirs),
        _unique_existing_paths(base_archive_dirs),
    )


def _resolve_audio_dirs(app, game_id: str) -> list[Path]:
    dirs: list[Path] = []
    settings = getattr(app, "_toolkit_settings", None)
    if settings is not None:
        try:
            game_paths = settings.get_game_paths(game_id)
            for path in game_paths.get("additional_paths", []):
                _add_with_data(dirs, Path(path))
        except Exception:
            pass

    nif_path = _resolve_nif_path(app)
    if nif_path:
        _add_nif_relative_dirs(dirs, Path(nif_path))

    extracted_dir = _resolve_extracted_dir(app, game_id)
    if extracted_dir is not None:
        _add_with_data(dirs, extracted_dir)
    return _unique_existing_paths(dirs)


def _resolve_archive_manager(
    app,
    game_id: str,
    user_archive_dirs: list[Path] | None = None,
    base_archive_dirs: list[Path] | None = None,
):
    ba2_mgr = getattr(app, "ba2_manager", None)
    if ba2_mgr is not None:
        return ba2_mgr

    user_dirs = list(user_archive_dirs or [])
    base_dirs = list(base_archive_dirs or [])
    if not user_dirs and not base_dirs:
        user_dirs, base_dirs = _resolve_archive_dirs_from_settings(app, game_id)
    if not user_dirs and not base_dirs:
        return None

    try:
        from creation_lib.textures.texture_dirs import create_ba2_manager

        return create_ba2_manager(user_dirs, base_dirs)
    except Exception as exc:
        _log.debug("Sound archive manager unavailable: %s", exc)
        return None


def _resolve_archive_dirs_from_settings(app, game_id: str) -> tuple[list[Path], list[Path]]:
    user_archive_dirs: list[Path] = []
    base_archive_dirs: list[Path] = []
    settings = getattr(app, "_toolkit_settings", None)
    if settings is None:
        return user_archive_dirs, base_archive_dirs
    try:
        game_paths = settings.get_game_paths(game_id)
        for path in game_paths.get("additional_paths", []):
            _add_with_data(user_archive_dirs, Path(path))
        root = str(game_paths.get("root_dir", "") or "").strip()
        if root:
            _add_with_data(base_archive_dirs, Path(root))
    except Exception:
        pass
    return (
        _unique_existing_paths(user_archive_dirs),
        _unique_existing_paths(base_archive_dirs),
    )


def _resolve_extracted_dir(app, game_id: str) -> Path | None:
    settings = getattr(app, "_toolkit_settings", None)
    if settings is not None:
        try:
            paths = settings.get_game_paths(game_id)
            for key in ("extracted_dir", "extracted"):
                extracted = str(paths.get(key, "") or "").strip()
                if extracted:
                    path = Path(extracted)
                    if path.is_dir():
                        return path
        except Exception:
            pass

    fallback = Path(__file__).resolve().parents[2] / "extracted" / game_id
    return fallback if fallback.is_dir() else None


def _resolve_nif_path(app) -> str | None:
    for attr_name in ("nif_path", "current_path"):
        try:
            value = getattr(app, attr_name, None)
            if callable(value):
                value = value()
            if isinstance(value, (str, Path)) and str(value):
                return str(value)
        except Exception:
            pass
    try:
        session = getattr(getattr(app, "registry", None), "active_session", None)
        value = getattr(session, "file_path", None)
        if isinstance(value, (str, Path)) and str(value):
            return str(value)
    except Exception:
        pass
    return None


def _add_with_data(target: list[Path], path: Path) -> None:
    _append_unique(target, path)
    data_sub = path / "Data"
    if data_sub.is_dir():
        _append_unique(target, data_sub)


def _add_nif_relative_dirs(target: list[Path], nif_path: Path) -> None:
    nif_dir = nif_path.parent
    _append_unique(target, nif_dir)
    current = nif_dir
    for _ in range(8):
        current = current.parent
        if current == current.parent:
            break
        if current.name.lower() == "data" and (current / "Meshes").is_dir():
            _append_unique(target, current)
            break


def _append_unique(target: list[Path], path: Path) -> None:
    if path not in target:
        target.append(path)


def _unique_existing_paths(paths) -> list[Path]:
    result: list[Path] = []
    for path in paths or []:
        try:
            p = Path(path)
        except TypeError:
            continue
        if p.is_dir() and p not in result:
            result.append(p)
    return result


def _resolve_audio_path_in_dirs(dirs: list[Path], sound_path: str) -> Path | None:
    for base_dir in dirs:
        resolved = _resolve_audio_path(base_dir, sound_path)
        if resolved is not None:
            return resolved
    return None


def _resolve_audio_path(extracted_dir: Path, sound_path: str) -> Path | None:
    for rel in _audio_path_candidates(sound_path):
        resolved = _case_insensitive_resolve(extracted_dir, rel)
        if resolved is not None:
            return resolved
    return None


def _resolve_archive_audio_path(ba2_mgr, sound_path: str) -> Path | None:
    for rel in _audio_path_candidates(sound_path):
        try:
            data = ba2_mgr.find(rel)
        except Exception as exc:
            _log.debug("Archive sound lookup failed for %s: %s", rel, exc)
            continue
        if data is not None:
            return _write_archive_audio_preview(rel, data)
    return None


def _audio_path_candidates(sound_path: str) -> list[str]:
    rel = sound_path.replace("\\", "/").strip().strip('"').strip("'")
    if not rel:
        return []
    for prefix in ("data/", "./"):
        if rel.lower().startswith(prefix):
            rel = rel[len(prefix):]
    roots = [rel]
    if not rel.lower().startswith("sound/"):
        roots.append(f"Sound/{rel}")

    candidates: list[str] = []
    seen: set[str] = set()
    for root in roots:
        path = Path(root)
        suffix = path.suffix.lower()
        if suffix == ".wav":
            suffixes = [".xwm", ".wav", ".fuz"]
        elif suffix:
            suffixes = [path.suffix, ".xwm", ".wav", ".fuz"]
        else:
            suffixes = ["", ".xwm", ".wav", ".fuz"]
        for item_suffix in suffixes:
            try:
                candidate = str(path.with_suffix(item_suffix)).replace("\\", "/")
            except ValueError:
                continue
            key = candidate.lower()
            if candidate and key not in seen:
                seen.add(key)
                candidates.append(candidate)
    return candidates


def _case_insensitive_resolve(base: Path, rel_path: str) -> Path | None:
    candidate = base / rel_path
    if candidate.is_file():
        return candidate

    current = base
    for segment in rel_path.split("/"):
        if not segment:
            continue
        if not current.is_dir():
            return None
        found = None
        segment_lower = segment.lower()
        try:
            for child in current.iterdir():
                if child.name.lower() == segment_lower:
                    found = child
                    break
        except PermissionError:
            return None
        if found is None:
            return None
        current = found
    return current if current.is_file() else None


def _write_archive_audio_preview(sound_path: str, data: bytes) -> Path:
    out_dir = _sound_preview_dir()
    suffix = Path(sound_path).suffix or ".xwm"
    digest = hashlib.sha1(sound_path.lower().encode("utf-8")).hexdigest()[:16]
    out_path = out_dir / f"archive-{digest}{suffix}"
    out_path.write_bytes(data)
    return out_path


def _sound_preview_dir() -> Path:
    out_dir = Path(tempfile.gettempdir()) / "modkit21_nif_sound_preview"
    out_dir.mkdir(parents=True, exist_ok=True)
    return out_dir


def _to_wav_for_preview(path: Path) -> Path:
    path = Path(path)
    if path.suffix.lower() == ".wav":
        return path

    cache_key = _sound_wav_preview_cache_key(path)
    cached = _get_cached_wav_preview(cache_key)
    if cached is not None:
        return cached

    from ui.voice_changer.format_converter import to_wav

    wav_path = Path(to_wav(str(path), str(_sound_wav_preview_dir(path))))
    return _cache_wav_preview(cache_key, wav_path)


def _sound_wav_preview_cache_key(path: Path) -> tuple:
    normalized = _normalize_cache_path(path)
    try:
        stat = path.stat()
    except OSError:
        return (normalized, None, None)
    return (normalized, stat.st_mtime_ns, stat.st_size)


def _get_cached_wav_preview(cache_key: tuple) -> Path | None:
    cached = _SOUND_WAV_PREVIEW_CACHE.get(cache_key)
    if cached is None:
        return None
    if not cached.is_file():
        _SOUND_WAV_PREVIEW_CACHE.pop(cache_key, None)
        return None
    _SOUND_WAV_PREVIEW_CACHE.pop(cache_key, None)
    _SOUND_WAV_PREVIEW_CACHE[cache_key] = cached
    return cached


def _cache_wav_preview(cache_key: tuple, wav_path: Path) -> Path:
    if cache_key in _SOUND_WAV_PREVIEW_CACHE:
        _SOUND_WAV_PREVIEW_CACHE.pop(cache_key, None)
    _SOUND_WAV_PREVIEW_CACHE[cache_key] = wav_path
    while len(_SOUND_WAV_PREVIEW_CACHE) > _MAX_SOUND_WAV_PREVIEW_CACHE:
        oldest = next(iter(_SOUND_WAV_PREVIEW_CACHE))
        _SOUND_WAV_PREVIEW_CACHE.pop(oldest, None)
    return wav_path


def _sound_wav_preview_dir(path: Path) -> Path:
    digest = hashlib.sha1(_normalize_cache_path(path).encode("utf-8")).hexdigest()[:16]
    out_dir = _sound_preview_dir() / "wav" / digest
    out_dir.mkdir(parents=True, exist_ok=True)
    return out_dir
