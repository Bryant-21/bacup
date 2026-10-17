"""Inert reads only; generated members and disposable archives."""
from contextlib import closing
import threading
import zipfile

from PIL import Image
import pytest

from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.package_preview import read
from creation_lib.mod_manager.paths import Cancelled, ManagerError


@pytest.mark.parametrize("archive", [False, True])
@pytest.mark.parametrize("name,payload,kind", [
    ("readme.txt", b"hello generated fixture", "text"),
    ("utf16.txt", "Generated unicode \u263a".encode("utf-16"), "text"),
    ("binary.bin", b"\x00\xff\x05", "hex"),
    ("payload.html", b"<script>untrusted()</script>", "text"),
    ("empty.txt", b"", "text"),
])
def test_text_and_binary_are_bounded_inert_and_never_mutated(tmp_path, archive, name, payload, kind):
    path = tmp_path / ("fixture.zip" if archive else "folder")
    if archive:
        with zipfile.ZipFile(path, "w") as file:
            file.writestr(name, payload)
        before = path.read_bytes()
    else:
        path.mkdir()
        (path / name).write_bytes(payload)
        before = payload
    with closing(Package(path)) as package:
        result = read(package, name)
        assert result.kind == kind and result.bytes == len(payload)
        assert result.pixels is None and not result.truncated
    assert (path if archive else path / name).read_bytes() == before


def test_wrapped_image_preview_uses_package_relative_member_and_bounded_decode(tmp_path):
    root = tmp_path / "root"
    (root / "wrapper/fomod").mkdir(parents=True)
    (root / "wrapper/fomod/ModuleConfig.xml").write_text("<config><moduleName>Wrapped</moduleName></config>")
    image = root / "wrapper/art.png"
    Image.new("RGB", (1800, 900), (30, 90, 200)).save(image)
    before = image.read_bytes()
    with closing(Package(root)) as package:
        assert package.prefix == "wrapper/"
        result = read(package, "wrapper/art.png")
        assert result.kind == "image" and result.pixels.shape == (512, 1024, 3)
    assert image.read_bytes() == before


def test_long_text_truncates_at_display_limit_without_cutting_utf8_into_hex(tmp_path):
    (tmp_path / "long.txt").write_text("a" * 65535 + "\u263a" + "b" * 100000, encoding="utf-8")
    with closing(Package(tmp_path)) as package:
        result = read(package, "long.txt")
        assert result.kind == "text" and len(result.text) == 32768 and result.truncated


@pytest.mark.parametrize("source", ["../outside.txt", "C:/outside.txt", "missing.txt"])
def test_preview_rejects_missing_or_unsafe_member(tmp_path, source):
    with closing(Package(tmp_path)) as package, pytest.raises(ManagerError):
        read(package, source)


@pytest.mark.parametrize("changed", ["file", "config", "archive", "delete", "cancel", "large", "bad-image"])
def test_changed_cancelled_and_oversized_reads_leave_no_owned_outputs(tmp_path, changed):
    root = tmp_path / "source"
    (root / "fomod").mkdir(parents=True)
    (root / "fomod/ModuleConfig.xml").write_text("<config><moduleName>Fixture</moduleName></config>")
    name = "file.png" if changed == "bad-image" else "file.txt"
    (root / name).write_bytes(b"invalid image" if changed == "bad-image" else b"generated")
    if changed == "large":
        with (root / name).open("wb") as file:
            file.truncate(8 * 1024**2 + 1)
    path = root
    if changed == "archive":
        path = tmp_path / "archive.zip"
        with zipfile.ZipFile(path, "w") as file:
            file.writestr(name, b"generated")
    cancel = threading.Event()
    with closing(Package(path)) as package:
        if changed == "file":
            (root / name).write_bytes(b"generated changed")
        elif changed == "config":
            (root / "fomod/ModuleConfig.xml").write_text("changed metadata")
        elif changed == "delete":
            (root / name).unlink()
        elif changed == "archive":
            path.write_bytes(path.read_bytes() + b"changed archive")
        elif changed == "cancel":
            cancel.set()
        with pytest.raises((ManagerError, Cancelled)):
            read(package, name, cancel)
    assert {p.name for p in tmp_path.iterdir()} <= {"source", "archive.zip"}
