# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller onedir spec for the headless BACUP regen runner."""

import os
import sys

from PyInstaller.utils.hooks import (
    collect_data_files,
    collect_dynamic_libs,
    collect_submodules,
)


spec_root = os.path.abspath(globals().get("SPECPATH", os.getcwd()))
bacup_root = (
    spec_root
    if os.path.basename(spec_root).lower() == "bacup"
    else os.path.join(spec_root, "bacup")
)
repo_root = os.path.dirname(bacup_root)
scripts_root = os.path.join(repo_root, "scripts")
snapshot_root = os.environ.get("BACUP_REGEN_PACKAGE_ROOT")
if snapshot_root:
    bacup_python_root = snapshot_root
    creation_python_root = snapshot_root
else:
    bacup_python_root = os.path.join(bacup_root, "py_bacup_lib", "python")
    creation_python_root = os.path.join(repo_root, "py_creation_lib", "python")
bacup_native_root = os.path.join(bacup_root, "py_bacup_lib", "native")
creation_package_root = os.path.join(creation_python_root, "creation_lib")

for source_root in (repo_root, scripts_root, bacup_python_root, creation_python_root):
    if source_root not in sys.path:
        sys.path.insert(0, source_root)


def production_module(name):
    leaf = name.rsplit(".", 1)[-1]
    return (
        ".tests" not in name
        and not leaf.startswith("test")
        and not name.startswith("creation_lib.ui")
        and not name.startswith("creation_lib.max")
    )


bacup_modules = collect_submodules("bacup_lib", filter=production_module)
creation_modules = collect_submodules("creation_lib", filter=production_module)
bacup_datas = collect_data_files("bacup_lib")
bacup_binaries = collect_dynamic_libs("bacup_lib")
creation_binaries = collect_dynamic_libs("creation_lib")
numpy_binaries = collect_dynamic_libs("numpy")
pil_datas = collect_data_files("PIL")
root_resource = os.path.join(repo_root, "resource")

creation_datas = [
    (
        os.path.join(creation_package_root, "resources"),
        "creation_lib/resources",
    ),
    (
        os.path.join(creation_package_root, "nif", "nif_xml"),
        "creation_lib/nif/nif_xml",
    ),
    (
        os.path.join(creation_package_root, "havok", "resources"),
        "creation_lib/havok/resources",
    ),
    (
        os.path.join(creation_package_root, "esp", "schema", "data"),
        "creation_lib/esp/schema/data",
    ),
    (
        os.path.join(creation_package_root, "renderer", "shaders"),
        "creation_lib/renderer/shaders",
    ),
    (
        os.path.join(creation_package_root, "renderer", "assets"),
        "creation_lib/renderer/assets",
    ),
]

bacup_native_datas = [
    (
        os.path.join(bacup_native_root, "conversion", "src", "embedded"),
        "native/conversion/src/embedded",
    ),
    (
        os.path.join(
            bacup_native_root,
            "conversion",
            "src",
            "fnv_legacy_scripting",
            "data",
        ),
        "native/conversion/src/fnv_legacy_scripting/data",
    ),
    (
        os.path.join(
            bacup_native_root,
            "conversion",
            "src",
            "phase",
            "resources",
            "face",
        ),
        "native/conversion/src/phase/resources/face",
    ),
]

lod_datas = [
    (
        os.path.join(bacup_root, "scripts", "lod_settings"),
        "bacup/scripts/lod_settings",
    ),
    (
        os.path.join(bacup_root, "scripts", "lod_settings.fo76fo4.json"),
        "bacup/scripts",
    ),
    (
        os.path.join(
            bacup_root,
            "scripts",
            "lod_settings.appalachia512_debug.json",
        ),
        "bacup/scripts",
    ),
]

a = Analysis(
    [os.path.join(scripts_root, "regen.py")],
    pathex=[repo_root, scripts_root, bacup_python_root, creation_python_root],
    binaries=[
        *bacup_binaries,
        *creation_binaries,
        *numpy_binaries,
        (os.path.join(root_resource, "xg.dll"), "creation_lib/resources"),
    ],
    datas=[
        *bacup_datas,
        *pil_datas,
        *creation_datas,
        *bacup_native_datas,
        *lod_datas,
        (os.path.join(repo_root, "VERSION"), "."),
    ],
    hiddenimports=[
        "_conversion_cli",
        *bacup_modules,
        *creation_modules,
        "bacup_lib._native",
        "creation_lib._native",
        "numpy",
        "PIL",
        "PIL.Image",
        "yaml",
        "psutil",
        "orjson",
        "zstandard",
        "lz4",
        "sqlite3",
        "winreg",
        "ctypes",
        "multiprocessing",
        "xml.etree.ElementTree",
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        "bacup_ui",
        "creation_lib.ui",
        "creation_lib.max",
        "fastmcp",
        "imgui_bundle",
        "sentence_transformers",
        "torch",
        "torchvision",
        "torchaudio",
        "transformers",
        "pytest",
        "mypy",
        "ruff",
        "IPython",
        "jupyter",
        "matplotlib",
        "pandas",
    ],
    noarchive=False,
)

pyz = PYZ(a.pure)
exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="BACUP-Regen",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    icon=os.path.join(repo_root, "resource", "icons", "modbox21-converter.ico"),
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name="BACUP-Regen",
)
