"""HKX creature parity comparator.

Two comparison strategies, chosen per creature:

1. **identical_pair** (Deathclaw, Ghoul, etc. — creatures present in both FO76
   and vanilla FO4 with the same skeleton and behavior topology). Run the
   full FO76→FO4 conversion on the source, then diff the converted HKXFile
   against the vanilla FO4 HKXFile field-by-field. Classes, class counts,
   and scalar/vector member values must all match (floats compared within
   `tolerance`). Catches any regression that changes the class list or
   corrupts individual member values.

   Identical-pair tests may supply a `ParityAllowlist` to document fields
   that are legitimate FO76-source divergences (e.g. FO76 retuned the
   Deathclaw foot IK heights, or carries a runtime property-sheet array
   vanilla doesn't have). Allowlist entries require a human-readable
   reason — the check output lists applied allowances at the end of the
   report so the waiver is visible.

2. **schema** (Snallygaster, Sheepsquatch, Scorchbeast, Floater — creatures
   that only exist in FO76, with no vanilla FO4 equivalent). Run the
   conversion, then validate the converted output against a
   `CreatureSchema`: required classes exist, required files exist, key
   fields inside hkbCharacterData pass their predicate checks (scale
   non-zero, model axis vectors non-zero, animation bundle non-empty,
   etc.), and no stray directories exist in the output. Catches the
   corruption class where `character.hkx` writes scale=0, zero-vector
   model axes, and empty animation bundles — none of which an
   identical-pair test can catch since there's no FO4 Snallygaster to
   diff against.

Both strategies return a `ComparisonResult` with per-check pass/fail and
actionable failure messages. The comparator does not raise on check
failures; it is the caller's (the pytest test) responsibility to assert
on `result.passed`.
"""
from __future__ import annotations

import fnmatch
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Iterable

from creation_lib.hkxpack import detect_format, load_hkx_bytes
from creation_lib.hkxpack.model import (
    HKXArrayMember,
    HKXDirectMember,
    HKXEnumMember,
    HKXFile,
    HKXObject,
    HKXPointerMember,
    HKXStringMember,
)
from creation_lib.havok.native_runtime import _require_native


# ── Result types ────────────────────────────────────────────────────────────

@dataclass
class CheckResult:
    """One atomic check within a ComparisonResult.

    `message` is an actionable human-readable string when `passed` is False;
    empty string when passing. The tests surface this directly in assertion
    failures so the output points at the specific field/value at issue.
    """
    name: str
    passed: bool
    message: str = ""


@dataclass
class ComparisonResult:
    """Aggregated result of a single HKX comparison / validation run."""
    subject: str            # creature name or file path
    strategy: str           # "identical_pair" | "schema"
    checks: list[CheckResult] = field(default_factory=list)
    allowances: list[str] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        return all(c.passed for c in self.checks)

    def failures(self) -> list[CheckResult]:
        return [c for c in self.checks if not c.passed]

    def format_report(self) -> str:
        """Produce a human-readable report for use in pytest failure output."""
        lines = [
            f"ComparisonResult({self.subject}, strategy={self.strategy}, "
            f"{len(self.checks)} checks, passed={self.passed})",
        ]
        for c in self.checks:
            marker = "PASS" if c.passed else "FAIL"
            lines.append(f"  [{marker}] {c.name}")
            if not c.passed and c.message:
                for ml in c.message.splitlines():
                    lines.append(f"         {ml}")
        if self.allowances:
            lines.append(f"  Applied allowances ({len(self.allowances)}):")
            for a in self.allowances:
                lines.append(f"         {a}")
        return "\n".join(lines)

    def add(self, check: CheckResult) -> None:
        self.checks.append(check)


@dataclass
class ParityAllowlist:
    """Per-test waivers for legitimate FO76-source divergences from vanilla FO4.

    Identical-pair tests (e.g. Deathclaw) occasionally diff on fields that
    are not conversion bugs: FO76 retuned some values (Deathclaw foot IK
    heights), added a runtime property system vanilla doesn't have, uses a
    different bone ordering in its skeleton, etc. Rather than silently
    lower the test bar, each test passes an explicit allowlist naming the
    divergences, with a human-readable reason for each.

    Entry kinds:

    - ``extra_classes`` — class names the converted file may have that the
      expected file doesn't. Keyed by class, value is the reason.
      Used for `hkbBoneWeightArray` stray entries that FO76 carries as
      property-system payloads with no vanilla equivalent.

    - ``extra_class_counts`` — mapping of class name to max allowed extra
      count (converted - expected). Used alongside `extra_classes` for
      classes that also appear in vanilla but FO76 has more of.

    - ``field_patterns`` — glob-style (``fnmatch``) dotted field paths as
      reported by `_diff_by_class`, e.g.
      ``hkbFootIkDriverInfo[0].legs.array[*]: nested struct diff: .ankleIndex*``.
      Matched fields are reported as allowances, not failures. Use
      simplified patterns that match only the *prefix* of the issue label
      (e.g. ``hkbFootIkDriverInfo[0].legs``) — every field issue is
      matched against each pattern using substring-prefix rules, not
      full fnmatch, so you don't need to spell out the nested diff text.

    Each entry carries a reason string that is surfaced in the report as
    an "Applied allowance" line. If an allowlisted entry doesn't
    actually match anything in the diff, that's a warning — the field
    may already be fixed and the waiver can be dropped.
    """
    extra_classes: dict[str, str] = field(default_factory=dict)
    extra_class_counts: dict[str, tuple[int, str]] = field(default_factory=dict)
    field_patterns: dict[str, str] = field(default_factory=dict)

    def match_field(self, label: str) -> str | None:
        """Return the allowance reason for a field issue label, or None."""
        for pattern, reason in self.field_patterns.items():
            if _label_matches_allowlist(label, pattern):
                return reason
        return None


