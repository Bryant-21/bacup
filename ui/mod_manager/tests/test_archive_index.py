"""Generated archive/container fixtures; no game archives or payload extraction."""
from contextlib import contextmanager
from pathlib import Path
import struct
import threading
import zlib

import pytest

from creation_lib.mod_manager.archive_index import CachedArchives, build, ini_archives
from creation_lib.mod_manager.cli_reader import run_json
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_standalone import game_fixture


def ba2_fixture(path, names):
    """Small uncompressed GNRL BA2 containing only generated literal text."""
    payload = b"x"
    records_end = 24 + len(names) * 36
    data = struct.pack("<4sI4sIQ", b"BTDX", 1, b"GNRL", len(names), records_end + len(names))
    for index, name in enumerate(names):
        extension = Path(name).suffix.lstrip(".").encode()[:4].ljust(4, b"\0")
        data += struct.pack("<I4sIIQIII", zlib.crc32(name.encode()), extension, zlib.crc32(str(Path(name).parent).encode()), 0x00100100,
                            records_end + index, 0, len(payload), 0xBAADF00D)
    data += payload * len(names)
    for name in names:
        encoded = name.encode("utf-8")
        data += struct.pack("<H", len(encoded)) + encoded
    path.write_bytes(data)
    return path


def test_actual_cli_lists_generated_ba2_without_mutation(tmp_path):
    path = ba2_fixture(tmp_path / "Generated.ba2", ["textures/fixture.txt", "meshes/other.txt"])
    before = path.read_bytes()
    result = run_json(("archive", "list", str(path)))
    assert result["file_count"] == 2 and set(result["files"]) == {"textures/fixture.txt", "meshes/other.txt"}
    assert path.read_bytes() == before


def test_archive_cache_reopen_invalidation_cancel_and_unsafe_members(tmp_path):
    library = Library(tmp_path / "library", create=True)
    path = ba2_fixture(tmp_path / "Generated.ba2", ["texture.txt"])
    calls = []
    def reader(command, **kwargs):
        calls.append(command)
        return {"format": "BA2", "file_count": 1, "files": ["textures/fixture.txt"]}
    cache = CachedArchives(library, reader)
    assert cache(path)["files"] == ["textures/fixture.txt"]
    assert CachedArchives(Library(library.root), reader)(path)["file_count"] == 1 and len(calls) == 1
    path.write_bytes(path.read_bytes() + b"changed fixture")
    cache(path)
    assert len(calls) == 2
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        cache(path, cancel)
    for files in (["../escape"], ["C:/outside"], ["A.txt", "a.txt"]):
        with pytest.raises(ManagerError):
            cache.validate({"file_count": len(files), "files": files})


def test_archive_provenance_shadowing_members_ini_and_plugin_candidates(tmp_path):
    game = game_fixture(tmp_path)
    ba2_fixture(game / "Data" / "Base.ba2", ["textures/shared.txt"])
    (game / "Data" / "Addon.esp").write_bytes(b"generated placeholder")
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game)
    package_root = tmp_path / "package"
    package_root.mkdir()
    ba2_fixture(package_root / "Base.ba2", ["textures/shared.txt", "textures/owned.txt"])
    ba2_fixture(package_root / "Addon - Main.ba2", ["textures/shared.txt"])
    package = Package(package_root)
    try:
        library.install(package, package.plain_plan(), "Owned archive mod", profile_id)
    finally:
        package.close()
    with library._db() as db:
        import json
        files = library.profiles()[0].files
        files["plugins.txt"] = "*Addon.esp\n"
        files["Fallout4.ini"] = "[Archive]\nsResourceArchiveList=Base.ba2\n"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile_id))
    before = {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()}
    index = build(library, profile_id)
    base = [archive for archive in index.archives if archive.name == "Base.ba2"]
    assert len(base) == 2 and sum(archive.selected for archive in base) == 1
    assert next(archive for archive in base if archive.selected).provider.name == "Owned archive mod"
    assert next(archive for archive in base if archive.selected).activation == "listed"
    attached = next(archive for archive in index.archives if archive.name == "Addon - Main.ba2")
    assert attached.activation == "plugin candidate" and "validation" in attached.reason
    common = next(member for member in index.members if member.path == "textures/shared.txt")
    assert len(common.candidates) == 2 and len(common.archives) == 2
    assert any("unverified" in issue for issue in index.issues)
    assert {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()} == before
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        build(library, profile_id, cancel)


def test_ini_archive_custom_override_and_cache_cancel_before_commit(tmp_path, monkeypatch):
    assert ini_archives({"Fallout4.ini": "[Archive]\nsResourceArchiveList=A.ba2,B.ba2\nsResourceArchiveList2=C.ba2\n",
                         "Fallout4Custom.ini": "[Archive]\nsResourceArchiveList=D.ba2\n"}) == ("D.ba2", "C.ba2")
    library = Library(tmp_path / "library", create=True)
    path = ba2_fixture(tmp_path / "Generated.ba2", ["texture.txt"])
    cancel, original = threading.Event(), library._db
    @contextmanager
    def db():
        with original() as connection:
            class Wrapped:
                def execute(self, query, *args):
                    result = connection.execute(query, *args)
                    if query.startswith("INSERT INTO archive_metadata"):
                        cancel.set()
                    return result
            yield Wrapped()
    monkeypatch.setattr(library, "_db", db)
    with pytest.raises(Cancelled):
        CachedArchives(library)(path, cancel)
    with original() as connection:
        assert connection.execute("SELECT count(*) FROM archive_metadata").fetchone()[0] == 0


def test_shared_source_archive_has_exactly_one_selected_provider(tmp_path):
    game = game_fixture(tmp_path)
    source = tmp_path / "shared source"
    source.mkdir()
    ba2_fixture(source / "Base.ba2", ["fixture.txt"])
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game)
    with library._db() as db:
        for priority in range(2):
            identifier = f"fixture-{priority}"
            db.execute("INSERT INTO mods(id,name,source,kind) VALUES (?,?,?,'external')", (identifier, identifier, str(source)))
            db.execute("INSERT INTO entries VALUES (?,?,?,1,'')", (profile, identifier, priority))
    def reader(*args, **kwargs):
        return {"format": "BA2", "file_count": 1, "files": ["fixture.txt"]}
    index = build(library, profile, reader=reader)
    assert len(index.archives) == 2 and sum(archive.selected for archive in index.archives) == 1
    assert next(archive for archive in index.archives if archive.selected).provider.mod_id == "fixture-1"
    assert len(index.members[0].archives) == 1
