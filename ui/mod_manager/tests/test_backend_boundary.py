import ast
from pathlib import Path

import creation_lib.mod_manager


def test_ui_has_no_duplicate_backend_modules_or_imports():
    ui = Path(__file__).resolve().parents[1]
    backend = Path(creation_lib.mod_manager.__file__).parent
    services = {path.stem for path in backend.glob("*.py") if path.stem != "__init__"}
    assert not [path.name for path in ui.glob("*.py") if path.stem in services]
    for path in ui.rglob("*.py"):
        tree = ast.parse(path.read_text("utf-8-sig"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom):
                module = node.module or ""
                if module == "ui.mod_manager":
                    assert not services.intersection(alias.name for alias in node.names), (path, node.lineno)
                if module.startswith("ui.mod_manager."):
                    assert module.split(".")[2] not in services, (path, node.lineno)
                if path.parent == ui and node.level == 1:
                    assert module.split(".")[0] not in services, (path, node.lineno)
            elif isinstance(node, ast.Import):
                for alias in node.names:
                    if alias.name.startswith("ui.mod_manager."):
                        assert alias.name.split(".")[2] not in services, (path, node.lineno)
