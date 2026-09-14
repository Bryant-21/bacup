from __future__ import annotations

import hashlib
import shutil
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path, PurePosixPath


_MANIFEST_FIELDS = frozenset(
    {"semantic_role", "source_path", "target_path", "required"}
)
_QUEST_RUNTIME_MANIFEST_FIELDS = frozenset({"output_plugin", "intents"})
_QUEST_RUNTIME_INTENT_FIELDS = frozenset(
    {
        "source_info",
        "target_info",
        "source_speaker",
        "target_speaker",
        "response_number",
        "transcript",
        "voice_type",
        "target_voice_path",
        "source_fuz",
        "source_lip",
        "target_audio",
        "target_lip",
        "sequence",
        "admission",
    }
)
_VOICE_TYPE_FIELDS = frozenset(
    {"source_voice_type", "target_voice_type", "target_identity"}
)
_EVIDENCE_FIELDS = frozenset(
    {"relative_path", "exists", "byte_len", "blake3", "provenance"}
)
_SOURCE_ARCHIVE_PROVENANCE_FIELDS = frozenset({"kind", "archive"})
_LOOSE_FILE_PROVENANCE_FIELDS = frozenset({"kind"})
_CONVERTED_PROVENANCE_FIELDS = frozenset(
    {"kind", "source_blake3", "converter"}
)
_AUDIO_FIELDS = frozenset({"format", "evidence"})
_SEQUENCE_FIELDS = frozenset({"kind", "source_seq", "target_seq"})


@dataclass(frozen=True)
class SkyrimRuntimeAssetCopy:
    semantic_role: str
    source_path: str
    target_path: str
    source_root: str
    size: int
    sha256: str


@dataclass(frozen=True)
class SkyrimRuntimeAssetReceipt:
    rows: tuple[SkyrimRuntimeAssetCopy, ...]

    @property
    def files_copied(self) -> int:
        return len(self.rows)

    @property
    def output_paths(self) -> tuple[str, ...]:
        return tuple(row.target_path for row in self.rows)


@dataclass(frozen=True)
class SkyrimQuestRuntimeAssetCopy:
    """One already-produced quest/dialogue runtime artifact copied to FO4 Data."""

    semantic_role: str
    source_path: str
    target_path: str
    source_root: str
    size: int
    blake3: str
    source_blake3: str


@dataclass(frozen=True)
class SkyrimQuestRuntimeAssetReceipt:
    """Deterministic closure receipt for voice, lip, and sequence outputs."""

    rows: tuple[SkyrimQuestRuntimeAssetCopy, ...]

    @property
    def files_copied(self) -> int:
        return len(self.rows)

    @property
    def output_paths(self) -> tuple[str, ...]:
        return tuple(row.target_path for row in self.rows)


@dataclass(frozen=True)
class _AssetPlan:
    semantic_role: str
    source_path: str
    target_path: str
    source: Path
    source_root: Path
    target: Path


@dataclass(frozen=True)
class _QuestRuntimeAssetPlan:
    semantic_role: str
    target_path: str
    source_evidence: Mapping[str, object]
    target_evidence: Mapping[str, object]


