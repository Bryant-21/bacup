import subprocess
import sys


def test_matching_brackets_render_with_native_imgui():
    result = subprocess.run(
        [sys.executable, "-c", """
from imgui_bundle import hello_imgui, imgui, immapp
from ui.papyrus.papyrus_syntax_editor import PapyrusSyntaxEditor

editor = PapyrusSyntaxEditor()
editor.set_text('Function Test()\\nEndFunction')
editor.set_cursor_position(0, 13)
params = hello_imgui.RunnerParams()
params.ini_disable = True
params.app_window_params.hidden = True
params.app_window_params.window_geometry.size = (640, 480)
rendered = []

def draw():
    imgui.set_next_window_size(imgui.ImVec2(600, 400))
    imgui.begin('Papyrus render regression')
    editor.render('editor', False, imgui.ImVec2(560, 320), False)
    imgui.end()
    rendered.append(True)
    params.app_shall_exit = True

params.callbacks.show_gui = draw
immapp.run(params)
assert rendered
"""],
        capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == 0, result.stdout + result.stderr
