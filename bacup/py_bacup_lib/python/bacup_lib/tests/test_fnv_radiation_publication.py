from pathlib import Path
from types import SimpleNamespace
import pytest

from bacup_lib.tests.test_unified_driver import (
    StubRunner, UnifiedDriver, make_request, stub_record_runtime,
)


@pytest.mark.parametrize("source_game", ["fnv", "fo3"])
def test_fnv_radiation_catalog_precedes_source_release_and_save(tmp_path, monkeypatch, source_game):
    request = make_request(tmp_path)
    request.source_game = source_game
    request.options.convert_scripts = False
    driver = UnifiedDriver(request, sink_id=None)
    calls = []
    stub_record_runtime(driver, [], monkeypatch)
    original_context = driver.record_runtime._build_context

    class FakeRun:
        id = 123
        mapper_live = True
        source_live = True

        def run_phase(self, phase, **kwargs):
            assert phase == "prepare_fnv_radiation_runtime"
            assert self.mapper_live and self.source_live
            calls.append("prepare")

        def release_remap_state(self):
            self.mapper_live = False
            calls.append("mapper release")

        def release_master_handles(self):
            calls.append("masters release")
            return 0

        def release_source_handle(self):
            self.source_live = False
            calls.append("source release")
            return False

        def save_target(self, path, **kwargs):
            assert not self.source_live
            calls.append("save")
            Path(path).write_bytes(b"final plugin")

    def make_context(*args, **kwargs):
        context = original_context(*args, **kwargs)
        context._rust_conversion_run = FakeRun()
        return context

    def execute_phase(number, label, body, runner, **kwargs):
        if label in {"Legacy Creature Runtime", "Build ESP"}:
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_context)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", execute_phase)
    driver.run_record_track(StubRunner())
    assert calls == ["prepare", "source release", "save"]
