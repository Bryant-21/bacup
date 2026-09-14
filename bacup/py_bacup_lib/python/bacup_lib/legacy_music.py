from __future__ import annotations

import os
import subprocess
import threading
from collections.abc import Callable, Iterable
from dataclasses import replace
from pathlib import Path

from bacup_lib.models import AssetProvenance, AssetRef

_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0

_LEGACY_AUDIO_EXTENSIONS = {".mp3", ".ogg", ".wav", ".xwm"}
_LEGACY_TRANSCODE_EXTENSIONS = {".mp3", ".ogg", ".wav"}


def discover_legacy_music_tracks(
    *,
    source_game: str,
    target_game: str,
    output_plugin_name: str,
    primary_roots: Iterable[Path],
    additional_roots: Iterable[Path],
) -> list[dict[str, str]]:
    if source_game.lower() not in {"fnv", "fo3"} or target_game.lower() != "fo4":
        return []

    plugin_stem = Path(output_plugin_name).stem or "Converted"
    groups = [
        (source_game.upper(), tuple(Path(root) for root in primary_roots)),
        (
            "FO3" if source_game.lower() == "fnv" else "GRAFTED",
            tuple(Path(root) for root in additional_roots),
        ),
    ]
    rows: list[dict[str, str]] = []
    seen_music_roots: set[str] = set()
    seen_targets: set[str] = set()

    for tag, roots in groups:
        for root in roots:
            for music_root, target_prefix in _find_music_roots(root):
                root_key = os.path.normcase(str(music_root.resolve()))
                if root_key in seen_music_roots:
                    continue
                seen_music_roots.add(root_key)
                for source in sorted(music_root.rglob("*"), key=lambda path: path.as_posix().casefold()):
                    if not source.is_file() or source.suffix.casefold() not in _LEGACY_AUDIO_EXTENSIONS:
                        continue
                    relative = source.relative_to(music_root)
                    target_suffix = (
                        ".xwm"
                        if source.suffix.casefold() in _LEGACY_TRANSCODE_EXTENSIONS
                        else source.suffix
                    )
                    target_relative = (target_prefix / relative).with_suffix(target_suffix)
                    asset_path = Path("Music") / plugin_stem / tag / target_relative
                    target_key = asset_path.as_posix().casefold()
                    if target_key in seen_targets:
                        continue
                    seen_targets.add(target_key)
                    rows.append(
                        {
                            "legacy_relative_path": relative.as_posix(),
                            "source_path": str(source),
                            "asset_path": asset_path.as_posix(),
                            "track_path": _music_track_record_path(asset_path),
                        }
                    )
    return rows


def augment_legacy_music_assets(
    assets: list[AssetRef],
    tracks: list[dict[str, str]],
) -> list[AssetRef]:
    if not tracks:
        return assets

    legacy_refs = [
        asset.source_path
        for asset in assets
        if asset.asset_type in {"sound", "audio"}
        and asset.provenance is not None
        and asset.provenance.added_by_record_sig == "MUSC"
        and asset.provenance.added_by_field == "FNAM"
    ]
    expanded = [
        asset
        for asset in assets
        if not (
            asset.asset_type in {"sound", "audio"}
            and asset.provenance is not None
            and asset.provenance.added_by_record_sig == "MUSC"
            and asset.provenance.added_by_field == "FNAM"
        )
    ]
    existing = {
        (asset.asset_type.casefold(), asset.source_path.replace("\\", "/").casefold())
        for asset in expanded
    }
    for track in tracks:
        if not any(
            _legacy_ref_matches_track(legacy_ref, track["legacy_relative_path"])
            for legacy_ref in legacy_refs
        ):
            continue
        key = ("sound", track["asset_path"].casefold())
        if key in existing:
            continue
        existing.add(key)
        expanded.append(
            AssetRef(
                asset_type="sound",
                source_path=track["asset_path"],
                resolved_path=track["source_path"],
                provenance=AssetProvenance(
                    added_by_record_fk="",
                    added_by_record_eid="",
                    added_by_field="FNAM",
                    walk_depth=0,
                    walker_pass="legacy_music_tree",
                    added_by_record_sig="MUSC",
                ),
            )
        )
    return expanded


