"""Memory + disk PNG cache for shape gallery thumbnails."""
from __future__ import annotations

from pathlib import Path
from PIL import Image
import io


class ThumbnailCache:
    """Two-layer cache: in-memory GL texture IDs + on-disk PNG files.

    Memory cache: dict[(shape_id, size)] -> GL texture_id (int)
    Disk cache:   {cache_dir}/{game}/{shape_id}_{size}.png

    GL texture IDs are only valid in the current GL context session.
    PNG files persist across sessions for fast startup.
    """

    def __init__(self, game: str, cache_dir: Path) -> None:
        self._mem: dict[tuple[int, int], int] = {}
        self._dir = cache_dir / game
        self._dir.mkdir(parents=True, exist_ok=True)

    def get(self, shape_id: int, size: int) -> int | None:
        """Return cached GL texture ID or None on miss."""
        return self._mem.get((shape_id, size))

    def put(self, shape_id: int, size: int, texture_id: int, pixels: bytes) -> None:
        """Store texture ID in memory and write PNG to disk.

        pixels: raw RGBA bytes (size * size * 4)
        """
        self._mem[(shape_id, size)] = texture_id
        img = Image.frombytes("RGBA", (size, size), pixels)
        png_path = self._dir / f"{shape_id}_{size}.png"
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        png_path.write_bytes(buf.getvalue())

    def load_png(self, shape_id: int, size: int) -> bytes | None:
        """Return raw PNG file bytes if cached on disk, else None."""
        path = self._dir / f"{shape_id}_{size}.png"
        return path.read_bytes() if path.exists() else None

    def invalidate(self, disk: bool = False) -> None:
        """Clear in-memory texture IDs and optionally disk cache."""
        self._mem.clear()
        if disk:
            import shutil
            if self._dir.exists():
                shutil.rmtree(self._dir, ignore_errors=True)
                self._dir.mkdir(parents=True, exist_ok=True)
