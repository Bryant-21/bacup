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


def test_broken_toolchain_aborts_before_anything_is_stripped():
    resolutions = [
        _res("A", "compiler_unavailable", "target script source dir not configured"),
        _res("B", "compiler_unavailable", "target script source dir not configured"),
    ]
    assert resolutions[0].is_infrastructure_failure is True
    assert _res("A", "compile_failed").is_infrastructure_failure is False

    with pytest.raises(RuntimeError) as excinfo:
        _assert_script_toolchain_healthy(resolutions)

    message = str(excinfo.value)
    assert "toolchain unavailable" in message
    assert "target script source dir not configured" in message
    assert "2 script(s)" in message

    # Backstop for systemic causes no status marks as systemic.
    with pytest.raises(RuntimeError, match="0 of 50 script"):
        _assert_script_toolchain_healthy([_res(f"S{i}", "compile_failed") for i in range(50)])


@pytest.mark.parametrize(
    "resolutions",
    [
        pytest.param([_res("OnlyOne", "addition_missing")], id="lone_failure"),
        pytest.param(
            [
                _res("Good", "compiled"),
                _res("Bad", "compile_failed", "17:2: cannot assign None to Bool"),
                _res("Gone", "source_missing"),
            ],
            id="ordinary_failures",
        ),
        pytest.param([_res(f"T{i}", "target") for i in range(3)], id="all_target"),
        pytest.param([], id="empty"),
    ],
)
def test_per_script_failures_pass_through_to_stripping(resolutions):
    _assert_script_toolchain_healthy(resolutions)
