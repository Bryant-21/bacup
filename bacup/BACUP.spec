# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller spec for B.A.C.U.P.

Build with:
  pyinstaller bacup/BACUP.spec
"""

import os
import sys
from PyInstaller.utils.hooks import (
    collect_data_files,
    collect_dynamic_libs,
    collect_submodules,
)

block_cipher = None
spec_root = os.path.abspath(globals().get("SPECPATH", os.getcwd()))
bacup_root = (
    spec_root
    if os.path.basename(spec_root).lower() == "bacup"
    else os.path.join(spec_root, "bacup")
)
repo_root = os.path.dirname(bacup_root)
bacup_python_root = os.path.join(bacup_root, "py_bacup_lib", "python")
creation_python_root = os.path.join(repo_root, "py_creation_lib", "python")
bacup_native_root = os.path.join(bacup_root, "py_bacup_lib", "native")
creation_package_root = os.path.join(creation_python_root, "creation_lib")
resource_root = os.path.join(repo_root, "resource")
for source_root in (repo_root, bacup_python_root, creation_python_root):
    if source_root not in sys.path:
        sys.path.insert(0, source_root)

exe_name = os.environ.get("MODBOX21_EXE_NAME", "BACUP")
dist_name = os.environ.get("MODBOX21_DIST_NAME", exe_name)
icon_path = os.environ.get(
    "MODBOX21_ICON", os.path.join(repo_root, "resource", "icons", "modbox21-converter.ico")
)
is_nif_build = dist_name.lower().endswith("-nif") or exe_name.lower().endswith("-nif")
onefile = os.environ.get("MODBOX21_ONEFILE", "").lower() not in ("", "0", "false", "no")

# Onefile mode has no folder beside the EXE, so resource/ (tool binaries, icons)
# must be bundled into the archive. Exclude spriggit (large, unused by the
# converter). In onedir mode the build script copies resource/ alongside instead.
excluded_resource_entries = {"spriggit", "steam_api64.dll", "steam_appid.txt"}
onefile_resource_datas = []
if onefile and os.path.isdir(resource_root):
    for _entry in os.listdir(resource_root):
        if _entry.lower() in excluded_resource_entries:
            continue
        _src = os.path.join(resource_root, _entry)
        _dest = f"resource/{_entry}" if os.path.isdir(_src) else "resource"
        onefile_resource_datas.append((_src, _dest))

# ---------------------------------------------------------------------------
# Analysis
# ---------------------------------------------------------------------------

# Collect imgui_bundle data and dynamic libs (native .pyd files)
imgui_datas = collect_data_files("imgui_bundle")
imgui_libs = collect_dynamic_libs("imgui_bundle")
imgui_mods = collect_submodules("imgui_bundle")

# Collect creation_lib runtime modules that are imported lazily by workspace code.
creation_lib_mods = collect_submodules(
    "creation_lib",
    filter=lambda name: ".tests" not in name
    and not name.rsplit(".", 1)[-1].startswith("test"),
)

bacup_lib_mods = collect_submodules(
    "bacup_lib",
    filter=lambda name: ".tests" not in name
    and not name.rsplit(".", 1)[-1].startswith("test"),
)
bacup_ui_mods = collect_submodules(
    "bacup_ui",
    filter=lambda name: ".tests" not in name
    and not name.rsplit(".", 1)[-1].startswith("test"),
)
bacup_lib_datas = collect_data_files("bacup_lib")
bacup_ui_datas = collect_data_files("bacup_ui")
bacup_lib_binaries = collect_dynamic_libs("bacup_lib")

# Collect every ui submodule so PyInstaller analyzes their imports
# (workspaces import lazily via importlib.import_module — without this,
# transitive deps like watchdog and sounddevice are missed).
ui_mods = collect_submodules(
    "ui",
    filter=lambda name: ".tests" not in name
    and not name.rsplit(".", 1)[-1].startswith("test"),
)
ui_datas = collect_data_files("ui")

# File watchers are imported by lazily loaded UI workspaces.
watchdog_mods = collect_submodules("watchdog")

# Collect numpy dynamic libs
numpy_libs = collect_dynamic_libs("numpy")

# Collect PIL data
pil_datas = collect_data_files("PIL")

# Collect sqlite_vec extension
sqlite_vec_libs = collect_dynamic_libs("sqlite_vec")

# Collect winpty (pywinpty) native DLLs for the AI terminal PTY backend
winpty_libs = collect_dynamic_libs("winpty")
winpty_datas = collect_data_files("winpty")

# Collect Autodesk FBX SDK bindings (.pyd + runtime DLL)
import glob as _glob
_fbx_pyd = _glob.glob(os.path.join(
    os.path.dirname(sys.executable), "..", "Lib", "site-packages", "fbx*.pyd"
))
if not _fbx_pyd:
    # Also check the repository venv site-packages
    _fbx_pyd = _glob.glob(os.path.join(repo_root, ".venv", "Lib", "site-packages", "fbx*.pyd"))
_fbx_binaries = [(p, ".") for p in _fbx_pyd]

# FBX SDK runtime DLL
_fbx_dll = r"C:\Program Files\Autodesk\FBX\FBX SDK\2020.3.9\lib\x64\release\libfbxsdk.dll"
if os.path.isfile(_fbx_dll):
    _fbx_binaries.append((_fbx_dll, "."))

if is_nif_build:
    creation_resource_datas = [
        (os.path.join(creation_package_root, "resources", "xWMAEncode.exe"), "creation_lib/resources"),
        (os.path.join(creation_package_root, "resources", "BmlFuzEncode.exe"), "creation_lib/resources"),
        (os.path.join(creation_package_root, "resources", "xtexconv.exe"), "creation_lib/resources"),
        (os.path.join(creation_package_root, "resources", "classxml"), "creation_lib/resources/classxml"),
        (os.path.join(creation_package_root, "resources", "classxml_2012"), "creation_lib/resources/classxml_2012"),
        (os.path.join(creation_package_root, "resources", "classxml_2015"), "creation_lib/resources/classxml_2015"),
    ]
else:
    creation_resource_datas = [
        (os.path.join(creation_package_root, "resources"), "creation_lib/resources"),
    ]

bacup_native_resource_datas = [
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

nif_excludes = [
    "trimesh",
] if is_nif_build else []

a = Analysis(
    [os.path.join(bacup_python_root, "bacup_ui", "__main__.py")],
    pathex=[repo_root, bacup_python_root, creation_python_root],
    binaries=[
        *imgui_libs,
        *numpy_libs,
        *sqlite_vec_libs,
        *winpty_libs,
        *bacup_lib_binaries,
        *_fbx_binaries,
    ],
    datas=[
        *imgui_datas,
        *pil_datas,
        *winpty_datas,
        *bacup_lib_datas,
        *bacup_ui_datas,
        *ui_datas,
        # App code and data
        (os.path.join(creation_package_root, "ba2"), "py_creation_lib/python/creation_lib/ba2"),
        (os.path.join(creation_package_root, "db"), "py_creation_lib/python/creation_lib/db"),
        (os.path.join(creation_package_root, "fbx"), "py_creation_lib/python/creation_lib/fbx"),
        (os.path.join(creation_package_root, "nif"), "py_creation_lib/python/creation_lib/nif"),
        (os.path.join(creation_package_root, "nif", "nif_xml"), "creation_lib/nif/nif_xml"),
        *creation_resource_datas,
        (os.path.join(creation_package_root, "renderer", "shaders"), "creation_lib/renderer/shaders"),
        (os.path.join(creation_package_root, "renderer", "assets"), "creation_lib/renderer/assets"),
        *bacup_native_resource_datas,
        (os.path.join(bacup_root, "scripts", "lod_settings"), "bacup/scripts/lod_settings"),
        (os.path.join(bacup_root, "scripts", "lod_settings.fo76fo4.json"), "bacup/scripts"),
        (os.path.join(bacup_root, "scripts", "lod_settings.appalachia512_debug.json"), "bacup/scripts"),
        *([ (os.path.join(repo_root, "configs"), "configs") ] if os.path.isdir(os.path.join(repo_root, "configs")) else []),
        (os.path.join(repo_root, "VERSION"), "."),
        *onefile_resource_datas,
    ],
    hiddenimports=[
        *imgui_mods,
        *creation_lib_mods,
        *bacup_lib_mods,
        *bacup_ui_mods,
        *ui_mods,
        *watchdog_mods,
        "imgui_bundle",
        "imgui_bundle.imgui",
        "imgui_bundle.hello_imgui",
        "imgui_bundle.immapp",
        "imgui_bundle.imgui_md",
        "numpy",
        "PIL",
        "PIL.Image",
        "sqlite3",
        "sqlite_vec",
        "winreg",
        "fbx",
        "creation_lib.fbx",
        "creation_lib.fbx.nif_to_fbx",
        "ctypes",
        "multiprocessing",
        "xml.etree.ElementTree",
        # AI terminal PTY backend
        "pyte",
        "pyte.screens",
        "pyte.streams",
        "pyte.modes",
        "winpty",
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        # Large ML libraries — not needed (FTS5 keyword search only)
        "sentence_transformers",
        "torch",
        "torchvision",
        "torchaudio",
        "transformers",
        "tokenizers",
        "safetensors",
        "huggingface_hub",
        # Hunyuan3D local inference — local model feature, not bundled in release
        "hy3dgen",
        "hy3dshape",
        "diffusers",
        "accelerate",
        "torchvision",
        # MCP server frameworks — not needed in standalone
        "fastmcp",
        "pygls",
        "lark",
        "mcp",
        # Dev/test tools
        "pytest",
        "mypy",
        "black",
        "ruff",
        "isort",
        # Other unused
        "IPython",
        "jupyter",
        "notebook",
        "matplotlib",
        "pandas",
        "cv2",
        *nif_excludes,
    ],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

# ---------------------------------------------------------------------------
# PYZ (compressed Python modules)
# ---------------------------------------------------------------------------

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

# ---------------------------------------------------------------------------
# EXE
# ---------------------------------------------------------------------------

# UPX is DISABLED: UPX 5.x corrupts several bundled native DLLs (python312.dll,
# glfw3.dll, numpy OpenBLAS), which crashes the frozen app at startup with a
# native access violation in ntdll (heap corruption, no Python traceback).
_UPX = False

if onefile:
    # Single self-contained EXE (binaries + datas folded into the archive).
    exe = EXE(
        pyz,
        a.scripts,
        a.binaries,
        a.zipfiles,
        a.datas,
        [],
        name=exe_name,
        debug=False,
        bootloader_ignore_signals=False,
        strip=False,
        upx=_UPX,
        runtime_tmpdir=None,
        console=False,
        disable_windowed_traceback=False,
        argv_emulation=False,
        target_arch=None,
        codesign_identity=None,
        entitlements_file=None,
        icon=icon_path,
    )
else:
    exe = EXE(
        pyz,
        a.scripts,
        [],
        exclude_binaries=True,
        name=exe_name,
        debug=False,
        bootloader_ignore_signals=False,
        strip=False,
        upx=_UPX,
        console=False,
        disable_windowed_traceback=False,
        argv_emulation=False,
        target_arch=None,
        codesign_identity=None,
        entitlements_file=None,
        icon=icon_path,
    )

    # COLLECT (onedir mode) — skipped entirely for onefile builds.
    coll = COLLECT(
        exe,
        a.binaries,
        a.zipfiles,
        a.datas,
        strip=False,
        upx=_UPX,
        upx_exclude=[],
        name=dist_name,
    )
