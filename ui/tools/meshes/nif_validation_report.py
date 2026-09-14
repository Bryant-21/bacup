"""Parsing, path planning, and repair execution for NIF validation reports."""

from __future__ import annotations

import json
import os
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterable, Mapping, Sequence


_SEVERITY_RANK = {"info": 1, "warning": 2, "error": 3}


@dataclass(frozen=True, slots=True)
class ValidationFinding:
    severity: str
    rule: str
    check: str
    message: str
    block_id: int | None = None
    block_type: str = ""
    field: str = ""

    @classmethod
    def from_dict(cls, value: Mapping[str, Any]) -> "ValidationFinding":
        block_id = value.get("block_id")
        return cls(
            severity=str(value.get("severity", "warning")).lower(),
            rule=str(value.get("rule", "unknown")),
            check=str(value.get("check", "")),
            message=str(value.get("message", "")),
            block_id=block_id if isinstance(block_id, int) else None,
            block_type=str(value.get("block_type") or ""),
            field=str(value.get("field") or ""),
        )


@dataclass(frozen=True, slots=True)
class ValidationIssueFile:
    path: str
    game: str
    findings: tuple[ValidationFinding, ...]
    success: bool = True
    load_error: str = ""

    @property
    def highest_severity(self) -> str:
        if not self.success:
            return "error"
        return max(
            (finding.severity for finding in self.findings),
            key=lambda severity: _SEVERITY_RANK.get(severity, 0),
            default="info",
        )

    @property
    def rules(self) -> tuple[str, ...]:
        return tuple(sorted({finding.rule for finding in self.findings}))


@dataclass(frozen=True, slots=True)
class ReportTotals:
    files_scanned: int
    files_with_issues: int
    load_failures: int
    errors: int
    warnings: int
    info: int


@dataclass(frozen=True, slots=True)
class ReportFilters:
    text: str = ""
    severity: str = ""
    rule: str = ""
    check: str = ""
    load_failures: str = "all"


@dataclass(frozen=True, slots=True)
class ValidationReport:
    source_path: Path
    summary: Mapping[str, Any]
    issue_files: tuple[ValidationIssueFile, ...]
    totals: ReportTotals
    rules: tuple[str, ...]
    checks: tuple[str, ...]
    payload: Mapping[str, Any]

    @property
    def include_optional(self) -> bool:
        return self.summary.get("include_optional") is True

    @classmethod
    def load(cls, path: str | Path) -> "ValidationReport":
        source_path = Path(path)
        with source_path.open("r", encoding="utf-8") as stream:
            data = json.load(stream)
        return cls.from_dict(data, source_path=source_path)

    @classmethod
    def from_dict(
        cls,
        data: Mapping[str, Any],
        *,
        source_path: str | Path = "",
    ) -> "ValidationReport":
        if not isinstance(data, Mapping):
            raise ValueError("Validation report root must be a JSON object")

        summary = data.get("summary", {})
        if not isinstance(summary, Mapping):
            raise ValueError("Validation report 'summary' must be a JSON object")

        if "issues" in data:
            raw_files = data["issues"]
        elif "results" in data:
            raw_files = data["results"]
        else:
            raise ValueError(
                "Expected an issue-only 'issues' list or CLI 'results' list"
            )
        if not isinstance(raw_files, list):
            raise ValueError("Validation report file entries must be a JSON list")

        issue_files = tuple(
            issue_file
            for raw_file in raw_files
            if isinstance(raw_file, Mapping)
            for issue_file in (_parse_issue_file(raw_file),)
            if issue_file.findings or not issue_file.success
        )
        severities = _finding_severity_counts(issue_files)
        reported_issue_files = _summary_count(
            summary,
            "files_with_issues",
            "files_with_findings",
            fallback=len(issue_files),
        )
        totals = ReportTotals(
            files_scanned=_summary_count(
                summary, "files_scanned", "files", fallback=len(raw_files)
            ),
            files_with_issues=max(reported_issue_files, len(issue_files)),
            load_failures=_load_failure_count(summary, issue_files),
            errors=_summary_severity_count(summary, "error", severities),
            warnings=_summary_severity_count(summary, "warning", severities),
            info=_summary_severity_count(summary, "info", severities),
        )
        return cls(
            source_path=Path(source_path),
            summary=dict(summary),
            issue_files=issue_files,
            totals=totals,
            rules=tuple(
                sorted({f.rule for item in issue_files for f in item.findings})
            ),
            checks=tuple(
                sorted(
                    {f.check for item in issue_files for f in item.findings if f.check}
                )
            ),
            payload=data,
        )


