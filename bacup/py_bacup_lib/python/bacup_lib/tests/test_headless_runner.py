"""Tests for NullConversionRunner and StreamingConversionRunner."""
import io

from bacup_lib.runner import (
    NullConversionRunner,
    StreamingConversionRunner,
)


def test_null_runner_has_runner_interface():
    # ConversionRunner is a concrete threaded class, not an ABC/Protocol.
    # NullConversionRunner and StreamingConversionRunner are standalone duck-type
    # implementations — isinstance(runner, ConversionRunner) would require
    # inheritance from a class that mandates a work_fn constructor arg.
    # Use attribute checks instead.
    runner = NullConversionRunner()
    assert hasattr(runner, "emit_log")
    assert hasattr(runner, "emit_status")
    assert hasattr(runner, "emit_phase_start")
    assert hasattr(runner, "emit_item_progress")
    assert hasattr(runner, "emit_phase_complete")
    assert hasattr(runner, "emit_complete")
    assert hasattr(runner, "is_cancelled")


def test_null_runner_silent():
    runner = NullConversionRunner()
    assert runner.is_cancelled() is False
    runner.emit_log("INFO", "anything")
    runner.emit_status("working")
    runner.emit_phase_start(object())
    runner.emit_item_progress(object())
    runner.emit_phase_complete(object())
    runner.emit_complete("mod_path", object())


def test_streaming_runner_writes_log():
    buf = io.StringIO()
    runner = StreamingConversionRunner(stream=buf)
    runner.emit_log("WARN", "a message")
    out = buf.getvalue()
    assert "WARN" in out and "a message" in out


def test_streaming_runner_writes_phase():
    buf = io.StringIO()
    runner = StreamingConversionRunner(stream=buf)
    runner.emit_phase_start("nifs")
    assert "nifs" in buf.getvalue()


def test_streaming_runner_writes_status():
    buf = io.StringIO()
    runner = StreamingConversionRunner(stream=buf)
    runner.emit_status("Writing reports")
    assert "status" in buf.getvalue() and "Writing reports" in buf.getvalue()


def test_streaming_runner_not_cancelled():
    runner = StreamingConversionRunner()
    assert runner.is_cancelled() is False


def test_streaming_runner_default_writes_to_stdout(capsys):
    runner = StreamingConversionRunner()
    runner.emit_log("INFO", "hello")
    assert "hello" in capsys.readouterr().out
