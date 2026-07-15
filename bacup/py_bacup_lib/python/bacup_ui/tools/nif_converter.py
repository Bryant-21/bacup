"""Bulk NIF Converter — batch convert NIF files between game formats or to FBX."""

from __future__ import annotations

import logging
import os
from dataclasses import dataclass
from pathlib import Path

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import (
    begin_form,
    draw_combo_field,
    draw_path_row,
    draw_run_cancel_buttons,
    end_form,
)

_log = logging.getLogger("tools.bulk_nif_converter")

# (display label, game_id or None for FBX)
_OUTPUT_FORMATS: list[tuple[str, str | None]] = [
    ("FBX",                      None),
    ("NIF — Fallout 4",          "fo4"),
]
_FORMAT_LABELS: list[str] = [f[0] for f in _OUTPUT_FORMATS]


@dataclass
class _FileResult:
    rel_path: str
    source_game: str  # display name or "unknown" or "—"
    status: str       # "ok" | "warn" | "error" | "skipped"
    note: str         # e.g. "2 changes", "[same game]", "[unknown game]"


# ---------------------------------------------------------------------------
# Pure helpers (extracted for unit testing)
# ---------------------------------------------------------------------------

def _resolve_output_path(
    rel_dir: str | None,
    filename: str,
    out_dir: str,
    ext: str,
) -> str:
    """Build the output file path, preserving relative subfolder structure.

    Args:
        rel_dir: Relative subdirectory from source root (None or "" for root).
        filename: Source filename (e.g. "weapon.nif").
        out_dir: Absolute output directory.
        ext: Target extension including dot (e.g. ".nif" or ".fbx").

    Returns:
        Absolute output path.
    """
    base = os.path.splitext(filename)[0] + ext
    if rel_dir:
        # Normalize rel_dir so forward-slash paths work on Windows too
        return os.path.normpath(os.path.join(out_dir, rel_dir, base))
    return os.path.normpath(os.path.join(out_dir, base))


def _auto_output_dir(src_dir: str) -> str:
    """Generate a default output directory as a sibling of the source folder.

    Example: /path/to/nifs  ->  /path/to/nifs_converted
    """
    normalized = os.path.normpath(src_dir)
    parent = os.path.dirname(normalized)
    name = os.path.basename(normalized)
    return os.path.join(parent, name + "_converted")


def _find_texture_sibling_file(
    src: str,
    filename: str,
    role: str,
    target_role: str,
    source_profile,
) -> Path | None:
    current_suffix = source_profile.texture_suffixes.get(role)
    target_suffix = source_profile.texture_suffixes.get(target_role)
    if not current_suffix or not target_suffix:
        return None

    stem, ext = os.path.splitext(filename)
    idx = stem.lower().rfind(current_suffix.lower())
    if idx < 0:
        return None

    sibling_name = stem[:idx] + target_suffix + ext
    sibling = Path(src).with_name(sibling_name)
    return sibling if sibling.is_file() else None


def _find_mod_root(src_dir: str) -> str | None:
    """Walk up from src_dir to find the folder that contains a 'Meshes' subfolder."""
    current = os.path.normpath(os.path.abspath(src_dir))
    seen: set[str] = set()
    while current not in seen:
        seen.add(current)
        try:
            entries = os.listdir(current)
        except OSError:
            break
        if any(e.lower() == "meshes" for e in entries):
            return current
        parent = os.path.dirname(current)
        if parent == current:
            break
        current = parent
    return None


def _find_subfolder(parent: str, name: str) -> str | None:
    """Case-insensitive immediate-child folder lookup."""
    try:
        for entry in os.listdir(parent):
            if entry.lower() == name.lower() and os.path.isdir(os.path.join(parent, entry)):
                return os.path.join(parent, entry)
    except OSError:
        pass
    return None


