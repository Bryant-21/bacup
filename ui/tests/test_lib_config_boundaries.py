from __future__ import annotations

import ast
import re
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parents[2]


def _runtime_lib_files() -> list[Path]:
    files: list[Path] = []
    for path in (PROJECT_ROOT / "py_creation_lib" / "python" / "creation_lib").rglob("*.py"):
        rel_parts = path.relative_to(PROJECT_ROOT).parts
        if "tests" in rel_parts or path.name.startswith("test_"):
            continue
        if path.relative_to(PROJECT_ROOT).as_posix() == "py_creation_lib/python/creation_lib/core/app_paths.py":
            continue
        files.append(path)
    return files


def test_runtime_lib_does_not_import_app_path_boundaries():
    offenders: list[str] = []
    import_re = re.compile(
        r"^\s*(?:from\s+(?:creation_lib\.core\.app_paths|app\.paths)\s+import|import\s+(?:creation_lib\.core\.app_paths|app\.paths)\b)",
        re.MULTILINE,
    )
    for path in _runtime_lib_files():
        text = path.read_text(encoding="utf-8", errors="replace")
        if import_re.search(text):
            offenders.append(path.relative_to(PROJECT_ROOT).as_posix())
    assert offenders == []


def test_runtime_lib_does_not_import_path_defaults():
    offenders: list[str] = []
    import_re = re.compile(
        r"^\s*(?:from\s+creation_lib\.core\.path_defaults\s+import|import\s+creation_lib\.core\.path_defaults\b)",
        re.MULTILINE,
    )
    for path in _runtime_lib_files():
        text = path.read_text(encoding="utf-8", errors="replace")
        if import_re.search(text):
            offenders.append(path.relative_to(PROJECT_ROOT).as_posix())
    assert offenders == []


def test_runtime_lib_does_not_infer_workspace_path_defaults():
    offenders: list[str] = []
    forbidden = (
        "default_project_root",
        "default_db_dir",
        "default_resource_dir",
        "default_logs_dir",
        'Path.cwd() / "data"',
        "Path.cwd() / 'data'",
    )
    for path in _runtime_lib_files():
        rel = path.relative_to(PROJECT_ROOT).as_posix()
        if rel == "py_creation_lib/python/creation_lib/core/path_defaults.py":
            offenders.append(rel)
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for pattern in forbidden:
            if pattern in text:
                offenders.append(f"{rel}: {pattern}")
                break
    assert offenders == []


class _DotEnvPathVisitor(ast.NodeVisitor):
    def __init__(self) -> None:
        self.lines: list[int] = []

    def visit_BinOp(self, node: ast.BinOp) -> None:
        if isinstance(node.op, ast.Div) and _contains_dotenv_literal(node.right):
            self.lines.append(node.lineno)
        self.generic_visit(node)


def _contains_dotenv_literal(node: ast.AST) -> bool:
    return isinstance(node, ast.Constant) and node.value == ".env"


def test_runtime_lib_does_not_build_dotenv_paths():
    offenders: list[str] = []
    for path in _runtime_lib_files():
        source = path.read_text(encoding="utf-8", errors="replace")
        try:
            tree = ast.parse(source)
        except SyntaxError:
            continue
        visitor = _DotEnvPathVisitor()
        visitor.visit(tree)
        for line in visitor.lines:
            offenders.append(f"{path.relative_to(PROJECT_ROOT).as_posix()}:{line}")
    assert offenders == []


def test_env_config_builds_game_context_from_env_dict(tmp_path):
    from app.env_config import build_game_context_from_env

    ctx = build_game_context_from_env(
        "fo4",
        {
            "FO4_DIR": str(tmp_path / "Fallout 4"),
            "FO4_EXTRACTED_DIR": str(tmp_path / "Extracted"),
            "ADDON_NODE_INDEX_START": "22000",
        },
    )

    assert ctx.game == "fo4"
    assert ctx.root_dir == tmp_path / "Fallout 4"
    assert ctx.data_dir == tmp_path / "Fallout 4" / "Data"
    assert ctx.extracted_dir == tmp_path / "Extracted"
    assert ctx.strings_dirs == (
        tmp_path / "Fallout 4" / "Data" / "Strings",
        tmp_path / "Extracted" / "Strings",
        tmp_path / "Extracted" / "Data" / "Strings",
    )
    assert ctx.addon_index_start == 22000


def test_toolkit_settings_adapter_builds_game_context(tmp_path):
    from ui.toolkit.lib_config import build_game_context_from_settings
    from ui.toolkit.settings import ToolkitSettings

    settings_path = tmp_path / "toolkit_settings.json"
    settings = ToolkitSettings(path=settings_path, editor_settings_path=tmp_path / "missing.json")
    settings._paths["fo4"]["root_dir"] = str(tmp_path / "Root")
    settings._paths["fo4"]["extracted_dir"] = str(tmp_path / "Extracted")
    settings.addon_node_index_start = 23000

    ctx = build_game_context_from_settings(settings, "fo4")

    assert ctx.root_dir == tmp_path / "Root"
    assert ctx.data_dir == tmp_path / "Root" / "Data"
    assert ctx.extracted_dir == tmp_path / "Extracted"
    assert ctx.addon_index_start == 23000


def test_rust_workspace_belongs_to_py_creation_lib():
    assert not (PROJECT_ROOT / "Cargo.toml").exists()
    assert (PROJECT_ROOT / "py_creation_lib" / "Cargo.toml").is_file()
    assert (PROJECT_ROOT / "py_creation_lib" / "Cargo.lock").is_file()

    ensure_native = (PROJECT_ROOT / "scripts" / "ensure_native.py").read_text(
        encoding="utf-8"
    )
    assert '"cargo", "metadata", "--format-version", "1"' in ensure_native
    assert 'REPO_ROOT / "py_creation_lib"' in ensure_native
    assert 'REPO_ROOT / "bacup" / "py_bacup_lib"' in ensure_native
    assert "--no-deps" not in ensure_native
