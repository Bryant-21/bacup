"""Validation panel — run checks on loaded NIF and report issues."""

import logging

from imgui_bundle import hello_imgui, imgui

_log = logging.getLogger("nif_editor.validation")


# Severity levels
ERROR = "ERROR"
WARNING = "WARNING"
INFO = "INFO"

_SEVERITY_COLORS = {
    ERROR: (0.9, 0.3, 0.3, 1.0),
    WARNING: (0.9, 0.7, 0.2, 1.0),
    INFO: (0.5, 0.7, 0.9, 1.0),
}


class ValidationPanel:
    """imgui panel that validates NIF data and reports issues."""

    def __init__(self, app):
        self.app = app
        self._visible = False  # Hidden by default, shown via Tools > Validate
        self.window_name = "Validation"
        self._dock_space = "RightDock"
        self._needs_dock = True
        self._issues: list[tuple[str, int, str]] = []  # (severity, block_id, message)

    def show(self):
        self._visible = True
        self._needs_dock = True

    def _get_game_profile(self):
        """Get the active session's game profile, or None."""
        try:
            session = self.app.registry.active_session
            return getattr(session, 'game_profile', None)
        except (AttributeError, KeyError):
            return None

    def validate(self):
        """Run all validation checks."""
        self._issues.clear()

        nif = self.app.nif_file
        if not nif:
            return

        # Game-specific checks (run first — global context)
        profile = self._get_game_profile()
        if profile:
            self._check_bs_version(nif, profile)
            self._check_material_format(nif, profile)
            self._check_game_paths_configured(profile)
            self._check_texture_naming(nif, profile)

        # Shared generic checks used by the CLI as well.
        from creation_lib.nif.validation import validate_nif

        for issue in validate_nif(nif)["issues"]:
            self._issues.append((
                issue["severity"].upper(),
                issue["block"],
                issue["message"],
            ))

        self._visible = True

    def _apply_dock(self):
        """Dock into assigned dock space on first render or re-show."""
        if self._needs_dock:
            dp = hello_imgui.get_runner_params().docking_params
            dock_id = dp.dock_space_id_from_name(self._dock_space)
            if dock_id is not None:
                imgui.set_next_window_dock_id(dock_id)
            self._needs_dock = False

    def draw(self):
        """Draw the validation panel."""
        if not self._visible:
            return

        self._apply_dock()
        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return

        if imgui.button("Run Validation", imgui.ImVec2(150, 0)):
            self.validate()

        imgui.same_line()
        if imgui.button("Fix Issues", imgui.ImVec2(110, 0)):
            self.fix_issues()
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "Fixes invalid refs, duplicate names, degenerate triangles, "
                "and nonzero unnormalized normals."
            )

        imgui.same_line()
        count_e = sum(1 for s, _, _ in self._issues if s == ERROR)
        count_w = sum(1 for s, _, _ in self._issues if s == WARNING)
        count_i = sum(1 for s, _, _ in self._issues if s == INFO)
        imgui.text(f"E:{count_e}  W:{count_w}  I:{count_i}")

        imgui.separator()

        if not self._issues:
            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), "No issues found (run validation first)")
            imgui.end()
            return

        imgui.begin_child("validation_scroll", imgui.ImVec2(0, 0), imgui.ChildFlags_.borders.value)
        for severity, block_id, message in self._issues:
            color = _SEVERITY_COLORS.get(severity, (0.7, 0.7, 0.7, 1.0))

            # Format label: global issues (block_id=-1) don't show block number
            if block_id < 0:
                label = f"[{severity}] {message}"
            else:
                label = f"[{severity}] Block {block_id}: {message}"
            imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(*color))
            try:
                clicked, _ = imgui.selectable(label, False)
                if clicked:
                    if block_id >= 0 and hasattr(self.app, 'selection_mgr'):
                        self.app.selection_mgr.select_by_block_id(block_id)
            finally:
                imgui.pop_style_color()

        imgui.end_child()
        imgui.end()

    def fix_issues(self):
        nif = self.app.nif_file
        if not nif:
            return

        from creation_lib.nif.actions import SnapshotAction

        cmd = SnapshotAction(_description="Fix validation issues")
        cmd.capture_before(nif)
        fixed = self._fix_validation_issues(nif)
        if fixed:
            cmd.capture_after(nif)
            try:
                self.app.undo_manager.push(self.app.registry.active_id, cmd)
            except AttributeError:
                pass
            if hasattr(self.app, "_nif_dirty"):
                self.app._nif_dirty = True
            try:
                self.app.rebuild_scene_from_nif()
            except AttributeError:
                self.app.status_text = f"Fixed {fixed} validation issue(s)"
            except Exception as exc:
                _log.exception("rebuild_scene_from_nif failed after validation fixes")
                self.app.status_text = f"Fixed {fixed} validation issue(s); reload error: {exc}"
            else:
                self.app.status_text = f"Fixed {fixed} validation issue(s)"
        else:
            self.app.status_text = "No automatically fixable validation issues"

        self.validate()

    def _fix_validation_issues(self, nif) -> int:
        """Fix validation issues that do not require guessing asset intent."""
        from creation_lib.nif.validation import fix_validation_issues

        return fix_validation_issues(nif)

    def _check_external_geometry(self, nif):
        """Compatibility wrapper for tests and direct callers."""
        from creation_lib.nif.validation import validate_external_geometry

        for issue in validate_external_geometry(nif):
            self._issues.append(issue.as_tuple())

    # -------------------------------------------------------------------
    # Game-specific validation checks
    # -------------------------------------------------------------------

    def _check_bs_version(self, nif, profile):
        """Check that BS version matches the selected game profile."""
        bs_version = getattr(nif.header, 'bs_version', None)
        if bs_version is None:
            return
        lo, hi = profile.bs_version_range
        if not (lo <= bs_version <= hi):
            self._issues.append((
                WARNING, -1,
                f"BS version {bs_version} does not match {profile.display_name} "
                f"(expected {lo}-{hi}). Game may be misdetected."
            ))

    def _check_material_format(self, nif, profile):
        """Check that material references match the expected format for the game."""
        expected = profile.material_format  # "bgsm" or "mat"
        for block in nif.blocks:
            if not nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
                continue
            sp_ref = block.get_field("Shader Property")
            ref_id = sp_ref if isinstance(sp_ref, int) else -1
            if ref_id < 0:
                continue
            try:
                shader_prop = nif.get_block(ref_id)
            except (IndexError, KeyError):
                continue
            if not shader_prop:
                continue
            mat_name = shader_prop.get_field("Name") or ""
            if isinstance(mat_name, list):
                mat_name = "".join(str(c) for c in mat_name)
            mat_name = mat_name.lower().rstrip("\x00")
            if not mat_name:
                continue
            # Check for format mismatch
            if expected == "mat" and (mat_name.endswith(".bgsm") or mat_name.endswith(".bgem")):
                self._issues.append((
                    WARNING, block.block_id,
                    f"BGSM/BGEM material on {profile.display_name} NIF "
                    f"(expected .mat): {mat_name}"
                ))
            elif expected == "bgsm" and mat_name.endswith(".mat"):
                self._issues.append((
                    WARNING, block.block_id,
                    f"Starfield .mat material on {profile.display_name} NIF "
                    f"(expected .bgsm/.bgem): {mat_name}"
                ))

    def _check_game_paths_configured(self, profile):
        """Warn if the game's paths are not configured in settings."""
        try:
            settings = self.app.settings
            game_paths = settings.get_game_paths(profile.id)
            has_any = any(
                v for k, v in game_paths.items()
                if k in ("root_dir", "extracted_dir") and v
            )
            if not has_any:
                self._issues.append((
                    INFO, -1,
                    f"{profile.display_name} paths not configured "
                    f"-- textures unavailable. Configure in Settings > Paths."
                ))
        except (AttributeError, TypeError):
            pass  # Settings not available

    def _check_texture_naming(self, nif, profile):
        """Check texture naming conventions for FO76 (spec-gloss textures on metallic-roughness game)."""
        if profile.material_model != "metallic-roughness":
            return
        for block in nif.blocks:
            if not nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
                continue
            sp_ref = block.get_field("Shader Property")
            ref_id = sp_ref if isinstance(sp_ref, int) else -1
            if ref_id < 0:
                continue
            try:
                shader_prop = nif.get_block(ref_id)
            except (IndexError, KeyError):
                continue
            if not shader_prop:
                continue
            # Check texture set for spec-gloss naming on metallic-roughness game
            tex_set_ref = shader_prop.get_field("Texture Set")
            if tex_set_ref is None or not isinstance(tex_set_ref, int) or tex_set_ref < 0:
                continue
            try:
                tex_set = nif.get_block(tex_set_ref)
            except (IndexError, KeyError):
                continue
            if not tex_set:
                continue
            textures = tex_set.get_field("Textures") or []
            # Slot 7 is specular in FO4 — if it ends with _s.dds on FO76, warn
            if len(textures) > 7:
                spec_path = (textures[7] or "").lower()
                if spec_path.endswith("_s.dds"):
                    self._issues.append((
                        WARNING, block.block_id,
                        f"FO4-style specular texture (*_s.dds) on "
                        f"{profile.display_name} NIF: {textures[7]}"
                    ))
