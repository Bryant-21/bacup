import logging
from dataclasses import asdict
from pathlib import Path
import re
from typing import Any, Dict, List, Optional

from .bethesda import Mod
from .utils import bethesda_image_url

# Optional pypandoc support for markdown to wikitext conversion.
# Used automatically if pypandoc (and a pandoc binary) are available; falls
# back to the regex-based converter below otherwise.
_pypandoc = None
_pypandoc_initialized = False


def _init_pypandoc():
    """Lazily try to import pypandoc and ensure a pandoc binary is available."""
    global _pypandoc, _pypandoc_initialized
    if _pypandoc_initialized:
        return
    _pypandoc_initialized = True
    try:
        import pypandoc as _pypandoc_module
        try:
            _pypandoc_module.get_pandoc_version()
        except OSError:
            logging.warning("Pandoc binary not found, downloading...")
            _pypandoc_module.download_pandoc()
        _pypandoc = _pypandoc_module
        logging.info("pypandoc initialized successfully")
    except ImportError:
        logging.info("pypandoc not available, using regex wikitext converter")
    except Exception:
        logging.exception("Failed to initialize pypandoc, using regex fallback")


PLATFORM_EMOJI = {
    "XBOXONE": ":xbox:",
    "XBOXSERIESX": ":xbox:",
    "PLAYSTATION4": ":playstation:",
    "PLAYSTATION5": ":playstation:",
    "WINDOWS": ":pc:",
    "ALL": ":globe_with_meridians:",
}
PLATFORM_FULL_NAME = {
    "XBOXONE": "Xbox One",
    "XBOXSERIESX": "Xbox Series X|S",
    "PLAYSTATION4": "PlayStation 4",
    "PLAYSTATION5": "PlayStation 5",
    "WINDOWS": "Windows",
    "ALL": "All Platforms",
}
PLATFORM_WIKI_LABEL = {
    "WINDOWS": "PC",
    "XBOXONE": "Xbox",
    "XBOXSERIESX": "Xbox",
    "PLAYSTATION4": "PlayStation",
    "PLAYSTATION5": "PlayStation",
}
PRODUCT_WIKI_NAME = {
    "FALLOUT4": "Fallout 4",
    "STARFIELD": "Starfield",
    "SKYRIM": "Skyrim",
}

PRODUCT_WIKI_FOOTER = {
    "FALLOUT4": "FO4",
    "STARFIELD": "Starfield",
    "SKYRIM": "Skyrim",
}
MAX_TITLE_FLAIRS = 10
PRICE_EMOJI = ":credits:"


def _product_label(mod: Mod) -> str:
    return mod.product_title or mod.product or "MOD"


def _join_list(items: List[str]) -> str:
    return ", ".join(items) if items else "N/A"


def _summary_text(mod: Mod) -> str:
    if mod.overview:
        return _strip_markdown(mod.overview.strip())
    if mod.description:
        return _strip_markdown(mod.description.strip().splitlines()[0])
    return "No summary provided."


def _platform_emojis(platforms: List[str]) -> str:
    tokens = []
    for platform in platforms:
        emoji = PLATFORM_EMOJI.get(platform)
        if emoji and emoji not in tokens:
            tokens.append(emoji)
    return " ".join(tokens[:MAX_TITLE_FLAIRS]) if tokens else ""


def _platform_full_names(platforms: List[str]) -> str:
    if not platforms:
        return "N/A"
    return ", ".join(PLATFORM_FULL_NAME.get(p, p) for p in platforms)


def _platform_wiki_labels(platforms: List[str]) -> str:
    if not platforms:
        return "N/A"
    labels = []
    for platform in platforms:
        label = PLATFORM_WIKI_LABEL.get(platform, platform)
        if label not in labels:
            labels.append(label)
    return ", ".join(labels)


def _format_value(value: Any) -> str:
    if value is None:
        return "N/A"
    if isinstance(value, (list, tuple)):
        return ", ".join(str(item) for item in value) if value else "N/A"
    if isinstance(value, dict):
        if not value:
            return "N/A"
        return ", ".join(f"{key}={value[key]}" for key in sorted(value))
    return str(value)


def _release_notes_text(mod: Mod) -> str:
    if not mod.release_notes:
        return "N/A"
    parts: List[str] = []
    for entry in mod.release_notes:
        platform = entry.get("hardware_platform") or "UNKNOWN"
        notes = entry.get("release_notes") or []
        if not notes:
            parts.append(f"- {platform}: N/A")
            continue
        for note in notes:
            version = note.get("version_name") or "Unknown version"
            text = note.get("note") or ""
            parts.append(f"- {platform} {version}: {text}".strip())
    return "\n".join(parts) if parts else "N/A"


def _strip_markdown(text: str) -> str:
    cleaned = text.replace("\r\n", "\n").replace("\r", "\n").strip()
    cleaned = re.sub(r"```.*?```", "", cleaned, flags=re.DOTALL)
    cleaned = re.sub(r"`([^`]*)`", r"\1", cleaned)
    cleaned = re.sub(r"!\[([^\]]*)\]\([^)]+\)", r"\1", cleaned)
    cleaned = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", cleaned)
    return cleaned


