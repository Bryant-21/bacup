"""Frozen-vs-dev path resolution for ModBox21.

When running as a PyInstaller EXE:
  - app_root = directory containing the EXE (user-writable: settings, databases, logs)
  - code_root = sys._MEIPASS (read-only bundled code + data)

When running in dev mode (uv run python -m ui.toolkit):
  - app_root = project root (<your-checkout>)
  - code_root = project root
"""

import os
import sys
from pathlib import Path


def is_frozen() -> bool:
    """True when running inside a PyInstaller bundle."""
    return getattr(sys, "frozen", False)


def _looks_like_project_root(path: Path) -> bool:
    """Heuristic check for a ModBox21 workspace root."""
    return (path / "app").is_dir() and (path / "ui").is_dir()


def find_project_root(start: Path | None = None) -> Path | None:
    """Find the active project root from env, cwd, or ancestor search.

    Priority:
    1. ``MOD_ROOT`` env var
    2. current working directory and its parents
    3. caller-provided ``start`` path and its parents
    """
    env_root = os.environ.get("MOD_ROOT", "").strip()
    if env_root:
        candidate = Path(env_root).resolve()
        if _looks_like_project_root(candidate):
            return candidate

    for seed in filter(None, [Path.cwd(), start.resolve() if start else None]):
        candidate = seed
        if candidate.is_file():
            candidate = candidate.parent
        for parent in [candidate, *candidate.parents]:
            if _looks_like_project_root(parent):
                return parent
    return None


def load_dotenv_into_environ(env_path: Path | None = None, *, override: bool = False) -> Path | None:
    """Load simple KEY=VALUE pairs from .env into ``os.environ``.

    Supports quoted values and skips comments/blank lines. Shell expressions
    like ``MOD_ROOT=...`` are intentionally left untouched if already present
    in the process environment.
    """
    project_root = None
    if env_path is None:
        project_root = find_project_root()
        env_path = project_root / ".env" if project_root else None
        if env_path is None and is_frozen():
            exe_env_path = Path(sys.executable).resolve().parent / ".env"
            if exe_env_path.is_file():
                env_path = exe_env_path
    elif env_path.is_file():
        project_root = env_path.parent
    if env_path is None or not env_path.is_file():
        return None

    if project_root is not None and (override or "MOD_ROOT" not in os.environ):
        os.environ["MOD_ROOT"] = str(project_root)

    for raw_line in env_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not key:
            continue
        if not override and key in os.environ:
            continue
        if (value.startswith('"') and value.endswith('"')) or (value.startswith("'") and value.endswith("'")):
            value = value[1:-1]
        for env_key, env_val in os.environ.items():
            value = value.replace(f"${{{env_key}}}", env_val)
            value = value.replace(f"${env_key}", env_val)
        os.environ[key] = value
    return env_path


def get_app_root() -> Path:
    """Directory containing the EXE (frozen) or project root (dev).

    This is the user-writable root for settings, databases, and logs.
    """
    if is_frozen():
        project_root = find_project_root(Path(sys.executable))
        if project_root is not None:
            return project_root
        return Path(sys.executable).resolve().parent
    project_root = find_project_root(Path(__file__))
    if project_root is not None:
        return project_root
    # app/paths.py lives one level shallower than py_creation_lib/python/creation_lib/core/app_paths.py,
    # so the `__file__`-relative fallback uses two .parent steps instead of three.
    return Path(__file__).resolve().parent.parent


def get_exe_dir() -> Path:
    """Directory containing the EXE (frozen) or project root (dev).

    Unlike get_app_root(), this never walks up to a project-root marker, so
    frozen output (extraction, generated mods) always lands next to the EXE
    even when the EXE is unzipped inside a project tree.
    """
    if is_frozen():
        return Path(sys.executable).resolve().parent
    return get_app_root()


def get_code_root() -> Path:
    """sys._MEIPASS (frozen) or project root (dev).

    This is the read-only root for bundled code, nif.xml, preprocess scripts, etc.
    """
    if is_frozen():
        return Path(sys._MEIPASS)
    return get_app_root()


def get_resource_dir() -> Path:
    """Directory containing tool binaries (texconv.exe, xWMAEncode.exe, etc.).

    Frozen: {code_root}/resource/
    Dev: {project_root}/resource/
    """
    return get_code_root() / "resource"


def get_db_dir() -> Path:
    """Directory containing SQLite databases.

    Both frozen and dev: {app_root}/data/
    """
    return get_app_root() / "data"


def get_settings_path() -> Path:
    """Legacy-compatible path to the full toolkit variant settings.

    New code should prefer get_shared_settings_path() and
    get_variant_settings_path().
    """
    return get_variant_settings_path("full")


def get_settings_config_dir() -> Path:
    """Directory for shared and per-variant toolkit settings."""
    if is_frozen():
        return get_app_root() / "settings"
    return get_app_root() / "ui" / "toolkit" / "settings_data"


def get_shared_settings_path() -> Path:
    """Path to shared toolkit settings."""
    return get_settings_config_dir() / "shared_settings.json"


def get_variant_settings_path(variant_id: str = "full") -> Path:
    """Path to per-variant toolkit settings."""
    return get_settings_config_dir() / "variants" / f"{variant_id}.json"


def get_logs_dir() -> Path:
    """Directory for log files.

    Frozen builds always log next to the EXE so each standalone variant keeps
    its own ``logs/`` even when unzipped inside the project tree (where the
    project-root marker search would otherwise redirect logs upstream).
    """
    if is_frozen():
        return Path(sys.executable).resolve().parent / "logs"
    return get_app_root() / "logs"


def get_ini_dir() -> Path:
    """Directory for hello_imgui .ini settings files."""
    d = get_app_root() / "settings"
    d.mkdir(exist_ok=True)
    return d
