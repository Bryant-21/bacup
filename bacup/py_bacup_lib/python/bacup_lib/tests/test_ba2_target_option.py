import inspect

from bacup_lib.regen_pipeline import RegenOptions
from bacup_lib.workflows import unified


def test_regen_options_defaults_to_nextgen():
    assert RegenOptions().fo4_ba2_target == "nextgen"


def test_finalize_sinks_accepts_ba2_target():
    sig = inspect.signature(unified.finalize_sinks_for_mod)
    assert sig.parameters["fo4_ba2_target"].default == "nextgen"


def test_run_unified_accepts_ba2_target():
    sig = inspect.signature(unified.run_unified)
    assert sig.parameters["fo4_ba2_target"].default == "nextgen"
