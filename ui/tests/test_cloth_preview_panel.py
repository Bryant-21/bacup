import json
import sys
from types import SimpleNamespace

import numpy as np
import pytest

from ui.cloth_maker.panels.preview_panel import _NativeSimState


@pytest.fixture
def fake_havok_native(monkeypatch):
    havok_native = SimpleNamespace()
    nif_native = SimpleNamespace()
    monkeypatch.setitem(
        sys.modules,
        "creation_lib._native",
        SimpleNamespace(havok_native=havok_native, nif_core_native=nif_native),
    )
    return havok_native


def _state() -> _NativeSimState:
    positions = np.array([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], dtype=np.float32)
    return _NativeSimState(
        blob=b"blob",
        num_particles=2,
        fixed_count=1,
        positions=positions.copy(),
        prev_positions=positions.copy(),
        velocities=np.zeros_like(positions),
        fixed_mask=np.array([True, False]),
    )


def test_native_sim_state_does_not_advance_on_native_failure(fake_havok_native):
    def fail_step(*_args):
        raise RuntimeError("bad config")

    fake_havok_native.cloth_step_from_blob_state = fail_step
    state = _state()
    before_positions = state.positions.copy()

    with pytest.raises(RuntimeError, match="bad config"):
        state.step(2, 12, -686.7, 0.0, 0.0, 0.0, 0.999)

    assert state.frame_count == 0
    np.testing.assert_array_equal(state.positions, before_positions)


def test_native_sim_state_uses_incremental_step_api(fake_havok_native):
    calls = []

    def step_state(blob, positions_json, prev_positions_json, config_json):
        calls.append((blob, positions_json, prev_positions_json, config_json))
        return json.dumps({
            "positions": [[0.0, 0.0, 0.0], [1.0, 0.0, -1.0]],
            "prev_positions": [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        })

    def replay_from_zero(*_args):
        raise AssertionError("replay API should not be used when incremental API exists")

    fake_havok_native.cloth_step_from_blob_state = step_state
    fake_havok_native.cloth_simulate_from_blob = replay_from_zero
    state = _state()

    state.step(2, 12, -686.7, 0.0, 0.0, 0.0, 0.999)

    assert state.frame_count == 1
    assert len(calls) == 1
    np.testing.assert_array_equal(
        state.positions,
        np.array([[0.0, 0.0, 0.0], [1.0, 0.0, -1.0]], dtype=np.float32),
    )
    np.testing.assert_array_equal(
        state.velocities,
        np.array([[0.0, 0.0, 0.0], [0.0, 0.0, -1.0]], dtype=np.float32),
    )


def test_preview_panel_reads_nif_bytes_with_pathlib(monkeypatch, tmp_path, fake_havok_native):
    from pathlib import Path

    from ui.cloth_maker.panels.preview_panel import PreviewPanel

    nif_path = tmp_path / "cloth.nif"
    nif_path.write_bytes(b"nif")
    read_paths = []

    def read_bytes(path_self):
        read_paths.append(path_self)
        return b"nif"

    def extract_blob(data):
        assert data == b"nif"
        return b"blob"

    def simulate(_blob, frame, _config):
        z = -float(frame)
        return json.dumps({
            "positions": [[0.0, 0.0, z], [1.0, 0.0, z]],
            "n_particles": 2,
            "fixed_count": 1,
        })

    monkeypatch.setattr(Path, "read_bytes", read_bytes)
    sys.modules["creation_lib._native"].nif_core_native.cloth_extract_blob = extract_blob
    fake_havok_native.cloth_simulate_from_blob = simulate
    app = SimpleNamespace(scene=SimpleNamespace(loaded=True, nif_path=nif_path))
    panel = PreviewPanel(app)

    assert panel._ensure_solver()
    assert read_paths == [nif_path]
