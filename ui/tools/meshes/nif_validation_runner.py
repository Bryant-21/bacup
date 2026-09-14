"""Background scan support for the standalone NIF validator tool."""

from __future__ import annotations

import json
import os
import threading
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterable, Mapping

from ui.tools.meshes.nif_validation_report import ValidationReport


@dataclass(frozen=True, slots=True)
class ValidationCheck:
    id: str
    title: str
    group: str
    extensions: tuple[str, ...]
    optional: bool


@dataclass(frozen=True, slots=True)
class ValidationScanResult:
    payload: Mapping[str, Any]
    report: ValidationReport
    cancelled: bool
    candidates: int
    processed: int


ValidationProgress = Callable[[int, int, Mapping[str, Any]], None]
ValidationDiscovered = Callable[[int, int], None]
ValidationCancelCheck = Callable[[], bool]
NifValidator = Callable[[str, str | None, bool, bool], Mapping[str, Any]]
DdsValidator = Callable[..., Mapping[str, Any]]


def load_validation_checks(
    features_loader: Callable[[], Mapping[str, Any]] | None = None,
) -> tuple[ValidationCheck, ...]:
    if features_loader is None:
        from creation_lib.nif import native_runtime

        features_loader = native_runtime.nif_features_raw
    raw_checks = features_loader().get("checks", [])
    checks = []
    for raw_check in raw_checks:
        if not isinstance(raw_check, Mapping) or not raw_check.get("id"):
            continue
        extensions = tuple(
            str(extension).lower().lstrip(".")
            for extension in raw_check.get("extensions", ())
            if extension
        )
        checks.append(
            ValidationCheck(
                id=str(raw_check["id"]),
                title=str(raw_check.get("title") or raw_check["id"]),
                group=str(raw_check.get("group") or "Other"),
                extensions=extensions,
                optional=bool(raw_check.get("optional", False)),
            )
        )
    return tuple(checks)


def default_validation_check_ids(
    checks: Iterable[ValidationCheck],
) -> frozenset[str]:
    return frozenset(check.id for check in checks if not check.optional)


def collect_validation_files(
    source: str | Path,
    checks: Iterable[ValidationCheck],
    selected_check_ids: Iterable[str],
    *,
    recursive: bool = True,
    path_contains: str = "",
) -> tuple[Path, ...]:
    source_path = Path(source).expanduser().resolve(strict=False)
    if not source_path.exists():
        raise FileNotFoundError(f"Input path not found: {source_path}")

    selected = set(selected_check_ids)
    extensions = {
        f".{extension}"
        for check in checks
        if check.id in selected
        for extension in check.extensions
    }
    if not selected:
        raise ValueError("Select at least one validation check")
    if not extensions:
        raise ValueError("The selected checks do not support any file types")

    needle = path_contains.strip().casefold()

    def accepted(path: Path, display_path: str) -> bool:
        return path.suffix.lower() in extensions and (
            not needle or needle in display_path.casefold()
        )

    if source_path.is_file():
        if not accepted(source_path, source_path.name):
            return ()
        return (source_path,)

    candidates = source_path.rglob("*") if recursive else source_path.glob("*")
    files = [
        path.resolve(strict=False)
        for path in candidates
        if path.is_file()
        and accepted(path, str(path.relative_to(source_path)))
    ]
    files.sort(key=lambda path: os.path.normcase(str(path)))
    return tuple(files)


