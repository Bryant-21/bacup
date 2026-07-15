"""Help panel — embedded user guide for the Scope Aligner."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# Scope Aligner

Visually align weapon scopes, sights, and attachments on Bethesda
game meshes with real-time 3D preview and animation support.

---

## Quick Start

1. **Load a weapon mesh** — set the weapon NIF path in the Setup panel
2. **Load a scope mesh** — set the scope/sight NIF path
3. **Load an animation** (optional) — set the ADS (aim down sights) animation
4. **Adjust offsets** — use the Offsets panel sliders to position the scope
5. **Export** — save the aligned scope NIF with updated transforms

---

## Setup Panel

Configure the input files for alignment:

| Field | Description |
|-------|-------------|
| **Weapon Mesh** | Path to the base weapon `.nif` file |
| **Scope Mesh** | Path to the scope or sight `.nif` file |
| **Animation** | Path to the ADS animation `.hkx` file (optional) |

Use the browse buttons to select files, or drag and drop onto the fields.

---

## Offsets Panel

Fine-tune the scope position and orientation with six sliders:

**Translation:**
- **X** — left / right offset
- **Y** — forward / backward offset
- **Z** — up / down offset

**Rotation:**
- **Pitch** — tilt up / down
- **Yaw** — rotate left / right
- **Roll** — twist clockwise / counterclockwise

Each slider supports Ctrl+click for precise numeric input.
Use the reset button to return all offsets to zero.

---

## Output Panel

Configure export settings:

- **Output Path** — where the aligned NIF will be saved
- **Apply to Node** — which node in the scope NIF receives the transform
- **Coordinate Space** — local or world space offset application
- **Export** — write the modified NIF with the current offset values

---

## Preset Bodies

Load reference body meshes for visual context:

- Helps judge scope height relative to the character's eye position
- Select from built-in presets (male, female) or load a custom body NIF
- The body mesh is display-only and not included in the export

---

## Viewport

The 3D preview shows the weapon and scope together in real time.

| Input | Action |
|-------|--------|
| `Alt`+LMB drag | Orbit camera |
| MMB drag | Pan camera |
| Scroll wheel | Zoom in / out |
| `A` | Frame all meshes |
| `S` | Frame scope |

When an animation is loaded, use the playback controls to preview
the aim-down-sights motion and verify alignment at full zoom.

---

## Tips

- **Crosshair overlay** — enable it in the viewport toolbar to see a
  centered reticle; align the scope's lens to this point
- **Animation preview** — load the ADS animation to verify the scope
  lines up correctly when the weapon is raised to the camera
- **Start coarse, refine fine** — use large slider ranges first to get
  the ballpark position, then narrow the range for pixel-perfect tuning
- **Compare in-game** — export, deploy, and check in first-person;
  the viewport is approximate but not pixel-identical to the engine
- Use the **zoom slider** in the viewport toolbar for close-up inspection
  of the reticle area
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##aligner")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
