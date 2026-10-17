"""Generated FO4 metadata blocks only; fixtures are not playable saves."""
from dataclasses import replace
from pathlib import Path
import struct
import threading

import pytest

from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.plugin_state import Catalog, Plugin
from creation_lib.mod_manager.save_inspection import compare_profile, compare_saved, inspect, validate
from creation_lib.mod_manager.saves import Save
from .test_saves import save_fixture


def fixture(path, regular=("Fallout4.esm", "Fixture.esp"), light=("Small.esl",), *, version=15, form_version=68, screenshot=None):
    save_fixture(path)
    original = path.read_bytes()
    header = bytearray(original[16:16 + struct.unpack_from("<I", original, 12)[0]])
    struct.pack_into("<I", header, 0, version)
    width, height, pixels = screenshot if screenshot is not None else (2, 1, b"\0" * 8)
    header.extend(struct.pack("<II", width, height))
    def text(value):
        encoded = value.encode("utf-8")
        return struct.pack("<H", len(encoded)) + encoded
    table = bytes([len(regular)]) + b"".join(text(name) for name in regular)
    if version == 15 and form_version >= 68:
        table += struct.pack("<H", len(light)) + b"".join(text(name) for name in light)
    path.write_bytes(b"FO4_SAVEGAME" + struct.pack("<I", len(header)) + header + pixels + bytes([form_version])
                     + text("1.10.163.0") + struct.pack("<I", len(table)) + table + b"inert generated body")
    return save(path)


def save(path):
    info = path.stat()
    return Save(path.name, str(path), info.st_size, info.st_mtime_ns)


@pytest.mark.parametrize("version,form,light", [(11, 60, ()), (12, 61, ()), (13, 67, ()), (15, 67, ()), (15, 68, ("Small.esl",)), (15, 100, ("Small.esl",))])
def test_generated_versions_and_light_tables_read_only(tmp_path, version, form, light):
    source = fixture(tmp_path / "generated.fos", version=version, form_version=form, light=light)
    before = Path(source.path).read_bytes()
    result = inspect(source)
    assert result.regular == ("Fallout4.esm", "Fixture.esp") and result.light == light
    assert (result.width, result.height) == (2, 1) and result.basic["player"] == "Fixture Character"
    assert result.game_version == "1.10.163.0" and result.form_version == form
    assert "body" in result.basic["coverage"] and Path(source.path).read_bytes() == before


@pytest.mark.parametrize("fault", ["version", "combination", "image", "table", "truncated-image", "truncated-table", "trailing-table", "duplicate", "unsafe-name", "light-count"])
def test_corrupt_or_unsupported_metadata_fails_without_writes(tmp_path, fault):
    path = tmp_path / "generated.fos"
    fixture(path)
    data = bytearray(path.read_bytes())
    header_end = 16 + struct.unpack_from("<I", data, 12)[0]
    table_size_at = header_end + 8 + 1 + 2 + len("1.10.163.0")
    if fault == "version":
        struct.pack_into("<I", data, 16, 16)
    elif fault == "combination":
        struct.pack_into("<I", data, 16, 13)
    elif fault == "image":
        struct.pack_into("<II", data, header_end - 8, 8192, 8192)
    elif fault == "table":
        struct.pack_into("<I", data, table_size_at, 8 * 1024**2 + 1)
    elif fault == "truncated-image":
        data = data[:header_end + 7]
    elif fault == "truncated-table":
        data = data[:table_size_at + 6]
    elif fault == "trailing-table":
        struct.pack_into("<I", data, table_size_at, struct.unpack_from("<I", data, table_size_at)[0] + 1)
    elif fault == "duplicate":
        fixture(path, regular=("Fixture.esp", "fixture.ESP"))
        data = path.read_bytes()
    elif fault == "unsafe-name":
        fixture(path, regular=("../escape.esp",))
        data = path.read_bytes()
    elif fault == "light-count":
        fixture(path, regular=())
        data = bytearray(path.read_bytes())
        struct.pack_into("<H", data, table_size_at + 5, 4097)
    path.write_bytes(data)
    with pytest.raises(ManagerError):
        inspect(save(path))
    assert path.read_bytes() == data


def test_cancel_stale_file_and_basic_only_fixture(tmp_path):
    path = tmp_path / "generated.fos"
    source = fixture(path)
    event = threading.Event()
    event.set()
    with pytest.raises(Cancelled):
        inspect(source, event)
    result = inspect(source)
    path.write_bytes(path.read_bytes() + b"changed")
    with pytest.raises(ManagerError, match="changed"):
        validate(result)
    with pytest.raises(ManagerError, match="changed"):
        inspect(source)
    save_fixture(path)
    with pytest.raises(ManagerError):
        inspect(save(path))


