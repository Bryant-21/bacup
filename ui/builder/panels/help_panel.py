"""Help panel — embedded user guide for the Mod Manager."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# Mod Manager

Create, build, deploy, and manage Bethesda mods with a streamlined
workflow from YAML source to in-game testing.

---

## Quick Start

1. **Create a mod** — click New Mod and enter a name
2. **Add records** — edit YAML files to define weapons, armor, quests, etc.
3. **Build** — compile YAML into an `.esp` plugin file
4. **Deploy** — copy the `.esp` and packed archives to the game Data folder
5. **Test** — launch the game and verify your changes

---

## Mod List

The main panel shows all mods in the `mods/` folder:

| Column | Description |
|--------|-------------|
| **Name** | Mod folder name (always prefixed with your author tag) |
| **Game** | Target game (FO4, SkyrimSE, Starfield) |
| **Status** | Build state — clean, built, deployed, or error |
| **Records** | Count of YAML record files |

Click a mod to select it and see details. Double-click to open
the mod folder in the file browser.

---

## Create Mod

Scaffold a new mod with the proper directory structure:

- **Name** — mod name (author prefix is added automatically)
- **Game** — target game (sets the `.game` file)
- **Template** — optional starting template (weapon, armor, quest, etc.)

The scaffolder creates:
- YAML record directories
- Asset folders (meshes, textures, scripts, sound)
- A `.game` file for game targeting
- A starter YAML file if a template is selected

---

## Build & Deploy

### Build
Compiles YAML source into a loadable `.esp` plugin:

- Validates all YAML records for schema correctness
- Runs modbox to convert YAML to binary `.esp`
- Reports errors with file and line references

### Deploy
Copies built files to the game Data folder:

- `.esp` plugin file
- BA2/BSA archives (packed from asset folders)
- Loose files if archive packing is disabled

### Build + Deploy
One-click shortcut that runs both steps in sequence.

---

## Import

Import an existing `.esp` file as editable YAML:

- Select an `.esp` file from disk
- Serializes it to YAML in a new mod folder
- All records become editable text files
- Useful for studying existing mods or porting between games

---

## Release

Package a mod for distribution:

- Collects `.esp`, archives, and documentation
- Creates a Nexus-ready `.zip` file
- Includes the mod's `README.md` if present
- Optionally includes FOMOD installer metadata

---

## Undeploy

Remove mod files from the game Data folder:

- Deletes the `.esp` and any packed archives
- Does not touch the source files in `mods/`
- Useful for quick cleanup between test iterations

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` | New mod |
| `Ctrl+B` | Build selected mod |
| `Ctrl+D` | Deploy selected mod |
| `Ctrl+Shift+B` | Build + Deploy |
| `Delete` | Undeploy selected mod |

---

## Tips

- Each mod has a **`.game` file** that specifies the target game —
  this is how build scripts know which settings and archive
  format to use
- Use **Import** to learn how vanilla records are structured in YAML
- **Build frequently** — Validation catches schema errors early
- The mod list auto-refreshes when files change on disk
- Deploy copies files — it does not create symlinks (Windows limitation)
- Check the output log at the bottom for detailed build/deploy messages
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##mod_builder")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
