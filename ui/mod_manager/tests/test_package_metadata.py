import os

import pytest

from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import ManagerError
from .test_mo2 import junction


def fixture(tmp_path):
    source = tmp_path / "source"
    directory = source / "payload"
    directory.mkdir(parents=True)
    (directory / "file.txt").write_bytes(b"original fixture")
    target = tmp_path / "unrelated fixture"
    target.mkdir()
    (target / "file.txt").write_bytes(b"unrelated bytes must not be read")
    return source, directory, target


def test_directory_replaced_after_metadata_enumeration_is_rejected_before_publication(tmp_path):
    source, directory, target = fixture(tmp_path)
    replaced = False
    package = Package.__new__(Package)
    class ChangeAfterListing:
        def is_set(self):
            nonlocal replaced
            if getattr(package, "_zip_entries", {}) and not replaced:
                directory.rename(source / "retained original")
                junction(directory, target)
                replaced = True
            return False
    try:
        with pytest.raises(ManagerError, match="reparse"):
            Package.__init__(package, source, cancel=ChangeAfterListing(), inspect_installer=False)
        assert replaced
        assert (target / "file.txt").read_bytes() == b"unrelated bytes must not be read"
    finally:
        if replaced:
            directory.rmdir() if os.name == "nt" else directory.unlink()


def test_payload_open_rechecks_full_ancestry_after_metadata_snapshot(tmp_path):
    source, directory, target = fixture(tmp_path)
    package = Package(source, inspect_installer=False)
    directory.rename(source / "retained original")
    junction(directory, target)
    try:
        with pytest.raises(ManagerError, match="reparse"):
            with package.open("payload/file.txt"):
                pytest.fail("An unsafe source must never be opened.")
        assert (source / "retained original/file.txt").read_bytes() == b"original fixture"
    finally:
        package.close()
        directory.rmdir() if os.name == "nt" else directory.unlink()
