"""A broken Papyrus toolchain must abort, not strip every record's VMAD.

Regression cover for a user-reported run that finished "successfully" with
``compiled=0 failed=6742 stripped_vmad=13624`` — the FO4 Creation Kit was not
installed, so no script could compile, and the pipeline read that as 6742
individual script failures and removed their bindings from the plugin.
"""
from __future__ import annotations

import pytest

from bacup_lib.workflows.unified import (
    _ScriptResolution,
    _assert_script_toolchain_healthy,
)


def _res(name: str, status: str, message: str = "") -> _ScriptResolution:
    return _ScriptResolution(script_name=name, status=status, message=message)


def test_compiler_unavailable_aborts_before_anything_is_stripped():
    resolutions = [
        _res("A", "compiler_unavailable", "target script source dir not configured"),
        _res("B", "compiler_unavailable", "target script source dir not configured"),
    ]

    with pytest.raises(RuntimeError) as excinfo:
        _assert_script_toolchain_healthy(resolutions)

    message = str(excinfo.value)
    assert "toolchain unavailable" in message
    assert "target script source dir not configured" in message
    assert "2 script(s)" in message


def test_zero_compiled_at_scale_aborts_without_an_infrastructure_status():
    """The backstop, for systemic causes no status marks as systemic."""
    resolutions = [_res(f"S{i}", "compile_failed") for i in range(50)]

    with pytest.raises(RuntimeError, match="0 of 50 script"):
        _assert_script_toolchain_healthy(resolutions)


def test_a_lone_failing_script_still_strips_its_own_binding():
    """Scale bound: one genuine failure is ordinary and must pass through.

    Stripping that record's VMAD is the correct outcome here, so the backstop
    must not hijack it.
    """
    _assert_script_toolchain_healthy([_res("OnlyOne", "addition_missing")])


def test_ordinary_per_script_failures_are_allowed_through():
    """A healthy run carries real failures; those must still reach stripping."""
    resolutions = [
        _res("Good", "compiled"),
        _res("Bad", "compile_failed", "17:2: cannot assign None to Bool"),
        _res("Gone", "source_missing"),
    ]

    _assert_script_toolchain_healthy(resolutions)


def test_all_target_scripts_is_not_a_zero_compiled_failure():
    """`target` scripts defer to the base game's PEX and never compile."""
    resolutions = [_res(f"T{i}", "target") for i in range(3)]

    _assert_script_toolchain_healthy(resolutions)


def test_empty_run_is_not_a_failure():
    _assert_script_toolchain_healthy([])


def test_infrastructure_failures_are_not_treated_as_script_failures():
    assert _res("A", "compiler_unavailable").is_infrastructure_failure is True
    assert _res("A", "compile_failed").is_infrastructure_failure is False
    assert _res("A", "source_missing").is_infrastructure_failure is False
    assert _res("A", "compiled").is_infrastructure_failure is False
