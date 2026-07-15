"""Python orchestration boundary for native texture conversion."""
from __future__ import annotations

import shutil
from pathlib import Path

from creation_lib.core.game_profiles import GameProfile
from creation_lib.material_tools import native_runtime
from creation_lib.textures.naming import convert_texture_name, detect_texture_role

from .compression import compression_for_role


def build_outputs(
    files_and_roles: list[tuple[Path, str | None]],
    output_dir: Path,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> list[dict[str, str]]:
    outputs: list[dict[str, str]] = []
    seen: set[tuple[str, str]] = set()
    roles = {role for _, role in files_and_roles if role is not None}
    is_fo76_to_fo4_bundle = (
        source_profile.id == "fo76"
        and target_profile.id == "fo4"
        and {"diffuse", "reflectivity", "lighting"}.issubset(roles)
    )

    for filepath, role in files_and_roles:
        if role is None:
            continue
        if is_fo76_to_fo4_bundle and role in {"reflectivity", "lighting"}:
            continue
        out_role = _output_role(role, source_profile, target_profile)
        if out_role is None:
            continue
        _append_output(
            outputs,
            seen,
            filepath,
            out_role,
            output_dir,
            source_profile,
            target_profile,
        )

    if is_fo76_to_fo4_bundle:
        reflectivity_file = next(
            path for path, role in files_and_roles if role == "reflectivity"
        )
        lighting_file = next(path for path, role in files_and_roles if role == "lighting")
        _append_output(
            outputs,
            seen,
            reflectivity_file,
            "specular",
            output_dir,
            source_profile,
            target_profile,
        )
        _append_output(
            outputs,
            seen,
            lighting_file,
            "glow",
            output_dir,
            source_profile,
            target_profile,
        )

    return outputs


def convert_texture_paths(
    files_and_roles: list[tuple[Path, str | None]],
    output_dir: Path,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> dict:
    dds_files_and_roles = [
        (filepath, role)
        for filepath, role in files_and_roles
        if role is not None and filepath.suffix.lower() == ".dds"
    ]
    skipped = [
        {
            "role": role,
            "reason": (
                "not a DDS texture" if role is not None else "unrecognized texture role"
            ),
        }
        for filepath, role in files_and_roles
        if role is None or filepath.suffix.lower() != ".dds"
    ]
    if not dds_files_and_roles:
        return {"converted": [], "skipped": skipped}

    if not _has_native_path_converter(source_profile, target_profile):
        result = _copy_dds_files_with_converted_names(
            dds_files_and_roles,
            output_dir,
            source_profile,
            target_profile,
        )
        if skipped:
            result = {
                "converted": list(result.get("converted", [])),
                "skipped": list(result.get("skipped", [])) + skipped,
            }
        return result

    results = []
    bundle_roles = {"diffuse", "reflectivity", "lighting"}
    dds_roles = {role for _, role in dds_files_and_roles}
    if (
        source_profile.id == "fo76"
        and target_profile.id == "fo4"
        and bundle_roles.issubset(dds_roles)
    ):
        bundle_files = [
            (filepath, role)
            for filepath, role in dds_files_and_roles
            if role in bundle_roles
        ]
        extra_files = [
            (filepath, role)
            for filepath, role in dds_files_and_roles
            if role not in bundle_roles
        ]
        results.append(
            _convert_dds_files(bundle_files, output_dir, source_profile, target_profile)
        )
        if extra_files:
            results.append(
                _convert_dds_files(
                    extra_files,
                    output_dir,
                    source_profile,
                    target_profile,
                )
            )
    else:
        results.append(
            _convert_dds_files(
                dds_files_and_roles,
                output_dir,
                source_profile,
                target_profile,
            )
        )

    result = {
        "converted": [
            item
            for native_result in results
            for item in native_result.get("converted", [])
        ],
        "skipped": [
            item
            for native_result in results
            for item in native_result.get("skipped", [])
        ],
    }
    if skipped:
        result = {
            "converted": list(result.get("converted", [])),
            "skipped": list(result.get("skipped", [])) + skipped,
        }
    return result


def _has_native_path_converter(
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> bool:
    return source_profile.id == target_profile.id or (
        source_profile.id == "fo76" and target_profile.id == "fo4"
    )


def _copy_dds_files_with_converted_names(
    files_and_roles: list[tuple[Path, str | None]],
    output_dir: Path,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> dict:
    converted = []
    skipped = []
    seen: set[Path] = set()
    for filepath, role in files_and_roles:
        if role is None:
            skipped.append({"role": role, "reason": "unrecognized texture role"})
            continue
        output_path = output_dir / convert_texture_name(
            filepath.name,
            source_profile,
            target_profile,
        )
        if output_path in seen:
            skipped.append({"role": role, "reason": "duplicate output path"})
            continue
        seen.add(output_path)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(filepath, output_path)
        converted.append({"role": role, "path": str(output_path)})
    return {"converted": converted, "skipped": skipped}


def texture_params(source_profile: GameProfile) -> dict[str, float]:
    remix_profile = getattr(source_profile, "texture_remix", None)
    if remix_profile is None:
        return {
            "ao_multiplier": 0.5,
            "specular_multiplier": 1.0,
            "gloss_multiplier": 1.0,
            "spec_offset": 0.8,
        }
    return {
        "ao_multiplier": float(remix_profile.ao_multiplier),
        "specular_multiplier": float(remix_profile.specular_multiplier),
        "gloss_multiplier": float(remix_profile.gloss_multiplier),
        "spec_offset": float(remix_profile.spec_offset),
    }


def _convert_dds_files(
    files_and_roles: list[tuple[Path, str | None]],
    output_dir: Path,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> dict:
    payload = {
        "source_game": source_profile.id,
        "target_game": target_profile.id,
        "inputs": [
            {"role": role, "path": str(filepath)}
            for filepath, role in files_and_roles
            if role is not None
        ],
        "outputs": build_outputs(
            files_and_roles,
            output_dir,
            source_profile,
            target_profile,
        ),
        "params": texture_params(source_profile),
    }
    return native_runtime.convert_texture_set_paths(payload)


def _append_output(
    outputs: list[dict[str, str]],
    seen: set[tuple[str, str]],
    filepath: Path,
    out_role: str,
    output_dir: Path,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> None:
    new_name = _convert_texture_name_for_role(
        filepath.name,
        out_role,
        source_profile,
        target_profile,
    )
    key = (out_role, new_name)
    if key in seen:
        return
    seen.add(key)
    outputs.append(
        {
            "role": out_role,
            "path": str(output_dir / new_name),
            "format": compression_for_role(out_role),
        }
    )


def _output_role(
    role: str,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> str | None:
    if source_profile.id == "fo76" and target_profile.id == "fo4":
        if role in {"reflectivity", "lighting"}:
            return "specular"
        if role in target_profile.texture_suffixes:
            return role
        return None
    if role in target_profile.texture_suffixes:
        return role
    return None


def _convert_texture_name_for_role(
    filename: str,
    out_role: str,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> str:
    source_role = detect_texture_role(filename, source_profile)
    target_suffix = target_profile.texture_suffixes.get(out_role)
    if source_role is None or target_suffix is None:
        return convert_texture_name(filename, source_profile, target_profile)

    source_suffix = source_profile.texture_suffixes[source_role]
    path = Path(filename)
    stem = path.stem
    idx = stem.lower().rfind(source_suffix.lower())
    if idx < 0:
        return convert_texture_name(filename, source_profile, target_profile)
    return f"{stem[:idx]}{target_suffix}{path.suffix}"
