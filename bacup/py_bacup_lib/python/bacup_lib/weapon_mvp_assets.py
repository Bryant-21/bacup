from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
from typing import Iterable, Mapping

from bacup_lib.asset_paths import normalize_asset_source_path
from bacup_lib.models import AssetRef


_FORM_KEY_ID_PLUGIN_RE = re.compile(
    r"^\s*(?:0x)?(?P<form_id>[0-9a-fA-F]{1,8})\s*[@:]\s*(?P<plugin>[^@:]+?)\s*$"
)
_FORM_KEY_PLUGIN_ID_RE = re.compile(
    r"^\s*(?P<plugin>[^@:]+?)\s*[@:]\s*(?:0x)?(?P<form_id>[0-9a-fA-F]{1,8})\s*$"
)
_ASSET_ROOTS = {
    "material": "materials",
    "nif": "meshes",
    "texture": "textures",
}
_SUPPORTED_PAIRS = {("fnv", "fo4"), ("skyrimse", "fo4")}
_MELEE_POLICIES = {"bulk_melee_v1", "mvp_melee_v1"}


@dataclass(frozen=True)
class WeaponMvpAssetActivator:
    source_plugin: str
    source_form_id: int
    source_record_signature: str
    asset: tuple[str, str]

    def matches(self, candidate: AssetRef) -> bool:
        provenance = getattr(candidate, "provenance", None)
        signature = str(getattr(provenance, "added_by_record_sig", "") or "").upper()
        if signature != self.source_record_signature:
            return False
        form_key = str(getattr(provenance, "added_by_record_fk", "") or "")
        identity = _form_key_identity(form_key)
        return (
            identity
            == (self.source_plugin.casefold(), self.source_form_id & 0x00FF_FFFF)
            and _asset_key(candidate) == self.asset
        )


@dataclass(frozen=True)
class WeaponMvpAssetClaim:
    source_form_key: str
    editor_id: str
    owner_form_key: str
    source_record_signature: str
    source_field: str
    model_kind: str
    weapon_role: str
    asset: tuple[str, str]


@dataclass(frozen=True)
class WeaponMvpAssetClosure:
    label: str
    activators: tuple[WeaponMvpAssetActivator, ...]
    assets: frozenset[tuple[str, str]]
    claims: tuple[WeaponMvpAssetClaim, ...] = ()
    source_form_key: str = ""
    editor_id: str = ""

    def allows(self, asset: AssetRef) -> bool:
        return _asset_key(asset) in self.assets

    def activates(self, asset: AssetRef) -> bool:
        return any(activator.matches(asset) for activator in self.activators)

    def activating_assets(self, assets: Iterable[AssetRef]) -> tuple[AssetRef, ...]:
        candidates = tuple(assets)
        matched: list[AssetRef] = []
        for activator in self.activators:
            candidate = next(
                (asset for asset in candidates if activator.matches(asset)),
                None,
            )
            if candidate is None:
                return ()
            matched.append(candidate)
        return tuple(matched)

    def is_activated_by(self, assets: Iterable[AssetRef]) -> bool:
        return len(self.activating_assets(assets)) == len(self.activators)

    def missing_entries(
        self, assets: Iterable[AssetRef]
    ) -> tuple[tuple[str, str], ...]:
        existing = {_asset_key(asset) for asset in assets}
        return tuple(sorted(self.assets - existing))


@dataclass(frozen=True)
class WeaponMvpAssetCandidate:
    source_form_key: str
    editor_id: str
    status: str
    reason_code: str
    weapon_role: str
    policy: str
    target_profile: str
    claims: tuple[WeaponMvpAssetClaim, ...]


@dataclass(frozen=True)
class WeaponMvpAssetRejection:
    source_form_key: str
    editor_id: str
    reason_code: str


@dataclass(frozen=True)
class WeaponMvpAssetConflict:
    asset: tuple[str, str]
    roles: tuple[str, ...]
    policies: tuple[str, ...]
    source_form_keys: tuple[str, ...]
    reason_code: str = "mixed_weapon_roles"