def copy_skyrim_quest_runtime_assets(
    manifest: Mapping[str, object],
    source_asset_roots: Sequence[Path],
    converted_asset_roots: Sequence[Path],
    output_data_dir: Path,
) -> SkyrimQuestRuntimeAssetReceipt:
    """Copy only evidence-backed FO4 quest-runtime artifacts.

    ``manifest`` is the canonical mapping form of the Rust voice intent
    receipt: an output plugin plus admitted voice intents.  FUZ and source LIP
    evidence are checked under ``source_asset_roots``.  XWM/WAV, target LIP,
    and SEQ evidence must already exist under ``converted_asset_roots`` and
    have converted provenance.  This function never transcodes audio, creates
    lip data, or substitutes a raw Skyrim SEQ for its converted output.
    """
    output_plugin, plans = _normalize_quest_runtime_manifest(manifest)
    source_roots = _declared_roots(source_asset_roots, "Skyrim quest source asset")
    converted_roots = _declared_roots(
        converted_asset_roots, "Skyrim converted quest asset"
    )
    output_root = Path(output_data_dir).resolve()

    resolved: list[tuple[_QuestRuntimeAssetPlan, Path, Path, Path]] = []
    for plan in plans:
        source, _ = _find_evidence_asset(plan.source_evidence, source_roots, "source")
        converted, converted_root = _find_evidence_asset(
            plan.target_evidence, converted_roots, "converted"
        )
        _validate_quest_runtime_container(source)
        _validate_quest_runtime_container(converted)
        target = _checked_path(output_root, plan.target_path)
        resolved.append((plan, converted, converted_root, target))

    _reject_undeclared_quest_runtime_assets(output_root, plans)
    copied: list[SkyrimQuestRuntimeAssetCopy] = []
    for plan, converted, converted_root, target in resolved:
        target.parent.mkdir(parents=True, exist_ok=True)
        if converted != target:
            shutil.copy2(converted, target)
        copied.append(
            SkyrimQuestRuntimeAssetCopy(
                semantic_role=plan.semantic_role,
                source_path=str(plan.target_evidence["relative_path"]),
                target_path=plan.target_path,
                source_root=str(converted_root),
                size=target.stat().st_size,
                blake3=_blake3(target),
                source_blake3=str(
                    plan.target_evidence["provenance"]["source_blake3"]
                ),
            )
        )

    receipt = SkyrimQuestRuntimeAssetReceipt(
        tuple(sorted(copied, key=lambda row: (row.target_path.casefold(), row.semantic_role)))
    )
    validate_skyrim_quest_runtime_assets(output_root, manifest, receipt=receipt)
    return receipt


def validate_skyrim_quest_runtime_assets(
    output_data_dir: Path,
    manifest: Mapping[str, object],
    *,
    receipt: SkyrimQuestRuntimeAssetReceipt | None = None,
) -> tuple[str, ...]:
    """Validate the output half of a quest-runtime voice intent manifest."""
    output_root = _require_directory(output_data_dir, "Skyrim quest runtime output Data")
    _, plans = _normalize_quest_runtime_manifest(manifest)
    _reject_undeclared_quest_runtime_assets(output_root, plans)

    expected = {
        (plan.semantic_role, plan.target_path, str(plan.target_evidence["relative_path"]))
        for plan in plans
    }
    output_paths: list[str] = []
    for plan in plans:
        target = _checked_path(output_root, plan.target_path)
        _validate_quest_runtime_container(target)
        if _blake3(target) != str(plan.target_evidence["blake3"]):
            raise ValueError(
                f"Skyrim quest runtime digest differs for {plan.target_path!r}"
            )
        output_paths.append(plan.target_path)

    if receipt is not None:
        actual: set[tuple[str, str, str]] = set()
        for row in receipt.rows:
            identity = (row.semantic_role, row.target_path, row.source_path)
            if identity in actual:
                raise ValueError(f"duplicate Skyrim quest runtime receipt row: {identity!r}")
            actual.add(identity)
            target = _checked_path(output_root, row.target_path)
            if row.size != target.stat().st_size or row.blake3 != _blake3(target):
                raise ValueError(
                    f"Skyrim quest runtime receipt differs for {row.target_path!r}"
                )
        if actual != expected:
            raise ValueError(
                "Skyrim quest runtime receipt does not close the asset manifest: "
                f"missing={sorted(expected - actual)!r}, extra={sorted(actual - expected)!r}"
            )

    return tuple(sorted(output_paths, key=str.casefold))


