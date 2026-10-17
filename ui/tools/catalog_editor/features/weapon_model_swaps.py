"""WeaponModelSwaps extension: a guided editor for a weapon skin's mesh swaps.

Mirrors Tales' ``WeaponModelSwapRules.h``: a skin OMOD, while attached, makes every mesh the weapon loads that a pair lists
load its replacement instead. Paths compare lowercase, with backslashes and without a leading ``Meshes\\``.
"""

from __future__ import annotations

import copy
import struct
from dataclasses import dataclass, field

from creation_lib.esp.native_runtime import (
    plugin_handle_get,
    plugin_handle_record_subrecords,
    plugin_handle_search_records,
)

from ui.tools.catalog_editor.features import (
    FeatureExtension, InspectorTab, RemoveWording, ResultView, SectionAction, register,
)
from ui.tools.catalog_editor.forms import PluginFormSource
from ui.tools.catalog_editor.model import ADDED, OVERRIDDEN, REMOVED, FeatureDocument, Problem, Row

FEATURE = "WeaponModelSwaps"
SECTION = "skins"
MAX_APPLIES_SHOWN = 12

MESH_NODE = {"type": "string", "x-asset": {"kind": "nif", "root": "Meshes", "strip_root": True}}


def normalize_mesh(path: str) -> str:
    """The spelling Tales compares meshes by."""
    text = path.replace("/", "\\").lower().strip()
    return text.removeprefix("meshes\\")


def pair_issues(models: list) -> list[str]:
    """Why Tales would skip some of a skin's pairs (it keeps the first pair for each original)."""
    out, seen = [], set()
    for index, pair in enumerate(models, start=1):
        if not (isinstance(pair, list) and len(pair) == 2 and all(isinstance(part, str) for part in pair)):
            out.append(f"swap {index}: not a pair of two mesh paths, Tales skips it")
            continue
        original, replacement = normalize_mesh(pair[0]), normalize_mesh(pair[1])
        if not original or not replacement:
            out.append(f"swap {index}: a mesh path is empty, Tales skips it")
        elif original == replacement:
            out.append(f"swap {index}: replaces a mesh with itself, Tales skips it")
        elif original in seen:
            out.append(f"swap {index}: {pair[0]} is already swapped by an earlier pair; only the first counts")
        seen.add(original)
    return out


def skin_issues(entry: dict, asset_exists=None) -> list[str]:
    models = entry.get("models") or []
    out = [] if models else ["no mesh swaps; Tales ignores a skin without any"]
    out += pair_issues(models)
    if asset_exists is not None:
        for pair in models:
            if isinstance(pair, list) and len(pair) == 2 and isinstance(pair[1], str) and pair[1].strip():
                if asset_exists(pair[1]) is False:
                    out.append(f"replacement mesh {pair[1]} was not found in any loaded folder or archive; the weapon keeps its own mesh")
    return out


# --- Reading the skin and its weapons from the loaded plugins ---------------------------------------------


@dataclass
class Mod:
    key: str
    editor_id: str
    model: str
    attach_point: str
    keywords: tuple[str, ...]


@dataclass
class Weapon:
    key: str
    editor_id: str
    name: str
    model: str
    attach_points: frozenset[str]
    keywords: frozenset[str]


@dataclass
class SkinInfo:
    key: str
    editor_id: str = ""
    name: str = ""
    signature: str = ""
    form_type: str = ""
    attach_point: str = ""
    weapons: list[Weapon] = field(default_factory=list)
    mods_by_weapon: dict[str, list[Mod]] = field(default_factory=dict)


def _text(data: bytes) -> str:
    return data.split(b"\0", 1)[0].decode("utf-8", "replace")


def _ids(data: bytes) -> list[int]:
    return list(struct.unpack_from(f"<{len(data) // 4}I", data))


def _local_id(forms: PluginFormSource, plugin, object_id: int) -> int:
    masters = plugin_handle_get(plugin.handle, "masters") or []
    return (len(masters) << 24) | object_id


