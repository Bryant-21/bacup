"""Help panel -- embedded user guide for the Cloth Maker."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# Cloth Maker

Create, edit, and preview Havok cloth physics (BSClothExtraData) on
Fallout 4 NIF meshes. Works with NIFs that already have cloth data
**and** bare NIFs that need cloth added from scratch.

---

## Getting Started

1. **Import a NIF** -- `File > Import NIF` or click the folder icon in
   the toolbar. The NIF can be a bare mesh (no cloth) or one that
   already contains `BSClothExtraData`.
2. **Add cloth (bare NIF)** -- use **Region** painting to mark which
   areas should simulate, or apply a **Template** preset.
3. **Edit cloth (existing)** -- explore the **Cloth Tree**, tweak
   **Parameters**, or use **Authoring** for full control.
4. **Preview & export** -- simulate in **Preview**, then
   `File > Export NIF`.

---

## Modes (Right Panels)

| Panel | Purpose |
|-------|---------|
| **Viewer** | Read-only inspection of the loaded cloth data |
| **Parameters** | Tweak global cloth values (stiffness, damping, gravity, iterations) |
| **Preview** | Live simulation preview with adjustable wind and time scale |
| **Templates** | Apply a pre-built cloth preset (e.g. cape, skirt, hair) to quickly set up physics |
| **Cloth Area** | Pick which mesh parts (BSTriShapes) the cloth area uses, and which shapes the brush should leave alone |
| **Region** | Paint triangles and pin vertices, pick topology/material, then generate cloth |
| **Authoring** | Full manual editing -- add/remove particles, constraints, capsules, and pins |

---

## Defining a New Cloth Area

Creating cloth from a bare NIF is a three step flow: pick which mesh
parts to paint on, paint the exact triangles and pin vertices, then
generate the simulation.

### 1. Cloth Area panel -- include/exclude shapes

Open the **Cloth Area** panel. It lists every BSTriShape in the NIF.

* **Include in Cloth Area** -- check the shapes that make up the cloth
  (e.g. the skirt), then click **Use as Cloth Area** to replace the
  current selection or **Add to Cloth Area** to append. This fills the
  triangle set that will be simulated.
* **Exclude from Painting** -- check shapes that should be *locked out*
  of the Region and Pin brush. Useful when a skirt shares a body with
  the torso trishape and you want to brush on the skirt without ever
  touching the torso. Excluded shapes are ignored by both the Region
  brush and the Pin brush.

### 2. Region panel -- fine-tune with the brush

Open the **Region** panel.

1. Check **Enable Brush**.
2. Pick **Region Brush** (paints triangles) or **Pin Brush** (paints
   fixed vertices that become anchor points).
3. Left-click + drag on the mesh to paint, hold **Shift** to erase.
4. Adjust **Radius** / **Strength** with the sliders.

The brush only touches triangles and vertices belonging to shapes that
are **not** excluded in the Cloth Area panel.

### 3. Generate constraints

Still in the **Region** panel:

1. Pick a **Topology** preset -- this decides which constraint types
   are generated:
   * `thin_cloth` -- single-layer fabric, light stretch + bend
   * `thick_cloth` -- double-sided stiffer fabric
   * `chain` -- heavy mass, loose bend (chain mail)
   * `skirt_flaps` -- vertical strips with strong stretch
   * `soft_body` -- volume-preserving all-around constraints
2. Pick a **Material** preset (Silk / Cotton / Denim / Leather / ...)
   which sets per-particle mass and overall stiffness.
3. Click **Generate Cloth**. The painted triangles become particles,
   the pin vertices become fixed particles, and the topology preset
   creates StandardLink / StretchLink / BendStiffness /
   LocalRangeConstraint sets for the Havok solver.

You can immediately preview the result in the **Preview** panel and
tweak global values in **Parameters**.

### Brushes are mutually exclusive

The **Region** brush (paint triangles / pins) and the **Authoring**
Vertex Painting brush (per-particle mass / pin) cannot both be active
at the same time. Enabling one automatically disables the other. This
avoids painting particles on stale cloth data while building a new
region.

---

## Viewport Controls

| Input | Action |
|-------|--------|
| `Alt` + LMB drag | Orbit camera |
| `Alt` + MMB drag | Pan camera |
| Scroll wheel | Zoom |

---

## Overlay Toggles (View Menu)

| Toggle | Shows |
|--------|-------|
| **Particles** | Cloth simulation particles as dots |
| **Constraints** | Links between particles (stretch / bend) |
| **Capsules** | Collision capsules attached to bones |
| **Pin Markers** | Fixed (pinned) particles that follow the skeleton |

All overlays can be toggled from the `View` menu while a cloth is
loaded.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+I` | Import NIF |
| `Ctrl+E` | Export NIF |
| `F1` | Toggle this help panel |

---

## Workflow Tips

- **New cloth from scratch** -- import a bare NIF (no cloth data),
  then use **Region** painting or a **Template** preset to generate
  the cloth setup.
- **Quick start** -- use the **Templates** panel to apply a cloth
  preset, then fine-tune in **Parameters**.
- **Custom shapes** -- use **Region** painting to mark which areas of
  the mesh should simulate, then let the generator build particles and
  constraints for those regions.
- **Full control** -- switch to **Authoring** mode to manually place
  particles, draw constraints, position collision capsules, and pin
  vertices to bones.
- **Iterate fast** -- the **Preview** panel runs a live simulation so
  you can see changes without launching the game.
- **Export** -- when satisfied, `File > Export NIF` writes the updated
  BSClothExtraData back into a NIF file ready for the game.
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##cloth_maker")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
