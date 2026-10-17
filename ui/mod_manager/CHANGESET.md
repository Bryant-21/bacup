# Mod-manager change inventory

Implementation is primarily in `ui/mod_manager`. Eight previously reported shared integration paths are included: toolkit archive routing/first-run entrypoint, standalone variant, the authoring workspace display name, its variant test map, early helper dispatch in app.py and private native payload/public-build preflight in ModBox21.spec/build_toolkit.ps1. The existing workspace registry already routes `mod_manager` to this feature. The current customization/metadata/owned-plugin continuation changes dedicated manager files only. Shared styling, dependencies, ESM editor and record/session services were not edited. Unrelated working changes were preserved. No commit, push, publication or real-game deployment was performed.

The existing controller/library/package/transaction/runtime foundations were extended through dedicated services and UI modules. SQLite metadata, worker jobs, native clipped tree controls and the existing modern theme remain the architecture. Payload ownership, source references, local profile state and native launch are explicit.

Capabilities and exact coverage: [README.md](README.md). Remaining acceptance: [PARITY_REVIEW.md](PARITY_REVIEW.md). Runtime setup/licensing: [F4SE_TARGETS.md](F4SE_TARGETS.md). Checkpoint: [PROGRESS.md](PROGRESS.md).

The cumulative feature inventory links **317 paths: 309 dedicated manager files and eight reported shared integration files**. Earlier changes are present in the concurrent checkout baseline. The installer continuation adds eighteen dedicated paths; Nexus controls add six and routing/batches add eleven. No new shared integration, source dependency or native API change was introduced. This inventory is cumulative, not a claim that every path changed in this last turn.

## Changed or added files

Browser routing/batch continuation additions:

- [NEXUS_ROUTING.md](NEXUS_ROUTING.md)
- [pipe_peers.py](pipe_peers.py)
- [nxm_instances.py](nxm_instances.py)
- [nxm_routing.py](nxm_routing.py)
- [nxm_routing_ui.py](nxm_routing_ui.py)
- [nexus_bulk.py](nexus_bulk.py)
- [nexus_bulk_ui.py](nexus_bulk_ui.py)
- [nxm_process_fixture.py](tests/nxm_process_fixture.py)
- [test_nxm_routing.py](tests/test_nxm_routing.py)
- [test_nexus_bulk.py](tests/test_nexus_bulk.py)
- [test_nxm_bulk_native.py](tests/test_nxm_bulk_native.py)

Existing dedicated changes in this pass: browser_links.py, integration.py, controller.py,
workspace.py, toolbar_ui.py, tests/test_nexus.py, tests/test_widget_ids.py,
tests/render_workspace.py and these coverage/review documents. No shared app paths changed.

Nexus continuation additions:

- [NEXUS_CONTROLS.md](NEXUS_CONTROLS.md)
- [nexus_batch.py](nexus_batch.py)
- [nexus_account.py](nexus_account.py)
- [nexus_account_ui.py](nexus_account_ui.py)
- [test_nexus_controls.py](tests/test_nexus_controls.py)
- [test_nexus_controls_native.py](tests/test_nexus_controls_native.py)

Existing dedicated changes in the Nexus pass: nexus.py, nexus_state.py, nexus_links.py,
controller.py, target_mod_sources.py, target_customization_ui.py, workspace.py,
download_install.py, download_install_ui.py, tests/test_widget_ids.py,
tests/render_workspace.py and these linked documentation/review files.

Installer continuation adds eighteen dedicated paths; no shared integration path changed:

- [INSTALLER_COVERAGE.md](INSTALLER_COVERAGE.md)
- [installer_archives.py](installer_archives.py)
- [installer_versions.py](installer_versions.py)
- [installer_repair.py](installer_repair.py)
- [installer_repair_ui.py](installer_repair_ui.py)
- [installer_wizard.py](installer_wizard.py)
- [installer_decoration.py](installer_decoration.py)
- [installer_decoration_ui.py](installer_decoration_ui.py)
- [reference_overlay.py](reference_overlay.py)
- [package_preview.py](package_preview.py)
- [package_preview_ui.py](package_preview_ui.py)
- [tree_regression_probe.py](tests/tree_regression_probe.py)
- [test_installer_discovery.py](tests/test_installer_discovery.py)
- [test_installer_repairs.py](tests/test_installer_repairs.py)
- [test_installer_decoration.py](tests/test_installer_decoration.py)
- [test_installer_wizard_native.py](tests/test_installer_wizard_native.py)
- [test_reference_overlay.py](tests/test_reference_overlay.py)
- [test_package_preview.py](tests/test_package_preview.py)

