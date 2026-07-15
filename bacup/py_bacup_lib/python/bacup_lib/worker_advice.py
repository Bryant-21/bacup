"""RAM-aware worker-count recommendation for the conversion pipeline.

The pipeline's default worker count is CPU/2, but full regen is memory-heavy
(~8 GB resident base plus per-worker headroom).
On low-RAM machines CPU/2 can OOM, so this caps the recommendation by RAM.

Env-free: no os.environ reads. The RAM probe uses psutil (a system probe), and
degrades to 0.0 (treated as "unknown") if psutil is unavailable.
"""
from __future__ import annotations

from dataclasses import dataclass

# Resident pipeline base before any asset workers, and rough per-worker headroom.
_BASE_RESERVE_GB = 8.0
_PER_WORKER_GB = 2.0


@dataclass(frozen=True)
class WorkerRecommendation:
    recommended: int
    total_ram_gb: float
    cpu_count: int
    note: str


def recommend_workers(total_ram_gb: float, cpu_count: int) -> WorkerRecommendation:
    cpu = max(1, int(cpu_count))
    cpu_default = max(1, cpu // 2)
    ram_gb = max(0.0, float(total_ram_gb))

    if ram_gb <= 0.0:
        # RAM unknown — fall back to the CPU-only default.
        return WorkerRecommendation(
            cpu_default, ram_gb, cpu, f"{cpu_default} workers (CPU/2; RAM unknown)."
        )

    if ram_gb > _BASE_RESERVE_GB:
        ram_limited = max(1, int((ram_gb - _BASE_RESERVE_GB) // _PER_WORKER_GB))
    else:
        ram_limited = 1

    recommended = max(1, min(cpu_default, ram_limited))
    if recommended < cpu_default:
        note = (
            f"{ram_gb:.0f} GB RAM limits workers to {recommended} "
            f"(CPU could run {cpu_default})."
        )
    else:
        note = f"{recommended} workers (CPU/2); {ram_gb:.0f} GB RAM is ample."
    return WorkerRecommendation(recommended, ram_gb, cpu, note)


def detect_system_ram_gb() -> float:
    """Total system RAM in GB, or 0.0 if psutil is unavailable."""
    try:
        import psutil

        return psutil.virtual_memory().total / (1024**3)
    except Exception:
        return 0.0
