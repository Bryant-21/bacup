from creation_lib.audio.voice_reference import VoiceLine, VoiceReferenceIndex
from creation_lib.ui.workspaces.voice_browser import VoiceBrowserWorkspace
from creation_lib.ui.workspaces.voice_browser.workspace import (
    _filter_workspace_voice_lines,
    _group_workspace_voice_lines,
    _voice_group_label,
)


def test_voice_browser_workspace_defaults() -> None:
    workspace = VoiceBrowserWorkspace()

    assert workspace.id == "voice_browser"
    assert workspace.name == "Voice Browser"
    assert workspace.get_settings_defaults()["game"] == "fo4"
    assert workspace.get_settings_defaults()["plugin"] == ""
    assert "language" not in workspace.get_settings_defaults()


def test_voice_browser_ignores_saved_language_setting() -> None:
    workspace = VoiceBrowserWorkspace()

    workspace.apply_settings({"language": "French"})
    workspace.initialize()

    assert workspace._language == "English"


def test_voice_line_search_filters_by_dialogue_text_not_character_name() -> None:
    index = _voice_index(
        [
            _voice_line("Nick Valentine", "DetectiveVoice", "The missing person was here."),
            _voice_line("Piper", "PiperVoice", "Valentine is waiting outside."),
        ]
    )

    assert _filter_workspace_voice_lines(index, "Valentine") == [index.lines[1]]
    assert _group_workspace_voice_lines(index, "Valentine") == [("PiperVoice", 1)]


def test_voice_line_search_filters_selected_voice_lines() -> None:
    index = _voice_index(
        [
            _voice_line("Nick Valentine", "DetectiveVoice", "The missing person was here."),
            _voice_line("Nick Valentine", "DetectiveVoice", "The case is closed."),
            _voice_line("Piper", "PiperVoice", "The missing person is a story."),
        ]
    )

    assert _filter_workspace_voice_lines(index, "missing", group="DetectiveVoice") == [index.lines[0]]


def test_voice_browser_groups_by_voice_type() -> None:
    line = _voice_line("Protagonist", "playervoicemale01", "War never changes.")

    assert _voice_group_label(line) == "playervoicemale01"


def test_voice_browser_filters_by_plugin() -> None:
    index = _voice_index(
        [
            _voice_line("Nick Valentine", "DetectiveVoice", "The case is closed.", plugin="Fallout4.esm"),
            _voice_line("Nick Valentine", "DetectiveVoice", "The case is open.", plugin="DLCNukaWorld.esm"),
        ]
    )

    assert _filter_workspace_voice_lines(index, "case", plugin="DLCNukaWorld.esm") == [index.lines[1]]
    assert _group_workspace_voice_lines(index, "case", plugin="DLCNukaWorld.esm") == [("DetectiveVoice", 1)]


def _voice_index(lines: list[VoiceLine]) -> VoiceReferenceIndex:
    return VoiceReferenceIndex(
        game="fo4",
        language="English",
        data_dir="",
        strings_dir="",
        plugin_paths=[],
        archive_paths=[],
        lines=lines,
    )


def _voice_line(
    character: str,
    voice_type: str,
    text: str,
    *,
    plugin: str = "Fallout4.esm",
) -> VoiceLine:
    return VoiceLine(
        game="fo4",
        plugin=plugin,
        info_form_id="00000001",
        response_number=1,
        response_text=text,
        response_filename="00000001_1.fuz",
        voice_type=voice_type,
        characters=[character],
    )