Existing dedicated changes in this pass: controller.py, installer_context.py, fomod.py,
package.py, workspace.py, mod_list_ui.py, library.py, transactions.py, launch_targets.py,
plugin_state.py, tests/test_installer_context.py, tests/render_workspace.py and the linked
README/PROGRESS/PARITY_REVIEW/ACCEPTANCE_REVIEW/UI_WORKFLOWS/FOMOD_DIFFERENTIAL/CHANGESET docs.

Current dedicated additions:

- [CUSTOMIZATION.md](CUSTOMIZATION.md)
- [columns.py](columns.py)
- [columns_ui.py](columns_ui.py)
- [shortcuts.py](shortcuts.py)
- [shortcuts_ui.py](shortcuts_ui.py)
- [metadata_observer.py](metadata_observer.py)
- [metadata_notifications_ui.py](metadata_notifications_ui.py)
- [plugin_order.py](plugin_order.py)
- [plugin_file_actions.py](plugin_file_actions.py)
- [plugin_file_ui.py](plugin_file_ui.py)
- [plugin_flag_writer.py](plugin_flag_writer.py)
- [tests/test_columns.py](tests/test_columns.py)
- [tests/test_shortcuts.py](tests/test_shortcuts.py)
- [tests/test_metadata_observer.py](tests/test_metadata_observer.py)
- [tests/metadata_observer_probe.py](tests/metadata_observer_probe.py)
- [tests/test_plugin_file_actions.py](tests/test_plugin_file_actions.py)
- [tests/test_preferences_native.py](tests/test_preferences_native.py)
- [tests/test_parity_workflows_native.py](tests/test_parity_workflows_native.py)

Current integration updates controller.py, library.py, mod_list_ui.py, toolbar_ui.py,
workspace.py, panes_ui.py, plugin_keys.py, plugin_priority.py, plugin_state.py,
installer_context.py, source_metadata_ui.py. Existing test_plugin_priority.py,
test_widget_ids.py and render_workspace.py add shifting/input/ghost-context/render coverage.
README.md, UI_WORKFLOWS.md, ACCEPTANCE_REVIEW.md, PARITY_REVIEW.md and PROGRESS.md document
the defined coverage and remaining acceptance. All are already linked in this inventory.

Latest parity reconciliation corrects current remaining-acceptance rows and stale in-app
guidance. It updates nine existing dedicated paths: workspace.py, toolbar_ui.py,
PARITY_REVIEW.md, ACCEPTANCE_REVIEW.md, F4SE_ACCEPTANCE.md, UI_WORKFLOWS.md, README.md,
PROGRESS.md and CHANGESET.md. No new feature, shared integration or native contract change
is introduced. The latest private packaged pixel capture remains explicitly unverified.

Latest dedicated additions:

- [launch_resolution.py](launch_resolution.py)
- [local_status.py](local_status.py)
- [executable_catalog.py](executable_catalog.py)
- [executable_editor_ui.py](executable_editor_ui.py)
- [native/tool_fixture.inc](native/tool_fixture.inc)
- [tests/project_tool_probe.py](tests/project_tool_probe.py)
- [tests/test_launch_resolution.py](tests/test_launch_resolution.py)
- [tests/test_local_status.py](tests/test_local_status.py)
- [tests/test_executable_catalog.py](tests/test_executable_catalog.py)
- [tests/test_priority_native.py](tests/test_priority_native.py)

Integration is confined to existing manager controller/runtime/launcher/target controls,
workspace/pane/toolbar/modlist/status services, archive/combined indexes, native lease/rename
code and fixture dispatch, native render/input tests and the linked manager documentation.
The eight shared integration paths below belong to earlier checkpoints. This continuation
does not touch shared registration/navigation/styling or the ESM editor/session services.

