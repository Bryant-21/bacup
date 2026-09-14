from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.skyrim_runtime_assets import (
    _blake3,
    copy_skyrim_quest_runtime_assets,
    copy_skyrim_runtime_assets,
    validate_skyrim_quest_runtime_assets,
    validate_skyrim_runtime_assets,
)


FIXTURE_PATH = (
    Path(__file__).parent
    / "fixtures"
    / "skyrim_runtime_audio"
    / "reward_and_voice.json"
)


def _fixture() -> dict[str, list[dict[str, object]]]:
    return json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))


def _manifest() -> list[dict[str, object]]:
    fixture = _fixture()
    return [
        {
            "semantic_role": "music_track",
            "source_path": row["source_path"],
            "target_path": row["target_path"],
            "required": True,
        }
        for row in fixture["music"]
    ] + [
        {
            "semantic_role": "dialogue_voice",
            "source_path": row["source_path"],
            "target_path": row["target_path"],
            "required": True,
        }
        for row in fixture["voice"]
    ]


def _write_xwm(path: Path, payload: bytes = b"music") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    body = b"XWMA" + payload
    path.write_bytes(b"RIFF" + len(body).to_bytes(4, "little") + body)


def _write_fuz(path: Path, payload: bytes = b"voice payload") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"FUZE" + payload)


def _write_sources(root: Path, manifest: list[dict[str, object]]) -> None:
    for row in manifest:
        path = root.joinpath(*Path(str(row["source_path"])).parts)
        if path.suffix.casefold() == ".xwm":
            _write_xwm(path)
        else:
            _write_fuz(path)


def test_fixture_keeps_exact_reward_and_voice_slice_out_of_production() -> None:
    fixture = _fixture()
    assert len(fixture["music"]) == 4
    assert len(fixture["voice"]) == 27
    assert {row["source_music_form_id"] for row in fixture["music"]} == {"0007F810"}
    assert {row["source_track_form_id"] for row in fixture["music"]} == {
        "0002482C",
        "0007AE08",
        "0007AE09",
        "0007AE0A",
    }


def test_copies_and_validates_manifest_receipt_closure(tmp_path: Path) -> None:
    manifest = _manifest()
    source = tmp_path / "source"
    output = tmp_path / "output"
    source.mkdir()
    _write_sources(source, manifest)

    receipt = copy_skyrim_runtime_assets(manifest, source, output)

    assert receipt.files_copied == 31
    assert len(receipt.output_paths) == 31
    assert all(len(row.sha256) == 64 for row in receipt.rows)
    assert (
        validate_skyrim_runtime_assets(output, manifest, receipt=receipt)
        == receipt.output_paths
    )


def test_prefers_archive_extracted_root(tmp_path: Path) -> None:
    manifest = _manifest()
    source_data = tmp_path / "Skyrim" / "Data"
    extracted = tmp_path / "SkyrimExtracted"
    output = tmp_path / "output"
    source_data.mkdir(parents=True)
    extracted.mkdir()
    (source_data / "Skyrim - Sounds.bsa").write_bytes(b"BSA\x00")
    (source_data / "Skyrim - Voices_en0.bsa").write_bytes(b"BSA\x00")
    _write_sources(extracted, manifest)

    receipt = copy_skyrim_runtime_assets(
        manifest,
        source_data,
        output,
        source_extracted_dir=extracted,
    )

    assert receipt.files_copied == 31
    assert {row.source_root for row in receipt.rows} == {str(extracted.resolve())}


def test_missing_source_fails_before_output_is_created(tmp_path: Path) -> None:
    manifest = _manifest()
    source = tmp_path / "source"
    output = tmp_path / "output"
    source.mkdir()
    _write_sources(source, manifest[:-1])

    with pytest.raises(FileNotFoundError, match="source asset is missing"):
        copy_skyrim_runtime_assets(manifest, source, output)

    assert not output.exists()


