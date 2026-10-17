from pathlib import Path

import pytest

from creation_lib.mod_manager.mo2 import read_text
from creation_lib.mod_manager.paths import ManagerError


def test_metadata_growth_between_stat_and_read_is_bounded(monkeypatch, tmp_path):
    path = tmp_path / "fixture.ini"
    path.write_bytes(b"small")
    original = Path.open
    def grow(self, mode="r", *args, **kwargs):
        if self == path and mode == "rb":
            with original(self, "wb") as stream:
                stream.write(b"x" * 100)
        return original(self, mode, *args, **kwargs)
    monkeypatch.setattr(Path, "open", grow)
    with pytest.raises(ManagerError, match="exceeds"):
        read_text(path, limit=10)


@pytest.mark.parametrize("encoding", ["utf-8-sig", "utf-16", "cp1252"])
def test_bounded_metadata_keeps_import_encoding_support(tmp_path, encoding):
    path = tmp_path / "fixture.ini"
    path.write_bytes("[General]\nname=Ästhetik\n".encode(encoding))
    assert read_text(path, limit=128) == "[General]\nname=Ästhetik\n"
