"""Read-only instance discovery, including redirected bases and metadata-only entries."""
import hashlib
import os
from pathlib import Path
import subprocess

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import ManagerError
from .fixtures import make_mo2


@pytest.mark.parametrize("raw,expected", [
    ('  "ordinary value"  ', "ordinary value"),
    (r"@ByteArray(C:\\Games\\Fallout\x204)", "C:\\Games\\Fallout 4"),
    (r"@ByteArray(\xc3\x84sthetik)", "Ästhetik"),
    (r"@ByteArray(\xe9)", "é"),
    ("@ByteArray(東京)", "東京"),
])
def test_qt_scalar_decoding_preserves_paths_and_unicode(raw, expected):
    from creation_lib.mod_manager.mo2 import qt_string
    assert qt_string(raw) == expected


def test_executable_metadata_filter_preserves_unicode_case_matching(tmp_path):
    import configparser
    from creation_lib.mod_manager.mo2 import instance_configuration
    config = configparser.ConfigParser(interpolation=None)
    config.read_string("[customExecutables]\n1\\title=Fixture\n1\\arguments=--apİkey fixture\n")
    result = instance_configuration(config, tmp_path, tmp_path, tmp_path, tmp_path)
    assert result["executables"][0]["arguments"] == "[credential-bearing command field omitted]"


def test_locked_plugin_order_is_an_inert_snapshot(tmp_path):
    source = make_mo2(tmp_path)
    locked = source / "profiles/Default/lockedorder.txt"
    locked.write_text("Fixture.esp|2\n", encoding="utf-8", newline="\n")
    preview = inspect_instance(source)
    profile = next(profile for profile in preview.profiles if profile.name == "Default")
    assert profile.files["lockedorder.txt"] == "Fixture.esp|2\n"
    assert locked.read_text() == "Fixture.esp|2\n"


def test_linked_metadata_is_rejected_before_is_file_probe(tmp_path, monkeypatch):
    from creation_lib.mod_manager.mo2 import read_ini
    target = tmp_path / "metadata"
    target.mkdir()
    (target / "meta.ini").write_text("generated metadata")
    link = tmp_path / "linked-metadata"
    junction(link, target)
    path = link / "meta.ini"
    original_is_file = Path.is_file
    def forbidden_probe(current, *args, **kwargs):
        assert current != path, "Linked metadata must be rejected before probing its target"
        return original_is_file(current, *args, **kwargs)
    monkeypatch.setattr(Path, "is_file", forbidden_probe)
    with pytest.raises(ManagerError, match="reparse"):
        read_ini(path)


def junction(link, target):
    if os.name == "nt":
        result = subprocess.run(["cmd.exe", "/c", "mklink", "/J", str(link), str(target)],
                                capture_output=True, check=False)
        assert result.returncode == 0, result.stderr
    else:
        link.symlink_to(target, target_is_directory=True)


def hashes(paths):
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}


def test_redirected_instance_junctions_and_unmanaged_labels_import_without_reading_links(tmp_path, monkeypatch):
    container = tmp_path / "External data"
    container.mkdir()
    base = make_mo2(container)
    instance = tmp_path / "Instance name"
    instance.mkdir()
    encoded_base = base.as_posix().replace(" ", r"\x20")
    ini = instance / "ModOrganizer.ini"
    ini.write_text(f"[General]\ngameName=Fallout 4\n[Settings]\nbase_directory=@ByteArray({encoded_base})\nmod_directory=%base_dir%/mods\nprofiles_directory=%BASE_DIR%/profiles\n", encoding="utf-8")
    profile = base / "profiles" / "Default" / "modlist.txt"
    profile.write_text("+Linked content\n*Unmanaged: ExamplePlugin\n+Base\n", encoding="utf-8")
    target = tmp_path / "Link target"
    target.mkdir()
    protected = target / "meta.ini"
    protected.write_text("This metadata must never be read.")
    links = [base / "mods" / "Linked content", base / "mods" / "Unused link"]
    for link in links:
        junction(link, target)
    original_read = Path.read_bytes
    before = hashes([ini, profile, protected])
    def guarded_read(path):
        assert not path.is_relative_to(target)
        assert not any(path.is_relative_to(link) for link in links)
        return original_read(path)
    try:
        with monkeypatch.context() as patch:
            patch.setattr(Path, "read_bytes", guarded_read)
            preview = inspect_instance(ini)
        assert preview.mods_root == str((base / "mods").resolve())
        assert preview.profiles_root == str((base / "profiles").resolve())
        assert len([warning for warning in preview.warnings if warning.startswith("Skipped linked")]) == 2
        imported = next(value for value in preview.profiles if value.name == "Default")
        values = {mod.name: mod for mod in imported.mods}
        assert values["Unmanaged: ExamplePlugin"].kind == "foreign" and not values["Unmanaged: ExamplePlugin"].source
        assert values["Linked content"].enabled and not values["Linked content"].source
        library = Library(tmp_path / "library", create=True)
        assert library.import_mo2(preview, {imported.id}) == 1
        assert library.import_mo2(preview, {imported.id}) == 0
        library.customize(imported.id, values["Unmanaged: ExamplePlugin"].id, enabled=False)
        with pytest.raises(ManagerError, match="Linked content.*no importable"):
            library.export_plan(imported.id)
        assert hashes([ini, profile, protected]) == before
    finally:
        for link in links:
            if os.name == "nt":
                link.rmdir()  # Only the fixture junction is removed; its target stays intact.
            else:
                link.unlink()
    assert protected.read_text() == "This metadata must never be read."


