"""Consume FNV dialogue-voice manifests with the FO4 release audio pipeline.

The native FNV converter writes intent only.  This module discovers the
source OGG, decodes it to WAV, and delegates LIP/XWM/FUZ generation to
``creation_lib.audio.release``.  It never writes OGG bytes into a FUZ file.
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from dataclasses import asdict, dataclass
import json
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tempfile
from typing import Any, Literal, Mapping, Sequence

from creation_lib.audio import release as audio_release
from creation_lib.paths import get_resource_dir


VOICE_MANIFEST_VERSION = 1
VoiceStatus = Literal[
    "written",
    "already_exists",
    "source_unavailable",
    "source_ambiguous",
    "tool_missing",
    "decode_failed",
    "transcript_unavailable",
    "fuz_failed",
]


@dataclass(frozen=True)
class FnvVoiceManifestEntry:
    origin_plugin: str
    source_root: str
    source_voice_type: str
    source_candidates: tuple[str, ...]
    target_plugin: str
    target_voice_type_edid: str
    info_form_id: str
    response_index: int
    target_path: str
    transcript: str | None

    @classmethod
    def from_json(cls, payload: dict[str, Any]) -> "FnvVoiceManifestEntry":
        candidates = payload.get("source_candidates")
        if not isinstance(candidates, list) or not all(isinstance(item, str) for item in candidates):
            raise ValueError("voice manifest entry requires string source_candidates")
        entry = cls(
            origin_plugin=_required_string(payload, "origin_plugin"),
            source_root=_required_string(payload, "source_root"),
            source_voice_type=_required_string(payload, "source_voice_type"),
            source_candidates=tuple(candidates),
            target_plugin=_required_string(payload, "target_plugin"),
            target_voice_type_edid=_required_string(payload, "target_voice_type_edid"),
            info_form_id=_normalize_info_form_id(_required_string(payload, "info_form_id")),
            response_index=_required_nonnegative_int(payload, "response_index"),
            target_path=_required_string(payload, "target_path"),
            transcript=payload.get("transcript"),
        )
        if entry.transcript is not None and not isinstance(entry.transcript, str):
            raise ValueError("voice manifest transcript must be a string or null")
        expected = target_voice_relative_path(
            entry.target_plugin,
            entry.target_voice_type_edid,
            entry.info_form_id,
            entry.response_index,
        )
        if entry.target_path != expected:
            raise ValueError(
                f"voice manifest target_path must be {expected!r}, got {entry.target_path!r}"
            )
        expected_sources = _canonical_source_candidates(
            entry.origin_plugin,
            entry.source_voice_type,
            entry.info_form_id,
            entry.response_index,
        )
        if tuple(candidate.casefold() for candidate in entry.source_candidates) != tuple(
            candidate.casefold() for candidate in expected_sources
        ):
            raise ValueError("voice manifest source_candidates are not canonical INFO response paths")
        return entry


@dataclass(frozen=True)
class FnvVoiceManifest:
    entries: tuple[FnvVoiceManifestEntry, ...]

    @classmethod
    def load(cls, manifest_path: Path) -> "FnvVoiceManifest":
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
        if not isinstance(payload, dict) or payload.get("version") != VOICE_MANIFEST_VERSION:
            raise ValueError(f"unsupported FNV voice manifest: {manifest_path}")
        entries = payload.get("entries")
        if not isinstance(entries, list):
            raise ValueError("voice manifest requires an entries array")
        return cls(tuple(FnvVoiceManifestEntry.from_json(entry) for entry in entries))


@dataclass(frozen=True)
class FnvVoiceResult:
    info_form_id: str
    response_index: int
    target_path: str
    status: VoiceStatus
    detail: str
    source_path: str | None = None


class FnvVoiceProcessingError(RuntimeError):
    """Strict mode failure containing every per-response voice status."""

    def __init__(self, results: list[FnvVoiceResult]) -> None:
        self.results = results
        failed = sum(result.status not in {"written", "already_exists"} for result in results)
        super().__init__(f"FNV voice processing failed for {failed} required response(s)")


def target_voice_relative_path(
    target_plugin: str,
    target_voice_type_edid: str,
    info_form_id: str,
    response_index: int,
) -> str:
    """Return the FO4 FUZ path for exactly one INFO response."""
    if not target_plugin or not target_voice_type_edid:
        raise ValueError("target plugin and target voice type EDID are required")
    if response_index < 0:
        raise ValueError("response index must be non-negative")
    return (
        f"Sound/Voice/{target_plugin}/{target_voice_type_edid}/"
        f"{_normalize_info_form_id(info_form_id)}_{response_index + 1}.fuz"
    )


def find_source_voice(
    entry: FnvVoiceManifestEntry,
    source_roots: Mapping[str, Path | Sequence[Path]],
) -> Path | None:
    """Find an extracted FNV OGG without guessing another response's audio."""
    return _find_source_voice_detail(entry, source_roots)[0]


