from __future__ import annotations
from dataclasses import dataclass
from importlib.resources import files
from pathlib import Path
import yaml

from bacup_lib.source_pairs import SOURCE_PAIRS

VALID_FAMILIES = frozenset(
    {"Meshes", "Materials", "Textures", "Terrain", "LOD", "Animations", "Scripts", "Sounds"}
)
FAMILY_SENTINELS = frozenset({"ALL", "NONE"})


def bundled_upgrade_manifest_path() -> Path:
    resource = files("bacup_lib").joinpath(
        "resources", "conversion", "upgrade_manifest.yaml"
    )
    return Path(str(resource))


@dataclass(frozen=True)
class UpgradeVersion:
    id: str
    families_by_conversion: tuple[tuple[str, tuple[str, ...]], ...]
    force_regen_by_conversion: tuple[tuple[str, bool], ...] = ()
    notes_by_conversion: tuple[tuple[str, tuple[str, ...]], ...] = ()

    def notes_for_conversion(self, conversion_id: str) -> tuple[str, ...]:
        return next(
            (notes for key, notes in self.notes_by_conversion if key == conversion_id),
            (),
        )

    def families_for_conversion(self, conversion_id: str) -> tuple[str, ...]:
        families = next(
            (
                scoped
                for key, scoped in self.families_by_conversion
                if key == conversion_id
            ),
            None,
        )
        if families is None:
            raise ValueError(
                f"version {self.id!r} has no families for conversion "
                f"{conversion_id!r}; use [NONE] for no changes"
            )
        return () if "NONE" in families else families

    def force_regen_for_conversion(self, conversion_id: str) -> bool:
        return next(
            (
                scoped
                for key, scoped in self.force_regen_by_conversion
                if key == conversion_id
            ),
            False,
        )

@dataclass(frozen=True)
class UpgradeManifest:
    current: str
    versions: tuple[UpgradeVersion, ...]  # oldest -> newest

    def index_of(self, version_id: str) -> int | None:
        for i, v in enumerate(self.versions):
            if v.id == version_id:
                return i
        return None