def run_validation_scan(
    source: str | Path,
    checks: Iterable[ValidationCheck],
    selected_check_ids: Iterable[str],
    *,
    recursive: bool = True,
    path_contains: str = "",
    skip_errors: bool = True,
    max_workers: int = 0,
    cancel_requested: ValidationCancelCheck | None = None,
    on_discovered: ValidationDiscovered | None = None,
    on_progress: ValidationProgress | None = None,
    nif_validator: NifValidator | None = None,
    dds_validator: DdsValidator | None = None,
) -> ValidationScanResult:
    check_catalog = tuple(checks)
    checks_by_id = {check.id: check for check in check_catalog}
    selected = frozenset(selected_check_ids)
    unknown = sorted(selected - checks_by_id.keys())
    if unknown:
        raise ValueError(f"Unknown validation check(s): {', '.join(unknown)}")

    source_path = Path(source).expanduser().resolve(strict=False)
    files = collect_validation_files(
        source_path,
        check_catalog,
        selected,
        recursive=recursive,
        path_contains=path_contains,
    )
    if not files:
        raise ValueError(f"No files match the selected checks: {source_path}")

    worker_count = _bounded_worker_count(max_workers, len(files))
    include_optional = any(checks_by_id[check_id].optional for check_id in selected)
    if on_discovered is not None:
        on_discovered(len(files), worker_count)

    if nif_validator is None:
        from creation_lib.nif import native_runtime

        nif_validator = native_runtime.validate_nif_file_raw
    if dds_validator is None:
        from creation_lib.dds import native_runtime as dds_native_runtime

        dds_validator = dds_native_runtime.validate_dds_file_raw

    results: list[dict[str, Any]] = []
    state_lock = threading.Lock()
    next_index = 0
    completed = 0
    stopped_on_error = False

    def should_cancel() -> bool:
        return cancel_requested is not None and cancel_requested()

    def validate_one(file_path: Path) -> dict[str, Any]:
        try:
            if file_path.suffix.lower() == ".dds":
                report = dict(
                    dds_validator(str(file_path), include_optional=include_optional)
                )
            else:
                report = dict(
                    nif_validator(
                        str(file_path),
                        None,
                        False,
                        include_optional,
                    )
                )
            raw_findings = report.get("findings", [])
            report["findings"] = [
                finding
                for finding in raw_findings
                if isinstance(finding, Mapping)
                and finding.get("check", finding.get("rule")) in selected
            ]
            return {
                "path": str(file_path),
                "output": "",
                "success": True,
                **report,
            }
        except Exception as exc:
            return {
                "path": str(file_path),
                "output": "",
                "success": False,
                "error": str(exc),
                "changed": False,
                "changes": [],
                "warnings": [],
                "findings": [],
            }

    def worker() -> None:
        nonlocal next_index, completed, stopped_on_error
        while True:
            with state_lock:
                if should_cancel() or stopped_on_error or next_index >= len(files):
                    return
                file_path = files[next_index]
                next_index += 1
            result = validate_one(file_path)
            with state_lock:
                results.append(result)
                completed += 1
                current = completed
                if not result["success"] and not skip_errors:
                    stopped_on_error = True
            if on_progress is not None:
                on_progress(current, len(files), result)

    with ThreadPoolExecutor(
        max_workers=worker_count,
        thread_name_prefix="nif-validation-scan",
    ) as executor:
        futures = [executor.submit(worker) for _ in range(worker_count)]
        for future in futures:
            future.result()

    results.sort(key=lambda item: os.path.normcase(str(item["path"])))
    payload = _build_scan_payload(
        source_path,
        results,
        candidate_count=len(files),
        jobs=worker_count,
        include_optional=include_optional,
        selected_checks=selected,
        cancelled=should_cancel(),
        stopped_on_error=stopped_on_error,
        recursive=recursive,
        path_contains=path_contains,
    )
    marker = (
        source_path.parent / "NIF_VALIDATION_REPORT.json"
        if source_path.is_file()
        else source_path / "NIF_VALIDATION_REPORT.json"
    )
    report = ValidationReport.from_dict(payload, source_path=marker)
    return ValidationScanResult(
        payload=payload,
        report=report,
        cancelled=should_cancel(),
        candidates=len(files),
        processed=len(results),
    )


def write_validation_payload(payload: Mapping[str, Any], path: str | Path) -> Path:
    destination = Path(path).expanduser().resolve(strict=False)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return destination


def _bounded_worker_count(requested: int, file_count: int) -> int:
    automatic = min(8, max(1, (os.cpu_count() or 1) - 1))
    return max(1, min(requested if requested > 0 else automatic, file_count, 16))


def _build_scan_payload(
    source: Path,
    results: list[dict[str, Any]],
    *,
    candidate_count: int,
    jobs: int,
    include_optional: bool,
    selected_checks: frozenset[str],
    cancelled: bool,
    stopped_on_error: bool,
    recursive: bool,
    path_contains: str,
) -> dict[str, Any]:
    severities = Counter()
    rules = Counter()
    games = Counter()
    for result in results:
        if result["success"]:
            games[result.get("game", "unknown")] += 1
        for finding in result.get("findings", []):
            severities[finding.get("severity", "warning")] += 1
            rules[finding.get("rule", "unknown")] += 1

    summary = {
        "path": str(source),
        "files": len(results),
        "candidate_files": candidate_count,
        "valid": sum(
            result["success"]
            and not any(
                finding.get("severity") == "error"
                for finding in result.get("findings", [])
            )
            for result in results
        ),
        "files_with_findings": sum(bool(result.get("findings")) for result in results),
        "fixed_files": 0,
        "changes": 0,
        "games": dict(sorted(games.items())),
        "findings": dict(sorted(severities.items())),
        "rules": dict(sorted(rules.items())),
        "failures": [
            {"path": result["path"], "error": result.get("error", "")}
            for result in results
            if not result["success"]
        ],
        "jobs": jobs,
        "include_optional": include_optional,
        "checks": sorted(selected_checks),
        "cancelled": cancelled,
        "stopped_on_error": stopped_on_error,
        "recursive": recursive,
        "path_contains": path_contains,
    }
    return {"summary": summary, "results": results}