@dataclass(frozen=True)
class WeaponMvpAssetPlan:
    candidates: tuple[WeaponMvpAssetCandidate, ...]
    admitted: tuple[WeaponMvpAssetClosure, ...]
    rejected: tuple[WeaponMvpAssetRejection, ...]
    conflicts: tuple[WeaponMvpAssetConflict, ...]

    @property
    def candidate_count(self) -> int:
        return len(self.candidates)

    @property
    def admitted_count(self) -> int:
        return len(self.admitted)

    @property
    def rejected_count(self) -> int:
        return len(self.rejected)

    @property
    def conflict_count(self) -> int:
        return len(self.conflicts)

    @property
    def assets(self) -> frozenset[tuple[str, str]]:
        return frozenset(asset for closure in self.admitted for asset in closure.assets)

    @property
    def admitted_claims(self) -> tuple[WeaponMvpAssetClaim, ...]:
        return tuple(
            sorted(
                (claim for closure in self.admitted for claim in closure.claims),
                key=_claim_sort_key,
            )
        )

    def allows(self, asset: AssetRef) -> bool:
        return _asset_key(asset) in self.assets


def canonical_weapon_asset_key(asset_type: str, source_path: str) -> tuple[str, str]:
    normalized_type = str(asset_type or "").casefold()
    normalized_path = normalize_asset_source_path(str(source_path or "")).casefold()
    expected_root = _ASSET_ROOTS.get(normalized_type)
    if expected_root and normalized_path.split("/", 1)[0] != expected_root:
        normalized_path = f"{expected_root}/{normalized_path}"
    return normalized_type, normalized_path


def _asset_key(asset: AssetRef) -> tuple[str, str]:
    return canonical_weapon_asset_key(
        str(getattr(asset, "asset_type", "") or ""),
        str(getattr(asset, "source_path", "") or ""),
    )


def _form_key_identity(form_key: str) -> tuple[str, int] | None:
    match = _FORM_KEY_ID_PLUGIN_RE.fullmatch(
        form_key
    ) or _FORM_KEY_PLUGIN_ID_RE.fullmatch(form_key)
    if match is None:
        return None
    return Path(match.group("plugin")).name.casefold(), (
        int(match.group("form_id"), 16) & 0x00FF_FFFF
    )


def _source_form_key(row: Mapping[str, object]) -> str:
    form_key = str(row.get("source_form_key") or "")
    if form_key:
        return form_key
    plugin = str(row.get("source_plugin") or "")
    local_form_id = row.get("source_local_form_id")
    if plugin and isinstance(local_form_id, int):
        return f"{local_form_id & 0x00FF_FFFF:06X}@{plugin}"
    return ""


def _normalized_role(row: Mapping[str, object]) -> str:
    return str(row.get("role") or row.get("weapon_role") or "unknown").casefold()


def _claim_sort_key(claim: WeaponMvpAssetClaim) -> tuple[str, ...]:
    return (
        claim.asset[0],
        claim.asset[1],
        claim.source_form_key.casefold(),
        claim.owner_form_key.casefold(),
        claim.model_kind,
        claim.source_field,
    )


def _metadata_model_claims(
    row: Mapping[str, object],
) -> tuple[WeaponMvpAssetClaim, ...]:
    source_form_key = _source_form_key(row)
    editor_id = str(row.get("editor_id") or "")
    source_signature = str(row.get("source_signature") or "WEAP").upper()
    weapon_role = _normalized_role(row)
    authoritative_models = "world_model" in row or "first_person_model" in row
    model_fields: list[tuple[str, str, str, str]] = []
    if authoritative_models:
        model_fields.append(
            ("world_model", "world", source_form_key, source_signature)
        )
        first_person_owner = str(row.get("first_person_stat_form_key") or source_form_key)
        model_fields.append(
            (
                "first_person_model",
                "first_person",
                first_person_owner,
                "STAT" if row.get("first_person_stat_form_key") else source_signature,
            )
        )
    else:
        model_fields.extend(
            [
                ("base_model", "world", source_form_key, source_signature),
                ("model_mod1", "attachment_1", source_form_key, source_signature),
                ("model_mod2", "attachment_2", source_form_key, source_signature),
                ("model_mod3", "attachment_3", source_form_key, source_signature),
            ]
        )

    claims = []
    for field_name, model_kind, owner_form_key, owner_signature in model_fields:
        source_path = str(row.get(field_name) or "")
        if not source_path:
            continue
        asset = canonical_weapon_asset_key("nif", source_path)
        if not asset[1]:
            continue
        claims.append(
            WeaponMvpAssetClaim(
                source_form_key=source_form_key,
                editor_id=editor_id,
                owner_form_key=owner_form_key,
                source_record_signature=owner_signature,
                source_field=field_name,
                model_kind=model_kind,
                weapon_role=weapon_role,
                asset=asset,
            )
        )
    return tuple(sorted(set(claims), key=_claim_sort_key))