def _label_matches_allowlist(label: str, pattern: str) -> bool:
    """Match a field-issue label against an allowlist pattern.

    Supports:
    - ``*`` anywhere as a wildcard (fnmatch semantics)
    - A bare prefix like ``hkbFootIkDriverInfo[0].legs`` matching any
      label that starts with that string
    """
    if fnmatch.fnmatchcase(label, pattern):
        return True
    if "*" not in pattern and label.startswith(pattern):
        return True
    return False


# ── HKX loading ─────────────────────────────────────────────────────────────

def load_hkx_any(path: Path) -> HKXFile:
    """Load an HKX packfile or tagfile and return the in-memory model.

    Applies the 2015→2014 migration when reading TAG0 tagfiles, so the
    returned HKXFile is always in the target FO4 shape. Raises ValueError
    on unrecognized formats.
    """
    data = Path(path).read_bytes()
    fmt = detect_format(data)
    if fmt is None:
        raise ValueError(f"Unrecognized HKX format: {path}")
    fmt_type, version_name = fmt
    if fmt_type == "tagfile":
        if version_name.startswith(("2015", "2016")):
            data = bytes(_require_native().havok_convert_bytes(data, "fo4"))
        hkx, _registry = load_hkx_bytes(data)
        return hkx
    if fmt_type == "packfile":
        hkx, _registry = load_hkx_bytes(data)
        return hkx
    raise ValueError(f"Unknown format type {fmt_type!r} for {path}")


def convert_fo76_to_fo4(source: Path, dest: Path) -> None:
    """Run the real HavokConverter pipeline on a source HKX.

    This is what the orchestrator's havok phase calls per file. Tests use
    it directly so the suite exercises the same code path as a live
    conversion without spinning up the full 8-phase orchestrator.
    """
    from creation_lib.havok_convert.converter import HavokConverter
    dest.parent.mkdir(parents=True, exist_ok=True)
    converter = HavokConverter()
    converter.convert_file(str(source), str(dest), target_version=53)  # 53 = FO4


# ── HKXObject field access helpers ──────────────────────────────────────────

def find_objects(hkx: HKXFile, class_name: str) -> list[HKXObject]:
    return [o for o in hkx.objects if o.class_name == class_name]


def get_member(obj: HKXObject, name: str):
    """Return the first member of `obj` with matching name, or None."""
    for m in obj.members:
        if m.name == name:
            return m
    return None


def get_scalar(obj: HKXObject, name: str) -> Any:
    """Return a primitive member's .value, or None."""
    m = get_member(obj, name)
    if isinstance(m, HKXDirectMember):
        return m.value
    return None


def get_vector(obj: HKXObject, name: str) -> list[float] | None:
    """Return a vector/complex member's value list, or None."""
    m = get_member(obj, name)
    if isinstance(m, HKXDirectMember) and isinstance(m.value, list):
        return [float(v) for v in m.value]
    return None


def get_array(obj: HKXObject, name: str) -> list[Any] | None:
    m = get_member(obj, name)
    if isinstance(m, HKXArrayMember):
        return m.contents
    return None


def iter_nested_structs(member: HKXDirectMember) -> Iterable[HKXObject]:
    """Yield HKXObject descendants of a direct-member whose value is a nested struct."""
    if isinstance(member.value, HKXObject):
        yield member.value
        for m in member.value.members:
            if isinstance(m, HKXDirectMember):
                yield from iter_nested_structs(m)


def canonicalize_value(val: Any) -> Any:
    """Convert a member value into a hashable / comparable canonical form.

    Nested HKXObject struct values are recursively canonicalized into a
    tuple of (class_name, frozenset(members)), making them suitable for
    set operations and equality comparisons. Lists stay as tuples.
    """
    if isinstance(val, HKXObject):
        return (
            "struct",
            val.class_name,
            tuple(
                (m.name, canonicalize_member(m)) for m in val.members
            ),
        )
    if isinstance(val, list):
        return tuple(canonicalize_value(v) for v in val)
    return val


def canonicalize_member(m) -> Any:
    """Reduce an HKX member to a canonical tuple for diffing."""
    if isinstance(m, HKXDirectMember):
        return ("direct", canonicalize_value(m.value))
    if isinstance(m, HKXArrayMember):
        return (
            "array",
            m.ctype,
            tuple(canonicalize_value(v) for v in m.contents),
        )
    if isinstance(m, HKXPointerMember):
        # Pointer targets are local names (#0042). We care about null vs
        # non-null, not the exact id, since ids reshuffle across writes.
        return ("ptr", "null" if not m.target else "non-null")
    if isinstance(m, HKXStringMember):
        return ("str", m.value, m.is_null)
    if isinstance(m, HKXEnumMember):
        return ("enum", m.enum_name, m.value)
    return ("unknown", repr(m))


# ── Check predicates (used in CreatureSchema) ───────────────────────────────

class Check:
    """Base predicate. Subclasses override `evaluate`.

    Each subclass is a tiny dataclass-ish value type. The schema assembles
    instances; `validate_hkx_schema` invokes `evaluate` per field.
    """
    def evaluate(self, value: Any) -> tuple[bool, str]:
        raise NotImplementedError


@dataclass
class NonZero(Check):
    """Scalar must be a non-zero number."""
    def evaluate(self, value: Any) -> tuple[bool, str]:
        if value is None:
            return False, "field missing from record"
        try:
            v = float(value)
        except (TypeError, ValueError):
            return False, f"field is {value!r}, not a number"
        if v == 0.0:
            return False, f"field is zero (value={value!r})"
        return True, ""


@dataclass
class NonZeroVector(Check):
    """Vector must have at least one non-zero component."""
    def evaluate(self, value: Any) -> tuple[bool, str]:
        if value is None:
            return False, "vector field missing from record"
        if not isinstance(value, (list, tuple)):
            return False, f"expected vector, got {type(value).__name__}"
        if all(float(x) == 0.0 for x in value):
            return False, f"vector is all zeros: {list(value)}"
        return True, ""


@dataclass
class NonEmpty(Check):
    """Array field must contain at least one element."""
    def evaluate(self, value: Any) -> tuple[bool, str]:
        if value is None:
            return False, "array field missing from record"
        if not hasattr(value, "__len__"):
            return False, f"expected array, got {type(value).__name__}"
        if len(value) == 0:
            return False, "array is empty"
        return True, ""