Owned mutations and private acceptance:

- [NATIVE_API_MATRIX.md](NATIVE_API_MATRIX.md)
- [native/overlay_state.inc](native/overlay_state.inc)
- [native/mutations.inc](native/mutations.inc)
- [native/mutation_fixture.inc](native/mutation_fixture.inc)
- [tests/project_mutation_probe.py](tests/project_mutation_probe.py)
- [tests/private_frozen_probe.py](tests/private_frozen_probe.py)

The same continuation updates these thirteen existing files, for nineteen files total:

- [native/filespace.cpp](native/filespace.cpp), [native/fixture.cpp](native/fixture.cpp), [native/CMakeLists.txt](native/CMakeLists.txt)
- [native_builder.py](native_builder.py), [runtime_helper.py](runtime_helper.py), [runtime_delivery.py](runtime_delivery.py)
- [tests/project_runtime_probe.py](tests/project_runtime_probe.py), [tests/test_native_builder.py](tests/test_native_builder.py)
- [PROJECT_VFS.md](PROJECT_VFS.md), [PROGRESS.md](PROGRESS.md), [README.md](README.md), [PARITY_REVIEW.md](PARITY_REVIEW.md), [CHANGESET.md](CHANGESET.md)

Reported fixes and target customization:

- [ACCEPTANCE_REVIEW.md](ACCEPTANCE_REVIEW.md)
- [browser_links.py](browser_links.py)
- [nxm_entry.py](nxm_entry.py)
- [status.py](status.py)
- [tests/test_nexus.py](tests/test_nexus.py)
- [tests/test_tree_status.py](tests/test_tree_status.py)
- [REPORTED_FIXES.md](REPORTED_FIXES.md)
- [diagnostics.py](diagnostics.py)
- [runtime_setup.py](runtime_setup.py)
- [status_icons.py](status_icons.py)
- [tests/test_manager_diagnostics.py](tests/test_manager_diagnostics.py)
- [tests/test_runtime_setup.py](tests/test_runtime_setup.py)
- [mo2_migration.py](mo2_migration.py)
- [native_runtime.py](native_runtime.py)
- [tests/test_instance_migration.py](tests/test_instance_migration.py)
- [target_options.py](target_options.py)
- [target_mod_sources.py](target_mod_sources.py)
- [target_preferences.py](target_preferences.py)
- [target_removal.py](target_removal.py)
- [target_customization_ui.py](target_customization_ui.py)
- [tests/test_target_customization.py](tests/test_target_customization.py)
- [tests/test_target_preferences.py](tests/test_target_preferences.py)

Latest dedicated additions: combined loose/archive resolution, FO4 plugin-table comparisons,
metadata adapter API, schema differential fixtures, reviewed reference/separator removal,
save screenshots/bounded caches, selected plugin block drag, retained-bundle cleanup and
explicit default global FO4 save discovery.

Latest integration and organization additions:

