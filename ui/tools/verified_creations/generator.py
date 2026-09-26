import concurrent.futures
import logging
import shutil
from datetime import datetime
from pathlib import Path
from typing import Callable, List, Optional

from .bethesda import BethesdaClient, Mod
from .formatter import _image_urls, build_post_title, render_post_body
from .utils import download_image, parse_iso

logger = logging.getLogger("tools.verified_creations")

_TEMPLATE_DIR = Path(__file__).parent / "templates"
TEMPLATES = {
    "reddit": _TEMPLATE_DIR / "post.md",
    "discord": _TEMPLATE_DIR / "discord_post.md",
    "wiki": _TEMPLATE_DIR / "wiki_post.txt",
}

# The marketplace listing re-includes BGS's older Creation Club content (222 Skyrim items).
_BGS_IGNORE_BEFORE = "2025-01-01T00:00:00+00:00"
_BGS_AUTHOR = "bethesdagamestudios"
_PAGE_SIZE = 100


def _mod_date(mod: Mod) -> Optional[datetime]:
    return parse_iso(mod.first_published_at) or parse_iso(mod.published_at)


def _has_paid_price(prices: List[dict]) -> bool:
    for price in prices:
        try:
            if price.get("amount") is not None and float(price["amount"]) > 0:
                return True
        except (TypeError, ValueError):
            continue
    return False


def _is_new_creation(mod: Mod) -> bool:
    """Marketplace items are always verified; this drops free ones and old BGS content."""
    author = mod.author_displayname or "Unknown"
    is_bgs = author.lower() == _BGS_AUTHOR
    mod_date = _mod_date(mod)
    if is_bgs and mod_date and mod_date < parse_iso(_BGS_IGNORE_BEFORE):
        logger.info("Skip %s (%s): BGS ignore cutoff", mod.mod_id, author)
        return False
    if not _has_paid_price(mod.prices) and not is_bgs:
        logger.info("Skip %s (%s): no paid price", mod.mod_id, author)
        return False
    return True


def _safe_filename(value: str) -> str:
    return "".join(ch if ch.isalnum() or ch in "-_" else "_" for ch in value).strip("_")


def _download_images(mod: Mod, post_dir: Path, reuse_dir: Optional[Path] = None) -> None:
    targets = []
    if mod.preview_image_url:
        targets.append(("image_00_preview.jpg", mod.preview_image_url))
    targets += [(f"image_{i + 1:02d}.jpg", url) for i, url in enumerate(_image_urls(mod))]
    for name, url in targets:
        existing = reuse_dir / name if reuse_dir else None
        if existing and existing.exists():
            shutil.copy(existing, post_dir / name)
        else:
            download_image(url, post_dir / name)


def _write_post(mod: Mod, kind: str, out_dir: Path) -> None:
    title = build_post_title(mod, "new", include_emojis=False)
    body = render_post_body(mod, "new", TEMPLATES[kind])
    pub_date = mod.first_published_at or mod.published_at or "unknown"
    base_name = _safe_filename(f"{pub_date[:10]}_{mod.author_displayname or 'Unknown'}_{mod.title}")

    if kind == "discord":
        (out_dir / f"{base_name}.md").write_text(f"# {title}\n\n{body}\n", encoding="utf-8")
        return

    post_dir = out_dir / base_name
    post_dir.mkdir(parents=True, exist_ok=True)
    if kind == "reddit":
        (post_dir / f"{base_name}.md").write_text(f"# {title}\n\n{body}\n", encoding="utf-8")
        _download_images(mod, post_dir)
    else:
        (post_dir / f"{_safe_filename(mod.title)}.wiki").write_text(body + "\n", encoding="utf-8")
        _download_images(mod, post_dir, reuse_dir=out_dir.parent / "reddit" / base_name)


def generate_posts(
    product: str,
    kind: str,
    cutoff: datetime,
    output_dir: Path,
    cancelled: Callable[[], bool] = lambda: False,
    on_page: Callable[[int, int], None] = lambda page, written: None,
) -> int:
    """Write one post per verified Creation published after ``cutoff``; returns the count."""
    client = BethesdaClient()
    out_dir = output_dir / kind
    out_dir.mkdir(parents=True, exist_ok=True)

    # Paid Creations are often created months before release and the listing is ordered by
    # creation time, so there is no safe early stop: read the whole marketplace catalog.
    page = 1
    written = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
        while not cancelled():
            mods = client.fetch_mods(product, page, _PAGE_SIZE)
            logger.info("Fetched %s mods (page %s)", len(mods), page)
            if not mods:
                break

            futures = []
            for mod in mods:
                mod_time = _mod_date(mod)
                if mod_time and mod_time <= cutoff:
                    continue
                if _is_new_creation(mod):
                    futures.append(executor.submit(_write_post, mod, kind, out_dir))

            for future in concurrent.futures.as_completed(futures):
                try:
                    future.result()
                    written += 1
                except Exception as exc:
                    logger.error("Failed to write %s post: %s", kind, exc)

            on_page(page, written)
            if len(mods) < _PAGE_SIZE:
                break
            page += 1

    logger.info("Wrote %s %s posts to %s", written, kind, out_dir)
    return written
