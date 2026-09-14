"""Standalone toolkit UI for selecting, validating, reviewing, and fixing NIFs."""

from __future__ import annotations

import logging
import queue
import threading
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path
from typing import Mapping

from imgui_bundle import imgui

from creation_lib.ui.widgets.modern import expandable_section

from ui.tools.base import BaseTool
from ui.tools.meshes.nif_validation_report import (
    ReportFilters,
    ValidationFixBatchResult,
    ValidationFixFileResult,
    ValidationIssueFile,
    ValidationReport,
    bounded_fix_worker_count,
    build_validation_fix_plan,
    filter_issue_files,
    run_validation_fix_plan,
    select_fix_issue_files,
    sort_issue_files,
)
from ui.tools.meshes.nif_validation_runner import (
    ValidationCheck,
    ValidationScanResult,
    default_validation_check_ids,
    load_validation_checks,
    run_validation_scan,
    write_validation_payload,
)


_log = logging.getLogger("tools.nif_validation_report")

_SEVERITY_COLORS = {
    "error": imgui.ImVec4(0.95, 0.35, 0.35, 1.0),
    "warning": imgui.ImVec4(0.95, 0.72, 0.25, 1.0),
    "info": imgui.ImVec4(0.45, 0.72, 0.95, 1.0),
}

_SORT_FIELDS = {
    1: "severity",
    2: "findings",
    3: "game",
    4: "rule",
    5: "path",
}


