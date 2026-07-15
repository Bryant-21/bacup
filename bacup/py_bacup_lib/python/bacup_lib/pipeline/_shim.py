"""Internal orchestrator-shaped shim used by pipeline phase wrappers."""
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from bacup_lib.models import ConversionContext, RecordNode


class _OrchestratorShim:
    """Namespace plus fallback-to-fixups attribute proxy."""

    def __init__(self, ns, fixups):
        self.__dict__["_ns"] = ns
        self.__dict__["_fixups"] = fixups

    def __getattr__(self, name):
        ns = self.__dict__["_ns"]
        if hasattr(ns, name):
            return getattr(ns, name)
        fixups = self.__dict__["_fixups"]
        if fixups is not None and hasattr(fixups, name):
            return getattr(fixups, name)
        raise AttributeError(name)

    def __setattr__(self, name, value):
        if name in ("_ns", "_fixups"):
            self.__dict__[name] = value
            return
        setattr(self.__dict__["_ns"], name, value)


def build_orchestrator_shim(records: list["RecordNode"], ctx: "ConversionContext"):
    """Build an orchestrator-shaped object from a ConversionContext and records."""
    from bacup_lib.models import DependencyGraph
    from bacup_lib.workflows.asset_phases import ConversionFixups
    from creation_lib.core.game_profiles import get_profile

    root = records[0] if records else None
    graph = DependencyGraph(
        root=root,
        all_records=list(records),
        all_assets=[],
        errors=[],
    )
    diagnostics_root = Path(getattr(ctx, "diagnostics_root", None) or ctx.mod_path)

    def _diagnostics_dir() -> str:
        diagnostics_root.mkdir(parents=True, exist_ok=True)
        return str(diagnostics_root)

    def _diagnostics_path(*parts: str) -> str:
        path = diagnostics_root.joinpath(*parts)
        path.parent.mkdir(parents=True, exist_ok=True)
        return str(path)

    namespace = SimpleNamespace(
        source_game=ctx.source_game,
        target_game=ctx.target_game,
        mod_path=str(ctx.mod_path),
        graph=graph,
        use_base_game_assets=True,
        preserve_source_ids=bool(getattr(ctx, "preserve_source_ids", True)),
        overwrite_existing=bool(getattr(ctx, "overwrite_existing", False)),
        pbr_carry=bool(getattr(ctx, "pbr_carry", False)),
        texture_landscape_mip_flooding=bool(
            getattr(ctx, "texture_landscape_mip_flooding", False)
        ),
        target_extracted_dir=str(ctx.target_extracted_dir or ""),
        target_asset_store=getattr(ctx, "target_asset_store", None),
        target_asset_index=getattr(ctx, "target_asset_index", None),
        target_data_dir=str(ctx.target_data_dir or ""),
        source_data_dir=str(getattr(ctx, "source_data_dir", "") or ""),
        diagnostics_root=str(diagnostics_root),
        _diagnostics_dir=_diagnostics_dir,
        _diagnostics_path=_diagnostics_path,
        source_plugin_handle=getattr(ctx, "source_plugin_handle", None),
        source_master_handles=list(getattr(ctx, "source_master_handles", []) or []),
        target_master_handles=list(getattr(ctx, "target_master_handles", []) or []),
        converted_plugin_registry=getattr(ctx, "converted_plugin_registry", None),
        conversion_workers=getattr(ctx, "conversion_workers", None),
        disable_nif_collision_memo=bool(
            getattr(ctx, "disable_nif_collision_memo", False)
        ),
        records_limit=getattr(ctx, "records_limit", None),
        convert_precombined_nifs=bool(getattr(ctx, "convert_precombined_nifs", True)),
        base_asset_relocation_mesh_roots=tuple(
            getattr(ctx, "base_asset_relocation_mesh_roots", ()) or ()
        ),
        base_asset_namespace=str(getattr(ctx, "base_asset_namespace", "") or ""),
        is_whole_plugin=bool(getattr(ctx, "is_whole_plugin", False)),
        output_plugin_extension=(
            "." + str(ctx.output_plugin_name).rsplit(".", 1)[1]
            if "." in str(ctx.output_plugin_name)
            else ".esp"
        ),
        emit_authoring_yaml=bool(getattr(ctx, "emit_authoring_yaml", True)),
        addon_index_start=20000,
        addon_registry_filename=".addon_registry.json",
        seed_addon_registry=False,
        _source_profile=get_profile(ctx.source_game),
        _target_profile=get_profile(ctx.target_game),
        _summary=ctx.summary,
        _material_overrides=dict(getattr(ctx, "material_overrides", {}) or {}),
        _formkey_mapper=ctx.formkey_mapper,
        _log_lines=getattr(ctx, "log_lines", []),
        _conversion_decisions=getattr(ctx, "conversion_decisions", []),
        _rust_conversion_run=getattr(ctx, "_rust_conversion_run", None),
        _fnv_legacy_links=getattr(ctx, "fnv_legacy_links", {}),
        _fnv_legacy_result=getattr(ctx, "fnv_legacy_result", None),
        _target_behavior_paths=None,
        _asset_map={},
        _addon_index_map=dict(getattr(ctx, "addon_index_map", {}) or {}),
        _materials_cdb=None,
        _raw_material_files_converted=bool(
            getattr(ctx, "raw_material_files_converted", False)
        ),
    )
    namespace.graph.all_assets = list(getattr(ctx, "assets", namespace.graph.all_assets))
    shim = _OrchestratorShim(namespace, ctx.fixups)
    if ctx.fixups is None:
        shim._fixups = ConversionFixups(shim)
    return shim
