import inspect

from bacup_lib.workflows import unified


def test_unified_entrypoints_default_to_nextgen_ba2_target():
    for entrypoint in (unified.finalize_sinks_for_mod, unified.run_unified):
        sig = inspect.signature(entrypoint)
        assert sig.parameters["fo4_ba2_target"].default == "nextgen"
