import importlib.util
import sys
from pathlib import Path


def _load_golden_module():
    path = Path(__file__).resolve().parents[4] / "scripts" / "conversion_golden.py"
    spec = importlib.util.spec_from_file_location("conversion_golden", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules["conversion_golden"] = module
    spec.loader.exec_module(module)
    return module


def test_hash_tree_is_stable_and_excludes_nondeterministic(tmp_path):
    golden = _load_golden_module()
    (tmp_path / "Out.esp").write_bytes(b"\x00\x01\x02")
    (tmp_path / "conversion_timing.json").write_text("{}")  # excluded
    (tmp_path / "asset_provenance.jsonl").write_text("x")    # excluded
    first = golden.hash_tree(tmp_path)
    second = golden.hash_tree(tmp_path)
    assert first == second
    assert "Out.esp" in first
    assert "conversion_timing.json" not in first
    assert "asset_provenance.jsonl" not in first


def test_diff_trees_reports_mismatch(tmp_path):
    golden = _load_golden_module()
    a = tmp_path / "a"
    b = tmp_path / "b"
    a.mkdir()
    b.mkdir()
    (a / "Out.esp").write_bytes(b"\x00")
    (b / "Out.esp").write_bytes(b"\x01")
    diff = golden.diff_trees(golden.hash_tree(a), golden.hash_tree(b))
    assert diff == ["Out.esp: hash mismatch"]


def test_hash_tree_excludes_report_artifacts(tmp_path):
    golden = _load_golden_module()
    (tmp_path / "Out.esp").write_bytes(b"\xde\xad\xbe\xef")
    (tmp_path / "conversion_memory.md").write_text("elapsed: 123.456s\npeak RSS: 99.9 GB\n")
    nested = tmp_path / "SeventySix"
    nested.mkdir()
    (nested / "script_port_report.json").write_text('{"scripts": []}')
    tree = golden.hash_tree(tmp_path)
    assert "Out.esp" in tree
    assert not any("conversion_memory.md" in k for k in tree)
    assert not any("script_port_report.json" in k for k in tree)


def test_manifest_only_capture_skips_tree(tmp_path):
    import argparse
    golden = _load_golden_module()
    out = tmp_path / "output"
    out.mkdir()
    (out / "Out.esp").write_bytes(b"\x01\x02\x03")
    gold = tmp_path / "golden"

    # capture with --manifest-only
    golden.main(["capture", "--manifest-only", "--output-dir", str(out), "--golden-dir", str(gold)])

    assert (gold / "_manifest.json").is_file()
    assert not (gold / "tree").exists()

    # compare should pass (identical)
    rc = golden.main(["compare", "--output-dir", str(out), "--golden-dir", str(gold)])
    assert rc == 0

    # mutate the output and compare should fail
    (out / "Out.esp").write_bytes(b"\xff\xfe\xfd")
    rc = golden.main(["compare", "--output-dir", str(out), "--golden-dir", str(gold)])
    assert rc == 1
