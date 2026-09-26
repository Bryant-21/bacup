"""Tests for NullConversionRunner and StreamingConversionRunner."""
import io

from bacup_lib.runner import (
    NullConversionRunner,
    StreamingConversionRunner,
)


def test_null_runner_is_silent_duck_typed_runner():
    runner = NullConversionRunner()
    assert runner.is_cancelled() is False
    runner.emit_log("INFO", "anything")
    runner.emit_status("working")
    runner.emit_phase_start(object())
    runner.emit_item_progress(object())
    runner.emit_phase_complete(object())
    runner.emit_complete("mod_path", object())


def test_streaming_runner_writes_events(capsys):
    buf = io.StringIO()
    runner = StreamingConversionRunner(stream=buf)
    assert runner.is_cancelled() is False
    runner.emit_log("WARN", "a message")
    runner.emit_phase_start("nifs")
    runner.emit_status("Writing reports")
    out = buf.getvalue()
    assert "WARN" in out and "a message" in out
    assert "nifs" in out
    assert "status" in out and "Writing reports" in out

    StreamingConversionRunner().emit_log("INFO", "hello")
    assert "hello" in capsys.readouterr().out