def copy_skyrim_runtime_assets(
    manifest: Sequence[Mapping[str, object]],
    source_data_dir: Path | None,
    output_data_dir: Path,
    *,
    source_extracted_dir: Path | None = None,
    additional_source_asset_roots: Sequence[Path] = (),
) -> SkyrimRuntimeAssetReceipt:
    normalized_manifest = _normalize_manifest(manifest)
    source_roots = _source_roots(
        source_data_dir=source_data_dir,
        source_extracted_dir=source_extracted_dir,
        additional_source_asset_roots=additional_source_asset_roots,
    )
    output_root = Path(output_data_dir).resolve()
    plans = tuple(
        _plan_asset(entry, source_roots, output_root) for entry in normalized_manifest
    )

    for plan in plans:
        _validate_container(plan.source)
    _reject_undeclared_terminal_assets(output_root, normalized_manifest)

    copied: list[SkyrimRuntimeAssetCopy] = []
    for plan in plans:
        plan.target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(plan.source, plan.target)
        copied.append(
            SkyrimRuntimeAssetCopy(
                semantic_role=plan.semantic_role,
                source_path=plan.source_path,
                target_path=plan.target_path,
                source_root=str(plan.source_root),
                size=plan.target.stat().st_size,
                sha256=_sha256(plan.target),
            )
        )

    receipt = SkyrimRuntimeAssetReceipt(tuple(copied))
    validate_skyrim_runtime_assets(output_root, manifest, receipt=receipt)
    return receipt


def validate_skyrim_runtime_assets(
    output_data_dir: Path,
    asset_manifest: Sequence[Mapping[str, object]],
    *,
    receipt: SkyrimRuntimeAssetReceipt | None = None,
) -> tuple[str, ...]:
    output_root = _require_directory(output_data_dir, "Skyrim runtime output Data")
    manifest = _normalize_manifest(asset_manifest)
    _reject_undeclared_terminal_assets(output_root, manifest)

    expected_receipt = {
        (entry["semantic_role"], entry["source_path"], entry["target_path"])
        for entry in manifest
    }
    actual_receipt: set[tuple[str, str, str]] = set()
    output_paths: list[str] = []
    for entry in manifest:
        target_path = entry["target_path"]
        target = _checked_path(output_root, target_path)
        _validate_container(target)
        output_paths.append(target_path)

    if receipt is not None:
        for copied in receipt.rows:
            identity = (
                copied.semantic_role,
                copied.source_path,
                copied.target_path,
            )
            if identity in actual_receipt:
                raise ValueError(f"duplicate Skyrim runtime receipt row: {identity!r}")
            actual_receipt.add(identity)
            target = _checked_path(output_root, copied.target_path)
            if copied.size != target.stat().st_size:
                raise ValueError(
                    f"Skyrim runtime receipt size differs for {copied.target_path!r}"
                )
            if copied.sha256 != _sha256(target):
                raise ValueError(
                    f"Skyrim runtime receipt digest differs for {copied.target_path!r}"
                )
        if actual_receipt != expected_receipt:
            missing = expected_receipt - actual_receipt
            extra = actual_receipt - expected_receipt
            raise ValueError(
                "Skyrim runtime receipt does not close the asset manifest: "
                f"missing={sorted(missing)!r}, extra={sorted(extra)!r}"
            )

    return tuple(output_paths)


def _normalize_manifest(
    rows: Sequence[Mapping[str, object]],
) -> tuple[dict[str, str | bool], ...]:
    if not rows:
        raise ValueError("Skyrim runtime asset manifest must not be empty")
    normalized: list[dict[str, str | bool]] = []
    identities: set[tuple[str, str, str]] = set()
    targets: set[str] = set()
    for index, row in enumerate(rows):
        if not isinstance(row, Mapping) or frozenset(row) != _MANIFEST_FIELDS:
            raise ValueError(
                f"Skyrim runtime asset row {index} must have exactly "
                f"{sorted(_MANIFEST_FIELDS)!r}"
            )
        role = row["semantic_role"]
        source = row["source_path"]
        target = row["target_path"]
        required = row["required"]
        if not isinstance(role, str) or not role.strip():
            raise ValueError(f"Skyrim runtime asset row {index} has no semantic role")
        if not isinstance(source, str) or not source.strip():
            raise ValueError(f"Skyrim runtime asset row {index} has no source path")
        if not isinstance(target, str) or not target.strip():
            raise ValueError(f"Skyrim runtime asset row {index} has no target path")
        if required is not True:
            raise ValueError(
                f"Skyrim runtime closure row {index} must be explicitly required"
            )
        source = _portable_relative_path(source)
        target = _portable_relative_path(target)
        if PurePosixPath(source).suffix.casefold() not in {".xwm", ".fuz"}:
            raise ValueError(f"unsupported Skyrim runtime source asset: {source!r}")
        if (
            PurePosixPath(source).suffix.casefold()
            != PurePosixPath(target).suffix.casefold()
        ):
            raise ValueError(
                f"Skyrim runtime source/target containers differ: {source!r}, {target!r}"
            )
        identity = (role, source.casefold(), target.casefold())
        if identity in identities:
            raise ValueError(f"duplicate Skyrim runtime asset row {index}")
        identities.add(identity)
        if target.casefold() in targets:
            raise ValueError(f"duplicate Skyrim runtime target path: {target!r}")
        targets.add(target.casefold())
        normalized.append(
            {
                "semantic_role": role,
                "source_path": source,
                "target_path": target,
                "required": True,
            }
        )
    return tuple(normalized)


