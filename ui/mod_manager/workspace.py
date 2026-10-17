from __future__ import annotations

from pathlib import Path
import json
import webbrowser

from imgui_bundle import imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.forms import pick_file
from creation_lib.ui.widgets.modern import action_button, heading, scaled, section, status_indicator

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.locations import default_library_path
from creation_lib.mod_manager.mo2 import identity
from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.nexus import GAMES
from creation_lib.mod_manager.fomod import EXTENDERS
from creation_lib.mod_manager.conflicts import reveal
from creation_lib.mod_manager.tree import ModTree
from creation_lib.mod_manager.status import file_states
from creation_lib.mod_manager.diagnostics import emit as emit_diagnostics
from .mod_list_ui import draw_tree
from .launcher_ui import LauncherUI
from creation_lib.mod_manager.instances import GAMES as INSTANCE_GAMES
from .panes_ui import ManagerPanes
from .toolbar_ui import draw_navigation, handle_navigation_keys, handle_list_keys, focus_search, dismiss_workflow
from .profile_transfers_ui import ProfileTransfersUI
from .metadata_ui import MetadataUI
from .reference_sources_ui import SourceUI
from .source_metadata_ui import SourceMetadataUI
from .plugin_priority_ui import PluginPriorityUI


_NS = "##mod_manager"


