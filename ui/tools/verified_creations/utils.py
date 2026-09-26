import base64
import io
import json
import logging
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

import requests
from PIL import Image

logger = logging.getLogger("tools.verified_creations")


def iso_from_epoch(value: Optional[int]) -> Optional[str]:
    if value is None:
        return None
    try:
        return datetime.fromtimestamp(int(value), tz=timezone.utc).isoformat()
    except (TypeError, ValueError, OSError):
        return None


def parse_iso(value: Optional[str]) -> Optional[datetime]:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None


def bethesda_image_url(s3bucket: Optional[str], s3key: Optional[str]) -> Optional[str]:
    if not s3bucket or not s3key:
        return None
    payload = {
        "bucket": s3bucket,
        "key": s3key,
        "edits": {"resize": {}},
        "outputFormat": "webp",
    }
    raw = json.dumps(payload, separators=(",", ":"), ensure_ascii=True).encode("utf-8")
    token = base64.b64encode(raw).decode("ascii")
    return f"https://ugcmods.bethesda.net/image/{token}"


def download_image(url: str, output_path: Path) -> bool:
    try:
        response = requests.get(url, timeout=30)
        response.raise_for_status()
        image = Image.open(io.BytesIO(response.content))
        if image.mode in ("RGBA", "P"):
            image = image.convert("RGB")
        image.save(output_path, "JPEG", quality=90)
        return True
    except Exception as e:
        logger.error("Failed to download or convert image %s to %s: %s", url, output_path, e)
        return False
