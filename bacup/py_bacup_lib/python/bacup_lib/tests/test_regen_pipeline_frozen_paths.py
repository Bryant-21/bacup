from pathlib import Path
from types import SimpleNamespace

from bacup_lib import regen_pipeline


def test_sanitizer_worker_accepts_shallow_frozen_module_path(monkeypatch, tmp_path):
    captured = {}

    def fake_run(args, **kwargs):
        captured["cwd"] = kwargs["cwd"]
        return SimpleNamespace(
            returncode=0,
            stdout='{"changed": 0, "saved_path": null}\n',
            stderr="",
        )

    monkeypatch.setattr(
        regen_pipeline, "__file__", "X:/bacup_lib/regen_pipeline.py"
    )
    monkeypatch.setattr(regen_pipeline.subprocess, "run", fake_run)

    changed, saved_path = regen_pipeline._run_fo4_ck_sanitizer_worker(
        tmp_path / "SeventySix.esm"
    )

    assert changed == 0
    assert saved_path is None
    assert captured["cwd"] == str(Path("X:/"))
