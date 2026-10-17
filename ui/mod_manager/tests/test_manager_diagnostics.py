"""One root LogPanel receives manager lifecycle diagnostics without private handlers."""
import logging

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.diagnostics import LOG, emit
from creation_lib.mod_manager.paths import Cancelled
from ui.toolkit.shared_panels import LogPanel


def test_main_log_deduplicates_repeat_frames_and_preserves_handler_lifecycle():
    root = logging.getLogger()
    original = tuple(root.handlers)
    panel, manager = LogPanel(), ManagerController()
    try:
        panel.install()
        panel.install()
        assert root.handlers.count(panel._handler) == 1 and not LOG.handlers
        manager.status = "Fixture operation"
        manager.submit("Fixture operation", lambda *_: "complete", lambda value: setattr(manager, "status", value))
        manager._future.result(timeout=5)
        manager.poll()
        for _ in range(10):
            manager.poll()
        events = [message for _, _, message in panel._lines if "toolkit.mod_manager" in message]
        assert len(events) == 2 and "running" in events[0] and "completed" in events[1]
        manager.error = "Fixture error"
        emit(manager)
        emit(manager)
        assert panel._lines[-1][0] == logging.ERROR
        manager.error, manager.status, manager.job_state = "", "Cancelled fixture", "cancelled"
        emit(manager)
        assert panel._lines[-1][0] == logging.WARNING
    finally:
        manager.cleanup()
        panel.uninstall()
        panel.uninstall()
    assert tuple(root.handlers) == original


def test_failed_and_cancelled_workers_emit_once_at_terminal_state():
    manager, panel = ManagerController(), LogPanel()
    panel.install()
    try:
        for error in (OSError("Fixture failure"), Cancelled("Fixture cancellation")):
            def fail(*_, error=error):
                raise error
            manager.submit("Fixture worker", fail, lambda _: None)
            manager._future.exception(timeout=5)
            manager.poll()
            manager.poll()
        records = [(level, text) for level, _, text in panel._lines if "toolkit.mod_manager" in text]
        assert len(records) == 4
        assert records[1][0] == logging.ERROR and "Fixture failure" in records[1][1]
        assert records[3][0] == logging.WARNING and "Fixture cancellation" in records[3][1]
    finally:
        manager.cleanup()
        panel.uninstall()