def prepare_legacy_music_assets(
    assets: list[AssetRef],
    cache_root: Path,
    *,
    converter: Callable[[Path, Path], None] | None = None,
) -> list[AssetRef]:
    convert = converter or _convert_legacy_audio_to_xwm
    prepared: list[AssetRef] = []
    for asset in assets:
        if asset.asset_type not in {"sound", "audio"} or not asset.resolved_path:
            prepared.append(asset)
            continue
        source = Path(asset.resolved_path)
        if source.suffix.casefold() not in _LEGACY_TRANSCODE_EXTENSIONS:
            prepared.append(asset)
            continue
        relative = Path(asset.source_path.replace("\\", "/")).with_suffix(".xwm")
        destination = cache_root / relative
        current = (
            destination.is_file()
            and destination.stat().st_size > 0
            and destination.stat().st_mtime >= source.stat().st_mtime
        )
        if not current:
            destination.parent.mkdir(parents=True, exist_ok=True)
            convert(source, destination)
        prepared.append(
            replace(
                asset,
                source_path=relative.as_posix(),
                resolved_path=str(destination),
                resolution_error=None,
            )
        )
    return prepared


def _music_track_record_path(asset_path: Path) -> str:
    # FO4 ships every music file as .xwm but names it .wav in the MUST record
    # (506 of 506 named vanilla tracks) — the audio loader resolves the encoded
    # file from the .wav path. A literal .xwm in the record does not resolve.
    return "Data\\" + str(asset_path.with_suffix(".wav")).replace("/", "\\")


def _legacy_ref_matches_track(legacy_ref: str, legacy_relative_path: str) -> bool:
    wanted = _normalize_legacy_path(legacy_ref)
    relative = _normalize_legacy_path(legacy_relative_path)
    if not wanted:
        return False
    if "." in wanted.rsplit("/", 1)[-1]:
        return _without_audio_extension(relative) == _without_audio_extension(wanted)
    return relative.startswith(wanted.rstrip("/") + "/")


def _normalize_legacy_path(path: str) -> str:
    normalized = path.strip().replace("\\", "/").strip("/").casefold()
    for prefix in ("data/music/", "music/"):
        if normalized.startswith(prefix):
            return normalized[len(prefix) :].strip("/")
    return normalized


def _without_audio_extension(path: str) -> str:
    stem, separator, extension = path.rpartition(".")
    if separator and extension in {suffix.lstrip(".") for suffix in _LEGACY_AUDIO_EXTENSIONS}:
        return stem
    return path


def _find_music_roots(root: Path) -> list[tuple[Path, Path]]:
    found: list[tuple[Path, Path]] = []
    seen: set[str] = set()
    for base in (root, root / "Data"):
        if not base.is_dir():
            continue
        candidates: list[tuple[Path | None, Path]] = [
            (_find_child(base, "music"), Path()),
        ]
        sound_root = _find_child(base, "sound")
        fx_root = _find_child(sound_root, "fx") if sound_root is not None else None
        candidates.append(
            (
                _find_child(fx_root, "mus") if fx_root is not None else None,
                Path("Sound/fx/mus"),
            )
        )
        for candidate, target_prefix in candidates:
            if candidate is None:
                continue
            key = os.path.normcase(str(candidate.resolve()))
            if key not in seen:
                seen.add(key)
                found.append((candidate, target_prefix))
    return found


def _find_child(parent: Path, name: str) -> Path | None:
    direct = parent / name
    if direct.is_dir():
        return direct
    return next(
        (child for child in parent.iterdir() if child.is_dir() and child.name.casefold() == name),
        None,
    )


def _convert_legacy_audio_to_xwm(source: Path, destination: Path) -> None:
    from creation_lib.paths import get_resource_dir
    from pedalboard.io import AudioFile

    encoder = get_resource_dir() / "xWMAEncode.exe"
    if not encoder.is_file():
        raise FileNotFoundError(f"xWMAEncode.exe not found: {encoder}")

    token = f"{os.getpid()}.{threading.get_ident()}"
    temporary_wav = destination.with_name(f".{destination.stem}.{token}.wav")
    temporary_xwm = destination.with_name(f".{destination.stem}.{token}.xwm")
    try:
        with AudioFile(str(source)) as reader:
            with AudioFile(
                str(temporary_wav),
                "w",
                reader.samplerate,
                reader.num_channels,
            ) as writer:
                remaining = reader.frames
                while remaining > 0:
                    frames = min(remaining, reader.samplerate * 30)
                    writer.write(reader.read(frames))
                    remaining -= frames
        result = subprocess.run(
            [str(encoder), str(temporary_wav), str(temporary_xwm)],
            capture_output=True,
            check=False,
            creationflags=_CREATE_NO_WINDOW,
        )
        if result.returncode != 0 or not temporary_xwm.is_file():
            detail = result.stderr.decode(errors="replace").strip()
            raise RuntimeError(detail or f"xWMAEncode failed with exit code {result.returncode}")
        temporary_xwm.replace(destination)
    finally:
        temporary_wav.unlink(missing_ok=True)
        temporary_xwm.unlink(missing_ok=True)