def test_comparison_two_saves_indices_types_case_and_add_remove(tmp_path):
    left = inspect(fixture(tmp_path / "left.fos", regular=("Base.esm", "A.esp", "Move.esp"), light=("Small.esl",)))
    right = inspect(fixture(tmp_path / "right.fos", regular=("base.ESM", "New.esp", "Small.esl"), light=("Move.esp",)))
    result = compare_saved(left, right)
    assert result.missing == ("A.esp",) and result.added == ("New.esp",)
    assert result.type_changed == ("Move.esp", "Small.esl")
    assert result.moved == (("Move.esp", "02", "FE:000"), ("Small.esl", "FE:000", "02"))
    assert not result.same_plugins
    assert compare_saved(left, replace(left, regular=tuple(name.upper() for name in left.regular))).same_plugins


def plugin(name, *, enabled=True, path="generated", light=False, priority=0, known=True):
    return Plugin(name, path, "Fixture", "fixture", enabled, priority, header={"flags": {"esl": light}} if known else None)


def test_profile_comparison_missing_disabled_added_unknown_and_order(tmp_path):
    source = inspect(fixture(tmp_path / "generated.fos", regular=("Base.esm", "Disabled.esp", "Missing.esp"), light=("Small.esl",)))
    catalog = Catalog("profile", "signature", "fo4", (plugin("Base.esm"), plugin("Disabled.esp", enabled=False),
                     plugin("Missing.esp", path=""), plugin("New.esp", priority=1), plugin("Small.esl", light=True, priority=2)))
    result = compare_profile(source, catalog)
    assert result.missing == ("Missing.esp",) and result.disabled == ("Disabled.esp",) and result.added == ("New.esp",)
    assert not result.same_plugins
    catalog = replace(catalog, plugins=(*catalog.plugins, plugin("Unknown.esp", known=False)))
    assert compare_profile(source, catalog).unknown == ("Unknown.esp",)
    assert compare_profile(source, catalog).moved == ()
    with pytest.raises(ManagerError, match="Fallout 4"):
        compare_profile(source, replace(catalog, game="skyrimse"))


def test_selected_source_inventory_identity_blocks_same_size_timestamp_replacement(tmp_path):
    import os
    from creation_lib.mod_manager.instances import create
    from creation_lib.mod_manager.library import Library
    from creation_lib.mod_manager.saves import inventory_folder
    from .test_standalone import game_fixture
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    root = tmp_path / "selected source saves"
    root.mkdir()
    source = fixture(root / "generated.fos")
    indexed = inventory_folder(library, profile, root)
    assert indexed.external and indexed.files[0].identity is not None
    original = Path(source.path)
    before = original.read_bytes()
    original.rename(root / "moved.fos")
    original.write_bytes(before)
    os.utime(original, ns=(source.modified, source.modified))
    with pytest.raises(ManagerError, match="changed after"):
        inspect(indexed.files[0])
    assert original.read_bytes() == before and (root / "moved.fos").read_bytes() == before


def test_mutation_during_plugin_read_is_rejected_and_stream_closed(tmp_path, monkeypatch):
    source = fixture(tmp_path / "generated.fos")
    path, opened, changed = Path(source.path), [], []
    before, original_open = path.read_bytes(), Path.open
    threshold = 16 + struct.unpack_from("<I", before, 12)[0] + 8
    class ChangingRead:
        def __init__(self, stream):
            self.stream = stream
        def __getattr__(self, name):
            return getattr(self.stream, name)
        def __enter__(self):
            self.stream.__enter__()
            return self
        def __exit__(self, *args):
            return self.stream.__exit__(*args)
        def read(self, size):
            value = self.stream.read(size)
            if self.stream.tell() > threshold and not changed:
                changed.append(True)
                with original_open(path, "ab") as target:
                    target.write(b"generated concurrent change")
            return value
    def patched(target, *args, **kwargs):
        stream = original_open(target, *args, **kwargs)
        if target == path and args and args[0] == "rb":
            opened.append(stream)
            return ChangingRead(stream)
        return stream
    monkeypatch.setattr(Path, "open", patched)
    with pytest.raises(ManagerError, match="changed during"):
        inspect(source)
    assert changed and all(stream.closed for stream in opened)