@pytest.mark.parametrize("encoding", ["utf-8-sig", "utf-16", "cp1252"])
def test_empty_profile_with_absent_mods_directory_is_valid_metadata_import(tmp_path, encoding):
    root = tmp_path / "Empty instance"
    profile = root / "profiles" / "Default"
    profile.mkdir(parents=True)
    (root / "ModOrganizer.ini").write_text("[General]\ngameName=Fallout 4\n[Settings]\n", encoding=encoding)
    (profile / "modlist.txt").write_text("# Empty MO2 profile\n", encoding=encoding)
    preview = inspect_instance(root)
    assert len(preview.profiles) == 1 and preview.profiles[0].mods == []
    assert any("mods directory is missing" in warning for warning in preview.warnings)
    library = Library(tmp_path / "library", create=True)
    assert library.import_mo2(preview) == 1
    assert library.rows(library.profiles()[0].id) == []


def test_unmanaged_display_text_is_not_a_file_path_but_managed_names_still_are(tmp_path):
    root = make_mo2(tmp_path)
    modlist = root / "profiles" / "Default" / "modlist.txt"
    modlist.write_text("*Unmanaged: ExamplePlugin\n")
    preview = inspect_instance(modlist.parent)
    assert preview.profiles[0].mods[0].kind == "foreign"
    modlist.write_text("+../Escape\n")
    with pytest.raises(ManagerError, match="Unsafe Windows path"):
        inspect_instance(modlist.parent)


def test_missing_selected_path_has_actionable_error(tmp_path):
    with pytest.raises(ManagerError, match="instance or profile path does not exist"):
        inspect_instance(tmp_path / "missing")


def test_missing_profiles_directory_and_no_discoverable_profiles_are_clear(tmp_path):
    root = tmp_path / "instance"
    root.mkdir()
    with pytest.raises(ManagerError, match="profiles directory is missing"):
        inspect_instance(root)
    (root / "profiles").mkdir()
    with pytest.raises(ManagerError, match="No MO2 profiles containing modlist.txt"):
        inspect_instance(root)


def test_linked_profile_is_skipped_but_linked_selected_root_is_rejected(tmp_path):
    root = make_mo2(tmp_path)
    link = root / "profiles" / "Linked profile"
    junction(link, root / "profiles" / "Default")
    try:
        preview = inspect_instance(root)
        assert len(preview.profiles) == 2
        assert any("Skipped linked or reparse-point profile" in warning for warning in preview.warnings)
        with pytest.raises(ManagerError, match="Links and Windows reparse"):
            inspect_instance(link)
    finally:
        link.rmdir() if os.name == "nt" else link.unlink()


