from bacup_lib.regen_pipeline import _write_conversion_reports
from bacup_lib.timing_report import TimingReport


def test_conversion_reports_write_to_diagnostics_root(tmp_path):
    output_root = tmp_path / "SeventySix"
    diagnostics_root = output_root / "logs" / "20260629-120000-pid123" / "logs"
    output_root.mkdir()

    timing = TimingReport()
    timing.record("phase", 0.25)

    _write_conversion_reports(timing, None, diagnostics_root, 1.0)

    assert (diagnostics_root / "conversion_timing.json").is_file()
    assert not (output_root / "conversion_timing.json").exists()
