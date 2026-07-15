from __future__ import annotations

from pathlib import Path


NIF_LIKE_EXTENSIONS = frozenset({".nif", ".bto", ".btr"})
NIF_LIKE_FILETYPES = [
    ("NIF/BTO/BTR files", "*.nif;*.bto;*.btr"),
    ("All files", "*.*"),
]


def is_nif_like_path(path: str | Path) -> bool:
    return Path(path).suffix.lower() in NIF_LIKE_EXTENSIONS