def test_approved_mapping_repairs_existing_import_and_stages_without_changing_source(tmp_path):
    root = make_mo2(tmp_path)
    alias = root / "mods" / "Linked content"
    target = tmp_path / "target"
    (target / "textures").mkdir(parents=True)
    payload = target / "textures" / "linked.txt"
    payload.write_bytes(b"synthetic linked payload")
    (target / "meta.ini").write_text("[General]\nversion=1\n")
    (root / "profiles" / "Default" / "modlist.txt").write_text("+Linked content\n+Base\n")
    junction(alias, target)
    try:
        preview = inspect_instance(root)
        profile = next(profile for profile in preview.profiles if profile.name == "Default")
        link = preview.linked_sources[0]
        assert link.can_use_target and not link.source and link.status == "Source approval required"
        mod = next(mod for mod in profile.mods if mod.name == "Linked content")
        library = Library(tmp_path / "library", create=True)
        library.import_mo2(preview, {profile.id})
        library.customize(profile.id, mod.id, tags="keep", notes="local notes", group="My/Group", enabled=False)
        approved = inspect_instance(root, source_mappings={link.alias: link.target})
        assert approved.linked_sources[0].source == str(target.resolve())
        assert library.import_mo2(approved, {profile.id}) == 0 and library.last_import_repaired == 1
        row = next(row for row in library.rows(profile.id) if row.id == mod.id)
        assert row.source == str(target.resolve()) and row.tags == "keep" and row.notes == "local notes"
        assert row.group == "My/Group" and not row.enabled
        library.customize(profile.id, mod.id, enabled=True)
        staged = library.export(library.export_plan(profile.id))
        assert (staged / "textures" / "linked.txt").read_bytes() == b"synthetic linked payload"
        assert payload.read_bytes() == b"synthetic linked payload"
        assert library.import_mo2(approved, {profile.id}) == 0 and library.last_import_repaired == 0
        assert library.configurations()[0]["configuration"]["source_mappings"][link.alias] == str(target.resolve())
        replacement = tmp_path / "replacement"
        replacement.mkdir()
        (replacement / "other.txt").write_bytes(b"replacement fixture")
        remapped = inspect_instance(root, source_mappings={link.alias: str(replacement)})
        assert library.import_mo2(remapped, {profile.id}) == 0 and library.last_import_repaired == 1
        assert next(row for row in library.rows(profile.id) if row.id == mod.id).source == str(replacement.resolve())
        assert os.path.samefile(alias, target)  # Approval never retargets the real link.
    finally:
        alias.rmdir() if os.name == "nt" else alias.unlink()


def test_missing_link_target_can_be_mapped_to_explicit_ordinary_replacement(tmp_path):
    root = make_mo2(tmp_path)
    target, alias = tmp_path / "missing target", root / "mods" / "Missing link"
    target.mkdir()
    junction(alias, target)
    target.rmdir()
    try:
        preview = inspect_instance(root)
        linked = preview.linked_sources[0]
        assert linked.status == "Missing target" and not linked.can_use_target and not linked.source
        replacement = tmp_path / "replacement"
        replacement.mkdir()
        approved = inspect_instance(root, source_mappings={linked.alias: str(replacement)})
        assert approved.linked_sources[0].source == str(replacement.resolve())
        with pytest.raises(ManagerError, match="Links and Windows reparse"):
            inspect_instance(root, source_mappings={linked.alias: str(alias)})
    finally:
        alias.rmdir() if os.name == "nt" else alias.unlink()


def test_approved_mod_root_does_not_allow_nested_payload_links(tmp_path):
    root = make_mo2(tmp_path)
    target, nested_target = tmp_path / "target", tmp_path / "nested target"
    target.mkdir()
    nested_target.mkdir()
    (nested_target / "escape.txt").write_text("synthetic")
    alias, nested = root / "mods" / "Linked content", target / "nested"
    junction(alias, target)
    junction(nested, nested_target)
    (root / "profiles" / "Default" / "modlist.txt").write_text("+Linked content\n")
    try:
        preview = inspect_instance(root, source_mappings={str(alias): str(target)})
        profile = next(profile for profile in preview.profiles if profile.name == "Default")
        library = Library(tmp_path / "library", create=True)
        library.import_mo2(preview, {profile.id})
        with pytest.raises(ManagerError, match="Links and Windows reparse"):
            library.export_plan(profile.id)
    finally:
        nested.rmdir() if os.name == "nt" else nested.unlink()
        alias.rmdir() if os.name == "nt" else alias.unlink()


def test_unapproved_remote_drive_link_target_is_not_probed(tmp_path, monkeypatch):
    from creation_lib.mod_manager import mo2
    root = make_mo2(tmp_path)
    alias, target = root / "mods/Remote fixture", tmp_path / "target"
    target.mkdir()
    junction(alias, target)
    calls, original = [], mo2._ordinary_source
    def ordinary(path):
        calls.append(str(path))
        return original(path)
    monkeypatch.setattr(mo2, "local_mod_target", lambda path: False)
    monkeypatch.setattr(mo2, "_ordinary_source", ordinary)
    try:
        preview = inspect_instance(root)
        assert not calls and not preview.linked_sources[0].can_use_target
        assert "Network" in preview.linked_sources[0].status
    finally:
        alias.rmdir() if os.name == "nt" else alias.unlink()
