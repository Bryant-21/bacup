import time
from bacup_lib.runner import Drainer


class _FakeRun:
    """Yields one batch of progress events, then nothing."""

    def __init__(self):
        self._batches = [[
            {"kind": "progress", "phase": "convert_textures_v2",
             "current": 5, "total": 10, "item": "a_d.dds"},
        ]]

    def drain_events(self, _max=256):
        return self._batches.pop(0) if self._batches else []


class _RecordingRunner:
    def __init__(self):
        self.logs = []

    def emit_log(self, level, message):
        self.logs.append((level, message))

    def emit_item_progress(self, progress):
        pass

    def is_cancelled(self):
        return False


def test_drainer_forwards_progress_live():
    run, runner = _FakeRun(), _RecordingRunner()
    drainer = Drainer(run, runner)
    drainer.start()
    time.sleep(0.1)
    drainer.stop()
    joined = " ".join(m for _, m in runner.logs)
    assert "convert_textures_v2" in joined
    assert "5/10" in joined
