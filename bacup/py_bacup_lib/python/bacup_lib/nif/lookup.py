"""Secondary asset lookup from the NIF index database.

After the walker finds NIF mesh assets, this class queries the
prebuilt NIF index ({game}_nifs.db) to find associated textures,
materials, and behavior graphs embedded in those NIFs.
"""
from __future__ import annotations

import logging
import os

from bacup_lib.asset_paths import normalize_asset_source_path
from bacup_lib.models import AssetRef
from creation_lib.db.native_runtime import Database

_log = logging.getLogger("conversion.nif_lookup")


class NifIndexLookup:
    """Query the NIF index DB for textures, materials, and behaviors."""

    def __init__(self, db_path: str, game: str):
        self._db_path = db_path
        self._game = game
        self._available = os.path.isfile(db_path)
        self._conn: Database | None = None
        if not self._available:
            _log.warning("NIF index DB not found: %s — secondary extraction disabled", db_path)

    def _get_conn(self) -> Database | None:
        if not self._available:
            return None
        if self._conn is None:
            self._conn = Database.open(self._db_path, "ro")
        return self._conn

    def close(self) -> None:
        conn = self._conn
        if conn is not None:
            conn.close()
            self._conn = None

    def _nif_id(self, nif_path: str) -> str:
        """Build the nif_id key: {game}/{path} (lowercased for matching)."""
        return f"{self._game}/{nif_path.replace(chr(92), '/')}".lower()

    def _query_by_nif_path(self, table: str, column: str, nif_path: str) -> list[str]:
        """Generic query: get column values from a NIF-related table by NIF path."""
        conn = self._get_conn()
        if conn is None:
            return []
        nif_id = self._nif_id(nif_path)
        try:
            rows = conn.query_all(
                f"SELECT {column} AS v FROM {table} "
                f"WHERE nif_id = ?",
                [nif_id],
            )
            if not rows:
                # Fallback: match by path column alone (handles game prefix mismatch)
                norm = nif_path.replace("\\", "/").lower()
                rows = conn.query_all(
                    f"SELECT t.{column} AS v FROM {table} t "
                    f"JOIN nifs n ON t.nif_id = n.id "
                    f"WHERE n.path = ?",
                    [norm],
                )
            return [r["v"] for r in rows if r.get("v") is not None]
        except Exception:
            # Table may be absent in minimal NIF indexes (e.g. nif_behavior_refs).
            return []

    def get_textures(self, nif_path: str) -> list[str]:
        """Get texture paths referenced by a NIF."""
        return self._query_by_nif_path("nif_textures", "texture_path", nif_path)

    def get_materials(self, nif_path: str) -> list[str]:
        """Get material paths (.bgsm/.bgem) referenced by a NIF."""
        return self._query_by_nif_path("nif_materials", "material_path", nif_path)

    def get_behaviors(self, nif_path: str) -> list[str]:
        """Get behavior graph paths referenced by a NIF."""
        return self._query_by_nif_path("nif_behavior_refs", "behavior_path", nif_path)

    def get_secondary_assets(self, nif_path: str) -> list[AssetRef]:
        """Get all secondary assets (textures, materials, behaviors) for a NIF.

        Returns AssetRef objects ready to be added to the dependency graph.
        Resolution (disk path check) is NOT done here — the walker handles that.
        """
        assets: list[AssetRef] = []
        for tex in self.get_textures(nif_path):
            assets.append(AssetRef("texture", normalize_asset_source_path(tex)))
        for mat in self.get_materials(nif_path):
            assets.append(AssetRef("material", normalize_asset_source_path(mat)))
        for beh in self.get_behaviors(nif_path):
            assets.append(AssetRef("behavior", normalize_asset_source_path(beh)))
        return assets
