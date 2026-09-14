from __future__ import annotations

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from creation_lib.pex.native_runtime import compile_psc


def test_native_compile_success_does_not_classify_bare_names_as_resolved() -> None:
    source = """Scriptname Tranche5UnresolvedNameProbe Extends Quest

Function Probe()
    MissingBoundProperty.SetValue(1.0)
EndFunction
"""
    base = _fo4_base_source()
    assert base is not None

    result = compile_psc(
        source,
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path="Tranche5UnresolvedNameProbe.psc",
    )

    assert result.ok
    assert result.pex_bytes is not None
    assert result.diagnostics == []
    assert "Property MissingBoundProperty" not in source