def _find_source_voice_detail(
    entry: FnvVoiceManifestEntry,
    source_roots: Mapping[str, Path | Sequence[Path]],
    *,
    directory_cache: dict[Path, tuple[Path, ...]] | None = None,
) -> tuple[Path | None, str | None]:
    roots = _roots_for_provenance(entry.source_root, source_roots)
    if not roots:
        return None, "no source root is registered for this provenance key"
    canonical = _canonical_source_candidates(
        entry.origin_plugin,
        entry.source_voice_type,
        entry.info_form_id,
        entry.response_index,
    )
    numbered = Path(canonical[0])
    canonical_names = {Path(candidate).name.casefold() for candidate in canonical}
    prefixed_numbered = "_" + numbered.name.casefold()
    matches: set[Path] = set()
    for root in roots:
        resolved_root = root.resolve()
        voice_dir = (resolved_root / numbered.parent).resolve()
        if not voice_dir.is_relative_to(resolved_root) or not voice_dir.is_dir():
            continue
        if directory_cache is None:
            files = tuple(voice_dir.iterdir())
        else:
            files = directory_cache.get(voice_dir)
            if files is None:
                files = tuple(voice_dir.iterdir())
                directory_cache[voice_dir] = files
        for path in files:
            if not path.is_file() or path.suffix.casefold() != ".ogg":
                continue
            name = path.name.casefold()
            if name in canonical_names or name.endswith(prefixed_numbered):
                matches.add(path.resolve())
    if len(matches) == 1:
        return next(iter(matches)), None
    if len(matches) > 1:
        return None, f"{len(matches)} source OGG files match the INFO response suffix"
    return None, "no canonical or uniquely prefixed source OGG exists"


def available_voice_tools(
    *, resource_dir: Path | None = None, ffmpeg_path: str = "ffmpeg"
) -> tuple[bool, tuple[str, ...]]:
    """Return whether all external tools required by the release pipeline exist."""
    resource_dir = resource_dir or get_resource_dir()
    tools = audio_release._tool_paths(resource_dir)
    missing: list[str] = []
    if not _executable_available(ffmpeg_path):
        missing.append("ffmpeg")
    for label, key in (
        ("FaceFXWrapper.exe", "facefx"),
        ("FonixData.cdf", "fonix_cdf"),
        ("xWMAEncode.exe", "xwma"),
        ("BmlFuzEncode.exe", "bmlfuz"),
    ):
        if not Path(tools[key]).is_file():
            missing.append(label)
    return not missing, tuple(missing)