def _normalize_quest_runtime_manifest(
    manifest: Mapping[str, object],
) -> tuple[str, tuple[_QuestRuntimeAssetPlan, ...]]:
    if not isinstance(manifest, Mapping) or frozenset(manifest) != _QUEST_RUNTIME_MANIFEST_FIELDS:
        raise ValueError(
            "Skyrim quest runtime asset manifest must have exactly "
            f"{sorted(_QUEST_RUNTIME_MANIFEST_FIELDS)!r}"
        )
    output_plugin = manifest["output_plugin"]
    intents = manifest["intents"]
    if not isinstance(output_plugin, str):
        raise ValueError("Skyrim quest runtime asset manifest has no output plugin")
    output_namespace = _canonical_path_component(output_plugin.rsplit(".", 1)[0])
    if output_namespace is None:
        raise ValueError("Skyrim quest runtime asset manifest has an invalid output plugin")
    if not isinstance(intents, Sequence) or isinstance(intents, (str, bytes)) or not intents:
        raise ValueError("Skyrim quest runtime asset manifest must have admitted intents")

    plans: list[_QuestRuntimeAssetPlan] = []
    target_paths: set[str] = set()
    for index, intent in enumerate(intents):
        plans.extend(_normalize_quest_runtime_intent(index, intent, output_namespace))
    for plan in plans:
        key = plan.target_path.casefold()
        if key in target_paths:
            raise ValueError(
                f"Skyrim quest runtime target asset collision: {plan.target_path!r}"
            )
        target_paths.add(key)
    return output_namespace, tuple(
        sorted(plans, key=lambda plan: (plan.target_path.casefold(), plan.semantic_role))
    )