def load_upgrade_manifest(path: Path) -> UpgradeManifest:
    data = yaml.safe_load(Path(path).read_text(encoding="utf-8"))
    parsed_versions = []
    for v in data["versions"]:
        legacy_fields = {"families", "force_regen", "notes"}.intersection(v)
        if legacy_fields:
            fields = ", ".join(sorted(legacy_fields))
            raise ValueError(
                f"legacy global field(s) {fields} are not supported in version "
                f"{v['id']!r}; use per-conversion mappings"
            )
        raw_notes_by_conversion = v.get("notes_by_conversion") or {}
        if not isinstance(raw_notes_by_conversion, dict):
            raise ValueError(
                f"notes_by_conversion must be a mapping in version {v['id']!r}"
            )
        notes_by_conversion = []
        for conversion_id, notes in raw_notes_by_conversion.items():
            if not isinstance(notes, list):
                raise ValueError(
                    f"notes_by_conversion[{conversion_id!r}] must be a list "
                    f"in version {v['id']!r}"
                )
            notes_by_conversion.append(
                (str(conversion_id), tuple(str(note) for note in notes))
            )
        raw_families_by_conversion = v.get("families_by_conversion")
        if not isinstance(raw_families_by_conversion, dict):
            raise ValueError(
                f"families_by_conversion is required and must be a mapping "
                f"in version {v['id']!r}"
            )
        families_by_conversion = []
        for conversion_id, families in raw_families_by_conversion.items():
            if not isinstance(families, list):
                raise ValueError(
                    f"families_by_conversion[{conversion_id!r}] must be a list "
                    f"in version {v['id']!r}"
                )
            families_by_conversion.append(
                (str(conversion_id), tuple(str(family) for family in families))
            )
        raw_force_regen_by_conversion = v.get("force_regen_by_conversion") or {}
        if not isinstance(raw_force_regen_by_conversion, dict):
            raise ValueError(
                f"force_regen_by_conversion must be a mapping in version {v['id']!r}"
            )
        force_regen_by_conversion = []
        for conversion_id, scoped_force_regen in raw_force_regen_by_conversion.items():
            if not isinstance(scoped_force_regen, bool):
                raise ValueError(
                    f"force_regen_by_conversion[{conversion_id!r}] must be true or false "
                    f"in version {v['id']!r}"
                )
            force_regen_by_conversion.append(
                (str(conversion_id), scoped_force_regen)
            )
        parsed_versions.append(
            UpgradeVersion(
                str(v["id"]),
                families_by_conversion=tuple(families_by_conversion),
                force_regen_by_conversion=tuple(force_regen_by_conversion),
                notes_by_conversion=tuple(notes_by_conversion),
            )
        )
    versions = tuple(parsed_versions)
    for v in versions:
        family_sets = tuple(
            (f"families_by_conversion[{conversion_id!r}]", families)
            for conversion_id, families in v.families_by_conversion
        )
        for field_name, families in family_sets:
            if "NONE" in families and len(families) != 1:
                raise ValueError(
                    f"NONE cannot be combined with other families in {field_name} "
                    f"for version {v.id!r}"
                )
            for fam in families:
                if fam not in FAMILY_SENTINELS and fam not in VALID_FAMILIES:
                    raise ValueError(f"unknown family {fam!r} in version {v.id!r}")
        configured = {
            conversion_id for conversion_id, _families in v.families_by_conversion
        }
        missing = set(SOURCE_PAIRS).difference(configured)
        if missing:
            raise ValueError(
                f"version {v.id!r} is missing families_by_conversion entries for: "
                f"{', '.join(sorted(missing))}; use [NONE] for no changes"
            )
        unknown = configured.difference(SOURCE_PAIRS)
        if unknown:
            raise ValueError(
                f"version {v.id!r} has unknown conversion entries: "
                f"{', '.join(sorted(unknown))}"
            )
    return UpgradeManifest(str(data["current"]), versions)


def resolve_family_union(
    manifest: UpgradeManifest,
    from_version: str | None,
    target_version: str,
    *,
    conversion_id: str,
) -> frozenset[str]:
    target_idx = manifest.index_of(target_version)
    if target_idx is None:
        raise ValueError(f"target version {target_version!r} not in manifest")
    if from_version is None:
        return frozenset({"ALL"})
    from_idx = manifest.index_of(from_version)
    if from_idx is None:
        return frozenset({"ALL"})            # unknown installed version -> full build
    if from_idx > target_idx:
        raise ValueError(f"downgrade {from_version!r} -> {target_version!r} not supported")
    families: set[str] = set()
    for v in manifest.versions[from_idx + 1 : target_idx + 1]:
        version_families = v.families_for_conversion(conversion_id)
        if "ALL" in version_families:
            return frozenset({"ALL"})
        families.update(version_families)
    # Script patches can change without a version bump, so a target that opts
    # into Scripts keeps that family repeatable even when already installed.
    if "Scripts" in manifest.versions[target_idx].families_for_conversion(
        conversion_id
    ):
        families.add("Scripts")
    return frozenset(families)


def requires_forced_regen(
    manifest: UpgradeManifest,
    from_version: str | None,
    target_version: str,
    *,
    conversion_id: str,
) -> bool:
    target_idx = manifest.index_of(target_version)
    if target_idx is None:
        raise ValueError(f"target version {target_version!r} not in manifest")
    from_idx = manifest.index_of(from_version) if from_version is not None else None
    if from_idx is not None and from_idx > target_idx:
        raise ValueError(f"downgrade {from_version!r} -> {target_version!r} not supported")
    start = 0 if from_idx is None else from_idx + 1
    return any(
        v.force_regen_for_conversion(conversion_id)
        for v in manifest.versions[start : target_idx + 1]
    )
