"""Handle-ownership boundary for the unified conversion workflow.

The native ConversionRun is the only long-lived plugin owner. Python must not
hold persistent creation_lib plugin handles for the source plugin or the
target masters — the run reparses both by path, so a Python-side handle
duplicates a multi-GB parse tree for the whole run. Short-lived
open/operate/close handles remain allowed.
"""

import re
from pathlib import Path
from types import SimpleNamespace

import pytest

import bacup_lib.workflows.unified as unified

SRC = Path(unified.__file__).read_text(encoding="utf-8")


class _StubRunner:
    def emit_log(self, *args, **kwargs):
        pass

    def __getattr__(self, name):
        return lambda *args, **kwargs: None


def test_no_python_owned_master_handles():
    assert "open_target_master_handles" not in SRC


def test_source_plugin_handle_is_never_persisted():
    setattr_values = re.findall(
        r'setattr\(\s*ctx,\s*"source_plugin_handle",\s*([^\s,)]+)\s*\)', SRC
    )
    assert setattr_values, "expected the close helper's None reset to survive"
    assert set(setattr_values) == {"None"}

    kwarg_values = re.findall(r"source_plugin_handle=\s*([^\s,)]+)", SRC)
    assert kwarg_values, "expected ConversionContext(source_plugin_handle=None)"
    assert set(kwarg_values) == {"None"}


def test_context_receives_no_persistent_master_handles():
    assert "target_master_handles: list = []" in SRC


def test_run_master_paths_come_from_resolved_official_set():
    # The run loads masters from ctx.target_master_plugin_paths and seeds
    # config.target_master_names from them at creation. The request's explicit
    # paths are usually empty, so this must be the resolved official set —
    # req-only wiring gives the run zero masters (e123e1c1d regression).
    assert "resolve_target_master_plugin_paths(" in SRC
    assert "ctx.target_master_plugin_paths = target_master_plugin_paths" in SRC
    assert (
        "ctx.target_master_plugin_paths = "
        "[Path(path) for path in self._req.target_master_paths]"
    ) not in SRC


def test_build_context_passes_resolved_masters_to_conversion_run(
    tmp_path, monkeypatch
):
    # Behavioral guard for the e123e1c1d regression: an empty explicit master
    # list plus a valid target_data_dir must still hand ConversionRun the
    # resolved official master paths, without retaining any Python handles.
    import bacup_lib.run as run_module
    from bacup_lib.models import PluginPortOptions, PluginPortRequest

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    for name in ("Fallout4.esm", "DLCRobot.esm"):
        (data_dir / name).write_bytes(b"")
    src = tmp_path / "SeventySix.esm"
    src.write_bytes(b"not a real plugin")

    request = PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[src],
        output_root=tmp_path / "out",
        target_data_dir=data_dir,
        options=PluginPortOptions(
            translate_records=False,
            convert_terrain=False,
            build_esp=False,
            convert_scripts=False,
            convert_nifs=False,
            convert_btos=False,
            convert_textures=False,
            convert_materials=False,
            convert_havok=False,
            synthesize_drivers=False,
            convert_animations=False,
            copy_sounds=False,
            validate_output=False,
        ),
    )
    monkeypatch.setattr(unified, "build_target_asset_store", lambda **_: None)
    monkeypatch.setattr(
        unified,
        "build_target_asset_index",
        lambda *args, **kwargs: SimpleNamespace(
            asset_count=0, files_scanned=0, warnings=[], keys=[], owners={}
        ),
    )

    runtime = unified._UnifiedRecordRuntime(request)
    ctx = runtime._build_context(
        src, "SeventySix.esm", tmp_path / "out" / "SeventySix", _StubRunner()
    )

    expected = [data_dir / "Fallout4.esm", data_dir / "DLCRobot.esm"]
    assert ctx.target_master_plugin_paths == expected
    assert ctx.target_master_handles == []
    assert ctx.source_plugin_handle is None

    class _Stop(Exception):
        pass

    captured = {}

    def fake_create_new(*args, **kwargs):
        captured["kwargs"] = kwargs
        raise _Stop

    monkeypatch.setattr(run_module.ConversionRun, "create_new", fake_create_new)

    with pytest.raises(_Stop):
        runtime._translate_records_rust([], ctx, _StubRunner(), None)

    assert captured["kwargs"]["master_plugin_paths"] == [
        str(path) for path in expected
    ]


def test_run_master_and_remap_release_follow_vendor_dialogue():
    vendor = SRC.index("after:vendor_dialogue")
    assert vendor < SRC.index("release_remap_state()")
    assert vendor < SRC.index("release_master_handles()")


def test_run_source_handle_released_before_run_drop():
    assert SRC.index("release_source_handle()") < SRC.index(
        "_drain_and_drop_rust_run(ctx)"
    )