@dataclass(frozen=True, slots=True)
class ValidationFixPlanItem:
    issue_file: ValidationIssueFile
    source: Path
    target: Path


@dataclass(frozen=True, slots=True)
class ValidationFixFileResult:
    source: Path
    target: Path
    success: bool
    changed: bool = False
    changes: int = 0
    error: str = ""


@dataclass(frozen=True, slots=True)
class ValidationFixBatchResult:
    files: tuple[ValidationFixFileResult, ...]

    @property
    def succeeded(self) -> int:
        return sum(file.success for file in self.files)

    @property
    def failed(self) -> int:
        return len(self.files) - self.succeeded

    @property
    def changed(self) -> int:
        return sum(file.success and file.changed for file in self.files)


ValidationFixer = Callable[[str, str | None, bool, bool], Mapping[str, Any]]
ValidationFixProgress = Callable[[int, int, ValidationFixFileResult], None]


def select_fix_issue_files(
    selected: ValidationIssueFile | None,
    filtered: Iterable[ValidationIssueFile],
    scope: str,
) -> tuple[ValidationIssueFile, ...]:
    if scope == "selected":
        return (selected,) if selected is not None else ()
    if scope not in {"filtered", "all"}:
        raise ValueError(f"Unknown fix scope: {scope}")

    result: list[ValidationIssueFile] = []
    seen: set[str] = set()
    for issue_file in filtered:
        key = issue_file.path.casefold()
        if key not in seen:
            seen.add(key)
            result.append(issue_file)
    return tuple(result)


def resolve_issue_source(
    report: ValidationReport,
    issue_file: ValidationIssueFile,
) -> Path:
    raw_path = Path(issue_file.path).expanduser()
    if raw_path.is_absolute():
        return raw_path.resolve(strict=False)

    relative_path = _safe_relative_path(raw_path)
    report_candidate = (report.source_path.parent / relative_path).resolve(
        strict=False
    )
    candidates = [report_candidate]
    for root in _report_roots(report):
        candidates.extend(
            (
                _join_with_path_overlap(root, relative_path),
                (root / relative_path).resolve(strict=False),
            )
        )
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return report_candidate


def build_validation_fix_plan(
    report: ValidationReport,
    issue_files: Iterable[ValidationIssueFile],
    *,
    in_place: bool,
    output_root: str | Path | None = None,
) -> tuple[ValidationFixPlanItem, ...]:
    destination = (
        Path(output_root).expanduser().resolve(strict=False)
        if output_root is not None and str(output_root).strip()
        else None
    )
    if not in_place and destination is None:
        raise ValueError("Select an output folder before starting repairs")

    result: list[ValidationFixPlanItem] = []
    seen: set[str] = set()
    for issue_file in issue_files:
        source = resolve_issue_source(report, issue_file)
        key = os.path.normcase(str(source))
        if key in seen:
            continue
        seen.add(key)

        if in_place:
            target = source
        else:
            assert destination is not None
            target = (destination / _issue_output_path(report, issue_file, source)).resolve(
                strict=False
            )
            if os.path.normcase(str(target)) == key:
                raise ValueError(
                    f"Output folder would overwrite the source file: {source}"
                )
        result.append(
            ValidationFixPlanItem(
                issue_file=issue_file,
                source=source,
                target=target,
            )
        )
    return tuple(result)