def _nif_texture_paths(nif) -> list[str]:
    """Collect unique, non-empty texture paths from all BSShaderTextureSet blocks."""
    paths: list[str] = []
    seen: set[str] = set()
    for block in nif.blocks:
        if block.type_name != "BSShaderTextureSet":
            continue
        textures = block.get_field("Textures")
        if not textures or not isinstance(textures, list):
            continue
        for tex in textures:
            if tex and isinstance(tex, str):
                normalized = tex.replace("\\", "/").strip().rstrip("\x00")
                if normalized and normalized not in seen:
                    seen.add(normalized)
                    paths.append(normalized)
    return paths


def _nif_material_paths(nif) -> list[str]:
    """Collect unique .bgsm/.bgem paths from shader property Name fields.

    Absolute build-server prefixes (e.g. ``C:\\Projects\\76\\Build\\PC\\Data\\``)
    are stripped before deduping so the returned paths are Data-relative and
    resolvable against the mod root on disk.
    """
    from creation_lib.nif.path_utils import normalize_material_path

    paths: list[str] = []
    seen: set[str] = set()
    for block in nif.blocks:
        if block.type_name not in ("BSLightingShaderProperty", "BSEffectShaderProperty"):
            continue
        name = block.get_field("Name") or ""
        if isinstance(name, list):
            name = "".join(str(c) for c in name)
        name = normalize_material_path(name).replace("\\", "/")
        if os.path.splitext(name)[1].lower() in (".bgsm", ".bgem") and name not in seen:
            seen.add(name)
            paths.append(name)
    return paths


def _export_texture_file(
    tex_path: str,
    mod_root: str,
    out_dir: str,
    source_profile,
    target_profile,
) -> tuple[str, str]:
    """Resolve, convert, and export one texture to out_dir/Data/<tex_path>.

    Args:
        tex_path: Data-relative path with forward slashes, e.g. "Textures/Weapons/gun_d.dds".
        mod_root: Root folder containing Textures/, Materials/, Meshes/ siblings.
        out_dir: Conversion output root (Data/ subfolders will be created inside).
        source_profile: Source game profile.
        target_profile: Target game profile.

    Returns:
        (status, note) where status is "ok" / "warn" / "error".
    """
    import shutil
    from creation_lib.textures.naming import detect_texture_role, convert_texture_name

    src = os.path.normpath(os.path.join(mod_root, tex_path.replace("/", os.sep)))
    if not os.path.isfile(src):
        return "warn", "not found"

    filename = tex_path.split("/")[-1]
    dir_part = tex_path[: tex_path.rfind(filename)].rstrip("/")  # e.g. "Textures/Weapons/Gun"

    if source_profile.id == target_profile.id:
        out_path = os.path.normpath(os.path.join(out_dir, "Data", tex_path.replace("/", os.sep)))
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        shutil.copy2(src, out_path)
        return "ok", "copied"

    try:
        from bacup_lib.texture.native import convert_texture_paths

        role = detect_texture_role(filename, source_profile)
        native_out_dir = Path(out_dir) / "Data" / dir_part.replace("/", os.sep)
        if source_profile.id == "fo76" and target_profile.id == "fo4":
            if role == "lighting":
                reflectivity = _find_texture_sibling_file(
                    src, filename, role, "reflectivity", source_profile
                )
                if reflectivity is not None:
                    return "ok", "merged with reflectivity sibling"
            elif role == "reflectivity":
                lighting = _find_texture_sibling_file(
                    src, filename, role, "lighting", source_profile
                )
                if lighting is not None:
                    result = convert_texture_paths(
                        [(Path(src), "reflectivity"), (lighting, "lighting")],
                        native_out_dir,
                        source_profile,
                        target_profile,
                    )
                    if result.get("converted"):
                        return "ok", "converted"

        result = convert_texture_paths(
            [(Path(src), role)],
            native_out_dir,
            source_profile,
            target_profile,
        )
        if result.get("converted"):
            return "ok", "converted"
    except Exception as exc:
        if "No converter" not in str(exc):
            return "error", str(exc)
        # No channel converter for this pair — rename suffix only and copy bytes.
        pass

    try:
        new_filename = convert_texture_name(filename, source_profile, target_profile)
        out_path = os.path.normpath(
            os.path.join(out_dir, "Data", dir_part.replace("/", os.sep), new_filename)
        )
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        shutil.copy2(src, out_path)
        return "ok", f"renamed→{new_filename}"
    except Exception as exc:
        return "error", str(exc)