def process_voice_manifest(
    manifest_path: Path,
    *,
    source_roots: Mapping[str, Path | Sequence[Path]],
    output_mod_dir: Path,
    target_plugin: str,
    resource_dir: Path | None = None,
    ffmpeg_path: str = "ffmpeg",
    strict: bool = True,
    workers: int = 1,
) -> list[FnvVoiceResult]:
    """Generate FO4 dialogue FUZ files and return a status for every entry.

    ``source_roots`` maps manifest provenance keys to one or more extracted
    Data roots.  A merged source must supply each FNV, DLC, and FO3 root under
    the key emitted in ``source_root``; the helper never falls back to another
    plugin's voice folder.
    """
    manifest = FnvVoiceManifest.load(manifest_path)
    _validate_manifest_targets(manifest, target_plugin)
    resource_dir = resource_dir or get_resource_dir()
    results: list[FnvVoiceResult | None] = [None] * len(manifest.entries)
    pending: list[tuple[int, FnvVoiceManifestEntry, Path, Path, bool]] = []
    directory_cache: dict[Path, tuple[Path, ...]] = {}
    for index, entry in enumerate(manifest.entries):
        target_path = _output_path(output_mod_dir / "data", entry.target_path)
        if target_path.exists():
            if _is_valid_fuz(target_path):
                results[index] = _result(
                    entry, "already_exists", "validated FUZ already exists"
                )
                continue
            invalid_existing = True
        else:
            invalid_existing = False
        source_path, discovery_error = _find_source_voice_detail(
            entry,
            source_roots,
            directory_cache=directory_cache,
        )
        if source_path is None:
            status: VoiceStatus = (
                "source_ambiguous"
                if discovery_error and "source OGG files match" in discovery_error
                else "source_unavailable"
            )
            results[index] = _result(
                entry, status, discovery_error or "source discovery failed"
            )
            continue
        if not entry.transcript or not entry.transcript.strip():
            results[index] = _result(
                entry,
                "transcript_unavailable",
                "response transcript is empty",
                source_path,
            )
            continue
        pending.append((index, entry, target_path, source_path, invalid_existing))

    if pending:
        available, missing = available_voice_tools(
            resource_dir=resource_dir,
            ffmpeg_path=ffmpeg_path,
        )
        if not available:
            for index, entry, _target_path, source_path, _invalid_existing in pending:
                results[index] = _result(
                    entry,
                    "tool_missing",
                    "required voice tool unavailable: " + ", ".join(missing),
                    source_path,
                )
        else:
            def generate(
                item: tuple[int, FnvVoiceManifestEntry, Path, Path, bool]
            ) -> tuple[int, FnvVoiceResult]:
                index, entry, target_path, source_path, invalid_existing = item
                return index, _generate_voice(
                    entry,
                    target_path=target_path,
                    source_path=source_path,
                    invalid_existing=invalid_existing,
                    resource_dir=resource_dir,
                    ffmpeg_path=ffmpeg_path,
                )

            worker_count = min(max(1, int(workers)), len(pending))
            if worker_count == 1:
                generated = map(generate, pending)
            else:
                with ThreadPoolExecutor(
                    max_workers=worker_count,
                    thread_name_prefix="fnv-voice",
                ) as executor:
                    generated = list(executor.map(generate, pending))
            for index, result in generated:
                results[index] = result

    completed = [result for result in results if result is not None]
    if len(completed) != len(results):
        raise RuntimeError("FNV voice processing produced incomplete result accounting")
    if strict and any(
        result.status not in {"written", "already_exists"} for result in completed
    ):
        raise FnvVoiceProcessingError(completed)
    return completed


def _generate_voice(
    entry: FnvVoiceManifestEntry,
    *,
    target_path: Path,
    source_path: Path,
    invalid_existing: bool,
    resource_dir: Path,
    ffmpeg_path: str,
) -> FnvVoiceResult:
    target_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(prefix="fnv_voice_", dir=target_path.parent) as temp_dir:
            wav_path = Path(temp_dir) / "source.wav"
            decode = subprocess.run(
                [ffmpeg_path, "-y", "-i", str(source_path), str(wav_path)],
                capture_output=True,
                text=True,
                check=False,
            )
            if decode.returncode != 0 or not wav_path.is_file():
                detail = decode.stderr.strip() or "ffmpeg did not produce WAV output"
                return _result(entry, "decode_failed", detail, source_path)
            generated_fuz = audio_release.process_voice_wav(
                str(wav_path),
                entry.transcript or "",
                ffmpeg_path=ffmpeg_path,
                resource_dir=resource_dir,
            )
            generated_path = Path(generated_fuz) if generated_fuz else None
            if generated_path is None or not _is_valid_fuz(generated_path):
                return _result(
                    entry,
                    "fuz_failed",
                    "release audio pipeline did not produce a valid FUZ",
                    source_path,
                )
            generated_path.replace(target_path)
    except OSError as error:
        return _result(entry, "fuz_failed", str(error), source_path)
    detail = "generated through creation_lib.audio.release"
    if invalid_existing:
        detail += "; replaced invalid existing FUZ"
    return _result(entry, "written", detail, source_path)


