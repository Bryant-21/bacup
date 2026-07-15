from __future__ import annotations

import os
from pathlib import Path
from typing import Callable, Iterable

from creation_lib.db.native_runtime import Database


CATALOG_FILENAME = "fo4_target_assets.sqlite3"


def _conversion_data_root() -> Path:
    # LOCALAPPDATA read is an OS-level path (allowed by the env-read policy),
    # not project config. The catalog is a large derived index of the FO4 BA2s;
    # it lives in this writable cache root, never in git or packaged resources.
    local_app_data = os.environ.get("LOCALAPPDATA")
    root = Path(local_app_data) if local_app_data else Path.home() / "AppData" / "Local"
    return root / "modkit21" / "conversion"


def default_target_asset_catalog() -> Path:
    return _conversion_data_root() / CATALOG_FILENAME


def default_target_asset_cache_dir() -> Path:
    return _conversion_data_root() / "target_assets"


def _catalog_is_current(catalog_path: Path) -> bool:
    if not catalog_path.is_file():
        return False
    from bacup_lib.native_runtime import load_native_module

    expected = int(load_native_module().conversion_target_asset_catalog_schema_version())
    try:
        with Database.open(str(catalog_path), mode="ro") as db:
            row = db.query_one(
                "SELECT value FROM metadata WHERE key = 'schema_version'"
            )
    except (RuntimeError, OSError):
        return False
    return bool(row) and str(row["value"]) == str(expected)


def ensure_target_asset_catalog(
    fo4_data_dir: str | Path,
    catalog_path: str | Path | None = None,
    *,
    game_build: str = "",
    log: Callable[[str], None] | None = None,
) -> Path:
    """Build the FO4 target-asset catalog from the official BA2s if it is
    missing or built against an older schema, then return its path.

    The catalog is a derived index of the FO4 BA2s (always installed), so it is
    reproduced locally on demand rather than shipped. FO4-update staleness (BA2
    size drift) self-heals inside the store at runtime and does NOT trigger a
    rebuild here; only a missing file or schema bump does.
    """
    catalog_path = Path(catalog_path or default_target_asset_catalog())
    if _catalog_is_current(catalog_path):
        return catalog_path

    if log:
        log(
            "Building FO4 target-asset catalog from official BA2s "
            f"(one-time, a few minutes): {catalog_path}"
        )
    catalog_path.parent.mkdir(parents=True, exist_ok=True)
    from bacup_lib.native_runtime import load_native_module

    load_native_module().conversion_build_target_asset_catalog(
        str(fo4_data_dir), str(catalog_path), game_build
    )
    if log:
        log(f"FO4 target-asset catalog ready: {catalog_path}")
    return catalog_path


def normalize_target_asset_path(path: str | Path) -> str:
    value = str(path).replace("\\", "/").strip().strip("/")
    lowered = value.casefold()
    marker = "/data/"
    if marker in lowered:
        value = value[lowered.rfind(marker) + len(marker) :]
    elif lowered.startswith("data/"):
        value = value[5:]
    return value.strip("/").casefold()


class TargetAssetStore:
    def __init__(
        self,
        *,
        target_data_dir: str | Path,
        catalog_path: str | Path | None = None,
        cache_dir: str | Path | None = None,
        overlay_dir: str | Path | None = None,
    ) -> None:
        from bacup_lib.native_runtime import load_native_module

        native = load_native_module()
        native_type = getattr(native, "TargetAssetStore", None)
        if native_type is None:
            raise RuntimeError(
                "conversion native runtime does not expose TargetAssetStore; rebuild the native extension"
            )
        self.catalog_path = Path(catalog_path or default_target_asset_catalog())
        self.cache_dir = Path(cache_dir or default_target_asset_cache_dir())
        self.overlay_dir = Path(overlay_dir) if overlay_dir else None
        self._native = native_type(
            str(Path(target_data_dir)),
            str(self.catalog_path),
            str(self.cache_dir),
            str(self.overlay_dir) if self.overlay_dir else None,
        )

    def has_asset(self, path: str | Path) -> bool:
        return bool(self._native.has_asset(str(path)))

    def list_assets(self, *, prefix: str = "", suffix: str = "") -> list[str]:
        return list(self._native.list_assets(prefix, suffix))

    def dependency_closure(self, paths: Iterable[str | Path]) -> list[str]:
        return list(self._native.dependency_closure([str(path) for path in paths]))

    def materialize(self, path: str | Path) -> Path | None:
        result = self._native.materialize(str(path))
        return Path(result) if result else None

    def materialize_many(
        self,
        paths: Iterable[str | Path],
        *,
        include_dependencies: bool = False,
    ) -> list[Path]:
        return [
            Path(path)
            for path in self._native.materialize_many(
                [str(path) for path in paths], include_dependencies
            )
        ]

    @property
    def cache_data_root(self) -> Path:
        return Path(self._native.cache_data_root)

    @property
    def asset_count(self) -> int:
        return int(self._native.asset_count)

    @property
    def warnings(self) -> list[str]:
        return list(self._native.warnings)

    def stats(self) -> dict[str, int]:
        return {str(key): int(value) for key, value in self._native.stats().items()}


def build_target_asset_store(
    *,
    target_data_dir: str | Path | None,
    catalog_path: str | Path | None = None,
    cache_dir: str | Path | None = None,
    overlay_dir: str | Path | None = None,
) -> TargetAssetStore | None:
    if target_data_dir is None:
        return None
    return TargetAssetStore(
        target_data_dir=target_data_dir,
        catalog_path=catalog_path,
        cache_dir=cache_dir,
        overlay_dir=overlay_dir,
    )