@dataclass
class HasNonZeroEntry(Check):
    """Array must contain at least one element that itself has a non-zero value.

    Used for hkbVariableValueSet.wordVariableValues where an empty list
    AND a list of all-zero entries both indicate the reader produced
    nothing useful. Accepts an optional `getter` callable so callers can
    pull the "value" field out of elements that are themselves structs.
    """
    getter: Callable[[Any], Any] | None = None

    def evaluate(self, value: Any) -> tuple[bool, str]:
        if not value:
            return False, "array is empty or missing"
        for elem in value:
            v = self.getter(elem) if self.getter else elem
            try:
                if float(v) != 0.0:
                    return True, ""
            except (TypeError, ValueError):
                pass
        return False, f"no non-zero entry in array of {len(value)} elements"


@dataclass
class Equal(Check):
    """Exact equality check."""
    expected: Any

    def evaluate(self, value: Any) -> tuple[bool, str]:
        if value == self.expected:
            return True, ""
        return False, f"got {value!r}, expected {self.expected!r}"


@dataclass
class InRange(Check):
    """Numeric range check (inclusive)."""
    lo: float
    hi: float

    def evaluate(self, value: Any) -> tuple[bool, str]:
        try:
            v = float(value)
        except (TypeError, ValueError):
            return False, f"not a number: {value!r}"
        if v < self.lo or v > self.hi:
            return False, f"out of range [{self.lo}, {self.hi}]: {v}"
        return True, ""


# ── CreatureSchema ──────────────────────────────────────────────────────────

@dataclass
class CreatureSchema:
    """A 'valid output' schema for an FO76-only creature conversion.

    Since there is no vanilla FO4 equivalent to diff against, we instead
    assert that the converted output (directory of .hkx files) has the
    required shape. Start simple: flat dict of field name -> Check, plus
    file-set and stray-dir lists. The schema operates only on the
    creature's character.hkx; behavior/animation validation is left to
    other tests that don't know the expected FO4 schema.

    `forbidden_fields_per_class` asserts that specific member names DO NOT
    appear on any object of that class — used to catch FO76-only fields
    (memSizeAndFlags, refCount, numHands, propertySheets) that leak through
    the tagfile reader when it picks the wrong class schema.
    """
    name: str
    required_classes: list[str] = field(default_factory=list)
    character_data_checks: dict[str, Check] = field(default_factory=dict)
    character_string_data_checks: dict[str, Check] = field(default_factory=dict)
    variable_value_set_checks: dict[str, Check] = field(default_factory=dict)
    required_files: list[str] = field(default_factory=list)
    forbidden_stray_patterns: list[str] = field(default_factory=list)
    forbidden_fields_per_class: dict[str, list[str]] = field(default_factory=dict)


# ── Strategy 1: identical-pair comparison ───────────────────────────────────

def compare_hkx_files(
    converted: Path,
    expected: Path,
    *,
    tolerance: float = 1e-4,
    subject: str | None = None,
    allowlist: ParityAllowlist | None = None,
) -> ComparisonResult:
    """Diff a converted HKX against a vanilla FO4 HKX, field by field.

    Used for identical-creature pairs (Deathclaw etc.) where both games
    share the same creature and the converted output should be
    functionally indistinguishable from the vanilla file.

    Checks:
        - Class list equality (no missing, no extra)
        - Per-class object count
        - For each paired object (by class, in order): each member's
          canonical value, with numeric tolerance applied to floats

    `allowlist` (optional) suppresses specific known FO76-source
    divergences — see `ParityAllowlist`. Every allowlisted entry that
    actually matches a diff is recorded in `result.allowances` so the
    waivers are always visible in the pytest failure output.

    Returns a ComparisonResult with one CheckResult per top-level check.
    """
    result = ComparisonResult(
        subject=subject or str(converted),
        strategy="identical_pair",
    )
    al = allowlist or ParityAllowlist()

    try:
        conv = load_hkx_any(converted)
        exp = load_hkx_any(expected)
    except Exception as e:
        result.add(CheckResult("load", False, f"{type(e).__name__}: {e}"))
        return result
    result.add(CheckResult("load", True))

    # 1. Class list
    conv_classes = sorted({o.class_name for o in conv.objects})
    exp_classes = sorted({o.class_name for o in exp.objects})
    missing = sorted(set(exp_classes) - set(conv_classes))
    extra_raw = sorted(set(conv_classes) - set(exp_classes))
    extra = []
    for cls in extra_raw:
        if cls in al.extra_classes:
            result.allowances.append(
                f"class_list: extra class {cls!r} ({al.extra_classes[cls]})"
            )
        else:
            extra.append(cls)
    if missing or extra:
        msg_parts = []
        if missing:
            msg_parts.append(f"missing: {missing}")
        if extra:
            msg_parts.append(f"extra: {extra}")
        result.add(CheckResult("class_list", False, "; ".join(msg_parts)))
    else:
        result.add(CheckResult("class_list", True))

    # 2. Class counts
    from collections import Counter
    conv_counts = Counter(o.class_name for o in conv.objects)
    exp_counts = Counter(o.class_name for o in exp.objects)
    count_issues = []
    for cls in set(conv_counts) | set(exp_counts):
        if conv_counts[cls] == exp_counts[cls]:
            continue
        diff = conv_counts[cls] - exp_counts[cls]
        # Fully-waived class (extra_classes): allow any count mismatch for it
        if cls in al.extra_classes and exp_counts[cls] == 0:
            result.allowances.append(
                f"class_counts: {cls} count {conv_counts[cls]} "
                f"(allowed: {al.extra_classes[cls]})"
            )
            continue
        # Explicit extra-count waiver
        if cls in al.extra_class_counts:
            max_extra, reason = al.extra_class_counts[cls]
            if diff > 0 and diff <= max_extra:
                result.allowances.append(
                    f"class_counts: {cls} converted={conv_counts[cls]} "
                    f"expected={exp_counts[cls]} ({reason})"
                )
                continue
        count_issues.append(
            f"{cls}: converted={conv_counts[cls]} expected={exp_counts[cls]}"
        )
    if count_issues:
        result.add(CheckResult("class_counts", False, "\n".join(count_issues)))
    else:
        result.add(CheckResult("class_counts", True))

    # 3. Per-class field diff
    field_issues_raw = _diff_by_class(conv, exp, tolerance)
    field_issues: list[str] = []
    for issue in field_issues_raw:
        reason = al.match_field(issue)
        if reason is None:
            field_issues.append(issue)
        else:
            result.allowances.append(f"field_values: {issue} ({reason})")
    if field_issues:
        result.add(
            CheckResult(
                "field_values",
                False,
                "\n".join(field_issues[:30])
                + (f"\n... and {len(field_issues) - 30} more"
                   if len(field_issues) > 30 else ""),
            )
        )
    else:
        result.add(CheckResult("field_values", True))

    return result