def _escape_markdown(text: str) -> str:
    return re.sub(r"([\\`*_{}\[\]()#+\-.!|>])", r"\\\1", text)


def _clean_description(text: Optional[str]) -> str:
    if not text:
        return "N/A"
    cleaned = _strip_markdown(text)
    if not cleaned:
        return "N/A"
    return _escape_markdown(cleaned)


def _markdown_description(text: Optional[str]) -> str:
    if not text:
        return "N/A"
    return text.replace("\r\n", "\n").replace("\r", "\n").strip()


def _wiki_description(text: Optional[str]) -> str:
    """Convert markdown text to MediaWiki wikitext format."""
    if not text:
        return "N/A"
    result = text.replace("\r\n", "\n").replace("\r", "\n").strip()
    
    # Use pypandoc if available; otherwise fall through to the regex converter
    _init_pypandoc()
    if _pypandoc is not None:
        logging.info("Using pypandoc for markdown to wikitext conversion")
        return _pypandoc.convert_text(result, 'mediawiki', format='markdown')
    
    # Fallback to regex-based conversion
    # Convert headers: ### Header -> === Header ===
    result = re.sub(r"^######\s*(.+)$", r"====== \1 ======", result, flags=re.MULTILINE)
    result = re.sub(r"^#####\s*(.+)$", r"===== \1 =====", result, flags=re.MULTILINE)
    result = re.sub(r"^####\s*(.+)$", r"==== \1 ====", result, flags=re.MULTILINE)
    result = re.sub(r"^###\s*(.+)$", r"=== \1 ===", result, flags=re.MULTILINE)
    result = re.sub(r"^##\s*(.+)$", r"== \1 ==", result, flags=re.MULTILINE)
    result = re.sub(r"^#\s*(.+)$", r"= \1 =", result, flags=re.MULTILINE)
    
    # - item or * item -> * item
    result = re.sub(r"^[\-\*]\s+", r"* ", result, flags=re.MULTILINE)
    
    # Convert bold: **text** or __text__ -> '''text'''
    result = re.sub(r"\*\*(.+?)\*\*", r"'''\1'''", result)
    result = re.sub(r"__(.+?)__", r"'''\1'''", result)
    
    # Convert italic: *text* or _text_ -> ''text''
    result = re.sub(r"(?<!['\w])\*([^*\n]+)\*(?!['\w])", r"''\1''", result)
    result = re.sub(r"(?<!['\w])_([^_\n]+)_(?!['\w])", r"''\1''", result)
    
    # Convert links: [text](url) -> [url text]
    result = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r"[\2 \1]", result)
    
    # Convert images: ![alt](url) -> [[File:url|alt]]
    result = re.sub(r"!\[([^\]]*)\]\(([^)]+)\)", r"[[File:\2|\1]]", result)
    
    # Convert inline code: `code` -> <code>code</code>
    result = re.sub(r"`([^`]+)`", r"<code>\1</code>", result)
    
    # Convert code blocks: ```code``` -> <pre>code</pre>
    result = re.sub(r"```\w*\n?(.*?)```", r"<pre>\1</pre>", result, flags=re.DOTALL)
    
    # Convert ordered lists: 1. item -> # item
    result = re.sub(r"^\d+\.\s+", r"# ", result, flags=re.MULTILINE)
    
    # Convert horizontal rules: --- or *** -> ----
    result = re.sub(r"^[\-\*]{3,}\s*$", r"----", result, flags=re.MULTILINE)
    
    return result


def _totals(mod: Mod) -> Dict[str, Any]:
    totals = mod.stats.get("totals") if isinstance(mod.stats, dict) else None
    return totals if isinstance(totals, dict) else {}


def _price_text(mod: Mod) -> str:
    if not mod.prices:
        return "N/A"
    parts = []
    for price in mod.prices:
        amount = price.get("amount")
        if amount is None:
            continue
        parts.append(f"{PRICE_EMOJI} {amount}")
    if not parts:
        return "N/A"
    return parts[0]


def _price_credits(mod: Mod) -> str:
    if not mod.prices:
        return "N/A"
    for price in mod.prices:
        amount = price.get("amount")
        if amount is None:
            continue
        return f"{{{{credits|{amount}}}}}"
    return "N/A"


def _price_text_plain(mod: Mod) -> str:
    if not mod.prices:
        return "N/A"
    parts = []
    for price in mod.prices:
        amount = price.get("amount")
        if amount is None:
            continue
        parts.append(f"{amount} Credits")
    if not parts:
        return "N/A"
    return parts[0]


def _cover_filename(mod: Mod) -> str:
    filename = mod.cover_image.get("filename")
    if isinstance(filename, str) and filename:
        return filename
    return "N/A"


def _release_date(mod: Mod) -> str:
    if not mod.first_published_at:
        return "N/A"
    return mod.first_published_at[:10]


