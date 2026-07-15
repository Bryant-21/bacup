from pathlib import Path

import app.paths as ap
from bacup_ui.setup import _estimated_extract_gb


def test_get_exe_dir_frozen_is_exe_parent(monkeypatch, tmp_path):
    exe = tmp_path / "standalone" / "TalesFromAppalachia.exe"
    exe.parent.mkdir(parents=True)
    monkeypatch.setattr(ap.sys, "frozen", True, raising=False)
    monkeypatch.setattr(ap.sys, "executable", str(exe))
    assert ap.get_exe_dir() == exe.parent


def test_get_exe_dir_dev_is_app_root(monkeypatch):
    monkeypatch.setattr(ap.sys, "frozen", False, raising=False)
    assert ap.get_exe_dir() == ap.get_app_root()


def test_estimated_extract_gb_sums_ba2(tmp_path):
    data = tmp_path / "Data"
    data.mkdir()
    (data / "a.ba2").write_bytes(b"\0" * (1024**3))  # 1 GiB
    (data / "b.ba2").write_bytes(b"\0" * (1024**3))  # 1 GiB
    (data / "notes.txt").write_bytes(b"ignore me")
    gb = _estimated_extract_gb(str(tmp_path))
    assert gb is not None
    assert 2.5 < gb < 3.1  # 2 GiB * 1.4 = 2.8


def test_estimated_extract_gb_none_without_data(tmp_path):
    assert _estimated_extract_gb(str(tmp_path)) is None
