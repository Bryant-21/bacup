import importlib
import importlib.machinery
import importlib.util
from pathlib import Path
import sys


def install_packaged_native(path: Path) -> None:
    # Load before bacup_lib.__init__: its runner imports toolkit services.
    loader = importlib.machinery.ExtensionFileLoader("bacup_lib._native", str(path))
    spec = importlib.util.spec_from_file_location("bacup_lib._native", path, loader=loader)
    native = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = native
    spec.loader.exec_module(native)
    creation = native._creation_native
    creation.__file__ = str(path)
    sys.modules["creation_lib._native"] = creation
    for name, module in (("creation_lib", creation), ("bacup_lib", native)):
        package = importlib.import_module(name)
        package._native = module


if getattr(sys, "frozen", False):
    install_packaged_native(Path(sys._MEIPASS) / "bacup_lib/_native.pyd")
