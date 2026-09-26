import pytest

from bacup_lib import combat_perk_ui as ui


def test_missing_source_never_publishes_a_fake_widget(tmp_path):
    (tmp_path / "source/interface").mkdir(parents=True)
    with pytest.raises(FileNotFoundError, match="Missing combat perk HUD source"):
        ui.convert_combat_perk_ui(tmp_path / "source", tmp_path / "output")
    assert not (tmp_path / "output").exists()
