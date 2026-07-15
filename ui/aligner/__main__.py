"""Entry point: python -m ui.aligner"""
import sys
from pathlib import Path

# Ensure project root is on sys.path so `creation_lib.*` imports resolve
_project_root = str(Path(__file__).resolve().parents[2])
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

from .aligner_app import main

main()