def _read_mod(forms: PluginFormSource, plugin, form_id: int, key: str, editor_id: str) -> Mod | None:
    editor, model, data, target = editor_id, "", b"", b""
    for sig, value, _ in plugin_handle_record_subrecords(plugin.handle, form_id) or []:
        if sig == "MODL":
            model = _text(value)
        elif sig == "DATA":
            data = value
        elif sig == "MNAM":
            target = value
    if len(data) < 20 or data[10:14] != b"WEAP":
        return None
    attach = struct.unpack_from("<I", data, 16)[0]
    return Mod(key, editor, model, forms.key(attach, plugin.handle) if attach else "",
               tuple(forms.key(k, plugin.handle) for k in _ids(target)))


def _read_weapon(forms: PluginFormSource, plugin, form_id: int, key: str, editor_id: str, name: str) -> Weapon:
    model, appr, kwda = "", b"", b""
    for sig, value, _ in plugin_handle_record_subrecords(plugin.handle, form_id) or []:
        if sig == "MODL":
            model = _text(value)
        elif sig == "APPR":
            appr += value
        elif sig == "KWDA":
            kwda += value
    return Weapon(key, editor_id, name, model, frozenset(forms.key(k, plugin.handle) for k in _ids(appr)),
                  frozenset(forms.key(k, plugin.handle) for k in _ids(kwda)))


def read_world(forms: PluginFormSource) -> tuple[list[Weapon], dict[str, Mod]]:
    """Every WEAP and every weapon OMOD in the loaded plugins."""
    weapons: list[Weapon] = []
    mods: dict[str, Mod] = {}
    for plugin in forms.session.plugins:
        for signature in ("WEAP", "OMOD"):
            try:
                rows = plugin_handle_search_records(plugin.handle, "*", signatures=[signature], limit=1_000_000)
            except Exception:
                continue
            for row in rows:
                key = forms.key(row["form_id"], plugin.handle)
                if forms.key(row["form_id"], plugin.handle).split(":", 1)[0].casefold() != plugin.plugin_name.casefold():
                    continue  # an override of a master's record
                if signature == "WEAP":
                    weapons.append(_read_weapon(forms, plugin, row["form_id"], key, row["editor_id"] or "",
                                                row.get("full_name") or ""))
                else:
                    mod = _read_mod(forms, plugin, row["form_id"], key, row["editor_id"] or "")
                    if mod is not None:
                        mods[key] = mod
    return weapons, mods


def _fits(weapon: Weapon, mod: Mod) -> bool:
    return bool(mod.attach_point) and mod.attach_point in weapon.attach_points and (
        not mod.keywords or any(keyword in weapon.keywords for keyword in mod.keywords))


def describe_skin(forms: PluginFormSource, key: str, world: tuple[list[Weapon], dict[str, Mod]]) -> SkinInfo:
    info = SkinInfo(key)
    resolved = forms.resolve(key)
    if resolved is None:
        return info
    info.signature, info.editor_id = resolved
    mod = world[1].get(key)
    if mod is None:
        return info
    info.form_type, info.attach_point = "WEAP", mod.attach_point
    info.weapons = [weapon for weapon in world[0] if _fits(weapon, mod)]
    for weapon in info.weapons:
        info.mods_by_weapon[weapon.key] = [other for other in world[1].values()
                                          if other.key != key and other.model and _fits(weapon, other)]
    return info


def candidate_meshes(weapon: Weapon, mods: list[Mod]) -> list[tuple[str, str]]:
    """(label, mesh path) the weapon can load: its own model and its attachments' models."""
    out = [(f"Weapon {weapon.editor_id}", weapon.model)] if weapon.model else []
    out += [(f"Mod {mod.editor_id}", mod.model) for mod in mods if mod.model]
    seen, unique = set(), []
    for label, path in out:
        if normalize_mesh(path) not in seen:
            seen.add(normalize_mesh(path))
            unique.append((label, path))
    return unique


# --- Extension ------------------------------------------------------------------------------------------------


