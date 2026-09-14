from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import threading
import wave

import pytest

from bacup_lib import fnv_voice
from creation_lib.paths import get_resource_dir


def _manifest(tmp_path: Path, entries: list[dict]) -> Path:
    path = tmp_path / "fnv_voice_manifest.json"
    path.write_text(json.dumps({"version": 1, "entries": entries}), encoding="utf-8")
    return path


def _entry(
    info_form_id: str = "130161",
    response_index: int = 0,
    *,
    origin_plugin: str = "FalloutNV.esm",
    source_root: str = "fnv-base",
) -> dict:
    canonical_info_form_id = f"{int(info_form_id, 16):08X}"
    target = fnv_voice.target_voice_relative_path(
        "B21_nv_FalloutNV.esm",
        "MaleAdult01DefaultB",
        canonical_info_form_id,
        response_index,
    )
    suffix = response_index + 1
    return {
        "origin_plugin": origin_plugin,
        "source_root": source_root,
        "source_voice_type": "MaleAdult",
        "source_candidates": [
            f"Sound/Voice/{origin_plugin}/MaleAdult/{canonical_info_form_id}_{suffix}.ogg",
            *(
                [f"Sound/Voice/{origin_plugin}/MaleAdult/{canonical_info_form_id}.ogg"]
                if response_index == 0
                else []
            ),
        ],
        "target_plugin": "B21_nv_FalloutNV.esm",
        "target_voice_type_edid": "MaleAdult01DefaultB",
        "info_form_id": info_form_id,
        "response_index": response_index,
        "target_path": target,
        "transcript": "A valid response.",
    }


def test_golden_manifest_paths_use_mapped_vtyp_edid(tmp_path: Path) -> None:
    first = _entry("130161", 0)
    second = _entry("134B9B", 0)
    third = _entry("134B9B", 1)
    manifest = fnv_voice.FnvVoiceManifest.load(_manifest(tmp_path, [first, second, third]))

    assert manifest.entries[0].target_path.endswith("MaleAdult01DefaultB/00130161_1.fuz")
    assert manifest.entries[1].target_path.endswith("MaleAdult01DefaultB/00134B9B_1.fuz")
    assert manifest.entries[2].target_path.endswith("MaleAdult01DefaultB/00134B9B_2.fuz")
    assert all("MaleEvenToned" not in item.target_path for item in manifest.entries)
    assert fnv_voice.target_voice_relative_path("Converted.esm", "Voice", "1", 0).endswith(
        "/00000001_1.fuz"
    )


def test_manifest_rejects_target_path_that_does_not_match_response_identity(tmp_path: Path) -> None:
    entry = _entry()
    entry["target_path"] = entry["target_path"].replace("_1.fuz", "_2.fuz")
    with pytest.raises(ValueError, match="target_path"):
        fnv_voice.FnvVoiceManifest.load(_manifest(tmp_path, [entry]))


def test_source_discovery_reports_unavailable_without_guessing_response(tmp_path: Path) -> None:
    entry = _entry("134B9B", 1)
    source_root = tmp_path / "source"
    existing = source_root / "Sound/Voice/FalloutNV.esm/MaleAdult/00134B9B_1.ogg"
    existing.parent.mkdir(parents=True)
    existing.write_bytes(b"not response two")

    results = fnv_voice.process_voice_manifest(
        _manifest(tmp_path, [entry]),
        source_roots={"fnv-base": source_root},
        output_mod_dir=tmp_path / "output",
        target_plugin="B21_nv_FalloutNV.esm",
        resource_dir=tmp_path / "resource",
        strict=False,
    )

    assert results[0].status == "source_unavailable"


def test_source_found_but_missing_external_tool_is_explicit(tmp_path: Path) -> None:
    entry = _entry()
    source_root = tmp_path / "source"
    source = source_root / entry["source_candidates"][0]
    source.parent.mkdir(parents=True)
    source.write_bytes(b"placeholder")

    results = fnv_voice.process_voice_manifest(
        _manifest(tmp_path, [entry]),
        source_roots={"fnv-base": source_root},
        output_mod_dir=tmp_path / "output",
        target_plugin="B21_nv_FalloutNV.esm",
        resource_dir=tmp_path / "resource",
        ffmpeg_path="fnv-voice-missing-ffmpeg",
        strict=False,
    )

    assert results[0].status == "tool_missing"
    assert "ffmpeg" in results[0].detail


def _voice_tools_ready() -> bool:
    ready, _missing = fnv_voice.available_voice_tools(resource_dir=get_resource_dir())
    return ready


