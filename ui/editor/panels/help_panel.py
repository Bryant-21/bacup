"""Help panel — embedded user guide for the NIF Editor."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# NIF Editor

View, inspect, and edit Bethesda NIF meshes with a full 3D viewport,
scene tree, property editor, and animation timeline.

---

## Quick Start

1. **Open a NIF** — `File > Open` or `Ctrl+O`
2. **Navigate the Scene Tree** — expand nodes to explore the NIF hierarchy
3. **Select a node** — click in the tree or click geometry in the viewport
4. **Inspect properties** — the Properties panel shows fields for the selected block
5. **Edit and save** — modify transforms, flags, or textures, then `Ctrl+S`

---

## Scene Tree

The scene tree shows the hierarchical structure of all NIF blocks:

- **NiNode** — group/bone nodes (transforms, children)
- **BSTriShape / BSSubIndexTriShape** — mesh geometry
- **BSEffectShaderProperty / BSLightingShaderProperty** — materials
- **NiControllerManager** — animation controllers

Click a node to select it. Double-click to frame it in the viewport.
Right-click for context menu (copy, delete, add child).

---

## Properties Panel

Edit fields on the selected block:

- **Transform** — translation, rotation, scale
- **Flags** — visibility, shadow cast/receive, collision
- **Shader Properties** — material type, shader flags, emissive settings
- **Vertex Data** — vertex/triangle counts (read-only summary)

Changes are reflected in the viewport in real time.

---

## Texture Set Editor

When a shader property is selected, the Texture Set Editor shows all
texture slots:

| Slot | Texture | Suffix |
|------|---------|--------|
| 0 | Diffuse | `_d.dds` |
| 1 | Normal | `_n.dds` |
| 2 | Specular / Glow | `_s.dds` / `_g.dds` |
| 3 | Height / Parallax | `_p.dds` |
| 4 | Environment | `_e.dds` |
| 5 | Environment Mask | `_em.dds` |
| 6 | Subsurface | `_sk.dds` |
| 7 | Backlight | `_b.dds` |

Click a slot to browse for a replacement texture.

---

## Render Modes

| Mode | Description |
|------|-------------|
| **Textured** | Default — shows diffuse textures with basic lighting |
| **Wireframe** | Mesh edges only |
| **UV Checker** | Checkered pattern mapped to UVs for distortion checking |
| **Normals** | Visualize normal directions as RGB colors |

Toggle via the toolbar or the View menu.

---

## Skeleton Tools

- **Bone Hierarchy** — tree view of the skeleton structure
- **Generate Partitions** — auto-partition skin data for the selected mesh
- Bones are shown as wireframe overlays when the skeleton layer is enabled

---

## Validation Panel

Checks the NIF for common issues:

- Missing textures or invalid texture paths
- Bones referenced in skin but missing from skeleton
- Zero-weight vertices
- Incorrect shader flags for the target game
- Unsupported block types

---

## Animation

If the NIF contains animation controllers:

- **Mini toolbar** — previous/next sequence, play/pause, stop, loop, and speed +/- controls
- **Timeline** — drag to scrub the viewport pose at the selected time
- **Graph** — edits the selected channel by default; enable context curves when comparison is useful
- **Sequence list** — switch between named animation sequences

---

## Camera Controls

### Navigation Styles

Select in `Settings > Navigation`: **Default**, **3ds Max**, or **Blender**.

**Default / 3ds Max style:**

| Input | Action |
|-------|--------|
| `Alt`+LMB drag | Orbit |
| MMB drag | Pan |
| Scroll wheel | Zoom |

### View Shortcuts

| Key | View |
|-----|------|
| Numpad `4` / `6` | Left / Right |
| Numpad `8` / `2` | Front / Back |
| Numpad `7` / `1` | Top / Bottom |
| `A` | Frame All |
| `S` | Frame Selected |

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open NIF |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `W` | Move gizmo |
| `Alt+LMB` | Rotate light direction |
| `Delete` | Delete selected block |

---

## File Browser

Browse extracted NIF files from the game data directory:

- Filter by folder path or filename
- Double-click to open in the editor
- Attach meshes via connect points for weapon/armor assembly

---

## Batch Operations

Run multi-file operations across folders:

- Texture path replacement
- Block removal by type
- Shader flag updates
- Export reports (block lists, texture lists)

---

## Controls Overlay

Press `H` to toggle an on-screen HUD showing current keybindings
and navigation controls. Useful while learning the editor.

---

## Tips

- Use **Frame Selected** (`S`) to quickly center on a node
- `Alt+LMB` drag in the viewport rotates the light — helpful for
  inspecting normal maps
- The validation panel catches most common NIF issues before in-game testing
- Use batch operations to fix texture paths across an entire mod at once
- Connect points let you preview weapon + scope or armor + body together
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##nif")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