def _candidate(row: Mapping[str, object]) -> WeaponMvpAssetCandidate:
    return WeaponMvpAssetCandidate(
        source_form_key=_source_form_key(row),
        editor_id=str(row.get("editor_id") or ""),
        status=str(row.get("status") or "").casefold(),
        reason_code=str(row.get("reason_code") or ""),
        weapon_role=_normalized_role(row),
        policy=str(row.get("policy") or row.get("policy_id") or "").casefold(),
        target_profile=str(row.get("target_profile") or "").casefold(),
        claims=_metadata_model_claims(row),
    )


def _candidate_sort_key(candidate: WeaponMvpAssetCandidate) -> tuple[str, ...]:
    return (
        candidate.source_form_key.casefold(),
        candidate.editor_id.casefold(),
        candidate.status,
        candidate.reason_code,
    )


def _is_model_less_unarmed(candidate: WeaponMvpAssetCandidate) -> bool:
    return candidate.target_profile == "unarmed"


def _identity_matches_activator(
    candidate: WeaponMvpAssetCandidate,
    activator: WeaponMvpAssetActivator,
) -> bool:
    return _form_key_identity(candidate.source_form_key) == (
        activator.source_plugin.casefold(),
        activator.source_form_id & 0x00FF_FFFF,
    )


def _fixture_for_candidate(
    source_game: str,
    target_game: str,
    candidate: WeaponMvpAssetCandidate,
) -> WeaponMvpAssetClosure | None:
    fixture = _CLOSURES_BY_PAIR.get((source_game.casefold(), target_game.casefold()))
    if fixture is None:
        return None
    source_activators = [
        activator
        for activator in fixture.activators
        if activator.source_record_signature == "WEAP"
    ]
    if not source_activators or not any(
        _identity_matches_activator(candidate, activator)
        for activator in source_activators
    ):
        return None
    model_assets = {claim.asset for claim in candidate.claims}
    if not all(activator.asset in model_assets for activator in fixture.activators):
        return None
    return fixture


def _owner_asset_claims(
    candidate: WeaponMvpAssetCandidate,
    assets: Iterable[AssetRef],
) -> tuple[WeaponMvpAssetClaim, ...]:
    owners = {
        identity
        for identity in (
            _form_key_identity(claim.owner_form_key) for claim in candidate.claims
        )
        if identity is not None
    }
    claims = []
    for asset_ref in assets:
        asset = _asset_key(asset_ref)
        if asset[0] not in _ASSET_ROOTS or not asset[1]:
            continue
        provenance = getattr(asset_ref, "provenance", None)
        owner_form_key = str(getattr(provenance, "added_by_record_fk", "") or "")
        if _form_key_identity(owner_form_key) not in owners:
            continue
        claims.append(
            WeaponMvpAssetClaim(
                source_form_key=candidate.source_form_key,
                editor_id=candidate.editor_id,
                owner_form_key=owner_form_key,
                source_record_signature=str(
                    getattr(provenance, "added_by_record_sig", "") or ""
                ).upper(),
                source_field=str(getattr(provenance, "added_by_field", "") or ""),
                model_kind="dependency",
                weapon_role=candidate.weapon_role,
                asset=asset,
            )
        )
    return tuple(sorted(set(claims), key=_claim_sort_key))


