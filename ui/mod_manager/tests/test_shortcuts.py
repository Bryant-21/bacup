import pytest

from ui.mod_manager import shortcuts
from creation_lib.mod_manager.paths import ManagerError


def test_defaults_and_separate_pane_bindings_are_valid():
    assert shortcuts.validate({}) == shortcuts.DEFAULTS
    custom = shortcuts.validate({"mod_edit": "ctrl + shift + j", "plugin_edit": "Ctrl+Shift+J", "refresh": "F6"})
    assert custom["mod_edit"] == custom["plugin_edit"] == "Ctrl+Shift+J" and custom["refresh"] == "F6"


@pytest.mark.parametrize("value", ["Ctrl+Ctrl+A", "Ctrl+A+B", "Win+J", "Alt+F4", "Ctrl+", 1, "x" * 100])
def test_invalid_chords_fail_without_changing_preferences(value):
    with pytest.raises(ManagerError):
        shortcuts.parse(value)


def test_conflicts_across_global_and_local_contexts_are_rejected():
    with pytest.raises(ManagerError, match="conflicts"):
        shortcuts.validate({"mod_edit": "Ctrl+E"})
    with pytest.raises(ManagerError, match="conflicts"):
        shortcuts.validate({"mod_remove": "F2"})
    assert shortcuts.validate({"mod_remove": ""})["mod_remove"] == ""


def test_corrupted_persisted_settings_recover_defaults_and_unknown_commands_cannot_save():
    assert shortcuts.normalize({"refresh": "bad chord"}) == shortcuts.DEFAULTS
    with pytest.raises(ManagerError, match="unknown"):
        shortcuts.validate({"launch_everything": "Ctrl+J"})
