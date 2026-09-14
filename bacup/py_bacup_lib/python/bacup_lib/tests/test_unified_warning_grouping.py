from __future__ import annotations

from types import SimpleNamespace

from bacup_lib.workflows import unified


class _Run:
    def __init__(self) -> None:
        self._decision_batches = [
            [{"kind": "record_decision", "message": "first"}],
            [{"kind": "record_decision", "message": "second"}],
        ]
        self._warning_batches = [
            [
                "unsupported_target_record:ABCD",
                "unique warning",
                "unsupported_target_record:ABCD",
                "unsupported_target_record:EFGH",
                "unsupported_target_record:ABCD",
            ],
            ["unique warning", "later warning", "unique warning"],
        ]
        self.decision_drains = 0
        self.warning_drains = 0

    def drain_decisions(self):
        batch = self._decision_batches[self.decision_drains]
        self.decision_drains += 1
        return batch

    def drain_warnings(self):
        batch = self._warning_batches[self.warning_drains]
        self.warning_drains += 1
        return batch


def test_inline_warning_batches_group_duplicates_and_preserve_accounting() -> None:
    runtime = object.__new__(unified._UnifiedRecordRuntime)
    run = _Run()
    logs: list[tuple[str, str]] = []
    runner = SimpleNamespace(
        emit_log=lambda level, message: logs.append((level, message))
    )
    existing_decision = {"kind": "record_decision", "message": "existing"}
    ctx = SimpleNamespace(
        conversion_decisions=[existing_decision],
        addon_index_map={},
        summary=SimpleNamespace(records_warnings=4),
    )

    runtime._emit_run_warnings_inline(run, runner, ctx)
    runtime._emit_run_warnings_inline(run, runner, ctx)

    assert logs == [
        ("WARN", "unsupported_target_record:ABCD (3 occurrences)"),
        ("WARN", "unique warning"),
        ("WARN", "unsupported_target_record:EFGH"),
        ("WARN", "unique warning (2 occurrences)"),
        ("WARN", "later warning"),
    ]
    assert ctx.summary.records_warnings == 12
    assert ctx.conversion_decisions == [
        existing_decision,
        {"kind": "record_decision", "message": "first"},
        {"kind": "record_decision", "message": "second"},
    ]
    assert run.decision_drains == 2
    assert run.warning_drains == 2
