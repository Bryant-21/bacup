from pathlib import Path
import os
import subprocess
import sys


def build_packaged_native(repo_root: Path) -> Path:
    target = repo_root / "build/bacup-packaged-native"
    env = os.environ.copy()
    env["PYO3_PYTHON"] = sys.executable
    subprocess.run(
        [
            "cargo", "build", "--release", "--manifest-path",
            str(repo_root / "bacup/py_bacup_lib/Cargo.toml"),
            "-p", "bacup_lib_native", "--features", "packaged-creation",
            "--target-dir", str(target),
        ],
        cwd=repo_root, env=env, check=True,
    )
    return target / "release/_native.dll"


if __name__ == "__main__":
    print(build_packaged_native(Path(__file__).resolve().parents[2]))
