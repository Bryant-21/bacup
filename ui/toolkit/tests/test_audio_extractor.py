import threading

from ui.tools.audio import extractor
from ui.tools.audio.extractor import AudioExtractorTool


def test_audio_worker_count_uses_half_available_cpus(monkeypatch) -> None:
    monkeypatch.setattr(extractor.os, "cpu_count", lambda: 8)
    assert extractor._audio_worker_count() == 4


def test_audio_worker_count_has_one_worker_minimum(monkeypatch) -> None:
    monkeypatch.setattr(extractor.os, "cpu_count", lambda: None)
    assert extractor._audio_worker_count() == 1


def test_xwm_files_are_converted_in_parallel(tmp_path, monkeypatch) -> None:
    for index in range(4):
        (tmp_path / f"sound_{index}.xwm").write_bytes(b"xwm")

    tool = AudioExtractorTool()
    tool._input_path = str(tmp_path)
    barrier = threading.Barrier(4, timeout=2)

    def convert(_xwm_path: str, wav_path: str) -> None:
        barrier.wait()
        with open(wav_path, "wb") as wav_file:
            wav_file.write(b"wav")

    monkeypatch.setattr(extractor.os, "cpu_count", lambda: 8)
    monkeypatch.setattr(tool, "_convert_xwm_to_wav", convert)

    tool._do_extract()

    assert tool._result_msg == "Processed 4, failed 0 of 4 files."
    assert len(list(tmp_path.glob("*.wav"))) == 4
