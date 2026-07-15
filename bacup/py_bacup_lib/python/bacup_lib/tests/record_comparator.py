"""Field-level structural diff between two record dicts."""
from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class CompareResult:
    """Result of comparing two record dicts."""

    matched: set[str] = field(default_factory=set)
    mismatched: dict[str, tuple] = field(default_factory=dict)  # field -> (actual, expected)
    extra: set[str] = field(default_factory=set)  # in actual but not expected
    missing: set[str] = field(default_factory=set)  # in expected but not actual

    def match_ratio(self) -> float:
        total = len(self.matched) + len(self.mismatched)
        return len(self.matched) / total if total > 0 else 0.0

    def summary(self) -> str:
        lines = [
            f"Matched: {len(self.matched)}, Mismatched: {len(self.mismatched)}, "
            f"Extra: {len(self.extra)}, Missing: {len(self.missing)} "
            f"({self.match_ratio():.0%} match)",
        ]
        for f, (a, e) in sorted(self.mismatched.items()):
            lines.append(f"  MISMATCH {f}: got {_trunc(a)} expected {_trunc(e)}")
        for f in sorted(self.extra):
            lines.append(f"  EXTRA    {f}")
        for f in sorted(self.missing):
            lines.append(f"  MISSING  {f}")
        return "\n".join(lines)


def _trunc(val, maxlen: int = 80) -> str:
    s = repr(val)
    return s if len(s) <= maxlen else s[:maxlen] + "..."


class RecordComparator:
    """Compare two record dicts field-by-field."""

    @staticmethod
    def compare(
        actual: dict,
        expected: dict,
        ignore: set[str] | None = None,
    ) -> CompareResult:
        ignore = ignore or set()
        result = CompareResult()

        actual_keys = set(actual.keys()) - ignore
        expected_keys = set(expected.keys()) - ignore

        common = actual_keys & expected_keys
        result.extra = actual_keys - expected_keys
        result.missing = expected_keys - actual_keys

        for key in common:
            if _deep_eq(actual[key], expected[key]):
                result.matched.add(key)
            else:
                result.mismatched[key] = (actual[key], expected[key])

        return result


def _deep_eq(a, b) -> bool:
    """Deep equality that handles dicts, lists, and primitives."""
    if type(a) != type(b):
        # Allow int/float cross-comparison
        if isinstance(a, (int, float)) and isinstance(b, (int, float)):
            return float(a) == float(b)
        return False
    if isinstance(a, dict):
        if set(a.keys()) != set(b.keys()):
            return False
        return all(_deep_eq(a[k], b[k]) for k in a)
    if isinstance(a, list):
        if len(a) != len(b):
            return False
        return all(_deep_eq(x, y) for x, y in zip(a, b))
    return a == b