def _normalize_quest_runtime_intent(
    index: int,
    raw_intent: object,
    output_namespace: str,
) -> tuple[_QuestRuntimeAssetPlan, ...]:
    if not isinstance(raw_intent, Mapping) or frozenset(raw_intent) != _QUEST_RUNTIME_INTENT_FIELDS:
        raise ValueError(
            f"Skyrim quest runtime intent {index} must have exactly "
            f"{sorted(_QUEST_RUNTIME_INTENT_FIELDS)!r}"
        )
    if raw_intent["admission"] != "ready":
        raise ValueError(
            f"Skyrim quest runtime intent {index} is not admitted for asset copy"
        )
    transcript = raw_intent["transcript"]
    if not isinstance(transcript, str) or not transcript.strip():
        raise ValueError(f"Skyrim quest runtime intent {index} has no voiced transcript")
    response_number = raw_intent["response_number"]
    if not isinstance(response_number, int) or isinstance(response_number, bool) or not 0 <= response_number <= 255:
        raise ValueError(f"Skyrim quest runtime intent {index} has an invalid response number")

    voice_type = raw_intent["voice_type"]
    if not isinstance(voice_type, Mapping) or frozenset(voice_type) != _VOICE_TYPE_FIELDS:
        raise ValueError(f"Skyrim quest runtime intent {index} has no complete voice type")
    identity = voice_type["target_identity"]
    if not isinstance(identity, str):
        raise ValueError(f"Skyrim quest runtime intent {index} has an invalid voice identity")
    identity = _canonical_path_component(identity)
    if identity is None:
        raise ValueError(f"Skyrim quest runtime intent {index} has an invalid voice identity")

    voice_path = raw_intent["target_voice_path"]
    if not isinstance(voice_path, str):
        raise ValueError(f"Skyrim quest runtime intent {index} has no target voice path")
    voice_path = _portable_relative_path(voice_path).casefold()
    expected_voice_path = f"sound/voice/{output_namespace}/{identity}"
    if voice_path != expected_voice_path:
        raise ValueError(
            f"Skyrim quest runtime intent {index} has a noncanonical target voice path"
        )

    source_fuz = _normalize_evidence(
        raw_intent["source_fuz"], f"Skyrim quest runtime intent {index} source FUZ"
    )
    source_lip = _normalize_evidence(
        raw_intent["source_lip"], f"Skyrim quest runtime intent {index} source LIP"
    )
    _validate_source_evidence(source_fuz, "fuz")
    _validate_source_evidence(source_lip, "lip")
    audio_format, target_audio = _normalize_target_audio(
        raw_intent["target_audio"], index
    )
    target_lip = _normalize_evidence(
        raw_intent["target_lip"], f"Skyrim quest runtime intent {index} target LIP"
    )
    _validate_converted_evidence(target_audio, audio_format, source_fuz["blake3"])
    _validate_converted_evidence(target_lip, "lip", source_lip["blake3"])

    target_audio_path = str(target_audio["relative_path"])
    target_lip_path = str(target_lip["relative_path"])
    audio_relative = PurePosixPath(target_audio_path)
    lip_relative = PurePosixPath(target_lip_path)
    if (
        audio_relative.parent.as_posix() != voice_path
        or lip_relative.parent.as_posix() != voice_path
        or audio_relative.stem != lip_relative.stem
    ):
        raise ValueError(
            f"Skyrim quest runtime intent {index} voice assets are outside one target voice identity"
        )

    plans = [
        _QuestRuntimeAssetPlan(
            semantic_role="dialogue_audio",
            target_path=target_audio_path,
            source_evidence=source_fuz,
            target_evidence=target_audio,
        ),
        _QuestRuntimeAssetPlan(
            semantic_role="dialogue_lip",
            target_path=target_lip_path,
            source_evidence=source_lip,
            target_evidence=target_lip,
        ),
    ]
    sequence = raw_intent["sequence"]
    if sequence is not None:
        plans.append(_normalize_sequence(sequence, index))
    return tuple(plans)


def _normalize_target_audio(
    raw_audio: object, index: int
) -> tuple[str, dict[str, object]]:
    if not isinstance(raw_audio, Mapping) or frozenset(raw_audio) != _AUDIO_FIELDS:
        raise ValueError(f"Skyrim quest runtime intent {index} has no target audio evidence")
    audio_format = raw_audio["format"]
    if not isinstance(audio_format, str) or audio_format.casefold() not in {"xwm", "wav"}:
        raise ValueError(f"Skyrim quest runtime intent {index} has an unsupported audio format")
    return audio_format.casefold(), _normalize_evidence(
        raw_audio["evidence"], f"Skyrim quest runtime intent {index} target audio"
    )


def _normalize_sequence(raw_sequence: object, index: int) -> _QuestRuntimeAssetPlan:
    if not isinstance(raw_sequence, Mapping) or frozenset(raw_sequence) != _SEQUENCE_FIELDS:
        raise ValueError(f"Skyrim quest runtime intent {index} has an invalid sequence intent")
    kind = raw_sequence["kind"]
    if kind not in {"scene", "start_game_enabled_quest"}:
        raise ValueError(f"Skyrim quest runtime intent {index} has an unsupported sequence kind")
    source = _normalize_evidence(
        raw_sequence["source_seq"], f"Skyrim quest runtime intent {index} source SEQ"
    )
    target = _normalize_evidence(
        raw_sequence["target_seq"], f"Skyrim quest runtime intent {index} target SEQ"
    )
    _validate_source_evidence(source, "seq")
    _validate_converted_evidence(target, "seq", source["blake3"])
    return _QuestRuntimeAssetPlan(
        semantic_role=f"sequence_{kind}",
        target_path=str(target["relative_path"]),
        source_evidence=source,
        target_evidence=target,
    )