@pytest.mark.integration
@pytest.mark.skipif(
    not _voice_tools_ready(),
    reason="requires installed ffmpeg plus FaceFXWrapper, FonixData, xWMAEncode, and BmlFuzEncode",
)
def test_tool_available_pipeline_generates_real_fuz(tmp_path: Path) -> None:
    """Only skips when a required external voice encoder is genuinely absent."""
    entry = _entry()
    source_root = tmp_path / "source"
    ogg_path = source_root / entry["source_candidates"][0]
    ogg_path.parent.mkdir(parents=True)
    wav_path = tmp_path / "input.wav"
    with wave.open(str(wav_path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(44_100)
        wav.writeframes(b"\x00\x00" * 44_100)
    subprocess.run(
        [shutil.which("ffmpeg") or "ffmpeg", "-y", "-i", str(wav_path), str(ogg_path)],
        check=True,
        capture_output=True,
    )

    results = fnv_voice.process_voice_manifest(
        _manifest(tmp_path, [entry]),
        source_roots={"fnv-base": source_root},
        output_mod_dir=tmp_path / "output",
        target_plugin="B21_nv_FalloutNV.esm",
        resource_dir=get_resource_dir(),
    )

    assert results[0].status == "written"
    assert (tmp_path / "output" / "data" / entry["target_path"]).is_file()


def test_multi_root_provenance_finds_fnv_dlc_and_fo3_without_plugin_fallback(tmp_path: Path) -> None:
    fnv = tmp_path / "fnv"
    dlc = tmp_path / "dlc"
    fo3 = tmp_path / "fo3"
    entries = [
        _entry("130161", source_root="fnv-base"),
        _entry("134B9B", source_root="fnv-dlc", origin_plugin="DeadMoney.esm"),
        _entry("134B9C", source_root="fo3-base", origin_plugin="Fallout3.esm"),
    ]
    for root, entry in zip((fnv, dlc, fo3), entries, strict=True):
        source = root / entry["source_candidates"][0]
        source.parent.mkdir(parents=True)
        source.write_bytes(b"source is intentionally not decoded in this test")

    manifest = fnv_voice.FnvVoiceManifest.load(_manifest(tmp_path, entries))
    roots = {"fnv-base": fnv, "fnv-dlc": [dlc], "fo3-base": fo3}
    assert [fnv_voice.find_source_voice(entry, roots) for entry in manifest.entries] == [
        fnv / entries[0]["source_candidates"][0],
        dlc / entries[1]["source_candidates"][0],
        fo3 / entries[2]["source_candidates"][0],
    ]
    assert fnv_voice.find_source_voice(manifest.entries[1], {"fnv-dlc": [fnv]}) is None


def test_prefixed_extracted_ogg_resolves_only_in_its_provenance_voice_folder(tmp_path: Path) -> None:
    entry = _entry()
    source_root = tmp_path / "fnv"
    voice_dir = source_root / Path(entry["source_candidates"][0]).parent
    voice_dir.mkdir(parents=True)
    prefixed = voice_dir / "vtechattic_vtechatticupnvt_00130161_1.ogg"
    prefixed.write_bytes(b"prefixed source")

    manifest = fnv_voice.FnvVoiceManifest.load(_manifest(tmp_path, [entry]))
    assert fnv_voice.find_source_voice(manifest.entries[0], {"fnv-base": source_root}) == prefixed


def test_multiple_prefixed_oggs_fail_closed(tmp_path: Path) -> None:
    entry = _entry()
    source_root = tmp_path / "fnv"
    voice_dir = source_root / Path(entry["source_candidates"][0]).parent
    voice_dir.mkdir(parents=True)
    for prefix in ("vtechattic_vtechatticupnvt", "another_voice"):
        (voice_dir / f"{prefix}_00130161_1.ogg").write_bytes(b"ambiguous source")

    results = fnv_voice.process_voice_manifest(
        _manifest(tmp_path, [entry]),
        source_roots={"fnv-base": source_root},
        output_mod_dir=tmp_path / "output",
        target_plugin="B21_nv_FalloutNV.esm",
        resource_dir=tmp_path / "resource",
        strict=False,
    )
    assert results[0].status == "source_ambiguous"


def test_strict_missing_required_voice_raises_with_statuses(tmp_path: Path) -> None:
    with pytest.raises(fnv_voice.FnvVoiceProcessingError) as error:
        fnv_voice.process_voice_manifest(
            _manifest(tmp_path, [_entry()]),
            source_roots={"fnv-base": tmp_path / "missing"},
            output_mod_dir=tmp_path / "output",
            target_plugin="B21_nv_FalloutNV.esm",
            resource_dir=tmp_path / "resource",
        )
    assert error.value.results[0].status == "source_unavailable"


def test_rejects_target_plugin_mismatch_and_duplicate_target_path(tmp_path: Path) -> None:
    wrong_plugin = _entry()
    wrong_plugin["target_plugin"] = "Other.esm"
    wrong_plugin["target_path"] = fnv_voice.target_voice_relative_path(
        "Other.esm", "MaleAdult01DefaultB", "130161", 0
    )
    with pytest.raises(ValueError, match="not requested plugin"):
        fnv_voice.process_voice_manifest(
            _manifest(tmp_path, [wrong_plugin]),
            source_roots={"fnv-base": tmp_path},
            output_mod_dir=tmp_path / "output",
            target_plugin="B21_nv_FalloutNV.esm",
        )

    first = _entry()
    duplicate = _entry()
    with pytest.raises(ValueError, match="duplicate"):
        fnv_voice.process_voice_manifest(
            _manifest(tmp_path, [first, duplicate]),
            source_roots={"fnv-base": tmp_path},
            output_mod_dir=tmp_path / "output",
            target_plugin="B21_nv_FalloutNV.esm",
        )


def test_invalid_existing_fuz_is_replaced_only_after_success(
    tmp_path: Path, monkeypatch
) -> None:
    entry = _entry()
    source_root = tmp_path / "source"
    source = source_root / entry["source_candidates"][0]
    source.parent.mkdir(parents=True)
    source.write_bytes(b"placeholder")
    existing = tmp_path / "output" / "data" / entry["target_path"]
    existing.parent.mkdir(parents=True)
    existing.write_bytes(b"not a fuz")

    monkeypatch.setattr(fnv_voice, "available_voice_tools", lambda **_kwargs: (True, ()))

    def decode(_command, **_kwargs):
        wav_path = Path(_command[-1])
        wav_path.write_bytes(b"wav")
        return subprocess.CompletedProcess(_command, 0, "", "")

    def encode(wav_path, _transcript, **_kwargs):
        generated = Path(wav_path).with_suffix(".fuz")
        generated.write_bytes(b"FUZE" + b"generated voice")
        return str(generated)

    monkeypatch.setattr(fnv_voice.subprocess, "run", decode)
    monkeypatch.setattr(fnv_voice.audio_release, "process_voice_wav", encode)

    results = fnv_voice.process_voice_manifest(
        _manifest(tmp_path, [entry]),
        source_roots={"fnv-base": source_root},
        output_mod_dir=tmp_path / "output",
        target_plugin="B21_nv_FalloutNV.esm",
        resource_dir=tmp_path / "resource",
        strict=False,
    )

    assert results[0].status == "written"
    assert "replaced invalid" in results[0].detail
    assert existing.read_bytes() == b"FUZE" + b"generated voice"


def test_failed_regeneration_preserves_invalid_existing_fuz(tmp_path: Path) -> None:
    entry = _entry()
    existing = tmp_path / "output" / "data" / entry["target_path"]
    existing.parent.mkdir(parents=True)
    existing.write_bytes(b"not a fuz")

    results = fnv_voice.process_voice_manifest(
        _manifest(tmp_path, [entry]),
        source_roots={"fnv-base": tmp_path / "missing"},
        output_mod_dir=tmp_path / "output",
        target_plugin="B21_nv_FalloutNV.esm",
        resource_dir=tmp_path / "resource",
        strict=False,
    )

    assert results[0].status == "source_unavailable"
    assert existing.read_bytes() == b"not a fuz"


def test_full_manifest_uses_one_tool_probe_and_preserves_result_order(
    tmp_path: Path, monkeypatch
) -> None:
    entries = [_entry("130161"), _entry("134B9B")]
    source_root = tmp_path / "source"
    for entry in entries:
        source = source_root / entry["source_candidates"][0]
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_bytes(b"placeholder")

    probe_count = 0

    def probe(**_kwargs):
        nonlocal probe_count
        probe_count += 1
        return True, ()

    def decode(command, **_kwargs):
        Path(command[-1]).write_bytes(b"wav")
        return subprocess.CompletedProcess(command, 0, "", "")

    worker_threads: set[int] = set()
    worker_lock = threading.Lock()
    worker_barrier = threading.Barrier(2)

    def encode(wav_path, _transcript, **_kwargs):
        with worker_lock:
            worker_threads.add(threading.get_ident())
        worker_barrier.wait(timeout=5)
        generated = Path(wav_path).with_suffix(".fuz")
        generated.write_bytes(b"FUZE" + b"generated voice")
        return str(generated)

    monkeypatch.setattr(fnv_voice, "available_voice_tools", probe)
    monkeypatch.setattr(fnv_voice.subprocess, "run", decode)
    monkeypatch.setattr(fnv_voice.audio_release, "process_voice_wav", encode)

    results = fnv_voice.process_voice_manifest(
        _manifest(tmp_path, entries),
        source_roots={"fnv-base": source_root},
        output_mod_dir=tmp_path / "output",
        target_plugin="B21_nv_FalloutNV.esm",
        resource_dir=tmp_path / "resource",
        workers=2,
    )

    assert probe_count == 1
    assert [result.info_form_id for result in results] == ["00130161", "00134B9B"]
    assert [result.status for result in results] == ["written", "written"]
    assert len(worker_threads) == 2