def _diff_by_class(
    conv: HKXFile, exp: HKXFile, tolerance: float
) -> list[str]:
    """Walk each shared class's object list in order and report mismatched members.

    Objects are paired by position within their class group, since the
    packfile doesn't preserve stable identity across read/write.
    """
    by_class_conv: dict[str, list[HKXObject]] = {}
    by_class_exp: dict[str, list[HKXObject]] = {}
    for o in conv.objects:
        by_class_conv.setdefault(o.class_name, []).append(o)
    for o in exp.objects:
        by_class_exp.setdefault(o.class_name, []).append(o)

    issues: list[str] = []
    for cls in sorted(set(by_class_conv) & set(by_class_exp)):
        conv_list = by_class_conv[cls]
        exp_list = by_class_exp[cls]
        pair_count = min(len(conv_list), len(exp_list))
        for idx in range(pair_count):
            issues.extend(
                _diff_object_members(
                    f"{cls}[{idx}]", conv_list[idx], exp_list[idx], tolerance
                )
            )
    return issues


def _diff_object_members(
    label: str, a: HKXObject, b: HKXObject, tolerance: float
) -> list[str]:
    """Compare every member of two HKXObjects, returning a list of human-readable issues."""
    issues: list[str] = []
    a_members = {m.name: m for m in a.members}
    b_members = {m.name: m for m in b.members}

    only_a = sorted(set(a_members) - set(b_members))
    only_b = sorted(set(b_members) - set(a_members))
    for name in only_a:
        issues.append(f"{label}.{name}: extra (not in expected)")
    for name in only_b:
        issues.append(f"{label}.{name}: missing (in expected, not in converted)")

    for name in sorted(set(a_members) & set(b_members)):
        ma = a_members[name]
        mb = b_members[name]
        if type(ma) is not type(mb):
            issues.append(
                f"{label}.{name}: type differs "
                f"({type(ma).__name__} vs {type(mb).__name__})"
            )
            continue
        msg = _compare_members(ma, mb, tolerance)
        if msg:
            issues.append(f"{label}.{name}: {msg}")
    return issues


def _compare_members(a, b, tolerance: float) -> str:
    """Return empty string if the two members are equivalent, else a description."""
    if isinstance(a, HKXDirectMember):
        return _compare_values(a.value, b.value, tolerance)
    if isinstance(a, HKXArrayMember):
        if len(a.contents) != len(b.contents):
            return f"array len {len(a.contents)} vs {len(b.contents)}"
        if a.ctype != b.ctype:
            return f"array ctype {a.ctype!r} vs {b.ctype!r}"
        for i, (ea, eb) in enumerate(zip(a.contents, b.contents)):
            msg = _compare_values(ea, eb, tolerance)
            if msg:
                return f"array[{i}]: {msg}"
        return ""
    if isinstance(a, HKXPointerMember):
        a_null = not a.target
        b_null = not b.target
        if a_null != b_null:
            return f"pointer null {a_null} vs {b_null}"
        return ""
    if isinstance(a, HKXStringMember):
        if a.is_null != b.is_null:
            return f"string is_null {a.is_null} vs {b.is_null}"
        if a.value != b.value:
            return f"string {a.value!r} vs {b.value!r}"
        return ""
    if isinstance(a, HKXEnumMember):
        if a.value != b.value:
            return f"enum value {a.value!r} vs {b.value!r}"
        return ""
    return ""


def _compare_values(a, b, tolerance: float) -> str:
    """Compare two direct-member values, honoring tolerance for floats."""
    if isinstance(a, HKXObject) and isinstance(b, HKXObject):
        # Recurse into nested inline structs
        sub_issues = _diff_object_members("", a, b, tolerance)
        if sub_issues:
            return "nested struct diff: " + "; ".join(sub_issues[:5])
        return ""
    if isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            return f"list len {len(a)} vs {len(b)}"
        for i, (ea, eb) in enumerate(zip(a, b)):
            if isinstance(ea, float) or isinstance(eb, float):
                if abs(float(ea) - float(eb)) > tolerance:
                    return f"list[{i}] {ea} vs {eb}"
            elif ea != eb:
                return f"list[{i}] {ea!r} vs {eb!r}"
        return ""
    if isinstance(a, float) or isinstance(b, float):
        try:
            if abs(float(a) - float(b)) > tolerance:
                return f"{a} vs {b}"
        except (TypeError, ValueError):
            pass
        return ""
    if a != b:
        return f"{a!r} vs {b!r}"
    return ""


# ── Strategy 2: schema validation ───────────────────────────────────────────

