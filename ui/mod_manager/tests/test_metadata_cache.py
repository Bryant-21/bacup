from pathlib import Path

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from .test_save_inspection import fixture
from .test_standalone import game_fixture


def finish(manager):
    manager._future.result(timeout=10)
    manager.poll()
    assert not manager.error, manager.error


def test_browsing_many_saves_bounds_retained_metadata_images_and_comparisons(tmp_path):
    manager = ManagerController()
    try:
        manager.library = Library(tmp_path / "library", create=True)
        manager.profile_id = create(manager.library, "fo4", game_fixture(tmp_path))
        manager.refresh()
        root = manager.library.root / "runtime" / "profiles" / manager.profile_id / "saves"
        root.mkdir(parents=True)
        sources = [fixture(root / f"generated-{index:02}.fos") for index in range(36)]
        manager.inspect_saves()
        finish(manager)
        for source in sources[:-1]:
            manager.inspect_save_plugins(source.path)
            finish(manager)
            manager.preview_save_screenshot(source.path)
            finish(manager)
            manager.compare_save_to(source.path, sources[-1].path)
            finish(manager)
            assert len(manager.save_inspections) <= 8 and len(manager.save_comparisons) <= 8 and len(manager.save_images) <= 4
        assert len(manager.save_details) == 32 and sources[0].path not in manager.save_inspections
        assert sources[0].path not in manager.save_images and sources[0].path not in manager.save_comparisons
        recent = sources[-2]
        prior = manager.save_images[recent.path]
        Path(recent.path).write_bytes(Path(recent.path).read_bytes() + b"generated concurrent change")
        manager.preview_save_screenshot(recent.path)
        manager._future.exception(timeout=10)
        manager.poll()
        assert manager.job_state == "failed" and manager.save_images[recent.path] is prior
    finally:
        manager.cleanup()