class NifValidationReportTool(BaseTool):
    """Run native validation and explicitly repair selected NIF files."""

    name = "NIF Validator"
    tool_id = "nif_validation_report"
    description = "Select, validate, review, and repair NIF, KF, and DDS assets"
    category = "NIF"

    def __init__(self):
        super().__init__()
        self._toolkit_settings = None
        self.report: ValidationReport | None = None
        self.report_payload: Mapping | None = None
        self.report_path = ""
        self.input_path = ""
        self.include_subdirectories = True
        self.path_contains = ""
        self.skip_errors = True
        self.validation_jobs = bounded_fix_worker_count(0, 16)
        self.check_catalog: tuple[ValidationCheck, ...] = ()
        self.enabled_check_ids: set[str] = set()
        self._saved_check_ids: set[str] | None = None
        self.check_list_filter = ""
        self.error = ""
        self.text_filter = ""
        self.severity_filter = ""
        self.rule_filter = ""
        self.check_filter = ""
        self.failure_filter = "all"
        self.sort_field = "severity"
        self.sort_descending = True
        self.filtered_files: list[ValidationIssueFile] = []
        self.selected_path = ""
        self._selected_issue: ValidationIssueFile | None = None
        self.fix_mode = "output"
        self.fix_output_folder = ""
        self.fix_jobs = bounded_fix_worker_count(0, 16)
        self.fix_status = ""
        self.fix_error = ""
        self.fix_completed = 0
        self.fix_total = 0
        self.fix_changed = 0
        self.fix_changes = 0
        self.fix_failed = 0
        self.fix_file_errors: list[ValidationFixFileResult] = []
        self._pending_in_place: tuple[ValidationIssueFile, ...] = ()
        self._open_in_place_confirmation = False
        self._fix_events: queue.SimpleQueue[tuple] = queue.SimpleQueue()
        self._fix_executor: ThreadPoolExecutor | None = None
        self._fix_future: Future[ValidationFixBatchResult] | None = None
        self._fix_skipped_audit_only = 0
        self.scan_status = ""
        self.scan_completed = 0
        self.scan_total = 0
        self.scan_failures = 0
        self.scan_last_path = ""
        self._scan_cancel = threading.Event()
        self._scan_events: queue.SimpleQueue[tuple] = queue.SimpleQueue()
        self._scan_executor: ThreadPoolExecutor | None = None
        self._scan_future: Future[ValidationScanResult] | None = None

    def restore_path(self, path: str) -> None:
        if path and Path(path).is_file():
            self.load_report(path)

    def load_report(self, path: str | Path) -> bool:
        if self._scan_future is not None or self._fix_future is not None:
            self.error = "Wait for the current validation or repair batch"
            return False
        try:
            report = ValidationReport.load(path)
        except Exception as exc:
            self.error = f"Could not load validation report: {exc}"
            _log.exception("Failed to load NIF validation report %s", path)
            return False

        self.report = report
        self.report_payload = report.payload
        self.report_path = str(report.source_path.resolve())
        report_input = report.summary.get("path") or report.summary.get("scan_path")
        if isinstance(report_input, str) and report_input.strip():
            self.input_path = report_input
        report_checks = report.summary.get("checks")
        if isinstance(report_checks, list) and self.check_catalog:
            known = {check.id for check in self.check_catalog}
            selected = {str(check) for check in report_checks} & known
            if selected:
                self.enabled_check_ids = selected
        if not self.fix_output_folder.strip():
            self.fix_output_folder = str(
                report.source_path.parent / "nif_validation_fixed"
            )
        self.error = ""
        self.selected_path = ""
        self._selected_issue = None
        self._requery()
        return True

    def open_report_dialog(self) -> None:
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file

            path = pick_file(
                "Open NIF Validation Report",
                [
                    ("JSON reports", "*.json"),
                    ("All files", "*.*"),
                ],
                default_path=self._default_dialog_path(),
            )
            if path:
                self.load_report(path)
        except Exception as exc:
            self.error = f"Could not open report picker: {exc}"
            _log.exception("NIF validation report picker failed")

    def select_input_file_dialog(self) -> None:
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file

            path = pick_file(
                "Select NIF, KF, or DDS file",
                [
                    ("NIF assets", "*.nif *.kf *.dds"),
                    ("All files", "*.*"),
                ],
                default_path=self._input_dialog_path(),
            )
            if path:
                self.input_path = path
                self.error = ""
        except Exception as exc:
            self.error = f"Could not open input file picker: {exc}"
            _log.exception("NIF validation input file picker failed")

    def select_input_folder_dialog(self) -> None:
        try:
            from creation_lib.ui.widgets.pick_folder import pick_folder

            path = pick_folder(
                "Select folder containing NIF assets",
                self._input_dialog_path(),
            )
            if path:
                self.input_path = path
                self.error = ""
        except Exception as exc:
            self.error = f"Could not open input folder picker: {exc}"
            _log.exception("NIF validation input folder picker failed")

    def save_report_dialog(self) -> None:
        if self.report_payload is None:
            self.error = "Run validation or open a report before saving"
            return
        try:
            from creation_lib.ui.widgets.pick_folder import pick_save_file

            path = pick_save_file(
                "Save NIF validation report",
                [("JSON reports", "*.json"), ("All files", "*.*")],
                default_ext=".json",
                initialfile="NIF_VALIDATION_REPORT.json",
                default_path=self._report_dialog_path(),
            )
            if path:
                destination = write_validation_payload(self.report_payload, path)
                self.report_path = str(destination)
                self.error = ""
                self.scan_status = f"Saved report: {destination}"
        except Exception as exc:
            self.error = f"Could not save validation report: {exc}"
            _log.exception("NIF validation report save failed")

    def initialize(self) -> None:
        super().initialize()
        try:
            self.check_catalog = load_validation_checks()
        except Exception as exc:
            self.error = f"Could not load native validation checks: {exc}"
            _log.exception("Failed to load native NIF validation check catalog")
        else:
            known = {check.id for check in self.check_catalog}
            if self._saved_check_ids is None:
                self.enabled_check_ids = set(
                    default_validation_check_ids(self.check_catalog)
                )
            else:
                self.enabled_check_ids = self._saved_check_ids & known
        self.restore_path(self.report_path)

    def draw_content(self) -> None:
        self._poll_scan()
        self._poll_fix()

        self._draw_input_controls()
        self._draw_check_controls()
        self._draw_run_controls()
        self._draw_report_controls()

        if self.error:
            imgui.push_style_color(imgui.Col_.text, _SEVERITY_COLORS["error"])
            imgui.text_wrapped(self.error)
            imgui.pop_style_color()

        if self.report is None:
            imgui.spacing()
            imgui.text_disabled(
                "Select an input file or folder, choose checks, then run validation."
            )
            return

        self._draw_summary()
        self._draw_filters()
        self._draw_fix_controls()
        self._draw_issue_table()
        self._draw_selected_details()
        self._draw_in_place_confirmation()

    def _draw_input_controls(self) -> None:
        imgui.separator_text("Input")
        busy = self._scan_future is not None or self._fix_future is not None
        if busy:
            imgui.begin_disabled()
        button_width = 105.0
        imgui.set_next_item_width(
            max(180.0, imgui.get_content_region_avail().x - button_width * 2 - 16.0)
        )
        changed, input_path = imgui.input_text_with_hint(
            "##nif_validation_input",
            "Select one NIF/KF/DDS file or a folder...",
            self.input_path,
        )
        if changed:
            self.input_path = input_path
        imgui.same_line()
        if imgui.button("Select File...", imgui.ImVec2(button_width, 0)):
            self.select_input_file_dialog()
        imgui.same_line()
        if imgui.button("Select Folder...", imgui.ImVec2(button_width, 0)):
            self.select_input_folder_dialog()

        _, self.include_subdirectories = imgui.checkbox(
            "Include subdirectories", self.include_subdirectories
        )
        imgui.same_line()
        _, self.skip_errors = imgui.checkbox(
            "Continue after load errors", self.skip_errors
        )
        imgui.same_line()
        imgui.text_disabled("Game rules are detected from each file header")

        imgui.set_next_item_width(-1)
        changed, path_contains = imgui.input_text_with_hint(
            "##nif_validation_path_contains",
            "Optional path filter (folder or file name contains)...",
            self.path_contains,
        )
        if changed:
            self.path_contains = path_contains
        if busy:
            imgui.end_disabled()

    def _draw_check_controls(self) -> None:
        selected_count = len(self.enabled_check_ids)
        header = f"Validation checks ({selected_count}/{len(self.check_catalog)} selected)"
        with expandable_section(
            header,
            imgui.TreeNodeFlags_.default_open.value,
        ) as expanded:
            if not expanded:
                return

            busy = self._scan_future is not None or self._fix_future is not None
            if busy:
                imgui.begin_disabled()
            imgui.set_next_item_width(max(180.0, imgui.get_content_region_avail().x - 330.0))
            changed, check_filter = imgui.input_text_with_hint(
                "##nif_validation_check_filter",
                "Filter checks...",
                self.check_list_filter,
            )
            if changed:
                self.check_list_filter = check_filter
            imgui.same_line()
            if imgui.small_button("All"):
                self.enabled_check_ids = {check.id for check in self.check_catalog}
            imgui.same_line()
            if imgui.small_button("Recommended"):
                self.enabled_check_ids = set(
                    default_validation_check_ids(self.check_catalog)
                )
            imgui.same_line()
            if imgui.small_button("None"):
                self.enabled_check_ids.clear()

            if imgui.begin_child(
                "##nif_validation_checks",
                imgui.ImVec2(0, 170.0),
                imgui.ChildFlags_.borders.value,
            ):
                needle = self.check_list_filter.strip().casefold()
                current_group = ""
                for check in self.check_catalog:
                    searchable = " ".join(
                        (check.title, check.id, check.group, *check.extensions)
                    ).casefold()
                    if needle and needle not in searchable:
                        continue
                    if check.group != current_group:
                        current_group = check.group
                        imgui.separator_text(current_group)
                    checked = check.id in self.enabled_check_ids
                    changed, checked = imgui.checkbox(
                        f"{check.title}##validation_check_{check.id}", checked
                    )
                    if changed:
                        if checked:
                            self.enabled_check_ids.add(check.id)
                        else:
                            self.enabled_check_ids.discard(check.id)
                    imgui.same_line()
                    extensions = ", ".join(f".{ext}" for ext in check.extensions)
                    optional = " | optional" if check.optional else ""
                    imgui.text_disabled(f"{extensions}{optional} | {check.id}")
            imgui.end_child()
            if busy:
                imgui.end_disabled()

    def _draw_run_controls(self) -> None:
        imgui.set_next_item_width(230.0)
        if self._scan_future is not None or self._fix_future is not None:
            imgui.begin_disabled()
        _, self.validation_jobs = imgui.slider_int(
            "Parallel workers##nif_validation_jobs",
            self.validation_jobs,
            1,
            16,
        )
        if self._scan_future is not None or self._fix_future is not None:
            imgui.end_disabled()

        imgui.same_line()
        can_run = bool(self.input_path.strip() and self.enabled_check_ids)
        if not can_run or self._scan_future is not None or self._fix_future is not None:
            imgui.begin_disabled()
        if imgui.button("Run Validation", imgui.ImVec2(150.0, 0)):
            self._start_scan()
        if not can_run or self._scan_future is not None or self._fix_future is not None:
            imgui.end_disabled()

        if self._scan_future is not None:
            imgui.same_line()
            if imgui.button("Cancel Validation"):
                self._scan_cancel.set()
            fraction = self.scan_completed / self.scan_total if self.scan_total else 0.0
            overlay = (
                f"{self.scan_completed:,} / {self.scan_total:,}"
                if self.scan_total
                else "Collecting files..."
            )
            imgui.progress_bar(fraction, imgui.ImVec2(-1, 0), overlay)
        if self.scan_status:
            imgui.text_wrapped(self.scan_status)

    def _draw_report_controls(self) -> None:
        imgui.separator_text("Results")
        busy = self._scan_future is not None or self._fix_future is not None
        if busy:
            imgui.begin_disabled()
        if imgui.button("Open Report..."):
            self.open_report_dialog()
        imgui.same_line()
        if self.report_payload is None:
            imgui.begin_disabled()
        if imgui.button("Save Report..."):
            self.save_report_dialog()
        if self.report_payload is None:
            imgui.end_disabled()
        imgui.same_line()
        can_reload = bool(self.report_path and Path(self.report_path).is_file())
        if not can_reload:
            imgui.begin_disabled()
        if imgui.button("Reload Report") and can_reload:
            self.load_report(self.report_path)
        if not can_reload:
            imgui.end_disabled()
        if busy:
            imgui.end_disabled()

        if self.report_path:
            imgui.same_line()
            imgui.text_disabled(self.report_path)
            if imgui.is_item_hovered():
                imgui.set_tooltip(self.report_path)

    def _start_scan(self) -> None:
        if self._scan_future is not None or self._fix_future is not None:
            return
        if not self.input_path.strip():
            self.error = "Select an input file or folder"
            return
        if not self.enabled_check_ids:
            self.error = "Select at least one validation check"
            return

        self.error = ""
        self.report = None
        self.report_payload = None
        self.report_path = ""
        self.filtered_files = []
        self.selected_path = ""
        self._selected_issue = None
        self.scan_status = "Collecting matching files..."
        self.scan_completed = 0
        self.scan_total = 0
        self.scan_failures = 0
        self.scan_last_path = ""
        self._scan_cancel.clear()
        self._scan_executor = ThreadPoolExecutor(
            max_workers=1,
            thread_name_prefix="nif-validation-controller",
        )
        self._scan_future = self._scan_executor.submit(
            self._execute_scan,
            self.input_path,
            tuple(self.check_catalog),
            frozenset(self.enabled_check_ids),
            self.include_subdirectories,
            self.path_contains,
            self.skip_errors,
            self.validation_jobs,
        )

    def _execute_scan(
        self,
        input_path: str,
        checks: tuple[ValidationCheck, ...],
        selected_checks: frozenset[str],
        recursive: bool,
        path_contains: str,
        skip_errors: bool,
        jobs: int,
    ) -> ValidationScanResult:
        return run_validation_scan(
            input_path,
            checks,
            selected_checks,
            recursive=recursive,
            path_contains=path_contains,
            skip_errors=skip_errors,
            max_workers=jobs,
            cancel_requested=self._scan_cancel.is_set,
            on_discovered=lambda total, workers: self._scan_events.put(
                ("discovered", total, workers)
            ),
            on_progress=lambda completed, total, result: self._scan_events.put(
                ("progress", completed, total, result)
            ),
        )

    def _poll_scan(self) -> None:
        while True:
            try:
                event = self._scan_events.get_nowait()
            except queue.Empty:
                break
            if event[0] == "discovered":
                _, total, workers = event
                self.scan_total = total
                self.scan_status = (
                    f"Validating {total:,} file(s) with {workers} worker(s)..."
                )
                continue
            _, completed, total, result = event
            self.scan_completed = max(self.scan_completed, completed)
            self.scan_total = total
            self.scan_last_path = str(result.get("path", ""))
            self.scan_failures += int(not result.get("success", False))
            self.scan_status = (
                f"Validated {completed:,}/{total:,}; "
                f"load failures: {self.scan_failures:,}; {self.scan_last_path}"
            )

        if self._scan_future is None or not self._scan_future.done():
            return
        try:
            result = self._scan_future.result()
        except Exception as exc:
            self.error = f"Validation failed: {exc}"
            self.scan_status = "Validation did not produce results."
            _log.exception("NIF validation scan failed")
        else:
            self.report = result.report
            self.report_payload = result.payload
            self.scan_completed = result.processed
            self.scan_total = result.candidates
            self.report_path = ""
            if not self.fix_output_folder.strip():
                source = Path(self.input_path).expanduser().resolve(strict=False)
                self.fix_output_folder = str(
                    source.parent / "nif_validation_fixed"
                    if source.is_file()
                    else source.parent / f"{source.name}_fixed"
                )
            self._requery()
            status = "Cancelled" if result.cancelled else "Finished"
            if result.payload["summary"].get("stopped_on_error"):
                status = "Stopped after load error"
            self.scan_status = (
                f"{status}: {result.processed:,}/{result.candidates:,} file(s), "
                f"{result.report.totals.files_with_issues:,} issue file(s), "
                f"{result.report.totals.load_failures:,} load failure(s)."
            )
        finally:
            self._scan_future = None
            if self._scan_executor is not None:
                self._scan_executor.shutdown(wait=False)
                self._scan_executor = None

    def _draw_summary(self) -> None:
        assert self.report is not None
        totals = self.report.totals
        metrics = (
            ("Scanned", totals.files_scanned, None),
            ("Issue files", totals.files_with_issues, None),
            ("Load failures", totals.load_failures, _SEVERITY_COLORS["error"]),
            ("Errors", totals.errors, _SEVERITY_COLORS["error"]),
            ("Warnings", totals.warnings, _SEVERITY_COLORS["warning"]),
            ("Info", totals.info, _SEVERITY_COLORS["info"]),
        )
        flags = (
            imgui.TableFlags_.sizing_stretch_same | imgui.TableFlags_.borders_inner_v
        )
        if imgui.begin_table("##report_summary", len(metrics), flags):
            for label, value, color in metrics:
                imgui.table_next_column()
                imgui.text_disabled(label)
                if color is None:
                    imgui.text(f"{value:,}")
                else:
                    imgui.text_colored(color, f"{value:,}")
            imgui.end_table()
        imgui.separator()

    def _draw_filters(self) -> None:
        assert self.report is not None
        changed = False

        imgui.set_next_item_width(-1)
        text_changed, new_text = imgui.input_text_with_hint(
            "##report_text_filter",
            "Filter path, rule, check, block type, or message...",
            self.text_filter,
        )
        if text_changed:
            self.text_filter = new_text
            changed = True

        combo_width = max(135.0, imgui.get_content_region_avail().x / 5.0 - 12.0)
        severity_changed, severity = self._draw_combo(
            "Severity##report_severity",
            self.severity_filter,
            ("error", "warning", "info"),
            "All severities",
            combo_width,
        )
        changed |= severity_changed
        self.severity_filter = severity

        imgui.same_line()
        rule_changed, rule = self._draw_combo(
            "Rule##report_rule",
            self.rule_filter,
            self.report.rules,
            "All rules",
            combo_width,
        )
        changed |= rule_changed
        self.rule_filter = rule

        imgui.same_line()
        check_changed, check = self._draw_combo(
            "Check##report_check",
            self.check_filter,
            self.report.checks,
            "All checks",
            combo_width,
        )
        changed |= check_changed
        self.check_filter = check

        imgui.same_line()
        failure_changed, failure = self._draw_combo(
            "Loads##report_loads",
            self.failure_filter,
            ("all", "only", "exclude"),
            "All files",
            combo_width,
            labels={
                "all": "All files",
                "only": "Load failures",
                "exclude": "Loaded files",
            },
            include_empty=False,
        )
        changed |= failure_changed
        self.failure_filter = failure

        imgui.same_line()
        if imgui.button("Clear"):
            self.text_filter = ""
            self.severity_filter = ""
            self.rule_filter = ""
            self.check_filter = ""
            self.failure_filter = "all"
            changed = True

        if changed:
            self._requery()

        imgui.text_disabled(
            f"Showing {len(self.filtered_files):,} of {len(self.report.issue_files):,} issue files"
        )

    @staticmethod
    def _draw_combo(
        label: str,
        current: str,
        options,
        empty_label: str,
        width: float,
        *,
        labels: dict[str, str] | None = None,
        include_empty: bool = True,
    ) -> tuple[bool, str]:
        labels = labels or {}
        preview = labels.get(current, current) if current else empty_label
        changed = False
        imgui.set_next_item_width(width)
        if imgui.begin_combo(label, preview):
            if include_empty:
                selected, _ = imgui.selectable(empty_label, not current)
                if selected:
                    current = ""
                    changed = True
            for option in options:
                selected, _ = imgui.selectable(
                    labels.get(option, option), current == option
                )
                if selected:
                    current = option
                    changed = True
            imgui.end_combo()
        return changed, current

    def _draw_fix_controls(self) -> None:
        assert self.report is not None
        imgui.separator_text("Repair")
        imgui.text_disabled(
            "Repair applies all safe native fixes for the chosen NIF/KF files; DDS checks are audit-only."
        )
        running = self._fix_future is not None or self._scan_future is not None
        if running:
            imgui.begin_disabled()

        if imgui.radio_button("Write to output folder", self.fix_mode == "output"):
            self.fix_mode = "output"
        imgui.same_line()
        if imgui.radio_button("Replace originals", self.fix_mode == "in_place"):
            self.fix_mode = "in_place"

        if self.fix_mode == "output":
            browse_width = 95.0
            imgui.set_next_item_width(
                max(180.0, imgui.get_content_region_avail().x - browse_width - 8.0)
            )
            changed, output_folder = imgui.input_text_with_hint(
                "##validation_fix_output",
                "Select an empty or separate output folder...",
                self.fix_output_folder,
            )
            if changed:
                self.fix_output_folder = output_folder
            imgui.same_line()
            if imgui.button("Browse...##validation_fix_output"):
                self._pick_fix_output_folder()
        else:
            imgui.text_colored(
                _SEVERITY_COLORS["warning"],
                "Original files will be overwritten only after confirmation.",
            )

        imgui.set_next_item_width(220.0)
        _, self.fix_jobs = imgui.slider_int(
            "Parallel repair workers##validation_fix_jobs",
            self.fix_jobs,
            1,
            16,
        )
        if "include_optional" in self.report.summary:
            state = "enabled" if self.report.include_optional else "disabled"
            imgui.same_line()
            imgui.text_disabled(f"Optional checks from report: {state}")

        selected = self._selected_file()
        if selected is None:
            imgui.begin_disabled()
        if imgui.button("Fix Selected File"):
            self._request_fix("selected")
        if selected is None:
            imgui.end_disabled()

        imgui.same_line()
        if not self.filtered_files:
            imgui.begin_disabled()
        if imgui.button(f"Fix Filtered Files ({len(self.filtered_files):,})"):
            self._request_fix("filtered")
        if not self.filtered_files:
            imgui.end_disabled()

        imgui.same_line()
        if not self.report.issue_files:
            imgui.begin_disabled()
        if imgui.button(f"Fix All Issue Files ({len(self.report.issue_files):,})"):
            self._request_fix("all")
        if not self.report.issue_files:
            imgui.end_disabled()

        if running:
            imgui.end_disabled()

        if running:
            fraction = self.fix_completed / self.fix_total if self.fix_total else 0.0
            overlay = f"{self.fix_completed:,} / {self.fix_total:,}"
            imgui.progress_bar(fraction, imgui.ImVec2(-1, 0), overlay)
        if self.fix_status:
            imgui.text_wrapped(self.fix_status)
        if self.fix_error:
            imgui.text_colored(_SEVERITY_COLORS["error"], self.fix_error)
        if self.fix_file_errors:
            if imgui.tree_node(
                f"File errors ({len(self.fix_file_errors):,})##validation_fix_errors"
            ):
                flags = (
                    imgui.TableFlags_.borders_inner_h
                    | imgui.TableFlags_.row_bg
                    | imgui.TableFlags_.scroll_y
                    | imgui.TableFlags_.resizable
                )
                if imgui.begin_table(
                    "##validation_fix_file_errors",
                    2,
                    flags,
                    imgui.ImVec2(0, 130.0),
                ):
                    imgui.table_setup_column(
                        "File", imgui.TableColumnFlags_.width_stretch, 0.55
                    )
                    imgui.table_setup_column(
                        "Error", imgui.TableColumnFlags_.width_stretch, 0.45
                    )
                    imgui.table_headers_row()
                    clipper = imgui.ListClipper()
                    clipper.begin(len(self.fix_file_errors))
                    while clipper.step():
                        for row_index in range(
                            clipper.display_start,
                            clipper.display_end,
                        ):
                            result = self.fix_file_errors[row_index]
                            imgui.table_next_row()
                            imgui.table_next_column()
                            imgui.text_unformatted(str(result.source))
                            imgui.table_next_column()
                            imgui.text_wrapped(result.error)
                    clipper.end()
                    imgui.end_table()
                imgui.tree_pop()

    def _pick_fix_output_folder(self) -> None:
        try:
            from creation_lib.ui.widgets.pick_folder import pick_folder

            path = pick_folder(
                "Select NIF repair output folder",
                self.fix_output_folder or self._default_dialog_path(),
            )
            if path:
                self.fix_output_folder = path
                self.fix_error = ""
        except Exception as exc:
            self.fix_error = f"Could not open output folder picker: {exc}"
            _log.exception("NIF validation repair output picker failed")

    def _request_fix(self, scope: str) -> None:
        assert self.report is not None
        candidates = (
            self.report.issue_files if scope == "all" else self.filtered_files
        )
        issue_files = select_fix_issue_files(
            self._selected_file(),
            candidates,
            scope,
        )
        repairable = tuple(
            issue_file
            for issue_file in issue_files
            if Path(issue_file.path).suffix.lower() in {".nif", ".kf"}
        )
        self._fix_skipped_audit_only = len(issue_files) - len(repairable)
        issue_files = repairable
        if not issue_files:
            self.fix_error = "No repairable NIF or KF issue files are selected"
            return
        if self.fix_mode == "in_place":
            self._pending_in_place = issue_files
            self._open_in_place_confirmation = True
            return
        if not self.fix_output_folder.strip():
            self.fix_error = "Select an output folder before starting repairs"
            return
        self._start_fix(issue_files)

    def _draw_in_place_confirmation(self) -> None:
        if self._open_in_place_confirmation:
            imgui.open_popup("Confirm In-Place NIF Repairs##validation_report")
            self._open_in_place_confirmation = False

        opened, _ = imgui.begin_popup_modal(
            "Confirm In-Place NIF Repairs##validation_report",
            True,
            imgui.WindowFlags_.always_auto_resize.value,
        )
        if not opened:
            return

        imgui.text_wrapped(
            f"Replace {len(self._pending_in_place):,} original NIF file(s)?"
        )
        imgui.text_colored(
            _SEVERITY_COLORS["warning"],
            "This writes fixes directly into the source files and cannot be undone here.",
        )
        if imgui.button("Replace Originals"):
            issue_files = self._pending_in_place
            self._pending_in_place = ()
            imgui.close_current_popup()
            self._start_fix(issue_files)
        imgui.same_line()
        if imgui.button("Cancel"):
            self._pending_in_place = ()
            imgui.close_current_popup()
        imgui.end_popup()

    def _start_fix(self, issue_files: tuple[ValidationIssueFile, ...]) -> None:
        if (
            self.report is None
            or self._scan_future is not None
            or self._fix_future is not None
        ):
            return
        report = self.report
        in_place = self.fix_mode == "in_place"
        output_folder = None if in_place else self.fix_output_folder
        self.fix_status = "Planning repairs..."
        self.fix_error = ""
        self.fix_completed = 0
        self.fix_total = len(issue_files)
        self.fix_changed = 0
        self.fix_changes = 0
        self.fix_failed = 0
        self.fix_file_errors = []
        self._fix_executor = ThreadPoolExecutor(
            max_workers=1,
            thread_name_prefix="nif-validation-batch",
        )
        self._fix_future = self._fix_executor.submit(
            self._execute_fix,
            report,
            issue_files,
            in_place,
            output_folder,
            self.fix_jobs,
        )

    def _execute_fix(
        self,
        report: ValidationReport,
        issue_files: tuple[ValidationIssueFile, ...],
        in_place: bool,
        output_folder: str | None,
        jobs: int,
    ) -> ValidationFixBatchResult:
        plan = build_validation_fix_plan(
            report,
            issue_files,
            in_place=in_place,
            output_root=output_folder,
        )
        self._fix_events.put(("planned", len(plan)))
        return run_validation_fix_plan(
            plan,
            include_optional=report.include_optional,
            max_workers=jobs,
            on_progress=lambda completed, total, result: self._fix_events.put(
                ("progress", completed, total, result)
            ),
        )

    def _poll_fix(self) -> None:
        while True:
            try:
                event = self._fix_events.get_nowait()
            except queue.Empty:
                break
            if event[0] == "planned":
                self.fix_total = event[1]
                self.fix_status = f"Repairing {self.fix_total:,} file(s)..."
                continue
            _, completed, total, result = event
            self.fix_completed = completed
            self.fix_total = total
            self.fix_changed += int(result.changed)
            self.fix_changes += result.changes
            if not result.success:
                self.fix_failed += 1
                self.fix_file_errors.append(result)

        if self._fix_future is None or not self._fix_future.done():
            return
        try:
            result = self._fix_future.result()
        except Exception as exc:
            self.fix_error = f"Repair batch failed during planning: {exc}"
            self.fix_status = "No report data was changed."
        else:
            self.fix_completed = len(result.files)
            self.fix_total = len(result.files)
            self.fix_status = (
                f"Finished: {result.changed:,} file(s) changed, "
                f"{result.succeeded - result.changed:,} unchanged, "
                f"{result.failed:,} failed, {self.fix_changes:,} total changes. "
                f"{self._fix_skipped_audit_only:,} audit-only file(s) skipped. "
                "The JSON report was not modified."
            )
        finally:
            self._fix_future = None
            if self._fix_executor is not None:
                self._fix_executor.shutdown(wait=False)
                self._fix_executor = None

    def cleanup(self) -> None:
        self._scan_cancel.set()
        if self._scan_executor is not None:
            self._scan_executor.shutdown(wait=False, cancel_futures=True)
            self._scan_executor = None
        if self._fix_executor is not None:
            self._fix_executor.shutdown(wait=False, cancel_futures=True)
            self._fix_executor = None
        super().cleanup()

    def get_default_settings(self) -> dict:
        return {
            "input_path": "",
            "include_subdirectories": True,
            "path_contains": "",
            "skip_errors": True,
            "validation_jobs": bounded_fix_worker_count(0, 16),
            "report_path": "",
            "fix_mode": "output",
            "fix_output_folder": "",
            "fix_jobs": bounded_fix_worker_count(0, 16),
        }

    def apply_settings(self, settings: dict) -> None:
        self.input_path = str(settings.get("input_path", self.input_path))
        self.include_subdirectories = bool(
            settings.get("include_subdirectories", self.include_subdirectories)
        )
        self.path_contains = str(settings.get("path_contains", self.path_contains))
        self.skip_errors = bool(settings.get("skip_errors", self.skip_errors))
        validation_jobs = settings.get("validation_jobs", self.validation_jobs)
        if isinstance(validation_jobs, int):
            self.validation_jobs = max(1, min(validation_jobs, 16))
        check_ids = settings.get("enabled_check_ids")
        if isinstance(check_ids, list):
            self._saved_check_ids = {str(check_id) for check_id in check_ids}
        self.report_path = str(settings.get("report_path", self.report_path))
        mode = str(settings.get("fix_mode", self.fix_mode))
        self.fix_mode = mode if mode in {"output", "in_place"} else "output"
        self.fix_output_folder = str(
            settings.get("fix_output_folder", self.fix_output_folder)
        )
        jobs = settings.get("fix_jobs", self.fix_jobs)
        if isinstance(jobs, int):
            self.fix_jobs = max(1, min(jobs, 16))
        if self._initialized:
            self.restore_path(self.report_path)

    def collect_settings(self) -> dict:
        return {
            "input_path": self.input_path,
            "include_subdirectories": self.include_subdirectories,
            "path_contains": self.path_contains,
            "skip_errors": self.skip_errors,
            "validation_jobs": self.validation_jobs,
            "enabled_check_ids": sorted(self.enabled_check_ids),
            "report_path": self.report_path,
            "fix_mode": self.fix_mode,
            "fix_output_folder": self.fix_output_folder,
            "fix_jobs": self.fix_jobs,
        }

    def _draw_issue_table(self) -> None:
        detail_height = 205.0 if self._selected_file() is not None else 35.0
        table_height = max(130.0, imgui.get_content_region_avail().y - detail_height)
        flags = (
            imgui.TableFlags_.borders_inner_h
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.scroll_y
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.sortable
            | imgui.TableFlags_.sizing_stretch_prop
        )
        if not imgui.begin_table(
            "##validation_report_files", 5, flags, imgui.ImVec2(0, table_height)
        ):
            return

        imgui.table_setup_column(
            "Severity",
            imgui.TableColumnFlags_.width_fixed
            | imgui.TableColumnFlags_.default_sort
            | imgui.TableColumnFlags_.prefer_sort_descending,
            95.0,
            1,
        )
        imgui.table_setup_column(
            "Findings", imgui.TableColumnFlags_.width_fixed, 75.0, 2
        )
        imgui.table_setup_column("Game", imgui.TableColumnFlags_.width_fixed, 75.0, 3)
        imgui.table_setup_column(
            "Rules", imgui.TableColumnFlags_.width_stretch, 0.30, 4
        )
        imgui.table_setup_column("Path", imgui.TableColumnFlags_.width_stretch, 0.70, 5)
        imgui.table_setup_scroll_freeze(0, 1)
        imgui.table_headers_row()
        self._apply_table_sort()

        clipper = imgui.ListClipper()
        clipper.begin(len(self.filtered_files))
        while clipper.step():
            for row_index in range(clipper.display_start, clipper.display_end):
                issue_file = self.filtered_files[row_index]
                self._draw_issue_row(issue_file, row_index)
        clipper.end()
        imgui.end_table()

    def _apply_table_sort(self) -> None:
        specs = imgui.table_get_sort_specs()
        if specs is None or specs.specs_count < 1:
            return
        spec = specs.get_specs(0)
        field = _SORT_FIELDS.get(spec.column_user_id)
        if field is None:
            return
        descending = spec.sort_direction == imgui.SortDirection_.descending
        if field != self.sort_field or descending != self.sort_descending:
            self.sort_field = field
            self.sort_descending = descending
            self._requery()
        specs.specs_dirty = False

    def _draw_issue_row(self, issue_file: ValidationIssueFile, row_index: int) -> None:
        imgui.table_next_row()
        imgui.table_next_column()
        severity = (
            "load failed" if not issue_file.success else issue_file.highest_severity
        )
        color = _SEVERITY_COLORS.get(issue_file.highest_severity)
        if color is not None:
            imgui.push_style_color(imgui.Col_.text, color)
        clicked, _ = imgui.selectable(
            f"{severity.upper()}##report_row_{row_index}",
            self.selected_path == issue_file.path,
            imgui.SelectableFlags_.span_all_columns,
        )
        if color is not None:
            imgui.pop_style_color()
        if clicked:
            self.selected_path = issue_file.path
            self._selected_issue = issue_file

        imgui.table_next_column()
        imgui.text_unformatted(str(len(issue_file.findings)))
        imgui.table_next_column()
        imgui.text_unformatted(issue_file.game)
        imgui.table_next_column()
        rules = ", ".join(issue_file.rules)
        imgui.text_unformatted(rules)
        if imgui.is_item_hovered() and rules:
            imgui.set_tooltip(rules)
        imgui.table_next_column()
        imgui.text_unformatted(issue_file.path)
        if imgui.is_item_hovered():
            imgui.set_tooltip(issue_file.path)

    def _draw_selected_details(self) -> None:
        issue_file = self._selected_file()
        if issue_file is None:
            return

        imgui.separator_text("Selected file")
        imgui.text_wrapped(issue_file.path)
        if issue_file.load_error:
            imgui.text_colored(_SEVERITY_COLORS["error"], issue_file.load_error)

        flags = (
            imgui.TableFlags_.borders_inner_h
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.scroll_y
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.sizing_stretch_prop
        )
        if imgui.begin_table(
            "##validation_report_details", 4, flags, imgui.ImVec2(0, 145.0)
        ):
            imgui.table_setup_column(
                "Severity", imgui.TableColumnFlags_.width_fixed, 75.0
            )
            imgui.table_setup_column(
                "Rule / Check", imgui.TableColumnFlags_.width_stretch, 0.24
            )
            imgui.table_setup_column(
                "Location", imgui.TableColumnFlags_.width_stretch, 0.18
            )
            imgui.table_setup_column(
                "Message", imgui.TableColumnFlags_.width_stretch, 0.58
            )
            imgui.table_setup_scroll_freeze(0, 1)
            imgui.table_headers_row()
            for finding in issue_file.findings:
                imgui.table_next_row()
                imgui.table_next_column()
                color = _SEVERITY_COLORS.get(finding.severity)
                if color is None:
                    imgui.text_unformatted(finding.severity.upper())
                else:
                    imgui.text_colored(color, finding.severity.upper())
                imgui.table_next_column()
                check = f" / {finding.check}" if finding.check else ""
                imgui.text_unformatted(f"{finding.rule}{check}")
                imgui.table_next_column()
                location = []
                if finding.block_id is not None:
                    location.append(f"Block {finding.block_id}")
                if finding.block_type:
                    location.append(finding.block_type)
                if finding.field:
                    location.append(finding.field)
                imgui.text_unformatted(" | ".join(location))
                imgui.table_next_column()
                imgui.text_wrapped(finding.message)
            imgui.end_table()

    def _selected_file(self) -> ValidationIssueFile | None:
        return self._selected_issue

    def _requery(self) -> None:
        if self.report is None:
            self.filtered_files = []
            return
        filters = ReportFilters(
            text=self.text_filter,
            severity=self.severity_filter,
            rule=self.rule_filter,
            check=self.check_filter,
            load_failures=self.failure_filter,
        )
        filtered = filter_issue_files(self.report.issue_files, filters)
        self.filtered_files = sort_issue_files(
            filtered, self.sort_field, self.sort_descending
        )
        self._selected_issue = next(
            (item for item in self.filtered_files if item.path == self.selected_path),
            None,
        )
        if self._selected_issue is None:
            self.selected_path = ""

    def _default_dialog_path(self) -> str:
        if self.report_path:
            return str(Path(self.report_path).parent)
        if self._toolkit_settings is None:
            return ""
        try:
            game_id = self._toolkit_settings.get_active_game()
            game_paths = self._toolkit_settings.get_game_paths(game_id)
            return str(game_paths.get("extracted_dir") or "")
        except (AttributeError, TypeError):
            return ""

    def _input_dialog_path(self) -> str:
        if self.input_path:
            path = Path(self.input_path)
            return str(path.parent if path.suffix else path)
        return self._default_dialog_path()

    def _report_dialog_path(self) -> str:
        if self.report_path:
            return str(Path(self.report_path).parent)
        if self.input_path:
            path = Path(self.input_path)
            return str(path.parent if path.suffix else path)
        return self._default_dialog_path()
