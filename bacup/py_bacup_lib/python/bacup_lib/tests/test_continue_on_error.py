from types import SimpleNamespace

import pytest

from bacup_lib.models import PluginPortOptions
from bacup_lib.workflows import unified


class _Runner:
    def __init__(self) -> None:
        self.logs: list[tuple[str, str]] = []
        self.completed: list[str] = []

    def emit_log(self, level: str, message: str) -> None:
        self.logs.append((level, message))

    def emit_phase_start(self, _progress) -> None:
        pass

    def emit_phase_complete(self, progress) -> None:
        self.completed.append(progress.status)

    def emit_item_progress(self, _progress) -> None:
        pass


def _request(continue_on_error: bool):
    return SimpleNamespace(
        options=PluginPortOptions(continue_on_error=continue_on_error),
        phase_failures=[],
    )


def _fail(_progress):
    raise FileNotFoundError("interface/seventysixmenu.swf")


def test_post_phase_failure_is_recorded_and_skipped_when_continuing():
    request, runner = _request(True), _Runner()

    unified._run_post_phase("Convert Keypad UI", _fail, runner, request=request)

    assert request.phase_failures == ["Convert Keypad UI: interface/seventysixmenu.swf"]
    assert runner.completed == ["error"]
    assert runner.logs[-1][0] == "ERROR" and "continuing" in runner.logs[-1][1]


def test_post_phase_failure_stops_the_run_without_continue():
    request, runner = _request(False), _Runner()

    with pytest.raises(FileNotFoundError):
        unified._run_post_phase("Convert Keypad UI", _fail, runner, request=request)

    assert request.phase_failures == ["Convert Keypad UI: interface/seventysixmenu.swf"]


@pytest.mark.parametrize("continue_on_error", [True, False])
def test_fatal_record_phase_follows_continue_on_error(continue_on_error):
    runtime = unified._UnifiedRecordRuntime.__new__(unified._UnifiedRecordRuntime)
    runtime._req = _request(continue_on_error)
    runner = _Runner()

    def run():
        runtime._run_phase(4, "Build ESP", _fail, runner, raise_on_error=True)

    if continue_on_error:
        run()
    else:
        with pytest.raises(FileNotFoundError):
            run()
    assert runtime._req.phase_failures == ["Build ESP: interface/seventysixmenu.swf"]


def test_soft_record_phase_error_is_recorded():
    runtime = unified._UnifiedRecordRuntime.__new__(unified._UnifiedRecordRuntime)
    runtime._req = _request(False)

    def soft_error(progress):
        progress.status = "error"
        progress.error = "missing BTD for APPALACHIA"

    runtime._run_phase(7, "Convert Terrain", soft_error, _Runner())

    assert runtime._req.phase_failures == ["Convert Terrain: missing BTD for APPALACHIA"]
