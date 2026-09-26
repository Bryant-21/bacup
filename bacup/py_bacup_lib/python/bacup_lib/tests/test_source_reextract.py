from pathlib import Path

from bacup_lib import source_reextract


def _fake_archives(monkeypatch, data_dir: Path, archives: dict[str, dict[str, bytes | None]]):
    paths = []
    for name in archives:
        path = data_dir / name
        path.write_bytes(b"BTDX")
        paths.append(path)
    monkeypatch.setattr(source_reextract, "find_archives", lambda _dir, _fmt: paths)
    monkeypatch.setattr(
        source_reextract.native_runtime, "list_archive", lambda archive: list(archives[Path(archive).name])
    )
    monkeypatch.setattr(
        source_reextract.native_runtime,
        "extract_one",
        lambda archive, member: archives[Path(archive).name][member],
    )


def test_restores_only_missing_ui_sources_and_later_archives_win(tmp_path, monkeypatch):
    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    extracted = tmp_path / "extracted"
    (extracted / "interface").mkdir(parents=True)
    (extracted / "interface/kept.swf").write_bytes(b"user copy")
    _fake_archives(monkeypatch, data_dir, {
        "SeventySix - Interface.ba2": {
            "interface/kept.swf": b"archive copy",
            "interface/missing.swf": b"old",
            "interface/translate_en.txt": b"table",
            "meshes/rock.nif": b"not ui",
        },
        "SeventySix - 00UpdateMain.ba2": {"interface/missing.swf": b"new"},
        "SeventySix - Textures01.ba2": {"interface/icon.swf": b"never read"},
    })

    result = source_reextract.restore_missing_ui_sources(extracted, [data_dir])

    assert sorted(result.restored) == ["interface/missing.swf", "interface/translate_en.txt"]
    assert result.failed == {}
    assert (extracted / "interface/missing.swf").read_bytes() == b"new"
    assert (extracted / "interface/kept.swf").read_bytes() == b"user copy"
    assert not (extracted / "meshes").exists()
    assert not (extracted / "interface/icon.swf").exists()


def test_reports_files_the_archive_cannot_return(tmp_path, monkeypatch):
    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    _fake_archives(monkeypatch, data_dir, {
        "SeventySix - Interface.ba2": {"interface/broken.swf": None, "interface/ok.swf": b"ok"},
    })

    result = source_reextract.restore_missing_ui_sources(tmp_path / "extracted", [data_dir])

    assert result.restored == ["interface/ok.swf"]
    assert list(result.failed) == ["interface/broken.swf"]
