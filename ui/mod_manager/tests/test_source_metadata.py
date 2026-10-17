import threading

import pytest

from creation_lib.mod_manager import source_metadata as sync
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2
from .test_reference_sources import files

METADATA = '[General]\nversion=3.4\nnotes="Source note\\nsecond line"\ntags=visual,fixture\ncategory=1,2\ngameName=Fallout4\nmodid=987\n[installedFiles]\nsize=1\n1\\modid=987\n1\\fileid=654\n'


@pytest.fixture
def target(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    profile = library.profiles()[0].id
    row = next(row for row in library.rows(profile) if row.name == "Base")
    with library._db() as db:
        db.execute("UPDATE mods SET name='My alias',notes='My notes',tags='local tag' WHERE id=?", (row.id,))
        db.execute("INSERT INTO categories VALUES ('Visual/Textures',0)")
    metadata = source / "mods" / "Base" / "meta.ini"
    metadata.write_text(METADATA, encoding="utf-8")
    mapping = tmp_path / "categories.dat"
    mapping.write_text("1|visual/Textures|0|0\n2|Gameplay/Fixture|0|0\n", encoding="utf-8")
    return library, profile, row.id, source, metadata, mapping


def plan(target, *, mapped=True):
    library, profile, mod, _, _, mapping = target
    return sync.review(library, profile, mod, str(mapping) if mapped else "")


def test_selective_refresh_repeat_import_reopen_undo_preserves_customization_and_payloads(target):
    library, profile, mod, source, _, _ = target
    before, payloads = library.rows(profile), files(source)
    reviewed = plan(target)
    fields = {item.name: item for item in reviewed.fields}
    assert reviewed.profiles == 2 and not any(item.issue for item in reviewed.fields)
    assert fields["notes"].value == "Source note\nsecond line"
    assert fields["nexus"].value == ("fallout4", 987, 654)
    assert fields["categories"].value == ("Visual/Textures", "Gameplay/Fixture")
    sync.apply(library, reviewed, ("version", "categories", "nexus"))
    library.import_mo2(inspect_instance(source), reconcile=True)
    reopened = Library(library.root)
    for p in reopened.profiles():
        row = next(item for item in reopened.rows(p.id) if item.id == mod)
        assert (row.name, row.notes, row.tags) == ("My alias", "My notes", "local tag")
        assert row.version == "3.4" and row.categories == fields["categories"].value
        assert (row.nexus_game, row.nexus_mod_id, row.nexus_file_id) == fields["nexus"].value
    assert sync.undoable(reopened, profile) == {mod}
    sync.undo(reopened, profile, mod)
    assert reopened.rows(profile) == before and not sync.undoable(reopened, profile)
    assert files(source) == payloads


def test_explicit_empty_and_missing_fields_and_unresolved_categories(target):
    library, profile, mod, _, metadata, _ = target
    without_map = {item.name: item for item in plan(target, mapped=False).fields}
    assert without_map["categories"].issue and not without_map["version"].issue
    with pytest.raises(ManagerError, match="unavailable"):
        sync.apply(library, plan(target, mapped=False), ("categories",))
    metadata.write_text("[General]\nnotes=\ncategory=0\nmodid=-1\n", encoding="utf-8")
    reviewed = plan(target, mapped=False)
    fields = {item.name: item for item in reviewed.fields}
    assert fields["version"].issue and fields["tags"].issue
    sync.apply(library, reviewed, ("notes", "categories", "nexus"))
    row = next(item for item in library.rows(profile) if item.id == mod)
    assert row.notes == "" and row.version == "1.2" and row.tags == "local tag"
    assert row.categories == () and row.nexus_mod_id == row.nexus_file_id == 0


@pytest.mark.parametrize("array", ["size=2\n1\\modid=987\n1\\fileid=11\n2\\modid=987\n2\\fileid=22\n", "size=1\n1\\modid=123\n1\\fileid=11\n"])
def test_ambiguous_installed_file_identity_is_cleared_explicitly(target, array):
    library, profile, mod, _, metadata, _ = target
    metadata.write_text("[General]\ngameName=Fallout4\nmodid=987\nfileid=999\n[installedFiles]\n" + array, encoding="utf-8")
    reviewed = plan(target)
    nexus = next(item for item in reviewed.fields if item.name == "nexus")
    assert not nexus.issue and nexus.warning and nexus.value == ("fallout4", 987, 0)
    sync.apply(library, reviewed, ("nexus",))
    assert next(item for item in library.rows(profile) if item.id == mod).nexus_file_id == 0


@pytest.mark.parametrize("text,field", [("version=" + "x" * 201, "version"), ("notes=@Variant(binary)", "notes"), ("tags=bad\\ncontrol", "tags"), ("modid=99999999999", "nexus"), ("modid=10\ngameName=Unknown game", "nexus"), ("modid=10\ngameName=Fallout4\nrepository=Other", "nexus"), ("modid=10\n[installedFiles]\nsize=129", "nexus"), ("modid=10\n[installedFiles]\nsize=0\n1\\modid=10\n1\\fileid=12", "nexus"), ("category=99", "categories")])
def test_invalid_source_field_is_unselectable_and_never_overwrites_local_metadata(target, text, field):
    library, profile, _, source, metadata, _ = target
    metadata.write_text("[General]\n" + text + "\n", encoding="utf-8")
    before, payloads = library.rows(profile), files(source)
    reviewed = plan(target)
    assert next(item for item in reviewed.fields if item.name == field).issue
    with pytest.raises(ManagerError, match="unavailable"):
        sync.apply(library, reviewed, (field,))
    assert library.rows(profile) == before and files(source) == payloads


@pytest.mark.parametrize("change", ["metadata", "mapping", "local", "membership", "taxonomy", "source"])
def test_changed_inputs_or_local_context_reject_stale_refresh(target, change):
    library, profile, mod, source, metadata, mapping = target
    reviewed = plan(target)
    if change in {"metadata", "mapping"}:
        path = metadata if change == "metadata" else mapping
        path.write_text(path.read_text("utf-8") + "\n; later fixture\n", encoding="utf-8")
    else:
        with library._db() as db:
            if change == "membership":
                db.execute("UPDATE entries SET enabled=0 WHERE mod_id=?", (mod,))
            elif change == "taxonomy":
                db.execute("UPDATE categories SET nexus_id=1")
            else:
                db.execute(f"UPDATE mods SET {'source' if change == 'source' else 'notes'}='later' WHERE id=?", (mod,))
    before, payloads = library.rows(profile), files(source)
    with pytest.raises(ManagerError, match="changed"):
        sync.apply(library, reviewed, ("version",))
    assert library.rows(profile) == before and files(source) == payloads


@pytest.mark.parametrize("phase", ["apply", "undo"])
@pytest.mark.parametrize("outcome", ["cancelled", "failed", "source-change"])
def test_cancel_failure_and_commit_boundary_input_change_roll_back(target, monkeypatch, phase, outcome):
    library, profile, mod, source, metadata, _ = target
    reviewed = plan(target)
    if phase == "undo":
        sync.apply(library, reviewed, ("version", "notes"))
    before, cancel, payloads = library.rows(profile), threading.Event(), files(source)
    def stop():
        if outcome == "failed":
            raise OSError("generated metadata commit failure")
        if outcome == "cancelled":
            cancel.set()
        else:
            metadata.write_text("[General]\nversion=later\n", encoding="utf-8")
    monkeypatch.setattr(sync, "_checkpoint", stop)
    if phase == "undo" and outcome == "source-change":
        # Undo is local metadata and deliberately does not read external files.
        sync.undo(library, profile, mod)
        assert next(item for item in library.rows(profile) if item.id == mod).version == "1.2"
    else:
        with pytest.raises(Cancelled if outcome == "cancelled" else OSError if outcome == "failed" else ManagerError):
            if phase == "apply":
                sync.apply(library, reviewed, ("version",), cancel)
            else:
                sync.undo(library, profile, mod, cancel)
        assert library.rows(profile) == before
    # Only the test fault injector may change its generated metadata fixture.
    current = files(source)
    assert current.keys() == payloads.keys()
    assert all(name == "mods\\Base\\meta.ini" or content == current[name] for name, content in payloads.items())
    if outcome != "source-change":
        assert current == payloads


def test_guarded_multi_step_undo_and_later_local_edits(target):
    library, profile, mod, _, _, _ = target
    before = library.rows(profile)
    sync.apply(library, plan(target), ("version",))
    sync.apply(library, plan(target), ("notes", "tags"))
    sync.undo(library, profile, mod)
    sync.undo(library, profile, mod)
    assert library.rows(profile) == before
    sync.apply(library, plan(target), ("version",))
    with library._db() as db:
        db.execute("UPDATE mods SET notes='later' WHERE id=?", (mod,))
    with pytest.raises(ManagerError, match="Undo is blocked"):
        sync.undo(library, profile, mod)


def test_malformed_duplicate_large_linked_and_missing_inputs_are_rejected(target, tmp_path, monkeypatch):
    from .test_mo2 import junction
    library, profile, mod, _, metadata, _ = target
    for text in ("[General]\nversion=a\nversion=b\n", "invalid metadata", "[General]\nversion=" + "x" * (sync.LIMIT + 1)):
        metadata.write_text(text, encoding="utf-8")
        with pytest.raises(ManagerError):
            plan(target)
    metadata.unlink()
    with pytest.raises(OSError):
        plan(target)
    metadata.write_text(METADATA, encoding="utf-8")
    link = tmp_path / "metadata-link"
    junction(link, metadata.parent)
    with pytest.raises(ManagerError, match="reparse|Links"):
        sync.review(library, profile, mod, str(link / "categories.dat"))
    metadata.write_text(METADATA, encoding="utf-8")
    with pytest.raises(ManagerError, match="Choose reviewed"):
        sync.apply(library, plan(target), ())
    with library._db() as db:
        db.execute("UPDATE mods SET kind='owned' WHERE id=?", (mod,))
    with pytest.raises(ManagerError, match="referenced"):
        plan(target)
    with library._db() as db:
        db.execute("UPDATE mods SET kind='external' WHERE id=?", (mod,))
    reviewed = plan(target)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda _: [("fixture", {"state": "running"})])
    with pytest.raises(ManagerError, match="runtime"):
        sync.apply(library, reviewed, ("version",))


@pytest.mark.parametrize("encoding", ["utf-8-sig", "utf-16", "cp1252"])
def test_supported_encodings_and_qsettings_scalars(target, encoding):
    _, _, _, _, metadata, _ = target
    metadata.write_bytes('[General]\nversion=2.0\nnotes="café\\tline"\ntags=@@literal\n'.encode(encoding))
    fields = {item.name: item for item in plan(target, mapped=False).fields}
    assert fields["notes"].value == "café\tline" and fields["tags"].value == "@literal"
    assert not fields["notes"].issue
