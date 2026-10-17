from pathlib import Path
import shutil


RUNTIME_NAME = "B21_ConversionRuntime.dll"
RUNTIME_DESTINATION = Path("F4SE/Plugins") / RUNTIME_NAME
CLOCK_CLASS = "B21_ConversionClock"
NATIVE_CALLS = {
    CLOCK_CLASS: ("GetGameModeClock", "RequestNextGameMode", "CancelGameMode", "CanRunGameMode"),
    "B21_ConversionCreature": ("IsLimbGone", "KillActorWithLimb"),
}


def built_runtime_binary(repository: Path) -> Path:
    project = repository / "mods/B21_ConversionRuntime"
    binary = project / RUNTIME_DESTINATION
    if not binary.is_file():
        raise FileNotFoundError(f"Converted runtime support requires a built {binary}")
    sources = [project / "xmake.lua", *project.joinpath("src").rglob("*")]
    if any(path.is_file() and path.stat().st_mtime_ns > binary.stat().st_mtime_ns for path in sources):
        raise RuntimeError(f"Rebuild B21_ConversionRuntime before conversion: {binary} predates its source")
    return binary


def runtime_binary() -> Path:
    packaged = Path(__file__).parent / "resources/conversion_runtime" / RUNTIME_NAME
    if packaged.is_file():
        return packaged
    for parent in Path(__file__).resolve().parents:
        if (parent / "mods/B21_ConversionRuntime").is_dir():
            return built_runtime_binary(parent)
    raise FileNotFoundError("B21_ConversionRuntime is missing from this BACUP installation")


def stage_conversion_runtime(mod_path: Path) -> Path:
    source = runtime_binary()
    destination = mod_path / RUNTIME_DESTINATION
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    return destination


def stage_fnv_catalog_runtime(mod_path: Path, output_plugin: str) -> Path | None:
    directory = mod_path / "F4SE/Plugins/B21_ConversionRuntime"
    catalogs = (
        directory / f"{Path(output_plugin).stem}.creature-weapons.json",
        directory / "FnvRadiation" / f"{output_plugin}.json",
        directory / "FnvImageSpace" / f"{output_plugin}.json",
        directory / "PropertySequences" / f"{output_plugin}.json",
        directory / "Weapons" / f"{output_plugin}.json",
        directory / "LegacyWeaponAmmo" / f"{output_plugin}.json",
        directory / "LegacyStats" / f"{output_plugin}.json",
    )
    if not any(path.is_file() for path in catalogs):
        return None
    return stage_conversion_runtime(mod_path)


def prepare_fnv_native_manifest(mod_path: Path, manifest: list[dict]) -> tuple[list[dict], dict[str, set[str]]]:
    source_root = mod_path / "Scripts/Source/User"
    consumers: dict[str, set[str]] = {}
    for entry in manifest:
        relative = Path(entry["relative_source_path"])
        source = mod_path / relative if relative.parts and relative.parts[0].casefold() == "scripts" else source_root / relative
        source.resolve().relative_to(source_root.resolve())
        if not source.is_file():
            continue
        lines = [line.partition(";")[0] for line in source.read_text(encoding="utf-8").splitlines()]
        for helper, functions in NATIVE_CALLS.items():
            if any(f"{helper}.{function}(" in line for line in lines for function in functions):
                consumers.setdefault(helper, set()).add(entry["class_name"])
    if not consumers:
        return manifest, consumers
    result = list(manifest)
    for helper in consumers:
        if any(entry["class_name"].casefold() == helper.casefold() for entry in manifest):
            raise RuntimeError(f"FNV generated manifest collides with native helper {helper}")
        source = Path(__file__).parent / "script_additions/fnv_fo4" / f"{helper}.psc"
        source_root.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, source_root / source.name)
        result.append({
            "class_name": helper,
            "relative_source_path": f"Scripts/Source/User/{helper}.psc",
            "kind": "native_runtime",
            "required": True,
        })
    return result, consumers


def stage_fnv_native_runtime(mod_path: Path, consumers: dict[str, set[str]], evidence: list[dict]) -> Path | None:
    compiled = {entry["class_name"]: entry for entry in evidence if entry["compiled_success"]}
    needed = [helper for helper, classes in consumers.items() if classes.intersection(compiled)]
    if not needed:
        return None
    for helper_class in needed:
        helper = compiled.get(helper_class)
        if helper is None:
            raise RuntimeError(f"FNV native consumer compiled without required {helper_class}.pex")
        helper_pex = mod_path / helper["relative_pex_path"]
        if not helper_pex.is_file() or helper_pex.stat().st_size == 0:
            raise RuntimeError(f"FNV native helper PEX is missing or empty: {helper_pex}")
    return stage_conversion_runtime(mod_path)