def _fixture_dependency_claims(
    candidate: WeaponMvpAssetCandidate,
    fixture: WeaponMvpAssetClosure | None,
) -> tuple[WeaponMvpAssetClaim, ...]:
    if fixture is None:
        return ()
    model_assets = {claim.asset for claim in candidate.claims}
    return tuple(
        WeaponMvpAssetClaim(
            source_form_key=candidate.source_form_key,
            editor_id=candidate.editor_id,
            owner_form_key=candidate.source_form_key,
            source_record_signature="WEAP",
            source_field="fixture_dependency",
            model_kind="dependency",
            weapon_role=candidate.weapon_role,
            asset=asset,
        )
        for asset in sorted(fixture.assets - model_assets)
    )


def _rejection(
    candidate: WeaponMvpAssetCandidate, reason_code: str
) -> WeaponMvpAssetRejection:
    return WeaponMvpAssetRejection(
        source_form_key=candidate.source_form_key,
        editor_id=candidate.editor_id,
        reason_code=reason_code,
    )


def build_weapon_mvp_asset_plan(
    source_game: str,
    target_game: str,
    assets: Iterable[AssetRef],
    metadata_rows: Iterable[Mapping[str, object]],
) -> WeaponMvpAssetPlan:
    asset_refs = tuple(assets)
    candidates = tuple(sorted((_candidate(row) for row in metadata_rows), key=_candidate_sort_key))
    if (source_game.casefold(), target_game.casefold()) not in _SUPPORTED_PAIRS:
        return WeaponMvpAssetPlan(candidates=(), admitted=(), rejected=(), conflicts=())

    asset_index: dict[tuple[str, str], list[AssetRef]] = {}
    for asset_ref in asset_refs:
        asset_index.setdefault(_asset_key(asset_ref), []).append(asset_ref)

    rejected: list[WeaponMvpAssetRejection] = []
    prelim: list[tuple[WeaponMvpAssetCandidate, WeaponMvpAssetClosure]] = []
    for candidate in candidates:
        if not candidate.source_form_key:
            rejected.append(_rejection(candidate, "missing_source_form_key"))
            continue
        if candidate.status != "admitted":
            rejected.append(
                _rejection(candidate, candidate.reason_code or "not_admitted_melee")
            )
            continue
        if candidate.weapon_role != "melee":
            rejected.append(_rejection(candidate, "not_melee_role"))
            continue
        if candidate.policy not in _MELEE_POLICIES:
            rejected.append(_rejection(candidate, "unsupported_melee_policy"))
            continue
        if not candidate.claims and not _is_model_less_unarmed(candidate):
            rejected.append(_rejection(candidate, "missing_required_model"))
            continue
        unresolved_models = [
            claim.asset
            for claim in candidate.claims
            if not any(
                str(getattr(asset_ref, "resolved_path", "") or "")
                for asset_ref in asset_index.get(claim.asset, ())
            )
        ]
        if unresolved_models:
            rejected.append(_rejection(candidate, "unresolved_required_model"))
            continue

        fixture = _fixture_for_candidate(source_game, target_game, candidate)
        claims = tuple(
            sorted(
                set(
                    candidate.claims
                    + _owner_asset_claims(candidate, asset_refs)
                    + _fixture_dependency_claims(candidate, fixture)
                ),
                key=_claim_sort_key,
            )
        )
        closure = WeaponMvpAssetClosure(
            label=candidate.editor_id or candidate.source_form_key,
            activators=(),
            assets=frozenset(claim.asset for claim in claims),
            claims=claims,
            source_form_key=candidate.source_form_key,
            editor_id=candidate.editor_id,
        )
        prelim.append((candidate, closure))

    path_roles: dict[tuple[str, str], set[str]] = {}
    path_policies: dict[tuple[str, str], set[str]] = {}
    path_owners: dict[tuple[str, str], set[str]] = {}
    for candidate in candidates:
        for claim in candidate.claims:
            path_roles.setdefault(claim.asset, set()).add(candidate.weapon_role)
            if candidate.policy:
                path_policies.setdefault(claim.asset, set()).add(candidate.policy)
            path_owners.setdefault(claim.asset, set()).add(candidate.source_form_key)
    conflict_paths = {
        asset
        for asset, roles in path_roles.items()
        if "melee" in roles
        and (
            any(role != "melee" for role in roles)
            or any(
                policy not in _MELEE_POLICIES
                for policy in path_policies.get(asset, ())
            )
        )
    }
    conflicts = tuple(
        WeaponMvpAssetConflict(
            asset=asset,
            roles=tuple(sorted(path_roles[asset])),
            policies=tuple(sorted(path_policies.get(asset, ()))),
            source_form_keys=tuple(sorted(path_owners[asset], key=str.casefold)),
            reason_code=(
                "mixed_weapon_roles"
                if any(role != "melee" for role in path_roles[asset])
                else "mixed_weapon_policies"
            ),
        )
        for asset in sorted(conflict_paths)
    )

    admitted = []
    for candidate, closure in prelim:
        candidate_conflicts = [
            conflict
            for conflict in conflicts
            if any(claim.asset == conflict.asset for claim in candidate.claims)
        ]
        if candidate_conflicts:
            rejected.append(_rejection(candidate, candidate_conflicts[0].reason_code))
            continue
        admitted.append(closure)

    return WeaponMvpAssetPlan(
        candidates=candidates,
        admitted=tuple(
            sorted(
                admitted,
                key=lambda closure: (
                    closure.source_form_key.casefold(),
                    closure.editor_id.casefold(),
                ),
            )
        ),
        rejected=tuple(
            sorted(
                rejected,
                key=lambda entry: (
                    entry.source_form_key.casefold(),
                    entry.editor_id.casefold(),
                    entry.reason_code,
                ),
            )
        ),
        conflicts=conflicts,
    )