def validate_hkx_schema(
    character_hkx: Path,
    schema: CreatureSchema,
    *,
    converted_dir: Path | None = None,
) -> ComparisonResult:
    """Validate a converted character.hkx + optional output dir against a schema.

    `character_hkx` is the converted file. If `converted_dir` is given,
    the schema's `required_files` / `forbidden_stray_patterns` are also
    checked against the directory listing.
    """
    result = ComparisonResult(subject=schema.name, strategy="schema")

    try:
        hkx = load_hkx_any(character_hkx)
    except Exception as e:
        result.add(CheckResult("load", False, f"{type(e).__name__}: {e}"))
        return result
    result.add(CheckResult("load", True))

    # 1. Required classes
    present_classes = {o.class_name for o in hkx.objects}
    missing_classes = [c for c in schema.required_classes if c not in present_classes]
    if missing_classes:
        result.add(
            CheckResult(
                "required_classes",
                False,
                f"missing: {missing_classes}",
            )
        )
    else:
        result.add(CheckResult("required_classes", True))

    # 2. hkbCharacterData field checks
    _run_object_checks(
        result,
        hkx,
        "hkbCharacterData",
        schema.character_data_checks,
        "character_data",
    )

    # 3. hkbCharacterStringData field checks
    _run_object_checks(
        result,
        hkx,
        "hkbCharacterStringData",
        schema.character_string_data_checks,
        "character_string_data",
    )

    # 4. hkbVariableValueSet field checks
    _run_object_checks(
        result,
        hkx,
        "hkbVariableValueSet",
        schema.variable_value_set_checks,
        "variable_value_set",
    )

    # 5. Forbidden fields per class — fields that must NOT appear on any
    # object of the given class. Used to catch FO76-only fields that the
    # tagfile reader leaks when it picks the wrong class schema.
    for class_name, forbidden_names in schema.forbidden_fields_per_class.items():
        objs = find_objects(hkx, class_name)
        offenders: list[str] = []
        for obj_idx, obj in enumerate(objs):
            for m in obj.members:
                if m.name in forbidden_names:
                    offenders.append(f"{class_name}[{obj_idx}].{m.name}")
        if offenders:
            result.add(
                CheckResult(
                    f"forbidden_fields.{class_name}",
                    False,
                    f"found FO76-only fields: {offenders}",
                )
            )
        else:
            result.add(CheckResult(f"forbidden_fields.{class_name}", True))

    # 6. Required files and stray directories
    if converted_dir is not None and schema.required_files:
        missing_files = [
            rel for rel in schema.required_files
            if not (converted_dir / rel).is_file()
        ]
        if missing_files:
            result.add(
                CheckResult(
                    "required_files",
                    False,
                    f"missing: {missing_files}",
                )
            )
        else:
            result.add(CheckResult("required_files", True))

    if converted_dir is not None and schema.forbidden_stray_patterns:
        strays: list[str] = []
        for path in converted_dir.rglob("*"):
            if not path.is_file():
                continue
            rel_posix = path.relative_to(converted_dir).as_posix()
            for pat in schema.forbidden_stray_patterns:
                if fnmatch.fnmatch(rel_posix, pat):
                    strays.append(rel_posix)
                    break
        if strays:
            result.add(
                CheckResult(
                    "forbidden_strays",
                    False,
                    f"found {len(strays)} stray files: {strays[:10]}",
                )
            )
        else:
            result.add(CheckResult("forbidden_strays", True))

    return result


def _resolve_field_path(obj: HKXObject, path: str):
    """Walk a dotted field path through nested inline-struct members.

    Each segment of `path` is a member name on the current HKXObject;
    inline struct values (HKXDirectMember whose `.value` is itself an
    HKXObject) are traversed transparently. Returns the terminal member
    object, or (None, "error string") on miss.
    """
    segments = path.split(".")
    current_obj: HKXObject | None = obj
    for i, seg in enumerate(segments):
        if current_obj is None:
            return None, f"parent missing at segment {'.'.join(segments[:i]) or '<root>'}"
        member = get_member(current_obj, seg)
        if member is None:
            return None, f"segment {seg!r} not found at {'.'.join(segments[:i]) or '<root>'}"
        if i == len(segments) - 1:
            return member, ""
        # Must descend further — only inline-struct direct members support this.
        if isinstance(member, HKXDirectMember) and isinstance(member.value, HKXObject):
            current_obj = member.value
            continue
        return None, (
            f"segment {seg!r} at {'.'.join(segments[:i+1])} is "
            f"{type(member).__name__}, not a nested struct"
        )


def _run_object_checks(
    result: ComparisonResult,
    hkx: HKXFile,
    class_name: str,
    checks: dict[str, Check],
    label: str,
) -> None:
    """Run field predicates against the first object of the given class.

    Field names may be dotted paths (e.g.
    `characterControllerSetup.rigidBodySetup.shapeSetup.capsuleHeight`)
    to descend into nested inline-struct members. Resolves each field
    name against direct scalar members, array members, or string
    members so the same schema syntax works for any. Emits one
    CheckResult per check so failures point at the exact field.
    """
    if not checks:
        return
    objs = find_objects(hkx, class_name)
    if not objs:
        result.add(
            CheckResult(
                f"{label}.class_present",
                False,
                f"no {class_name} object in file",
            )
        )
        return
    obj = objs[0]
    for field_name, check in checks.items():
        if "." in field_name:
            member, err = _resolve_field_path(obj, field_name)
            if member is None:
                result.add(
                    CheckResult(
                        f"{label}.{field_name}",
                        False,
                        f"field missing from {class_name}: {err}",
                    )
                )
                continue
        else:
            member = get_member(obj, field_name)
            if member is None:
                result.add(
                    CheckResult(
                        f"{label}.{field_name}",
                        False,
                        f"field missing from {class_name}",
                    )
                )
                continue
        if isinstance(member, HKXDirectMember):
            passed, msg = check.evaluate(member.value)
        elif isinstance(member, HKXArrayMember):
            passed, msg = check.evaluate(member.contents)
        elif isinstance(member, HKXStringMember):
            passed, msg = check.evaluate(member.value)
        else:
            passed, msg = False, f"unsupported member type {type(member).__name__}"
        result.add(
            CheckResult(
                f"{label}.{field_name}",
                passed,
                msg,
            )
        )


# ── Skeleton.nif ragdoll collision inspection ──────────────────────────────

