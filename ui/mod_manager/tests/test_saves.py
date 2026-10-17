"""Generated basic save headers and owned inventory; no live/global saves."""
import struct
import threading
from pathlib import Path

import pytest

from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.saves import fo4_header, inventory
from .test_standalone import game_fixture


def save_fixture(path, player="Fixture Character"):
    def text(value):
        encoded = value.encode("utf-8")
        return struct.pack("<H", len(encoded)) + encoded
    fields = struct.pack("<II", 11, 7) + text(player) + struct.pack("<I", 42) + text("Fixture Settlement") + text("01.02.03") + text("HumanRace")
    fields += struct.pack("<HffQ", 0, 1., 2., 133000000000000000)
    path.write_bytes(b"FO4_SAVEGAME" + struct.pack("<I", len(fields)) + fields + b"generated inert body")
    return path


def test_owned_inventory_companion_and_basic_fo4_header_preserve_source_bytes(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    root = library.root / "runtime" / "profiles" / profile / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    (root / "Fixture.f4se").write_bytes(b"generated extender companion")
    (root / "ignored.tmp").write_bytes(b"not a save")
    before = path.read_bytes()
    index = inventory(library, profile)
    assert len(index.files) == 1 and index.files[0].companion.endswith("Fixture.f4se")
    data = fo4_header(index.files[0])
    assert data["player"] == "Fixture Character" and data["level"] == 42 and data["number"] == 7
    assert data["location"] == "Fixture Settlement" and data["header_version"] == 11
    assert path.read_bytes() == before and "unverified" in data["coverage"]
    path.write_bytes(before + b"changed")
    with pytest.raises(ManagerError, match="changed after"):
        fo4_header(index.files[0])


def test_save_inventory_cancel_and_bad_headers_fail_explicitly(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    root = library.root / "runtime" / "profiles" / profile / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        inventory(library, profile, cancel)
    save = inventory(library, profile).files[0]
    with pytest.raises(Cancelled):
        fo4_header(save, cancel)
    for value in (b"bad signature", b"FO4_SAVEGAME", b"FO4_SAVEGAME" + struct.pack("<IIIH", 100, 11, 7, 5000)):
        path.write_bytes(value)
        with pytest.raises(ManagerError):
            fo4_header(inventory(library, profile).files[0])


@pytest.mark.parametrize("fault, message", [
    ("oversized-text", "4 KiB field limit"),
    ("oversized-header", "unsupported basic header size"),
    ("undersized-header", "beyond the declared basic header"),
    ("invalid-utf8", "invalid UTF-8"),
    ("control-text", "control characters"),
    ("invalid-filetime", "invalid creation timestamp"),
])
def test_malformed_basic_header_is_rejected_without_changing_save(tmp_path, fault, message):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    root = library.root / "runtime" / "profiles" / profile / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    data = bytearray(path.read_bytes())
    # FO4 basic fields follow the 12-byte signature and uint32 header length.
    if fault == "oversized-text":
        struct.pack_into("<H", data, 24, 4097)
    elif fault == "oversized-header":
        struct.pack_into("<I", data, 12, 65537)
    elif fault == "undersized-header":
        struct.pack_into("<I", data, 12, 8)
    elif fault == "invalid-utf8":
        data[26] = 0xff
    elif fault == "control-text":
        data[26] = 1
    elif fault == "invalid-filetime":
        header_end = 16 + struct.unpack_from("<I", data, 12)[0]
        struct.pack_into("<Q", data, header_end - 8, 2**64 - 1)
    path.write_bytes(data)
    save = inventory(library, profile).files[0]
    with pytest.raises(ManagerError, match=message):
        fo4_header(save)
    assert path.read_bytes() == data


def test_cancellation_during_header_read_returns_no_details_and_preserves_save(tmp_path, monkeypatch):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    root = library.root / "runtime" / "profiles" / profile / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    original = path.read_bytes()
    save, cancel, original_open = inventory(library, profile).files[0], threading.Event(), Path.open
    streams = []

    class CancelRead:
        def __init__(self, stream):
            self.stream = stream

        def __enter__(self):
            self.stream.__enter__()
            return self

        def __exit__(self, *args):
            return self.stream.__exit__(*args)

        def tell(self):
            return self.stream.tell()

        def read(self, size):
            data = self.stream.read(size)
            if self.stream.tell() >= 20:
                cancel.set()
            return data

    def opened(target, *args, **kwargs):
        stream = original_open(target, *args, **kwargs)
        if target == path and args and args[0] == "rb":
            streams.append(stream)
            return CancelRead(stream)
        return stream

    with monkeypatch.context() as patch:
        patch.setattr(Path, "open", opened)
        with pytest.raises(Cancelled):
            fo4_header(save, cancel)
    assert streams and all(stream.closed for stream in streams)
    assert path.read_bytes() == original
