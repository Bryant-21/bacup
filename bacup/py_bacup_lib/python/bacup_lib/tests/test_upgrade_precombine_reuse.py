from pathlib import Path
from types import SimpleNamespace

from creation_lib._native import previs_native

from bacup_lib.workflows import unified


def _workspace(tmp_path: Path, *, with_vis: bool = True) -> tuple[Path, Path, Path]:
    mod = tmp_path / "SeventySix"
    (mod / "data/Meshes/Precombined/SeventySix.esm").mkdir(parents=True)
    if with_vis:
        (mod / "data/Vis/SeventySix.esm").mkdir(parents=True)
    for name in ("SeventySix - Geometry.csg", "SeventySix.cdx"):
        (mod / name).write_bytes(b"old")
    plugin = mod / "SeventySix.esm"
    plugin.write_bytes(b"new")
    previous = mod / ".upgrade_previous.esm"
    previous.write_bytes(b"previous")
    return mod, plugin, previous


def _driver():
    return SimpleNamespace(_req=SimpleNamespace(options=SimpleNamespace(generate_previs=True)))


def _runner(logs):
    return SimpleNamespace(emit_log=lambda _level, message: logs.append(message))


def test_reuse_grafts_previous_stamps_and_keeps_the_old_files(tmp_path, monkeypatch):
    mod, plugin, previous = _workspace(tmp_path)
    calls, rebuilt, logs = [], [], []

    def graft(old, new, output):
        calls.append((old, new))
        Path(output).write_bytes(b"grafted")
        return 3

    monkeypatch.setattr(previs_native, "graft_cell_stamps", graft)
    monkeypatch.setattr(unified, "_rebuild_cell_offsets_after_build", lambda *_a: rebuilt.append(True))

    assert unified._reuse_previous_precombines(_driver(), _runner(logs), mod, plugin, previous)
    assert calls == [(str(previous), str(plugin))]
    assert plugin.read_bytes() == b"grafted"
    assert rebuilt == [True]
    assert (mod / "SeventySix - Geometry.csg").read_bytes() == b"old"


def test_reuse_falls_back_when_files_are_missing_or_stamps_do_not_fit(tmp_path, monkeypatch):
    mod, plugin, previous = _workspace(tmp_path, with_vis=False)
    logs = []
    monkeypatch.setattr(previs_native, "graft_cell_stamps", lambda *_a: (_ for _ in ()).throw(AssertionError("no graft")))
    assert not unified._reuse_previous_precombines(_driver(), _runner(logs), mod, plugin, previous)
    assert "Vis UVDs" in logs[-1]

    (mod / "data/Vis/SeventySix.esm").mkdir(parents=True)

    def mismatch(_old, _new, output):
        Path(output).write_bytes(b"partial")
        raise ValueError("stamped CELL 0001F00D is not in the new plugin")

    monkeypatch.setattr(previs_native, "graft_cell_stamps", mismatch)
    assert not unified._reuse_previous_precombines(_driver(), _runner(logs), mod, plugin, previous)
    assert plugin.read_bytes() == b"new"
    assert not plugin.with_name("SeventySix.esm.grafted").exists()
    assert "do not fit" in logs[-1]


def test_previs_is_skipped_after_reuse(tmp_path):
    import pytest

    request = SimpleNamespace(target_game="fo4", options=SimpleNamespace(build_esp=True, generate_previs=True))
    driver = SimpleNamespace(_reused_precombines=False, ctx=SimpleNamespace(output_plugin_name="Missing.esm"), _req=request)
    with pytest.raises(FileNotFoundError):
        unified._generate_previs_after_build(driver, _runner([]), tmp_path)
    driver._reused_precombines = True
    unified._generate_previs_after_build(driver, _runner([]), tmp_path)
