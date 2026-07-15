"""Tests for voice changer preset manager."""
import json
import os
import pytest


@pytest.fixture
def tmp_dirs(tmp_path):
    builtin = tmp_path / "builtin"
    user = tmp_path / "user"
    builtin.mkdir()
    user.mkdir()
    # Write a built-in preset
    preset = {
        "name": "Test Preset",
        "description": "A test preset",
        "chain": [
            {"type": "Gain", "enabled": True, "params": {"gain_db": -6.0}}
        ],
    }
    (builtin / "test_preset.json").write_text(json.dumps(preset))
    return builtin, user


class TestPresetManager:
    def test_list_presets_finds_builtin(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        presets = mgr.list_presets()
        assert len(presets) == 1
        assert presets[0]["name"] == "Test Preset"
        assert presets[0]["builtin"] is True

    def test_load_preset(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        preset = mgr.load_preset("test_preset")
        assert preset["name"] == "Test Preset"
        assert len(preset["chain"]) == 1

    def test_save_user_preset(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        chain = [{"type": "Gain", "enabled": True, "params": {"gain_db": 0.0}}]
        mgr.save_preset("my_custom", "My Custom", "custom desc", chain)
        # Should appear in user dir
        assert (tmp_dirs[1] / "my_custom.json").exists()
        loaded = mgr.load_preset("my_custom")
        assert loaded["name"] == "My Custom"

    def test_user_preset_overrides_builtin(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        # Save user version with same slug
        chain = [{"type": "Gain", "enabled": True, "params": {"gain_db": -12.0}}]
        mgr.save_preset("test_preset", "Test Preset Modified", "modified", chain)
        loaded = mgr.load_preset("test_preset")
        assert loaded["name"] == "Test Preset Modified"

    def test_reset_to_default(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        # Save user override
        chain = [{"type": "Gain", "enabled": True, "params": {"gain_db": -12.0}}]
        mgr.save_preset("test_preset", "Modified", "mod", chain)
        # Reset
        mgr.reset_preset("test_preset")
        loaded = mgr.load_preset("test_preset")
        assert loaded["name"] == "Test Preset"

    def test_delete_user_preset(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        chain = [{"type": "Gain", "enabled": True, "params": {"gain_db": 0.0}}]
        mgr.save_preset("deleteme", "Delete Me", "temp", chain)
        assert mgr.load_preset("deleteme") is not None
        mgr.delete_preset("deleteme")
        assert mgr.load_preset("deleteme") is None

    def test_cannot_delete_builtin(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        mgr.delete_preset("test_preset")  # should not raise
        # Builtin still loadable
        loaded = mgr.load_preset("test_preset")
        assert loaded is not None

    def test_is_builtin(self, tmp_dirs):
        from ui.voice_changer.preset_manager import PresetManager
        mgr = PresetManager(builtin_dir=str(tmp_dirs[0]), user_dir=str(tmp_dirs[1]))
        assert mgr.is_builtin("test_preset") is True
        chain = [{"type": "Gain", "enabled": True, "params": {"gain_db": 0.0}}]
        mgr.save_preset("custom", "Custom", "c", chain)
        assert mgr.is_builtin("custom") is False
