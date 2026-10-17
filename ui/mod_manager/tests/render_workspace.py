"""Native host rendering with disposable fixtures and isolated settings."""
from __future__ import annotations

import argparse
import json
from contextlib import ExitStack
from dataclasses import replace
from pathlib import Path
import tempfile
import time
import tracemalloc
from types import SimpleNamespace
from unittest.mock import patch

from imgui_bundle import hello_imgui, imgui, immapp
from PIL import Image

from creation_lib.mod_manager.fomod import Environment, Fomod
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.controller import PreparedInstall
from creation_lib.mod_manager.conflicts import build_index
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces
from .fixtures import make_mo2, make_package


def fixture_executables(source, directory):
    from .test_runtime import pe
    game = directory / "runtime fixture game"
    (game / "Data").mkdir(parents=True, exist_ok=True)
    pe(game / "f4se_loader.exe")
    pe(game / "Fallout4.exe")
    with (source / "ModOrganizer.ini").open("a", encoding="utf-8") as output:
        output.write(f"\n[customExecutables]\n1\\title=F4SE\n1\\binary={game / 'f4se_loader.exe'}\n1\\workingDirectory={game}\n2\\title=Fallout 4\n2\\binary={game / 'Fallout4.exe'}\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--view", choices=("library", "setup", "plugins", "archives", "combined-data", "save-plugins", "delete-save-backup", "delete-overwrite-backup", "data", "saves", "source-saves", "save-backups", "recipes", "rename-title", "edit-field", "remove-owned", "prune-backup", "artwork", "import", "profile", "install", "review", "stage", "failed", "cancelled", "empty", "nexus", "settings", "migrated", "executables", "launch", "reinstall", "recovery", "uninstall", "copy", "copied", "copy-failed", "copy-cancelled", "files"), default="library")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rows", type=int, default=0)
    parser.add_argument("--width", type=int, default=1600)
    parser.add_argument("--height", type=int, default=1000)
    parser.add_argument("--frames", type=int, default=35)
    parser.add_argument("--font-scale", type=float, default=1.0, help="Exercise inherited ImGui text sizing; does not change OS DPI.")
    parser.add_argument("--automatic-conflicts", action="store_true", help="Wait for the automatic offline scan without a manual inspection command.")
    parser.add_argument("--associated-loader", action="store_true", help="Prepare a generated external F4SE loader through the controller against a stale game field; never launch.")
    parser.add_argument("--metrics", type=Path)
    parser.add_argument("--scroll-bottom", action="store_true")
    parser.add_argument("--show-log", action="store_true")
    parser.add_argument("--archive-preview", action="store_true", help="Render a bounded BA2 member preview.")
    parser.add_argument("--host", choices=("full", "mod_manager"), default="full")
    parser.add_argument("--integrated", action="store_true", help="Load the complete Toolkit workspace registry with the manager active.")
    parser.add_argument("--runtime-setup", action="store_true", help="Render empty-bundle runtime setup without executing anything.")
    parser.add_argument("--group-edit", action="store_true", help="Review a generated local hierarchy rename.")
    parser.add_argument("--source-relink", action="store_true", help="Review a generated copied source folder.")
    parser.add_argument("--source-metadata", action="store_true", help="Review generated source metadata selectively.")
    parser.add_argument("--columns", action="store_true", help="Render the column preference draft.")
    parser.add_argument("--shortcuts", action="store_true", help="Render command binding preferences.")
    parser.add_argument("--metadata-notices", action="store_true", help="Observe one generated external metadata edit without importing fields.")
    parser.add_argument("--owned-plugin-action", action="store_true", help="Review ghosting one generated owned plugin; never apply or launch.")
    parser.add_argument("--installer-header", action="store_true", help="Render generated bounded header art through the real texture backend.")
    parser.add_argument("--installer-repair", action="store_true", help="Render missing-source repair on generated payloads.")
    parser.add_argument("--package-preview", action="store_true", help="Render an inert generated archive-member preview.")
    parser.add_argument("--plugin-priority", action="store_true", help="Review a generated plugin draft priority (use --view plugins).")
    parser.add_argument("--launch-targets", action="store_true", help="Review a generated FO4 folder with fixed fixture version metadata.")
    parser.add_argument("--target-library", action="store_true", help="Render the library with a generated selected FO4 target, without launching.")
    parser.add_argument("--root-setup", action="store_true", help="Review standard F4SE root placement in a generated inert fixture, without applying or launching.")
    parser.add_argument("--target-customization", choices=("Arguments", "Source", "Preferences", "Remove"), help="Review target-only metadata in disposable fixtures; never apply or launch.")
    parser.add_argument("--nexus-badges", action="store_true", help="Populate fixture-only fetched account badges and a precomputed 20,000-edge replacement graph.")
    parser.add_argument("--nexus-account", action="store_true", help="Render an unendorse review with a mocked session; never send.")
    parser.add_argument("--download-destination", action="store_true", help="Render generated owned archive routing to references; never install.")
    parser.add_argument("--browser-routing", action="store_true", help="Render two disposable IPC destinations; never send or download.")
    parser.add_argument("--nexus-bulk-account", action="store_true", help="Render a mocked account batch review; never send.")
    parser.add_argument("--nexus-bulk-downloads", action="store_true", help="Render synthetic download batch review; never transfer.")
    parser.add_argument("--mo2-instance", type=Path, help="Read-only import preview; never imports this instance into the fixture library.")
    parser.add_argument("--no-library", action="store_true", help="Render automatic first-use import without an open library.")
    args = parser.parse_args()
    if args.metrics:
        tracemalloc.start()
    if args.mo2_instance and args.view != "import":
        parser.error("--mo2-instance is only supported with --view import")
    if args.plugin_priority and args.view != "plugins":
        parser.error("--plugin-priority requires --view plugins")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="mod-manager-render-") as temporary, patch("app.paths.get_db_dir") as data_root, ExitStack() as resources:
        directory = Path(temporary)
        data_root.return_value = directory / "app data"
        resources.enter_context(patch("ui.toolkit.app_paths.get_ini_dir", return_value=directory / "app ini"))
        settings = ToolkitSettings(path=directory / "settings.json", editor_settings_path=directory / "no-editor.json")
        settings.save = lambda: None
        settings.setup_complete = True
        settings.active_workspace = "mod_manager"
        settings.set_workspace_settings("mod_manager", {})
        settings.window_width, settings.window_height = args.width, args.height
        for game in ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"):
            settings.set_game_root_dir(game, "")
            settings.set_game_extracted_dir(game, "")
        base_variant = get_variant(args.host)
        variant = replace(base_variant, id="mod_manager_preview", workspace_ids=base_variant.workspace_ids if args.integrated else ("mod_manager",),
                          default_workspace="mod_manager", include_ai_panel=False)
        workspaces = create_workspaces(settings, variant.workspace_ids)
        workspace = next(ws for ws in workspaces if ws.id == "mod_manager")
        manager = workspace.manager
        manager._startup_auth_checked = True
        resources.callback(manager.cleanup)
        if args.view not in {"empty", "setup"}:
            source = make_mo2(directory)
            fixture_executables(source, directory)
            manager.library = Library(directory / "library", create=True)
            manager.import_preview = inspect_instance(source)
            manager.library.import_mo2(manager.import_preview)
            manager.refresh()
            manager.selected_id = next(row.id for row in manager.rows if row.name == "Override")
            workspace._library_path = str(manager.library.root)
            manager.status = "Library ready. Organize your collection or install a package."
            if args.rows:
                with manager.library._db() as db:
                    db.executemany("INSERT INTO mods(id,name,source,kind,tags) VALUES (?,?,?,'external',?)",
                                   [(f"synthetic-{i}", f"Fixture Mod {i:05d}", "", "favorite" if i % 7 == 0 else "") for i in range(args.rows)])
                    db.executemany("INSERT INTO entries VALUES (?,?,?,1,?)",
                                   [(manager.profile_id, f"synthetic-{i}", i + 4, f"Group {i % 10}") for i in range(args.rows)])
                manager.refresh()
        if args.view == "setup":
            from .test_standalone import game_fixture
            workspace._setup_root = str(game_fixture(directory))
            workspace._library_path = str(directory / "standalone library")
        elif args.view in {"combined-data", "save-plugins"}:
            from .test_standalone import game_fixture
            from .test_archive_index import ba2_fixture
            from .test_save_inspection import fixture as save_fixture
            from creation_lib.mod_manager.instances import create
            game = game_fixture(directory)
            manager.library = Library(directory / "standalone library", create=True)
            manager.profile_id = create(manager.library, "fo4", game)
            with manager.library._db() as db:
                files = manager.library.profiles()[0].files
                files["Fallout4.ini"] = "[Archive]\nsResourceArchiveList=Base.ba2\n"
                files["Fallout4Custom.ini"] = "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n"
                db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), manager.profile_id))
            manager.refresh()
            workspace._library_path = str(manager.library.root)
            if args.view == "combined-data":
                ba2_fixture(game / "Data" / "Base.ba2", ["textures/shared.txt", "meshes/archive-only.txt"])
                (game / "Data" / "textures").mkdir()
                (game / "Data" / "textures" / "shared.txt").write_bytes(b"generated loose override")
                manager.inspect_combined_data()
                workspace._right_pane = "Data"
                workspace.panes.data_expanded = {"textures", "meshes"}
            else:
                root = manager.library.root / "runtime" / "profiles" / manager.profile_id / "saves"
                root.mkdir(parents=True)
                colors = bytes(channel for y in range(216) for x in range(384) for channel in (x % 256, y % 256, (x + y) % 256, 255))
                save = save_fixture(root / "Generated.fos", screenshot=(384, 216, colors))
                save_fixture(root / "Other.fos", regular=("Fallout4.esm", "New.esp"), light=())
                manager.inspect_saves()
                manager._future.result(timeout=15)
                manager.poll()
                manager.inspect_save_plugins(save.path)
                manager._future.result(timeout=15)
                manager.poll()
                manager.compare_save_to(save.path, str(root / "Other.fos"))
                manager._future.result(timeout=15)
                manager.poll()
                manager.preview_save_screenshot(save.path)
                workspace._save_compare_target = str(root / "Other.fos")
                workspace._save_inspect_path, workspace._tab = save.path, "Save plugins"
                workspace._right_pane = "Saves"
                workspace.panes.save_selected = save.path
        elif args.view in {"plugins", "archives"}:
            from .test_standalone import game_fixture
            from .test_archive_index import ba2_fixture
            from creation_lib.mod_manager.instances import create
            from creation_lib.mod_manager.cli_reader import run_json
            import subprocess
            import shutil
            game = game_fixture(directory)
            executable = shutil.which("modkit.exe") or shutil.which("modkit")
            result = subprocess.run([executable, "--game", "fo4", "--format", "json", "esp", "new", str(game / "Data" / "Generated.esp"), "--no-masters"], capture_output=True, timeout=30)
            assert result.returncode == 0, result.stderr.decode(errors="replace")
            # Keep read-only metadata on the supported CLI boundary in this capture.
            assert run_json(("esp", "header", str(game / "Data" / "Generated.esp")), game="fo4")
            manager.library = Library(directory / "standalone library", create=True)
            manager.profile_id = create(manager.library, "fo4", game)
            if args.view == "plugins":
                payload = (game / "Data" / "Generated.esp").read_bytes()
                for title, plugin in (("Generated base", "GeneratedBase.esm"), ("Generated lighting", "Lighting.esp"),
                                      ("Generated textures", "Textures.esp"), ("Generated light patch", "LightPatch.esl"),
                                      ("Generated interface", "Interface.esp"), ("Generated gameplay", "Gameplay.esp")):
                    folder = directory / title
                    folder.mkdir()
                    (folder / plugin).write_bytes(payload)
                    package = Package(folder)
                    try:
                        manager.library.install(package, package.plain_plan(), title, manager.profile_id)
                    finally:
                        package.close()
            manager.refresh()
            workspace._library_path = str(manager.library.root)
            workspace._right_pane = "Plugins" if args.view == "plugins" else "Archives"
            if args.view == "plugins":
                manager.inspect_plugins(metadata=True)
                manager._future.result(timeout=30)
                manager.poll()
                from creation_lib.mod_manager.plugin_state import review
                manager.apply_plugins(review(manager.plugin_catalog, enabled=[p.name for p in manager.plugin_catalog.plugins]))
                manager._future.result(timeout=30)
                manager.poll()
            else:
                if args.archive_preview:
                    from creation_lib.ba2.native_runtime import pack_archive
                    from creation_lib.mod_manager.archive_index import preview
                    payload = directory / "archive-payload"
                    payload.mkdir()
                    (payload / "readme.txt").write_text("Generated archive preview\n\nRead a member from its selected provider without extracting it into the game.\n\nSupports text, DDS and other images, and bounded binary inspection.")
                    pack_archive(str(payload), str(game / "Data" / "Generated.ba2"), "fo4", compress=True)
                else:
                    ba2_fixture(game / "Data" / "Generated.ba2", ["textures/fixture.txt", "meshes/other.txt"])
                manager.inspect_archives()
                if args.archive_preview:
                    manager._future.result(timeout=30)
                    manager.poll()
                    member = manager.archive_index.members[0]
                    workspace.panes._archive_selected = member.path
                    workspace.panes.archive_preview.context = id(manager.archive_index), member.path
                    workspace.panes.archive_preview.result = preview(member.archives[0], member.path)
        elif args.view == "migrated":
            from .test_instance_migration import full_instance
            source, _, _, _ = full_instance(directory / "full fixture")
            fixture_executables(source, directory)
            manager.library = manager.import_preview = None
            manager.refresh()
            workspace._library_path, workspace._mo2_path = "", str(source)
            manager.migrate_mo2(source)
            workspace._one_click_migration = workspace._show_imported_profiles = True
        elif args.view in {"settings", "nexus"}:
            workspace._tab = "Settings" if args.view == "settings" else "Nexus"
        elif args.view == "executables":
            workspace.launcher.edit(workspace.launcher.selected())
            workspace._tab = "Executables"
        elif args.view == "rename-title":
            workspace.metadata.rename()
        elif args.view == "edit-field":
            workspace.metadata.edit_field(manager.selected_id, "notes")
        elif args.view == "remove-owned":
            from .test_transactions import setup_owned
            library, profile, mod, _ = setup_owned(directory / "owned removal fixture")
            manager.library, manager.profile_id = library, profile
            manager.refresh()
            manager.selected_id = mod
            workspace.metadata.remove()
        elif args.view == "launch" and not args.associated_loader:
            from .test_runtime import pe
            from creation_lib.mod_manager.runtime import prepare
            bundle = directory / "fixture bundle"
            bundle.mkdir()
            for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
                pe(bundle / name, machine)
            game = directory / "runtime fixture game"
            user = directory / "game user fixture"
            manager.launch_review = prepare(manager.library, manager.profile_id, workspace.launcher.selected(), str(game), str(bundle),
                                            user_targets=(user / "plugins", user / "ini"))
            workspace._tab = "Launch"
        elif args.view == "import":
            workspace._tab = "Import MO2"
            if args.mo2_instance:
                manager.import_preview = inspect_instance(args.mo2_instance)
                manager.status = f"Found {len(manager.import_preview.profiles):,} profiles. Review the import before adding them."
            workspace._mo2_path = str(args.mo2_instance or source)
            if args.no_library:
                manager.library = None
                manager.refresh()
                workspace._library_path = ""
                manager.status = "Choose your MO2 instance to migrate all supported profiles and mod folders."
        elif args.view == "profile":
            workspace._tab = "Profile config"
        elif args.view == "recipes":
            from creation_lib.mod_manager.profile_recipes import export
            path = export(manager.library, manager.profile_id)
            manager.inspect_recipe(path)
            manager._future.result(timeout=10)
            manager.poll()
            manager.review_recipe(manager.recipe_suggestions, manager.recipe.name + " (transferred)")
            manager._future.result(timeout=10)
            manager.poll()
            workspace.transfers.path = str(path)
            workspace._tab = "Profile transfers"
        elif args.view in {"copy", "copied", "copy-failed", "copy-cancelled"}:
            selected = [row.id for row in manager.rows if row.kind == "external"]
            workspace._migration_selected, workspace._tab = set(selected), "Copy sources"
            manager.migration_review = manager.library.review_migration(selected)
            if args.view == "copied":
                manager.migrate()
            elif args.view == "copy-failed":
                from .test_migration import contents
                original = {row.id: contents(row.source) for row in manager.rows if row.kind == "external"}
                (Path(manager.rows_by_id[selected[0]].source) / "new-after-review.txt").write_bytes(b"synthetic stale review")
                manager.migrate()
            elif args.view == "copy-cancelled":
                manager.library.owned._checkpoint = lambda name: manager.cancel() if name == "before-commit" else None
                manager.migrate()
        elif args.view == "files":
            workspace._tab = "Files"
            manager.file_index = build_index(manager.library, manager.profile_id)
            workspace._file_selected = "textures/shared.txt"
        elif args.view == "data":
            manager.file_index = build_index(manager.library, manager.profile_id)
            workspace._right_pane = "Data"
            workspace.panes.data_expanded = {"textures"}
        elif args.view in {"delete-save-backup", "delete-overwrite-backup"}:
            from .test_backup_prune import fixture
            from ui.mod_manager.backup_prune_ui import open_review
            library, profile, _, backup, _ = fixture(directory / "bundle removal", "save" if args.view == "delete-save-backup" else "overwrite")
            manager.library, manager.profile_id = library, profile
            manager.refresh()
            kind = "save" if args.view == "delete-save-backup" else "overwrite"
            if kind == "save":
                manager.inspect_save_backups()
            else:
                manager.inspect_overwrite()
            manager._future.result(timeout=15)
            manager.poll()
            open_review(workspace, kind, backup.name)
        elif args.view in {"saves", "source-saves", "save-backups"}:
            from .test_standalone import game_fixture
            from .test_saves import save_fixture
            from creation_lib.mod_manager.instances import create
            from creation_lib.mod_manager.saves import fo4_header, inventory
            manager.library = Library(directory / "saves library", create=True)
            manager.profile_id = create(manager.library, "fo4", game_fixture(directory))
            manager.refresh()
            root = manager.library.root / "runtime" / "profiles" / manager.profile_id / "saves"
            root.mkdir(parents=True)
            save_fixture(root / "Fixture.fos")
            manager.save_index = inventory(manager.library, manager.profile_id)
            save = manager.save_index.files[0]
            manager.save_details[save.path] = fo4_header(save)
            workspace._right_pane, workspace.panes.save_selected = "Saves", save.path
            if args.view == "source-saves":
                from creation_lib.mod_manager.saves import inventory_folder
                source_folder = directory / "selected source saves"
                source_folder.mkdir()
                source_path = save_fixture(source_folder / "Source.fos")
                source_path.with_suffix(".f4se").write_bytes(b"generated companion")
                manager.save_index = inventory_folder(manager.library, manager.profile_id, source_folder)
                save = manager.save_index.files[0]
                manager.save_details = {save.path: fo4_header(save)}
                workspace.panes.save_selected = save.path
            if args.view == "save-backups":
                from creation_lib.mod_manager import save_backups, save_transfer
                save_path = Path(save.path)
                save_path.with_suffix(".f4se").write_bytes(b"generated companion")
                manager.save_index = inventory(manager.library, manager.profile_id)
                backup = Path(save_transfer.apply(manager.library, save_transfer.review(manager.library, manager.save_index, save.path)))
                save_path.unlink()
                save_path.with_suffix(".f4se").unlink()
                manager.save_index, manager.save_details = None, {}
                manager.save_backups = save_backups.inventory(manager.library, manager.profile_id)
                manager.save_transfer_plan = save_backups.review(manager.library, manager.profile_id, backup.name, manager.profile_id)
                workspace._save_backup_selected, workspace._tab = backup.name, "Save backups"
        elif args.installer_header or args.installer_repair or args.package_preview:
            from .test_installer_repairs import repair_package
            path = repair_package(directory / "installer fixture")
            if args.installer_header:
                Image.new("RGB", (320, 180), (180, 80, 20)).save(path / "fomod/header.png")
                config = path / "fomod/ModuleConfig.xml"
                xml = config.read_text().replace('<moduleName>', '<moduleName position="RightOfImage" colour="F4B860">')
                xml = xml.replace('</moduleName>', '</moduleName><moduleImage path="fomod/header.png" showFade="true" height="10000"/>')
                config.write_text(xml)
            manager.prepare_install(path, manual=args.package_preview)
            manager._future.result(timeout=30)
            manager.poll()
            assert not manager.error, manager.error
            workspace._tab, workspace._install_name = "Install", "Generated fixture"
            if args.package_preview:
                from ui.mod_manager.package_preview_ui import PackagePreviewUI
                from creation_lib.mod_manager.package_preview import read
                workspace._package_preview = PackagePreviewUI(workspace)
                workspace._package_preview.prepared = manager.prepared
                workspace._package_preview.result = read(manager.prepared.package, "replacement.txt")
                workspace._source_tree_package = manager.prepared
                workspace._source_tree_selected = "replacement.txt"
        elif args.view == "artwork":
            from creation_lib.mod_manager.installer_preview import image, metadata
            from creation_lib.mod_manager.installation import source_tree
            path = make_package(directory, zipped=False)
            Image.new("RGB", (320, 180), (90, 120, 160)).save(path / "preview.png")
            module = path / "fomod" / "ModuleConfig.xml"
            module.write_bytes(module.read_bytes().replace(b"</moduleName>", b'</moduleName><moduleImage path="preview.png"/>').replace(b'<plugin name="High">', b'<plugin name="High"><image path="preview.png"/>'))
            path.joinpath("fomod", "info.xml").write_text('<fomod><Name>Generated Artwork Fixture</Name><Author>Fixture Author</Author><Version>1.2</Version></fomod>')
            package = Package(path)
            manager.prepared = PreparedInstall(package, Fomod(package.read_config(), compatibility="mo2"), Environment({"Fixture.esp": "Active", "Absent.esp": "Missing"}), source_tree=source_tree(package), metadata=metadata(package))
            manager.installer_image, manager.installer_image_source = image(package, "preview.png"), "preview.png"
            workspace._install_name, workspace._tab = "Generated Artwork Fixture", "Install"
        elif args.view in {"install", "review", "failed", "cancelled"}:
            package = Package(make_package(directory))
            manager.prepared = PreparedInstall(package, Fomod(package.read_config()), Environment({"Fixture.esp": "Active"}))
            manager.choices = {"0:0": {"0:0:1"}}
            workspace._install_name = "Fixture Texture Pack"
            workspace._tab = "Install"
            if args.view == "review":
                manager.install_plan = manager.prepared.plan(manager.choices)
                manager.install_pairs = list(manager.install_plan.items())
                workspace._step = 2
            elif args.view == "failed":
                manager.prepared.fomod = Fomod(package.read_config().replace(b'source="base.txt"', b'source="missing-payload.txt"'))
                manager.review_install()
            elif args.view == "cancelled":
                manager.review_install()
                manager._future.result(timeout=30)
                manager.poll()
                copy_plan = manager.library._copy_plan
                def cancellable_copy(sources, destination, cancel=None, progress=None):
                    def report(fraction, target):
                        cancel.set()
                        if progress:
                            progress(fraction, target)
                    return copy_plan(sources, destination, cancel, report)
                manager.library._copy_plan = cancellable_copy
                manager.install("Cancelled fixture")
        elif args.view == "stage":
            manager.export_preview = manager.library.export_plan(manager.profile_id)
            manager.conflict_pairs = list(manager.export_preview.conflicts.items())
            workspace._tab = "Stage"
        elif args.view == "nexus":
            workspace._tab = "Nexus"
            manager.nexus_client = SimpleNamespace(user={"name": "Disposable fixture account"}, forget=lambda: None)
            manager.accept_nxm("nxm://fallout4/mods/123/files/456?key=fixture&expires=4000000000&user_id=42")
        elif args.view in {"reinstall", "recovery", "uninstall", "prune-backup"}:
            package = Package(make_package(directory))
            engine = Fomod(package.read_config())
            identifier = manager.library.install(package, engine.plan(package), "Owned Fixture", manager.profile_id)
            manager.library.create_profile("Owned clone", manager.profile_id)
            (manager.library.root / "mods" / identifier / "user-added.ini").write_text("preserve this user data")
            review = manager.library.review_owned(identifier)
            if args.view in {"recovery", "prune-backup"}:
                operation = manager.library.uninstall(review)
                manager.refresh()
                if args.view == "prune-backup":
                    manager.review_prune(operation)
                else:
                    manager.review_restore(operation)
                manager._future.result(timeout=10)
                manager.poll()
                workspace._tab = "Recovery"
                package.close()
            elif args.view == "reinstall":
                manager.refresh()
                manager.selected_id = identifier
                manager.prepared = PreparedInstall(package, engine, Environment({"Fixture.esp": "Active"}), replacement_review=review)
                manager.install_plan = engine.plan(package)
                manager.install_pairs = list(manager.install_plan.items())
                workspace._install_name, workspace._tab = "Owned Fixture", "Install"
            else:
                manager.refresh()
                manager.selected_id = identifier
                manager.lifecycle_review = review
                workspace._tab = "Library"
                package.close()
        settings.set_workspace_settings("mod_manager", {"mo2_path": workspace._mo2_path})
        if any((args.columns, args.shortcuts, args.metadata_notices, args.owned_plugin_action)) and manager._future:
            manager._future.result(timeout=30)
            manager.poll()
        if args.columns:
            workspace.columns_ui.open()
        if args.shortcuts:
            workspace.shortcuts_ui.open()
        if args.metadata_notices:
            from creation_lib.mod_manager.metadata_observer import scan
            scan(manager.library)
            watched = Path(manager.rows_by_id[manager.selected_id].source) / "meta.ini"
            watched.write_bytes(watched.read_bytes().replace(b"version=1.2", b"version=4.2"))
            manager.metadata_observer.index = scan(manager.library)
            workspace.metadata_notifications.open()
            workspace.metadata_notifications.selected = manager.metadata_observer.index.notices[0]
        if args.owned_plugin_action:
            from creation_lib.mod_manager.plugin_state import inspect, inventory
            from creation_lib.mod_manager.plugin_file_actions import review as review_plugin_file
            from .test_plugin_file_actions import plugin_bytes
            generated = directory / "owned plugin package"
            generated.mkdir()
            (generated / "Owned.esp").write_bytes(plugin_bytes())
            package = Package(generated)
            try:
                manager.library.install(package, package.plain_plan(), "Generated plugin provider", manager.profile_id)
            finally:
                package.close()
            manager.refresh()
            manager.plugin_catalog = inspect(inventory(manager.library, manager.profile_id), reader=lambda *args: ({"flags": {}}, ()))
            plugin = next(p for p in manager.plugin_catalog.plugins if p.name == "Owned.esp")
            workspace.plugin_file_ui.open(plugin.name)
            workspace.plugin_file_ui.action = "ghost"
            workspace.plugin_file_ui.plan = review_plugin_file(manager.library, manager.plugin_catalog, plugin, "ghost")
        if args.group_edit:
            workspace.metadata.rename_group("Visual")
        if args.source_relink:
            import shutil
            selected = manager.rows_by_id[manager.selected_id]
            relocated = directory / "Relocated fixture source"
            shutil.copytree(selected.source, relocated)
            workspace.sources.open()
            workspace.sources.path = str(relocated)
            manager.review_reference_source(selected.id, str(relocated))
        if args.source_metadata:
            selected = manager.rows_by_id[manager.selected_id]
            Path(selected.source).joinpath("meta.ini").write_text('[General]\nversion=2.0\nnotes="Source note\\nsecond line"\ntags=fixture\ncategory=0\ngameName=Fallout4\nmodid=123\nfileid=456\n', encoding="utf-8")
            workspace.source_metadata.open()
        if args.plugin_priority:
            manager._future.result(timeout=20)
            manager.poll()
            catalog = manager.plugin_catalog
            panes = workspace.panes
            panes.catalog, panes.order = catalog, [p.name for p in catalog.plugins]
            panes.active = {p.name.casefold() for p in catalog.plugins if p.enabled}
            panes.locked = {name.casefold(): (name, priority) for name, priority in catalog.locked}
            workspace.plugin_priority.open("Generated.esp")
        if args.launch_targets or args.target_library or args.root_setup or args.target_customization:
            from .test_pe_metadata import image
            from unittest.mock import patch as fixture_patch
            game = directory / "Generated FO4 1.10.163"
            (game / "Data").mkdir(parents=True)
            image(game / "Fallout4.exe", version=(1, 10, 163, 0))
            with fixture_patch("creation_lib.mod_manager.pe_metadata.file_version", return_value=(1, 10, 163, 0)):
                manager.review_target("FO4 legacy", game)
                manager._future.result(timeout=10)
                manager.poll()
            workspace.targets_ui.name, workspace.targets_ui.folder = "FO4 legacy", str(game)
            if args.target_library or args.root_setup or args.target_customization:
                from creation_lib.mod_manager.launch_targets import apply
                identifier = apply(manager.library, manager.target_review)
                manager.target_review = None
                manager.refresh()
                manager.select_target(identifier)
                settings.set_workspace_settings("mod_manager", {"target_id": identifier})
                if args.root_setup:
                    from .test_f4se_setup import package
                    from .test_pe_metadata import fixture_version
                    from creation_lib.mod_manager import f4se_setup, f4se_root_setup
                    with fixture_patch("creation_lib.mod_manager.pe_metadata.file_version", side_effect=fixture_version), fixture_patch("creation_lib.mod_manager.runtime.process_names", return_value=set()):
                        f4se_setup.prepare(manager.library, identifier, archive=package(directory))
                        manager.refresh()
                        manager.root_setup_review = f4se_root_setup.review(manager.library, identifier)
                    workspace._tab = "Launch targets"
                if args.target_customization:
                    manager.selected_id = next(row.id for row in manager.rows if row.kind not in {"separator", "foreign"})
                    workspace._tab = ""
                    workspace.target_customization.open(args.target_customization)
                    if args.target_customization == "Arguments":
                        workspace.target_customization.arguments = "-v -priority above_normal"
                    elif args.target_customization == "Source":
                        source = directory / "Explicit variant"
                        source.mkdir()
                        (source / "version.txt").write_bytes(b"inert target source")
                        workspace.target_customization.source, workspace.target_customization.version = str(source), "fixture version"
                    elif args.target_customization == "Preferences":
                        workspace.target_customization.ini_scope, workspace.target_customization.save_scope = 1, 1
                    workspace.target_customization.review()
                    manager._future.result(timeout=10)
                    manager.poll()
            else:
                workspace._tab = "Launch targets"
        if args.runtime_setup:
            workspace._tab = "Runtime setup"
            workspace.launcher.find_bundles()
        if args.nexus_badges or args.nexus_account or args.download_destination:
            from creation_lib.mod_manager.nexus_state import File, State
            with manager.library._db() as db:
                db.executemany("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=? WHERE id=?", ((row.priority % 32 + 1, row.id) for row in manager.rows if row.kind not in {"separator", "foreign"}))
            manager.refresh()
            state = State("fallout4", 123, (File(20001, "new-fixture.zip", "2", "Main"),), tuple((old, old + 1) for old in range(1, 20001)), "Endorsed", True)
            for file_id in range(1, 33):
                state.replacements(file_id)
            manager.nexus_states[("fallout4", 123)] = state
            if args.nexus_account:
                from creation_lib.mod_manager.nexus import NexusClient
                from .test_nexus_state import Transport
                manager.nexus_client = NexusClient("fixture-session", transport=Transport())
                manager.nexus_client.user = {"user_id": 42, "name": "Generated offline account", "premium": False}
                row = next(row for row in manager.rows if row.kind == "external")
                workspace.nexus_account.open(row.id, "unendorse")
            if args.download_destination:
                from .test_download_install import owned_download
                path = owned_download(manager.library)
                workspace.download_installer.open({"path": str(path)})
                manager._future.result(timeout=15)
                manager.poll()
        if args.nexus_bulk_account or args.nexus_bulk_downloads:
            from .test_nexus_bulk import Account
            manager.nexus_client = Account()
            if args.nexus_bulk_account:
                selected = [row.id for row in manager.rows if row.kind == "external"][:3]
                with manager.library._db() as db:
                    for index, identifier in enumerate(selected):
                        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=?,nexus_file_id=1,version='1.2' WHERE id=?", (123 + index, identifier))
                manager.refresh()
                workspace.nexus_bulk.open_account(selected)
            else:
                for identifier in (456, 457, 458):
                    manager.accept_nxm(f"nxm://fallout4/mods/123/files/{identifier}?key=render-fixture-key&expires=4000000000&user_id=42")
                workspace.nexus_bulk.open_downloads()
        if args.browser_routing:
            from ui.mod_manager.integration import ensure_inbox
            from creation_lib.mod_manager.nxm_instances import Inbox
            from creation_lib.mod_manager.nxm_routing import context
            ensure_inbox(workspace)
            remote = directory / "Other library"
            remote.mkdir()
            inbox = Inbox(workspace._link_inbox.directory, context={**context(manager), "library_id": "other-fixture", "library_root": str(remote), "profile_name": "Other profile"})
            resources.callback(inbox.close)
            workspace.handle_nxm("nxm://fallout4/mods/123/files/456?key=render-fixture-key&expires=4000000000&user_id=42")
        if args.associated_loader:
            from .test_launch_resolution import installation
            from .test_pe_metadata import fixture_version
            from creation_lib.mod_manager.launcher import Executable
            game = installation(directory / "Selected external FO4")
            stale = installation(directory / "Previous FO4", (1, 11, 240, 0))
            item = manager.save_executable(Executable("external-fixture", "My external F4SE", str(game / "f4se_loader.exe"), "-v -waitforclose", str(game)))
            workspace.launcher.selected_id = item.id
            bundle = directory / "inert private bundle"
            bundle.mkdir()
            user = (directory / "user-plugins", directory / "user-inis")
            for path in user:
                path.mkdir()
            with patch("creation_lib.mod_manager.pe_metadata.file_version", side_effect=fixture_version), patch("creation_lib.mod_manager.native_builder.ensure", return_value=bundle), patch("creation_lib.mod_manager.runtime.fo4_user_targets", return_value=user):
                manager.prepare_launch(item.id, str(stale))
                manager._future.result(timeout=15)
                manager.poll()
            assert manager.launch_review.game_root == str(game)
            workspace._tab = "Launch"
            settings.set_workspace_settings("mod_manager", {"target_id": manager.target_id, "executable_id": item.id, "mo2_path": workspace._mo2_path})
        app = ToolkitApp(workspaces, settings, app_variant=variant)
        if args.view == "setup":
            workspace._library_path = str(directory / "standalone library")
            workspace._restore_library = False
        run = immapp.run
        frames = 0
        render_started = time.monotonic()
        selected_frame = 0
        drawn_names = set()
        frame_times, resident_bytes = [], []
        push_id, end_table, begin, end_child = imgui.push_id, imgui.end_table, imgui.begin, imgui.end_child
        collapsing_header = imgui.collapsing_header
        input_text = imgui.input_text
        def show_repair_fields(label, *arguments, **options):
            if args.installer_repair and label == "##missing_declared_source":
                imgui.set_scroll_from_pos_y(imgui.get_cursor_pos_y() - imgui.get_scroll_y(), .02)
            return input_text(label, *arguments, **options)
        input_multiline = imgui.input_text_multiline
        def show_preview(label, *arguments, **options):
            if args.package_preview and label == "##inert_package_preview":
                imgui.set_scroll_from_pos_y(imgui.get_cursor_pos_y() - imgui.get_scroll_y(), .02)
            return input_multiline(label, *arguments, **options)
        def open_preview(label, *arguments, **options):
            if args.package_preview and label == "Additional package sources":
                imgui.set_next_item_open(True, imgui.Cond_.always)
            return collapsing_header(label, *arguments, **options)
        def record_row(identifier, *arguments, **options):
            if (args.scroll_bottom or args.view == "migrated") and imgui.internal.get_current_window().name.startswith("Library"):
                row = manager.rows_by_id.get(identifier) if isinstance(identifier, str) else None
                if row:
                    drawn_names.add(row.name)
            return push_id(identifier, *arguments, **options)
        def scroll_table():
            if args.scroll_bottom and frames >= 5:
                imgui.set_scroll_y(imgui.get_scroll_max_y())
            end_table()
        def scroll_tree():
            if args.scroll_bottom and frames >= 5 and "mod_hierarchy" in imgui.internal.get_current_window().name:
                imgui.set_scroll_y(imgui.get_scroll_max_y())
            if args.scroll_bottom and frames >= 5 and "installer_details" in imgui.internal.get_current_window().name:
                imgui.set_scroll_y(imgui.get_scroll_max_y())
            if args.scroll_bottom and frames >= 5 and "target_customization_details" in imgui.internal.get_current_window().name:
                imgui.set_scroll_y(imgui.get_scroll_max_y())
            end_child()
        def scroll_details(name, *arguments, **options):
            result = begin(name, *arguments, **options)
            if args.view == "uninstall" and name.startswith("Details") and frames >= 5:
                imgui.set_scroll_y(imgui.get_scroll_max_y())
            if args.scroll_bottom and args.view in {"copy", "files", "import", "settings", "executables", "artwork"} and (name.startswith(("Library", "Settings")) or "###manager_workflow" in name) and frames >= 5:
                imgui.set_scroll_y(imgui.get_scroll_max_y())
            return result
        def render(runner_params, **kwargs):
            params = runner_params
            params.ini_disable = True
            params.app_window_params.hidden = True
            params.app_window_params.window_geometry.size = (args.width, args.height)
            params.app_window_params.window_geometry.window_size_measure_mode = hello_imgui.WindowSizeMeasureMode.screen_coords
            params.fps_idling.enable_idling = False
            for window in params.docking_params.dockable_windows:
                if window.label == "Log":
                    window.is_visible = args.show_log
            def finish():
                nonlocal frames, selected_frame
                frames += 1
                imgui.get_style().font_scale_main = args.font_scale
                if args.archive_preview and frames == 8:
                    from creation_lib.mod_manager.archive_index import preview
                    member = manager.archive_index.members[0]
                    workspace.panes._archive_selected = member.path
                    workspace.panes.archive_preview.context = id(manager.archive_index), member.path
                    workspace.panes.archive_preview.result = preview(member.archives[0], member.path)
                if args.metrics and frames > 10:
                    frame_times.append(imgui.get_io().delta_time * 1000)
                    try:
                        import psutil
                        resident_bytes.append(psutil.Process().memory_info().rss)
                    except ImportError:
                        pass
                if args.view == "migrated" and manager.instance_migration and not manager.selected_id:
                    manager.selected_id = next((row.id for row in manager.rows if row.name == "Override"), "")
                    selected_frame = frames
                    return
                if selected_frame and frames < selected_frame + 8:
                    return  # Let auto-sized details settle before capturing.
                scan_ready = not args.automatic_conflicts or manager.conflict_scan_state in {"ready", "partial", "error"}
                if frames >= args.frames and not manager.busy and scan_ready:
                    params.app_shall_exit = True
                if time.monotonic() - render_started > 180:
                    raise RuntimeError("Manager background work did not finish.")
            params.callbacks.post_render_dockable_windows = finish
            run(runner_params=params, **kwargs)
        with patch("ui.toolkit.app.immapp.run", render), patch("ui.toolkit.app_paths.get_ini_dir", return_value=directory / "ini"), patch.object(imgui, "push_id", record_row), patch.object(imgui, "end_table", scroll_table), patch.object(imgui, "end_child", scroll_tree), patch.object(imgui, "begin", scroll_details), patch.object(imgui, "collapsing_header", open_preview), patch.object(imgui, "input_text", show_repair_fields), patch.object(imgui, "input_text_multiline", show_preview):
            app.run()
        if args.scroll_bottom and args.rows:
            assert f"Fixture Mod {args.rows - 1:05d}" in drawn_names, "Last library row was not rendered after scrolling."
        if args.view == "migrated":
            assert manager.instance_migration.state == "complete" and len(manager.instance_migration.referenced) == 5
            assert not any((manager.library.root / "mods").iterdir())
            assert len(manager.profiles) == 3 and workspace._dialog == ""
            assert len(drawn_names) >= 3, f"Themed library rows are below the fold: only {drawn_names} drawn."
        if args.view == "failed":
            assert manager.job_state == "failed" and "missing-payload" in manager.error
        elif args.view == "cancelled":
            assert manager.job_state == "cancelled" and len(manager.rows) == 4
            assert not any((manager.library.root / "mods").iterdir())
            assert not any((manager.library.root / ".staging").iterdir())
        elif args.view == "copied":
            assert manager.job_state == "completed" and sum(row.kind == "owned" for row in manager.rows) == len(selected)
            assert len(manager.origins) == len(selected)
        elif args.view == "copy-failed":
            assert manager.job_state == "failed" and "changed after review" in manager.error
            assert all(row.kind != "owned" for row in manager.rows)
            for row in manager.rows:
                if row.kind == "external":
                    latest = contents(row.source)
                    latest.pop("new-after-review.txt", None)
                    assert latest == original[row.id]
        elif args.view == "copy-cancelled":
            assert manager.job_state == "cancelled" and all(row.kind != "owned" for row in manager.rows)
            assert not manager.origins and not any((manager.library.root / ".staging").iterdir())
        Image.fromarray(hello_imgui.final_app_window_screenshot()).save(args.output)
        if args.metrics:
            ordered = sorted(frame_times)
            assert ordered, "No native frame metrics were captured"
            args.metrics.parent.mkdir(parents=True, exist_ok=True)
            args.metrics.write_text(json.dumps({
                "rows": len(manager.rows), "size": [args.width, args.height], "frames": frames,
                "log_visible": args.show_log, "host": args.host, "integrated": args.integrated,
                "nexus_badges_fixture": args.nexus_badges,
                "nexus_account_fixture": args.nexus_account, "download_destination_fixture": args.download_destination,
                "warm_samples": len(ordered), "frame_ms_p50": ordered[len(ordered) // 2],
                "frame_ms_p95": ordered[min(len(ordered) - 1, int(len(ordered) * .95))],
                "frame_ms_max": ordered[-1], "peak_python_allocated_bytes": tracemalloc.get_traced_memory()[1],
                "peak_sampled_resident_bytes": max(resident_bytes) if resident_bytes else None,
                "last_row_rendered": f"Fixture Mod {args.rows - 1:05d}" in drawn_names if args.rows else None,
                "limitations": "Generated metadata; native hidden-window host; backend frame delta includes renderer/host/frame pacing, tracing and memory sampling. Cold setup/import and real mod payload/game I/O excluded from frame samples. RSS covers the entire toolkit host, not just this module.",
            }, indent=2), encoding="utf-8")
            tracemalloc.stop()
        print(f"Rendered {args.view}: {args.output} ({frames} frames, {len(manager.rows):,} rows)")


if __name__ == "__main__":
    main()
