import json

from creation_lib.swf import native_runtime

from bacup_lib import keypad_ui as ui
from bacup_lib.tests.test_legendary_perks_ui import swf


def test_conversion_changes_only_code_and_keeps_assets_in_output(tmp_path, monkeypatch):
    source = tmp_path / "source/interface"
    source.mkdir(parents=True)
    original = swf([(82, b"stock"), (2, b"selection artwork")])
    (source / ui.MENU).write_bytes(original)
    def compile(data, sources, dependencies):
        assert "KeypadMenu" in sources
        return swf([(82, b"bridge"), (2, b"selection artwork")])
    monkeypatch.setattr(native_runtime, "replace_as3_classes", compile)
    output = tmp_path / "converted/data"
    result = ui.convert_keypad_ui(source.parent, output)
    assert (source / ui.MENU).read_bytes() == original
    assert result["code_object"] == "root1.Menu_mc"
    assert json.loads((output / ui.OUTPUT / "conversion.json").read_text()) == result
    assert (output / ui.OUTPUT / ui.MENU).is_file()
