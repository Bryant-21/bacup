"""Command palette -- Ctrl+P searchable operation launcher."""
from imgui_bundle import imgui


class CommandPalette:
    def __init__(self, app):
        self.app = app
        self._visible = False
        self._search = ""
        self._commands: list[tuple[str, str, callable]] = []  # (name, category, fn)
        self._filtered: list[tuple[str, str, callable]] = []

        # Keyboard shortcut handled via imgui key events in draw()

    def register(self, name: str, category: str, fn: callable):
        self._commands.append((name, category, fn))

    def toggle(self):
        self._visible = not self._visible
        if self._visible:
            self._search = ""
            self._filtered = list(self._commands)

    def draw(self):
        if not self._visible:
            return
        imgui.set_next_window_size((500, 400), imgui.Cond_.first_use_ever.value)
        imgui.set_next_window_pos(
            (imgui.get_io().display_size.x / 2 - 250,
            imgui.get_io().display_size.y / 4),
            imgui.Cond_.first_use_ever.value,
        )
        _, opened = imgui.begin("Command Palette", True,
                               imgui.WindowFlags_.no_collapse.value)
        if not opened:
            self._visible = False
            imgui.end()
            return

        changed, self._search = imgui.input_text("##cmd_search", self._search)
        if changed:
            q = self._search.lower()
            self._filtered = [(n, c, f) for n, c, f in self._commands if q in n.lower() or q in c.lower()]

        imgui.separator()
        imgui.begin_child("cmd_list", imgui.ImVec2(0, 0))
        for name, category, fn in self._filtered:
            imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.5, 0.5, 0.5, 1.0))
            imgui.text(f"[{category}]")
            imgui.pop_style_color()
            imgui.same_line()
            clicked, _ = imgui.selectable(f"{name}##cmd", False)
            if clicked:
                fn()
                self._visible = False
        imgui.end_child()
        imgui.end()