# BGSM texture field names that may contain Data-relative texture paths.
_BGSM_TEX_FIELDS = [
    "DiffuseTexture", "NormalTexture", "SmoothSpecTexture", "GreyscaleTexture",
    "EnvmapTexture", "GlowTexture", "InnerLayerTexture", "WrinklesTexture",
    "DisplacementTexture", "SpecularTexture", "LightingTexture", "FlowTexture",
    "DistanceFieldAlphaTexture",
]
_BGEM_TEX_FIELDS = [
    "BaseTexture", "GrayscaleTexture", "EnvmapTexture", "NormalTexture",
    "EnvmapMaskTexture", "SpecularTexture", "LightingTexture", "GlowTexture",
]


def _convert_to_mat(
    src: str,
    ext: str,
    out_path: str,
    source_profile,
    target_profile,
) -> tuple[str, str, list[str]]:
    """Convert a BGSM or BGEM file at *src* to a Starfield .mat JSON at *out_path*.

    Returns (status, note, tex_paths_inside).
    """
    from creation_lib.textures.naming import convert_texture_name
    from creation_lib.material_tools.mat_writer import bgsm_to_mat, bgem_to_mat, write_mat

    mat_out = os.path.splitext(out_path)[0] + ".mat"
    os.makedirs(os.path.dirname(mat_out), exist_ok=True)

    try:
        if ext == ".bgsm":
            from creation_lib.material_tools.bgsm_bin import read_bgsm
            with open(src, "rb") as f:
                mat_data = read_bgsm(f)
            obj = bgsm_to_mat(mat_data, source_profile, target_profile)
            tex_fields = _BGSM_TEX_FIELDS
            mat_obj = mat_data
        elif ext == ".bgem":
            from creation_lib.material_tools.bgem_bin import read_bgem
            with open(src, "rb") as f:
                mat_data = read_bgem(f)
            obj = bgem_to_mat(mat_data, source_profile, target_profile)
            tex_fields = _BGEM_TEX_FIELDS
            mat_obj = mat_data
        else:
            import shutil
            shutil.copy2(src, out_path)
            return "ok", "copied", []

        write_mat(obj, mat_out)

        # Collect texture paths found in the source material for downstream export.
        tex_paths: list[str] = []
        for field in tex_fields:
            val = getattr(mat_obj, field, None)
            if val and isinstance(val, str) and val.strip():
                tex_paths.append(val.replace("\\", "/").strip())

        return "ok", f"→ .mat ({len(tex_paths)} textures)", tex_paths

    except Exception as exc:
        return "error", str(exc), []


