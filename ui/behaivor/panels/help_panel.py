"""Help panel — embedded user guide for the Behavior Graph Editor."""
from __future__ import annotations

from imgui_bundle import imgui, imgui_md

_GUIDE = r"""
# Behavior Graph Editor

Create and edit Havok behavior graphs (.hkx) for Bethesda games using
a visual node-based editor with import/export support.

---

## Quick Start

1. **Create a new graph** — `File > New` or open an existing XML behavior
2. **Add nodes** — drag from the Node Palette onto the canvas
3. **Connect nodes** — drag from an output pin to an input pin
4. **Edit properties** — select a node and configure it in the Properties panel
5. **Export** — save as XML, then pack to `.hkx` for in-game use

---

## Node Palette

Browse available node types organized by category:

| Category | Node Types |
|----------|------------|
| **State Machines** | `hkbStateMachine`, `hkbStateMachineStateInfo` |
| **Blenders** | `hkbBlenderGenerator`, `BSBoneSwitchGenerator` |
| **Clips** | `hkbClipGenerator`, `hkbClipTrigger` |
| **Modifiers** | `hkbModifierList`, `hkbEventDrivenModifier` |
| **Transitions** | `hkbBlendingTransitionEffect`, `hkbManualSelectorGenerator` |
| **Expressions** | `hkbExpressionCondition`, `BSComputeAddBoneAnimModifier` |
| **Generators** | `hkbBehaviorReferenceGenerator`, `hkbPoseMatchingGenerator` |

Drag a node from the palette to the canvas, or double-click to add it
at the center of the current view.

---

## Graph Canvas

The visual editing area for your behavior graph:

- **Drag nodes** — LMB drag on the node header to reposition
- **Connect** — LMB drag from an output pin to an input pin
- **Disconnect** — right-click a connection and select Delete
- **Select multiple** — LMB drag a selection box, or Shift+click
- **Pan** — MMB drag or hold Space+LMB drag
- **Zoom** — scroll wheel
- **Frame All** — `A` to fit the entire graph in view
- **Frame Selected** — `S` to center on selected nodes

---

## Properties Panel

Edit the selected node's configuration:

- **Name** — display name for the node
- **Type-specific fields** — each node type has its own properties
- **Bindings** — variable bindings for dynamic control
- **Events** — event triggers and listeners

Common property types:
- Numeric fields (blend weights, durations, thresholds)
- Dropdown selectors (blend mode, transition type)
- Reference fields (link to other nodes, clips, variables)
- Boolean toggles (enable/disable features)

---

## State Machines

The core organizational structure for behavior graphs:

- **States** — each state contains a generator (clip, blender, or nested state machine)
- **Transitions** — define how states connect with conditions and blend settings
- **Wildcards** — transitions that can trigger from any state
- **Nested** — state machines can contain other state machines for complex behaviors

To create a state machine:
1. Add an `hkbStateMachine` node
2. Add `hkbStateMachineStateInfo` nodes for each state
3. Connect generators to each state
4. Define transitions between states with conditions

---

## Clip Generators

Reference animation files for playback:

- **Animation Path** — path to the `.hkx` animation clip
- **Playback Speed** — animation speed multiplier
- **Crop Start / End** — trim the clip to a sub-range
- **Triggers** — fire events at specific frame times
- **Loop** — whether the clip repeats

---

## Blenders

Mix multiple animations together:

| Type | Description |
|------|-------------|
| **Lerp** | Linear interpolation between two generators |
| **Additive** | Layer one animation on top of another |
| **Ragdoll** | Blend between animation and physics |
| **Bone Switch** | Use different generators for different bones |

Configure blend weights via the Properties panel or bind them
to behavior variables for runtime control.

---

## File Menu

| Action | Description |
|--------|-------------|
| **New** | Create an empty behavior graph |
| **Open XML** | Import a behavior from XML format |
| **Save XML** | Export the graph as editable XML |
| **Export HKX** | Pack to binary `.hkx` via creation_lib.hkxpack (pure Python) |
| **Save Project** | Save the full project (graph + layout) |

---

## Behavior Browser

Search indexed behavior files from extracted game data:

- Browse by category (characters, creatures, weapons)
- Preview the node structure before importing
- Import individual nodes or entire sub-graphs
- Study vanilla behaviors to understand game patterns

Access via the Behavior Browser panel or `Ctrl+Shift+B`.

---

## Tips

- **Study vanilla first** — use the Behavior Browser to examine how
  the base game structures its behavior graphs before building custom ones
- **Start simple** — begin with a single state machine and two states,
  then expand incrementally
- **Name everything** — descriptive node names make complex graphs navigable
- **Test incrementally** — export and test after each major change rather
  than building the entire graph before testing
- **Events are global** — event names must match exactly between the
  behavior graph, animation annotations, and Papyrus scripts
- Use **Ctrl+Z / Ctrl+Y** to undo/redo graph edits
"""
USER_GUIDE_MARKDOWN = _GUIDE


class HelpPanel:
    """Dockable user guide panel with markdown rendering."""

    def draw(self):
        visible, _ = imgui.begin("Help##behavior")
        if not visible:
            imgui.end()
            return

        imgui.begin_child("##help_scroll")
        imgui_md.render(_GUIDE)
        imgui.end_child()

        imgui.end()