def _normalize_evidence(raw_evidence: object, label: str) -> dict[str, object]:
    if not isinstance(raw_evidence, Mapping) or frozenset(raw_evidence) != _EVIDENCE_FIELDS:
        raise ValueError(f"{label} evidence must have exactly {sorted(_EVIDENCE_FIELDS)!r}")
    path = raw_evidence["relative_path"]
    exists = raw_evidence["exists"]
    byte_len = raw_evidence["byte_len"]
    blake3 = raw_evidence["blake3"]
    provenance = raw_evidence["provenance"]
    if not isinstance(path, str) or not isinstance(exists, bool) or exists is not True:
        raise ValueError(f"{label} evidence must prove an existing file")
    if not isinstance(byte_len, int) or isinstance(byte_len, bool) or byte_len <= 0:
        raise ValueError(f"{label} evidence has an invalid byte length")
    if not isinstance(blake3, str) or not _is_blake3(blake3):
        raise ValueError(f"{label} evidence has an invalid BLAKE3 digest")
    if not isinstance(provenance, Mapping):
        raise ValueError(f"{label} evidence has no provenance")
    return {
        "relative_path": _portable_relative_path(path).casefold(),
        "exists": True,
        "byte_len": byte_len,
        "blake3": blake3.casefold(),
        "provenance": dict(provenance),
    }


def _validate_source_evidence(evidence: Mapping[str, object], extension: str) -> None:
    _validate_evidence_extension(evidence, extension)
    provenance = evidence["provenance"]
    assert isinstance(provenance, Mapping)
    kind = provenance.get("kind")
    if kind == "source_archive" and frozenset(provenance) == _SOURCE_ARCHIVE_PROVENANCE_FIELDS:
        archive = provenance["archive"]
        if isinstance(archive, str) and archive.strip():
            return
    if kind == "loose_file" and frozenset(provenance) == _LOOSE_FILE_PROVENANCE_FIELDS:
        return
    raise ValueError("Skyrim quest runtime source evidence has invalid provenance")


def _validate_converted_evidence(
    evidence: Mapping[str, object], extension: str, source_blake3: object
) -> None:
    _validate_evidence_extension(evidence, extension)
    provenance = evidence["provenance"]
    assert isinstance(provenance, Mapping)
    if frozenset(provenance) != _CONVERTED_PROVENANCE_FIELDS or provenance.get("kind") != "converted":
        raise ValueError("Skyrim quest runtime target evidence is not converted")
    if provenance["source_blake3"] != source_blake3:
        raise ValueError(
            "Skyrim quest runtime converted artifact does not match its source BLAKE3"
        )
    if not isinstance(provenance["converter"], str) or not provenance["converter"].strip():
        raise ValueError("Skyrim quest runtime converted artifact has no converter provenance")


def _validate_evidence_extension(evidence: Mapping[str, object], extension: str) -> None:
    if PurePosixPath(str(evidence["relative_path"])).suffix.casefold() != f".{extension}":
        raise ValueError(
            f"Skyrim quest runtime evidence path does not end in .{extension}"
        )


def _declared_roots(roots: Sequence[Path], label: str) -> tuple[Path, ...]:
    if not roots:
        raise FileNotFoundError(f"{label} roots are required")
    resolved: list[Path] = []
    seen: set[str] = set()
    for root in roots:
        checked = _require_directory(Path(root), label)
        key = str(checked).casefold()
        if key not in seen:
            seen.add(key)
            resolved.append(checked)
    return tuple(resolved)


def _find_evidence_asset(
    evidence: Mapping[str, object], roots: Sequence[Path], kind: str
) -> tuple[Path, Path]:
    relative_path = str(evidence["relative_path"])
    for root in roots:
        candidate = _checked_path(root, relative_path)
        if candidate.is_file():
            if candidate.stat().st_size != evidence["byte_len"]:
                raise ValueError(
                    f"Skyrim quest runtime {kind} asset size differs: {relative_path!r}"
                )
            if _blake3(candidate) != evidence["blake3"]:
                raise ValueError(
                    f"Skyrim quest runtime {kind} asset digest differs: {relative_path!r}"
                )
            return candidate, root
    searched = ", ".join(str(root) for root in roots)
    raise FileNotFoundError(
        f"Skyrim quest runtime {kind} asset is missing: {relative_path} (searched {searched})"
    )