- [archive_intake.py](archive_intake.py)
- [mod_drag.py](mod_drag.py)
- [row_edit.py](row_edit.py)
- [group_names.py](group_names.py)
- [tests/test_app_routing.py](tests/test_app_routing.py)
- [tests/test_mod_drag.py](tests/test_mod_drag.py)
- [tests/test_row_edit.py](tests/test_row_edit.py)
- [tests/test_group_names.py](tests/test_group_names.py)
- [download_locks.py](download_locks.py)
- [download_scheduler.py](download_scheduler.py)
- [tests/test_download_scheduler.py](tests/test_download_scheduler.py)
- [reference_sources.py](reference_sources.py)
- [reference_sources_ui.py](reference_sources_ui.py)
- [tests/test_reference_sources.py](tests/test_reference_sources.py)
- [source_metadata.py](source_metadata.py)
- [source_metadata_ui.py](source_metadata_ui.py)
- [tests/test_source_metadata.py](tests/test_source_metadata.py)
- [plugin_priority.py](plugin_priority.py)
- [plugin_priority_ui.py](plugin_priority_ui.py)
- [tests/test_plugin_priority.py](tests/test_plugin_priority.py)
- [launch_targets.py](launch_targets.py)
- [launch_targets_ui.py](launch_targets_ui.py)
- [f4se_setup.py](f4se_setup.py)
- [pe_metadata.py](pe_metadata.py)
- [tests/test_launch_targets.py](tests/test_launch_targets.py)
- [tests/test_f4se_setup.py](tests/test_f4se_setup.py)
- [tests/test_pe_metadata.py](tests/test_pe_metadata.py)
- [F4SE_TARGETS.md](F4SE_TARGETS.md)
- [f4se_root_setup.py](f4se_root_setup.py)
- [f4se_root_ui.py](f4se_root_ui.py)
- [tests/test_f4se_root_setup.py](tests/test_f4se_root_setup.py)
- [F4SE_ACCEPTANCE.md](F4SE_ACCEPTANCE.md)
- [F4SE_REAL_BOOT_PLAN.md](F4SE_REAL_BOOT_PLAN.md)
- [Toolkit archive routing](../toolkit/app.py)
- [Toolkit first-run entrypoint](../toolkit/__main__.py)
- [Standalone variant](../toolkit/variants.py)
- [Authoring workspace name](../toolkit/workspaces/mod_builder_workspace.py)
- [Variant expectations](../toolkit/tests/test_variants.py)

- [EXTENSIONS.md](EXTENSIONS.md)
- [UI_WORKFLOWS.md](UI_WORKFLOWS.md)
- [FOMOD_DIFFERENTIAL.md](FOMOD_DIFFERENTIAL.md)
- [combined_data.py](combined_data.py)
- [extensions.py](extensions.py)
- [reference_removal.py](reference_removal.py)
- [save_inspection.py](save_inspection.py)
- [save_inspection_ui.py](save_inspection_ui.py)
- [metadata_cache.py](metadata_cache.py)
- [plugin_drag.py](plugin_drag.py)
- [backup_prune.py](backup_prune.py)
- [backup_prune_ui.py](backup_prune_ui.py)
- [global_saves.py](global_saves.py)
- [tests/benchmark_metadata_adapters.py](tests/benchmark_metadata_adapters.py)
- [tests/fomod_reference_corpus.py](tests/fomod_reference_corpus.py)
- [tests/test_combined_data.py](tests/test_combined_data.py)
- [tests/test_extensions.py](tests/test_extensions.py)
- [tests/test_fomod_reference_corpus.py](tests/test_fomod_reference_corpus.py)
- [tests/test_reference_removal.py](tests/test_reference_removal.py)
- [tests/test_save_inspection.py](tests/test_save_inspection.py)
- [tests/test_save_screenshot.py](tests/test_save_screenshot.py)
- [tests/test_metadata_cache.py](tests/test_metadata_cache.py)
- [tests/test_plugin_drag.py](tests/test_plugin_drag.py)
- [tests/test_backup_prune.py](tests/test_backup_prune.py)
- [tests/test_global_saves.py](tests/test_global_saves.py)
- [tests/validate_fomod_schemas.ps1](tests/validate_fomod_schemas.ps1)

