from bacup_lib.worker_advice import (
    detect_system_ram_gb,
    recommend_workers,
)


def test_ample_ram_uses_cpu_half():
    rec = recommend_workers(total_ram_gb=64.0, cpu_count=16)
    assert rec.recommended == 8  # cpu // 2, RAM not the limit
    assert rec.cpu_count == 16


def test_low_ram_caps_below_cpu():
    rec = recommend_workers(total_ram_gb=12.0, cpu_count=16)
    # (12 - 8 base) / 2 per-worker = 2
    assert rec.recommended == 2
    assert "RAM" in rec.note


def test_tiny_ram_floors_at_one():
    rec = recommend_workers(total_ram_gb=6.0, cpu_count=8)
    assert rec.recommended == 1


def test_recommended_never_exceeds_cpu_or_below_one():
    assert recommend_workers(total_ram_gb=256.0, cpu_count=4).recommended == 2
    assert recommend_workers(total_ram_gb=256.0, cpu_count=1).recommended == 1
    assert recommend_workers(total_ram_gb=0.0, cpu_count=0).recommended == 1


def test_detect_system_ram_gb_returns_nonnegative_float():
    ram = detect_system_ram_gb()
    assert isinstance(ram, float)
    assert ram >= 0.0
