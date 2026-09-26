from pathlib import Path

import pytest

from bacup_lib import workshop_ui as ui

ROOT = Path(__file__).resolve().parents[5]
TARGET = ROOT / "tmp/workshop-prototype/fo4/interface/workshop.swf"


def test_missing_source_does_not_write(tmp_path):
    with pytest.raises(FileNotFoundError):
        ui.convert_workshop_prototype(tmp_path / "source", TARGET, tmp_path / "out")
    assert not (tmp_path / "out").exists()
