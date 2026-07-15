#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
work_root="$(mktemp -d "${TMPDIR:-/tmp}/bacup-wheel.XXXXXX")"

cleanup() {
    chmod -R u+w "$work_root" 2>/dev/null || true
    rm -rf "$work_root"
}
trap cleanup EXIT

case "$work_root/" in
    "$repo_root/"*)
        printf 'Temporary workspace must be outside the repository: %s\n' "$work_root" >&2
        exit 1
        ;;
esac

unset PYTHONHOME PYTHONPATH VIRTUAL_ENV UV_PROJECT_ENVIRONMENT

creation_dist="$work_root/creation-dist"
bacup_dist="$work_root/bacup-dist"
venv_dir="$work_root/venv"
runtime_dir="$work_root/runtime"
mkdir -p "$creation_dist" "$bacup_dist" "$runtime_dir"
cd "$runtime_dir"

find_single_wheel() {
    local wheel_dir="$1"
    local wheel_prefix="$2"
    local label="$3"
    local -a wheels=()

    while IFS= read -r -d '' wheel; do
        wheels+=("$wheel")
    done < <(find "$wheel_dir" -maxdepth 1 -type f -name "${wheel_prefix}-*.whl" -print0)

    if [[ "${#wheels[@]}" -ne 1 ]]; then
        printf 'Expected exactly one %s wheel in %s; found %d\n' \
            "$label" "$wheel_dir" "${#wheels[@]}" >&2
        find "$wheel_dir" -maxdepth 1 -type f -print >&2
        return 1
    fi
    printf '%s\n' "${wheels[0]}"
}

printf 'Building py-creation-lib wheel...\n'
uv build --wheel --out-dir "$creation_dist" "$repo_root/py_creation_lib"
creation_wheel="$(find_single_wheel "$creation_dist" "py_creation_lib" "py-creation-lib")"

printf 'Building bacup-lib wheel...\n'
uv build --wheel --out-dir "$bacup_dist" "$repo_root/bacup/py_bacup_lib"
bacup_wheel="$(find_single_wheel "$bacup_dist" "bacup_lib" "bacup-lib")"

uv venv "$venv_dir"
if [[ -f "$venv_dir/Scripts/python.exe" ]]; then
    venv_python="$venv_dir/Scripts/python.exe"
elif [[ -f "$venv_dir/bin/python" ]]; then
    venv_python="$venv_dir/bin/python"
else
    printf 'Could not find the virtual-environment Python under %s\n' "$venv_dir" >&2
    exit 1
fi

uv pip install --python "$venv_python" --no-deps "$creation_wheel"
CHECK_REPO_ROOT="$repo_root" "$venv_python" - <<'PY'
from __future__ import annotations

from importlib.util import find_spec
from pathlib import Path
import os
import sys

import creation_lib
import creation_lib._native as creation_native
import creation_lib.ck

repo_root = Path(os.environ["CHECK_REPO_ROOT"]).resolve()


def assert_outside_repo(path: str | Path) -> None:
    resolved = Path(path).resolve()
    try:
        resolved.relative_to(repo_root)
    except ValueError:
        return
    raise AssertionError(f"editable/source-tree path leaked into clean environment: {resolved}")


assert hasattr(creation_native, "esp_authoring_core")
assert hasattr(creation_native, "ck_native")
assert not hasattr(creation_native, "conversion_native")
assert find_spec("creation_lib.conversion") is None
assert find_spec("bacup_lib") is None
assert find_spec("bacup_ui") is None
assert_outside_repo(creation_lib.__file__)
assert_outside_repo(creation_native.__file__)
for entry in sys.path:
    if entry:
        assert_outside_repo(entry)
print("creation wheel isolation: ok")
PY

CHECK_BACUP_WHEEL="$bacup_wheel" "$venv_python" - <<'PY'
from __future__ import annotations

