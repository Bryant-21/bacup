"""Validated persisted command chords, scoped to the focused manager pane."""
from dataclasses import dataclass

from creation_lib.mod_manager.paths import ManagerError


DEFAULTS = {
    "search": "Ctrl+F", "profiles": "Ctrl+P", "executables": "Ctrl+E", "settings": "Ctrl+S",
    "nexus_game": "Ctrl+N", "install_picker": "Ctrl+M", "refresh": "F5", "dismiss": "Escape",
    "mod_folder": "Ctrl+Enter", "mod_select_all": "Ctrl+A", "mod_undo": "Ctrl+Z",
    "mod_up": "Ctrl+Up", "mod_down": "Ctrl+Down", "mod_edit": "F2", "mod_remove": "Delete",
    "mod_toggle": "Space", "mod_activate": "Enter", "mod_collapse_group": "Shift+Enter",
    "plugin_folder": "Ctrl+Enter", "plugin_select_all": "Ctrl+A", "plugin_up": "Ctrl+Up",
    "plugin_down": "Ctrl+Down", "plugin_toggle": "Space", "plugin_edit": "F2",
}
LABELS = {
    "search": "Focus search", "profiles": "Manage profiles", "executables": "Edit executables",
    "settings": "Open settings", "nexus_game": "Open game Nexus page", "install_picker": "Choose package",
    "refresh": "Refresh local conflicts", "dismiss": "Dismiss/cancel workflow",
    "mod_folder": "Mod: open folder", "mod_select_all": "Mod: select all", "mod_undo": "Mod: undo",
    "mod_up": "Mod: move up", "mod_down": "Mod: move down", "mod_edit": "Mod/group: edit field",
    "mod_remove": "Mod: review removal", "mod_toggle": "Mod: toggle enabled", "mod_activate": "Mod: activate row",
    "mod_collapse_group": "Mod: collapse selected group", "plugin_folder": "Plugin: open provider",
    "plugin_select_all": "Plugin: select all", "plugin_up": "Plugin: move up", "plugin_down": "Plugin: move down",
    "plugin_toggle": "Plugin: toggle enabled", "plugin_edit": "Plugin: edit draft priority",
}
KEYS = {name.casefold(): (name, enum) for name, enum in (
    ("Enter", "enter"), ("Escape", "escape"), ("Space", "space"), ("Delete", "delete"),
    ("Up", "up_arrow"), ("Down", "down_arrow"), ("Left", "left_arrow"), ("Right", "right_arrow"),
    ("Home", "home"), ("End", "end"), ("PageUp", "page_up"), ("PageDown", "page_down"),
    *[(f"F{i}", f"f{i}") for i in range(1, 25)],
    *[(chr(i).upper(), chr(i)) for i in range(ord("a"), ord("z") + 1)],
    *[(str(i), "_" + str(i)) for i in range(10)],
)}


@dataclass(frozen=True)
class Chord:
    text: str
    key: str
    ctrl: bool = False
    shift: bool = False
    alt: bool = False


def parse(value):
    if not isinstance(value, str) or len(value) > 64:
        raise ManagerError("Shortcut must be a key with optional Ctrl, Shift or Alt modifiers.")
    if not value.strip():
        return None
    parts = [part.strip().casefold() for part in value.split("+")]
    if len(parts) != len(set(parts)) or any(part not in {"ctrl", "shift", "alt"} for part in parts[:-1]) or parts[-1] not in KEYS:
        raise ManagerError("Use one supported key, for example Ctrl+Shift+J or F6; an empty value disables the command.")
    mods = {part for part in parts[:-1]}
    label, key = KEYS[parts[-1]]
    canonical = "+".join([name for name in ("Ctrl", "Shift", "Alt") if name.casefold() in mods] + [label])
    if canonical == "Alt+F4":
        raise ManagerError("Alt+F4 belongs to Windows window management.")
    return Chord(canonical, key, "ctrl" in mods, "shift" in mods, "alt" in mods)


def scope(command):
    return "mod" if command.startswith("mod_") else "plugin" if command.startswith("plugin_") else "global"


def validate(value):
    if not isinstance(value, dict) or any(key not in DEFAULTS for key in value):
        raise ManagerError("Shortcut settings contain an unknown manager command.")
    result, used = {}, {}
    for command, default in DEFAULTS.items():
        chord = parse(value.get(command, default))
        result[command] = chord.text if chord else ""
        if not chord:
            continue
        function_key = chord.key.startswith("f") and chord.key[1:].isdigit()
        if scope(command) == "global" and not chord.ctrl and not chord.alt and not function_key and command != "dismiss":
            raise ManagerError("Global commands require Ctrl/Alt or a function key to preserve row navigation.")
        for other, context in used.get(chord.text, ()):
            if context == scope(command) or "global" in {context, scope(command)}:
                raise ManagerError(f"{LABELS[command]} conflicts with {LABELS[other]} ({chord.text}).")
        used.setdefault(chord.text, []).append((command, scope(command)))
    return result


def normalize(value=None):
    try:
        return validate(value if isinstance(value, dict) else {})
    except ManagerError:
        return dict(DEFAULTS)


def label(workspace, command):
    return getattr(workspace, "_shortcuts", {}).get(command, DEFAULTS[command]) or "Disabled"


def pressed(workspace, command, repeat=False):
    from imgui_bundle import imgui
    chord = parse(getattr(workspace, "_shortcuts", {}).get(command, DEFAULTS[command]))
    if not chord:
        return False
    io = imgui.get_io()
    return (not io.key_super and io.key_ctrl == chord.ctrl and io.key_shift == chord.shift and io.key_alt == chord.alt
            and imgui.is_key_pressed(getattr(imgui.Key, chord.key), repeat))
