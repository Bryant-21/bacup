from dataclasses import dataclass
import html
from typing import Any, Dict, List, Optional

import logging
import requests

from .utils import bethesda_image_url, iso_from_epoch

logger = logging.getLogger("tools.verified_creations")


@dataclass(frozen=True)
class Mod:
    mod_id: str
    title: str
    overview: Optional[str]
    description: Optional[str]
    product: str
    product_title: Optional[str]
    content_type: Optional[str]
    hardware_platforms: List[str]
    categories: List[str]
    author_displayname: Optional[str]
    author_buid: Optional[str]
    author_verified: bool
    author_official: bool
    published_buid: Optional[str]
    updated_buid: Optional[str]
    created_at: Optional[str]
    published_at: Optional[str]
    first_published_at: Optional[str]
    updated_at: Optional[str]
    status: Optional[str]
    moderation_state: Optional[str]
    error_info: Optional[str]
    deleted: bool
    published: bool
    moderated: bool
    beta: bool
    maintenance: bool
    restricted: bool
    use_high_report_threshold: bool
    marketplace: bool
    review_revision: bool
    author_price: Optional[Any]
    required_dlc: List[Any]
    required_mods: List[str]
    achievement_friendly: bool
    default_locale: Optional[str]
    supported_locales: List[str]
    release_notes: List[Dict[str, Any]]
    stats: Dict[str, Any]
    custom_data: Optional[Any]
    catalog_info: List[Any]
    prices: List[Dict[str, Any]]
    preview_image_url: Optional[str]
    cover_image_url: Optional[str]
    preview_image: Dict[str, Any]
    cover_image: Dict[str, Any]
    screenshot_images: List[Dict[str, Any]]
    videos: List[Any]
    details_url: Optional[str]


def _extract_image_url(image_obj: Optional[Dict]) -> Optional[str]:
    if not image_obj:
        return None
    for key in ("url", "uri", "path", "link"):
        value = image_obj.get(key)
        if isinstance(value, str) and value:
            return value
    s3bucket = image_obj.get("s3bucket")
    s3key = image_obj.get("s3key")
    if isinstance(s3bucket, str) and isinstance(s3key, str) and s3bucket and s3key:
        return bethesda_image_url(s3bucket, s3key)
    for value in image_obj.values():
        if isinstance(value, str) and value.startswith("http"):
            return value
    return None


def _extract_prices(item: Dict[str, Any]) -> List[Dict[str, Any]]:
    prices: List[Dict[str, Any]] = []
    catalog_info = item.get("catalog_info") or []
    if isinstance(catalog_info, list):
        for catalog in catalog_info:
            if not isinstance(catalog, dict):
                continue
            for price in catalog.get("prices") or []:
                if isinstance(price, dict):
                    prices.append(price)
    return prices