def run_validation_fix_plan(
    plan: Sequence[ValidationFixPlanItem],
    *,
    include_optional: bool = False,
    max_workers: int = 0,
    fixer: ValidationFixer | None = None,
    on_progress: ValidationFixProgress | None = None,
) -> ValidationFixBatchResult:
    if not plan:
        return ValidationFixBatchResult(())
    worker_count = bounded_fix_worker_count(max_workers, len(plan))
    fix_file = fixer or _native_validation_fixer

    def run_one(item: ValidationFixPlanItem) -> ValidationFixFileResult:
        try:
            if not item.source.is_file():
                raise FileNotFoundError(f"Source file not found: {item.source}")
            if item.target != item.source:
                item.target.parent.mkdir(parents=True, exist_ok=True)
            report = fix_file(
                str(item.source),
                str(item.target),
                True,
                include_optional,
            )
            changes = report.get("changes", [])
            change_count = len(changes) if isinstance(changes, list) else 0
            return ValidationFixFileResult(
                source=item.source,
                target=item.target,
                success=True,
                changed=bool(report.get("changed", change_count > 0)),
                changes=change_count,
            )
        except Exception as exc:
            return ValidationFixFileResult(
                source=item.source,
                target=item.target,
                success=False,
                error=str(exc),
            )

    results: list[ValidationFixFileResult] = []
    with ThreadPoolExecutor(
        max_workers=worker_count,
        thread_name_prefix="nif-validation-fix",
    ) as executor:
        futures = [executor.submit(run_one, item) for item in plan]
        for completed, future in enumerate(as_completed(futures), 1):
            result = future.result()
            results.append(result)
            if on_progress is not None:
                on_progress(completed, len(plan), result)
    results.sort(key=lambda result: os.path.normcase(str(result.source)))
    return ValidationFixBatchResult(tuple(results))


def bounded_fix_worker_count(requested: int, file_count: int) -> int:
    automatic = min(8, max(1, (os.cpu_count() or 1) - 1))
    return max(1, min(requested if requested > 0 else automatic, file_count, 16))


def _native_validation_fixer(
    path: str,
    output_path: str | None,
    fix: bool,
    include_optional: bool,
) -> Mapping[str, Any]:
    from creation_lib.nif import native_runtime

    return native_runtime.validate_nif_file_raw(
        path,
        output_path,
        fix,
        include_optional,
    )


def _report_roots(report: ValidationReport) -> tuple[Path, ...]:
    roots: list[Path] = []
    for key in ("scan_path", "path"):
        value = report.summary.get(key)
        if not isinstance(value, str) or not value.strip():
            continue
        root = Path(value).expanduser().resolve(strict=False)
        if root.suffix.lower() in {".nif", ".kf"}:
            root = root.parent
        if root not in roots:
            roots.append(root)
    return tuple(roots)


def _issue_output_path(
    report: ValidationReport,
    issue_file: ValidationIssueFile,
    source: Path,
) -> Path:
    raw_path = Path(issue_file.path).expanduser()
    if not raw_path.is_absolute():
        return _safe_relative_path(raw_path)

    for root in _report_roots(report):
        try:
            return source.relative_to(root)
        except ValueError:
            continue

    anchor_name = source.drive.rstrip(":\\/") or "root"
    relative_parts = source.parts[1:] if source.anchor else source.parts
    return _safe_relative_path(Path(anchor_name, *relative_parts))


def _safe_relative_path(path: Path) -> Path:
    if path.is_absolute() or any(part == ".." for part in path.parts):
        raise ValueError(f"Unsafe relative report path: {path}")
    parts = tuple(part for part in path.parts if part not in ("", "."))
    if not parts:
        raise ValueError("Validation report contains an empty file path")
    return Path(*parts)


def _join_with_path_overlap(root: Path, relative_path: Path) -> Path:
    root_parts = root.parts
    relative_parts = relative_path.parts
    overlap = 0
    for count in range(1, min(len(root_parts), len(relative_parts)) + 1):
        if tuple(part.casefold() for part in root_parts[-count:]) == tuple(
            part.casefold() for part in relative_parts[:count]
        ):
            overlap = count
    return root.joinpath(*relative_parts[overlap:]).resolve(strict=False)


