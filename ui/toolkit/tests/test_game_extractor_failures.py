from types import SimpleNamespace

from ui.toolkit.setup_wizard import _GameExtractor


def _stub_extraction(monkeypatch, tmp_path, *, error):
    import creation_lib.preprocessor.extraction as extraction

    archive = tmp_path / "Data" / "SeventySix - Animations.ba2"
    archive.parent.mkdir(parents=True, exist_ok=True)
    archive.write_bytes(b"")
    saved = []
    monkeypatch.setattr(extraction, "find_archives", lambda *a, **k: [archive])
    monkeypatch.setattr(
        extraction, "group_archives_by_update_phase", lambda archives: [list(archives)]
    )
    monkeypatch.setattr(
        extraction,
        "plan_archive_extraction_batches",
        lambda archives, workers: [
            [SimpleNamespace(archive=a, file_workers=1, file_count=1) for a in archives]
        ],
    )
    monkeypatch.setattr(
        extraction,
        "extract_one",
        lambda a, *args, **kwargs: (a, 0 if error else 5, error),
    )
    monkeypatch.setattr(extraction, "build_manifest", lambda *a, **k: {"game": "fo76"})
    monkeypatch.setattr(
        extraction, "save_manifest", lambda output_dir, manifest: saved.append(output_dir)
    )
    return saved


def test_failed_archive_is_never_certified_as_extracted(monkeypatch, tmp_path):
    saved = _stub_extraction(monkeypatch, tmp_path, error="disk full")

    extractor = _GameExtractor([("fo76", str(tmp_path))], output_root=tmp_path / "out")
    extractor._run()

    # A manifest would mark the half-written tree complete, and a recorded
    # result would set it as the game's extracted dir.
    assert saved == []
    assert extractor.results == {}
    assert "failed to extract" in extractor.error


def test_clean_extraction_is_recorded(monkeypatch, tmp_path):
    saved = _stub_extraction(monkeypatch, tmp_path, error=None)

    extractor = _GameExtractor([("fo76", str(tmp_path))], output_root=tmp_path / "out")
    extractor._run()

    assert saved == [tmp_path / "out" / "fo76"]
    assert extractor.results == {"fo76": str(tmp_path / "out" / "fo76")}
    assert extractor.error == ""