@pytest.mark.parametrize(
    "contents, message",
    [
        (b"RIFF\x04\x00\x00\x00WAVE", "not RIFF/XWMA"),
        (b"RIFF\x00\x00\x00\x00XWMApayload", "invalid RIFF size"),
    ],
)
def test_invalid_xwm_fails_closed(
    tmp_path: Path, contents: bytes, message: str
) -> None:
    manifest = _manifest()
    source = tmp_path / "source"
    source.mkdir()
    _write_sources(source, manifest)
    first = source.joinpath(*Path(str(manifest[0]["source_path"])).parts)
    first.write_bytes(contents)

    with pytest.raises(ValueError, match=message):
        copy_skyrim_runtime_assets(manifest, source, tmp_path / "output")


def test_undeclared_terminal_asset_is_rejected(tmp_path: Path) -> None:
    manifest = _manifest()
    source = tmp_path / "source"
    output = tmp_path / "output"
    source.mkdir()
    output.mkdir()
    _write_sources(source, manifest)
    _write_xwm(output / "Music" / "Skyrim" / "Reward" / "bonus.xwm")

    with pytest.raises(ValueError, match="undeclared entries: bonus.xwm"):
        copy_skyrim_runtime_assets(manifest, source, output)


def test_manifest_rejects_unsafe_duplicate_and_optional_rows(tmp_path: Path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    row = {
        "semantic_role": "music_track",
        "source_path": "Music/Reward/track.xwm",
        "target_path": "../escape.xwm",
        "required": True,
    }
    with pytest.raises(ValueError, match="unsafe Skyrim runtime asset path"):
        copy_skyrim_runtime_assets([row], source, tmp_path / "output")

    row["target_path"] = "Music/Port/track.xwm"
    row["required"] = False
    with pytest.raises(ValueError, match="must be explicitly required"):
        copy_skyrim_runtime_assets([row], source, tmp_path / "output")


def _quest_evidence(
    path: Path,
    relative_path: str,
    provenance: dict[str, str],
) -> dict[str, object]:
    return {
        "relative_path": relative_path,
        "exists": True,
        "byte_len": path.stat().st_size,
        "blake3": _blake3(path),
        "provenance": provenance,
    }


def _quest_manifest(source: Path, converted: Path) -> dict[str, object]:
    source_fuz = source / "sound" / "voice" / "skyrim.esm" / "line.fuz"
    source_lip = source / "sound" / "voice" / "skyrim.esm" / "line.lip"
    target_audio = (
        converted
        / "sound"
        / "voice"
        / "questport"
        / "nord_male"
        / "001234_00.xwm"
    )
    target_lip = (
        converted
        / "sound"
        / "voice"
        / "questport"
        / "nord_male"
        / "001234_00.lip"
    )
    source_seq = source / "sound" / "sequences" / "quest.seq"
    target_seq = converted / "sound" / "sequences" / "questport.seq"
    _write_fuz(source_fuz)
    source_lip.parent.mkdir(parents=True, exist_ok=True)
    source_lip.write_bytes(b"source lip")
    _write_xwm(target_audio)
    target_lip.write_bytes(b"converted lip")
    source_seq.parent.mkdir(parents=True, exist_ok=True)
    source_seq.write_bytes(b"source sequence")
    target_seq.parent.mkdir(parents=True, exist_ok=True)
    target_seq.write_bytes(b"converted sequence")

    fuz = _quest_evidence(
        source_fuz,
        "sound/voice/skyrim.esm/line.fuz",
        {"kind": "source_archive", "archive": "Skyrim - Voices_en0.bsa"},
    )
    lip = _quest_evidence(
        source_lip,
        "sound/voice/skyrim.esm/line.lip",
        {"kind": "loose_file"},
    )
    sequence = _quest_evidence(
        source_seq,
        "sound/sequences/quest.seq",
        {"kind": "loose_file"},
    )
    audio = _quest_evidence(
        target_audio,
        "sound/voice/questport/nord_male/001234_00.xwm",
        {"kind": "converted", "source_blake3": fuz["blake3"], "converter": "xwm"},
    )
    target_lip_evidence = _quest_evidence(
        target_lip,
        "sound/voice/questport/nord_male/001234_00.lip",
        {"kind": "converted", "source_blake3": lip["blake3"], "converter": "lip"},
    )
    target_sequence = _quest_evidence(
        target_seq,
        "sound/sequences/questport.seq",
        {
            "kind": "converted",
            "source_blake3": sequence["blake3"],
            "converter": "seq-formid-rewrite",
        },
    )
    return {
        "output_plugin": "QuestPort.esp",
        "intents": [
            {
                "source_info": "001234@Skyrim.esm",
                "target_info": "001234@QuestPort.esp",
                "source_speaker": "000123@Skyrim.esm",
                "target_speaker": "000123@QuestPort.esp",
                "response_number": 0,
                "transcript": "A voiced quest line.",
                "voice_type": {
                    "source_voice_type": "000010@Skyrim.esm",
                    "target_voice_type": "000010@QuestPort.esp",
                    "target_identity": "Nord Male",
                },
                "target_voice_path": "sound/voice/questport/nord_male",
                "source_fuz": fuz,
                "source_lip": lip,
                "target_audio": {"format": "xwm", "evidence": audio},
                "target_lip": target_lip_evidence,
                "sequence": {
                    "kind": "scene",
                    "source_seq": sequence,
                    "target_seq": target_sequence,
                },
                "admission": "ready",
            }
        ],
    }


def test_quest_runtime_executor_copies_only_evidence_backed_outputs(
    tmp_path: Path,
) -> None:
    source = tmp_path / "source"
    converted = tmp_path / "converted"
    output = tmp_path / "output"
    source.mkdir()
    converted.mkdir()
    manifest = _quest_manifest(source, converted)

    receipt = copy_skyrim_quest_runtime_assets(
        manifest, [source], [converted], output
    )

    assert receipt.files_copied == 3
    assert receipt.output_paths == (
        "sound/sequences/questport.seq",
        "sound/voice/questport/nord_male/001234_00.lip",
        "sound/voice/questport/nord_male/001234_00.xwm",
    )
    assert validate_skyrim_quest_runtime_assets(output, manifest, receipt=receipt) == receipt.output_paths
    assert not (output / "sound" / "voice" / "skyrim.esm" / "line.fuz").exists()


def test_quest_runtime_executor_rejects_raw_sequence_and_bad_digest(
    tmp_path: Path,
) -> None:
    source = tmp_path / "source"
    converted = tmp_path / "converted"
    source.mkdir()
    converted.mkdir()
    manifest = _quest_manifest(source, converted)
    sequence = manifest["intents"][0]["sequence"]
    assert isinstance(sequence, dict)
    target_seq = sequence["target_seq"]
    assert isinstance(target_seq, dict)
    target_seq["provenance"] = {"kind": "loose_file"}
    with pytest.raises(ValueError, match="target evidence is not converted"):
        copy_skyrim_quest_runtime_assets(manifest, [source], [converted], tmp_path / "output")

    manifest = _quest_manifest(source, converted)
    audio = manifest["intents"][0]["target_audio"]
    assert isinstance(audio, dict)
    evidence = audio["evidence"]
    assert isinstance(evidence, dict)
    evidence["blake3"] = "0" * 64
    with pytest.raises(ValueError, match="asset digest differs"):
        copy_skyrim_quest_runtime_assets(manifest, [source], [converted], tmp_path / "output")


def test_quest_runtime_executor_rejects_target_collisions(tmp_path: Path) -> None:
    source = tmp_path / "source"
    converted = tmp_path / "converted"
    source.mkdir()
    converted.mkdir()
    manifest = _quest_manifest(source, converted)
    duplicate = manifest["intents"][0].copy()
    duplicate["response_number"] = 1
    manifest["intents"].append(duplicate)

    with pytest.raises(ValueError, match="target asset collision"):
        copy_skyrim_quest_runtime_assets(manifest, [source], [converted], tmp_path / "output")