# FO76 skeletons embed Havok collision as hk_2015 TAG0 tagfile blobs inside
# two NIF block types: `bhkPhysicsSystem` (loose collision, one rigid body)
# and `bhkRagdollSystem` (full ragdoll, many bodies + constraint chain). The
# conversion pipeline (`py_creation_lib/python/creation_lib/nif/convert_physics.py`) routes each blob through
# `HavokConverter.convert_bytes` which runs the same tagfile reader + 2015→
# 2014 migration + packfile writer as character.hkx conversion.
#
# The checks below validate the Havok contents of those blobs after
# conversion:
#   - `hknpRagdollData` exists
#   - `bodyCinfos` / `motionCinfos` / `constraintCinfos` arrays non-empty
#   - every `motionCinfos[i].inverseMass > 0` (mass=infinity → static body;
#     the FO76 ragdoll encodes dynamic bodies so all inverseMass must be >0)
#   - every `hknpCapsuleShape.convexRadius > 0` (zero radius = no collision
#     volume, the same class of bug we catch on character.hkx)
#   - capsule endpoint distance > epsilon (degenerate capsules collapse to
#     points and crash the runtime ragdoll solver)
#   - `boneToBodyMap` length ≥ `bodyCinfos` length (every body has at least
#     one bone — orphan bodies get detached from the skeleton animation)
#   - `constraintCinfos` count ≥ `bodyCinfos.length - 1` (constraint graph
#     must form a tree connecting every body back to the root)
#
# For identical-creature pairs (Deathclaw) we additionally diff the class
# counts between converted and vanilla FO4 blob — no stray classes, no
# missing classes. We do NOT diff scalar values because FO4 changes a few
# conventions (e.g. `hknpCapsuleShape.a/b` fourth component is always 1.0
# in FO4 but holds the radius in FO76; the migration correctly normalizes
# this but scalar byte-equality isn't a useful invariant here).


@dataclass
class NifCollisionSchema:
    """Validation schema for NIF embedded Havok collision data.

    Applied to each `bhkPhysicsSystem` / `bhkRagdollSystem` blob in the
    converted skeleton.nif. Counts are optional lower bounds: if set,
    the blob must contain at least that many objects of each class.
    """
    name: str
    require_ragdoll: bool = True
    min_capsule_count: int = 1
    min_body_count: int = 1
    min_constraint_count: int = 0
    min_bones_in_map: int = 1
    # Classes that must be absent from converted blobs. FO76-only classes
    # that the 2015→2014 migration is supposed to strip land here.
    forbidden_classes: list[str] = field(default_factory=lambda: [
        "hknpRefMassDistribution",  # FO76-only, migration strips it
    ])


def _extract_havok_blobs_from_nif(nif_path: Path) -> list[tuple[str, int, bytes]]:
    """Return every (block_type, block_id, blob_bytes) from a skeleton.nif.

    Walks bhkPhysicsSystem and bhkRagdollSystem blocks, reads their
    `Binary Data.Data` list-of-ints, and returns the raw payload bytes
    for each. Used by both the inline loader below and by tests that
    need to reconvert and reinspect after a run.
    """
    from creation_lib.nif.nif_file import NifFile
    nif = NifFile.load(str(nif_path))
    out: list[tuple[str, int, bytes]] = []
    for block in nif.blocks:
        if block.type_name not in ("bhkPhysicsSystem", "bhkRagdollSystem"):
            continue
        bd = block.get_field("Binary Data")
        if not isinstance(bd, dict):
            continue
        data = bd.get("Data")
        if data is None:
            continue
        raw = bytes(data) if not isinstance(data, (bytes, bytearray)) else bytes(data)
        if raw:
            out.append((block.type_name, block.block_id, raw))
    return out


def _load_havok_blob(blob: bytes) -> HKXFile:
    """Load a NIF-embedded Havok blob (packfile or tagfile) into HKXFile.

    Unlike `load_hkx_any`, this does NOT apply the 2015→2014 migration:
    the migration is already performed by `HavokConverter.convert_bytes`
    during NIF conversion, so loading the post-conversion blob should
    see an already-migrated 2014 packfile. Pre-conversion (source) blobs
    still deserialize against the 2015 classxml registry.
    """
    fmt = detect_format(blob)
    if fmt is None:
        raise ValueError("unknown havok format in blob")
    fmt_type, _version_name = fmt
    if fmt_type in ("packfile", "tagfile"):
        hkx, _registry = load_hkx_bytes(blob)
        return hkx
    raise ValueError(f"unsupported havok format: {fmt_type!r}")


def convert_skeleton_nif_physics(source: Path, dest: Path) -> None:
    """Run the real convert_physics pass on a skeleton.nif.

    Mirrors what `_phase_nifs` does in the orchestrator. Reads the
    source NIF, routes every embedded bhkPhysicsSystem/bhkRagdollSystem
    blob through HavokConverter, and writes the result to `dest` with
    the FO4 header version so the downstream nif reader accepts the
    converted blocks. Used by the skeleton parity tests.
    """
    from creation_lib.nif.nif_file import NifFile
    from creation_lib.havok_convert import HavokConverter, FO4

    nif = NifFile.load(str(source))
    converter = HavokConverter()
    for block in nif.blocks:
        if block.type_name not in ("bhkPhysicsSystem", "bhkRagdollSystem"):
            continue
        bd = block.get_field("Binary Data")
        if not isinstance(bd, dict):
            continue
        raw = bd.get("Data")
        if raw is None:
            continue
        src_bytes = bytes(raw) if not isinstance(raw, (bytes, bytearray)) else bytes(raw)
        if not src_bytes:
            continue
        out_bytes = converter.convert_bytes(src_bytes, FO4.id)
        bd["Data"] = list(out_bytes)
        bd["Data Size"] = len(out_bytes)
        block.set_field("Binary Data", bd)

    dest.parent.mkdir(parents=True, exist_ok=True)
    nif.save(str(dest))


def _get_array_contents(obj: HKXObject, name: str) -> list[Any] | None:
    m = get_member(obj, name)
    if isinstance(m, HKXArrayMember):
        return m.contents
    return None


