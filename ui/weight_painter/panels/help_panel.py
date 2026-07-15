"""Help panel — embedded user guide for the Weight Painter."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# Weight Painter

Paint, transfer, and manage bone weights and dismemberment segments
on Bethesda NIF meshes.

---

## Quick Start

1. **Import a mesh** — `File > Import Mesh` or `Ctrl+I` (`.nif` or `.obj`)
2. **Load a reference body** — toolbar person icon or `File > Load Reference Body`
3. **Transfer weights** — toolbar wand icon or `File > Transfer Weights`
4. **Select a bone** in the Bones panel and start painting
5. **Export** — `File > Export NIF` or `Ctrl+E`

---

## Brushes

Select with the number keys or click in the Brush panel.

| Key | Brush | What it does |
|-----|-------|-------------|
| `1` | **Paint** | Add / subtract / set weight on the selected bone |
| `2` | **Smooth** | Blend weights with neighboring vertices |
| `3` | **Blur** | Average neighbor weights (softer than smooth) |
| `4` | **Gradient** | Two-click: linear weight ramp between two points |
| `5` | **Mirror** | Copy weights across the X=0 plane |
| `6` | **Flood** | Fill all connected vertices to a weight |
| `7` | **Segment** | Assign triangles to a dismemberment segment |
| — | **Mask** | Lock vertices from editing |
| — | **Unmask** | Unlock masked vertices |

**Brush settings** (Brush panel sliders):
- **Radius** — brush size in world units (`[` / `]` to resize)
- **Strength** — how much weight is applied per stroke
- **Falloff** — edge softness (0 = hard edge, 1 = smooth)

**Modifiers:**
- `Shift`+click — temporarily switch to Smooth brush
- The Paint brush has three modes: **Add**, **Subtract**, **Set**

---

## Display Modes

| Key | Mode | Description |
|-----|------|-------------|
| `W` | **Weights** | Heatmap for the selected bone (blue → green → red) |
| `A` | **All Bones** | Blended view of all bone influences |
| `P` | **Segments** | Dismemberment segment colors |
| `V` | **Vertex Colors** | Show baked vertex colors (if present) |
| — | **Shaded** | Plain 3D shading, no overlay |

---

## Overlays

| Key | Overlay | Description |
|-----|---------|-------------|
| `F` | **Wireframe** | Mesh edge lines |
| `B` | **Segment Edges** | Yellow boundary lines between segments |
| `M` | **Show Mask** | Highlight locked vertices |

---

## Keyboard Shortcuts

### General
| Shortcut | Action |
|----------|--------|
| `Ctrl+I` | Import mesh |
| `Ctrl+E` | Export NIF |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `Ctrl+C` | Copy selected bone's weights |
| `Ctrl+V` | Paste copied weights to current bone |

### Toggles
| Key | Toggle |
|-----|--------|
| `X` | Mirror X painting |
| `N` | Auto-normalize |

### Camera
| Input | Action |
|-------|--------|
| `Alt`+drag | Orbit |
| Middle mouse drag | Pan |
| Right mouse drag | Pan |
| Scroll wheel | Zoom |

---

## Segments (Dismemberment)

FO4 meshes use **BSSubIndexTriShape** segments to define body part
regions for dismemberment and physics.  Each segment can contain
**sub-segments** with bone IDs and body part slots.

- The **Segments panel** shows the hierarchy as a tree
- Click a segment to highlight it in the viewport
- Use the **Segment brush** (`7`) to reassign triangles
- Right-click segments for context menu (add/delete/set body part)

---

## Weight Transfer

`File > Transfer Weights` opens the transfer dialog:

- **Source**: reference body preset or custom NIF
- **Methods**:
  - *Barycentric* — project onto source triangles (most precise)
  - *Proximity* — nearest-vertex weighted average (forgiving)
  - *Hybrid* — barycentric with proximity fallback (recommended)
- **Bone Filter** — only transfer bones matching a substring
- **Transfer Segments** — also copy segment assignments

---

## Bones Panel

- **Hierarchical tree** showing parent-child bone relationships
- **Search** to filter bones by name
- **All Bones / Single Bone** toggle for display mode
- **Copy / Paste / Swap** buttons for moving weights between bones
- Per-bone vertex count shown in parentheses

---

## Tips

- **Auto-normalize** (on by default) keeps total vertex weight at 1.0
- **Mirror X** paints symmetrically — great for armor and body meshes
- Hold `Shift` while painting for a quick smooth pass
- Use `[` and `]` to resize the brush without opening the panel
- The undo stack holds up to **50** operations
- Load a **reference body** before importing your mesh to enable
  one-click auto-skinning
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##weight_painter")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