from email.parser import BytesParser
from pathlib import Path
from zipfile import ZipFile
import os
import re

wheel = Path(os.environ["CHECK_BACUP_WHEEL"])
with ZipFile(wheel) as archive:
    metadata_names = [
        name for name in archive.namelist() if name.endswith(".dist-info/METADATA")
    ]
    assert len(metadata_names) == 1, metadata_names
    metadata = BytesParser().parsebytes(archive.read(metadata_names[0]))

requirements = metadata.get_all("Requires-Dist", [])
assert any(
    re.match(r"\s*py[-_.]creation[-_.]lib(?:\s|[<>=!~;(]|$)", requirement, re.I)
    for requirement in requirements
), requirements
print("BACUP wheel metadata dependency: ok")
PY

uv pip install \
    --python "$venv_python" \
    --find-links "$creation_dist" \
    "$bacup_wheel"

CHECK_REPO_ROOT="$repo_root" "$venv_python" - <<'PY'
from __future__ import annotations

from importlib.resources import files
from pathlib import Path
import os
import sys

import bacup_lib
import bacup_lib._native as bacup_native
import bacup_ui
import creation_lib._native as creation_native
from bacup_lib.native_maps import (
    native_face_resources_dir,
    native_translation_maps_dir,
)
from bacup_lib.native_runtime import load_native_module
from bacup_lib.upgrade_manifest import (
    bundled_upgrade_manifest_path,
    load_upgrade_manifest,
)

repo_root = Path(os.environ["CHECK_REPO_ROOT"]).resolve()


def assert_outside_repo(path: str | Path) -> Path:
    resolved = Path(path).resolve()
    try:
        resolved.relative_to(repo_root)
    except ValueError:
        return resolved
    raise AssertionError(f"editable/source-tree path leaked into clean environment: {resolved}")


assert hasattr(creation_native, "esp_authoring_core")
assert not hasattr(creation_native, "conversion_native")
assert hasattr(bacup_native, "esp_authoring_core")
assert hasattr(bacup_native, "conversion_native")
assert load_native_module()._raw is bacup_native.conversion_native

bacup_package = files("bacup_lib")
python_resources = [
    bacup_package.joinpath("animation", "weapon_family_table.yaml"),
    bacup_package.joinpath("script_patches", "Default1StateSyncActivator.psc"),
    bacup_package.joinpath("resources", "conversion", "upgrade_manifest.yaml"),
]
assert all(resource.is_file() for resource in python_resources), python_resources

upgrade_manifest_path = bundled_upgrade_manifest_path()
assert upgrade_manifest_path.is_file()
assert load_upgrade_manifest(upgrade_manifest_path).current

translation_maps = native_translation_maps_dir()
embedded = translation_maps.parent
native_src = embedded.parent
fnv_data = native_src / "fnv_legacy_scripting" / "data"
face_resources = native_face_resources_dir()
native_resources = [
    translation_maps / "fo76_to_fo4.yaml",
    embedded / "fo76_condition_functions.yaml",
    embedded / "whitelists" / "fo4.yaml",
    fnv_data / "fnv_to_fo4_script_functions.yaml",
    face_resources / "named_bones.yaml",
    face_resources / "fnv_to_fo4_correspondence_male.npz",
]
assert all(resource.is_file() for resource in native_resources), native_resources

# The shared `ui` framework remains a monorepo-only edge. The BACUP application
# spec bundles it; the isolated wheel check verifies bacup_ui and its variant
# module are installed without importing that shared framework.
assert files("bacup_ui").joinpath("variant.py").is_file()

for module in (bacup_lib, bacup_native, bacup_ui, creation_native):
    assert_outside_repo(module.__file__)
for resource in [*python_resources, *native_resources, upgrade_manifest_path]:
    assert_outside_repo(resource)
for entry in sys.path:
    if entry:
        assert_outside_repo(entry)
print("dual native load and BACUP packaged resources: ok")
PY