def _get_nested_scalar(obj: HKXObject, name: str) -> Any:
    """Return the value of a scalar member on a nested inline struct."""
    m = get_member(obj, name)
    if isinstance(m, HKXDirectMember):
        return m.value
    return None


def _inspect_ragdoll_blob(
    result: ComparisonResult,
    hkx: HKXFile,
    schema: NifCollisionSchema,
    label: str,
) -> None:
    """Run ragdoll-blob checks and append one CheckResult per check."""
    rd_objs = find_objects(hkx, "hknpRagdollData")
    if schema.require_ragdoll and not rd_objs:
        result.add(
            CheckResult(
                f"{label}.ragdoll_present",
                False,
                "no hknpRagdollData in blob",
            )
        )
        return
    if not rd_objs:
        return
    rd = rd_objs[0]

    body_cinfos = _get_array_contents(rd, "bodyCinfos") or []
    motion_cinfos = _get_array_contents(rd, "motionCinfos") or []
    constraint_cinfos = _get_array_contents(rd, "constraintCinfos") or []
    bone_to_body_map = _get_array_contents(rd, "boneToBodyMap") or []

    # Body count
    if len(body_cinfos) < schema.min_body_count:
        result.add(
            CheckResult(
                f"{label}.bodyCinfos",
                False,
                f"only {len(body_cinfos)} bodies, expected >= {schema.min_body_count}",
            )
        )
    else:
        result.add(CheckResult(f"{label}.bodyCinfos", True))

    # Motion cinfos present and aligned with bodies
    if len(motion_cinfos) != len(body_cinfos):
        result.add(
            CheckResult(
                f"{label}.motionCinfos",
                False,
                f"motionCinfos={len(motion_cinfos)} vs bodyCinfos={len(body_cinfos)} "
                f"— counts should match in a well-formed ragdoll",
            )
        )
    else:
        result.add(CheckResult(f"{label}.motionCinfos", True))

    # Every motion's inverseMass must be > 0 (dynamic body). inverseMass=0
    # means mass=infinity → static body, which is legal but invalid for a
    # ragdoll body chain.
    zero_mass_bodies: list[int] = []
    for idx, ci in enumerate(motion_cinfos):
        inv_mass = None
        if isinstance(ci, HKXObject):
            inv_mass = _get_nested_scalar(ci, "inverseMass")
        if inv_mass is None or inv_mass <= 0.0:
            zero_mass_bodies.append(idx)
    if zero_mass_bodies:
        result.add(
            CheckResult(
                f"{label}.inverseMass",
                False,
                f"{len(zero_mass_bodies)} bodies have inverseMass<=0 "
                f"(first few: {zero_mass_bodies[:5]}) — mass=infinity means "
                f"the ragdoll body is static and cannot animate",
            )
        )
    else:
        result.add(CheckResult(f"{label}.inverseMass", True))

    # Constraint count — constraint graph must form a tree
    if len(constraint_cinfos) < len(body_cinfos) - 1:
        result.add(
            CheckResult(
                f"{label}.constraintCinfos",
                False,
                f"only {len(constraint_cinfos)} constraints for "
                f"{len(body_cinfos)} bodies — needs >= {len(body_cinfos) - 1} "
                f"to form a connected tree",
            )
        )
    else:
        result.add(CheckResult(f"{label}.constraintCinfos", True))

    # Bone-to-body map covers every body
    if len(bone_to_body_map) < len(body_cinfos):
        result.add(
            CheckResult(
                f"{label}.boneToBodyMap",
                False,
                f"boneToBodyMap has {len(bone_to_body_map)} entries "
                f"for {len(body_cinfos)} bodies — orphan bodies will "
                f"not follow the animated skeleton",
            )
        )
    else:
        result.add(CheckResult(f"{label}.boneToBodyMap", True))


def _inspect_capsule_shapes(
    result: ComparisonResult,
    hkx: HKXFile,
    schema: NifCollisionSchema,
    label: str,
) -> None:
    """Check every hknpCapsuleShape for non-degenerate geometry."""
    caps = find_objects(hkx, "hknpCapsuleShape")
    if len(caps) < schema.min_capsule_count:
        result.add(
            CheckResult(
                f"{label}.capsule_count",
                False,
                f"only {len(caps)} hknpCapsuleShape objects, "
                f"expected >= {schema.min_capsule_count}",
            )
        )
        return
    result.add(CheckResult(f"{label}.capsule_count", True))

    bad_radius: list[int] = []
    bad_length: list[int] = []
    for idx, cap in enumerate(caps):
        radius = _get_nested_scalar(cap, "convexRadius")
        a = _get_nested_scalar(cap, "a")
        b = _get_nested_scalar(cap, "b")
        if radius is None or float(radius) <= 0.0:
            bad_radius.append(idx)
        if isinstance(a, list) and isinstance(b, list) and len(a) >= 3 and len(b) >= 3:
            dx, dy, dz = a[0] - b[0], a[1] - b[1], a[2] - b[2]
            length_sq = dx * dx + dy * dy + dz * dz
            if length_sq <= 1e-12:
                bad_length.append(idx)

    if bad_radius:
        result.add(
            CheckResult(
                f"{label}.capsule_radius",
                False,
                f"{len(bad_radius)} capsules have radius<=0 "
                f"(indices: {bad_radius[:10]})",
            )
        )
    else:
        result.add(CheckResult(f"{label}.capsule_radius", True))

    if bad_length:
        result.add(
            CheckResult(
                f"{label}.capsule_length",
                False,
                f"{len(bad_length)} capsules have endpoint distance=0 "
                f"(indices: {bad_length[:10]}) — collapses to a point",
            )
        )
    else:
        result.add(CheckResult(f"{label}.capsule_length", True))


def _check_forbidden_classes(
    result: ComparisonResult,
    hkx: HKXFile,
    schema: NifCollisionSchema,
    label: str,
) -> None:
    """Verify no FO76-only classes survived the 2015→2014 migration."""
    class_set = {o.class_name for o in hkx.objects}
    present = [c for c in schema.forbidden_classes if c in class_set]
    if present:
        result.add(
            CheckResult(
                f"{label}.forbidden_classes",
                False,
                f"FO76-only classes leaked into converted blob: {present}",
            )
        )
    else:
        result.add(CheckResult(f"{label}.forbidden_classes", True))