- [PARITY_REVIEW.md](PARITY_REVIEW.md)
- [PROGRESS.md](PROGRESS.md)
- [README.md](README.md)
- [archive_index.py](archive_index.py)
- [categories.py](categories.py)
- [cli_reader.py](cli_reader.py)
- [conflicts.py](conflicts.py)
- [controller.py](controller.py)
- [data_tree.py](data_tree.py)
- [dependency_environment.py](dependency_environment.py)
- [download_queue.py](download_queue.py)
- [file_actions.py](file_actions.py)
- [folder_actions.py](folder_actions.py)
- [fomod.py](fomod.py)
- [fomod_compatibility.py](fomod_compatibility.py)
- [history_prune.py](history_prune.py)
- [installation.py](installation.py)
- [installer_context.py](installer_context.py)
- [installer_preview.py](installer_preview.py)
- [instances.py](instances.py)
- [launcher_ui.py](launcher_ui.py)
- [library.py](library.py)
- [metadata_ui.py](metadata_ui.py)
- [mo2.py](mo2.py)
- [mod_list_ui.py](mod_list_ui.py)
- [mod_names.py](mod_names.py)
- [nexus.py](nexus.py)
- [nexus_links.py](nexus_links.py)
- [nexus_state.py](nexus_state.py)
- [organization.py](organization.py)
- [overwrite.py](overwrite.py)
- [package.py](package.py)
- [panes_ui.py](panes_ui.py)
- [plugin_keys.py](plugin_keys.py)
- [plugin_state.py](plugin_state.py)
- [profile_backup.py](profile_backup.py)
- [profile_recipes.py](profile_recipes.py)
- [profile_state.py](profile_state.py)
- [profile_transfers_ui.py](profile_transfers_ui.py)
- [read_process.py](read_process.py)
- [runtime.py](runtime.py)
- [runtime_helper.py](runtime_helper.py)
- [save_backups.py](save_backups.py)
- [save_transfer.py](save_transfer.py)
- [saves.py](saves.py)
- [tests/benchmark_manager_features.py](tests/benchmark_manager_features.py)
- [tests/render_workspace.py](tests/render_workspace.py)
- [tests/test_archive_index.py](tests/test_archive_index.py)
- [tests/test_bounded_metadata.py](tests/test_bounded_metadata.py)
- [tests/test_categories.py](tests/test_categories.py)
- [tests/test_cli_reader.py](tests/test_cli_reader.py)
- [tests/test_controller.py](tests/test_controller.py)
- [tests/test_data_tree.py](tests/test_data_tree.py)
- [tests/test_dependency_environment.py](tests/test_dependency_environment.py)
- [tests/test_download_queue.py](tests/test_download_queue.py)
- [tests/test_file_actions.py](tests/test_file_actions.py)
- [tests/test_folder_actions.py](tests/test_folder_actions.py)
- [tests/test_fomod.py](tests/test_fomod.py)
- [tests/test_fomod_compatibility.py](tests/test_fomod_compatibility.py)
- [tests/test_fomod_policy_controller.py](tests/test_fomod_policy_controller.py)
- [tests/test_history_prune.py](tests/test_history_prune.py)
- [tests/test_installation.py](tests/test_installation.py)
- [tests/test_installer_context.py](tests/test_installer_context.py)
- [tests/test_installer_context_controller.py](tests/test_installer_context_controller.py)
- [tests/test_installer_preview.py](tests/test_installer_preview.py)
- [tests/test_mod_names.py](tests/test_mod_names.py)
- [tests/test_nexus_links.py](tests/test_nexus_links.py)
- [tests/test_nexus_state.py](tests/test_nexus_state.py)
- [tests/test_organization.py](tests/test_organization.py)
- [tests/test_overwrite.py](tests/test_overwrite.py)
- [tests/test_plugin_state.py](tests/test_plugin_state.py)
- [tests/test_profile_backup.py](tests/test_profile_backup.py)
- [tests/test_profile_recipes.py](tests/test_profile_recipes.py)
- [tests/test_save_backups.py](tests/test_save_backups.py)
- [tests/test_save_transfer.py](tests/test_save_transfer.py)
- [tests/test_saves.py](tests/test_saves.py)
- [tests/test_source_saves.py](tests/test_source_saves.py)
- [tests/test_standalone.py](tests/test_standalone.py)
- [tests/test_widget_ids.py](tests/test_widget_ids.py)
- [toolbar_ui.py](toolbar_ui.py)
- [transactions.py](transactions.py)
- [workspace.py](workspace.py)
- [CHANGESET.md](CHANGESET.md)

## Original native filespace, daily workflows and reconciled support files