def _export_material_file(
    mat_path: str,
    mod_root: str,
    out_dir: str,
    source_profile,
    target_profile,
) -> tuple[str, str, list[str]]:
    """Resolve, convert, and export one BGSM/BGEM file.

    Also renames texture paths stored inside the material to match the target game.

    Returns:
        (status, note, tex_paths) where tex_paths are Data-relative texture paths
        found inside the material (so the caller can export them too).
    """
    import shutil
    from creation_lib.textures.naming import convert_texture_name

    src = os.path.normpath(os.path.join(mod_root, mat_path.replace("/", os.sep)))
    if not os.path.isfile(src):
        return "warn", "not found", []

    ext = os.path.splitext(mat_path)[1].lower()
    out_path = os.path.normpath(os.path.join(out_dir, "Data", mat_path.replace("/", os.sep)))
    os.makedirs(os.path.dirname(out_path), exist_ok=True)

    # Starfield .mat: convert source BGSM/BGEM → JSON .mat.
    # Source .mat (Starfield) → non-mat target: copy as-is (no reverse converter yet).
    if target_profile.material_format == "mat" and source_profile.material_format != "mat":
        return _convert_to_mat(src, ext, out_path, source_profile, target_profile)
    if source_profile.material_format == "mat":
        shutil.copy2(src, out_path)
        return "warn", "copied (.mat→bgsm not supported)", []

    def _rename_tex_fields(mat_obj, fields: list[str]) -> list[str]:
        found: list[str] = []
        for field in fields:
            val = getattr(mat_obj, field, None)
            if not val or not isinstance(val, str) or not val.strip():
                continue
            normalized = val.replace("\\", "/").strip()
            fname = normalized.split("/")[-1]
            new_name = convert_texture_name(fname, source_profile, target_profile)
            if new_name != fname:
                dir_prefix = normalized[: normalized.rfind(fname)]
                setattr(mat_obj, field, dir_prefix + new_name)
            found.append(normalized)
        return found

    try:
        if ext == ".bgsm":
            from creation_lib.material_tools.bgsm_bin import read_bgsm
            from creation_lib.material_tools.convert import downgrade_bgsm, BGSM_VERSION_FO4

            with open(src, "rb") as f:
                mat = read_bgsm(f)
            mat = downgrade_bgsm(mat, BGSM_VERSION_FO4, source_path=mat_path)
            tex_paths = _rename_tex_fields(mat, _BGSM_TEX_FIELDS)
            with open(out_path, "wb") as f:
                mat.write(f)
            return "ok", "converted", tex_paths

        elif ext == ".bgem":
            from creation_lib.material_tools.bgem_bin import read_bgem
            from creation_lib.material_tools.convert import downgrade_bgem, BGEM_VERSION_FO4

            with open(src, "rb") as f:
                mat = read_bgem(f)
            mat = downgrade_bgem(mat, BGEM_VERSION_FO4)
            tex_paths = _rename_tex_fields(mat, _BGEM_TEX_FIELDS)
            with open(out_path, "wb") as f:
                mat.write(f)
            return "ok", "converted", tex_paths

        else:
            shutil.copy2(src, out_path)
            return "ok", "copied", []

    except Exception as exc:
        return "error", str(exc), []


