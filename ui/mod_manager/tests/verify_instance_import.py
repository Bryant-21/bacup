"""Verify a user-selected instance into a disposable library; never copy mod payloads."""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.mo2_migration import discover
from creation_lib.mod_manager.paths import reject_links


def finish(manager):
    manager._future.result(timeout=30)
    manager.poll()
    assert manager.job_state == "completed", manager.error


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--instance", type=Path, required=True)
    parser.add_argument("--approve-discovered-targets", action="store_true",
                        help="Explicitly approve ordinary discovered targets for this temporary verification only.")
    parser.add_argument("--automatic-discovery-only", action="store_true", help="Verify the primary migration's metadata-only local-link discovery. No library or payload scans/copies.")
    args = parser.parse_args()
    initial = inspect_instance(args.instance)
    mappings = {link.alias: link.target for link in initial.linked_sources if link.can_use_target} if args.approve_discovered_targets else {}
    if args.automatic_discovery_only:
        with tempfile.TemporaryDirectory(prefix="modlist-discovery-") as temporary:
            preview, auto, issues = discover(args.instance, Path(temporary) / "library")
    else:
        preview = inspect_instance(args.instance, source_mappings=mappings)
    files = {Path(preview.root) / "ModOrganizer.ini"}
    files.update(Path(profile.source) / name for profile in preview.profiles for name in profile.files)
    files.update(Path(mod.source) / "meta.ini" for mod in preview.inventory if mod.source)
    files.update(Path(mod.source) / "meta.ini" for profile in preview.profiles for mod in profile.mods if mod.source)
    files = {path for path in files if path.is_file()}
    def snapshot():
        result = {}
        for path in files:
            reject_links(path)
            result[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
        return result
    before = snapshot()
    if args.automatic_discovery_only:
        with tempfile.TemporaryDirectory(prefix="modlist-discovery-") as temporary:
            repeated, repeated_auto, repeated_issues = discover(args.instance, Path(temporary) / "library")
        assert snapshot() == before and repeated_auto == auto and repeated_issues == issues
        print(json.dumps({"profiles": {p.name: len(p.mods) for p in preview.profiles}, "inventory_mod_folders": len(preview.inventory),
                          "automatic_ordinary_local_links": auto, "exceptions": issues, "metadata_files_verified_unchanged": len(before),
                          "read_only_discovery": True, "payloads_scanned": False, "payloads_copied": False, "library_created": False,
                          "repeat_profiles": len(repeated.profiles)}, ensure_ascii=True))
        return
    with tempfile.TemporaryDirectory(prefix="modlist-import-verification-") as temporary:
        manager = ManagerController()
        try:
            manager.import_preview = preview
            manager.open_library(Path(temporary) / "library", create=True)
            finish(manager)
            selected = {profile.id for profile in preview.profiles}
            manager.import_profiles(selected)
            finish(manager)
            counts = {profile.name: len(manager.library.rows(profile.id)) for profile in manager.profiles}
            first_status = manager.status
            manager.import_profiles(selected)
            finish(manager)
            assert "Imported 0 new profiles" in manager.status
            assert snapshot() == before, "Source metadata changed during read-only verification."
            print(json.dumps({"profiles": counts, "linked_sources": len(preview.linked_sources),
                              "approved_sources": sum(bool(link.source) for link in preview.linked_sources),
                              "unavailable_mods": sum(mod.kind == "external" and not mod.source for profile in preview.profiles for mod in profile.mods),
                              "metadata_files_verified_unchanged": len(before), "first_import": first_status,
                              "repeat_import": manager.status, "payloads_copied": False, "destination": "disposable temporary library"}, ensure_ascii=True))
        finally:
            manager.cleanup()


if __name__ == "__main__":
    main()
