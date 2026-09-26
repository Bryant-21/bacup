from pathlib import Path

import pytest

from bacup_lib import challenge_hud_ui as ui


def test_missing_source_does_not_publish_bundle(tmp_path):
    (tmp_path / "source/interface").mkdir(parents=True)
    with pytest.raises(FileNotFoundError, match="Missing challenge HUD source"):
        ui.convert_challenge_hud_ui(tmp_path / "source", tmp_path / "output")
    assert not (tmp_path / "output").exists()


SOURCE = Path(__file__).resolve().parents[5] / "extracted/fo76"


def test_flyout_roots_carry_the_bridges(tmp_path):
    if not (SOURCE / "interface/challengeflyout.swf").is_file():
        pytest.skip("FO76 source data unavailable")
    from creation_lib.swf import native_runtime

    ui.convert_challenge_hud_ui(SOURCE, tmp_path)
    for name, (root, bridge) in ui.ROOTS.items():
        movie = (tmp_path / ui.OUTPUT / name).read_bytes()
        assert (0, root.encode()) in ui.symbol_classes(movie)
        traits = native_runtime.abc_class_outline(movie, root)["instance_traits"]
        assert any(trait["name"] == bridge and trait["kind"] == "method" for trait in traits)


def test_banner_title_reaches_the_runtime_table(tmp_path):
    if not (SOURCE / "interface/challengeflyout.swf").is_file():
        pytest.skip("FO76 source data unavailable")
    from bacup_lib.translations import read_table, translation_path

    ui.convert_challenge_hud_ui(SOURCE, tmp_path / "data")
    assert "$ChallengeComplete_FlyoutTitle\tCHALLENGE COMPLETE!" in read_table(translation_path(tmp_path / "data"))
