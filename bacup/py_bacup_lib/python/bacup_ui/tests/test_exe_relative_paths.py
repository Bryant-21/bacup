import app.paths as ap


def test_get_exe_dir_frozen_is_exe_parent(monkeypatch, tmp_path):
    exe = tmp_path / "standalone" / "TalesFromAppalachia.exe"
    exe.parent.mkdir(parents=True)
    monkeypatch.setattr(ap.sys, "frozen", True, raising=False)
    monkeypatch.setattr(ap.sys, "executable", str(exe))
    assert ap.get_exe_dir() == exe.parent