- [PROJECT_VFS.md](PROJECT_VFS.md)
- [USVFS_LICENSE.txt](USVFS_LICENSE.txt)
- [__init__.py](__init__.py)
- [download_install.py](download_install.py)
- [download_install_ui.py](download_install_ui.py)
- [integration.py](integration.py)
- [launcher.py](launcher.py)
- [locations.py](locations.py)
- [migration.py](migration.py)
- [native/CMakeLists.txt](native/CMakeLists.txt)
- [native/filespace.cpp](native/filespace.cpp)
- [native/filespace.hpp](native/filespace.hpp)
- [native/fixture.cpp](native/fixture.cpp)
- [native/injection.cpp](native/injection.cpp)
- [native/vendor/minhook/LICENSE.txt](native/vendor/minhook/LICENSE.txt)
- [native/vendor/minhook/include/MinHook.h](native/vendor/minhook/include/MinHook.h)
- [native/vendor/minhook/src/buffer.c](native/vendor/minhook/src/buffer.c)
- [native/vendor/minhook/src/buffer.h](native/vendor/minhook/src/buffer.h)
- [native/vendor/minhook/src/hde/hde32.c](native/vendor/minhook/src/hde/hde32.c)
- [native/vendor/minhook/src/hde/hde32.h](native/vendor/minhook/src/hde/hde32.h)
- [native/vendor/minhook/src/hde/hde64.c](native/vendor/minhook/src/hde/hde64.c)
- [native/vendor/minhook/src/hde/hde64.h](native/vendor/minhook/src/hde/hde64.h)
- [native/vendor/minhook/src/hde/pstdint.h](native/vendor/minhook/src/hde/pstdint.h)
- [native/vendor/minhook/src/hde/table32.h](native/vendor/minhook/src/hde/table32.h)
- [native/vendor/minhook/src/hde/table64.h](native/vendor/minhook/src/hde/table64.h)
- [native/vendor/minhook/src/hook.c](native/vendor/minhook/src/hook.c)
- [native/vendor/minhook/src/trampoline.c](native/vendor/minhook/src/trampoline.c)
- [native/vendor/minhook/src/trampoline.h](native/vendor/minhook/src/trampoline.h)
- [native_builder.py](native_builder.py)
- [paths.py](paths.py)
- [process_identity.py](process_identity.py)
- [project_vfs.py](project_vfs.py)
- [runtime_delivery.py](runtime_delivery.py)
- [runtime_entry.py](runtime_entry.py)
- [sevenzip.py](sevenzip.py)
- [status_query.py](status_query.py)
- [status_query_ui.py](status_query_ui.py)
- [tests/__init__.py](tests/__init__.py)
- [tests/benchmark_archives.py](tests/benchmark_archives.py)
- [tests/benchmark_library.py](tests/benchmark_library.py)
- [tests/benchmark_project_vfs.py](tests/benchmark_project_vfs.py)
- [tests/crash_worker.py](tests/crash_worker.py)
- [tests/fixtures.py](tests/fixtures.py)
- [tests/native_runtime_probe.py](tests/native_runtime_probe.py)
- [tests/project_runtime_probe.py](tests/project_runtime_probe.py)
- [tests/project_vfs_probe.py](tests/project_vfs_probe.py)
- [tests/test_archives.py](tests/test_archives.py)
- [tests/test_conflicts.py](tests/test_conflicts.py)
- [tests/test_daily_native.py](tests/test_daily_native.py)
- [tests/test_download_install.py](tests/test_download_install.py)
- [tests/test_import_destination.py](tests/test_import_destination.py)
- [tests/test_launcher.py](tests/test_launcher.py)
- [tests/test_library.py](tests/test_library.py)
- [tests/test_migration.py](tests/test_migration.py)
- [tests/test_mo2.py](tests/test_mo2.py)
- [tests/test_native_builder.py](tests/test_native_builder.py)
- [tests/test_native_runtime_binding.py](tests/test_native_runtime_binding.py)
- [tests/test_package_metadata.py](tests/test_package_metadata.py)
- [tests/test_process_identity.py](tests/test_process_identity.py)
- [tests/test_profile_lifecycle.py](tests/test_profile_lifecycle.py)
- [tests/test_runtime.py](tests/test_runtime.py)
- [tests/test_status_query.py](tests/test_status_query.py)
- [tests/test_transactions.py](tests/test_transactions.py)
- [tests/verify_instance_import.py](tests/verify_instance_import.py)
- [tree.py](tree.py)
- [Private native payload preflight](../../ModBox21.spec)
- [Retained public-build gate](../../build_toolkit.ps1)
- [Frozen helper entrypoint](../../app.py)
- [native/LICENSE.txt](native/LICENSE.txt)
