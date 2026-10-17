import json
from pathlib import Path
import subprocess
import sys
import types

import pytest


ROOT = Path(__file__).resolve().parents[5]
COMBINED = ROOT / "build/bacup-packaged-native/release/_native.dll"
HOOK = ROOT / "bacup/scripts/rthook_packaged_native.py"


def _exports(module):
    return {
        name: {
            key: (type(getattr(value, key)).__name__, getattr(getattr(value, key), "__text_signature__", None))
            for key in dir(value) if not key.startswith("_")
        }
        for name in dir(module)
        if not name.startswith("_") and isinstance(value := getattr(module, name), types.ModuleType)
    }


def _run(code):
    if not COMBINED.is_file():
        pytest.skip("build bacup/scripts/build_packaged_native.py first")
    bootstrap = (
        "import runpy\nfrom pathlib import Path\n"
        f"runpy.run_path({str(HOOK)!r})['install_packaged_native'](Path({str(COMBINED)!r}))\n"
    )
    result = subprocess.run(
        [sys.executable, "-"], input=bootstrap + code, cwd=ROOT,
        capture_output=True, text=True,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    return result.stdout


def test_packaged_exports_match_development():
    from bacup_lib import _native as bacup
    from creation_lib import _native as creation

    expected = json.dumps({"creation": _exports(creation), "bacup": _exports(bacup)})
    code = (
        "import json, types\n"
        "from bacup_lib import _native as bacup\n"
        "from creation_lib import _native as creation\n"
        "def exports(module):\n"
        " return {name: {key: (type(getattr(value,key)).__name__, getattr(getattr(value,key),'__text_signature__',None)) "
        "for key in dir(value) if not key.startswith('_')} for name in dir(module) "
        "if not name.startswith('_') and isinstance(value := getattr(module,name),types.ModuleType)}\n"
        f"assert json.loads(json.dumps({{'creation':exports(creation),'bacup':exports(bacup)}})) == json.loads({expected!r})\n"
        "assert creation.__file__ == bacup.__file__\n"
        "assert not hasattr(creation, 'conversion_native')\n"
    )
    _run(code)


def test_development_backends_remain_separate():
    from bacup_lib import _native as bacup
    from creation_lib import _native as creation

    assert not hasattr(bacup, "_creation_native")
    assert creation.__file__ != bacup.__file__
    assert creation.esp_authoring_core is not bacup.esp_authoring_core


def test_packaged_handle_lifetimes_and_concurrent_callers():
    _run('''
import json
from concurrent.futures import ThreadPoolExecutor
from tempfile import TemporaryDirectory
from creation_lib import _native as creation
from bacup_lib import _native as bacup
from bacup_lib.run import ConversionRun

def open_plugin(api, name):
    payload = {"plugin": name + ".esp", "game": "fo4",
        "header": {"version": 1.0, "next_object_id": "000801"},
        "items": [{"type": "group", "label_text": "MISC", "group_type": 0,
            "children": [{"signature": "MISC", "form_id": "000800",
                "subrecords": [{"signature": "EDID", "data_hex": (name + "\\0").encode().hex()}]}]}]}
    return api.plugin_handle_import_text(json.dumps(payload), "json", "fo4")

c, b = creation.esp_authoring_core, bacup.esp_authoring_core
first, second = open_plugin(c, "Toolkit"), open_plugin(b, "Conversion")
assert first != second
assert c.plugin_handle_close(first)
assert b"Conversion\\0" in b.plugin_handle_to_bytes(second)
assert b.plugin_handle_close(second)

with TemporaryDirectory() as directory:
    toolkit = open_plugin(c, "SurvivesRunClose")
    with ConversionRun.create_new("fo4", "fo4", None, "First.esm", config={"mod_path": directory}) as run:
        run.save_target(run_nvnm_validator=False)
    assert b"SurvivesRunClose\\0" in c.plugin_handle_to_bytes(toolkit)
    with ConversionRun.create_new("fo4", "fo4", None, "Second.esm", config={"mod_path": directory}) as run:
        assert c.plugin_handle_close(toolkit)
        run.save_target(run_nvnm_validator=False)
        assert (Path(directory) / "Second.esm").is_file()

def exercise(index):
    api = c if index % 2 else b
    for iteration in range(12):
        name = f"Caller{index}_{iteration}"
        handle = open_plugin(api, name)
        assert name.encode() in api.plugin_handle_to_bytes(handle)
        assert api.plugin_handle_close(handle)

with ThreadPoolExecutor(max_workers=4) as pool:
    list(pool.map(exercise, range(4)))
''')
