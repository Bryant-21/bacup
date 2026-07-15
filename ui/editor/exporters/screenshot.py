"""Screenshot capture for the NIF editor using ModernGL FBO readback."""
import logging
from datetime import datetime
from pathlib import Path

from PIL import Image

_log = logging.getLogger("nif_editor.screenshot")


def capture_screenshot(
    renderer,
    filepath: str | None = None,
    nif_path: str | None = None,
) -> str | None:
    """Capture a screenshot of the current viewport FBO.

    Args:
        renderer: The SceneRenderer instance (must have fbo_texture).
        filepath: Output file path. Auto-generated if None.
        nif_path: Current NIF path for auto-naming.

    Returns:
        The saved file path on success, or None on failure.
    """
    if filepath is None:
        stem = "screenshot"
        if nif_path:
            stem = Path(nif_path).stem
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filepath = str(Path.cwd() / f"{stem}_{timestamp}.png")

    if not renderer or not renderer.fbo_texture:
        _log.error("No FBO texture to capture")
        return None

    try:
        tex = renderer.fbo_texture
        w, h = tex.size
        data = tex.read()
        img = Image.frombytes("RGBA", (w, h), data)
        img = img.transpose(Image.FLIP_TOP_BOTTOM)
        img.save(filepath)
        _log.info("Screenshot saved: %s", filepath)
        return filepath
    except Exception as e:
        _log.error("Failed to save screenshot: %s", e)
        return None