def filter_issue_files(
    issue_files: Iterable[ValidationIssueFile],
    filters: ReportFilters,
) -> list[ValidationIssueFile]:
    text = filters.text.strip().casefold()
    severity = filters.severity.strip().lower()
    result = []

    for issue_file in issue_files:
        if filters.load_failures == "only" and issue_file.success:
            continue
        if filters.load_failures == "exclude" and not issue_file.success:
            continue
        if text and not _file_contains_text(issue_file, text):
            continue

        if severity or filters.rule or filters.check:
            finding_match = any(
                (not severity or finding.severity == severity)
                and (not filters.rule or finding.rule == filters.rule)
                and (not filters.check or finding.check == filters.check)
                for finding in issue_file.findings
            )
            failure_matches_error = (
                not issue_file.success
                and severity in ("", "error")
                and not filters.rule
                and not filters.check
            )
            if not finding_match and not failure_matches_error:
                continue

        result.append(issue_file)

    return result


def sort_issue_files(
    issue_files: Iterable[ValidationIssueFile],
    field: str,
    descending: bool = False,
) -> list[ValidationIssueFile]:
    key_functions = {
        "path": lambda item: item.path.casefold(),
        "severity": lambda item: (
            4 if not item.success else _SEVERITY_RANK.get(item.highest_severity, 0)
        ),
        "findings": lambda item: len(item.findings),
        "game": lambda item: item.game.casefold(),
        "rule": lambda item: ", ".join(item.rules).casefold(),
    }
    try:
        key_function = key_functions[field]
    except KeyError as exc:
        raise ValueError(f"Unknown report sort field: {field}") from exc
    return sorted(issue_files, key=key_function, reverse=descending)


def _parse_issue_file(value: Mapping[str, Any]) -> ValidationIssueFile:
    raw_findings = value.get("findings", [])
    findings = (
        [
            ValidationFinding.from_dict(finding)
            for finding in raw_findings
            if isinstance(finding, Mapping)
        ]
        if isinstance(raw_findings, list)
        else []
    )

    raw_warnings = value.get("warnings", [])
    if isinstance(raw_warnings, list):
        findings.extend(
            ValidationFinding(
                severity="warning",
                rule="runtime-warning",
                check="runtime-warning",
                message=str(warning),
            )
            for warning in raw_warnings
            if warning
        )

    load_error = str(value.get("error") or "")
    success = bool(value.get("success", not load_error))
    return ValidationIssueFile(
        path=str(value.get("path", "")),
        game=str(value.get("game", "unknown")),
        findings=tuple(findings),
        success=success,
        load_error=load_error,
    )


def _summary_count(
    summary: Mapping[str, Any],
    *names: str,
    fallback: int,
) -> int:
    for name in names:
        value = summary.get(name)
        if isinstance(value, int):
            return value
    return fallback


def _load_failure_count(
    summary: Mapping[str, Any],
    issue_files: tuple[ValidationIssueFile, ...],
) -> int:
    value = summary.get("load_failures")
    if isinstance(value, int):
        return value
    failures = summary.get("failures")
    if isinstance(failures, list):
        return len(failures)
    return sum(not issue_file.success for issue_file in issue_files)


def _finding_severity_counts(
    issue_files: tuple[ValidationIssueFile, ...],
) -> dict[str, int]:
    counts: dict[str, int] = {}
    for issue_file in issue_files:
        for finding in issue_file.findings:
            counts[finding.severity] = counts.get(finding.severity, 0) + 1
    return counts


def _summary_severity_count(
    summary: Mapping[str, Any],
    severity: str,
    fallback: Mapping[str, int],
) -> int:
    for key in ("findings_by_severity", "findings"):
        counts = summary.get(key)
        if isinstance(counts, Mapping) and isinstance(counts.get(severity), int):
            return counts[severity]
    return fallback.get(severity, 0)


def _file_contains_text(issue_file: ValidationIssueFile, text: str) -> bool:
    if text in issue_file.path.casefold() or text in issue_file.game.casefold():
        return True
    if text in issue_file.load_error.casefold():
        return True
    return any(
        text
        in " ".join(
            (
                finding.rule,
                finding.check,
                finding.block_type,
                finding.field,
                finding.message,
            )
        ).casefold()
        for finding in issue_file.findings
    )
