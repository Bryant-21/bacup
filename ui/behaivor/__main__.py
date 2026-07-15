"""Entry point: python -m ui.behaivor"""
import sys
from pathlib import Path

# Ensure project root is on sys.path for creation_lib.* imports
_project_root = str(Path(__file__).resolve().parents[2])
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

from .main_window import main

main()