def _latest_version(mod: Mod) -> str:
    latest: Optional[Dict[str, Any]] = None
    for entry in mod.release_notes:
        notes = entry.get("release_notes") if isinstance(entry, dict) else None
        if not isinstance(notes, list):
            continue
        for note in notes:
            if not isinstance(note, dict):
                continue
            if not note.get("published", True):
                continue
            if latest is None or (note.get("utime") or 0) > (latest.get("utime") or 0):
                latest = note
    if latest and latest.get("version_name"):
        return str(latest.get("version_name"))
    return "N/A"


def _image_urls(mod: Mod, raw: bool = False) -> List[str]:
    urls: List[str] = []
    cover_url = _non_banner_cover_url(mod)
    if cover_url and cover_url not in urls:
        urls.append(cover_url)
    for image in mod.screenshot_images:
        if _is_banner_image(image):
            continue
        url = image.get("url") or image.get("uri") or image.get("path") or image.get("link")
        if not url:
            s3bucket = image.get("s3bucket")
            s3key = image.get("s3key")
            if s3bucket and s3key:
                url = bethesda_image_url(s3bucket, s3key)
        if url and url not in urls:
            urls.append(url)
    return urls


def _image_urls_text(mod: Mod) -> str:
    urls = _image_urls(mod)
    return "\n".join(f"- {url}" for url in urls) if urls else "N/A"


def _is_banner_image(image: Dict[str, Any]) -> bool:
    if not image:
        return False
    classification = str(image.get("classification") or "").lower()
    filename = str(image.get("filename") or "").lower()
    s3key = str(image.get("s3key") or "").lower()
    return "banner" in classification or "banner" in filename or "/banner" in s3key


def _banner_url(mod: Mod) -> str:
    if _is_banner_image(mod.cover_image):
        s3bucket = mod.cover_image.get("s3bucket")
        s3key = mod.cover_image.get("s3key")
        url = bethesda_image_url(s3bucket, s3key) if s3bucket and s3key else None
        return url or "N/A"
    return "N/A"


def _non_banner_cover_url(mod: Mod) -> str:
    if _is_banner_image(mod.cover_image):
        return None
    return mod.cover_image_url


def build_post_title(mod: Mod, post_type: str, include_emojis: bool = True) -> str:
    if post_type == "update":
        date_hint = mod.updated_at[:10] if mod.updated_at else "Update"
        return f"[{_product_label(mod)}] Update: {mod.title} ({date_hint})"
    creator = mod.author_displayname or "Unknown Creator"
    emojis = _platform_emojis(mod.hardware_platforms) if include_emojis else ""
    suffix = f" {emojis}" if emojis else ""
    return f"{creator} presents: {mod.title}{suffix}"


def render_post_body(mod: Mod, post_type: str, template_path: Path) -> str:
    template = template_path.read_text(encoding="utf-8")
    author = mod.author_displayname or "Unknown"
    author_url = (
        f"https://creations.bethesda.net/en/{mod.product.lower()}/all"
        f"?author_displayname={author}"
        if mod.product and author != "Unknown"
        else "N/A"
    )

    # Start with all base Mod fields
    data: Dict[str, Any] = asdict(mod)

    # Add/Override with computed or human-friendly fields
    data.update({
        "post_type": post_type,
        "title_plain": build_post_title(mod, post_type, include_emojis=False),
        "summary": _summary_text(mod),
        "description": _clean_description(mod.description),
        "description_markdown": _markdown_description(mod.description),
        "description_wiki": _wiki_description(mod.description),
        "author": author,
        "author_url": author_url,
        "product_title": mod.product_title or mod.product,
        "product_wiki": PRODUCT_WIKI_NAME.get(mod.product, mod.product_title or mod.product),
        "product_wiki_footer": PRODUCT_WIKI_FOOTER.get(mod.product, mod.product_title or mod.product),
        "platforms": _join_list(mod.hardware_platforms),
        "platform_full_names": _platform_full_names(mod.hardware_platforms),
        "platform_wiki": _platform_wiki_labels(mod.hardware_platforms),
        "platform_emojis": _platform_emojis(mod.hardware_platforms),
        "categories": _join_list(mod.categories),
        "prices": _price_text(mod),
        "prices_plain": _price_text_plain(mod),
        "price_credits": _price_credits(mod),
        "release_date": _release_date(mod),
        "size": "N/A",
        "version": _latest_version(mod),
        "xbox_link": "Link",
        "cover_image_filename": _cover_filename(mod),
        "banner_image_url": _banner_url(mod),
        "image_urls": _image_urls_text(mod),
        # Explicit aliases requested by user
        "ptime": mod.published_at or "N/A",
        "first_ptime": mod.first_published_at or "N/A",
        "ctime": mod.created_at or "N/A",
        "utime": mod.updated_at or "N/A",
    })

    # Ensure N/A for empty values that are expected to be strings in templates
    for key, value in data.items():
        if value is None:
            data[key] = "N/A"

    return template.format_map(_SafeDict(data))


class _SafeDict(dict):
    def __missing__(self, key: str) -> str:
        return "N/A"
