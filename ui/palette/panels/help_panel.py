"""Help panel — embedded user guide for the Palette Texture Generator."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# Palette Texture Generator

Generate Fallout 4 remap and gradient textures for color-swappable
armors, weapons, and power armor using the palette system.

---

## Quick Start

1. **Load a source texture** — the diffuse texture (`_d.dds`) of the target mesh
2. **Configure zones** — use Auto or Manual mode to define color regions
3. **Generate palette textures** — create the remap and gradient files
4. **Preview** — inspect the output in the preview pane
5. **Save** — export the remap (`_r.dds`) and gradient (`_g.dds`) textures

---

## Auto Mode

Automatic zone detection and palette generation:

- Analyzes the source texture's color distribution
- Clusters similar colors into remap zones automatically
- Adjustable **zone count** — how many distinct color regions to detect
- **Sensitivity** slider controls the clustering threshold
- Good for textures with clearly separated color areas

Click **Detect Zones** to run the analysis, then review and adjust
the results before generating.

---

## Manual Mode

Paint custom remap zones directly onto the texture:

- Select a **zone index** (0-15) from the zone list
- **Paint** on the texture preview to assign pixels to that zone
- **Brush size** and **tolerance** sliders control the painting tool
- **Flood fill** assigns an entire contiguous color region to a zone
- **Eraser** removes zone assignments (resets to unassigned)

Manual mode gives full control but requires more work. Use it when
auto-detection misses boundaries or groups colors incorrectly.

---

## Gradient Textures

Configure the gradient strip that defines color lookup:

| Setting | Description |
|---------|-------------|
| **Width** | Gradient resolution (default 256 pixels) |
| **Banded** | Flat color bands — sharp transitions between zones |
| **Smooth** | Blended gradient — soft transitions between zones |
| **Base Colors** | Starting color for each zone row |

The gradient texture maps zone indices to actual colors. Each row
corresponds to one zone, and the game samples across the row for
different color variants.

---

## Variant System

Create color variants from remap textures:

- A **variant** is a gradient with different colors per zone
- Add multiple variants to create a full color set
- **Preview** each variant applied to the source texture
- Variants share the same remap — only the gradient changes
- Common for power armor: chrome, military, rust, clean, etc.

---

## Output

The palette system produces two files:

| File | Purpose |
|------|---------|
| `_r.dds` | **Remap texture** — maps each pixel to a zone index (grayscale) |
| `_g.dds` | **Gradient texture** — color lookup table (one row per zone) |

Both files must be placed alongside the original diffuse texture
in the mod's texture folder.

---

## Tips

- Use the **debug preview** to visualize zone assignments — each zone
  is shown as a distinct false color on the mesh
- Start with **Auto Mode** and refine with **Manual Mode** for best results
- The remap texture should have clean zone boundaries — feathered edges
  cause color bleeding between zones in-game
- Keep zone count low (4-8) for most assets; power armor may need up to 12
- Test in-game with the paint station to verify zone assignments look correct
- Gradient width of 256 is standard — lower values save memory but reduce
  color precision
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##palette_ws")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
