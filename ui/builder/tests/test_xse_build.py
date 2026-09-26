from types import SimpleNamespace
from unittest.mock import patch

import pytest

from ui.builder.mod_builder_app import ModBuilderApp


@pytest.mark.parametrize("build_fails", [False, True])
def test_xse_build_compiles_before_staging_and_stops_on_failure(tmp_path, build_fails):
    app = object.__new__(ModBuilderApp)
    app._selected_mod = lambda: "B21_Test"
    app._selected_xse_src_dir = lambda: str(tmp_path)
    queued = []
    app._run_fn = lambda fn, **kwargs: queued.append(fn)
    app._on_xse_build()
    messages = []
    result = SimpleNamespace(returncode=1 if build_fails else 0, stdout="compiler output", stderr="")
    with patch("subprocess.run", return_value=result) as run:
        if build_fails:
            with pytest.raises(RuntimeError, match="xmake build failed"):
                queued[0](messages.append)
        else:
            queued[0](messages.append)
        expected = [["xmake", "build", "-y"]]
        if not build_fails:
            expected.append(["xmake", "install", "-y"])
        assert [call.args[0] for call in run.call_args_list] == expected
        assert all(call.kwargs["cwd"] == str(tmp_path) for call in run.call_args_list)
    assert ("DLL built and staged." in messages) is not build_fails