FNV_HATCHET_MVP_ASSET_CLOSURE = WeaponMvpAssetClosure(
    label="FNV Hatchet",
    activators=(
        WeaponMvpAssetActivator(
            source_plugin="FalloutNV.esm",
            source_form_id=0x11A8E4,
            source_record_signature="WEAP",
            asset=("nif", "meshes/weapons/1handmelee/hatchet.nif"),
        ),
    ),
    assets=frozenset(
        {
            ("nif", "meshes/weapons/1handmelee/hatchet.nif"),
            ("texture", "textures/weapons/1handmelee/hatchet_d.dds"),
            ("texture", "textures/weapons/1handmelee/hatchet_n.dds"),
        }
    ),
)


SKYRIM_STEEL_BATTLEAXE_MVP_ASSET_CLOSURE = WeaponMvpAssetClosure(
    label="Skyrim steel battleaxe",
    activators=(
        WeaponMvpAssetActivator(
            source_plugin="Skyrim.esm",
            source_form_id=0x013984,
            source_record_signature="WEAP",
            asset=("nif", "meshes/weapons/steel/steelbattleaxe.nif"),
        ),
        WeaponMvpAssetActivator(
            source_plugin="Skyrim.esm",
            source_form_id=0x020E27,
            source_record_signature="STAT",
            asset=("nif", "meshes/weapons/steel/1stpersonsteelbattleaxe.nif"),
        ),
    ),
    assets=frozenset(
        {
            ("nif", "meshes/weapons/steel/steelbattleaxe.nif"),
            ("nif", "meshes/weapons/steel/1stpersonsteelbattleaxe.nif"),
            ("texture", "textures/blood/bloodedge01.dds"),
            ("texture", "textures/blood/bloodedge01add.dds"),
            ("texture", "textures/blood/bloodedge01_n.dds"),
            ("texture", "textures/cubemaps/eyecubemap.dds"),
            ("texture", "textures/cubemaps/shinydull_e.dds"),
            ("texture", "textures/weapons/steel/steelbattleaxe.dds"),
            ("texture", "textures/weapons/steel/steelbattleaxe_n.dds"),
            ("texture", "textures/weapons/steel/steelbattleaxe_m.dds"),
        }
    ),
)


_CLOSURES_BY_PAIR = {
    ("fnv", "fo4"): FNV_HATCHET_MVP_ASSET_CLOSURE,
    ("skyrimse", "fo4"): SKYRIM_STEEL_BATTLEAXE_MVP_ASSET_CLOSURE,
}


def weapon_mvp_asset_closure(
    source_game: str,
    target_game: str,
    assets: Iterable[AssetRef],
) -> WeaponMvpAssetClosure | None:
    closure = _CLOSURES_BY_PAIR.get((source_game.casefold(), target_game.casefold()))
    if closure is None:
        return None
    return closure if closure.is_activated_by(assets) else None