class NifConverterTool(BaseTool):
    name = "Bulk NIF Converter"
    tool_id = "bulk_nif_converter"
    description = "Batch convert NIF files between game formats or to FBX"
    category = "NIF"

    def __init__(self):
        super().__init__()
        self._src_dir = ""
        self._out_dir = ""
        self._format_idx = 1   # default: NIF — Fallout 4
        self._include_subdirs = True
        self._skip_existing = True
        self._organize_mod_structure = False
        self._results: list[_FileResult] = []
        self._summary = ""

    # ------------------------------------------------------------------
    # Settings persistence
    # ------------------------------------------------------------------

    def get_default_settings(self) -> dict:
        return {
            "source_folder": "",
            "output_folder": "",
            "output_format": 1,
            "include_subdirs": True,
            "skip_existing": True,
            "organize_mod_structure": False,
        }

    def apply_settings(self, settings: dict) -> None:
        self._src_dir = settings.get("source_folder", "")
        self._out_dir = settings.get("output_folder", "")
        self._format_idx = settings.get("output_format", 1)
        self._include_subdirs = settings.get("include_subdirs", True)
        self._skip_existing = settings.get("skip_existing", True)
        self._organize_mod_structure = settings.get("organize_mod_structure", False)

    def collect_settings(self) -> dict:
        return {
            "source_folder": self._src_dir,
            "output_folder": self._out_dir,
            "output_format": self._format_idx,
            "include_subdirs": self._include_subdirs,
            "skip_existing": self._skip_existing,
            "organize_mod_structure": self._organize_mod_structure,
        }

    # ------------------------------------------------------------------
    # UI
    # ------------------------------------------------------------------

    def draw_content(self) -> None:
        # --- Form ---
        if begin_form("##nif_converter_form"):
            _, clicked = draw_path_row("Source Folder", self._src_dir)
            if clicked:
                path = pick_folder("Select folder with NIF files")
                if path:
                    self._src_dir = path

            display_out = self._out_dir or "(auto: <source folder>_converted)"
            _, clicked = draw_path_row("Output Folder", display_out)
            if clicked:
                path = pick_folder("Select output folder (optional)")
                if path:
                    self._out_dir = path

            changed, new_idx = draw_combo_field("Output Format", _FORMAT_LABELS, self._format_idx)
            if changed:
                self._format_idx = new_idx

            end_form()

        # --- Options ---
        imgui.spacing()
        _, self._include_subdirs = imgui.checkbox(
            "Include subdirectories", self._include_subdirs
        )
        imgui.same_line()
        _, self._skip_existing = imgui.checkbox(
            "Skip existing files", self._skip_existing
        )

        imgui.spacing()
        _, self._organize_mod_structure = imgui.checkbox(
            "Organize as mod structure (Data/Meshes, Data/Textures, Data/Materials) + convert assets",
            self._organize_mod_structure,
        )
        if self._organize_mod_structure and self._src_dir:
            mod_root = _find_mod_root(self._src_dir)
            if mod_root is None:
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.5, 0.0, 1.0))
                imgui.text(
                    "  Warning: no Meshes/ folder found above source — mod root cannot be detected."
                )
                imgui.pop_style_color()
            else:
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.5, 0.8, 0.5, 1.0))
                imgui.text(f"  Mod root: {mod_root}")
                imgui.pop_style_color()

        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        # --- Run / Cancel ---
        can_run = bool(self._src_dir) and os.path.isdir(self._src_dir)
        run_clicked, cancel_clicked = draw_run_cancel_buttons(self._running, can_run)

        if run_clicked:
            self._results = []
            self._summary = ""
            self._start_batch(self._run_task)
        if cancel_clicked:
            self._cancel_requested = True

        if self._src_dir and not can_run:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.5, 0.0, 1.0))
            imgui.text("Source folder does not exist.")
            imgui.pop_style_color()

        # --- Summary line ---
        if self._summary:
            imgui.spacing()
            imgui.text(self._summary)

        # --- Per-file log ---
        self._draw_log()

    def _draw_log(self) -> None:
        """Render the scrollable per-file results table."""
        if not self._results:
            return

        imgui.spacing()
        imgui.separator()
        imgui.text("Per-file log:")

        imgui.begin_child(
            "##nif_converter_log",
            imgui.ImVec2(0, 200),
            imgui.ChildFlags_.border,
        )

        flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.sizing_stretch_prop
        if imgui.begin_table("##log_table", 4, flags):
            imgui.table_setup_column("##icon",   imgui.TableColumnFlags_.width_fixed,   20)
            imgui.table_setup_column("Path",     imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("Type",     imgui.TableColumnFlags_.width_fixed,   130)
            imgui.table_setup_column("Note",     imgui.TableColumnFlags_.width_fixed,   200)
            imgui.table_headers_row()

            for r in self._results:
                imgui.table_next_row()

                imgui.table_set_column_index(0)
                if r.status == "ok":
                    imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.3, 1.0, 0.3, 1.0))
                    imgui.text_unformatted("v")
                    imgui.pop_style_color()
                elif r.status == "warn":
                    imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.8, 0.0, 1.0))
                    imgui.text_unformatted("!")
                    imgui.pop_style_color()
                elif r.status == "error":
                    imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.3, 0.3, 1.0))
                    imgui.text_unformatted("X")
                    imgui.pop_style_color()
                else:
                    imgui.text_unformatted("-")

                imgui.table_set_column_index(1)
                imgui.text_unformatted(r.rel_path)

                imgui.table_set_column_index(2)
                imgui.text_unformatted(r.source_game)

                imgui.table_set_column_index(3)
                imgui.text_unformatted(r.note)

            imgui.end_table()

        imgui.end_child()

    # ------------------------------------------------------------------
    # Worker
    # ------------------------------------------------------------------

    def _run_task(self) -> None:
        from creation_lib.nif import NifFile
        from creation_lib.nif import native_runtime as nif_native_runtime
        from creation_lib.core.game_profiles import get_profile

        _label, target_game_id = _OUTPUT_FORMATS[self._format_idx]
        is_fbx = target_game_id is None

        out_dir = self._out_dir or _auto_output_dir(self._src_dir)

        # Check FBX SDK availability before starting any work
        export_nif_to_fbx = None
        FbxExportOptions = None
        if is_fbx:
            try:
                from creation_lib.fbx import export_nif_to_fbx, FbxExportOptions
            except ImportError:
                self._error_msg = (
                    "FBX export not available. "
                    "Install Autodesk FBX SDK Python bindings."
                )
                return

        # Mod-structure mode setup
        mod_root: str | None = None
        meshes_root: str | None = None
        if self._organize_mod_structure and not is_fbx:
            mod_root = _find_mod_root(self._src_dir)
            if mod_root is None:
                self._error_msg = (
                    "Organize as mod structure: cannot detect mod root "
                    "(no Meshes/ folder found above source folder)."
                )
                return
            meshes_root = _find_subfolder(mod_root, "Meshes")
            if meshes_root is None:
                # src_dir is the meshes root itself (or a child where Meshes wasn't found
                # as a direct child — fall back gracefully)
                meshes_root = mod_root

        tasks = self.collect_files(
            self._src_dir,
            lambda p: p.lower().endswith(".nif"),
            include_subdirs=self._include_subdirs,
        )

        if not tasks:
            self._error_msg = "No NIF files found in source folder."
            return

        try:
            os.makedirs(out_dir, exist_ok=True)
        except OSError as exc:
            self._error_msg = f"Cannot create output folder: {exc}"
            return

        results: list[_FileResult] = []
        # n_warnings counts files that converted successfully but produced warnings
        # (these are also counted in n_converted — the categories are not mutually exclusive)
        n_converted = n_warnings = n_errors = n_skipped = 0
        # Track already-exported assets to avoid duplicate work across NIFs.
        copied_assets: set[str] = set()

        for i, (abs_path, rel_dir) in enumerate(tasks):
            if self._cancel_requested:
                break

            filename = os.path.basename(abs_path)
            rel_path = os.path.join(rel_dir, filename) if rel_dir else filename
            self._on_progress(i, len(tasks), f"Converting {filename}")

            ext = ".fbx" if is_fbx else ".nif"

            if self._organize_mod_structure and not is_fbx and meshes_root:
                # Place NIF under Data/Meshes/ preserving path relative to meshes_root.
                abs_dir = os.path.dirname(abs_path)
                try:
                    rel_from_meshes = os.path.relpath(abs_dir, meshes_root)
                except ValueError:
                    rel_from_meshes = rel_dir or ""
                out_path = os.path.normpath(
                    os.path.join(out_dir, "Data", "Meshes", rel_from_meshes, filename)
                )
            else:
                out_path = _resolve_output_path(rel_dir, filename, out_dir, ext)

            # Skip existing
            if self._skip_existing and os.path.exists(out_path):
                results.append(_FileResult(rel_path, "—", "skipped", "[skipped]"))
                n_skipped += 1
                continue

            # Load NIF
            try:
                nif = NifFile.load(abs_path)
            except Exception as exc:
                results.append(_FileResult(rel_path, "—", "error", str(exc)))
                n_errors += 1
                _log.warning("Failed to load %s: %s", abs_path, exc)
                continue

            source_profile = nif.detected_game
            source_name = source_profile.display_name if source_profile else "unknown"

            # Ensure output subdirectory exists
            out_subdir = os.path.dirname(out_path)
            if out_subdir:
                os.makedirs(out_subdir, exist_ok=True)

            if is_fbx:
                try:
                    ok = export_nif_to_fbx(nif, out_path, FbxExportOptions())
                    if ok:
                        results.append(_FileResult(rel_path, source_name, "ok", "exported"))
                        n_converted += 1
                    else:
                        results.append(_FileResult(rel_path, source_name, "error", "export failed"))
                        n_errors += 1
                except Exception as exc:
                    results.append(_FileResult(rel_path, source_name, "error", str(exc)))
                    n_errors += 1
                    _log.warning("FBX export failed for %s: %s", abs_path, exc)

            else:
                # NIF → NIF
                if source_profile is None:
                    results.append(_FileResult(rel_path, "unknown", "skipped", "[unknown game]"))
                    n_skipped += 1
                    continue

                if source_profile.id == target_game_id:
                    results.append(_FileResult(rel_path, source_name, "skipped", "[same game]"))
                    n_skipped += 1
                    continue

                target_profile = get_profile(target_game_id)
                try:
                    report = nif_native_runtime.convert_nif_file_raw(
                        abs_path,
                        out_path,
                        source_profile.id,
                        target_profile.id,
                        None,
                        {},
                    )
                    if report.get("supported"):
                        warnings = report.get("warnings", []) or []
                        changes = report.get("changes", []) or []
                        if warnings:
                            note = f"{len(changes)} changes, {len(warnings)} warnings"
                            results.append(_FileResult(rel_path, source_name, "warn", note))
                            n_warnings += 1
                        else:
                            note = f"{len(changes)} changes"
                            results.append(_FileResult(rel_path, source_name, "ok", note))
                        n_converted += 1

                        # Asset collection — scan the ORIGINAL nif for referenced paths.
                        if self._organize_mod_structure and mod_root:
                            self._collect_assets(
                                nif, mod_root, out_dir,
                                source_profile, target_profile,
                                results, copied_assets,
                            )
                    else:
                        errors = report.get("errors", []) or ["conversion failed"]
                        err = str(errors[0])
                        results.append(_FileResult(rel_path, source_name, "error", err))
                        n_errors += 1
                except Exception as exc:
                    results.append(_FileResult(rel_path, source_name, "error", str(exc)))
                    n_errors += 1
                    _log.warning("Conversion failed for %s: %s", abs_path, exc)

        self._results = results
        self._on_progress(len(tasks), len(tasks), "Done")

        if n_warnings:
            converted_str = f"{n_converted} converted ({n_warnings} with warnings)"
        else:
            converted_str = f"{n_converted} converted"
        self._summary = f"{converted_str}  {n_errors} errors  {n_skipped} skipped"
        if self._cancel_requested:
            self._result_msg = f"Cancelled — {len(results)}/{len(tasks)} processed.  {self._summary}"
        else:
            self._result_msg = f"Complete — {len(tasks)} files processed.  {self._summary}"

    def _collect_assets(
        self,
        nif,
        mod_root: str,
        out_dir: str,
        source_profile,
        target_profile,
        results: list[_FileResult],
        copied_assets: set[str],
    ) -> None:
        """Export all textures and materials referenced by *nif* into out_dir/Data/."""
        tex_queue: list[str] = list(_nif_texture_paths(nif))
        mat_paths: list[str] = _nif_material_paths(nif)

        # --- Materials ---
        for mat_path in mat_paths:
            if mat_path in copied_assets:
                continue
            copied_assets.add(mat_path)

            status, note, extra_tex = _export_material_file(
                mat_path, mod_root, out_dir, source_profile, target_profile
            )
            results.append(_FileResult(f"  {mat_path}", "material", status, note))
            _log.debug("Material %s: %s — %s", mat_path, status, note)

            # Queue any textures found inside the material file.
            for t in extra_tex:
                if t not in copied_assets:
                    tex_queue.append(t)

        # --- Textures ---
        for tex_path in tex_queue:
            if tex_path in copied_assets:
                continue
            copied_assets.add(tex_path)

            if self._skip_existing:
                # Build expected output path (approximate — suffix may change)
                candidate = os.path.normpath(
                    os.path.join(out_dir, "Data", tex_path.replace("/", os.sep))
                )
                if os.path.exists(candidate):
                    continue

            status, note = _export_texture_file(
                tex_path, mod_root, out_dir, source_profile, target_profile
            )
            results.append(_FileResult(f"  {tex_path}", "texture", status, note))
            _log.debug("Texture %s: %s — %s", tex_path, status, note)