def _validate_quest_runtime_container(path: Path) -> None:
    suffix = path.suffix.casefold()
    if suffix == ".fuz":
        _validate_fuz(path)
    elif suffix == ".xwm":
        _validate_xwm(path)
    elif suffix == ".wav":
        _validate_wav(path)
    elif suffix in {".lip", ".seq"}:
        if path.stat().st_size <= 0:
            raise ValueError(f"Skyrim quest runtime artifact is empty: {path}")
    else:
        raise ValueError(f"unsupported Skyrim quest runtime asset container: {path}")


def _validate_wav(path: Path) -> None:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise FileNotFoundError(f"required Skyrim WAV is missing: {path}") from error
    if len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError(f"Skyrim quest runtime WAV is not RIFF/WAVE: {path}")
    if int.from_bytes(data[4:8], "little") + 8 != len(data):
        raise ValueError(f"Skyrim quest runtime WAV has an invalid RIFF size: {path}")


def _reject_undeclared_quest_runtime_assets(
    output_root: Path, plans: Sequence[_QuestRuntimeAssetPlan]
) -> None:
    expected_by_parent: dict[str, set[str]] = {}
    parent_paths: dict[str, str] = {}
    for plan in plans:
        target = PurePosixPath(plan.target_path)
        parent_key = target.parent.as_posix().casefold()
        expected_by_parent.setdefault(parent_key, set()).add(target.name.casefold())
        parent_paths[parent_key] = target.parent.as_posix()
    for parent_key, expected in expected_by_parent.items():
        terminal = _checked_path(output_root, parent_paths[parent_key])
        if not terminal.exists():
            continue
        if not terminal.is_dir():
            raise ValueError(
                f"Skyrim quest runtime asset terminal is not a directory: {terminal}"
            )
        unexpected = {entry.name.casefold() for entry in terminal.iterdir()} - expected
        if unexpected:
            raise ValueError(
                "Skyrim quest runtime asset terminal contains undeclared entries: "
                + ", ".join(sorted(unexpected))
            )


def _canonical_path_component(value: str) -> str | None:
    component = "".join(
        character.casefold()
        if character.isascii() and (character.isalnum() or character in "_-")
        else "_"
        if character.isspace()
        else ""
        for character in value.strip()
    )
    return component if component and component not in {".", ".."} else None


def _is_blake3(value: str) -> bool:
    return len(value) == 64 and all(character in "0123456789abcdefABCDEF" for character in value)


def _blake3(path: Path) -> str:
    try:
        from bacup_lib import _native

        hashes = _native.conversion_native.conversion_hash_files_blake3(
            [str(path)], None
        )
    except (AttributeError, ImportError, OSError, RuntimeError) as error:
        raise RuntimeError("Skyrim quest runtime asset validation requires native BLAKE3 support") from error
    if not isinstance(hashes, Sequence) or len(hashes) != 1 or not isinstance(hashes[0], str):
        raise RuntimeError("native BLAKE3 support returned an invalid digest result")
    digest = hashes[0].casefold()
    if not _is_blake3(digest):
        raise RuntimeError("native BLAKE3 support returned an invalid digest")
    return digest


def _portable_relative_path(value: str) -> str:
    relative = PurePosixPath(value.strip().replace("\\", "/"))
    if (
        relative.is_absolute()
        or not relative.parts
        or any(part in {"", ".", ".."} or ":" in part for part in relative.parts)
    ):
        raise ValueError(f"unsafe Skyrim runtime asset path: {value!r}")
    return relative.as_posix()