def validate_nif_collision_schema(
    nif_path: Path,
    schema: NifCollisionSchema,
) -> ComparisonResult:
    """Validate every Havok blob in a skeleton.nif against a schema.

    Loads the NIF, extracts bhkPhysicsSystem/bhkRagdollSystem blob
    bytes, deserializes each blob with `creation_lib.hkxpack`, and runs capsule
    + ragdoll integrity checks. Used for FO76-only creatures where
    there is no vanilla FO4 equivalent to diff against.
    """
    result = ComparisonResult(subject=schema.name, strategy="nif_collision_schema")
    try:
        blobs = _extract_havok_blobs_from_nif(nif_path)
    except Exception as e:
        result.add(CheckResult("nif_load", False, f"{type(e).__name__}: {e}"))
        return result
    result.add(CheckResult("nif_load", True))

    if not blobs:
        result.add(
            CheckResult(
                "havok_blobs_present",
                False,
                "skeleton.nif has no bhkPhysicsSystem/bhkRagdollSystem blocks",
            )
        )
        return result
    result.add(CheckResult("havok_blobs_present", True))

    ragdoll_seen = False
    for block_type, block_id, blob in blobs:
        label = f"{block_type}[{block_id}]"
        try:
            hkx = _load_havok_blob(blob)
        except Exception as e:
            result.add(
                CheckResult(
                    f"{label}.load",
                    False,
                    f"{type(e).__name__}: {e}",
                )
            )
            continue
        result.add(CheckResult(f"{label}.load", True))

        _check_forbidden_classes(result, hkx, schema, label)
        _inspect_capsule_shapes(result, hkx, schema, label)
        if block_type == "bhkRagdollSystem":
            _inspect_ragdoll_blob(result, hkx, schema, label)
            ragdoll_seen = True

    if schema.require_ragdoll and not ragdoll_seen:
        result.add(
            CheckResult(
                "ragdoll_system_present",
                False,
                "no bhkRagdollSystem block found in skeleton.nif",
            )
        )
    else:
        result.add(CheckResult("ragdoll_system_present", True))

    return result


def compare_nif_collision(
    converted: Path,
    expected: Path,
    *,
    subject: str | None = None,
) -> ComparisonResult:
    """Diff two skeleton.nifs' Havok blob class-and-count composition.

    Identical-pair strategy for creatures present in both games
    (Deathclaw). Does NOT diff scalar values — FO4's normalization of
    capsule endpoint w-components (always 1.0, radius moves to
    convexRadius) is a legitimate format difference that shouldn't
    fail this check. Instead we assert each blob has the same class
    list and count as the corresponding vanilla FO4 blob.
    """
    result = ComparisonResult(
        subject=subject or str(converted),
        strategy="nif_collision_identical_pair",
    )
    try:
        conv_blobs = _extract_havok_blobs_from_nif(converted)
        exp_blobs = _extract_havok_blobs_from_nif(expected)
    except Exception as e:
        result.add(CheckResult("load", False, f"{type(e).__name__}: {e}"))
        return result
    result.add(CheckResult("load", True))

    # Pair blobs by block type in order of appearance in the file
    by_type_conv: dict[str, list[tuple[int, bytes]]] = {}
    by_type_exp: dict[str, list[tuple[int, bytes]]] = {}
    for bt, bid, blob in conv_blobs:
        by_type_conv.setdefault(bt, []).append((bid, blob))
    for bt, bid, blob in exp_blobs:
        by_type_exp.setdefault(bt, []).append((bid, blob))

    # Check block-type count match
    for bt in set(by_type_conv) | set(by_type_exp):
        c = len(by_type_conv.get(bt, []))
        e = len(by_type_exp.get(bt, []))
        if c != e:
            result.add(
                CheckResult(
                    f"block_count.{bt}",
                    False,
                    f"converted has {c} {bt} blocks, expected has {e}",
                )
            )
        else:
            result.add(CheckResult(f"block_count.{bt}", True))

    from collections import Counter
    for bt in sorted(set(by_type_conv) & set(by_type_exp)):
        conv_list = by_type_conv[bt]
        exp_list = by_type_exp[bt]
        for idx in range(min(len(conv_list), len(exp_list))):
            _, conv_bytes = conv_list[idx]
            _, exp_bytes = exp_list[idx]
            label = f"{bt}[{idx}]"
            try:
                conv_hkx = _load_havok_blob(conv_bytes)
                exp_hkx = _load_havok_blob(exp_bytes)
            except Exception as e:
                result.add(
                    CheckResult(
                        f"{label}.load",
                        False,
                        f"{type(e).__name__}: {e}",
                    )
                )
                continue
            conv_counts = Counter(o.class_name for o in conv_hkx.objects)
            exp_counts = Counter(o.class_name for o in exp_hkx.objects)
            missing = sorted(set(exp_counts) - set(conv_counts))
            extra = sorted(set(conv_counts) - set(exp_counts))
            parts = []
            if missing:
                parts.append(f"missing: {missing}")
            if extra:
                parts.append(f"extra: {extra}")
            if parts:
                result.add(
                    CheckResult(
                        f"{label}.class_list",
                        False,
                        "; ".join(parts),
                    )
                )
            else:
                result.add(CheckResult(f"{label}.class_list", True))

            count_issues = []
            for cls in set(conv_counts) | set(exp_counts):
                if conv_counts[cls] != exp_counts[cls]:
                    count_issues.append(
                        f"{cls}: converted={conv_counts[cls]} "
                        f"expected={exp_counts[cls]}"
                    )
            if count_issues:
                result.add(
                    CheckResult(
                        f"{label}.class_counts",
                        False,
                        "\n".join(count_issues),
                    )
                )
            else:
                result.add(CheckResult(f"{label}.class_counts", True))

    return result