class WeaponModelSwapsExtension(FeatureExtension):
    def __init__(self) -> None:
        self._world: tuple[object, tuple[list[Weapon], dict[str, Mod]]] | None = None
        self._infos: dict[str, SkinInfo] = {}
        self._weapon_choice: dict[str, int] = {}
        self._filter = ""
        self._mesh_cache: tuple[tuple[int, int], set[str]] | None = None
        self.last_exists = None

    def world(self, forms):
        if not isinstance(forms, PluginFormSource):
            return None
        if self._world is None or self._world[0] is not forms or len(forms.loaded_names) != self._world[1][2]:
            self._infos = {}
            self._world = (forms, (*read_world(forms), len(forms.loaded_names)))
        return self._world[1][:2]

    def skin_info(self, forms, key: str) -> SkinInfo | None:
        world = self.world(forms)
        if world is None or not key:
            return None
        if key not in self._infos:
            self._infos[key] = describe_skin(forms, key, world)
        return self._infos[key]

    def _asset_exists(self, tool):
        """A cached "does this mesh exist" check over the editor's asset roots; None while they are still being listed."""
        source = getattr(getattr(tool, "_assets", None), "source", None)
        if source is None:
            return None
        source.start("Meshes")
        if source.loading:
            return None
        if self._mesh_cache is None or self._mesh_cache[0] != (id(source), source.version):
            files = {item.path.replace("/", "\\").casefold() for item in source.snapshot("nif", "Meshes")}
            self._mesh_cache = ((id(source), source.version), files)
        files = self._mesh_cache[1]
        if not files:
            return None
        return lambda path: ("meshes\\" + normalize_mesh(path)) in files

    def section_actions(self, tool, doc, section):
        if section != SECTION:
            return []

        def apply(key: str) -> None:
            if not key:
                return
            existing = next((row for row in doc.rows(SECTION) if row.key == key and row.status != REMOVED), None)
            if existing is not None:
                tool.select_row(existing.key)
                tool.notify("That skin is already in the catalog.")
                return
            described = tool.forms.describe(key) if tool.forms else None
            editor_id = described.split(" [", 1)[0] if described else ""
            tool.select_row(doc.set_entry(SECTION, {"form": key, "editor_id": editor_id, "models": []}))

        return [SectionAction("Add skin from OMOD...", lambda: tool.pick_record(("OMOD",), apply),
                              "Pick the weapon skin OMOD, then list the meshes it swaps.")]

    def problems(self, doc, forms):
        if doc.name != FEATURE:
            return []
        out = []
        for row in doc.rows(SECTION):
            if row.status not in (ADDED, OVERRIDDEN) or not isinstance(row.entry, dict):
                continue
            for message in skin_issues(row.entry, self.last_exists):
                out.append(Problem(doc.name, SECTION, row.key, message, blocking=False))
            resolved = forms.resolve(row.key) if hasattr(forms, "resolve") else None
            if resolved is not None and resolved[0] != "OMOD":
                out.append(Problem(doc.name, SECTION, row.key, f"{row.key} is a {resolved[0]} record; a skin must be an OMOD",
                                   blocking=False))
        return out

    def custom_form(self, doc, section):
        return doc.name == FEATURE and section == SECTION

    def draw_form(self, tool, doc, section, row: Row, ctx, read_only: bool):
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color
        from ui.tools.catalog_editor import render

        entry = copy.deepcopy(row.entry) if isinstance(row.entry, dict) else {"form": "", "models": []}
        entry.setdefault("models", [])
        changed = False
        entry_schema = doc.entry_schema(SECTION)
        properties = entry_schema.get("properties", {})

        if imgui.collapsing_header("How a weapon skin works", imgui.TreeNodeFlags_.default_open if not entry["models"] else 0):
            imgui.text_wrapped(
                "While this skin (an OMOD) is attached to a weapon, every mesh the weapon loads that is listed on the left "
                "below is loaded as the mesh on the right instead. That covers the weapon itself and its attachments. "
                "Meshes that are not listed load normally, and a replacement that cannot be found is ignored.")

        did, new_form = render.draw_value("Skin (OMOD)", properties["form"], entry.get("form", ""), ctx, ("skin", "form"))
        if did and not read_only:
            entry["form"] = new_form
            changed = True
            described = tool.forms.describe(new_form) if tool.forms and new_form else None
            if described and not entry.get("editor_id"):
                entry["editor_id"] = described.split(" [", 1)[0]
        forms = tool.forms
        info = self.skin_info(forms, entry.get("form", ""))
        if info is not None and info.signature:
            title = info.editor_id or entry["form"]
            imgui.text(f"{title}  [{info.signature}]")
            if info.signature != "OMOD":
                imgui.text_colored(semantic_color("error"), "This is not an OMOD, so no weapon can ever have it attached.")
            elif not info.weapons:
                imgui.text_disabled("No weapon in the loaded plugins takes this OMOD (matching attach point and keywords).")
            else:
                names = [weapon.name or weapon.editor_id for weapon in info.weapons]
                shown = ", ".join(names[:MAX_APPLIES_SHOWN]) + (f" and {len(names) - MAX_APPLIES_SHOWN} more"
                                                               if len(names) > MAX_APPLIES_SHOWN else "")
                imgui.text_wrapped(f"Applies to: {shown}")
        elif entry.get("form"):
            imgui.text_colored(semantic_color("warning"), "That record is not in the loaded plugins.")

        imgui.separator()
        imgui.text("Mesh swaps")
        models = entry["models"]
        exists = self.last_exists = self._asset_exists(tool)
        if imgui.begin_table("swaps", 4, imgui.TableFlags_.row_bg | imgui.TableFlags_.resizable):
            imgui.table_setup_column("Replace this mesh", imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("With this mesh", imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("##status", imgui.TableColumnFlags_.width_fixed, 90)
            imgui.table_setup_column("##del", imgui.TableColumnFlags_.width_fixed, 28)
            imgui.table_headers_row()
            remove_at = None
            for index, pair in enumerate(models):
                if not (isinstance(pair, list) and len(pair) == 2):
                    continue
                imgui.table_next_row()
                imgui.push_id(index)
                for column in (0, 1):
                    imgui.table_set_column_index(column)
                    did, value = render.draw_value("", MESH_NODE, pair[column], ctx, ("swap", index, column))
                    if did and not read_only:
                        pair[column] = value
                        changed = True
                imgui.table_set_column_index(2)
                problems = pair_issues([pair])
                if problems:
                    imgui.text_colored(semantic_color("warning"), "ignored")
                    imgui.set_item_tooltip(problems[0])
                elif exists is not None and pair[1] and exists(pair[1]) is False:
                    imgui.text_colored(semantic_color("warning"), "not found")
                    imgui.set_item_tooltip("The replacement mesh is not in any loaded folder or archive.")
                imgui.table_set_column_index(3)
                imgui.begin_disabled(read_only)
                if imgui.small_button("x"):
                    remove_at = index
                imgui.end_disabled()
                imgui.pop_id()
            imgui.end_table()
            if remove_at is not None:
                del models[remove_at]
                changed = True
        imgui.begin_disabled(read_only)
        if imgui.button("Add swap"):
            models.append(["", ""])
            changed = True
        imgui.end_disabled()

        changed |= self._draw_candidates(tool, entry, info, read_only)
        if properties.get("editor_id") is not None:
            did, value = render.draw_value("Editor ID", properties["editor_id"], entry.get("editor_id", ""), ctx,
                                           ("skin", "editor_id"))
            if did and not read_only:
                entry["editor_id"] = value
                changed = True
        return changed, entry

    def result_view(self, doc, section):
        return ResultView("What this skin does", self.draw_effect) if doc.name == FEATURE and section == SECTION else None

    def inspector_tabs(self, view):
        if view.doc.name != FEATURE or view.section != SECTION:
            return []
        return [InspectorTab("Skin & swaps", lambda v: self.draw_form(v.tool, v.doc, v.section, v.row, v.ctx, v.read_only))]

    def sentence(self, view):
        if view.section != SECTION or not isinstance(view.entry, dict):
            return ""
        count = len(view.entry.get("models") or [])
        return (f"While this skin is attached, the weapon loads {count} replacement mesh{'es' * (count != 1)} "
                "instead of its own.") if count else "This skin swaps no meshes yet, so Tales ignores it."

    def main_list(self, doc, section):
        return ("models", "swaps") if section == SECTION else None

    def describe_item(self, doc, section, field_name, item, name_of):
        if field_name == "models" and isinstance(item, list) and len(item) == 2:
            return f"{item[0] or '(none)'} -> {item[1] or '(none)'}"
        return None

    def remove_wording(self, doc, section):
        if section != SECTION:
            return None
        return RemoveWording("Stop swapping meshes", "Your plugin removes this skin from the catalog, so weapons with "
                             "it attached keep their own meshes.", "Removed: weapons with this skin keep their own meshes.")

    def draw_effect(self, view):
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color

        entry = view.entry if isinstance(view.entry, dict) else {}
        info = self.skin_info(view.forms, entry.get("form", ""))
        if info is not None and info.weapons:
            names = [weapon.name or weapon.editor_id for weapon in info.weapons]
            more = f" and {len(names) - MAX_APPLIES_SHOWN} more" if len(names) > MAX_APPLIES_SHOWN else ""
            imgui.text_wrapped(f"Applies to: {', '.join(names[:MAX_APPLIES_SHOWN])}{more}")
        elif info is not None and info.signature:
            imgui.text_disabled("No weapon in the loaded plugins takes this OMOD.")
        models = entry.get("models") or []
        if not models:
            imgui.text_disabled("No mesh swaps yet; Tales ignores a skin without any.")
            return False, view.entry
        exists = self.last_exists
        flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h | imgui.TableFlags_.resizable
        if imgui.begin_table("##effect", 3, flags):
            imgui.table_setup_column("The weapon loads", imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("Tales loads instead", imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("##status", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("not found__").x)
            imgui.table_headers_row()
            for pair in models:
                if not isinstance(pair, list) or len(pair) != 2:
                    continue
                imgui.table_next_row()
                imgui.table_next_column()
                imgui.text_wrapped(pair[0] or "(none)")
                imgui.table_next_column()
                imgui.text_wrapped(pair[1] or "(none)")
                imgui.table_next_column()
                issues = pair_issues([pair])
                if issues:
                    imgui.text_colored(semantic_color("warning"), "ignored")
                    imgui.set_item_tooltip(issues[0])
                elif exists is not None and pair[1] and exists(pair[1]) is False:
                    imgui.text_colored(semantic_color("warning"), "not found")
            imgui.end_table()
        return False, view.entry

    def _draw_candidates(self, tool, entry: dict, info: SkinInfo | None, read_only: bool) -> bool:
        from imgui_bundle import imgui

        if info is None or not info.weapons:
            return False
        if not imgui.collapsing_header("Meshes this skin's weapons load (click to replace one)"):
            return False
        names = [weapon.name or weapon.editor_id for weapon in info.weapons]
        choice = min(self._weapon_choice.get(info.key, 0), len(names) - 1)
        imgui.set_next_item_width(300)
        moved, choice = imgui.combo("Weapon", choice, names)
        self._weapon_choice[info.key] = choice
        weapon = info.weapons[choice]
        imgui.set_next_item_width(300)
        _, self._filter = imgui.input_text("Filter", self._filter)
        taken = {normalize_mesh(pair[0]) for pair in entry["models"] if isinstance(pair, list) and pair}
        changed = False
        for label, path in candidate_meshes(weapon, info.mods_by_weapon.get(weapon.key, [])):
            if self._filter and self._filter.casefold() not in f"{label} {path}".casefold():
                continue
            listed = normalize_mesh(path) in taken
            imgui.push_id(f"{label}|{path}")
            imgui.begin_disabled(read_only or listed)
            if imgui.small_button("Replace this"):
                entry["models"].append([path, ""])
                changed = True
            imgui.end_disabled()
            imgui.same_line()
            imgui.text_disabled(f"{label}: ") if not listed else imgui.text_disabled(f"{label} (listed): ")
            imgui.same_line()
            imgui.text(path)
            imgui.pop_id()
        return changed


register(FEATURE, WeaponModelSwapsExtension())
