from pathlib import Path
import struct
import subprocess
import threading
import zlib

import pytest

from creation_lib.mod_manager.fomod import Fomod
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Limits, Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.sevenzip import executable, parse_listing, process
from .fixtures import make_package


def tool():
    try:
        return executable()
    except ManagerError:
        pytest.skip("7-Zip/NanaZip is not installed")


def rar_fixture(path, files):
    """Synthetic stored RAR4 headers; no third-party payload or compressor."""
    def header(data):
        return struct.pack("<H", zlib.crc32(data) & 0xffff) + data
    data = b"Rar!\x1a\x07\x00" + header(struct.pack("<BHHHI", 0x73, 0, 13, 0, 0))
    for name, content in files.items():
        name = name.encode("ascii")
        block = struct.pack("<BHHIIBIIBBHI", 0x74, 0x8000, 32 + len(name), len(content), len(content),
                            2, zlib.crc32(content), 0, 20, 0x30, len(name), 0x20) + name
        data += header(block) + content
    data += header(struct.pack("<BHH", 0x7b, 0, 7))
    path.write_bytes(data)
    return path


def seven_fixture(tmp_path, encrypted=False):
    source = make_package(tmp_path, zipped=False)
    (source / "@literal.txt").write_text("literal filename")
    archive = tmp_path / "Fixture.7z"
    args = [tool(), "a", "-t7z", "-bd", "-bsp0", str(archive), "."]
    if encrypted:
        args += ["-pfixture-password", "-mhe=on"]
    result = subprocess.run(args, cwd=source, capture_output=True, timeout=20)
    assert result.returncode == 0, result.stderr
    return archive


def test_real_solid_7z_fomod_install_and_literal_listfile_name(tmp_path):
    archive = seven_fixture(tmp_path)
    before = archive.read_bytes()
    package = Package(archive)
    try:
        with package.open("@literal.txt") as stream:
            assert stream.read() == b"literal filename"
        plan = Fomod(package.read_config()).plan(package)
        library = Library(tmp_path / "library", create=True)
        profile = library.create_profile("Main")
        library.install(package, plan, "Seven", profile)
        assert (Path(library.rows(profile)[0].source) / "textures/shared.txt").read_bytes() == b"standard"
    finally:
        package.close()
    assert archive.read_bytes() == before


def test_real_rar_stored_fomod_install(tmp_path):
    tool()
    folder = make_package(tmp_path, zipped=False)
    files = {file.relative_to(folder).as_posix(): file.read_bytes() for file in folder.rglob("*") if file.is_file()}
    archive = rar_fixture(tmp_path / "Fixture.rar", files)
    package = Package(archive)
    try:
        plan = Fomod(package.read_config()).plan(package)
        library = Library(tmp_path / "library", create=True)
        profile = library.create_profile("Main")
        library.install(package, plan, "RAR", profile)
        assert (Path(library.rows(profile)[0].source) / "textures/shared.txt").read_bytes() == b"standard"
    finally:
        package.close()


@pytest.mark.parametrize("name", ["../outside.txt", "C:/outside.txt", "file.txt:stream", "NUL.txt", "folder/../outside"])
def test_real_rar_unsafe_members_rejected_before_payload_read(tmp_path, name):
    tool()
    outside = tmp_path / "outside.txt"
    outside.write_text("untouched")
    archive = rar_fixture(tmp_path / "unsafe.rar", {name: b"malicious fixture"})
    with pytest.raises(ManagerError):
        Package(archive)
    assert outside.read_text() == "untouched"


def test_encrypted_and_corrupt_native_archives_fail_closed(tmp_path):
    archive = seven_fixture(tmp_path, encrypted=True)
    with pytest.raises(ManagerError, match="Archive reader failed"):
        Package(archive)
    bad = tmp_path / "bad.rar"
    bad.write_bytes(b"Rar!\x1a\x07\x00" + b"corrupt fixture")
    with pytest.raises(ManagerError):
        Package(bad)


def listing(properties):
    return ("Path = fixture\nType = 7z\n----------\n" + properties + "\n\n").encode()


@pytest.mark.parametrize("metadata", [
    "Path = a\nSize = 1\nSymbolic Link = outside",
    "Path = a\nSize = 1\nHard Link = outside",
    "Path = a\nSize = 1\nAttributes = lrwxrwxrwx",
    "Path = a\nSize = 1\nAttributes = prw-rw-rw-",
    "Path = a\nSize = 1\nAlternate Stream = +",
    "Path = a\nSize = 1\nAnti = +",
    "Path = a\nSize = 1\nEncrypted = +",
    "Path = a\nSize = 1\nPath = other",
    "Path = a\nnot a metadata line\nSize = 1",
    "Path = a\nSize = -1",
    "Path = a\nSize = 99999999999",
    "Path = a\nSize = 200000\nPacked Size = 1",
    "Path = a\nSize = 1\n\nPath = A\nSize = 1",
])
def test_native_metadata_rejects_links_collision_streams_and_limits(metadata):
    with pytest.raises(ManagerError):
        parse_listing(listing(metadata), Limits(), 100000)


def test_native_limits_cancellation_and_archive_change(tmp_path):
    archive = seven_fixture(tmp_path)
    with pytest.raises(ManagerError, match="count"):
        Package(archive, limits=Limits(files=2))
    package = Package(archive)
    cancel = threading.Event()
    cancel.set()
    package._cancel = cancel
    with pytest.raises(Cancelled):
        with package.open("base.txt"):
            pass
    package._cancel = None
    archive.write_bytes(archive.read_bytes() + b"changed")
    with pytest.raises(ManagerError, match="changed"):
        with package.open("base.txt"):
            pass
    package.close()


def test_native_watchdog_cancels_a_stalled_reader():
    import sys
    cancel = threading.Event()
    timer = threading.Timer(0.1, cancel.set)
    timer.start()
    try:
        with pytest.raises(Cancelled):
            with process([sys.executable, "-c", "import time; time.sleep(10)"], cancel) as stream:
                stream.read()
    finally:
        timer.cancel()


def test_corrupt_rar_payload_rolls_back_owned_install(tmp_path):
    tool()
    archive = rar_fixture(tmp_path / "corrupt.rar", {"file.txt": b"payload"})
    archive.write_bytes(archive.read_bytes().replace(b"payload", b"changed"))
    package = Package(archive)
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Main")
    try:
        with pytest.raises(ManagerError, match="Archive reader failed"):
            library.install(package, package.plain_plan(), "Corrupt", profile)
    finally:
        package.close()
    assert library.rows(profile) == [] and not any((library.root / "mods").iterdir())
    assert not any((library.root / ".staging").iterdir())
