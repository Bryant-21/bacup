from dataclasses import replace
import threading

import pytest

from creation_lib.mod_manager.combined_data import Candidate, resolve
from creation_lib.mod_manager.extensions import Adapter, Registry, builtin_registry
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_save_inspection import fixture
from .test_combined_data import provider
from creation_lib.mod_manager.conflicts import FileEntry


@pytest.mark.parametrize("change", [{"api_version": 2}, {"id": "../invalid"}, {"kind": "execute"}, {"game": ".."}, {"callback": None}, {"name": ""}, {"summary": "x" * 1025}])
def test_invalid_contract_registration_is_explicit(change):
    adapter = builtin_registry().adapters[0]
    with pytest.raises(ManagerError):
        Registry().register(replace(adapter, **change))


def test_explicit_registration_duplicate_missing_and_builtin_metadata(tmp_path):
    registry = builtin_registry()
    assert len(registry.adapters) == 2 and registry.adapter("save", "fallout4").game == "fo4"
    with pytest.raises(ManagerError, match="already registered"):
        registry.register(registry.adapters[0])
    with pytest.raises(ManagerError, match="No registered"):
        registry.adapter("save", "skyrim")
    source = fixture(tmp_path / "generated.fos")
    assert registry.inspect_save("fo4", source).light == ("Small.esl",)
    event = threading.Event()
    event.set()
    with pytest.raises(Cancelled):
        registry.inspect_save("fo4", source, event)


@pytest.mark.parametrize("fault", ["result", "path", "duplicate", "oversized", "stale", "cancel"])
def test_save_callback_faults_are_contained_and_no_result_published(tmp_path, fault):
    source = fixture(tmp_path / "generated.fos")
    result = builtin_registry().inspect_save("fo4", source)
    event = threading.Event()
    def callback(save, cancel):
        if fault == "result":
            return {}
        if fault == "path":
            return replace(result, path="elsewhere.fos")
        if fault == "duplicate":
            return replace(result, regular=("same.esp", "SAME.esp"))
        if fault == "oversized":
            return replace(result, light=("fixture.esl",) * 4097)
        if fault == "stale":
            return replace(result, stamp=(*result.stamp[:2], 0, 0))
        event.set()
        return result
    registry = Registry()
    registry.register(Adapter("test.save", "Generated callback", "save", "fo4", callback, "Fixture only"))
    with pytest.raises((ManagerError, Cancelled)):
        registry.inspect_save("fo4", source, event)


@pytest.mark.parametrize("fault", ["path", "inactive", "explanation"])
def test_archive_callback_cannot_publish_invalid_provenance(fault):
    loose = FileEntry("fixture.txt", (provider("owned"),))
    def callback(*args, **kwargs):
        result = resolve(*args, **kwargs)
        if fault == "path":
            return replace(result, path="another.txt")
        if fault == "inactive":
            return replace(result, winner=Candidate("loose", provider("invented")))
        return replace(result, reason="x" * 4097)
    registry = Registry()
    registry.register(Adapter("test.archive", "Generated callback", "archive", "fo4", callback, "Fixture only"))
    with pytest.raises(ManagerError):
        registry.resolve_archive("fixture.txt", loose, (), {})