def _source_roots(
    *,
    source_data_dir: Path | None,
    source_extracted_dir: Path | None,
    additional_source_asset_roots: Sequence[Path],
) -> tuple[Path, ...]:
    configured = (
        ("Skyrim extracted assets", source_extracted_dir),
        ("Skyrim source Data", source_data_dir),
        *(
            ("additional Skyrim asset root", root)
            for root in additional_source_asset_roots
        ),
    )
    roots: list[Path] = []
    seen: set[str] = set()
    for label, value in configured:
        if value is None:
            continue
        root = _require_directory(Path(value), label)
        key = str(root).replace("\\", "/").casefold()
        if key not in seen:
            seen.add(key)
            roots.append(root)
    if not roots:
        raise FileNotFoundError(
            "Skyrim runtime assets require an extracted or loose source root"
        )
    return tuple(roots)


def _plan_asset(
    entry: Mapping[str, str | bool],
    source_roots: Sequence[Path],
    output_root: Path,
) -> _AssetPlan:
    source_path = str(entry["source_path"])
    for root in source_roots:
        source = _checked_path(root, source_path)
        if source.is_file():
            return _AssetPlan(
                semantic_role=str(entry["semantic_role"]),
                source_path=source_path,
                target_path=str(entry["target_path"]),
                source=source,
                source_root=root,
                target=_checked_path(output_root, str(entry["target_path"])),
            )
    searched = ", ".join(str(root) for root in source_roots)
    raise FileNotFoundError(
        f"required Skyrim runtime source asset is missing: {source_path} "
        f"(searched {searched})"
    )


def _require_directory(path: Path, label: str) -> Path:
    resolved = Path(path).resolve()
    if not resolved.is_dir():
        raise FileNotFoundError(f"{label} directory is missing: {resolved}")
    return resolved


def _checked_path(root: Path, relative_path: str) -> Path:
    relative = PurePosixPath(_portable_relative_path(relative_path))
    resolved_root = Path(root).resolve()
    path = resolved_root.joinpath(*relative.parts).resolve()
    if not path.is_relative_to(resolved_root):
        raise ValueError(f"Skyrim runtime asset escapes Data: {relative_path!r}")
    return path


def _validate_container(path: Path) -> None:
    suffix = path.suffix.casefold()
    if suffix == ".xwm":
        _validate_xwm(path)
    elif suffix == ".fuz":
        _validate_fuz(path)
    else:
        raise ValueError(f"unsupported Skyrim runtime asset container: {path}")


def _validate_xwm(path: Path) -> None:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise FileNotFoundError(f"required Skyrim XWM is missing: {path}") from error
    if len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"XWMA":
        raise ValueError(f"Skyrim runtime XWM is not RIFF/XWMA: {path}")
    if int.from_bytes(data[4:8], "little") + 8 != len(data):
        raise ValueError(f"Skyrim runtime XWM has an invalid RIFF size: {path}")


def _validate_fuz(path: Path) -> None:
    try:
        with path.open("rb") as stream:
            header = stream.read(4)
        size = path.stat().st_size
    except OSError as error:
        raise FileNotFoundError(f"required Skyrim FUZ is missing: {path}") from error
    if size <= 12 or header != b"FUZE":
        raise ValueError(f"Skyrim runtime voice asset is not a nonempty FUZ: {path}")


def _reject_undeclared_terminal_assets(
    output_root: Path,
    manifest: Sequence[Mapping[str, str | bool]],
) -> None:
    expected_by_parent: dict[str, set[str]] = {}
    parent_paths: dict[str, str] = {}
    for entry in manifest:
        target = PurePosixPath(str(entry["target_path"]))
        parent_key = target.parent.as_posix().casefold()
        expected_by_parent.setdefault(parent_key, set()).add(target.name.casefold())
        parent_paths[parent_key] = target.parent.as_posix()
    for parent_key, expected in expected_by_parent.items():
        terminal = _checked_path(output_root, parent_paths[parent_key])
        if not terminal.exists():
            continue
        if not terminal.is_dir():
            raise ValueError(
                f"Skyrim runtime asset terminal is not a directory: {terminal}"
            )
        actual = {entry.name.casefold() for entry in terminal.iterdir()}
        unexpected = actual - expected
        if unexpected:
            raise ValueError(
                "Skyrim runtime asset terminal contains undeclared entries: "
                + ", ".join(sorted(unexpected))
            )


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()
