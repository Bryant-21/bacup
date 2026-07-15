# ui/editor/tests/test_lsp_service.py
import time
import pytest
from ui.papyrus.papyrus_lsp_service import LspService, LspRequest, PARSE_RESOLVE, COMPLETE


def test_lsp_service_start_stop():
    """Service starts and stops without error."""
    svc = LspService(db_path="/nonexistent/scripts.db")
    svc.start()
    time.sleep(0.05)
    svc.stop()  # Must not hang


def test_parse_error_recovery_clean_script():
    """A valid Papyrus script produces no errors and returns a parsed AST."""
    from ui.papyrus.papyrus_lsp_service import _parse_error_recovery
    text = "Scriptname MyScript\n\nFunction Foo()\nEndFunction\n"
    errors, final_ast = _parse_error_recovery(text)
    assert errors == [], f"Expected no errors, got {errors}"
    assert final_ast is not None


def test_parse_error_recovery_bad_line():
    """A script with a syntax error produces at least one error entry."""
    from ui.papyrus.papyrus_lsp_service import _parse_error_recovery
    text = "Scriptname MyScript\n\n@@@ invalid syntax @@@\n\nFunction Foo()\nEndFunction\n"
    errors, final_ast = _parse_error_recovery(text)
    assert len(errors) >= 1
    # Each error is (line_1indexed, col_1indexed, message_str)
    for err in errors:
        assert len(err) == 3
        assert isinstance(err[0], int)   # line
        assert isinstance(err[1], int)   # col
        assert isinstance(err[2], str)   # message


def test_parse_error_recovery_caps_iterations():
    """max_iterations parameter is respected."""
    from ui.papyrus.papyrus_lsp_service import _parse_error_recovery
    # Script with many errors — ensure we don't hang with a tight cap
    lines = [f"@@@ bad line {i} @@@" for i in range(20)]
    text = "Scriptname X\n" + "\n".join(lines) + "\n"
    errors, _ = _parse_error_recovery(text, max_iterations=3)
    assert len(errors) <= 3


def test_lsp_service_starts_worker_thread():
    """LspService spins up a worker thread on start; native parser releases
    the GIL inside Rust, so no subprocess is needed."""
    svc = LspService(db_path="/nonexistent/scripts.db")
    assert svc._thread is None
    svc.start()
    assert svc._thread is not None and svc._thread.is_alive()
    svc.stop()
    assert svc._thread is None


def test_lsp_service_parse_via_executor():
    """LspService processes a PARSE_RESOLVE request via the worker thread (no DB needed)."""
    import time
    svc = LspService(db_path="/nonexistent/scripts.db")
    svc.start()
    req = LspRequest(PARSE_RESOLVE, path="x.psc",
                     text="Scriptname X\n\nFunction Foo()\nEndFunction\n")
    svc.submit(req)
    # Wait up to 30s for the worker to process it (Lark cold-start can be slow)
    deadline = time.monotonic() + 30.0
    result = None
    while time.monotonic() < deadline:
        result = svc.poll_diagnostics()
        if result:
            break
        time.sleep(0.1)
    svc.stop()
    assert "x.psc" in result
    assert result["x.psc"] == []   # valid script → no diagnostics


def test_stale_parse_resolve_replaced():
    """Submitting two PARSE_RESOLVE for same path keeps only the latest."""
    svc = LspService(db_path="/nonexistent/scripts.db")
    svc.start()
    req1 = LspRequest(PARSE_RESOLVE, path="foo.psc", text="old content", line=0, col=0)
    req2 = LspRequest(PARSE_RESOLVE, path="foo.psc", text="new content", line=0, col=0)
    svc.submit(req1)
    svc.submit(req2)
    # Only latest should be in the slot
    with svc._lock:
        slot = svc._pending_parse.get("foo.psc")
    assert slot is None or slot.text == "new content"
    svc.stop()
