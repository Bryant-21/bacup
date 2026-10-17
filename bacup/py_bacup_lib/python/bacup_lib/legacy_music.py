from __future__ import annotations

import json
import os
import subprocess
import threading
from collections.abc import Callable, Iterable
from dataclasses import replace
from pathlib import Path

from bacup_lib.models import AssetRef

_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0

_LEGACY_TRANSCODE_EXTENSIONS = {".mp3", ".ogg", ".wav"}


def discover_legacy_music_tracks(
    *,
    source_game: str,
    target_game: str,
    output_plugin_name: str,
    primary_roots: Iterable[Path],
    additional_roots: Iterable[Path],
) -> list[dict[str, str]]:
    from bacup_lib.native_runtime import load_native_module

    options = {
        "source_game": source_game,
        "target_game": target_game,
        "output_plugin_name": output_plugin_name,
        "primary_roots": [str(root) for root in primary_roots],
        "additional_roots": [str(root) for root in additional_roots],
    }
    return json.loads(
        load_native_module().SourceAssetInventory().discover_legacy_music_json(
            json.dumps(options)
        )
    )


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
