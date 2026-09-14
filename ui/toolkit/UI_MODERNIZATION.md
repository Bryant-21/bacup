# ModBox21 shared UI

ModBox21, its standalone executables, and BACUP use the modern components in
[`creation_lib.ui`](../../py_creation_lib/python/creation_lib/ui/README.md).
Existing theme selections, including light themes, still control the accent.

The toolkit keeps its standard menu navigation and context-sensitive top
toolbar. There is no left navigation bar. Selecting a workspace uses the
existing activation path, saves the selection, and retains the current workspace
if initialization fails. The full toolkit and standalone tools keep the editor
width available for their docked panels.
The bottom Idle/FPS bar is hidden; background idling remains enabled.

The setup wizard, preferences, About and theme dialogs, shared logs, and tool
forms use the same theme and font roles. Form labels wrap, path rows can put
Browse on a second line, and numeric fields keep their bounds and exact entry.
The NIF controls overlay measures its columns and scales its geometry with DPI.
Editor canvases use zero outer window padding so content meets the dock edges.
Help pages use a compact 8 logical-pixel inset that scales with the font. Controls
and settings retain the theme's internal spacing.
Help scrolling regions share the panel background so the inset stays unobtrusive.
Material Editor field labels use shared wrapping form rows, and its version,
game and type selectors scale with the fonts.

The Theme dialog is resizable, with a scrolling list of all 66 shared colors,
hex/RGBA entry, color pickers, tab-state samples and live controls. Color names
can be searched or filtered by category. Save persists per-theme overrides in
`shared_settings.json`; Cancel discards the draft. Color resets affect the draft
until it is saved. Overrides survive focus/docking changes and apply to separate
setup windows. This replaces the temporary HelloImGui tweaks that were overwritten
by the next frame's theme application.

Loading overlays now use the shared themed card with elapsed time, determinate
or animated progress, and recent stages where available. This replaces the
Builder's hard-coded blue bar and the older editor, archive and voice-browser
spinners. BACUP setup and cleanup use the same component; native progress bars
and overlays both follow the editable `plot_histogram` color.

## Removed code

- The classic appearance switch and duplicate rendering branches in the theme,
  shell and settings sections.
- The separate toolkit font-loading implementation. Every font role now has
  an independent fallback in the shared loader, including the terminal font.
- `ui.tools.imgui_helpers`, after moving its remaining generic fields to
  `creation_lib.ui.widgets.forms` and migrating all callers.
- UI test data and stale bytecode from the ModBox21 packaging data collection.

`AppVariant`, `SettingsContext`, `SettingsWindow`, `apply_theme`,
`apply_tab_style`, and `configure_runner_appearance` no longer accept or expose
an `appearance` selector. New hosts use the shared appearance directly.

Workspace implementations, dockable-window identities, native renderers,
settings migrations, and tool actions remain in place. The CLI distribution's
small Tkinter setup utility remains separate because that distribution does
not carry the ImGui application runtime.

## Verification

```powershell
uv run --no-sync python -m ui.toolkit
uv run --no-sync python -m ui.toolkit.tests.render_smoke --workspace nif --output tmp/modbox-nif.png
uv run --no-sync python -m ui.toolkit.tests.render_smoke --workspace materials --theme starfield --width 1280 --height 760 --output tmp/modbox-materials.png
uv run --no-sync python -m ui.toolkit.tests.render_smoke --screen settings-paths --scale 1.5 --width 1920 --height 1140 --output tmp/modbox-settings.png
uv run --no-sync python -m pytest ui/toolkit/tests py_creation_lib/python/creation_lib/ui/tests bacup/py_bacup_lib/python/bacup_ui -q
```

The renderer uses isolated settings, hidden native windows, and no conversion,
extraction, deployment, or editing actions. It supports all six `setup-N` steps,
`settings-general`, `settings-paths`, `settings-indexes`, `about`, `theme`, `help`, and
`--variant nif` for the standalone host.

On 2026-09-12, all 44 registered workspaces rendered successfully. This exposed
an existing LOD Generator error: `begin_tab_bar` returns a boolean in the
installed binding, so its old tuple unpack was corrected. Its 8 tests pass.
The combined toolkit/shared/BACUP suites have 409 passes and the four known
baseline mismatches documented in the [BACUP notes](../../bacup/docs/ui-modernization.md).

Additional native captures cover the setup steps in dark/light themes, Settings,
About, theme selection, standalone NIF, and 100%, 150%, and
200% scaling. BACUP's main screen and setup also render with the unified API.
These are UI smoke checks, not end-to-end tests of every editor operation or
live transitions between physical monitors. Artifacts are under
`tmp/modbox-ui-modern/` and are not tracked.

The ModBox21 PyInstaller specification builds successfully. A separate build
with a temporary runtime hook rendered the packaged application with standard
menu navigation and exited normally. The hook isolates settings, hides the
window, and captures a screenshot; it is not part of the shipping specification.
Bundled fonts are present, and deleted UI bytecode and test files are excluded.