def parse_mod(item: Dict, mod_url_template: Optional[str] = None) -> Mod:
    mod_id = item.get("content_id") or ""
    product = item.get("product") or ""
    details_url = None
    if mod_id and product:
        if mod_url_template:
            try:
                details_url = mod_url_template.format(content_id=mod_id, product=product)
            except (KeyError, IndexError, ValueError):
                details_url = None
        else:
            details_url = f"https://creations.bethesda.net/en/{product.lower()}/details/{mod_id}"

    description_raw = item.get("description")
    overview_raw = item.get("overview")

    # Fallback logic: if description is missing, use overview. 
    # If overview is missing, use description (though overview is usually shorter).
    if not description_raw and overview_raw:
        description_raw = overview_raw
    if not overview_raw and description_raw:
        overview_raw = description_raw

    return Mod(
        mod_id=mod_id,
        title=html.unescape(item.get("title") or ""),
        overview=html.unescape(overview_raw) if overview_raw else None,
        description=html.unescape(description_raw) if description_raw else None,
        product=product,
        product_title=html.unescape(item.get("product_title")) if item.get("product_title") else None,
        content_type=item.get("content_type"),
        hardware_platforms=item.get("hardware_platforms") or [],
        categories=item.get("categories") or [],
        author_displayname=html.unescape(item.get("author_displayname")) if item.get("author_displayname") else None,
        author_buid=item.get("author_buid"),
        author_verified=bool(item.get("author_verified")),
        author_official=bool(item.get("author_official")),
        published_buid=item.get("published_buid"),
        updated_buid=item.get("updated_buid"),
        created_at=iso_from_epoch(item.get("ctime")),
        published_at=iso_from_epoch(item.get("ptime")),
        first_published_at=iso_from_epoch(item.get("first_ptime")),
        updated_at=iso_from_epoch(item.get("utime")),
        status=item.get("status"),
        moderation_state=item.get("moderation_state"),
        error_info=item.get("error_info"),
        deleted=bool(item.get("deleted")),
        published=bool(item.get("published")),
        moderated=bool(item.get("moderated")),
        beta=bool(item.get("beta")),
        maintenance=bool(item.get("maintenance")),
        restricted=bool(item.get("restricted")),
        use_high_report_threshold=bool(item.get("use_high_report_threshold")),
        marketplace=bool(item.get("marketplace")),
        review_revision=bool(item.get("review_revision")),
        author_price=item.get("author_price"),
        required_dlc=item.get("required_dlc") or [],
        required_mods=item.get("required_mods") or [],
        achievement_friendly=bool(item.get("achievement_friendly")),
        default_locale=item.get("default_locale"),
        supported_locales=item.get("supported_locales") or [],
        release_notes=item.get("release_notes") or [],
        stats=item.get("stats") or {},
        custom_data=item.get("custom_data"),
        catalog_info=item.get("catalog_info") or [],
        prices=_extract_prices(item),
        preview_image_url=_extract_image_url(item.get("preview_image")),
        cover_image_url=_extract_image_url(item.get("cover_image")),
        preview_image=item.get("preview_image") or {},
        cover_image=item.get("cover_image") or {},
        screenshot_images=item.get("screenshot_images") or [],
        videos=item.get("videos") or [],
        details_url=details_url,
    )


CORE_URL = "https://cdn.bethesda.net/data/core"
CONTENT_URL = "https://api.bethesda.net/ugcmods/v2/content"


class BethesdaClient:
    """Reads the public Creations listing; the bnet key is the public one from the core payload."""

    def __init__(self, timeout_seconds: float = 30.0) -> None:
        self.core_url = CORE_URL
        self.content_url = CONTENT_URL
        self._bnet_key: Optional[str] = None
        self.timeout_seconds = timeout_seconds
        self.session = requests.Session()

    def get_bnet_key(self) -> str:
        if self._bnet_key:
            return self._bnet_key
        response = self.session.get(self.core_url, timeout=self.timeout_seconds)
        response.raise_for_status()
        payload = response.json()
        key = payload.get("ugc", {}).get("bnetKey")
        if not key:
            raise RuntimeError("Unable to resolve ugc.bnetKey from core payload")
        self._bnet_key = key
        return key

    def _headers(self, product: str) -> Dict[str, str]:
        return {
            "accept": "application/json",
            "x-bnet-product": product,
            "x-bnet-key": self.get_bnet_key(),
        }

    def fetch_content(self, product: str, page: int, size: int) -> Dict:
        # `sort` is required but ignored: the listing is always newest-created first.
        # `marketplace` narrows it to the paid Creations catalog (BGS Creations included).
        params = {
            "product": product,
            "sort": "-ctime",
            "marketplace": "true",
            "time_period": "all_time",
            "size": size,
            "page": page,
            "counts_platform": "ALL",
        }
        response = self.session.get(
            self.content_url,
            params=params,
            headers=self._headers(product),
            timeout=self.timeout_seconds,
        )
        logger.debug("RAW_RESPONSE: %s", response.text)
        response.raise_for_status()
        payload = response.json()
        return payload.get("platform", {}).get("response", {})

    def fetch_mods(self, product: str, page: int, size: int) -> List[Mod]:
        items = self.fetch_content(product, page, size).get("data") or []
        mods: List[Mod] = []
        for item in items:
            if isinstance(item, dict) and isinstance(item.get("data"), dict):
                mods.append(parse_mod(item["data"]))
            else:
                mods.append(parse_mod(item))
        for mod in mods:
            logger.debug("MOD: %s %s", mod.mod_id, mod.title)
        return mods