class ModManagerWorkspace(BaseWorkspace):
    id, name, icon = "mod_manager", "Modlist Manager", "MODS"
    user_guide_body = """
Library and Settings are the main destinations. First choose a game installation and
separate manager library, then Create instance. Detection runs on explicit request.
The local profile and executables let you install/organize without MO2. To import an
existing instance optionally, use Add mod > Import MO2 instance. All detected
profiles become local metadata referencing the original mod folders. Import does not
copy, move or hash payloads. Ordinary local root links are resolved safely; missing,
network or unsafe roots require review. Unassigned mods get a disabled profile; MO2
overwrite is excluded from import and scanning. Unmanaged/game entries stay outside
managed rows and counts. Repeat import preserves local edits and removed profiles.

Profiles sit above the expandable mod tree. Create or clone a profile, delete only
a named local profile, or restore it from Removed profiles even after restart.
Original MO2 profiles and shared mods stay intact. Search reveals matching descendants.
Groups like Visual/Textures organize the display; higher priority wins file conflicts.
Display sorting preserves priority. The following are default bindings; Settings > Configure
shortcuts can change or disable manager commands. Up/Down select, Left/Right collapse/expand groups,
Space toggles enabled state, and Enter selects. A-Z cycles visible matching mod names;
Ctrl+Up/Down moves the selected set one priority position and Ctrl+Enter opens its folder.
Double-click shows Mod info; Ctrl+double-click opens the folder. Tags and notes belong to the shared mod.
File icons populate automatically from background loose-file and applicable archive scans.
Partial/unavailable inventories remain qualified; F5 requests a fresh scan.
A Nexus version difference does not prove a newer update;
account actions report confirmed or unconfirmed results. Settings manages local categories;
assignment/descendant rename updates shared mods across local profiles. The Archives pane can explicitly index member inventories,
container overrides and candidate overlaps with a persistent local cache. Activation
and member order are profile previews, with engine precedence still unverified.
Ctrl/Shift select multiple mods; drag changes priority/group and Ctrl+Z undoes local
entry organization. Settings controls font/row density, visible mod metadata, column order/widths
and command bindings. Drag header edges to resize and drag headers to reorder. Custom columns
scroll horizontally. Source metadata notices persist across restarts; review fields explicitly
or dismiss a notice. First observation establishes the baseline, and large libraries rotate
bounded batches while the selected referenced mod is prioritized.
The toolbar exposes Install, Add mod, Refresh files, Undo and Actions. Right-click a mod
to create an empty mod folder below it. Ctrl+F focuses
mod search, Ctrl+P opens profile actions, Ctrl+E edits executables, Ctrl+S opens Settings,
Ctrl+M chooses an archive and F5 refreshes local file/archive conflicts. Typing fields
keep their input. Escape closes a manager workflow and cancels its running worker.

The launcher selects configured executables. Launch options > Edit executables opens a draft
list/form for title, path, arguments, working directory, mode, Steam ID and hidden state.
Order and default selection are local; Save commits the draft and Cancel discards it.
An ordinary selected F4SE loader uses its containing validated FO4 installation even when
the previous game selection differs. A staged loader uses its explicit target association.
Run prepares a review; Cancel starts nothing. Direct mode uses the normal filesystem
without our selected mods. The initial modded backend supports x64 Fallout4.exe/F4SE
on Windows 10+, using this project's native virtual filespace. Referenced sources
stay in place; first writes copy only the affected file into owned output.
Steam must already be running. Other games, x86 runtimes, editors/generators, automatic
Steam bootstrap and onefile packaging are not supported by this modded adapter.
Private onedir startup/close/helper checks pass; latest packaged pixel capture is unavailable.
Real game, third-party tool and clean-machine validation remain open.

Modded launch maps local enabled/order state in place and uses local plugin/order
snapshots. The Plugins pane inventories providers and inspects headers through modkit.
Save load order validates dependency/core/master ordering and backs up local snapshots;
changed providers/order block launch until reviewed again. Local locked order retains absent
names; free plugins shift around anchored occupied slots. Owned plugin file opens a separate
inactive FO4 rename/ghost/master-flag review with single-file recovery and guarded Undo.
References remain read-only.
All FO4 INIs use local files; missing snapshots require acknowledgement of minimal
settings. Saves use a distinct virtual path; new Data output uses the local runtime
overwrite, which takes precedence on later runs. Direct source writes are refused.
Managed deletion/rename, whiteouts and ordinary atomic saves use owned output and rollback.
Hardlinks and unsupported intercepted operations fail explicitly. This runtime is not a
hostile-code sandbox; direct syscalls, unhooked brokers and loader-time changes are unverified.
Disposable native fixtures cover the bounded read/write/mutation/mapped-view/child contract;
see NATIVE_API_MATRIX.md for exact classes, flags, limits and unsupported combinations.
UI close leaves the helper/game running; helper failure terminates its supervised
game tree. Persisted sessions block library changes until recorded processes exit.
Cancel applies before process creation. Native tests used generated files and Python;
a real game launch has not been tested.

Mods > Install package accepts ZIP/7z/RAR or unpacked folders. 7z/RAR require an existing
supported 7-Zip/NanaZip decoder. Declarative FOMOD formats 1-5 support all five group
types, nested dependencies, flags, dynamic types, required/conditional files, folder
rules and priority. Unknown constructs fail explicitly. Module/option artwork has a bounded
worker preview and optional package information is displayed. Executable installer scripts
are unsupported. Bounded FO4 archive and requested game/matching F4SE context are discovered
read-only; unknown states need explicit input. Missing sources can bind to same-package
payloads. Review the plan after any change. Owned reinstall, uninstall
and restore retain backups and recover interrupted transactions. Recovery can permanently
delete one reviewed backup with explicit acknowledgement; installed mods stay intact.
Profile recipes export organization/configuration metadata and require explicit mod mapping
in another library. No payloads or executable commands are imported by that format.
Stage profile export
creates a separate combined folder; it never deploys to a game or edits Plugins.txt.
INI/plugin/order snapshots remain local metadata. Profile configuration can edit an
owned INI baseline with a previous-value backup and stale-editor guard. Destination review
can rename/relocate/exclude or explicitly include additional package sources. Owned file
and parent-folder hide/unhide retain a complete backup and recheck the reviewed contents.
Saves lists owned runtime saves and bounded FO4 display headers. Save locations also reads
an explicitly selected local source/global folder or this profile's MO2 source save folder;
it never scans Documents automatically. Default global FO4 saves explicitly resolves the Windows
Documents location and reads the default save folder; customized locations use Browse. Copy / back up save
reviews SHA-256 and copies a save/companion pair to another local profile or a new owned
backup without overwriting existing files. Source pairs can be imported into this local
profile; Save backups restores reviewed pairs into owned local profiles. Sources/backups
remain intact. Reviewed permanent deletion can remove one owned retained save/overwrite bundle
with acknowledgement and guarded interruption recovery. Body/companion compatibility remains unverified.

Nexus uses a session key, explicit download review and opt-in bounded version checks.
Imported download metadata retains references to original archives. Downloads opens those
references directly for installer review without copying the archive. Completed downloads require
an explicit new-mod or existing-owned-mod choice; original references cannot be overwritten.
Caches, saves, credentials and MO2 plugins are excluded. The persistent owned queue verifies range/ETag/hash checkpoints
and resumes/finalizes after restart. No live Nexus account/download test has been performed.
See ui/mod_manager/README.md and PARITY_REVIEW.md for tested scope and remaining gaps.
The original backend uses BSD-licensed MinHook; its notices ship with the runtime.
Optional legacy USVFS attribution and license are available in Settings.
"""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings)
        self.manager = ManagerController()
        self.launcher = LauncherUI(self, toolkit_settings)
        from .launch_targets_ui import TargetsUI
        self.targets_ui = TargetsUI(self)
        from .target_customization_ui import CustomizationUI
        self.target_customization = CustomizationUI(self)
        self.panes = ManagerPanes(self)
        from .download_install_ui import DownloadInstaller
        self.download_installer = DownloadInstaller(self)
        from .nexus_account_ui import AccountUI
        self.nexus_account = AccountUI(self)
        from .nxm_routing_ui import RoutingUI
        self.nxm_router = RoutingUI(self)
        from .nexus_bulk_ui import BulkUI
        self.nexus_bulk = BulkUI(self)
        self.transfers = ProfileTransfersUI(self)
        self.metadata = MetadataUI(self)
        self.sources = SourceUI(self)
        self.source_metadata = SourceMetadataUI(self)
        self.plugin_priority = PluginPriorityUI(self)
        self._library_path = ""
        self._page, self._dialog = "Library", ""
        self._focus_page = "Library"
        self._settings_request = ""
        self._mo2_path = ""
        self._profile_name = ""
        self._new_profile_clone = True
        self._query, self._group = "", ""
        self._focus_search, self._open_profiles = False, False
        self._enabled_only = False
        self._tab = "Library"
        self._incoming_package = ""
        self._tree_pending_select = ""
        self._record_field_rects = False
        self._detail_id = ""
        self._detail_group, self._tags, self._notes, self._priority = "", "", "", 0
        self._import_id = None
        self._import_selected = set()
        self._install_name, self._content_root = "", ""
        self._manual_install = False
        self._overwrite_name, self._overwrite_selected = "Generated output", ""
        self._overwrite_ack, self._overwrite_restore_ack, self._overwrite_key = False, False, None
        self._step = 0
        self._row_height = 28
        self._list_font_size = 0
        self._metadata_fields = ["status", "category", "nexus", "version"]
        from creation_lib.mod_manager.columns import normalize as normalize_columns
        from .columns_ui import ColumnsUI
        self._columns = normalize_columns()
        self._column_drag, self._column_header_rects, self._column_widths = None, {}, {}
        self.columns_ui = ColumnsUI(self)
        from .shortcuts import normalize as normalize_shortcuts
        from .shortcuts_ui import ShortcutsUI
        self._shortcuts = normalize_shortcuts()
        self.shortcuts_ui = ShortcutsUI(self)
        from .metadata_notifications_ui import MetadataNotificationsUI
        self.metadata_notifications = MetadataNotificationsUI(self)
        from .plugin_file_ui import PluginFileUI
        self.plugin_file_ui = PluginFileUI(self)
        from .installer_repair_ui import InstallerRepairUI
        self.installer_repair_ui = InstallerRepairUI(self)
        self._restore_library = False
        self._install_cache = None
        self._evaluation = None
        self._evaluation_error = ""
        self._sort_by, self._descending, self._origin = "priority", False, ""
        self._category_filter, self._category_rows_key, self._category_paths = None, None, []
        from creation_lib.mod_manager.status_query import normalized
        self._status_filters = normalized()
        self._api_key_input, self._nxm_input = "", ""
        self._remember_nexus = True
        self._protocol_confirmed = False
        self._previous_handler = ""
        self._link_inbox = None
        self._inbox_failed = False
        self._replacement_ack = False
        self._lifecycle_ack = False
        self._lifecycle_token = ""
        self._restore_ack = False
        self._restore_id = ""
        self._prune_key, self._prune_ack = None, False
        self._show_imported_profiles = False
        self._one_click_migration = False
        self._profile_filename = ""
        self._save_copy_path, self._save_copy_target, self._save_copy_ack = "", "", False
        self._save_backup_selected, self._save_backup_ack = "", False
        self._retained_origin, self._retained_key, self._retained_ack = "", None, False
        self._save_source_path = ""
        self._source_mappings = {}
        self._migration_selected = set()
        self._migration_ack = False
        self._migration_token = None
        self._file_query, self._file_mode, self._file_mod = "", "all", ""
        self._file_selected = ""
        self._include_disabled_files = True
        self._provider_query, self._provider_key, self._provider_rows = "", None, []
        self._provider_source = None
        self._closed_groups = {}
        self._tree_key, self._tree = None, None
        self._tree_sources, self._tree_mod_count = None, 0
        self._tree_scroll_selected = False
        self._drag_mod = None
        self._right_pane = "Plugins"
        self._visible_tree_key, self._visible_tree = None, []
        self._status_index, self._file_states = None, {}
        self._delete_profile_id, self._delete_profile_ack = "", False
        self._setup_game, self._setup_root, self._setup_name = "fo4", "", "Default"
        self._ini_key, self._ini_text, self._ini_expected = None, "", None
        self._show_filters, self._pane_ratio, self._tree_footer = False, .50, 0
        self._selected_mods, self._selection_anchor, self._selection_profile = set(), "", ""
        self._new_group_name = ""
        self._workflow_kind = ""
        self._job_was_busy = False
        self._backup_selected, self._backup_plan_key = "", None
        self._backup_restore_ack, self._backup_delete_ack = False, False
        self._backup_delete_key = None
        self._mapping_target, self._mapping_destination = "", ""
        self._source_tree_package = None
        self._source_tree_expanded = set()
        self._source_tree_query, self._source_tree_selected, self._source_tree_destination = "", "", ""
        self._category_selected, self._category_name = "", ""
        self._mod_category = ""
        self._mod_categories, self._category_detail_key = "", None
        self._category_nexus_id = 0
        self._installer_image_seen = None

    def get_dockable_windows(self):
        return [make_window(f"{page}{_NS}", "MainDockSpace") for page in ("Library", "Settings")]

    def initialize(self):
        self._bind_panels({f"Library{_NS}": self._draw_main, f"Settings{_NS}": self._draw_settings})
        self._initialized = True

    def draw(self):
        self.nxm_router.poll()
        was_busy = self.manager.busy or self._job_was_busy
        self.manager.poll()
        self._job_was_busy = self.manager.busy
        if was_busy and not self.manager.busy and self.manager.job_state == "completed":
            if self._dialog == "Copy sources" and self.manager.migration_review is None or self._dialog == "Install" and self.manager.prepared is None:
                self._dialog = ""
        if self._show_imported_profiles and not self.manager.busy:
            self._show_imported_profiles = False
            if self.manager.library:
                self._library_path = str(self.manager.library.root)
            if self.manager.job_state == "completed" or self._one_click_migration and self.manager.instance_migration:
                self._tab = "Library"
                if self._one_click_migration and self.manager.import_preview:
                    self._source_mappings.update(self.manager.import_preview.configuration.get("source_mappings", {}))
            self._one_click_migration = False
        if self._restore_library and not self.manager.busy:
            self._restore_library = False
            self._action(lambda: self.manager.open_library(self._library_path))
        from .archive_intake import poll
        poll(self)
        if self._workflow_kind and not self._dialog:
            self._focus_page = "Library"
            self._workflow_kind = ""
        if self._tab:
            previous_page = self._page
            previous_dialog = self._dialog
            requested, self._tab = self._tab, ""
            if requested in {"Library", "Settings", "Nexus"}:
                self._page = "Settings" if requested == "Nexus" else requested
                if requested == "Nexus":
                    self._settings_request = "Nexus"
                self._dialog = ""
            else:
                self._page, self._dialog = "Library", requested
            if self._page != previous_page or self._dialog or previous_dialog:
                self._focus_page = self._page
            if self._dialog:
                self._workflow_kind = ""

    def cleanup(self):
        self.nxm_router.close()
        if self._link_inbox:
            self._link_inbox.close()
        self.manager.cleanup()
        self._api_key_input, self._nxm_input = "", ""

    def confirm_exit(self):
        if self.manager.downloads_active:
            self.manager.pause_download()
            self.manager.poll()
            return None if self.manager.downloads_active else True
        if self.manager.busy:
            self.manager.cancel()
            self.manager.poll()
        return True if not self.manager.busy else None

    def get_settings_defaults(self):
        from creation_lib.mod_manager.columns import normalize
        from .shortcuts import DEFAULTS
        return {"library_path": "", "row_height": 28, "list_font_size": 0,
                "columns": normalize(),
                "shortcuts": dict(DEFAULTS),
                "metadata_fields": ["status", "category", "nexus", "version"], "show_filters": False,
                "pane_ratio": .50, "enable_on_install": True, "auto_updates": True, "auto_file_checks": False,
                "update_hours": 6, "download_slots": 1, "nexus_game": "fallout4", "sort_by": "priority", "descending": False,
                "source_mappings": {}, "mo2_path": "", "closed_groups": {}, "executable_id": "", "target_id": "", "runtime_game": "", "runtime_bundle": ""}

    def apply_settings(self, settings):
        from creation_lib.mod_manager.columns import normalize
        from .shortcuts import normalize as normalize_shortcuts
        self._columns = normalize(settings.get("columns"))
        self._shortcuts = normalize_shortcuts(settings.get("shortcuts"))
        self._show_filters = bool(settings.get("show_filters", False))
        from creation_lib.mod_manager.status_query import normalized
        try:
            self._status_filters = normalized(settings.get("status_filters") if isinstance(settings.get("status_filters"), dict) else None)
        except ManagerError:
            self._status_filters = normalized()
        category = settings.get("category_filter")
        self._category_filter = category if isinstance(category, str) and len(category) <= 200 else None
        self._pane_ratio = max(.25, min(.75, float(settings.get("pane_ratio", .50))))
        self.launcher.selected_id = str(settings.get("executable_id", ""))
        self.manager.target_id = str(settings.get("target_id", ""))
        self.launcher.game_root = str(settings.get("runtime_game", ""))
        self.launcher.bundle_root = str(settings.get("runtime_bundle", ""))
        self.launcher.disable_mcp = settings.get("disable_mcp", False) is True
        self._library_path = str(settings.get("library_path", ""))
        self._mo2_path = str(settings.get("mo2_path", ""))
        self._row_height = max(24, min(46, int(settings.get("row_height", 28))))
        font_size = int(settings.get("list_font_size", 0))
        self._list_font_size = max(18, min(28, font_size)) if font_size >= 18 else 0
        fields = settings.get("metadata_fields", ["status", "category", "nexus", "version"])
        self._metadata_fields = [name for name in ("status", "category", "nexus", "version", "notes") if isinstance(fields, list) and name in fields]
        self._restore_library = bool(self._library_path)
        self.manager.enable_on_install = bool(settings.get("enable_on_install", True))
        self.manager.auto_updates = bool(settings.get("auto_updates", True))
        self.manager.auto_file_checks = bool(settings.get("auto_file_checks", False))
        self.manager.update_hours = max(1, min(24, int(settings.get("update_hours", 6))))
        slots = settings.get("download_slots", 1)
        slots = slots if type(slots) is int and 1 <= slots <= 4 else 1
        if slots != self.manager.download_slots:
            self.manager.set_download_slots(slots)
        self.manager.nexus_game = str(settings.get("nexus_game", "fallout4"))
        self._sort_by = settings.get("sort_by", "priority")
        if self._sort_by not in {"priority", "name", "group", "version", "enabled", "category", "nexus", "notes", "status", "source", "conflicts", "updates", "endorsement", "tracking"}:
            self._sort_by = "priority"
        self._descending = bool(settings.get("descending", False))
        mappings = settings.get("source_mappings", {})
        self._source_mappings = {key: value for key, value in mappings.items() if isinstance(key, str) and isinstance(value, str)} if isinstance(mappings, dict) else {}
        closed = settings.get("closed_groups", {})
        self._closed_groups = {key: set(value) for key, value in closed.items() if isinstance(key, str) and isinstance(value, list) and all(isinstance(item, str) for item in value)} if isinstance(closed, dict) else {}

    def collect_settings(self):
        return {"library_path": self._library_path or (str(self.manager.library.root) if self.manager.library else ""),
                "show_filters": self._show_filters, "pane_ratio": self._pane_ratio,
                "row_height": self._row_height, "list_font_size": self._list_font_size,
                "columns": self._columns,
                "shortcuts": dict(self._shortcuts),
                "metadata_fields": list(self._metadata_fields), "enable_on_install": self.manager.enable_on_install,
                "auto_updates": self.manager.auto_updates, "update_hours": self.manager.update_hours,
                "auto_file_checks": self.manager.auto_file_checks,
                "download_slots": self.manager.download_slots,
                "nexus_game": self.manager.nexus_game, "sort_by": self._sort_by, "descending": self._descending,
                "category_filter": self._category_filter,
                "status_filters": dict(self._status_filters),
                "source_mappings": dict(self._source_mappings), "mo2_path": self._mo2_path,
                "executable_id": self.launcher.selected_id, "runtime_game": self.launcher.game_root,
                "target_id": self.manager.target_id,
                "runtime_bundle": self.launcher.bundle_root,
                "disable_mcp": self.launcher.disable_mcp,
                "closed_groups": {key: sorted(value) for key, value in self._closed_groups.items()}}

    def draw_settings(self):
        if action_button("Configure mod columns"):
            self.columns_ui.open()
            self._tab = "Columns"
        imgui.same_line()
        if action_button("Configure shortcuts"):
            self.shortcuts_ui.open()
            self._tab = "Shortcuts"
        if action_button("Source metadata notices", enabled=bool(self.manager.library)):
            self.metadata_notifications.open()
            self._tab = "Source metadata notices"
        imgui.set_next_item_width(max(scaled(80), imgui.get_content_region_avail().x - scaled(110)))
        _, self._row_height = imgui.slider_int("Row height", self._row_height, 24, 46)
        changed, inherit_font = imgui.checkbox("Use Toolkit text size", not self._list_font_size)
        if changed:
            self._list_font_size = 0 if inherit_font else 20
        if self._list_font_size:
            _, self._list_font_size = imgui.slider_int("Mod list font size", self._list_font_size, 18, 28)
        imgui.text_disabled("Visible mod metadata (narrow panes keep the highest-priority fields)")
        for name, label in (("status", "Status icons"), ("category", "Category"), ("nexus", "Nexus ID"), ("version", "Version"), ("notes", "Notes")):
            changed, enabled = imgui.checkbox(label, name in self._metadata_fields)
            if changed:
                if enabled:
                    self._metadata_fields.append(name)
                else:
                    self._metadata_fields.remove(name)
        _, self.manager.enable_on_install = imgui.checkbox("Enable mods on install", self.manager.enable_on_install)
        _, self.manager.auto_updates = imgui.checkbox("Automatic Nexus update checks", self.manager.auto_updates)
        imgui.set_next_item_width(max(scaled(80), imgui.get_content_region_avail().x - scaled(210)))
        _, self.manager.update_hours = imgui.slider_int("Update interval (hours)", self.manager.update_hours, 1, 24)
        if action_button("Manage categories", enabled=bool(self.manager.library) and not self.manager.busy and not self.manager.runtime_active):
            self._action(self.manager.inspect_categories)
            self._tab = "Categories"
        if imgui.collapsing_header("Metadata adapters"):
            imgui.text_wrapped("Versioned trusted application adapters. Packages and MO2 binaries are never automatically loaded.")
            for adapter in self.manager.extensions.adapters:
                imgui.push_id(adapter.id)
                imgui.text_wrapped(f"{adapter.name} | {adapter.game} | API {adapter.api_version}")
                imgui.set_item_tooltip(adapter.id + "\n" + adapter.summary)
                imgui.pop_id()

    def handle_nxm(self, link):
        from creation_lib.mod_manager.nexus import parse_nxm
        from creation_lib.mod_manager.nxm_instances import take_startup
        self.nxm_router.open(take_startup(parse_nxm(link) if isinstance(link, str) else link))

    def close_workflow(self):
        if self._dialog == "Browser download destination":
            self.nxm_router.dismiss()
        elif self._dialog == "Nexus batch review" and not self.manager.busy:
            self.nexus_bulk.plan, self.nexus_bulk.result, self.nexus_bulk.ack = None, None, False
        self._dialog = ""


    def open_file(self, path):
        if str(path).casefold().startswith("nxm:"):
            self.handle_nxm(path)
        else:
            from .archive_intake import accept
            accept(self, path)

    def handle_file_drop(self, paths, *, x=None, y=None):
        if len(paths) != 1:
            self.manager.error = "Drop one mod archive or unpacked folder for review at a time."
            return False
        try:
            self.open_file(paths[0])
        except (ManagerError, OSError) as exc:
            self.manager.error = str(exc)
            emit_diagnostics(self.manager)
            return False
        return True

    def draw_menu(self):
        if imgui.begin_menu("Mods"):
            for label, target in (("Create game instance...", "Setup"), ("Install package...", "Install"), ("Inspect files", "Files"), ("Manage owned overwrite...", "Overwrite"), ("Profile configuration", "Profile config"), ("Profile backups...", "Profile backups"), ("Configure executables...", "Executables"), ("Stage profile export...", "Stage"), ("Recovery", "Recovery"), ("Import MO2 instance...", "Import MO2")):
                if imgui.menu_item(label, "", False, not self.manager.busy and not self.manager.runtime_active)[0]:
                    self._tab = target
            if imgui.menu_item("Create group...", "", False, bool(self.manager.profile_id) and not self.manager.busy)[0]:
                self._tab = "New group"
            from .shortcuts import label as shortcut_label
            if imgui.menu_item("Undo organization", shortcut_label(self, "mod_undo"), False, bool(self.manager.profile_id) and not self.manager.busy)[0]:
                self._action(self.manager.undo_organization)
            imgui.end_menu()
        if self._view_helper:
            self._view_helper.draw([f"Library{_NS}", f"Settings{_NS}"])

    def has_toolbar(self):
        return False

    def draw_toolbar(self, font=None):
        pass

    def _action(self, callback):
        try:
            self.manager.error = ""
            callback()
        except (ManagerError, OSError) as exc:
            self.manager.error = str(exc)

    def _pick_package(self):
        path = pick_file("Open mod archive", [("Mod archives", "*.zip *.7z *.rar *.fomod")])
        if path:
            self._prepare(path)

    def _reveal_mod_folder(self):
        from creation_lib.mod_manager.folder_actions import reveal_mod
        m = self.manager
        self._action(lambda: reveal_mod(m.library, m.profile_id, m.selected_id))

    def _visit_nexus(self, mod=False):
        from creation_lib.mod_manager.nexus_links import open_game, open_mod
        m = self.manager
        self._action(lambda: open_mod(m.library, m.profile_id, m.selected_id) if mod else open_game(m.library, m.profile_id))

    def _select_group_descendants(self, group):
        m = self.manager
        rows = m.filtered(self._query, self._group, self._enabled_only, sort_by=self._sort_by, descending=self._descending, origin=self._origin, category=self._category_filter, statuses=self._status_filters)
        selected = [row.id for row in rows if row.kind != "separator" and (row.group == group or row.group.startswith(group + "/"))]
        self._selected_mods = set(selected)
        if selected:
            m.selected_id = self._selection_anchor = selected[0]
        m.status = f"Selected {len(selected):,} matching descendants in {group}."

    def _prepare(self, path, replace_id="", *, download_review=None, overlay_id=None):
        if overlay_id is None:
            overlay_id = getattr(self, "_install_overlay", "") if not replace_id and download_review is None else ""
        self._action(lambda: self.manager.prepare_install(path, replace_id, manual=self._manual_install, download_review=download_review, overlay_id=overlay_id))
        row = self.manager.rows_by_id.get(replace_id)
        self._install_name = row.name if row else Path(path).stem
        self._replacement_ack = False
        self._content_root, self._step = "", 0
        self._install_cache = None
        self._tab = "Install"

    def _draw_library_destination(self):
        m = self.manager
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.set_next_item_width(-1)
        _, self._library_path = imgui.input_text_with_hint("##library_path", "Optional destination folder", self._library_path)
        if action_button("Browse folder"):
            path = pick_folder("Choose an empty folder or an existing manager library", self._library_path)
            if path:
                self._library_path = path
        if action_button("Create library", primary=True):
            self._action(lambda: m.open_library(self._library_path, create=True))
        imgui.same_line()
        if action_button("Open", enabled=bool(self._library_path)):
            self._action(lambda: m.open_library(self._library_path))
        imgui.end_disabled()

    def _draw_main(self):
        self._draw_page("Library")

    def _draw_settings(self):
        self._draw_page("Settings")

    def _draw_page(self, page):
        imgui.push_style_var(imgui.StyleVar_.frame_padding, (scaled(5), scaled(2)))
        imgui.push_style_var(imgui.StyleVar_.item_spacing, (scaled(5), scaled(3)))
        if self._focus_page == page:
            imgui.set_next_window_focus()
            self._focus_page = ""
        visible = imgui.begin(f"{page}{_NS}", flags=imgui.WindowFlags_.no_focus_on_appearing)[0]
        try:
            if not visible:
                return
            if imgui.is_window_focused(imgui.FocusedFlags_.child_windows):
                self._page = page
            m = self.manager
            handle_navigation_keys(self)
            if page == "Library":
                draw_navigation(self)
            if self._incoming_package:
                imgui.text_wrapped("Pending package: " + Path(self._incoming_package).name)
                imgui.set_item_tooltip(self._incoming_package)
                if action_button("Discard pending package"):
                    self._incoming_package = ""

            if page == "Library" and (not m.library or not m.profiles):
                if m.job_state != "idle":
                    status_indicator(m.status, "accent" if m.busy else "muted")
            elif m.busy:
                status_indicator(m.status, "accent")
            if m.busy:
                fraction, detail = m.progress
                imgui.progress_bar(-imgui.get_time() if fraction is None else fraction, (-1, scaled(6)), "")
                if detail:
                    imgui.text_disabled(detail)
                if not self._dialog and action_button("Cancel operation"):
                    m.cancel()
            if m.error and not self._dialog:
                status_indicator(m.error, "error")
                if imgui.small_button("Dismiss error"):
                    m.error = ""
            imgui.spacing()
            if page == "Settings":
                if imgui.begin_tab_bar("manager_settings_sections"):
                    for label, draw in (("General", self.draw_settings),
                                        ("Library location", self._draw_library_destination),
                                        ("Nexus", self._draw_nexus),
                                        ("Runtime", self.launcher.draw_settings)):
                        flags = imgui.TabItemFlags_.set_selected if self._settings_request == label else 0
                        if imgui.begin_tab_item(label, flags=flags)[0]:
                            imgui.begin_child("settings_content", (0, 0))
                            try:
                                draw()
                            finally:
                                imgui.end_child()
                                imgui.end_tab_item()
                    imgui.end_tab_bar()
                    self._settings_request = ""
                return
            self._draw_migration_summary()
            observed = m.metadata_observer.index
            if observed and observed.count:
                if action_button(f"Source metadata changed ({observed.count})"):
                    self.metadata_notifications.open()
                imgui.set_item_tooltip("Read-only source observation; review or dismiss changes without overwriting local fields. " + str(observed.visited) + "/" + str(len(observed.targets)) + " files observed; large libraries rotate in bounded batches.")
            if not m.library or not m.profiles:
                self._draw_setup()
            else:
                self._draw_manager_layout()
                self._draw_status_footer()
            if self._dialog:
                self._draw_workflow()
            else:
                self._workflow_kind = ""
        finally:
            imgui.end()
            imgui.pop_style_var(2)

    def _draw_workflow(self):
        drawers = {"Setup": self._draw_setup, "Import MO2": self._draw_import, "Profile config": self._draw_profile_configuration,
                   "Install": self._draw_install, "Copy sources": self._draw_migration, "Files": self._draw_files,
                   "Stage": self._draw_stage, "Recovery": self._draw_recovery,
                   "New profile": self._draw_new_profile, "Delete profile": self._draw_delete_profile,
                   "Removed profiles": self._draw_removed_profiles, "Rename profile": self._draw_rename_profile,
                   "Executables": self.launcher.draw_editor, "Launch": self.launcher.draw_review,
                   "Runtime setup": self.launcher.draw_setup}
        drawers["Launch targets"] = self.targets_ui.draw
        drawers["Columns"] = self.columns_ui.draw
        drawers["Shortcuts"] = self.shortcuts_ui.draw
        drawers["Source metadata notices"] = self.metadata_notifications.draw
        drawers["Owned plugin file"] = self.plugin_file_ui.draw
        drawers["Target customization"] = self.target_customization.draw
        drawers["New group"] = self._draw_new_group
        drawers["Create empty mod"] = self.metadata.draw_empty
        drawers["Profile backups"] = self._draw_profile_backups
        drawers["Profile transfers"] = self.transfers.draw
        drawers["Rename mod title"] = self.metadata.draw_rename
        drawers["Edit mod field"] = self.metadata.draw_field
        drawers["Rename group"] = self.metadata.draw_group
        drawers["Relink source"] = self.sources.draw
        drawers["Refresh source metadata"] = self.source_metadata.draw
        drawers["Edit plugin priority"] = self.plugin_priority.draw
        drawers["Remove owned mod"] = self.metadata.draw_remove
        drawers["Remove references"] = self.metadata.draw_reference_removal
        drawers["Save copy"] = self._draw_save_copy
        drawers["Save backups"] = self._draw_save_backups
        from .backup_prune_ui import draw as draw_backup_deletion
        drawers["Backup deletion"] = lambda: draw_backup_deletion(self)
        from .save_inspection_ui import draw as draw_save_plugins
        drawers["Save plugins"] = lambda: draw_save_plugins(self)
        drawers["Browse saves"] = self._draw_source_saves
        drawers["Categories"] = self._draw_categories
        drawers["Overwrite"] = self._draw_overwrite
        drawers["Download destination"] = self.download_installer.draw
        drawers["Nexus account request"] = self.nexus_account.draw
        drawers["Browser download destination"] = self.nxm_router.draw
        drawers["Nexus batch review"] = self.nexus_bulk.draw
        size = imgui.get_io().display_size
        width, height = min(scaled(1050), size.x * .8), min(scaled(850), size.y * .85)
        if self._dialog in {"Create empty mod", "New group", "New profile", "Rename profile", "Rename mod title", "Edit plugin priority"}:
            width, height = min(scaled(560), size.x * .9), min(scaled(350), size.y * .8)
        imgui.set_next_window_size((width, height), imgui.Cond_.appearing)
        imgui.set_next_window_pos(((size.x - width) / 2, (size.y - height) / 2), imgui.Cond_.appearing)
        if self._workflow_kind != self._dialog:
            imgui.set_next_window_scroll((0, 0))
            imgui.set_next_window_focus()
            self._workflow_kind = self._dialog
        visible, opened = imgui.begin(self._dialog + "###manager_workflow" + _NS, True)
        try:
            if not opened:
                if self.manager.busy:
                    self.manager.cancel()
                self.close_workflow()
            elif visible:
                if dismiss_workflow(self):
                    return
                if self.manager.error:
                    status_indicator(self.manager.error, "error")
                if self.manager.busy:
                    fraction, detail = self.manager.progress
                    imgui.progress_bar(-imgui.get_time() if fraction is None else fraction, (-1, scaled(6)), "")
                    if detail:
                        imgui.text_disabled(detail)
                    if action_button("Cancel operation"):
                        self.manager.cancel()
                if self._dialog != "Executables" and action_button("Back to Library"):
                    self.close_workflow()
                else:
                    drawers[self._dialog]()
        finally:
            imgui.end()

    def _draw_manager_layout(self):
        available = imgui.get_content_region_avail()
        footer = imgui.get_frame_height_with_spacing()
        available.y = max(scaled(140), available.y - footer)
        if self._show_filters:
            imgui.begin_child("filter_sidebar", (min(scaled(210), available.x * .24), available.y))
            try:
                self._draw_filter_contents()
            finally:
                imgui.end_child()
            imgui.same_line()
        available = imgui.get_content_region_avail()
        available.y = max(scaled(140), available.y - footer)
        gutter = scaled(12)
        usable = max(scaled(160), available.x - gutter)
        left_min, right_min = min(scaled(390), usable * .48), min(scaled(420), usable * .48)
        left_width = max(left_min, min(usable - right_min, usable * self._pane_ratio))
        imgui.begin_child("modlist_pane", (left_width, available.y))
        try:
            self._draw_profile_selector()
            if not self._show_filters:
                imgui.set_next_item_width(-1)
                focus_search(self)
                _, self._query = imgui.input_text_with_hint("##search_mods", "Search mods...", self._query)
            self._tree_footer = 0
            self._draw_mod_rows()
        finally:
            imgui.end_child()
        imgui.same_line(spacing=0)
        imgui.invisible_button("##pane_splitter", (gutter, available.y))
        imgui.set_item_tooltip("Drag to resize mod and information panes")
        if imgui.is_item_hovered() or imgui.is_item_active():
            imgui.set_mouse_cursor(imgui.MouseCursor_.resize_ew)
        rect = imgui.get_item_rect_min(), imgui.get_item_rect_max()
        center = (rect[0].x + rect[1].x) * .5
        imgui.get_window_draw_list().add_line((center, rect[0].y + scaled(8)), (center, rect[1].y - scaled(8)), imgui.get_color_u32(imgui.Col_.separator), scaled(2))
        if imgui.is_item_active() and available.x:
            self._pane_ratio = max(left_min / usable, min(1 - right_min / usable, self._pane_ratio + imgui.get_io().mouse_delta.x / usable))
        imgui.same_line(spacing=0)
        imgui.begin_child("information_pane", (0, available.y))
        try:
            self._draw_details(embedded=True)
        finally:
            imgui.end_child()

    def _draw_status_footer(self):
        m = self.manager
        imgui.align_text_to_frame_padding()
        imgui.text_disabled(f"{m.mod_count:,} mods | {m.enabled_count:,} enabled | Local conflicts: {m.conflict_scan_state}")
        imgui.set_item_tooltip(m.error or m.status + "\n" + m.conflict_scan_progress[1] + "\n" + m.conflict_scan_error + "\nDiagnostics are in the Toolkit Log (View > Log).")

    def _draw_filter_contents(self):
        m = self.manager
        heading("Search and filter")
        imgui.set_next_item_width(-1)
        focus_search(self)
        _, self._query = imgui.input_text_with_hint("##search_mods", "Search mods...", self._query)
        _, self._enabled_only = imgui.checkbox("Enabled only", self._enabled_only)
        if imgui.collapsing_header("Filter options"):
            groups = m.groups
            if self._group not in groups:
                self._group = ""
            imgui.set_next_item_width(-1)
            changed, index = imgui.combo("##group_filter", groups.index(self._group), ["All groups"] + groups[1:])
            if changed:
                self._group = groups[index]
            if self._category_rows_key is not m.rows:
                self._category_rows_key = m.rows
                values = {}
                for row in m.rows:
                    for category in row.categories or ((row.category,) if row.category else ()):
                        for depth in range(1, len(category.split("/")) + 1):
                            value = "/".join(category.split("/")[:depth])
                            values.setdefault(value.casefold(), value)
                self._category_paths = [value for _, value in sorted(values.items())]
            categories = [None, ""] + self._category_paths
            if self._category_filter not in categories:
                self._category_filter = None
            imgui.text_unformatted("Category")
            imgui.set_next_item_width(-1)
            changed, index = imgui.combo("##category_filter", categories.index(self._category_filter), ["All categories", "Uncategorized"] + self._category_paths)
            if changed:
                self._category_filter = categories[index]
            columns = ["priority", "name", "group", "version", "enabled", "category", "nexus", "notes", "status", "source", "conflicts", "updates", "endorsement", "tracking"]
            imgui.set_next_item_width(-1)
            changed, index = imgui.combo("##display_sort", columns.index(self._sort_by), ["Priority", "Name", "Folder", "Version", "Enabled", "Category", "Nexus ID", "Notes", "Combined status", "Source kind", "Loose conflicts", "Updates", "Endorsement", "Tracking"])
            if changed:
                self._sort_by = columns[index]
            _, self._descending = imgui.checkbox("Reverse", self._descending)
            sources = ["", "external", "owned", "separator"]
            imgui.set_next_item_width(-1)
            changed, index = imgui.combo("##source_filter", sources.index(self._origin), ["All sources", "MO2 references", "Installed", "Groups"])
            if changed:
                self._origin = sources[index]
            from .status_query_ui import draw as draw_status_filters
            draw_status_filters(self)
            if action_button("Clear filters"):
                self._query, self._group, self._origin, self._category_filter, self._enabled_only = "", "", "", None, False
                from creation_lib.mod_manager.status_query import normalized
                self._status_filters = normalized()
        imgui.text_wrapped("Groups organize the view. Priority determines file overrides.")

    def _draw_setup(self):
        m = self.manager
        heading("Create your game instance", large=True)
        imgui.text_wrapped("Create a local profile and game executables to install and organize mods.")
        imgui.begin_disabled(m.busy or m.runtime_active)
        ids = [game.id for game in INSTANCE_GAMES]
        imgui.text_unformatted("Game")
        imgui.set_next_item_width(-1)
        changed, index = imgui.combo("##instance_game", ids.index(self._setup_game), [game.name for game in INSTANCE_GAMES])
        if changed:
            self._setup_game = ids[index]
            settings = self._toolkit_settings
            self._setup_root = settings.get_game_paths(self._setup_game).get("root_dir", "") if settings else ""
        imgui.set_next_item_width(-1)
        _, self._setup_root = imgui.input_text_with_hint("##instance_game_root", "Game installation folder", self._setup_root)
        if action_button("Choose game folder"):
            path = pick_folder("Choose the folder containing the game executable and Data", self._setup_root)
            if path:
                self._setup_root = path
        imgui.same_line()
        if action_button("Detect game"):
            def complete(path):
                if path:
                    self._setup_root = path
                    m.status = "Game installation detected. Review the path before creating your instance."
                else:
                    m.status = "Game installation was not found. Choose its folder manually."
            def detect(cancel, progress):
                from creation_lib.core.path_detector import detect_game_path
                return detect_game_path(self._setup_game)
            self._action(lambda: m.submit("Detecting game installation", detect, complete))
        imgui.set_next_item_width(-1)
        _, self._library_path = imgui.input_text_with_hint("##instance_library_root", "Manager library (leave blank for automatic location)", self._library_path)
        imgui.set_item_tooltip(f"Automatic library: {default_library_path()}")
        if action_button("Choose library folder"):
            path = pick_folder("Choose an empty folder or a manager-owned library", self._library_path)
            if path:
                self._library_path = path
        imgui.set_next_item_width(-1)
        imgui.text_unformatted("Initial profile")
        imgui.set_next_item_width(-1)
        _, self._setup_name = imgui.input_text("##instance_profile", self._setup_name)
        if action_button("Create instance", primary=True, enabled=bool(self._setup_root and self._setup_name.strip())):
            self._action(lambda: m.create_instance(self._setup_game, self._setup_root, self._library_path, self._setup_name))
            self._dialog = ""
        game = INSTANCE_GAMES[index]
        imgui.text_wrapped("Game setup is read only. Mods and profile metadata stay in the manager library.")
        if not game.native:
            imgui.text_wrapped("Native modded launch for this game remains unavailable.")
        else:
            imgui.text_wrapped("Native launch uses this project's virtual filespace; initial INIs require launch review.")
        imgui.end_disabled()

    def _draw_profile_selector(self):
        m = self.manager
        profile = next((profile for profile in m.profiles if profile.id == m.profile_id), None)
        imgui.begin_disabled(m.busy)
        imgui.set_next_item_width(min(scaled(220), max(scaled(100), imgui.get_content_region_avail().x - scaled(160))))
        if imgui.begin_combo("##profile", "Profile: " + profile.name if profile else "No profile"):
            for item in m.profiles:
                imgui.push_id(item.id)
                if imgui.selectable("##choose_profile", item.id == m.profile_id)[0]:
                    self._action(lambda key=item.id: m.select_profile(key))
                cursor = imgui.get_item_rect_min()
                imgui.get_window_draw_list().add_text(cursor, imgui.get_color_u32(imgui.Col_.text), item.name)
                imgui.set_item_tooltip(f"{item.name}\nLocal identity: {item.id}\nSource metadata: {item.source or 'Local profile'}")
                imgui.pop_id()
            imgui.end_combo()
        imgui.same_line()
        if action_button("Profiles") or self._open_profiles:
            self._open_profiles = False
            imgui.open_popup("profile_actions")
        if imgui.begin_popup("profile_actions"):
            if imgui.menu_item("New profile", "", False, bool(m.library))[0]:
                self._dialog = "New profile"
            if imgui.menu_item("Rename profile...", "", False, bool(profile))[0]:
                self._profile_name, self._dialog = profile.name, "Rename profile"
            if imgui.menu_item("Delete profile...", "", False, bool(profile))[0]:
                self._delete_profile_id, self._delete_profile_ack = profile.id, False
                self._dialog = "Delete profile"
            if imgui.menu_item("Removed profiles", "", False, bool(m.deleted_profiles))[0]:
                self._dialog = "Removed profiles"
            imgui.end_popup()
        imgui.end_disabled()

    def _draw_new_profile(self):
        m = self.manager
        heading("Create local profile")
        imgui.set_next_item_width(-1)
        _, self._profile_name = imgui.input_text_with_hint("##profile_name", "New profile name", self._profile_name)
        _, self._new_profile_clone = imgui.checkbox("Clone current profile", self._new_profile_clone)
        imgui.text_wrapped("Clone copies mod organization and manager profile settings, including saved launch-target settings. Save and output folders stay separate; use the save copy/backup controls to transfer saves.")
        if action_button("Create profile", enabled=bool(m.library and self._profile_name) and not m.busy):
            self._action(lambda: m.create_profile(self._profile_name, self._new_profile_clone and bool(m.profile_id)))
            self._dialog = ""

    def _draw_new_group(self):
        heading("Create an organization group")
        imgui.set_next_item_width(-1)
        _, self._new_group_name = imgui.input_text_with_hint("##new_group_name", "Group or nested path, such as Visual/Textures", self._new_group_name)
        if action_button("Create group", primary=True, enabled=bool(self._new_group_name.strip()) and not self.manager.busy):
            self._action(lambda: self.manager.create_group(self._new_group_name.split("/")[-1], self._new_group_name))
            self._dialog = ""

    def _draw_rename_profile(self):
        heading("Rename local profile")
        imgui.set_next_item_width(-1)
        _, self._profile_name = imgui.input_text("##rename_profile", self._profile_name)
        if action_button("Save profile name", primary=True, enabled=bool(self._profile_name.strip()) and not self.manager.busy):
            self._action(lambda: self.manager.rename_profile(self._profile_name))
            self._dialog = ""
        imgui.same_line()
        if action_button("Cancel rename"):
            self._dialog = ""

    def _draw_delete_profile(self):
        m = self.manager
        target = next((profile for profile in m.profiles if profile.id == self._delete_profile_id), None)
        heading("Remove local profile")
        if not target:
            imgui.text_wrapped("This local profile is no longer available.")
            return
        imgui.text_wrapped(f"Delete local profile: {target.name}\nIdentity: {target.id}")
        imgui.text_wrapped("Only this manager's profile metadata and organization are removed. Its MO2 profile, mod folders, shared mods and game files stay unchanged. Restore is available from Profiles > Removed profiles. Repeat MO2 import will keep it removed.")
        _, self._delete_profile_ack = imgui.checkbox("Remove only this local profile", self._delete_profile_ack)
        if action_button("Delete local profile", enabled=self._delete_profile_ack and not m.busy):
            self._action(lambda: m.delete_profile(target.id))
            self._dialog = ""
        imgui.same_line()
        if action_button("Cancel"):
            self._delete_profile_ack, self._dialog = False, ""

    def _draw_removed_profiles(self):
        heading("Removed local profiles")
        imgui.text_wrapped("Restore saved local organization. Source MO2 folders are never changed.")
        for profile in self.manager.deleted_profiles:
            imgui.push_id(profile["id"])
            imgui.text_unformatted(profile["name"])
            if action_button("Restore profile", enabled=not self.manager.busy):
                self._action(lambda key=profile["id"]: self.manager.restore_profile(key))
                self._dialog = ""
            imgui.pop_id()

    def _draw_migration_summary(self):
        m = self.manager
        result = m.instance_migration
        if result:
            if imgui.collapsing_header("Migration coverage and details"):
                imgui.text_wrapped(f"{result.original_profiles} source profiles | {len(result.referenced)} original folder references | {len(result.unavailable)} unavailable. No mod payloads copied or hashed.")
                imgui.text_wrapped(f"Download metadata: {result.downloads} items. Unassigned mods have a disabled local profile. MO2 overwrite output is excluded; create a named source mod if needed.")
                if result.game_content:
                    imgui.text_wrapped(f"{len(result.game_content)} game-directory entries excluded from managed mods. Original plugin/order snapshots are preserved.")
                if result.retired_profiles:
                    imgui.text_wrapped("Locally removed profiles stay removed: " + ", ".join(result.retired_profiles))
                for issue in result.issues[:100]:
                    imgui.text_wrapped(issue)
                for name in result.unavailable[:100]:
                    imgui.text_wrapped(f"Unavailable: {name}")
                imgui.text_wrapped("Excluded from import: game/DLC/unmanaged files, download archive payloads, caches, saves, credentials and MO2 plugins. Configure executables for the bounded x64 FO4 runtime; Run prepares this project's native virtual filespace.")
        unfinished = next((run for run in m.migration_runs if run["state"] in {"importing", "cancelled", "failed", "partial"}), None)
        if unfinished and not m.busy:
            imgui.text_wrapped(f"Import needs attention: {unfinished['source']}. Retry preserves local organization and source folders.")
            if action_button("Resume migration"):
                self._mo2_path = unfinished["source"]
                def resume():
                    m.migrate_mo2(self._mo2_path, str(m.library.root), self._source_mappings)
                    self._one_click_migration = self._show_imported_profiles = True
                self._action(resume)

    def _draw_library(self):
        self._draw_mod_rows()

    def _draw_mod_rows(self):
        m = self.manager
        rows = m.filtered(self._query, self._group, self._enabled_only, sort_by=self._sort_by, descending=self._descending, origin=self._origin, category=self._category_filter, statuses=self._status_filters)
        if not m.library:
            heading("Your mod library")
            imgui.text_wrapped("Create a standalone game instance to install and manage mods. MO2 import is available in the Mods menu.")
            return
        key = (id(m.rows), id(rows))
        if key != self._tree_key:
            self._tree_key, self._tree = key, ModTree(m.rows, rows)
            self._tree_sources = (m.rows, rows)
            self._tree_mod_count = sum(row.kind != 'separator' for row in rows)
            self._visible_tree_key = None
        imgui.text_disabled(f"{self._tree_mod_count:,} mods   /   {m.enabled_count:,} enabled")
        imgui.set_item_tooltip("Higher library priority wins file conflicts. Drag mods to change priority; header sorting only changes the view.")
        closed = self._closed_groups.setdefault(m.profile_id, set())
        reveal = bool(self._query or self._group or self._enabled_only or self._origin or self._category_filter is not None or any(value != "all" for value in self._status_filters.values()))
        visible_key = (key, frozenset(closed), reveal)
        if visible_key != self._visible_tree_key:
            self._visible_tree_key = visible_key
            self._visible_tree = self._tree.visible(closed, reveal=reveal)
        if self._status_index is not m.conflict_index:
            self._status_index, self._file_states = m.conflict_index, file_states(m.conflict_index)
        nodes = self._visible_tree
        if self._selection_profile != m.profile_id:
            self._selection_profile = m.profile_id
            self._selected_mods.clear()
        self._selected_mods.intersection_update(m.rows_by_id)
        draw_tree(self, nodes, closed, reveal)
        handle_list_keys(self, nodes, closed)

    def _draw_import(self):
        m = self.manager
        heading("Import MO2 instance")
        imgui.text_wrapped("Import all profiles, mod order, enabled states, groups and configuration snapshots. Mods remain referenced in their original MO2 folders.")
        imgui.set_next_item_width(-1)
        imgui.begin_disabled(m.busy)
        _, self._mo2_path = imgui.input_text_with_hint("##mo2_path", "MO2 instance directory", self._mo2_path)
        imgui.end_disabled()
        if action_button("Choose folder", enabled=not m.busy):
            path = pick_folder("Choose your MO2 instance directory", self._mo2_path)
            if path:
                self._mo2_path = path
        imgui.same_line()
        if action_button("Choose INI", enabled=not m.busy):
            path = pick_file("Choose ModOrganizer.ini", [("MO2 configuration", "*.ini")])
            if path:
                self._mo2_path = path
        imgui.text_wrapped(f"Destination: {m.resolve_destination(self._library_path)}")
        if action_button("Import instance", enabled=bool(self._mo2_path) and not m.busy):
            def migrate():
                m.migrate_mo2(self._mo2_path, self._library_path, self._source_mappings)
                self._one_click_migration = self._show_imported_profiles = True
            self._action(migrate)
        imgui.text_wrapped("No mod files are copied, moved or hashed. Ordinary local mod-root links are resolved safely. Game-directory content is excluded; game launch remains unconfigured.")
        if not imgui.collapsing_header("Advanced import options"):
            return
        with section("import_destination", "Optional destination"):
            imgui.begin_disabled(m.busy)
            imgui.set_next_item_width(-1)
            _, self._library_path = imgui.input_text_with_hint("##library_path", "Optional destination; leave blank for automatic library", self._library_path)
            if action_button("Browse destination"):
                path = pick_folder("Choose a destination or an existing manager library", self._library_path)
                if path:
                    self._library_path = path
            imgui.same_line()
            if action_button("Use default location"):
                self._library_path = str(default_library_path())
            imgui.end_disabled()
            imgui.text_wrapped("Missing folders are created automatically. Existing files require a manager-owned library.")
        if action_button("Preview import", enabled=bool(self._mo2_path) and not m.busy):
            self._action(lambda: m.inspect_mo2(self._mo2_path, self._source_mappings))
        preview = m.import_preview
        if not preview:
            return
        if self._import_id is not preview:
            previous = self._import_id
            self._import_id = preview
            keys = {profile.id for profile in preview.profiles}
            if previous and previous.root == preview.root:
                old_keys = {profile.id for profile in previous.profiles}
                self._import_selected = (self._import_selected & keys) | (keys - old_keys)
            else:
                self._import_selected = keys
        imgui.spacing()
        imgui.text_wrapped("Selected-profile import below stores metadata and source references only. Import instance above includes all detected profiles and unassigned mods. MO2 overwrite output is excluded.")
        with section("import_summary", "Import preview"):
            imgui.text_wrapped(f"Instance: {preview.root}\nMods: {preview.mods_root}\nProfiles: {preview.profiles_root}\nGame: {preview.game or 'Not specified'}")
            for profile in preview.profiles:
                imgui.begin_disabled(m.busy)
                imgui.push_id(profile.id)
                changed, value = imgui.checkbox(f"{profile.name} ({len(profile.mods):,} entries)###include_profile", profile.id in self._import_selected)
                imgui.pop_id()
                if changed:
                    self._import_selected.add(profile.id) if value else self._import_selected.discard(profile.id)
                imgui.end_disabled()
            if action_button("Import selected profiles", primary=True, enabled=bool(self._import_selected) and not m.busy):
                def commit():
                    m.import_profiles(self._import_selected, self._library_path)
                    self._show_imported_profiles = True
                self._action(commit)
            if not self._import_selected:
                imgui.text_wrapped("Select at least one profile to import.")
            pending = sum(not linked.source for linked in preview.linked_sources)
            if pending:
                status_indicator(f"{pending:,} linked sources need approval. Expand Linked mod sources below to review them.", "warning")
        if preview.linked_sources and imgui.collapsing_header(f"Linked mod sources ({len(preview.linked_sources):,})###linked_sources"):
            self._draw_source_mappings(preview)
        if imgui.collapsing_header(f"Import notes ({len(preview.warnings):,})"):
            for warning in preview.warnings[:100]:
                imgui.text_wrapped(warning)
            if len(preview.warnings) > 100:
                imgui.text_disabled("Additional warnings are available in the import preview service result.")
        if imgui.collapsing_header("Instance configuration (review only)"):
            self._show_instance_configuration(preview.configuration)

    def _invalidate_install(self):
        self._install_cache = None
        self.manager.install_plan = None
        self.manager.install_base_pairs, self.manager.install_mapping = [], {}

    def _draw_source_mappings(self, preview):
        m = self.manager
        with section("source_mapping_review", "Linked mod sources"):
            imgui.text_wrapped("MO2 source links require your explicit approval. Review the target or choose a replacement ordinary folder. Approval creates a read-only reference; it never changes the link or copies its files. Archive links remain unsupported.")
            imgui.begin_disabled(m.busy)
            for linked in preview.linked_sources:
                imgui.push_id(identity(linked.alias))
                imgui.text_unformatted(linked.name)
                imgui.text_wrapped(f"Link: {linked.alias}\nTarget: {linked.target or 'Unknown'}")
                status_indicator(linked.status, "success" if linked.source else "warning")
                if linked.source:
                    imgui.text_wrapped(f"Approved source: {linked.source}")
                if action_button("Use target read-only", enabled=linked.can_use_target and linked.source != linked.target):
                    self._source_mappings[linked.alias] = linked.target
                    self._action(lambda: m.inspect_mo2(self._mo2_path, self._source_mappings))
                imgui.same_line()
                if action_button("Map another folder"):
                    path = pick_folder(f"Choose a read-only source for {linked.name}", linked.target)
                    if path:
                        self._source_mappings[linked.alias] = path
                        self._action(lambda: m.inspect_mo2(self._mo2_path, self._source_mappings))
                imgui.pop_id()
            imgui.end_disabled()

    def _draw_categories(self):
        m = self.manager
        heading("Library categories", large=True)
        imgui.text_wrapped("Categories are shared across local profiles. Import the complete Nexus game catalog, including parent categories, or manage local categories below. Renames preserve mod assignments and Nexus mappings.")
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.text_unformatted(f"Nexus game: {m.nexus_game}")
        if action_button("Import Nexus categories", enabled=m.nexus_client is not None):
            self._action(m.import_nexus_categories)
        imgui.same_line()
        if action_button("Import cached categories"):
            self._action(lambda: m.import_nexus_categories(refresh=False))
        if action_button("Refresh categories"):
            self._action(m.inspect_categories)
        imgui.begin_child("library_categories", (0, scaled(170)), imgui.ChildFlags_.borders)
        try:
            clipper = imgui.ListClipper()
            clipper.begin(len(m.categories))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    item = m.categories[index]
                    imgui.push_id(item.name.casefold())
                    if imgui.selectable(f"{item.name} | {item.usage:,} mods###library_category", self._category_selected == item.name)[0]:
                        self._category_selected, self._category_name, self._category_nexus_id = item.name, item.name, item.nexus_id
                    imgui.pop_id()
        finally:
            imgui.end_child()
        imgui.set_next_item_width(-1)
        _, self._category_name = imgui.input_text_with_hint("##category_name", "New name or replacement category path", self._category_name)
        imgui.set_next_item_width(-1)
        _, self._category_nexus_id = imgui.slider_int("Nexus category ID (optional)", self._category_nexus_id, 0, 1_000_000)
        if action_button("Save category", enabled=bool(self._category_name)):
            self._action(lambda: m.edit_category("save", self._category_name, nexus_id=self._category_nexus_id))
        if self._category_selected:
            if action_button("Rename selected category", enabled=bool(self._category_name)):
                self._action(lambda: m.edit_category("rename", self._category_selected, self._category_name))
                self._category_selected = self._category_name
            if action_button("Remove unused category"):
                self._action(lambda: m.edit_category("remove", self._category_selected))
                self._category_selected = ""
        imgui.end_disabled()

    def _draw_save_copy(self):
        m = self.manager
        external = bool(m.save_index and m.save_index.external)
        heading("Copy source save into library" if external else "Copy an owned save", large=True)
        imgui.text_wrapped("Copies this save with its existing extender companion. Source files remain intact. Profile destinations must use the same game and have no matching filenames; save compatibility is unverified.")
        imgui.text_unformatted(Path(self._save_copy_path).name)
        profiles = [profile for profile in m.profiles if external or profile.id != m.profile_id]
        ids, labels = ["", *[profile.id for profile in profiles]], ["New owned backup bundle", *[profile.name for profile in profiles]]
        if self._save_copy_target not in ids:
            self._save_copy_target = ""
        imgui.begin_disabled(m.busy or m.runtime_active)
        changed, selected = imgui.combo("Destination", ids.index(self._save_copy_target), labels)
        if changed:
            self._save_copy_target = ids[selected]
            m.save_transfer_plan, self._save_copy_ack = None, False
        if action_button("Review save copy", enabled=bool(self._save_copy_path)):
            self._save_copy_ack = False
            self._action(lambda: m.review_save_copy(self._save_copy_path, self._save_copy_target))
        plan = m.save_transfer_plan
        if plan:
            for item in plan.files:
                imgui.text_unformatted(f"{item.name} | {item.size:,} bytes")
                imgui.set_item_tooltip(item.checksum)
            imgui.text_wrapped("The file names, source sizes, timestamps and SHA-256 checksums are checked again during staging. An interrupted placement retains a journal and finishes the pair on reopen.")
            _, self._save_copy_ack = imgui.checkbox("Create these reviewed owned copies", self._save_copy_ack)
            if action_button("Copy reviewed save pair", primary=True, enabled=self._save_copy_ack):
                self._action(m.apply_save_copy)
                self._save_copy_ack = False
            imgui.same_line()
            if action_button("Cancel save copy"):
                m.save_transfer_plan, self._save_copy_ack = None, False
        imgui.end_disabled()

    def _draw_source_saves(self):
        m = self.manager
        heading("Browse source / global saves", large=True)
        imgui.text_wrapped("Choose a save folder explicitly. This browser reads its files; copying a reviewed pair creates owned library files. Source/global saves remain intact. The selected local profile supplies game context; header/body compatibility is unverified.")
        imgui.begin_disabled(m.busy or m.runtime_active)
        try:
            imgui.set_next_item_width(-1)
            _, self._save_source_path = imgui.input_text_with_hint("##save_source_folder", "Source MO2 profile/saves or another ordinary save folder", self._save_source_path)
            if action_button("Choose save folder"):
                folder = pick_folder("Choose source/global save folder", self._save_source_path)
                if folder:
                    self._save_source_path = folder
            imgui.same_line()
            if action_button("Read selected save folder", primary=True, enabled=bool(self._save_source_path)):
                self._action(lambda: m.inspect_source_saves(self._save_source_path))
            index = m.save_index
            if index and index.external:
                imgui.text_wrapped(f"{len(index.files):,} source saves | {index.root}")
                if action_button("Show source saves"):
                    self._right_pane, self._dialog = "Saves", ""
        finally:
            imgui.end_disabled()

    def _draw_save_backups(self):
        m = self.manager
        heading("Retained save backups", large=True)
        imgui.text_wrapped("Restore a reviewed save and extender companion into this local profile. Matching destination names block restoration. The retained bundle stays intact; compatibility remains unverified.")
        imgui.begin_disabled(m.busy or m.runtime_active)
        try:
            imgui.begin_child("save_backup_details", (0, -imgui.get_frame_height_with_spacing() * 3))
            try:
                if action_button("Refresh save backups"):
                    self._save_backup_ack = False
                    self._action(m.inspect_save_backups)
                backups = m.save_backups
                if self._save_backup_selected not in {item.id for item in backups}:
                    self._save_backup_selected = ""
                imgui.text_disabled(f"{len(backups):,} retained bundles")
                plan = m.save_transfer_plan
                detail_height = len(plan.files) * imgui.get_text_line_height_with_spacing() if plan and plan.source_backup else 0
                list_height = min(scaled(140), max(scaled(60), imgui.get_content_region_avail().y - detail_height - imgui.get_style().item_spacing.y * 2))
                imgui.begin_child("save_backup_list", (0, list_height), imgui.ChildFlags_.borders)
                try:
                    clipper = imgui.ListClipper()
                    clipper.begin(len(backups), imgui.get_frame_height_with_spacing())
                    while clipper.step():
                        for index in range(clipper.display_start, clipper.display_end):
                            backup = backups[index]
                            imgui.push_id(backup.id)
                            if imgui.selectable(f"{backup.names[0]} | {backup.id[:8]}###save_backup", self._save_backup_selected == backup.id)[0]:
                                self._save_backup_selected = backup.id
                                m.save_transfer_plan, self._save_backup_ack = None, False
                            imgui.set_item_tooltip(f"{sum(backup.sizes):,} bytes\n" + "\n".join(backup.names))
                            imgui.pop_id()
                finally:
                    imgui.end_child()
                plan = m.save_transfer_plan
                if plan and plan.source_backup:
                    for item in plan.files:
                        imgui.text_unformatted(f"{item.name} | {item.size:,} bytes")
                        imgui.set_item_tooltip(item.checksum)
            finally:
                imgui.end_child()
            if action_button("Review save backup restore", enabled=bool(self._save_backup_selected)):
                self._save_backup_ack = False
                self._action(lambda: m.review_save_backup(self._save_backup_selected))
            imgui.same_line()
            if action_button("Review save backup deletion", enabled=bool(self._save_backup_selected)):
                from .backup_prune_ui import open_review
                self._action(lambda: open_review(self, "save", self._save_backup_selected))
            plan = m.save_transfer_plan
            if plan and plan.source_backup:
                _, self._save_backup_ack = imgui.checkbox("Restore these reviewed copies into this profile", self._save_backup_ack)
                if action_button("Restore reviewed save pair", primary=True, enabled=self._save_backup_ack):
                    self._action(m.apply_save_copy)
                    self._save_backup_ack = False
                imgui.same_line()
                if action_button("Cancel save restore"):
                    m.save_transfer_plan, self._save_backup_ack = None, False
        finally:
            imgui.end_disabled()

    def _draw_profile_backups(self):
        m = self.manager
        profile = next((profile for profile in m.profiles if profile.id == m.profile_id), None)
        heading("Profile metadata backups", large=True)
        if not profile:
            imgui.text_wrapped("Choose a local profile first.")
            return
        imgui.text_unformatted(profile.name)
        imgui.text_wrapped("Backups contain local activation/order, INIs and mod organization. They reference existing mod identities without copying or inspecting payloads. Restore keeps a backup of the current state.")
        imgui.begin_disabled(m.busy or m.runtime_active)
        if action_button("Back up current profile", primary=True):
            self._action(m.backup_profile)
        records = m.profile_backups
        if self._backup_selected not in {record["id"] for record in records}:
            self._backup_selected = records[0]["id"] if records else ""
        delete_key = (str(m.context_library.root), profile.id, self._backup_selected)
        if delete_key != self._backup_delete_key:
            self._backup_delete_key, self._backup_delete_ack = delete_key, False
        imgui.begin_child("profile_backup_list", (0, scaled(170)), imgui.ChildFlags_.borders)
        clipper = imgui.ListClipper()
        clipper.begin(len(records))
        while clipper.step():
            for index in range(clipper.display_start, clipper.display_end):
                record = records[index]
                imgui.push_id(record["id"])
                if imgui.selectable(f"{record['created']} UTC | {record['characters']:,} characters###profile_backup", self._backup_selected == record["id"])[0]:
                    self._backup_selected = record["id"]
                    self._backup_delete_ack = False
                    m.profile_restore = None
                imgui.pop_id()
        imgui.end_child()
        if action_button("Review backup restore", enabled=bool(self._backup_selected)):
            self._action(lambda: m.review_profile_backup(self._backup_selected))
        plan = m.profile_restore
        if plan:
            key = (plan.profile_id, plan.backup_id, plan.expected)
            if key != self._backup_plan_key:
                self._backup_plan_key, self._backup_restore_ack = key, False
            imgui.text_wrapped(f"Restore {plan.name}: {plan.entries:,} mod entries and {plan.files:,} configuration snapshots. Existing mod folders remain referenced.")
            _, self._backup_restore_ack = imgui.checkbox("Replace this local profile's configuration and organization", self._backup_restore_ack)
            if action_button("Restore reviewed profile backup", primary=True, enabled=self._backup_restore_ack):
                self._action(m.restore_profile_backup)
            imgui.same_line()
            if action_button("Cancel backup restore"):
                m.profile_restore, self._backup_restore_ack = None, False
        imgui.separator()
        _, self._backup_delete_ack = imgui.checkbox("Remove the selected metadata backup", self._backup_delete_ack)
        if action_button("Remove selected backup", enabled=bool(self._backup_selected) and self._backup_delete_ack):
            self._action(lambda: m.remove_profile_backup(self._backup_selected))
            self._backup_delete_ack = False
        imgui.end_disabled()

    def _draw_overwrite(self):
        m = self.manager
        heading("Owned runtime overwrite", large=True)
        imgui.text_wrapped("Manage only this local profile's generated output. MO2 overwrite is excluded from imports. Copy creates an owned mod and keeps the output intact; clearing moves the complete folder into retained backup. Restore requires an empty output folder.")
        if not m.library or not m.profile_id:
            imgui.text_wrapped("Choose a local profile first.")
            return
        imgui.begin_disabled(m.busy or m.runtime_active)
        try:
            if action_button("Review owned output", primary=True):
                self._action(m.inspect_overwrite)
            plan = m.overwrite_review
            current_key = (str(m.context_library.root), m.profile_id, plan.token if plan else "", self._overwrite_selected)
            if current_key != self._overwrite_key:
                self._overwrite_key = current_key
                self._overwrite_ack, self._overwrite_restore_ack = False, False
            if plan:
                imgui.text_disabled(f"{plan.files:,} files | {plan.bytes:,} bytes | {len(plan.directories):,} directories")
                imgui.set_next_item_width(-1)
                _, self._overwrite_name = imgui.input_text("New output mod name", self._overwrite_name)
                if action_button("Copy output to owned mod", enabled=bool(plan.files and self._overwrite_name.strip())):
                    self._action(lambda: m.change_overwrite("copy", self._overwrite_name))
                _, self._overwrite_ack = imgui.checkbox("Keep a complete backup and clear this owned output", self._overwrite_ack)
                if action_button("Clear output with backup", enabled=self._overwrite_ack and bool(plan.files or plan.directories)):
                    self._action(lambda: m.change_overwrite("clear"))
            imgui.separator()
            records = m.overwrite_history
            if self._overwrite_selected not in {record["id"] for record in records}:
                self._overwrite_selected = records[0]["id"] if records else ""
            imgui.begin_child("overwrite_backup_list", (0, scaled(120)), imgui.ChildFlags_.borders)
            try:
                clipper = imgui.ListClipper()
                clipper.begin(len(records))
                while clipper.step():
                    for index in range(clipper.display_start, clipper.display_end):
                        record = records[index]
                        imgui.push_id(record["id"])
                        if imgui.selectable(f"{record['created']} | {record['files']:,} files | {record['state']}###overwrite_backup", self._overwrite_selected == record["id"])[0]:
                            self._overwrite_selected = record["id"]
                            m.overwrite_restore, self._overwrite_restore_ack = None, False
                        imgui.pop_id()
            finally:
                imgui.end_child()
            selected = next((record for record in records if record["id"] == self._overwrite_selected), None)
            if action_button("Review output backup restore", enabled=bool(selected and selected["state"] == "complete")):
                self._action(lambda: m.review_overwrite_restore(self._overwrite_selected))
            imgui.same_line()
            if action_button("Review output backup deletion", enabled=bool(selected)):
                from .backup_prune_ui import open_review
                self._action(lambda: open_review(self, "overwrite", self._overwrite_selected))
            if m.overwrite_restore:
                _, self._overwrite_restore_ack = imgui.checkbox("Restore this backup into empty owned output", self._overwrite_restore_ack)
                if action_button("Restore reviewed output backup", enabled=self._overwrite_restore_ack, primary=True):
                    self._action(m.restore_overwrite)
                imgui.same_line()
                if action_button("Cancel output restore"):
                    m.overwrite_restore, self._overwrite_restore_ack = None, False
        finally:
            imgui.end_disabled()

    def _draw_profile_configuration(self):
        m = self.manager
        profile = next((profile for profile in m.profiles if profile.id == m.profile_id), None)
        heading("Profile configuration", large=True)
        if not profile:
            imgui.text_wrapped("Import or create a profile, then select it in Profiles.")
            return
        imgui.text_unformatted(profile.name)
        imgui.text_wrapped("Profile INIs can be edited locally. Activation and load order use the Plugins pane. Native launch maps local configuration without writing global game or MO2 profile files.")
        filenames = sorted(name for name in profile.files if not name.startswith("_"))
        imgui.push_id(profile.id)
        try:
            if filenames:
                if self._profile_filename not in filenames:
                    self._profile_filename = filenames[0]
                changed, index = imgui.combo("Profile file", filenames.index(self._profile_filename), filenames)
                if changed:
                    self._profile_filename = filenames[index]
                imgui.push_id(identity(self._profile_filename))
                try:
                    if self._profile_filename.casefold().endswith(".ini"):
                        key = (str(m.context_library.root), profile.id, self._profile_filename, profile.files[self._profile_filename])
                        if key != self._ini_key:
                            self._ini_key, self._ini_expected = key, profile.files[self._profile_filename]
                            from creation_lib.mod_manager.profile_state import effective_ini
                            self._ini_text = effective_ini(m.context_library, profile, self._profile_filename)
                        imgui.begin_disabled(m.busy or m.runtime_active)
                        _, self._ini_text = imgui.input_text_multiline("##profile_ini_editor", self._ini_text, (-1, scaled(280)))
                        if action_button("Save local INI", primary=True):
                            self._action(lambda: m.save_profile_ini(self._profile_filename, self._ini_text, self._ini_expected))
                        imgui.same_line()
                        if action_button("Reload INI"):
                            self._ini_key = None
                        imgui.same_line()
                        if action_button("Restore previous INI", enabled=self._profile_filename in json.loads(profile.files.get("_modbox_ini_backup", "{}"))):
                            self._action(lambda: m.restore_profile_ini(self._profile_filename, self._ini_expected))
                        imgui.end_disabled()
                        imgui.text_wrapped("Edits retain a previous local snapshot. Game-edited runtime settings are shown while their baseline matches this profile; save routing is reasserted by the native helper.")
                    else:
                        imgui.input_text_multiline("##profile_snapshot", profile.files[self._profile_filename],
                                                   (-1, scaled(280)), imgui.InputTextFlags_.read_only)
                finally:
                    imgui.pop_id()
            else:
                imgui.text_wrapped("This local profile has no imported configuration files.")
            if imgui.collapsing_header("Instance configuration (review only)"):
                self._show_instance_configuration(m.instance_configuration)
        finally:
            imgui.pop_id()

    @staticmethod
    def _show_instance_configuration(configuration):
        if not configuration:
            imgui.text_wrapped("This profile has no imported instance configuration.")
        for section_name, values in configuration.items():
            heading(section_name.replace("_", " ").title())
            if isinstance(values, dict):
                for name, value in values.items():
                    imgui.text_wrapped(f"{name}: {value}")
            elif isinstance(values, list):
                for executable in values:
                    imgui.text_wrapped(f"{executable.get('title', 'Executable')}: {executable.get('binary', '')}")
                    imgui.text_wrapped(f"Arguments: {executable.get('arguments', '')}\nWorking directory: {executable.get('workingdirectory', '')}")
        imgui.text_wrapped("Executable definitions are available in the launcher and Executables editor. Import never runs commands; game paths remain local to this manager unless configured separately in toolkit Settings.")

    def _draw_install(self):
        m = self.manager
        if not m.prepared:
            heading("Install a mod")
            imgui.text_wrapped("Open a ZIP, 7z, RAR archive or unpacked mod folder. FOMOD options are shown when available. Choose the content root for a plain package. 7z/RAR require an installed 7-Zip or NanaZip tool.")
            imgui.begin_disabled(m.busy or m.runtime_active)
            candidates = [("", "New owned mod")] + [(row.id, "Additive update: " + row.name) for row in m.rows if row.kind in {"external", "reference"} and row.source]
            selected = getattr(self, "_install_overlay", "")
            index = next((i for i, item in enumerate(candidates) if item[0] == selected), 0)
            imgui.set_next_item_width(-1)
            changed, index = imgui.combo("Installation destination", index, [item[1] for item in candidates])
            if changed:
                self._install_overlay = candidates[index][0]
            _, self._manual_install = imgui.checkbox("Manual layout (skip FOMOD rules)", self._manual_install)
            imgui.set_item_tooltip("Explicitly choose payload roots/variants and destinations. No FOMOD rules or installer scripts run; installer metadata is excluded.")
            imgui.end_disabled()
            if action_button("Open archive", primary=True, enabled=bool(m.profile_id) and not m.busy):
                self._pick_package()
            imgui.same_line()
            if action_button("Open folder", enabled=bool(m.profile_id) and not m.busy):
                path = pick_folder("Choose unpacked mod package")
                if path:
                    self._prepare(path)
            return
        prepared = m.prepared
        footer_height = imgui.get_frame_height_with_spacing() * (4 if prepared.fomod else 3)
        imgui.begin_child("installer_details", (0, -footer_height))
        try:
            self._draw_install_details()
        finally:
            imgui.end_child()
        if m.prepared is not prepared:
            return
        imgui.begin_disabled(m.busy)
        try:
            if prepared.fomod and self._evaluation:
                from creation_lib.mod_manager.installer_wizard import can_advance
                if action_button("Back", enabled=self._step > 0):
                    self._step -= 1
                imgui.same_line()
                if action_button("Next", enabled=can_advance(self._evaluation, self._step)):
                    self._step += 1
                from .toolbar_ui import _next_button
                _next_button("Reset current step choices")
                if action_button("Reset current step choices", enabled=self._step < len(self._evaluation.steps)):
                    for group in self._evaluation.steps[self._step].groups:
                        m.choices.pop(group.id, None)
                    self._invalidate_install()
            if action_button("Review destination files", primary=True):
                self._action(lambda: m.review_install(self._content_root))
            imgui.same_line()
            if action_button("Close installer"):
                self._action(m.discard_install)
            if m.install_plan:
                label = (("Merge with backup" if m.install_mode == "merge" else "Reinstall with backup")
                         if prepared.replacement_review else "Install into library")
                if action_button(label, primary=True,
                                 enabled=bool(self._install_name and m.profile_id) and (not prepared.replacement_review or self._replacement_ack)):
                    self._action(lambda: m.install(self._install_name, self._content_root))
                imgui.same_line()
                imgui.text_disabled(f"{len(m.install_pairs):,} reviewed files")
        finally:
            imgui.end_disabled()

    def _draw_install_details(self):
        m = self.manager
        prepared = m.prepared
        if prepared.fomod:
            from .installer_decoration_ui import draw as draw_header
            draw_header(prepared)
        else:
            heading("Plain package")
        imgui.text_disabled(f"{len(prepared.package.files):,} source files | {prepared.package.path}")
        if action_button("Reopen package and dependencies", enabled=not m.busy):
            self._prepare(prepared.package.path, prepared.replacement_review.mod_id if prepared.replacement_review else "")
            return
        if prepared.manual:
            imgui.text_wrapped("Manual layout: FOMOD rules were skipped. Select the correct Data root and variants, then review every destination. Installer scripts and metadata are excluded.")
        elif prepared.fomod and action_button("Reopen in manual layout", enabled=not m.busy):
            self._manual_install = True
            self._prepare(prepared.package.path, prepared.replacement_review.mod_id if prepared.replacement_review else "")
            return
        if prepared.metadata and imgui.collapsing_header("Package information"):
            for field, value in prepared.metadata.items():
                if field == "_error":
                    status_indicator("Optional info.xml: " + value, "warning")
                else:
                    imgui.text_wrapped(f"{field}: {value}")
            imgui.text_disabled("Author-supplied metadata; Nexus identity is unverified.")
        imgui.begin_disabled(m.busy)
        imgui.set_next_item_width(-1)
        imgui.text_unformatted("Installed name")
        imgui.begin_disabled(prepared.replacement_review is not None)
        _, self._install_name = imgui.input_text("##installed_name", self._install_name)
        imgui.end_disabled()
        if prepared.overlay_review:
            imgui.text_wrapped("Additive update above " + prepared.overlay_review.name + ". Incoming reviewed files are stored in a new owned mod; unmatched referenced files remain. No original payload is copied. This patch follows its reference's enabled state in this profile and target. You can adjust its priority afterward; other higher-priority mods can still win. File removals and full replacement are unavailable.")
        if prepared.replacement_review:
            with section("replacement_review", "Reinstall owned mod"):
                self._show_owned_review(prepared.replacement_review)
                imgui.text_wrapped("The current folder will be retained in recovery history. Every profile keeps its enabled state, group, and priority; all those profiles use the replacement content.")
                changed, mode = imgui.combo("Content behavior", 1 if m.install_mode == "merge" else 0, ["Replace current files", "Merge; keep unmatched current files"])
                if changed:
                    m.install_mode = "merge" if mode else "replace"
                    m.install_plan = None
                imgui.text_wrapped("Incoming files win path collisions. Merge retains unmatched existing files, including hidden content; Replace uses only the reviewed incoming plan.")
                _, self._replacement_ack = imgui.checkbox("Replace owned content in all affected profiles; keep backup", self._replacement_ack)
        if prepared.fomod:
            policies = ["MO2 (defined compatibility)", "Schema-oriented"]
            imgui.set_next_item_width(-1)
            changed, policy = imgui.combo("Installer compatibility", 0 if prepared.fomod.compatibility == "mo2" else 1, policies)
            imgui.set_item_tooltip("MO2: earlier-step flags, first checked duplicate flag, trimmed flag text, forced SelectAll and legacy encoding. Schema-oriented: same-page fixed-point flags, last duplicate flag, unavailable-choice rejection and strict Unicode. Changing policy resets choices and destination edits.")
            if changed:
                self._action(lambda: m.change_fomod_policy("mo2" if policy == 0 else "schema"))
            imgui.text_disabled("XML decoding: " + prepared.fomod.encoding)
            for notice in prepared.fomod.notices:
                imgui.text_wrapped(notice)
            if imgui.collapsing_header("Dependency environment"):
                imgui.text_wrapped("Known states use loose providers, plugin activation and the bounded FO4 archive load-plan policy. Requested game/matching F4SE versions use read-only metadata when available. Unknown states/versions need explicit input; this does not establish in-game archive attachment or a successful extender launch.")
                if prepared.context:
                    for snapshot in (prepared.context.archives, prepared.context.versions):
                        for issue in snapshot.issues if snapshot else ():
                            imgui.text_wrapped(issue)
                changed, prepared.environment.game_version = imgui.input_text("Game version", prepared.environment.game_version)
                changed2, prepared.environment.fomm_version = imgui.input_text("FOMM compatibility version", prepared.environment.fomm_version)
                if changed or changed2:
                    self._invalidate_install()
                for node, identifier in EXTENDERS.items():
                    if any(True for _ in prepared.fomod.root.iter(node)):
                        changed, value = imgui.input_text(f"{identifier.upper()} version", prepared.environment.extender_versions.get(identifier, ""))
                        if changed:
                            prepared.environment.extender_versions[identifier] = value
                            self._invalidate_install()
                dependencies = {}
                for node in prepared.fomod.root.iter("fileDependency"):
                    name = node.get("file")
                    dependencies.setdefault(name.replace("\\", "/").casefold(), name)
                for key, name in sorted(dependencies.items()):
                    states = ["Unknown", "Missing", "Inactive", "Active"]
                    current = prepared.environment.files.get(key, "Unknown")
                    imgui.text_unformatted(name)
                    imgui.set_next_item_width(-1)
                    imgui.push_id(identity(key))
                    changed, index = imgui.combo("##file_state", states.index(current), states)
                    imgui.pop_id()
                    if changed:
                        if index == 0:
                            prepared.environment.files.pop(key, None)
                        else:
                            prepared.environment.files[key] = states[index]
                        self._invalidate_install()
            signature = (id(prepared), tuple((key, tuple(sorted(value))) for key, value in sorted(m.choices.items())),
                         prepared.environment.game_version, prepared.environment.fomm_version,
                         (id(prepared.environment.files), prepared.environment.files.revision), tuple(sorted(prepared.environment.extender_versions.items())))
            if signature != self._install_cache:
                self._install_cache = signature
                try:
                    self._evaluation = prepared.fomod.evaluate(m.choices, prepared.environment)
                    self._evaluation_error = ""
                except ManagerError as exc:
                    self._evaluation, self._evaluation_error = None, str(exc)
            if self._evaluation_error:
                status_indicator(self._evaluation_error, "error")
            evaluation = self._evaluation
            self.installer_repair_ui.draw(evaluation)
            if evaluation:
                self._step = min(self._step, len(evaluation.steps))
                if self._step < len(evaluation.steps):
                    step = evaluation.steps[self._step]
                    status_indicator(f"Step {self._step + 1} of {len(evaluation.steps)}", "accent")
                    heading(step.name)
                    for group in step.groups:
                        with section(group.id, group.name):
                            imgui.text_disabled(group.type)
                            required_choice = group.type in {"SelectExactlyOne", "SelectAtMostOne"} and any(value.type == "Required" for value in group.options)
                            for option in group.options:
                                imgui.begin_disabled(option.type in {"Required", "NotUsable"} or group.type == "SelectAll" or required_choice)
                                imgui.push_id(option.id)
                                changed, selected = imgui.checkbox(f"{option.name}###choose_option", option.selected)
                                imgui.pop_id()
                                imgui.end_disabled()
                                if option.description:
                                    imgui.text_wrapped(option.description)
                                if option.image:
                                    imgui.push_id(option.id)
                                    if action_button("Preview option artwork"):
                                        self._action(lambda: m.preview_installer_image(option.image))
                                    imgui.pop_id()
                                if option.type in {"Required", "NotUsable", "CouldBeUsable"}:
                                    status_indicator(option.type, "muted")
                                if changed:
                                    chosen = {value.id for value in group.options if value.selected}
                                    if selected:
                                        if group.type in {"SelectExactlyOne", "SelectAtMostOne"}:
                                            chosen = {value.id for value in group.options if value.type == "Required"}
                                        chosen.add(option.id)
                                    else:
                                        chosen.discard(option.id)
                                    m.choices[group.id] = chosen
                                    self._invalidate_install()
                else:
                    heading("Review your installation")
                for error in evaluation.errors:
                    status_indicator(error, "error")
        else:
            imgui.set_next_item_width(-1)
            changed, self._content_root = imgui.input_text_with_hint("Content root", "Empty for archive root, or Data / wrapper/Data", self._content_root)
            if changed:
                self._invalidate_install()
        imgui.spacing()
        if not prepared.replacement_review:
            _, m.enable_on_install = imgui.checkbox("Enable mod after install", m.enable_on_install)
        if prepared.fomod and prepared.fomod.image and action_button("Preview module artwork"):
            self._action(lambda: m.preview_installer_image(prepared.fomod.image))
        if m.installer_image is not None:
            from imgui_bundle import immvision
            changed = m.installer_image is not self._installer_image_seen
            self._installer_image_seen = m.installer_image
            height, width = m.installer_image.shape[:2]
            maximum = min(scaled(220), imgui.get_content_region_avail().x)
            ratio = min(maximum / width, scaled(180) / height, 1)
            immvision.push_color_order_rgb()
            try:
                immvision.image_display("##installer_artwork", m.installer_image, (max(1, int(width * ratio)), max(1, int(height * ratio))), refresh_image=changed)
            finally:
                immvision.pop_color_order()
            imgui.set_item_tooltip(m.installer_image_source + "\nFirst frame; bounded preview.")
            if action_button("Close artwork preview"):
                m.installer_image, m.installer_image_source = None, ""
        self._draw_install_mapping()
        self._draw_install_sources()
        if m.install_plan:
            imgui.text_disabled(f"{len(m.install_pairs):,} reviewed files will be staged into the owned mod folder.")
            if imgui.begin_child("install_file_plan", (0, scaled(220)), imgui.ChildFlags_.borders):
                pairs = m.install_pairs
                clipper = imgui.ListClipper()
                clipper.begin(len(pairs))
                while clipper.step():
                    for i in range(clipper.display_start, clipper.display_end):
                        target, source = pairs[i]
                        imgui.text_unformatted(f"{target}  <-  {source}")
            imgui.end_child()
        imgui.end_disabled()

    def _draw_stage(self):
        m = self.manager
        heading("Stage this profile")
        imgui.text_wrapped("Review enabled mods and create a combined copy inside your library's exports folder. Each export uses a new folder. The separate reviewed launcher uses this project's native virtual filespace.")
        if action_button("Preview files and conflicts", primary=True, enabled=bool(m.profile_id) and not m.busy):
            self._action(m.preview_export)
        plan = m.export_preview
        if not plan:
            return
        imgui.spacing()
        with section("stage_summary", plan.profile_name):
            imgui.text_unformatted(f"{len(plan.files):,} winning files | {len(plan.conflicts):,} conflicts")
            if action_button("Create staged export", primary=True, enabled=not m.busy):
                self._action(m.export)
        if imgui.begin_child("conflict_files", (0, 0), imgui.ChildFlags_.borders):
            conflicts = m.conflict_pairs
            clipper = imgui.ListClipper()
            clipper.begin(len(conflicts))
            while clipper.step():
                for i in range(clipper.display_start, clipper.display_end):
                    path, names = conflicts[i]
                    imgui.text_unformatted(f"{path} | Winner: {names[-1]}")
                    imgui.set_item_tooltip(" < ".join(names))
        imgui.end_child()

    def _draw_migration(self):
        m = self.manager
        heading("Make imported sources independent", large=True)
        imgui.text_wrapped("Copy selected imported mod folders into your owned library. Original folders stay unchanged. All profiles keep the same mod identity, enabled states, groups, and priorities. Each source commits separately; cancelled batches retain completed copies and roll back the current incomplete copy.")
        eligible = [row for row in m.rows if row.kind == "external" and row.source]
        self._migration_selected.intersection_update(row.id for row in eligible)
        imgui.begin_disabled(m.busy)
        if action_button("Select available sources"):
            self._migration_selected = {row.id for row in eligible}
        imgui.same_line()
        if action_button("Clear source selection"):
            self._migration_selected.clear()
        if imgui.begin_child("source_copy_selection", (0, scaled(150)), imgui.ChildFlags_.borders):
            clipper = imgui.ListClipper()
            clipper.begin(len(eligible))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    row = eligible[index]
                    imgui.push_id(row.id)
                    changed, selected = imgui.checkbox("##include_source", row.id in self._migration_selected)
                    if changed:
                        self._migration_selected.add(row.id) if selected else self._migration_selected.discard(row.id)
                    imgui.same_line()
                    imgui.text_unformatted(row.name)
                    imgui.set_item_tooltip(row.source)
                    imgui.pop_id()
        imgui.end_child()
        imgui.text_wrapped(f"{len(self._migration_selected):,} / {len(eligible):,} available imported sources selected. Unmanaged game content is excluded.")
        if action_button("Review selected copies", primary=True, enabled=bool(self._migration_selected)):
            self._action(lambda: m.review_migration([row.id for row in eligible if row.id in self._migration_selected]))
        review = m.migration_review
        if review:
            if self._migration_token is not review:
                self._migration_token, self._migration_ack = review, False
            with section("copy_review", "Copy review"):
                imgui.text_wrapped(f"{len(review.sources):,} sources | {sum(len(item.manifest) for item in review.sources):,} files | {review.bytes:,} payload bytes\nEstimated space with headroom: {review.required:,} bytes\nAvailable at review: {review.free:,} bytes")
                imgui.text_wrapped("The complete folders are copied, including metadata, hidden files, and empty directories. Stored installer scripts stay inert. Source identity is retained; subsequent uninstall/reinstall affects only the owned copy.")
                if imgui.collapsing_header("Affected sources and profiles"):
                    for source in review.sources:
                        imgui.text_wrapped(f"{source.name}: {source.source}\nAffected profiles ({len(source.profiles)}): " + ", ".join(source.profiles[:20]))
                same = {item.mod_id for item in review.sources} == self._migration_selected
                if not same:
                    imgui.text_wrapped("Selection changed. Review the selected copies again.")
                if review.required > review.free:
                    status_indicator("Insufficient space at review. Free space is checked again before copying.", "warning")
                _, self._migration_ack = imgui.checkbox("Copy these sources and preserve every profile reference", self._migration_ack)
                if action_button("Copy into owned library", primary=True, enabled=same and self._migration_ack and review.required <= review.free):
                    self._action(m.migrate)
        imgui.end_disabled()

    def _draw_files(self):
        m = self.manager
        heading("File providers and conflicts", large=True)
        imgui.text_disabled("Loose-file providers; inspect container members separately in Archives.")
        imgui.set_item_tooltip("Game Data, enabled mods and owned runtime overwrite are inventoried on request. Higher priority wins; stable mod identity breaks ties. Disabled and .mohidden files never win.")
        imgui.begin_disabled(m.busy)
        changed, self._include_disabled_files = imgui.checkbox("Include disabled sources", self._include_disabled_files)
        if changed:
            m.file_index = None
        imgui.same_line()
        if action_button("Refresh file inspection", primary=True, enabled=bool(m.profile_id)):
            self._action(lambda: m.inspect_files(self._include_disabled_files))
        imgui.end_disabled()
        index = m.file_index
        if not index:
            imgui.text_wrapped("Refresh inspection to scan available source folders in the background. Organization changes invalidate this view; source files can change externally, so refresh before relying on it.")
            return
        imgui.set_next_item_width(-1)
        _, self._file_query = imgui.input_text_with_hint("##search_files", "Search paths or provider names...", self._file_query)
        modes = ["all", "conflicts", "winning", "losing", "uncontested", "inactive", "hidden"]
        imgui.set_next_item_width(scaled(160))
        changed, position = imgui.combo("File view", modes.index(self._file_mode), ["All files", "Conflicts", "Winning", "Losing", "Uncontested", "Inactive only", "Hidden"])
        if changed:
            self._file_mode = modes[position]
        if self._file_mod and self._file_mod not in m.rows_by_id:
            self._file_mod = ""
        if imgui.get_content_region_avail().x >= scaled(360):
            imgui.same_line()
        imgui.set_next_item_width(min(scaled(200), max(scaled(100), imgui.get_content_region_avail().x - scaled(130))))
        selected_mod = m.rows_by_id.get(self._file_mod)
        if imgui.begin_combo("Provider mod", selected_mod.name if selected_mod else "All providers"):
            _, self._provider_query = imgui.input_text_with_hint("##provider_search", "Search provider mods...", self._provider_query)
            if imgui.selectable("All providers###provider_all", not self._file_mod)[0]:
                self._file_mod = ""
            key = self._provider_query.casefold()
            if self._provider_source is not m.rows or self._provider_key != key:
                self._provider_source = m.rows
                self._provider_key = key
                self._provider_rows = [row for row in m.rows if row.kind in {"owned", "external"}
                                       and key in row.name.casefold()]
            if imgui.begin_child("provider_choices", (scaled(320), scaled(200))):
                clipper = imgui.ListClipper()
                clipper.begin(len(self._provider_rows))
                while clipper.step():
                    for position in range(clipper.display_start, clipper.display_end):
                        row = self._provider_rows[position]
                        imgui.push_id(row.id)
                        cursor = imgui.get_cursor_screen_pos()
                        if imgui.selectable("##choose_provider", row.id == self._file_mod)[0]:
                            self._file_mod = row.id
                            imgui.close_current_popup()
                        imgui.get_window_draw_list().add_text(cursor, imgui.get_color_u32(imgui.Col_.text), row.name)
                        imgui.pop_id()
            imgui.end_child()
            imgui.end_combo()
        entries = index.filtered(self._file_query, self._file_mode, self._file_mod)
        imgui.text_disabled(f"{len(entries):,} matches | {len(index.files):,} indexed paths | {len(index.issues):,} unindexed sources")
        if index.issues:
            status_indicator("Incomplete file inventory: review the unindexed sources before relying on conflict results.", "warning")
        if index.issues and imgui.collapsing_header("Unindexed source details"):
            for issue in index.issues[:100]:
                imgui.text_wrapped(issue)
        area_height = max(scaled(180), imgui.get_content_region_avail().y - scaled(8))
        if imgui.begin_table("file_inspection_split", 2, imgui.TableFlags_.resizable | imgui.TableFlags_.sizing_stretch_prop):
            imgui.table_setup_column("Paths", init_width_or_weight=.45)
            imgui.table_setup_column("Providers", init_width_or_weight=.55)
            imgui.table_next_row()
            imgui.table_set_column_index(0)
            if imgui.begin_child("file_index_rows", (0, area_height), imgui.ChildFlags_.borders):
                clipper = imgui.ListClipper()
                clipper.begin(len(entries))
                while clipper.step():
                    for position in range(clipper.display_start, clipper.display_end):
                        entry = entries[position]
                        imgui.push_id(identity(entry.path))
                        cursor = imgui.get_cursor_screen_pos()
                        if imgui.selectable("##select_file", entry.path == self._file_selected)[0]:
                            self._file_selected = entry.path
                        winner = entry.winner
                        label = entry.path + (f" | {winner.name}" if winner else " | No active provider")
                        imgui.get_window_draw_list().add_text(cursor, imgui.get_color_u32(imgui.Col_.text), label)
                        imgui.set_item_tooltip(label)
                        imgui.pop_id()
            imgui.end_child()
            imgui.table_set_column_index(1)
            if imgui.begin_child("file_provider_panel", (0, area_height), imgui.ChildFlags_.borders):
                chosen = index.selected(self._file_selected)
                if chosen:
                    imgui.text_wrapped(chosen.path)
                    imgui.text_disabled("Highest to lowest priority")
                    imgui.begin_disabled(m.busy)
                    if imgui.begin_child("file_provider_details", (0, 0)):
                        width = imgui.get_content_region_avail().x
                        used, lines = 0., 1
                        for label in ("Select mod", "Copy path", "Reveal"):
                            button_width = imgui.calc_text_size(label).x + scaled(36)
                            if used and used + button_width > width:
                                lines, used = lines + 1, 0.
                            used += button_width + scaled(8)
                        height = 2 * imgui.get_text_line_height_with_spacing() + (3 + lines) * imgui.get_frame_height_with_spacing()
                        clipper = imgui.ListClipper()
                        clipper.begin(len(chosen.providers), height)
                        while clipper.step():
                            for position in range(clipper.display_start, clipper.display_end):
                                cursor = imgui.get_cursor_pos_y()
                                provider = chosen.providers[len(chosen.providers) - position - 1]
                                self._draw_file_provider(provider, chosen.winner)
                                imgui.set_cursor_pos_y(cursor + height - imgui.get_style().item_spacing.y)
                                imgui.dummy((0, 0))
                    imgui.end_child()
                    imgui.end_disabled()
                else:
                    imgui.text_wrapped("Select a path to inspect its providers and source locations.")
            imgui.end_child()
            imgui.end_table()

    def _draw_file_provider(self, provider, winner):
        imgui.push_id(provider.mod_id)
        imgui.push_id(identity(provider.path))
        state = "Hidden" if provider.hidden else "Disabled" if not provider.enabled else "Winner" if provider is winner else "Losing"
        imgui.text_unformatted(f"{state}: {provider.name}")
        imgui.set_item_tooltip(provider.name)
        imgui.text_disabled(f"Priority {provider.priority} | {provider.size:,} bytes")
        imgui.set_next_item_width(-1)
        imgui.input_text("##provider_path", provider.path, imgui.InputTextFlags_.read_only)
        width, used = imgui.get_content_region_avail().x, 0.
        for label in ("Select mod###provider_select", "Copy path###provider_copy", "Reveal###provider_reveal"):
            button_width = imgui.calc_text_size(label.split("##", 1)[0]).x + scaled(36)
            if used and used + button_width <= width:
                imgui.same_line()
            else:
                used = 0.
            if action_button(label, enabled=provider.mod_id in self.manager.rows_by_id if label.startswith("Select") else True):
                if label.startswith("Select"):
                    self.manager.selected_id, self._tab = provider.mod_id, "Library"
                elif label.startswith("Copy"):
                    imgui.set_clipboard_text(provider.path)
                else:
                    self._action(lambda: reveal(provider))
            used += button_width + scaled(8)
        row = self.manager.rows_by_id.get(provider.mod_id)
        if row and row.kind == "owned":
            from creation_lib.mod_manager.file_actions import parent_folder
            folder = parent_folder(self.manager.library, provider)
            hidden_parent = any(part.casefold().endswith(".mohidden") for part in folder.split("/"))
            if action_button("Unhide owned file###provider_hidden" if provider.hidden else "Hide owned file###provider_hidden", enabled=not hidden_parent and not self.manager.busy and not self.manager.runtime_active):
                self._action(lambda: self.manager.toggle_owned_hidden(provider))
            imgui.set_item_tooltip("Copies this owned mod through a recoverable replacement and retains its full previous folder. Refresh Files afterward.")
            if folder:
                if action_button("Unhide parent folder###provider_parent_hidden" if folder.casefold().endswith(".mohidden") else "Hide parent folder###provider_parent_hidden", enabled=not self.manager.busy and not self.manager.runtime_active):
                    self._action(lambda: self.manager.toggle_owned_hidden(provider, parent=True))
                imgui.set_item_tooltip("Changes " + folder + " through full backed-up replacement, including nested and empty directories. Existing destination folders block the change.")
        imgui.pop_id()
        imgui.pop_id()

    def _draw_details(self, *, embedded=False):
        visible = True if embedded else imgui.begin(f"Details{_NS}")[0]
        scoped = False
        try:
            if not visible:
                return
            m = self.manager
            if self._page == "Settings":
                imgui.text_wrapped("Preferences apply to your library. Return to Library to inspect or organize a mod.")
                return
            self.launcher.draw_controls()
            imgui.spacing()
            if self.panes.draw():
                return
            from .toolbar_ui import handle_information_keys
            handle_information_keys(self)
            row = m.rows_by_id.get(m.selected_id)
            if row is None:
                heading("Make it yours")
                imgui.text_wrapped("Select a mod to edit its group, tags, notes, or priority. Organization is saved independently from your MO2 profiles.")
                return
            imgui.push_id(m.profile_id)
            imgui.push_id(row.id)
            scoped = True
            detail_key = (m.profile_id, row.id)
            if self._detail_id != detail_key:
                self._detail_id = detail_key
                self._detail_group, self._tags, self._notes, self._priority = row.group, row.tags, row.notes, row.priority
            category_key = row.id, row.category, row.categories
            if category_key != self._category_detail_key:
                self._category_detail_key = category_key
                self._mod_category = row.category
                self._mod_categories = "\n".join(row.categories[1:])
            heading(row.name, large=True)
            status_indicator("Original folder reference" if row.kind == "external" else row.kind.title(), "muted")
            if row.source:
                imgui.text_disabled("Source folder")
                imgui.set_next_item_width(-1)
                imgui.input_text("##source_folder", row.source, imgui.InputTextFlags_.read_only)
                imgui.set_item_tooltip(row.source)
            else:
                imgui.text_wrapped("This entry contains profile metadata only.")
            imgui.text_unformatted(f"Category: {row.category or 'Uncategorized'}")
            imgui.set_next_item_width(-1)
            _, self._mod_category = imgui.input_text_with_hint("##mod_category", "Shared category path; empty clears", self._mod_category)
            if imgui.collapsing_header("Additional categories"):
                imgui.text_wrapped("One additional category path per line. The primary field above is first; all assignments are shared across local profiles.")
                _, self._mod_categories = imgui.input_text_multiline("##mod_categories", self._mod_categories, (-1, scaled(75)))
            if action_button("Assign category", enabled=not m.busy and not m.runtime_active):
                values = ([self._mod_category] if self._mod_category else []) + self._mod_categories.splitlines()
                self._action(lambda: m.assign_category(self._selected_mods if row.id in self._selected_mods else {row.id}, values))
            imgui.begin_disabled(m.busy)
            if row.kind in {"external", "owned"}:
                if action_button("Inspect mod files"):
                    self._file_mod, self._tab = row.id, "Files"
                    self._action(lambda: m.inspect_files(self._include_disabled_files))
            origin = m.origins.get(row.id)
            if origin and imgui.collapsing_header("History"):
                imgui.text_wrapped(f"Copied from: {origin['source']}\nSource identity retained: {row.id}\nCopied: {origin['copied']}")
            with section("organization", "Organize"):
                imgui.text_unformatted("Folder / group")
                imgui.set_next_item_width(-1)
                _, self._detail_group = imgui.input_text("##group", self._detail_group)
                imgui.text_unformatted("Tags")
                imgui.set_next_item_width(-1)
                _, self._tags = imgui.input_text("##tags", self._tags)
                imgui.text_unformatted("Notes")
                _, self._notes = imgui.input_text_multiline("##notes", self._notes, (-1, scaled(90)))
                if action_button("Save organization", primary=True):
                    self._action(lambda: m.customize(row.id, group=self._detail_group, tags=self._tags, notes=self._notes))
            with section("priority", "File priority"):
                imgui.text_wrapped("Higher numbers override lower numbers. This affects file conflicts within the current profile.")
                imgui.set_next_item_width(-1)
                _, self._priority = imgui.slider_int("##priority", self._priority, 0, max(0, len(m.rows) - 1))
                if action_button("Move to priority"):
                    self._action(lambda: m.reorder(row.id, self._priority))
            if row.kind == "owned":
                with section("owned_lifecycle", "Owned content"):
                    if action_button("Reinstall archive"):
                        path = pick_file("Choose replacement mod archive", [("Mod archives", "*.zip *.7z *.rar *.fomod")])
                        if path:
                            self._prepare(path, row.id)
                    if action_button("Review uninstall"):
                        self._action(lambda: m.review_owned(row.id))
                    review = m.lifecycle_review
                    if review and review.mod_id == row.id and m.restore_preview is None:
                        if self._lifecycle_token != review.token:
                            self._lifecycle_token, self._lifecycle_ack = review.token, False
                        self._show_owned_review(review)
                        imgui.text_wrapped("Remove this mod from every affected profile and retain its folder in Recovery.")
                        _, self._lifecycle_ack = imgui.checkbox("Confirm", self._lifecycle_ack)
                        if action_button("Uninstall mod", primary=True, enabled=self._lifecycle_ack):
                            self._action(m.uninstall)
            if imgui.collapsing_header("Profile files"):
                if action_button("Profile config"):
                    self._tab = "Profile config"
                profile = next((profile for profile in m.profiles if profile.id == m.profile_id), None)
                if profile:
                    for key in sorted(profile.files):
                        if not key.startswith("_"):
                            imgui.text_unformatted(key)
            if row.nexus_game and row.nexus_mod_id:
                imgui.text_wrapped(f"Nexus: {row.nexus_game} / mod {row.nexus_mod_id}")
                latest = m.updates.get((row.nexus_game, row.nexus_mod_id))
                if latest:
                    imgui.text_wrapped(f"Nexus mod version: {latest}\nInstalled version: {row.version or 'Unknown'}")
                    if latest != row.version:
                        imgui.text_wrapped("Versions differ. Review Nexus files; this does not verify which file replaces this install.")
                if action_button("Refresh Nexus files and account", enabled=bool(m.nexus_client)):
                    self._action(lambda: m.inspect_nexus_mod(row.id))
                state = m.nexus_states.get((row.nexus_game, row.nexus_mod_id))
                if state:
                    imgui.text_wrapped(f"Endorsement: {state.endorsement} | Tracking: {'Tracked' if state.tracked else 'Untracked' if state.tracked is False else 'Unknown'}")
                    replacements = state.replacements(row.nexus_file_id)
                    if not replacements:
                        imgui.text_wrapped("No published replacement mapping for this installed file. Mod version differences alone do not establish an update.")
                    for file in replacements:
                        imgui.push_id(file.id)
                        imgui.text_wrapped(f"Replacement file {file.id}: {file.name} | {file.version}")
                        if action_button("Open replacement file on Nexus"):
                            from creation_lib.mod_manager.nexus_links import browse, file_page
                            self._action(lambda file=file: browse(file_page(state.game, state.mod_id, file.id)))
                        if action_button("Queue replacement download", enabled=bool(m.nexus_client and m.nexus_client.user['premium'])):
                            self._action(lambda file=file: m.accept_nxm(f"nxm://{state.game}/mods/{state.mod_id}/files/{file.id}"))
                        imgui.set_item_tooltip("Free accounts need a fresh Mod Manager Download link from the Nexus file page.")
                        imgui.pop_id()
                if action_button("Endorse on Nexus", enabled=bool(m.nexus_client) and (not state or state.endorsement != "Endorsed")):
                    self._action(lambda: self.nexus_account.open(row.id, "endorse"))
                if action_button("Unendorse on Nexus", enabled=bool(m.nexus_client)):
                    self._action(lambda: self.nexus_account.open(row.id, "unendorse"))
                if action_button("Review selected Nexus account actions", enabled=bool(m.nexus_client)):
                    ids = self._selected_mods if row.id in self._selected_mods else {row.id}
                    self._action(lambda: self.nexus_bulk.open_account(sorted(ids)))
                imgui.same_line()
                if action_button("Untrack on Nexus" if state and state.tracked else "Track on Nexus", enabled=bool(m.nexus_client)):
                    self._action(lambda: self.nexus_account.open(row.id, "untrack" if state and state.tracked else "track"))
            imgui.end_disabled()
        finally:
            if scoped:
                imgui.pop_id()
                imgui.pop_id()
            if not embedded:
                imgui.end()

    @staticmethod
    def _show_owned_review(review):
        imgui.text_wrapped(f"{review.file_count:,} files, {review.bytes:,} bytes. Affected profiles ({len(review.profiles)}): " + ", ".join(review.profiles[:20]))
        if len(review.profiles) > 20:
            imgui.text_wrapped("Only the first 20 names are shown. This operation affects every listed profile reference in the review.")
        if review.changed:
            status_indicator(f"{len(review.changed):,} changes or baseline warnings", "warning")
            for change in review.changed[:30]:
                imgui.text_wrapped(change)
            if len(review.changed) > 30:
                imgui.text_disabled("Additional changes are retained in the service review result.")
        else:
            imgui.text_wrapped("Current files match their recorded installation hashes.")

    def _draw_recovery(self):
        m = self.manager
        heading("Recover owned content", large=True)
        imgui.text_wrapped("Reinstall and uninstall retain previous folders here. Restore keeps backups. Permanent deletion requires review of one retained payload; installed mods stay intact. Interrupted new/partial files in .history require manual review.")
        if not m.library:
            imgui.text_wrapped("Open a library to inspect recovery history.")
            return
        for message in m.library.recovery_report:
            imgui.text_wrapped(message)
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.begin_child("recovery_backup_list", (0, max(scaled(90), imgui.get_content_region_avail().y * .30)))
        try:
            for entry in m.recovery_history:
                imgui.push_id(entry["id"])
                with section("recovery_entry", entry["name"]):
                    imgui.text_wrapped(f"{entry['kind'].title()} backup\n{entry['path']}")
                    if action_button("Review restoration"):
                        self._action(lambda identifier=entry["id"]: m.review_restore(identifier))
                    if action_button("Review permanent backup deletion"):
                        self._action(lambda identifier=entry["id"]: m.review_prune(identifier))
                imgui.pop_id()
        finally:
            imgui.end_child()
        if m.prune_review:
            plan = m.prune_review
            if self._prune_key != (plan.operation_id, plan.token):
                self._prune_key, self._prune_ack = (plan.operation_id, plan.token), False
            details_height = max(scaled(60), imgui.get_content_region_avail().y - imgui.get_frame_height_with_spacing() * 3 - scaled(12))
            imgui.begin_child("backup_delete_details", (0, details_height))
            try:
                heading("Permanently delete one recovery backup")
                imgui.text_wrapped(f"{plan.name}\n{plan.kind.title()} backup: {plan.files:,} files, {plan.bytes:,} bytes. This removes this retained payload and its restoration entry. Installed content and other retained folders remain intact.")
            finally:
                imgui.end_child()
            _, self._prune_ack = imgui.checkbox("Permanently delete this reviewed recovery backup", self._prune_ack)
            if action_button("Delete reviewed recovery backup", enabled=self._prune_ack, primary=True):
                self._action(m.prune_backup)
            if action_button("Cancel backup deletion"):
                m.prune_review, self._prune_ack = None, False
        if m.restore_preview:
            entry = m.restore_preview
            with section("restore_review", "Restore " + entry["name"]):
                imgui.text_wrapped(f"Recovery payload: {entry['file_count']:,} files, {entry['bytes']:,} bytes. Changes after this review require another review.")
                if self._restore_id != entry["id"]:
                    self._restore_id, self._restore_ack = entry["id"], False
                if m.lifecycle_review:
                    self._show_owned_review(m.lifecycle_review)
                    imgui.text_wrapped("Current content will also be backed up. Existing profile organization is preserved.")
                else:
                    imgui.text_wrapped("Restore the removed mod and its references in profiles that still exist. Other profile entries keep their relative order.")
                _, self._restore_ack = imgui.checkbox("Restore this owned content and retain backups", self._restore_ack)
                if action_button("Restore into library", primary=True, enabled=self._restore_ack):
                    self._action(m.restore)
        imgui.end_disabled()

    def _draw_nexus(self):
        m = self.manager
        heading("Nexus connection", large=True)
        imgui.text_wrapped("Connect with a personal API key. Remembered keys stay in Windows Credential Manager and reconnect at startup. Browser sign-in is unavailable until Nexus registers ModBox21.")
        imgui.begin_disabled(m.busy)
        if m.nexus_client:
            status_indicator(f"Connected: {m.nexus_client.user['name']}", "success")
            if action_button("Disconnect and forget key"):
                self._action(m.disconnect_nexus)
        else:
            imgui.set_next_item_width(-1)
            _, self._api_key_input = imgui.input_text_with_hint("##nexus_key", "Personal API key", self._api_key_input, imgui.InputTextFlags_.password)
            _, self._remember_nexus = imgui.checkbox("Remember securely and reconnect at startup", self._remember_nexus)
            if action_button("Connect to Nexus", primary=True, enabled=bool(self._api_key_input)):
                key, self._api_key_input = self._api_key_input, ""
                self._action(lambda: m.connect_nexus(key, remember=self._remember_nexus))
            if action_button("Reconnect saved account"):
                self._action(m.connect_nexus)
            imgui.same_line()
            if action_button("Open API access page"):
                webbrowser.open("https://www.nexusmods.com/users/myaccount?tab=api+access")
        domains = sorted(set(GAMES.values()))
        if m.nexus_game not in domains:
            m.nexus_game = "fallout4"
        imgui.set_next_item_width(scaled(260))
        changed, index = imgui.combo("Library game", domains.index(m.nexus_game), domains)
        if changed:
            m.nexus_game = domains[index]
        _, m.auto_updates = imgui.checkbox("Check mapped Nexus mod versions automatically", m.auto_updates)
        _, m.auto_file_checks = imgui.checkbox("Include published files and account state in automatic checks", m.auto_file_checks)
        _, m.update_hours = imgui.slider_int("Interval (hours)", m.update_hours, 1, 24)
        imgui.text_wrapped("Checks resume in batches of 20, at least a minute apart. The interval controls when each mod is checked again. Progress and rate-limit delays survive restarts.")
        if action_button("Check versions now", enabled=bool(m.nexus_client)):
            self._action(m.check_updates)
        if action_button("Check files, versions and account (next 20)", enabled=bool(m.nexus_client)):
            self._action(m.check_nexus_batch)
        with section("nexus_incoming", "Download requests"):
            if self.nxm_router.request and action_button("Review pending browser destination"):
                self._tab = "Browser download destination"
            if action_button("Review pending downloads (next 20)", enabled=bool(m.pending_links and m.nexus_client and m.library)):
                self._action(self.nexus_bulk.open_downloads)
            imgui.set_next_item_width(-1)
            _, self._nxm_input = imgui.input_text_with_hint("##nxm_paste", "Paste nxm:// link (tokens hidden)", self._nxm_input, imgui.InputTextFlags_.password)
            if action_button("Queue pasted link", enabled=bool(self._nxm_input)):
                value, self._nxm_input = self._nxm_input, ""
                self._action(lambda: m.accept_nxm(value))
            for identity, link in list(m.pending_links.items()):
                imgui.push_id(link.label)
                imgui.text_unformatted(link.label)
                active = m.transfer_active(identity)
                if action_button("Download", primary=True, enabled=bool(m.library and m.nexus_client) and not active):
                    self._action(lambda key=identity: m.download_nxm(key))
                imgui.same_line()
                if active and action_button("Pause request"):
                    m.pause_download(identity)
                if action_button("Remove request", enabled=not active):
                    m.pending_links.pop(identity, None)
                imgui.pop_id()
            for record in m.download_records:
                imgui.push_id(record["path"])
                imgui.text_unformatted(Path(record["path"]).name)
                if action_button("Review for install", enabled=bool(m.profile_id)):
                    self._action(lambda record=record: self.download_installer.open(record))
                imgui.pop_id()
        self._draw_download_jobs()
        if imgui.collapsing_header("Browser protocol setup (opt in)"):
            imgui.text_wrapped("This changes the current user's nxm handler to ModBox21. Existing MO2/Vortex browser handling may be replaced. Windows Default apps or the browser may also require confirmation. Registration is never automatic.")
            _, self._protocol_confirmed = imgui.checkbox("Use ModBox21 instead of my current nxm handler", self._protocol_confirmed)
            if action_button("Register browser handler", primary=True, enabled=self._protocol_confirmed):
                def register():
                    from creation_lib.mod_manager.browser_links import register_handler
                    self._previous_handler = register_handler(confirmed=True)
                    m.status = "Registered ModBox21 for nxm links. Approve the browser prompt; if Windows retains another default, select ModBox21 in Default apps."
                self._action(register)
            if self._previous_handler:
                imgui.text_wrapped(f"Previous handler (for manual restoration): {self._previous_handler}")
        imgui.end_disabled()

    def _draw_install_mapping(self):
        m = self.manager
        if not m.install_base_pairs or not imgui.collapsing_header("Customize destination mapping"):
            return
        imgui.text_wrapped("Rename, relocate or exclude reviewed incoming files. Review again after edits. Changing installer choices or the content root resets these mappings.")
        base = dict(m.install_base_pairs)
        if self._mapping_target not in base:
            self._mapping_target, self._mapping_destination = "", ""
        imgui.begin_child("mapping_sources", (0, scaled(130)), imgui.ChildFlags_.borders)
        try:
            clipper = imgui.ListClipper()
            clipper.begin(len(m.install_base_pairs))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    target, source = m.install_base_pairs[index]
                    imgui.push_id(target.casefold())
                    mapped = m.install_mapping.get(target, target)
                    if imgui.selectable(f"{target} → {mapped if mapped is not None else 'Excluded'}###mapping_source", self._mapping_target == target)[0]:
                        self._mapping_target, self._mapping_destination = target, mapped or target
                    imgui.set_item_tooltip("Source: " + source)
                    imgui.pop_id()
        finally:
            imgui.end_child()
        if self._mapping_target:
            imgui.set_next_item_width(-1)
            _, self._mapping_destination = imgui.input_text("##mapping_destination", self._mapping_destination)
            if action_button("Set destination"):
                def edit():
                    from creation_lib.mod_manager.paths import relative_path
                    m.install_mapping[self._mapping_target] = relative_path(self._mapping_destination)
                    m.install_plan = None
                self._action(edit)
            imgui.same_line()
            if action_button("Exclude file"):
                m.install_mapping[self._mapping_target] = None
                m.install_plan = None
        if m.install_mapping and action_button("Reset custom mappings"):
            m.install_mapping = {}
            m.install_plan = None

    def _draw_install_sources(self):
        m, prepared = self.manager, self.manager.prepared
        if not prepared or prepared.source_tree is None or not imgui.collapsing_header("Additional package sources"):
            return
        if self._source_tree_package is not prepared:
            self._source_tree_package = prepared
            self._source_tree_expanded = set()
            self._source_tree_query, self._source_tree_selected, self._source_tree_destination = "", "", ""
        imgui.text_wrapped("Select an additional payload from this package and choose its destination. These explicit additions survive option changes; every change requires another destination review. Metadata and hidden files are excluded.")
        imgui.set_next_item_width(-1)
        _, self._source_tree_query = imgui.input_text_with_hint("##package_source_filter", "Filter package paths", self._source_tree_query)
        rows = prepared.source_tree.visible(self._source_tree_expanded, self._source_tree_query)
        imgui.begin_child("additional_package_tree", (0, scaled(160)), imgui.ChildFlags_.borders)
        try:
            clipper = imgui.ListClipper()
            clipper.begin(len(rows))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    row = rows[index]
                    imgui.indent(scaled(16) * row.depth)
                    imgui.push_id(("file/" if row.entry else "folder/") + row.path.casefold())
                    if row.entry is None:
                        imgui.set_next_item_open(bool(self._source_tree_query) or row.path.casefold() in self._source_tree_expanded, imgui.Cond_.always)
                        opened = imgui.tree_node_ex(Path(row.path).name + "###package_source_folder", imgui.TreeNodeFlags_.no_tree_push_on_open | imgui.TreeNodeFlags_.span_avail_width)
                        if not self._source_tree_query:
                            (self._source_tree_expanded.add if opened else self._source_tree_expanded.discard)(row.path.casefold())
                    elif imgui.selectable(Path(row.path).name + "###package_source_file", self._source_tree_selected == row.path)[0]:
                        self._source_tree_selected = row.path
                        self._source_tree_destination = row.path[len(prepared.package.prefix):] if prepared.package.prefix and row.path.casefold().startswith(prepared.package.prefix.casefold()) else row.path
                    imgui.set_item_tooltip(row.path)
                    imgui.pop_id()
                    imgui.unindent(scaled(16) * row.depth)
        finally:
            imgui.end_child()
        if self._source_tree_selected:
            imgui.text_unformatted(self._source_tree_selected)
            if not hasattr(self, "_package_preview"):
                from .package_preview_ui import PackagePreviewUI
                self._package_preview = PackagePreviewUI(self)
            self._package_preview.draw(self._source_tree_selected)
            imgui.set_next_item_width(-1)
            _, self._source_tree_destination = imgui.input_text("##additional_destination", self._source_tree_destination)
            if action_button("Include package source"):
                def add():
                    from creation_lib.mod_manager.paths import relative_path
                    m.install_additions[relative_path(self._source_tree_destination)] = self._source_tree_selected
                    m.install_addition_pairs = sorted(m.install_additions.items())
                    m.install_plan = None
                self._action(add)
        if m.install_addition_pairs:
            imgui.begin_child("additional_destinations", (0, scaled(120)), imgui.ChildFlags_.borders)
            try:
                pairs = m.install_addition_pairs
                clipper = imgui.ListClipper()
                clipper.begin(len(pairs), imgui.get_frame_height_with_spacing())
                while clipper.step():
                    for index in range(clipper.display_start, clipper.display_end):
                        target, source = pairs[index]
                        imgui.push_id(target.casefold())
                        if action_button("Remove###remove_additional_source"):
                            del m.install_additions[target]
                            m.install_addition_pairs = sorted(m.install_additions.items())
                            m.install_plan = None
                        imgui.same_line()
                        imgui.text_unformatted(target)
                        imgui.set_item_tooltip(source)
                        imgui.pop_id()
            finally:
                imgui.end_child()

    def _draw_download_jobs(self):
        m = self.manager
        imgui.begin_disabled(m.busy or m.downloads_active or m.runtime_active)
        changed, slots = imgui.slider_int("Simultaneous downloads", m.download_slots, 1, 4)
        if changed:
            self._action(lambda: m.set_download_slots(slots))
        imgui.end_disabled()
        if m.downloads_active:
            imgui.text_unformatted(f"{len(m._download_scheduler.transfers)} active/queued requests, {m.download_slots} transfer slots")
            if action_button("Pause all downloads"):
                m.pause_download()
        jobs = list(m.download_jobs)
        if not jobs:
            return
        imgui.separator()
        imgui.text_unformatted("Persistent download queue")
        imgui.text_wrapped("Resume obtains a fresh download URL. Free accounts need a fresh browser link for this file in Nexus connection and requests. Strong ETags allow range resume; other responses restart safely. Completed archives are retained when history is removed.")
        imgui.begin_child("persistent_download_jobs", (0, scaled(220)), imgui.ChildFlags_.borders)
        try:
            clipper = imgui.ListClipper()
            row_height = imgui.get_text_line_height_with_spacing() + imgui.get_frame_height_with_spacing() * 2
            clipper.begin(len(jobs), row_height)
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    job = jobs[index]
                    cursor = imgui.get_cursor_pos_y()
                    imgui.push_id(job["id"])
                    imgui.text_unformatted(f"{job['game']} / mod {job['mod_id']} / file {job['file_id']} | {job['state']} | {job['record'].get('bytes', 0):,} bytes")
                    remove_label = "Remove completed history" if job["state"] == "complete" else "Discard partial and queue entry"
                    transferring = m.transfer_active(job["id"])
                    if job["state"] != "complete":
                        local = m.download_local_recovery(job)
                        resume_label = "Finish local recovery" if local else "Resume queued download"
                        if action_button("Pause download" if transferring else resume_label, enabled=transferring or (local or bool(m.nexus_client)) and not m.busy and not m.runtime_active):
                            self._action(lambda job=job: m.pause_download(job["id"]) if transferring else m.resume_download(job["id"]))
                        if imgui.get_content_region_avail().x >= imgui.calc_text_size(resume_label + remove_label).x + scaled(80):
                            imgui.same_line()
                    if action_button(remove_label, enabled=not m.busy and not m.runtime_active and not transferring):
                        self._action(lambda job=job: m.remove_download_job(job["id"]))
                    imgui.set_item_tooltip("Completed archives remain available to install." if job["state"] == "complete" else "Permanently discard this manager-owned partial download and its queued metadata.")
                    imgui.pop_id()
                    imgui.set_cursor_pos_y(cursor + row_height - imgui.get_style().item_spacing.y)
                    imgui.dummy((0, 0))
        finally:
            imgui.end_child()
