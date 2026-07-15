"""Help panel — embedded user guide for the Papyrus Editor."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# Papyrus Editor

Write, edit, and compile Papyrus scripts for Bethesda games with
syntax highlighting, LSP integration, and inline diagnostics.

---

## Quick Start

1. **Open a script** — `Ctrl+O` or browse the File Tree panel
2. **Edit** — write code in the syntax-highlighted editor
3. **Save** — `Ctrl+S` to save the current file
4. **Compile** — `Ctrl+B` to compile the script with the game's Papyrus compiler
5. **Fix errors** — check the Diagnostics panel for compiler output

---

## File Tree

The left panel shows a browsable tree of script files:

- **Project Scripts** — `.psc` files in your mod's `Scripts/Source/` folder
- **Base Game Scripts** — read-only vanilla scripts for reference
- **Filter** — type in the search box to filter by filename

Double-click a file to open it in a new editor tab. Right-click for
context menu (new file, rename, delete, reveal in explorer).

---

## Editor

The main code editor supports:

- **Syntax highlighting** — keywords, types, strings, comments, properties
- **Line numbers** — click the gutter to set breakpoints (if debugger attached)
- **Multiple tabs** — open several scripts at once, switch with `Ctrl+Tab`
- **Auto-indent** — smart indentation based on block structure
- **Find / Replace** — `Ctrl+F` to search, `Ctrl+H` to replace

---

## LSP Features

When the Papyrus language server is running:

| Feature | Description |
|---------|-------------|
| **Autocomplete** | Suggestions as you type — functions, properties, types |
| **Go to Definition** | `Ctrl+Click` or `F12` on a symbol to jump to its source |
| **Hover Info** | Hover over a symbol to see its type and documentation |
| **Diagnostics** | Real-time error/warning squiggles in the editor |
| **Find References** | `Shift+F12` to see all usages of a symbol |

---

## Diagnostics Panel

Shows compiler errors and warnings in a sortable list:

- **Error** — code will not compile; must be fixed
- **Warning** — code compiles but may have issues
- Click a diagnostic to jump to the offending line in the editor

Diagnostics update after each save or compile action.

---

## View Settings

Adjust the editor appearance:

- **Font Scale** — increase or decrease text size
- **Line Spacing** — adjust vertical space between lines
- **Theme** — light or dark editor theme
- **Word Wrap** — toggle soft line wrapping

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open file |
| `Ctrl+S` | Save file |
| `Ctrl+N` | New file |
| `Ctrl+B` | Compile current script |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `Ctrl+F` | Find |
| `Ctrl+H` | Find and Replace |
| `Ctrl+Tab` | Next editor tab |
| `Ctrl+W` | Close current tab |
| `F12` | Go to Definition |
| `Shift+F12` | Find References |

---

## Tips

- Every script must **extend a base type** — `ObjectReference`, `Quest`,
  `ActiveMagicEffect`, `ReferenceAlias`, etc.
- Use **Properties** to reference external forms (items, keywords, quests)
  rather than hardcoding FormIDs
- Compile frequently — Papyrus errors can be cryptic, so catching them
  early is easier than debugging a long script
- Open vanilla scripts from the File Tree as a reference for function
  signatures and usage patterns
- The base game scripts are read-only — to override one, copy it to
  your mod's `Scripts/Source/` folder first
- Use `Debug.Trace("MyMod: message")` for runtime logging — output
  appears in the Papyrus log file
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##papyrus")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