def write_voice_processing_report(results: list[FnvVoiceResult], report_path: Path) -> None:
    """Persist explicit conversion statuses for conversion reports and reruns."""
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(
        json.dumps({"results": [asdict(result) for result in results]}, indent=2) + "\n",
        encoding="utf-8",
    )


def _result(
    entry: FnvVoiceManifestEntry,
    status: VoiceStatus,
    detail: str,
    source_path: Path | None = None,
) -> FnvVoiceResult:
    return FnvVoiceResult(
        info_form_id=entry.info_form_id,
        response_index=entry.response_index,
        target_path=entry.target_path,
        status=status,
        detail=detail,
        source_path=str(source_path) if source_path else None,
    )


def _safe_relative_path(value: str) -> Path | None:
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts:
        return None
    return Path(*path.parts)


def _normalize_info_form_id(value: str) -> str:
    text = value.strip()
    try:
        return f"{int(text.removeprefix('0x').removeprefix('0X'), 16):08X}"
    except ValueError:
        return text.upper()


def _canonical_source_candidates(
    origin_plugin: str,
    source_voice_type: str,
    info_form_id: str,
    response_index: int,
) -> tuple[str, ...]:
    prefix = f"Sound/Voice/{origin_plugin}/{source_voice_type}/{info_form_id}"
    numbered = f"{prefix}_{response_index + 1}.ogg"
    return (numbered, f"{prefix}.ogg") if response_index == 0 else (numbered,)


def _roots_for_provenance(
    source_root: str,
    source_roots: Mapping[str, Path | Sequence[Path]],
) -> tuple[Path, ...]:
    for key, roots in source_roots.items():
        if key.casefold() != source_root.casefold():
            continue
        if isinstance(roots, Path):
            return (roots,)
        return tuple(Path(root) for root in roots)
    return ()


def _validate_manifest_targets(manifest: FnvVoiceManifest, target_plugin: str) -> None:
    seen: set[str] = set()
    for entry in manifest.entries:
        if entry.target_plugin.casefold() != target_plugin.casefold():
            raise ValueError(
                f"voice manifest INFO {entry.info_form_id} targets {entry.target_plugin!r}, "
                f"not requested plugin {target_plugin!r}"
            )
        key = entry.target_path.replace("\\", "/").casefold()
        if key in seen:
            raise ValueError(f"duplicate FNV voice manifest target path: {entry.target_path}")
        seen.add(key)


def _is_valid_fuz(path: Path) -> bool:
    try:
        if not path.is_file() or path.stat().st_size <= 12:
            return False
        with path.open("rb") as stream:
            return stream.read(4) == b"FUZE"
    except OSError:
        return False


def _output_path(output_data_dir: Path, relative_path: str) -> Path:
    relative = _safe_relative_path(relative_path)
    if relative is None:
        raise ValueError(f"unsafe voice output path: {relative_path!r}")
    output_root = output_data_dir.resolve()
    target = (output_root / relative).resolve()
    if not target.is_relative_to(output_root):
        raise ValueError(f"voice output escapes target data directory: {relative_path!r}")
    return target


def _executable_available(value: str) -> bool:
    return Path(value).is_file() or shutil.which(value) is not None


def _required_string(payload: dict[str, Any], field: str) -> str:
    value = payload.get(field)
    if not isinstance(value, str) or not value:
        raise ValueError(f"voice manifest requires non-empty {field}")
    return value


def _required_nonnegative_int(payload: dict[str, Any], field: str) -> int:
    value = payload.get(field)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValueError(f"voice manifest requires non-negative integer {field}")
    return value
