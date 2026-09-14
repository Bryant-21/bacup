"""UnifiedDriver shape tests. The record-track copy is
pinned cheaply here (label sequence + signal order against a stubbed
orchestrator); the REAL oracle is the byte-gate."""

from __future__ import annotations

import json
import os
import time
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.models import (
    AssetRef,
    ConversionSummary,
    LegacyPackExpectedCounts,
    LegacyPackOriginRow,
    PhaseProgress,
    PluginPortOptions,
    PluginPortRequest,
    WorldspaceCellBounds,
)
from bacup_lib.native_maps import native_translation_maps_dir
from bacup_lib.workflows import unified as unified_mod
from bacup_lib.workflows.unified import (
    TrackSignals,
    UnifiedDriver,
    _UnifiedRecordRuntime,
    _augment_fo76_to_fo4_script_skeleton,
    _copy_fo76_vaultboy_swfs,
    _finalize_fo76_pipboy_map_texture,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _resolve_source_strings_dir,
    _resolve_fo76_translate_tokens,
    _script_body_is_hollow,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


def test_asset_failure_parser_reads_native_wave_messages():
    lines = [
        "[ERROR] convert_nifs failed Meshes/lod/deep/item.nif: sf convert: no convertible geometry",
        "[WARN] NIF not found: Meshes/missing.nif: source path did not resolve",
        "[ERROR] texture job failed terrain bundle NewAtlantis/Soil: missing input",
    ]

    assert _UnifiedRecordRuntime._failed_paths(lines, "NIF") == [
        "Meshes/lod/deep/item.nif",
        "Meshes/missing.nif",
    ]
    assert _UnifiedRecordRuntime._failed_paths(lines, "Texture") == [
        "terrain bundle NewAtlantis/Soil: missing input"
    ]


def test_source_strings_dir_prefers_sidecars_beside_plugin(tmp_path: Path) -> None:
    source_plugin = tmp_path / "merge" / "Skyrim.esm"
    adjacent_strings = source_plugin.parent / "Strings"
    fallback_root = tmp_path / "extracted"
    adjacent_strings.mkdir(parents=True)
    (fallback_root / "Strings").mkdir(parents=True)

    assert _resolve_source_strings_dir(source_plugin, fallback_root) == str(
        adjacent_strings
    )


def test_source_strings_dir_uses_configured_data_strings_child(tmp_path: Path) -> None:
    source_plugin = tmp_path / "merge" / "Skyrim.esm"
    source_data_root = tmp_path / "extracted"
    configured_strings = source_data_root / "Strings"
    source_plugin.parent.mkdir(parents=True)
    configured_strings.mkdir(parents=True)

    assert _resolve_source_strings_dir(source_plugin, source_data_root) == str(
        configured_strings
    )


class StubRunner:
    def __init__(self):
        self.logs = []
        self.item_progress = []
        self.phase_starts = []
        self.phase_completions = []

    def emit_log(self, level, message):
        self.logs.append((level, message))

    def emit_item_progress(self, progress):
        self.item_progress.append(progress.current_item)

    def emit_phase_start(self, progress):
        self.phase_starts.append(progress.phase_name)

    def emit_phase_complete(self, progress):
        self.phase_completions.append((progress.phase_name, progress.status))

    def is_cancelled(self):
        return False

    def emit_complete(self, output_root, summary):
        self.completed = (output_root, summary)


def test_convert_face_uses_context_source_data_dir(tmp_path: Path) -> None:
    calls: list[tuple[str, dict]] = []

    class FakeRun:
        def run_phase(self, phase, **kwargs):
            calls.append((phase, kwargs))

    source_root = tmp_path / "source"
    request = SimpleNamespace(
        source_data_dir=tmp_path / "fallback",
        target_extracted_dir=tmp_path / "target",
        output_root=tmp_path / "output",
    )
    runtime = _UnifiedRecordRuntime(request)
    ctx = SimpleNamespace(
        _rust_conversion_run=FakeRun(),
        source_data_dir=source_root,
        target_asset_store=None,
        output_plugin_name="FalloutNV.esm",
        mod_path=tmp_path / "output",
    )

    runtime._run_convert_face_phase(
        ctx,
        StubRunner(),
        PhaseProgress(phase=5, phase_name="Convert NPC Faces"),
    )

    assert calls[0][0] == "convert_face"
    assert calls[0][1]["source_extracted_dir"] == str(source_root)
    assert calls[0][1]["params"]["translation_maps_dir"] == str(
        native_translation_maps_dir()
    )


def make_request(tmp_path: Path) -> PluginPortRequest:
    src = tmp_path / "SeventySix.esm"
    src.write_bytes(b"not a real plugin")
    return PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[src],
        output_root=tmp_path / "out",
        target_extracted_dir=None,
        target_data_dir=None,
        options=PluginPortOptions(
            translate_records=True,
            convert_terrain=True,
            build_esp=True,
            convert_scripts=True,
            # asset phases OFF on the record options (waves own them):
            convert_nifs=False,
            convert_btos=False,
            convert_textures=False,
            convert_materials=False,
            convert_havok=False,
            synthesize_drivers=False,
            convert_animations=False,
            copy_sounds=False,
            validate_output=False,
        ),
    )


def test_output_runtime_hazard_gate_fails_on_fo4_target_shape(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from creation_lib.esp.editor import runtime_hazards

    output_path = tmp_path / "FalloutNV.esm"
    output_path.write_bytes(b"plugin")
    request = SimpleNamespace(
        target_game="fo4",
        target_master_paths=[],
        target_data_dir=None,
        output_root=tmp_path,
    )
    runtime = _UnifiedRecordRuntime(request)
    ctx = SimpleNamespace(mod_path=tmp_path, output_plugin_name=output_path.name)
    profiles: list[str] = []

    class FakeReport:
        hazards = [SimpleNamespace(message="IMAD TNAM is empty")]

        def __len__(self):
            return len(self.hazards)

    def scan(path, *, game, profile):
        assert path == output_path
        assert game == "fo4"
        profiles.append(profile)
        return FakeReport()

    monkeypatch.setattr(runtime_hazards, "scan_runtime_hazards_path", scan)

    with pytest.raises(RuntimeError, match="1 runtime hazard"):
        runtime._check_output_runtime_hazards(ctx, StubRunner())

    assert profiles == [runtime_hazards.FO4_TARGET_SHAPE_PROFILE]


def _mvp_melee_row(
    form_key: str,
    editor_id: str,
    *,
    role: str,
    status: str,
    target_profile: str,
    world_model: str = "",
    first_person_model: str = "",
    first_person_resolution: str = "",
    reason_code: str = "",
) -> dict[str, object]:
    return {
        "source_form_key": form_key,
        "editor_id": editor_id,
        "role": role,
        "status": status,
        "reason_code": reason_code,
        "target_profile": target_profile,
        "world_model": world_model,
        "first_person_model": first_person_model,
        "first_person_resolution": first_person_resolution,
    }


def test_mvp_melee_phase_builds_authoritative_corpus_asset_plan(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from bacup_lib.source_pairs import (
        FNV_MVP_EXCLUDE_SIGNATURES,
        mvp_melee_policy_payload,
    )

    rows = [
        _mvp_melee_row(
            "000001@FalloutNV.esm",
            "OneHand",
            role="gun",
            status="admitted",
            target_profile="machete_one_hand",
            world_model="Weapons/OneHand.nif",
            first_person_model="Weapons/1stPersonOneHand.nif",
        ),
        _mvp_melee_row(
            "000002@FalloutNV.esm",
            "TwoHand",
            role="melee",
            status="admitted",
            target_profile="grognak_two_hand",
            world_model="Weapons/TwoHand.nif",
            first_person_model="Weapons/1stPersonTwoHand.nif",
        ),
        _mvp_melee_row(
            "000003@FalloutNV.esm",
            "Unarmed",
            role="melee",
            status="admitted",
            target_profile="unarmed",
        ),
        _mvp_melee_row(
            "000004@FalloutNV.esm",
            "Bow",
            role="gun",
            status="rejected",
            reason_code="ranged_animation",
            target_profile="",
        ),
        _mvp_melee_row(
            "000005@FalloutNV.esm",
            "Gun",
            role="gun",
            status="rejected",
            reason_code="ranged_animation",
            target_profile="",
        ),
        _mvp_melee_row(
            "000006@FalloutNV.esm",
            "Throwing",
            role="gun",
            status="rejected",
            reason_code="throwing_animation",
            target_profile="",
        ),
    ]
    assets = [
        AssetRef("nif", path, resolved_path=str(tmp_path / Path(path).name))
        for path in (
            "Weapons/OneHand.nif",
            "Weapons/1stPersonOneHand.nif",
            "Weapons/TwoHand.nif",
            "Weapons/1stPersonTwoHand.nif",
        )
    ]
    policy = mvp_melee_policy_payload("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES)
    assert policy is not None
    ctx = SimpleNamespace(
        source_game="fnv",
        target_game="fo4",
        assets=assets,
        mvp_melee_policy=policy,
    )
    calls: list[tuple[str, dict]] = []

    class FakeRun:
        id = 7

        def run_phase(self, phase, **kwargs):
            calls.append((phase, kwargs))
            return {"records_added": 3, "records_dropped": 3}

    monkeypatch.setattr(
        "bacup_lib.weapon_report.weapon_metadata_rows", lambda _source: tuple(rows)
    )
    runtime = _UnifiedRecordRuntime(make_request(tmp_path))

    runtime._run_mvp_melee_phase(FakeRun(), ctx, StubRunner())

    assert [phase for phase, _ in calls] == ["mvp_melee"]
    assert calls[0][1]["params"]["policy"]["policy_id"] == "bulk_melee_v1"
    assert ctx._weapon_mvp_asset_plan.candidate_count == 6
    assert ctx._weapon_mvp_asset_plan.admitted_count == 3
    assert {closure.editor_id for closure in ctx._weapon_mvp_asset_plan.admitted} == {
        "OneHand",
        "TwoHand",
        "Unarmed",
    }
    assert ctx.native_mvp_melee_stats["ranged_output"] == 0


def test_mvp_melee_asset_plan_rejects_shared_ranged_collision(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from bacup_lib.source_pairs import (
        FNV_MVP_EXCLUDE_SIGNATURES,
        mvp_melee_policy_payload,
    )

    shared = "Weapons/Shared.nif"
    rows = (
        _mvp_melee_row(
            "000010@FalloutNV.esm",
            "Melee",
            role="melee",
            status="admitted",
            target_profile="machete_one_hand",
            world_model=shared,
            first_person_model="Weapons/1stPersonMelee.nif",
        ),
        _mvp_melee_row(
            "000011@FalloutNV.esm",
            "Gun",
            role="gun",
            status="rejected",
            reason_code="ranged_animation",
            target_profile="",
            world_model=shared,
        ),
    )
    policy = mvp_melee_policy_payload("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES)
    ctx = SimpleNamespace(
        source_game="fnv",
        target_game="fo4",
        assets=[
            AssetRef("nif", shared, resolved_path=str(tmp_path / "Shared.nif")),
            AssetRef(
                "nif",
                "Weapons/1stPersonMelee.nif",
                resolved_path=str(tmp_path / "1stPersonMelee.nif"),
            ),
        ],
        mvp_melee_policy=policy,
    )
    monkeypatch.setattr(
        "bacup_lib.weapon_report.weapon_metadata_rows", lambda _source: rows
    )
    run = SimpleNamespace(
        id=8,
        run_phase=lambda *_args, **_kwargs: {
            "records_added": 1,
            "records_dropped": 1,
        },
    )

    _UnifiedRecordRuntime(make_request(tmp_path))._run_mvp_melee_phase(
        run, ctx, StubRunner()
    )

    assert ctx._weapon_mvp_asset_plan.admitted_count == 0
    assert ctx._weapon_mvp_asset_plan.conflict_count == 1
    assert ctx._weapon_mvp_asset_plan.conflicts[0].roles == ("gun", "melee")


def test_mvp_melee_asset_receipt_does_not_invent_first_person_models() -> None:
    rows = _UnifiedRecordRuntime._mvp_melee_asset_receipt_rows(
        [
            _mvp_melee_row(
                "000020@FalloutNV.esm",
                "ModeledMelee",
                role="melee",
                status="admitted",
                target_profile="machete_one_hand",
                world_model="Weapons/World.nif",
            )
        ],
        {"policy_id": "bulk_melee_v1"},
    )

    assert rows[0]["status"] == "rejected"
    assert rows[0]["reason_code"] == "missing_first_person_model"
    assert rows[0]["first_person_model"] == ""


@pytest.mark.parametrize(
    "target_profile", ["machete_one_hand", "grognak_two_hand"]
)
def test_mvp_melee_asset_receipt_preserves_native_record_only_admission(
    target_profile: str,
) -> None:
    rows = _UnifiedRecordRuntime._mvp_melee_asset_receipt_rows(
        [
            _mvp_melee_row(
                "000021@FalloutNV.esm",
                "IntegratedMelee",
                role="melee",
                status="admitted",
                target_profile=target_profile,
                first_person_resolution="none",
            )
        ],
        {"policy_id": "bulk_melee_v1"},
    )

    assert rows[0]["status"] == "admitted"
    assert rows[0]["target_profile"] == target_profile
    assert rows[0]["asset_disposition"] == "record_only"
    assert rows[0]["world_model"] == ""
    assert rows[0]["first_person_model"] == ""


def test_mvp_melee_phase_runs_after_fixups_before_material_rewrite() -> None:
    import inspect

    source = inspect.getsource(_UnifiedRecordRuntime._translate_records_rust)
    assert source.index('run.run_phase("fixups_v2"') < source.index(
        "self._run_mvp_melee_phase"
    )
    assert source.index("self._run_mvp_melee_phase") < source.index(
        '"rewrite_mswp_material_paths"'
    )
    assert source.index("self._run_mvp_melee_phase") < source.index(
        "self._run_mvp_creature_corpus_discovery"
    )
    assert source.index("self._run_mvp_creature_corpus_discovery") < source.index(
        '"rewrite_mswp_material_paths"'
    )
    assert source.index("self._run_mvp_creature_dependency_plan") < source.index(
        'run.run_phase("translate_v2"'
    )


def test_skyrim_quest_runtime_compiles_before_story_manager_and_finalizes_after_fixups() -> None:
    import inspect

    source = inspect.getsource(_UnifiedRecordRuntime._translate_records_rust)
    assert source.index("self._prepare_skyrim_minimal_quest_actions") < source.index(
        'run.run_phase("translate_v2"'
    )
    assert source.index('run.run_phase("translate_v2"') < source.index(
        "self._compile_and_attach_skyrim_minimal_quests"
    )
    assert source.index("self._compile_and_attach_skyrim_minimal_quests") < source.index(
        "self._copy_skyrim_quest_runtime_assets"
    )
    assert source.index("self._copy_skyrim_quest_runtime_assets") < source.index(
        'run.run_phase(\n            "emit_story_manager_subset"'
    )
    assert source.index('"emit_story_manager_subset"') < source.index(
        'run.run_phase("fixups_v2"'
    )


def test_skyrim_quest_preparation_uses_native_loader(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    native_calls: list[int] = []
    monkeypatch.setattr(
        unified_mod,
        "load_native_module",
        lambda: SimpleNamespace(
            conversion_run_skyrim_minimal_quest_candidates_json=lambda run_id: (
                native_calls.append(run_id) or "[]"
            )
        ),
    )
    request = make_request(tmp_path)
    request.source_game = "skyrimse"
    ctx = SimpleNamespace(source_game="skyrimse", target_game="fo4")

    prepared = _UnifiedRecordRuntime(request)._prepare_skyrim_minimal_quest_actions(
        run=SimpleNamespace(id=73),
        ctx=ctx,
        runner=StubRunner(),
    )

    assert prepared == 0
    assert native_calls == [73]


def test_skyrim_quest_runtime_asset_lookup_rejects_missing_declared_roots(
    tmp_path: Path,
) -> None:
    with pytest.raises(FileNotFoundError, match="missing from declared roots"):
        _UnifiedRecordRuntime._find_skyrim_quest_runtime_asset(
            (),
            ("sound/voice/questport/nord_male/001234_00.xwm",),
            "converted Skyrim quest XWM/WAV",
        )

    empty_root = tmp_path / "converted"
    empty_root.mkdir()
    with pytest.raises(FileNotFoundError, match="001234_00.xwm"):
        _UnifiedRecordRuntime._find_skyrim_quest_runtime_asset(
            (empty_root,),
            ("sound/voice/questport/nord_male/001234_00.xwm",),
            "converted Skyrim quest XWM/WAV",
        )


def test_skyrim_quest_runtime_converted_roots_exclude_output_staging(
    tmp_path: Path,
) -> None:
    stale_output = tmp_path / "port" / "data"
    stale_output.mkdir(parents=True)
    (stale_output / "stale.xwm").write_bytes(b"stale prior regeneration")
    ctx = SimpleNamespace(
        mod_path=tmp_path / "port",
        skyrim_quest_runtime_converted_asset_roots=(),
    )

    assert _UnifiedRecordRuntime._skyrim_quest_runtime_converted_roots(ctx) == ()

    nested = stale_output / "converted"
    nested.mkdir()
    for unsafe_root in (stale_output, nested, stale_output.parent):
        ctx.skyrim_quest_runtime_converted_asset_roots = (unsafe_root,)
        assert _UnifiedRecordRuntime._skyrim_quest_runtime_converted_roots(ctx) == ()

    declared = tmp_path / "fresh-converted"
    declared.mkdir()
    ctx.skyrim_quest_runtime_converted_asset_roots = (declared,)
    assert _UnifiedRecordRuntime._skyrim_quest_runtime_converted_roots(ctx) == (
        declared.resolve(),
    )


@pytest.mark.parametrize("serialize_tracks", [True, False])
def test_fatal_record_preflight_starts_no_output_or_asset_work(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    serialize_tracks: bool,
) -> None:
    request = make_request(tmp_path)
    request.source_game = "fnv"
    calls: list[str] = []

    def blocked_preflight(_request, _runner) -> None:
        calls.append("preflight")
        assert not request.output_root.exists()
        raise RuntimeError("legacy PACK preflight blocked conversion")

    def forbidden(name: str):
        def fail(*_args, **_kwargs):
            calls.append(name)
            raise AssertionError(f"{name} must not run after a fatal preflight")

        return fail

    monkeypatch.setattr(unified_mod, "_preflight_legacy_packs", blocked_preflight)
    monkeypatch.setattr(unified_mod, "load_native_module", forbidden("native"))
    monkeypatch.setattr(unified_mod, "UnifiedDriver", forbidden("driver"))
    monkeypatch.setattr(unified_mod, "run_asset_track", forbidden("assets"))

    with pytest.raises(RuntimeError, match="legacy PACK preflight blocked"):
        unified_mod.run_unified(
            request,
            StubRunner(),
            enable_ba2=False,
            serialize_tracks=serialize_tracks,
        )

    assert calls == ["preflight"]
    assert not request.output_root.exists()


def test_early_record_preflight_forwards_explicit_pack_exclusion(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from bacup_lib import run as run_mod

    request = make_request(tmp_path)
    request.source_game = "fnv"
    request.options.fnv_quest_slice = True
    request.options.exclude_signatures = frozenset({"pack"})
    request.legacy_pack_raw_source_counts = LegacyPackExpectedCounts(fnv=2, fo3=3)
    captured: dict[str, object] = {}

    class FakeRun:
        @classmethod
        def create_new(cls, *_args, **kwargs):
            captured["config"] = kwargs["config"]
            return cls()

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return None

        def preflight_legacy_packs(self) -> None:
            captured["preflight_called"] = True

        def drain_events(self, _max: int):
            return []

    monkeypatch.setattr(run_mod, "ConversionRun", FakeRun)

    unified_mod._preflight_legacy_packs(request, StubRunner())

    assert captured["preflight_called"] is True
    config = captured["config"]
    assert isinstance(config, dict)
    assert "PACK" in config["skip_record_signatures"]
    assert config["fnv_quest_slice"] is True
    assert config["legacy_pack_raw_source_counts"] == {"fnv": 2, "fo3": 3}


def test_resolve_fo76_translate_tokens_uses_interface_translate_file(
    tmp_path: Path,
) -> None:
    interface_dir = tmp_path / "interface"
    interface_dir.mkdir()
    (interface_dir / "translate_en.txt").write_text(
        "$REGION_THE_FOREST\tTHE FOREST\n$REGION_CRANBERRY_BOG\tCRANBERRY BOG\n",
        encoding="utf-16",
    )
    tables = {
        "en": {
            1: "$REGION_THE_FOREST",
            2: "$REGION_THEFOREST",
            3: "$REGION_CRANBERRY_BOG",
            4: "$UNKNOWN_USER",
            5: "Already Named",
        }
    }

    rewritten = _resolve_fo76_translate_tokens(tables, tmp_path)

    assert rewritten == 3
    assert tables["en"][1] == "The Forest"
    assert tables["en"][2] == "The Forest"
    assert tables["en"][3] == "Cranberry Bog"
    assert tables["en"][4] == "$UNKNOWN_USER"
    assert tables["en"][5] == "Already Named"


def test_unified_record_track_rejects_cell_bounds(tmp_path: Path, monkeypatch) -> None:
    request = make_request(tmp_path)
    request.options.cell_bounds = WorldspaceCellBounds(
        worldspace_editor_id="APPALACHIA",
        min_x=-1,
        min_y=-1,
        max_x=1,
        max_y=1,
    )
    driver = UnifiedDriver(request, sink_id=None)

    def fake_build_context(source_plugin, plugin_name, mod_path, runner):
        return SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
        )

    monkeypatch.setattr(driver._record_runtime, "_build_context", fake_build_context)

    with pytest.raises(
        ValueError, match="cell-bounds runs are not supported in unified driver"
    ):
        driver._convert_record_track(request.source_plugins[0], StubRunner())


def test_finalize_fo76_pipboy_map_texture_writes_fo4_legacy_dds(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    from PIL import Image

    from creation_lib.dds import io as dds_io

    source_root = tmp_path / "source"
    source_path = (
        source_root / "textures" / "interface" / "pip-boy" / "papermap_city_d.dds"
    )
    source_path.parent.mkdir(parents=True)
    source_path.write_bytes(b"dds")
    mod_path = tmp_path / "out" / "SeventySix"
    ctx = SimpleNamespace(source_data_dir=source_root, mod_path=mod_path)
    request = make_request(tmp_path)
    request.options.convert_textures = True
    runner = StubRunner()
    saved: dict[str, object] = {}

    def fake_load_image(path: str, mode: str = "RGBA") -> Image.Image:
        assert path == str(source_path)
        assert mode == "RGBA"
        return Image.new("RGBA", (4, 4), (1, 2, 3, 255))

    def fake_save_image(
        img: Image.Image,
        path: str,
        *,
        is_palette: bool = False,
        format: str | None = None,
        generate_mips: bool = False,
        use_gpu: bool = True,
    ) -> None:
        saved.update(
            path=Path(path),
            size=img.size,
            is_palette=is_palette,
            format=format,
            generate_mips=generate_mips,
            use_gpu=use_gpu,
        )

    monkeypatch.setattr(dds_io, "load_image", fake_load_image)
    monkeypatch.setattr(dds_io, "save_image", fake_save_image)

    _finalize_fo76_pipboy_map_texture(request, ctx, runner)

    assert saved == {
        "path": mod_path
        / "data"
        / "textures"
        / "interface"
        / "pip-boy"
        / "papermap_city_d.dds",
        "size": (2048, 2048),
        "is_palette": False,
        "format": "DXT1",
        "generate_mips": True,
        "use_gpu": False,
    }
    web_map = (
        mod_path
        / "PrismaUI_F4"
        / "views"
        / "B21_FullScreenMap"
        / "maps"
        / "appalachia"
        / "map.png"
    )
    assert web_map.is_file()
    with Image.open(web_map) as img:
        assert img.size == (4, 4)
    manifest = json.loads((web_map.parent / "map.json").read_text(encoding="utf-8"))
    assert manifest["worldspace"] == "Appalachia"
    assert manifest["discovery"] == "proximity"
    assert manifest["calibration"]["mode"] == "survey"
    assert any(
        log[0] == "INFO"
        and log[1].startswith("Wrote FO4-compatible Appalachia Pip-Boy map texture")
        for log in runner.logs
    )
    assert any(
        log[0] == "INFO"
        and log[1].startswith("Wrote generated Appalachia fullscreen map pack")
        for log in runner.logs
    )


def test_copy_fo76_vaultboy_swfs_preserves_both_interface_trees(
    tmp_path: Path,
) -> None:
    source_root = tmp_path / "source"
    vaultboys = source_root / "interface" / "components" / "vaultboys"
    quest_vaultboys = source_root / "interface" / "components" / "quest vault boys"
    (vaultboys / "perks").mkdir(parents=True)
    (quest_vaultboys / "quests").mkdir(parents=True)
    (quest_vaultboys / "locations").mkdir(parents=True)
    (vaultboys / "perks" / "perk.swf").write_bytes(b"perk")
    (quest_vaultboys / "quests" / "quest.swf").write_bytes(b"quest")
    (quest_vaultboys / "locations" / "location.swf").write_bytes(b"location")
    (vaultboys / "ignore.txt").write_text("not an SWF", encoding="utf-8")

    request = make_request(tmp_path)
    mod_path = tmp_path / "out" / "SeventySix"
    ctx = SimpleNamespace(source_data_dir=source_root, mod_path=mod_path)
    runner = StubRunner()

    copied = _copy_fo76_vaultboy_swfs(request, ctx, runner)

    assert copied == 3
    assert (
        mod_path
        / "data"
        / "Interface"
        / "Components"
        / "VaultBoys"
        / "perks"
        / "perk.swf"
    ).read_bytes() == b"perk"
    assert (
        mod_path
        / "data"
        / "Interface"
        / "Components"
        / "Quest Vault Boys"
        / "quests"
        / "quest.swf"
    ).read_bytes() == b"quest"
    assert (
        mod_path
        / "data"
        / "Interface"
        / "Components"
        / "Quest Vault Boys"
        / "locations"
        / "location.swf"
    ).read_bytes() == b"location"
    assert not (
        mod_path / "data" / "Interface" / "Components" / "VaultBoys" / "ignore.txt"
    ).exists()
    assert ("INFO", "Copied 3 FO76 VaultBoy SWF asset(s)") in runner.logs


def test_target_asset_preflight_ignores_live_data_loose_assets(
    tmp_path: Path,
    monkeypatch,
) -> None:
    import bacup_lib.workflows.unified as unified

    extracted_dir = tmp_path / "extracted" / "fo4"
    game_data_dir = tmp_path / "Fallout 4" / "Data"
    base_nif = extracted_dir / "Meshes" / "SetDressing" / "BaseOnly.nif"
    loose_nif = (
        game_data_dir / "Meshes" / "Actors" / "Frog" / "CharacterAssets" / "Frog.nif"
    )
    base_nif.parent.mkdir(parents=True)
    loose_nif.parent.mkdir(parents=True)
    base_nif.write_bytes(b"nif")
    loose_nif.write_bytes(b"loose override")

    request = make_request(tmp_path)
    request.target_extracted_dir = extracted_dir
    request.target_data_dir = game_data_dir
    request.options.translate_records = False
    request.options.convert_nifs = True

    class FakeStore:
        asset_count = 1
        warnings = []
        catalog_path = tmp_path / "catalog.sqlite3"
        cache_dir = tmp_path / "cache"

        def open_timings(self):
            return {}

        def has_asset(self, path):
            return str(path).replace("\\", "/").casefold() == (
                "meshes/setdressing/baseonly.nif"
            )

        def list_assets(self, *, prefix="", suffix=""):
            path = "meshes/setdressing/baseonly.nif"
            return (
                [path]
                if path.startswith(prefix.casefold())
                and path.endswith(suffix.casefold())
                else []
            )

    monkeypatch.setattr(unified, "build_target_asset_store", lambda **_: FakeStore())

    runtime = _UnifiedRecordRuntime(request)
    ctx = runtime._build_context(
        request.source_plugins[0],
        "SeventySix.esm",
        tmp_path / "out" / "SeventySix",
        StubRunner(),
    )

    index = ctx.target_asset_index
    assert index is not None
    assert index.has_asset(
        AssetRef("nif", "Meshes/SetDressing/BaseOnly.nif", resolved_path="")
    )
    assert not index.has_asset(
        AssetRef("nif", "Meshes/Actors/Frog/CharacterAssets/Frog.nif", resolved_path="")
    )
    assert index.owners == {}


def test_projected_worldspace_carry_log_failure_is_nonfatal(
    tmp_path,
    monkeypatch,
) -> None:
    from bacup_lib import worldspace_services
    from bacup_lib.pipeline import terrain as terrain_pipeline

    runtime = _UnifiedRecordRuntime(make_request(tmp_path))
    terrain = SimpleNamespace(source_worldspace_editor_id="APPALACHIA")
    ctx = SimpleNamespace(
        source_plugin_handle=SimpleNamespace(native_handle_id=11),
        mod_path=tmp_path,
        output_plugin_name="B21_Test.esm",
        rust_target_handle_id=None,
    )
    captured: dict[str, object] = {}

    monkeypatch.setattr(
        terrain_pipeline,
        "fo76_btd_work_items",
        lambda _ctx: [(terrain, tmp_path / "Appalachia.btd", "APPALACHIA")],
    )

    def fake_patch_target_worldspaces_subrecords(**kwargs):
        captured.update(kwargs)
        return [6]

    monkeypatch.setattr(
        worldspace_services,
        "patch_target_worldspaces_subrecords",
        fake_patch_target_worldspaces_subrecords,
    )

    class FailingLogRunner:
        def emit_log(self, level, message):
            raise OSError(22, "Invalid argument")

    runtime._patch_projected_worldspace_subrecords(
        ctx,
        FailingLogRunner(),
        tmp_path / "SeventySix.esm",
    )

    assert captured["worldspace_editor_ids"] == ["APPALACHIA"]
    assert captured["target_plugin_path"] == tmp_path / "B21_Test.esm"


def test_projected_worldspace_carry_does_not_restore_starfield_bounds(
    tmp_path,
    monkeypatch,
) -> None:
    from bacup_lib.pipeline import terrain as terrain_pipeline

    request = make_request(tmp_path)
    request.source_game = "starfield"
    request.target_game = "fo4"
    runtime = _UnifiedRecordRuntime(request)

    monkeypatch.setattr(
        terrain_pipeline,
        "fo76_btd_work_items",
        lambda _ctx: (_ for _ in ()).throw(
            AssertionError("Starfield WRLD bounds must stay relatticed")
        ),
    )

    runtime._patch_projected_worldspace_subrecords(
        SimpleNamespace(),
        StubRunner(),
        tmp_path / "Starfield.esm",
    )


def test_final_term_marker_repair_roundtrips_real_plugins(tmp_path) -> None:
    import struct

    from creation_lib.esp.native_runtime import (
        plugin_handle_add_record_raw,
        plugin_handle_call,
        plugin_handle_close,
        plugin_handle_load,
        plugin_handle_new,
        plugin_handle_record_subrecords,
    )

    source_path = tmp_path / "source" / "SeventySix.esm"
    output_path = tmp_path / "mod" / "SeventySix.esm"
    source_path.parent.mkdir()
    output_path.parent.mkdir()

    marker = struct.pack("<ffffI4B", 1.0, -59.0, 1.0, 0.0, 0, 0xFF, 1, 0, 0)
    zero_leading_marker = struct.pack(
        "<ffffI4B", 0.0, -61.0, 0.0, 0.0, 0, 0xFF, 1, 0, 0
    )
    records = [
        (0x0072_6E6C, "Storm_UpperAtrium_ClinicTerminal", marker, b"\x00\x00\x80\x00"),
        (0x006F_C53D, "zzzStorm_CR_B_SR", zero_leading_marker, b""),
    ]

    source_handle = plugin_handle_new(source_path.name, "fo76")
    target_handle = plugin_handle_new(output_path.name, "fo4")
    try:
        for form_id, editor_id, source_marker, corrupt_marker in records:
            plugin_handle_add_record_raw(
                source_handle,
                "TERM",
                form_id,
                0,
                0,
                208,
                1,
                [
                    ("EDID", editor_id.encode("cp1252") + b"\0", None),
                    ("XMRK", b"Markers\\MarkerDeskTerminal3rdP.nif\0", None),
                    ("ZNAM", source_marker, None),
                ],
            )
            plugin_handle_add_record_raw(
                target_handle,
                "TERM",
                form_id,
                0,
                0,
                131,
                1,
                [
                    ("EDID", editor_id.encode("cp1252") + b"\0", None),
                    ("XMRK", b"Markers\\MarkerDeskTerminal3rdP.nif\0", None),
                    ("SNAM", corrupt_marker, None),
                ],
            )
        plugin_handle_call(source_handle, "save", str(source_path))
        plugin_handle_call(target_handle, "save", str(output_path))
    finally:
        plugin_handle_close(source_handle)
        plugin_handle_close(target_handle)

    request = make_request(tmp_path)
    request.source_plugins = [source_path]
    runtime = _UnifiedRecordRuntime(request)
    ctx = SimpleNamespace(
        source_plugin_handle=None,
        mod_path=output_path.parent,
        output_plugin_name=output_path.name,
    )
    runner = StubRunner()

    runtime._repair_term_marker_parameters_final(ctx, runner, source_path)

    expected_markers = {
        form_id: source_marker for form_id, _, source_marker, _ in records
    }
    repaired_handle = plugin_handle_load(str(output_path), game="fo4")
    try:
        for form_id, expected_marker in expected_markers.items():
            subrecords = plugin_handle_record_subrecords(repaired_handle, form_id)
            signatures = [signature for signature, _, _ in subrecords]
            xmrk = signatures.index("XMRK")
            snam_rows = [
                (index, data)
                for index, (signature, data, _) in enumerate(subrecords)
                if signature == "SNAM"
            ]
            assert snam_rows == [(xmrk + 1, expected_marker)]
    finally:
        plugin_handle_close(repaired_handle)

    assert "modified=2" in runner.logs[-1][1]
    assert "audit_modified=0" in runner.logs[-1][1]
    assert not output_path.with_name(f"{output_path.name}.termrepair.tmp").exists()


def test_final_term_marker_repair_runs_after_last_esm_writer() -> None:
    import inspect

    source = inspect.getsource(unified_mod.run_unified)

    assert source.index("_regenerate_modt_after_asset_waves") < source.index(
        "_repair_term_marker_parameters_final"
    )
    assert source.index("_repair_term_marker_parameters_final") < source.index(
        "_finalize_fo76_pipboy_map_texture"
    )
    # OFST/CLSZ encode the serialized layout: the rebuild must follow every
    # other ESM writer and precede the asset-only finalizers.
    assert source.index("_repair_term_marker_parameters_final") < source.index(
        "_rebuild_cell_offsets_after_build"
    )
    assert source.index("_rebuild_cell_offsets_after_build") < source.index(
        "_finalize_fo76_pipboy_map_texture"
    )


def test_unified_driver_does_not_import_plugin_port():
    import inspect
    import bacup_lib.workflows.unified as unified

    source = inspect.getsource(unified)
    assert "workflows." + "plugin_port" not in source
    assert "PluginPort" + "Orchestrator" not in source


def test_iter_nif_files_parallel_returns_sorted_paths(tmp_path: Path) -> None:
    mesh_root = tmp_path / "Meshes"
    first = mesh_root / "Actors" / "B.nif"
    second = mesh_root / "Actors" / "Nested" / "A.nif"
    ignored = mesh_root / "Actors" / "Nested" / "not-a-nif.txt"
    root_nif = mesh_root / "Root.nif"
    for path in (first, second, ignored, root_nif):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"x")

    paths = _UnifiedRecordRuntime._iter_nif_files(mesh_root, workers=4)

    assert [path.relative_to(mesh_root).as_posix() for path in paths] == [
        "Actors/B.nif",
        "Actors/Nested/A.nif",
        "Root.nif",
    ]


def test_havok_bundle_expansion_uses_workers_and_resolves_companions(
    tmp_path: Path,
) -> None:
    runtime = _UnifiedRecordRuntime(make_request(tmp_path))
    source_root = tmp_path / "source"
    project = source_root / "Meshes" / "Effects" / "Foo" / "Foo.hkx"
    behavior = source_root / "Meshes" / "Effects" / "Foo" / "Behaviors" / "Behavior.hkx"
    animation = source_root / "Meshes" / "Effects" / "Foo" / "Animations" / "Clip.hkx"
    for path in (project, behavior, animation):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"hkx")
    source_plugin = source_root / "SeventySix.esm"
    source_plugin.write_bytes(b"plugin")
    ctx = SimpleNamespace(
        source_extracted_dir=source_root,
        source_data_dir=None,
        extracted_dir=None,
        conversion_workers=4,
    )
    assets = [
        AssetRef(
            "behavior",
            "Meshes/Effects/Foo/Foo.hkx",
            resolved_path=str(project),
        )
    ]

    expanded = runtime._augment_havok_behavior_bundles(
        assets,
        source_plugin,
        ctx,
        StubRunner(),
    )

    by_path = {asset.source_path: asset for asset in expanded}
    assert "Effects/Foo/Behaviors/Behavior.hkx" in by_path
    assert "Effects/Foo/Animations/Clip.hkx" in by_path
    assert by_path["Effects/Foo/Behaviors/Behavior.hkx"].resolved_path == str(behavior)
    assert by_path["Effects/Foo/Animations/Clip.hkx"].asset_type == "animation"


def stub_record_runtime(driver: UnifiedDriver, recorded: list, monkeypatch) -> None:
    runtime = driver.record_runtime

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if raise_on_error:
            recorded.append(("raise_on_error", label))

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        ctx = SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={3: 7},
            _rust_conversion_run=None,
        )
        return ctx

    monkeypatch.setattr(runtime, "_run_phase", record_phase)
    monkeypatch.setattr(runtime, "_topo_sort", lambda plugins, runner: list(plugins))
    monkeypatch.setattr(runtime, "_build_context", make_ctx)
    monkeypatch.setattr(
        runtime, "_clean_stale_authoring_for_direct_esp", lambda mod_path: None
    )
    monkeypatch.setattr(
        runtime,
        "_collect_assets_native",
        lambda sp, ctx, runner: ["asset-a", "asset-b"],
    )
    monkeypatch.setattr(runtime, "_apply_registry_mappings", lambda ctx: None)
    monkeypatch.setattr(
        runtime, "_run_optional_fnv_legacy_phase", lambda ctx, sp, runner: False
    )
    monkeypatch.setattr(
        runtime, "_run_convert_creatures_phase", lambda ctx, runner: None
    )
    monkeypatch.setattr(
        runtime, "_run_convert_equipment_phase", lambda ctx, runner: None
    )
    monkeypatch.setattr(runtime, "_close_source_handle", lambda ctx: None)
    monkeypatch.setattr(runtime, "_close_target_master_handles", lambda ctx: None)
    monkeypatch.setattr(
        runtime, "_emit_authoring_yaml_for_build", lambda ctx, runner: False
    )
    monkeypatch.setattr(
        runtime, "_patch_projected_worldspace_subrecords", lambda ctx, runner, sp: None
    )
    monkeypatch.setattr(runtime, "_drain_and_drop_rust_run", lambda ctx: None)
    monkeypatch.setattr(runtime, "_update_registry", lambda ctx: None)
    monkeypatch.setattr(runtime, "_merge_summary", lambda summary: None)
    monkeypatch.setattr(runtime, "_merge_run_result", lambda ctx: None)
    monkeypatch.setattr(driver, "_harvest_terrain_products", lambda *_args: None)


LEGACY_RECORD_ORDER = [
    "Translate Records",
    "Convert Terrain",
    "Emit Projected NavMeshes",
    "Convert Interior Cells",
    "Rebuild Projected NAVI",
    "Copy Projected Placed Children",
    "Synthesize Worldspace Persistent Cell",
    "Sync Projected Cell Locations",
    "Synthesize Encounter Zones",
    "Synthesize Interior Sky Regions",
    "Repair Placed-Child Refs",
    "Synthesize Vendor Dialogue",
    "Scaffold Mod",
    "Convert Scripts",
    "Inventory Quest Runtime Routes",
    "Build ESP",
    "Check Runtime Hazards",
]


def test_record_sequence_matches_legacy_order(tmp_path, monkeypatch):
    recorded: list = []
    signals = TrackSignals()
    driver = UnifiedDriver(make_request(tmp_path), sink_id=None, signals=signals)
    stub_record_runtime(driver, recorded, monkeypatch)

    # Probe signal firing order by interleaving signal events into `recorded`.
    for name in ("assets_ready", "fixups_done", "terrain_done"):
        event = getattr(signals, name)
        original_set = event.set

        def probed_set(_name=name, _orig=original_set):
            recorded.append(("signal", _name))
            _orig()

        monkeypatch.setattr(event, "set", probed_set)

    driver.run_record_track(StubRunner())

    labels = [item for kind, item in recorded if kind == "phase"]
    assert labels == LEGACY_RECORD_ORDER
    hard_fail_labels = [item for kind, item in recorded if kind == "raise_on_error"]
    assert hard_fail_labels == [
        "Translate Records",
        "Convert Terrain",
        "Build ESP",
        "Check Runtime Hazards",
    ]

    signal_events = [item for kind, item in recorded if kind == "signal"]
    assert signal_events == ["assets_ready", "fixups_done", "terrain_done"]

    # Signal interleaving: assets_ready before translate, fixups_done after
    # translate but before terrain, terrain_done right after terrain.
    assert recorded.index(("signal", "assets_ready")) < recorded.index(
        ("phase", "Translate Records")
    )
    assert (
        recorded.index(("phase", "Translate Records"))
        < recorded.index(("signal", "fixups_done"))
        < recorded.index(("phase", "Convert Terrain"))
    )
    assert (
        recorded.index(("phase", "Convert Terrain"))
        < recorded.index(("signal", "terrain_done"))
        < recorded.index(("phase", "Emit Projected NavMeshes"))
    )

    # Record-track products harvested for the waves.
    assert driver.assets == ["asset-a", "asset-b"]
    assert driver.addon_index_map == {3: 7}
    assert not signals.record_failed.is_set()


def test_translate_records_failure_aborts_before_terrain_and_build(
    tmp_path,
    monkeypatch,
):
    recorded: list = []
    signals = TrackSignals()
    request = make_request(tmp_path)
    request.source_game = "fnv"
    driver = UnifiedDriver(request, sink_id=None, signals=signals)
    runtime = driver.record_runtime
    run_phase = runtime._run_phase
    stub_record_runtime(driver, recorded, monkeypatch)
    monkeypatch.setattr(runtime, "_run_phase", run_phase)
    monkeypatch.setattr(
        runtime,
        "_translate_records_rust",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(
            RuntimeError("legacy creature target validation failed")
        ),
    )
    runner = StubRunner()

    with pytest.raises(RuntimeError, match="legacy creature target validation failed"):
        driver.run_record_track(runner)

    assert runner.phase_starts == ["Translate Records"]
    assert runner.phase_completions == [("Translate Records", "error")]
    assert signals.record_failed.is_set()
    assert not (request.output_root / "SeventySix" / "SeventySix.esm").exists()


def test_serialized_record_failure_starts_no_assets_or_postflight(
    tmp_path,
    monkeypatch,
):
    request = make_request(tmp_path)
    request.source_game = "fnv"
    calls: list[str] = []

    class Native:
        def sinks_create(self, _config):
            calls.append("sink_create")
            return 17

        def sinks_abort(self, _sink_id):
            calls.append("sink_abort")

        def sinks_drop(self, _sink_id):
            calls.append("sink_drop")

    class Driver:
        def __init__(self, *_args, **_kwargs):
            self.record_runtime = SimpleNamespace(
                _aggregate_summary=object(),
                run_result=object(),
            )
            self.signals = SimpleNamespace(
                record_done=SimpleNamespace(set=lambda: None)
            )
            self.defer_asset_a2_until_record_done = False
            self.ctx = None
            self.assets = []
            self.terrain_texture_jobs = []

        def run_record_track(self, _runner):
            calls.append("records")
            raise RuntimeError("Translate Records failed")

    class Mirror:
        def __init__(self, *_args, **_kwargs):
            pass

        def start(self):
            calls.append("mirror_start")

        def finish(self, status):
            calls.append(f"mirror_{status}")

    def forbidden(name):
        def fail(*_args, **_kwargs):
            calls.append(name)
            raise AssertionError(f"{name} ran after fatal record failure")

        return fail

    monkeypatch.setattr(unified_mod, "load_native_module", lambda: Native())
    monkeypatch.setattr(unified_mod, "UnifiedDriver", Driver)
    monkeypatch.setattr(unified_mod, "RunStateMirror", Mirror)
    monkeypatch.setattr(unified_mod, "run_asset_track", forbidden("assets"))
    monkeypatch.setattr(unified_mod, "_run_post_phase", forbidden("post_phase"))

    with pytest.raises(RuntimeError, match="Translate Records failed"):
        unified_mod.run_unified(
            request,
            StubRunner(),
            enable_ba2=False,
            serialize_tracks=True,
            lod_hook=forbidden("lod"),
            land_cache_hook=forbidden("land_cache"),
            record_preflight_complete=True,
        )

    assert calls == [
        "sink_create",
        "mirror_start",
        "records",
        "sink_abort",
        "mirror_failed",
        "sink_drop",
    ]


def test_build_esp_phase_requires_written_plugin(tmp_path, monkeypatch):
    recorded: list = []
    contexts: list = []
    request = make_request(tmp_path)
    request.options.convert_scripts = False
    driver = UnifiedDriver(request, sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)

    class FakeRustRun:
        id = 123

        def release_remap_state(self):
            pass

        def release_master_handles(self):
            return 0

        def release_source_handle(self):
            return False

        def save_target(self, *args, **kwargs):
            pass

        def release_source_handle(self):
            return False

        def release_master_handles(self):
            return 0

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        ctx = SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={3: 7},
            _rust_conversion_run=FakeRustRun(),
        )
        contexts.append(ctx)
        return ctx

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if label == "Build ESP":
            assert raise_on_error is True
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_ctx)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", record_phase)
    monkeypatch.setattr(driver, "_harvest_terrain_products", lambda *_args: None)

    with pytest.raises(
        FileNotFoundError,
        match=r"build_esp completed but did not write .*SeventySix\.esm",
    ):
        driver.run_record_track(StubRunner())
    assert contexts[0].summary.esp_built is False


def test_record_track_builds_esp_before_post_asset_modt(tmp_path, monkeypatch):
    recorded: list = []
    native_phases: list[str] = []
    required_phases: list[str] = []
    contexts: list = []
    request = make_request(tmp_path)
    request.options.convert_scripts = False
    driver = UnifiedDriver(request, sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)

    class FakeRustRun:
        id = 123

        def release_remap_state(self):
            pass

        def release_master_handles(self):
            return 0

        def release_source_handle(self):
            return False

        def save_target(self, output_path, **kwargs):
            native_phases.append("save_target")
            output_path = Path(output_path)
            output_path.parent.mkdir(parents=True, exist_ok=True)
            output_path.write_bytes(b"plugin")

        def release_source_handle(self):
            return False

        def release_master_handles(self):
            return 0

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        ctx = SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={3: 7},
            rust_target_handle_id=10,
            _rust_conversion_run=FakeRustRun(),
        )
        contexts.append(ctx)
        return ctx

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if raise_on_error:
            required_phases.append(label)
        if label == "Build ESP":
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_ctx)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", record_phase)
    monkeypatch.setattr(driver, "_harvest_terrain_products", lambda *_args: None)

    driver.run_record_track(StubRunner())

    assert native_phases == ["save_target"]
    assert required_phases == [
        "Translate Records",
        "Convert Terrain",
        "Build ESP",
        "Check Runtime Hazards",
    ]
    assert contexts[0].summary.esp_built is True


@pytest.mark.parametrize(
    ("source_game", "output_name"),
    [
        ("fo76", "SeventySix.esm"),
        ("fnv", "FalloutNV.esm"),
        ("skyrimse", "Skyrim.esm"),
    ],
)
def test_post_asset_modt_runs_for_every_fo4_source_and_replaces_closed_plugin(
    tmp_path, monkeypatch, source_game, output_name
):
    mod_path = tmp_path / source_game
    mod_path.mkdir()
    output_path = mod_path / output_name
    output_path.write_bytes(b"plugin")
    phases: list[tuple[str, dict]] = []
    plugin_events: list[object] = []

    class FakeRun:
        @classmethod
        def open_existing(cls, *args, **kwargs):
            plugin_events.append(("open_existing", args, kwargs))
            return cls()

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            plugin_events.append("drop")

        def save_target(self, path, **_kwargs):
            save_path = Path(path)
            assert save_path != output_path
            plugin_events.append(("save", save_path))
            save_path.write_bytes(b"updated plugin")

        def run_phase(self, phase, **kwargs):
            phases.append((phase, kwargs))
            return {"records_changed": 12 if phase == "emit_modt_manifest" else 3}

        def share_output_nif_dependencies(self, source):
            plugin_events.append(("share_dependencies", source))

    request = make_request(tmp_path)
    request.source_game = source_game
    driver = UnifiedDriver(request, sink_id=None)
    driver.ctx = SimpleNamespace(
        mod_path=mod_path,
        output_plugin_name=output_name,
    )
    monkeypatch.setattr("bacup_lib.run.ConversionRun", FakeRun)
    monkeypatch.setattr(
        driver.record_runtime,
        "_native_run_config",
        lambda _ctx: {"output_plugin_name": output_name},
    )
    real_replace = unified_mod.os.replace

    def checked_replace(source, destination):
        assert plugin_events[-1] == "drop"
        plugin_events.append(("replace", Path(source), Path(destination)))
        real_replace(source, destination)

    monkeypatch.setattr(unified_mod.os, "replace", checked_replace)

    runner = StubRunner()
    progress = PhaseProgress(
        phase=0,
        phase_name="Regenerate MODT",
        status="running",
    )
    unified_mod._regenerate_modt_after_asset_waves(
        driver,
        runner,
        mod_path,
        progress=progress,
        nif_run="asset-nif-run",
    )

    assert [phase for phase, _ in phases] == [
        "emit_modt_manifest",
        "regenerate_modt",
    ]
    assert ("share_dependencies", "asset-nif-run") in plugin_events
    assert "output_handle_id" not in phases[1][1]["params"]
    assert plugin_events[-3][0] == "save"
    temp_output_path = plugin_events[-3][1]
    assert temp_output_path.parent == output_path.parent
    assert temp_output_path.name.startswith(f".{output_path.name}.")
    assert temp_output_path.suffix == ".tmp"
    assert plugin_events[-2:] == ["drop", ("replace", temp_output_path, output_path)]
    assert output_path.read_bytes() == b"updated plugin"
    assert temp_output_path.exists() is False
    assert runner.logs[-1] == (
        "INFO",
        "post-asset MODT regeneration: manifest_entries=12 records_changed=3",
    )
    assert runner.item_progress == [
        "Building mesh manifest",
        "Regenerating MODT records",
        "Saving updated plugin",
        "",
    ]
    assert progress.completed_items == 3
    assert progress.total_items == 3


def test_native_run_config_carries_legacy_pack_provenance(tmp_path: Path) -> None:
    request = make_request(tmp_path)
    request.legacy_pack_origins = (
        LegacyPackOriginRow(
            merged_form_key="000900@FalloutNV.esm",
            source_game="fo3",
            source_plugin="Fallout3.esm",
            source_form_key="00000900@Fallout3.esm",
        ),
    )
    request.legacy_pack_expected_counts = LegacyPackExpectedCounts(fnv=1, fo3=1)
    request.legacy_pack_raw_source_counts = LegacyPackExpectedCounts(fnv=2, fo3=3)
    request.legacy_pack_provenance_required = True
    request.legacy_runtime_origins = (
        {
            "signature": "QUST",
            "merged_form_key": "00A900@FalloutNV.esm",
            "source_game": "fo3",
            "source_plugin": "Fallout3.esm",
            "source_form_key": "00000900@Fallout3.esm",
            "contributing_plugin": "ThePitt.esm",
            "source_parent_form_key": None,
            "merged_parent_form_key": None,
            "child_group_type": None,
        },
    )
    config = UnifiedDriver(request).record_runtime._native_run_config(
        SimpleNamespace(output_plugin_name="FalloutNV.esm")
    )

    assert config["legacy_pack_origins"][0]["source_game"] == "fo3"
    assert config["legacy_pack_raw_source_counts"] == {"fnv": 2, "fo3": 3}
    assert config["legacy_pack_expected_counts"] == {"fnv": 1, "fo3": 1}
    assert config["legacy_pack_provenance_required"] is True
    assert config["legacy_runtime_origins"][0]["contributing_plugin"] == "ThePitt.esm"


def test_melee_only_native_config_suppresses_creature_products(tmp_path: Path) -> None:
    from bacup_lib.source_pairs import SKYRIM_MVP_EXCLUDE_SIGNATURES

    request = make_request(tmp_path)
    request.source_game = "skyrimse"
    request.options.exclude_signatures = SKYRIM_MVP_EXCLUDE_SIGNATURES
    request.options.mvp_melee_only = True
    runtime = UnifiedDriver(request).record_runtime
    ctx = SimpleNamespace(output_plugin_name="Skyrim.esm")

    config = runtime._native_run_config(ctx)

    assert config["mvp_melee_only"] is True
    assert ctx.mvp_melee_policy["policy_id"] == "bulk_melee_v1"
    assert ctx.mvp_creature_corpus_policy is None
    assert ctx.mvp_creature_profile is None
    assert runtime._active_mvp_melee_policy()["policy_id"] == "bulk_melee_v1"
    assert runtime._active_mvp_creature_corpus_policy() is None
    assert runtime._active_mvp_creature_profile() is None


def test_native_run_config_forwards_overwrite_intent(tmp_path: Path) -> None:
    runtime = UnifiedDriver(make_request(tmp_path)).record_runtime

    assert runtime._native_run_config(
        SimpleNamespace(output_plugin_name="Output.esm", overwrite_existing=True)
    )["overwrite_existing"] is True
    assert runtime._native_run_config(
        SimpleNamespace(output_plugin_name="Output.esm")
    )["overwrite_existing"] is False


def test_native_run_config_supplies_exact_fnv_force_greet_donor_only_for_slice(
    tmp_path: Path,
) -> None:
    from bacup_lib.source_pairs import FNV_QUEST_SLICE_EXCLUDE_SIGNATURES

    request = make_request(tmp_path)
    request.source_game = "fnv"
    request.options.fnv_quest_slice = True
    request.options.exclude_signatures = FNV_QUEST_SLICE_EXCLUDE_SIGNATURES
    runtime = UnifiedDriver(request).record_runtime
    config = runtime._native_run_config(
        SimpleNamespace(output_plugin_name="FalloutNV.esm")
    )
    assert config["fnv_force_greet_donor_form_key"] == "05B307@Fallout4.esm"

    request.options.fnv_quest_slice = False
    request.options.exclude_signatures = frozenset()
    config = runtime._native_run_config(
        SimpleNamespace(output_plugin_name="FalloutNV.esm")
    )
    assert config["fnv_force_greet_donor_form_key"] is None


def test_native_run_config_rejects_required_slice_signature_exclusion(
    tmp_path: Path,
) -> None:
    from bacup_lib.source_pairs import FNV_QUEST_SLICE_EXCLUDE_SIGNATURES

    request = make_request(tmp_path)
    request.source_game = "fnv"
    request.options.fnv_quest_slice = True
    request.options.exclude_signatures = FNV_QUEST_SLICE_EXCLUDE_SIGNATURES | {"INFO"}
    runtime = UnifiedDriver(request).record_runtime

    with pytest.raises(RuntimeError, match="required record signatures: INFO"):
        runtime._native_run_config(SimpleNamespace(output_plugin_name="FalloutNV.esm"))


def test_native_run_config_rejects_quest_slice_without_placed_records(
    tmp_path: Path,
) -> None:
    request = make_request(tmp_path)
    request.source_game = "fnv"
    request.options.fnv_quest_slice = True
    request.options.convert_placed_records = False

    with pytest.raises(RuntimeError, match="requires placed-record conversion"):
        UnifiedDriver(request).record_runtime._native_run_config(
            SimpleNamespace(output_plugin_name="FalloutNV.esm")
        )


def test_grafted_asset_roots_resolve_after_primary_with_stable_dedup(tmp_path: Path):
    primary = tmp_path / "extracted" / "fnv"
    grafted = tmp_path / "extracted" / "fo3"
    source_plugin = tmp_path / "merge" / "FalloutNV.esm"
    source_plugin.parent.mkdir(parents=True)
    source_plugin.write_bytes(b"TES4")
    primary_asset = primary / "Meshes" / "clutter" / "shared.nif"
    grafted_asset = grafted / "Meshes" / "clutter" / "shared.nif"
    grafted_only = grafted / "Sound" / "fx" / "fo3_only.wav"
    for path, payload in (
        (primary_asset, b"fnv"),
        (grafted_asset, b"fo3"),
        (grafted_only, b"sound"),
    ):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(payload)

    request = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[source_plugin],
        output_root=tmp_path / "mods",
        source_data_dir=primary,
        additional_source_asset_roots=(
            grafted,
            Path(str(primary).upper()),
            grafted,
        ),
    )
    driver = UnifiedDriver(request, sink_id=None)
    ctx = SimpleNamespace(
        source_data_dir=primary,
        additional_source_asset_roots=request.additional_source_asset_roots,
    )

    runtime = driver._record_runtime
    assert runtime._native_asset_source_roots(source_plugin, ctx) == [
        primary,
        grafted,
        source_plugin.parent,
    ]
    assert runtime._resolve_native_asset_path(
        "nif", "Meshes/clutter/shared.nif", source_plugin, ctx
    ) == (str(primary_asset), None)
    assert runtime._resolve_native_asset_path(
        "sound", "Sound/fx/fo3_only.wav", source_plugin, ctx
    ) == (str(grafted_only), None)


def test_non_grafted_pair_asset_roots_remain_primary_then_plugin(tmp_path: Path):
    request = make_request(tmp_path)
    primary = tmp_path / "extracted" / "fo76"
    request.source_data_dir = primary
    driver = UnifiedDriver(request, sink_id=None)
    ctx = SimpleNamespace(
        source_data_dir=primary,
        additional_source_asset_roots=(),
    )

    assert driver._record_runtime._native_asset_source_roots(
        request.source_plugins[0], ctx
    ) == [
        primary,
        request.source_plugins[0].parent,
    ]


def test_legacy_nif_resolution_uses_unique_basename_path_drift(tmp_path: Path):
    primary = tmp_path / "extracted" / "fnv"
    source_plugin = tmp_path / "merge" / "FalloutNV.esm"
    source_plugin.parent.mkdir(parents=True)
    source_plugin.write_bytes(b"TES4")
    actual = (
        primary / "Meshes" / "terminals" / "dlc03" / "DLC03CrlMainframeTerminal01.nif"
    )
    actual.parent.mkdir(parents=True)
    actual.write_bytes(b"nif")
    request = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[source_plugin],
        output_root=tmp_path / "mods",
        source_data_dir=primary,
    )
    runtime = UnifiedDriver(request, sink_id=None).record_runtime
    ctx = SimpleNamespace(
        source_data_dir=primary,
        additional_source_asset_roots=(),
        conversion_workers=1,
    )

    assert runtime._resolve_native_asset_path(
        "nif",
        "DLC03/Architecture/Crawler/DLC03CrlMainframeTerminal01.nif",
        source_plugin,
        ctx,
    ) == (str(actual), None)


def test_legacy_nif_resolution_rejects_ambiguous_basename_path_drift(tmp_path: Path):
    primary = tmp_path / "extracted" / "fnv"
    source_plugin = tmp_path / "merge" / "FalloutNV.esm"
    source_plugin.parent.mkdir(parents=True)
    source_plugin.write_bytes(b"TES4")
    for directory in ("first", "second"):
        candidate = primary / "Meshes" / directory / "SharedName.nif"
        candidate.parent.mkdir(parents=True, exist_ok=True)
        candidate.write_bytes(directory.encode())
    request = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[source_plugin],
        output_root=tmp_path / "mods",
        source_data_dir=primary,
    )
    runtime = UnifiedDriver(request, sink_id=None).record_runtime
    ctx = SimpleNamespace(
        source_data_dir=primary,
        additional_source_asset_roots=(),
        conversion_workers=1,
    )

    resolved, error = runtime._resolve_native_asset_path(
        "nif", "wrong/SharedName.nif", source_plugin, ctx
    )

    assert resolved is None
    assert error is not None


@pytest.mark.parametrize(
    ("target_game", "build_esp"),
    [("skyrimse", True), ("fo4", False)],
)
def test_post_asset_modt_skips_non_fo4_and_no_build_paths(
    tmp_path, monkeypatch, target_game, build_esp
):
    request = make_request(tmp_path)
    request.target_game = target_game
    request.options.build_esp = build_esp
    driver = UnifiedDriver(request, sink_id=None)
    driver.ctx = SimpleNamespace(output_plugin_name="Unused.esm")
    monkeypatch.setattr(
        unified_mod.Plugin,
        "load",
        lambda *_args, **_kwargs: pytest.fail("skipped MODT path loaded a plugin"),
    )

    runner = StubRunner()
    unified_mod._regenerate_modt_after_asset_waves(driver, runner, tmp_path)

    assert runner.logs == []


def _fake_precombine_run(phases, plugin_events, *, assets_written):
    class FakeRun:
        @classmethod
        def open_existing(cls, *args, **kwargs):
            plugin_events.append(("open_existing", args, kwargs))
            return cls()

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            plugin_events.append("drop")

        def save_target(self, path, **_kwargs):
            plugin_events.append(("save", Path(path)))
            Path(path).write_bytes(b"stamped plugin")

        def run_phase(self, phase, **kwargs):
            phases.append((phase, kwargs))
            return {"assets_written": assets_written, "records_changed": 7}

    return FakeRun


def _precombine_driver(request, mod_path, output_name, monkeypatch):
    driver = UnifiedDriver(request, sink_id=None)
    driver.ctx = SimpleNamespace(mod_path=mod_path, output_plugin_name=output_name)
    monkeypatch.setattr(
        driver.record_runtime,
        "_native_run_config",
        lambda _ctx: {"output_plugin_name": output_name},
    )
    return driver


def test_generate_precombines_dispatches_phase_when_enabled(tmp_path, monkeypatch):
    mod_path = tmp_path / "fo76"
    mod_path.mkdir()
    output_name = "SeventySix.esm"
    (mod_path / output_name).write_bytes(b"plugin")
    phases: list[tuple[str, dict]] = []
    plugin_events: list[object] = []

    request = make_request(tmp_path)
    request.options.generate_precombines = True
    driver = _precombine_driver(request, mod_path, output_name, monkeypatch)
    monkeypatch.setattr(
        "bacup_lib.run.ConversionRun",
        _fake_precombine_run(phases, plugin_events, assets_written=3),
    )

    progress = PhaseProgress(
        phase=0, phase_name="Generate precombines", status="running"
    )
    runner = StubRunner()
    unified_mod._generate_precombines_after_asset_waves(
        driver, runner, mod_path, progress=progress
    )

    assert [phase for phase, _ in phases] == ["generate_precombines"]
    params = phases[0][1]["params"]
    # Minimal params only: no archive/extract-root keys handed from the pipeline.
    assert "include_cells" in params
    assert "mesh_archives" not in params
    assert "mesh_extract_roots" not in params
    assert phases[0][1]["mod_path"] == str(mod_path)
    assert any(isinstance(evt, tuple) and evt[0] == "save" for evt in plugin_events)
    assert any(evt == "drop" for evt in plugin_events)
    assert (mod_path / output_name).read_bytes() == b"stamped plugin"
    assert runner.logs[-1][0] == "INFO"
    assert "post-asset precombine generation" in runner.logs[-1][1]


def test_generate_precombines_no_assets_does_not_write(tmp_path, monkeypatch):
    mod_path = tmp_path / "fo76"
    mod_path.mkdir()
    output_name = "SeventySix.esm"
    (mod_path / output_name).write_bytes(b"plugin")
    phases: list[tuple[str, dict]] = []
    plugin_events: list[object] = []

    request = make_request(tmp_path)
    request.options.generate_precombines = True
    driver = _precombine_driver(request, mod_path, output_name, monkeypatch)
    monkeypatch.setattr(
        "bacup_lib.run.ConversionRun",
        _fake_precombine_run(phases, plugin_events, assets_written=0),
    )

    runner = StubRunner()
    unified_mod._generate_precombines_after_asset_waves(driver, runner, mod_path)

    assert [phase for phase, _ in phases] == ["generate_precombines"]
    assert not any(isinstance(evt, tuple) and evt[0] == "save" for evt in plugin_events)
    assert (mod_path / output_name).read_bytes() == b"plugin"


@pytest.mark.parametrize(
    ("target_game", "build_esp", "generate_precombines"),
    [
        ("skyrimse", True, True),
        ("fo4", False, True),
        ("fo4", True, False),  # gate off: the default full build must not run it
    ],
)
def test_generate_precombines_skips_when_disabled_or_unsupported(
    tmp_path, monkeypatch, target_game, build_esp, generate_precombines
):
    request = make_request(tmp_path)
    request.target_game = target_game
    request.options.build_esp = build_esp
    request.options.generate_precombines = generate_precombines
    driver = UnifiedDriver(request, sink_id=None)
    driver.ctx = SimpleNamespace(output_plugin_name="Unused.esm")
    monkeypatch.setattr(
        "bacup_lib.run.ConversionRun",
        lambda *a, **k: pytest.fail("disabled precombine path opened a plugin"),
    )

    runner = StubRunner()
    unified_mod._generate_precombines_after_asset_waves(driver, runner, tmp_path)

    assert runner.logs == []


def test_run_unified_schedules_precombines_after_modt_and_gated():
    import inspect

    source = inspect.getsource(unified_mod.run_unified)
    assert source.index("_regenerate_modt_after_asset_waves") < source.index(
        "_generate_precombines_after_asset_waves"
    )
    # The call is guarded by the (default-off) option, so a standard build skips it.
    assert 'getattr(request.options, "generate_precombines"' in source
    assert source.index("_generate_precombines_after_asset_waves") < source.index(
        "_repair_term_marker_parameters_final"
    )


def test_build_esp_phase_sets_summary_for_native_write(tmp_path, monkeypatch):
    recorded: list = []
    contexts: list = []
    request = make_request(tmp_path)
    request.options.convert_scripts = False
    driver = UnifiedDriver(request, sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)

    class FakeRustRun:
        id = 123

        def release_remap_state(self):
            pass

        def release_master_handles(self):
            return 0

        def release_source_handle(self):
            return False

        def save_target(self, output_path, **_kwargs):
            output_path = Path(output_path)
            output_path.parent.mkdir(parents=True, exist_ok=True)
            output_path.write_bytes(b"fresh")

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        ctx = SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={3: 7},
            _rust_conversion_run=FakeRustRun(),
        )
        contexts.append(ctx)
        return ctx

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if label == "Build ESP":
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_ctx)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", record_phase)
    monkeypatch.setattr(driver, "_harvest_terrain_products", lambda *_args: None)

    driver.run_record_track(StubRunner())

    assert contexts[0].summary.esp_built is True


def test_quest_runtime_inventory_runs_after_script_vmad_strip_and_before_save():
    import inspect

    script_phase = inspect.getsource(
        unified_mod._UnifiedRecordRuntime._run_convert_scripts_phase
    )
    record_track = inspect.getsource(unified_mod.UnifiedDriver._convert_record_track)

    assert "_reconcile_script_references(" in script_phase
    assert record_track.index('"Convert Scripts"') < record_track.index(
        '"Inventory Quest Runtime Routes"'
    )
    assert record_track.index('"Inventory Quest Runtime Routes"') < record_track.index(
        "rust_run.save_target("
    )


def test_quest_runtime_inventory_delegates_route_analysis_to_native():
    import inspect

    inventory_phase = inspect.getsource(
        unified_mod._UnifiedRecordRuntime._run_quest_runtime_inventory_phase
    )

    assert "conversion_run_quest_runtime_inventory_json" in inventory_phase
    assert "target_evidence_from_native_records" not in inventory_phase
    assert "source_evidence_from_plugin" not in inventory_phase
    assert "_build_pex_index" not in inventory_phase
    assert 'ctx.mod_path) / "data" / "Scripts"' in inventory_phase
    assert "self._req.options.convert_scripts" in inventory_phase
    assert "persistent mod-output" in inventory_phase


def test_build_esp_phase_rejects_stale_output_without_native_write(
    tmp_path, monkeypatch
):
    recorded: list = []
    contexts: list = []
    request = make_request(tmp_path)
    request.options.convert_scripts = False
    driver = UnifiedDriver(request, sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)

    class FakeRustRun:
        id = 123

        def release_remap_state(self):
            pass

        def release_master_handles(self):
            return 0

        def release_source_handle(self):
            return False

        def save_target(self, *_args, **_kwargs):
            raise RuntimeError("native save failed")

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        output_path = mod_path / plugin_name
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_bytes(b"stale")
        ctx = SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={3: 7},
            _rust_conversion_run=FakeRustRun(),
        )
        contexts.append(ctx)
        return ctx

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if label == "Build ESP":
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_ctx)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", record_phase)
    monkeypatch.setattr(driver, "_harvest_terrain_products", lambda *_args: None)

    with pytest.raises(RuntimeError, match="native save failed"):
        driver.run_record_track(StubRunner())

    assert contexts[0].summary.esp_built is False


def test_translate_v2_report_accounting_preserves_translated_semantics():
    stats = _UnifiedRecordRuntime._translate_stats_from_report(
        {
            "records_changed": 36,
            "records_vanilla_remapped": 4,
            "records_dropped": 3,
            "records_deferred": 2,
            "warnings": 1,
        }
    )

    assert stats["records_translated"] == 36
    assert sum(stats.values()) == 46


def test_native_phase_report_decoder_defaults_new_outcomes_for_legacy_tuple():
    from bacup_lib import native_runtime as conversion_native_runtime

    report = conversion_native_runtime._phase_report_from_raw((36, 0, 3, 1, 2, 4, 5))

    assert report["records_changed"] == 36
    assert report["records_dropped"] == 3
    assert report["assets_written"] == 1
    assert report["warnings"] == 2
    assert report["elapsed_ms"] == 4
    assert report["items_failed"] == 5
    assert report["records_vanilla_remapped"] == 0
    assert report["records_deferred"] == 0


def test_native_phase_report_decoder_reads_append_only_outcomes():
    from bacup_lib import native_runtime as conversion_native_runtime

    report = conversion_native_runtime._phase_report_from_raw(
        (36, 0, 3, 1, 2, 4, 5, 6, 7)
    )

    assert report["records_changed"] == 36
    assert report["records_dropped"] == 3
    assert report["assets_written"] == 1
    assert report["warnings"] == 2
    assert report["elapsed_ms"] == 4
    assert report["items_failed"] == 5
    assert report["records_vanilla_remapped"] == 6
    assert report["records_deferred"] == 7


def test_native_phase_report_decoder_rejects_unknown_tuple_length():
    from bacup_lib import native_runtime as conversion_native_runtime

    with pytest.raises(ValueError, match=r"7 or 9 fields, got 8"):
        conversion_native_runtime._phase_report_from_raw((0,) * 8)


def test_land_cache_hook_fires_after_projected_navmeshes(tmp_path, monkeypatch):
    recorded: list = []
    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)
    driver.on_land_cache_ready = lambda _ctx: (
        recorded.append(("hook", "land_cache")) or True
    )

    driver.run_record_track(StubRunner())

    assert (
        recorded.index(("phase", "Emit Projected NavMeshes"))
        < recorded.index(("hook", "land_cache"))
        < recorded.index(("phase", "Convert Interior Cells"))
    )


def test_synthesize_object_lod_phase_receives_conversion_workers(tmp_path, monkeypatch):
    recorded: list = []
    request = make_request(tmp_path)
    request.options.synthesize_object_lod = True
    driver = UnifiedDriver(request, sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)

    class FakeRustRun:
        def __init__(self):
            self.id = 123
            self.calls = []

        def release_remap_state(self):
            pass

        def release_master_handles(self):
            return 0

        def release_source_handle(self):
            return False

        def run_phase(self, phase, **kwargs):
            self.calls.append((phase, kwargs))
            return {}

    rust_run = FakeRustRun()
    source_dir = tmp_path / "source"
    source_dir.mkdir()

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        ctx = SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={3: 7},
            source_data_dir=source_dir,
            conversion_workers=20,
            _rust_conversion_run=rust_run,
        )
        return ctx

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if label == "Synthesize Object LOD":
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_ctx)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", record_phase)

    driver.run_record_track(StubRunner())

    assert rust_run.calls == [
        (
            "synthesize_object_lod",
            {
                "mod_path": str(tmp_path / "out" / "SeventySix"),
                "source_extracted_dir": str(source_dir),
                "params": {"conversion_workers": 20},
            },
        )
    ]


def test_synthesize_object_lod_runs_for_existing_output_without_translate(
    tmp_path, monkeypatch
):
    recorded: list = []
    request = make_request(tmp_path)
    request.options.translate_records = False
    request.options.convert_terrain = False
    request.options.build_esp = False
    request.options.convert_scripts = False
    request.options.synthesize_object_lod = True
    driver = UnifiedDriver(request, sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)

    synth_calls = []

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if label == "Synthesize Object LOD":
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_run_phase", record_phase)
    monkeypatch.setattr(
        driver.record_runtime,
        "_run_synthesize_object_lod_existing_output",
        lambda source_plugin, ctx, runner: synth_calls.append(
            (source_plugin, ctx.mod_path, ctx.output_plugin_name)
        ),
    )

    driver.run_record_track(StubRunner())

    assert ("phase", "Translate Records") not in recorded
    assert ("phase", "Synthesize Object LOD") in recorded
    assert synth_calls == [
        (
            request.source_plugins[0],
            tmp_path / "out" / "SeventySix",
            "SeventySix.esm",
        )
    ]


def test_synthesize_object_lod_uses_existing_output_when_build_esp_disabled(
    tmp_path, monkeypatch
):
    recorded: list = []
    request = make_request(tmp_path)
    request.options.build_esp = False
    request.options.synthesize_object_lod = True
    driver = UnifiedDriver(request, sink_id=None)
    stub_record_runtime(driver, recorded, monkeypatch)

    class FakeRustRun:
        def __init__(self):
            self.id = 123
            self.calls = []

        def release_remap_state(self):
            pass

        def release_master_handles(self):
            return 0

        def release_source_handle(self):
            return False

        def run_phase(self, phase, **kwargs):
            self.calls.append((phase, kwargs))
            return {}

    rust_run = FakeRustRun()
    synth_calls = []

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        return SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={3: 7},
            _rust_conversion_run=rust_run,
        )

    def record_phase(
        phase_no, label, body, runner, timing_ctx=None, raise_on_error=False
    ):
        recorded.append(("phase", label))
        if label == "Synthesize Object LOD":
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_ctx)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", record_phase)
    monkeypatch.setattr(
        driver.record_runtime,
        "_run_synthesize_object_lod_existing_output",
        lambda source_plugin, ctx, runner: synth_calls.append(
            (source_plugin, ctx.mod_path, ctx.output_plugin_name)
        ),
    )

    driver.run_record_track(StubRunner())

    assert ("phase", "Synthesize Object LOD") in recorded
    assert synth_calls == [
        (
            request.source_plugins[0],
            tmp_path / "out" / "SeventySix",
            "SeventySix.esm",
        )
    ]
    assert rust_run.calls == []


def test_existing_output_synthesize_replaces_plugin_after_closing_loaded_handle(
    tmp_path, monkeypatch
):
    import bacup_lib.run as run_module
    import bacup_lib.workflows.unified as unified

    request = make_request(tmp_path)
    runtime = _UnifiedRecordRuntime(request)
    source_dir = tmp_path / "source"
    source_dir.mkdir()
    mod_path = tmp_path / "out" / "SeventySix"
    mod_path.mkdir(parents=True)
    output_path = mod_path / "SeventySix.esm"
    output_path.write_bytes(b"TES4")

    class FakePlugin:
        def __init__(self, handle):
            self.native_handle_id = handle
            self.saved = []
            self.closed = False

        def save(self, path):
            save_path = Path(path)
            self.saved.append(save_path)
            if self.native_handle_id == 22:
                assert save_path != output_path
                assert output_path.read_bytes() == b"TES4"
                save_path.write_bytes(b"updated TES4")

        def close(self):
            self.closed = True

    source_plugin_handle = FakePlugin(11)
    target_plugin_handle = FakePlugin(22)
    loads = []

    def fake_load(path, **kwargs):
        loads.append((Path(path), kwargs))
        if Path(path) == request.source_plugins[0]:
            return source_plugin_handle
        return target_plugin_handle

    run_calls = []
    saved_paths = []

    class FakeConversionRun:
        @classmethod
        def open_existing(cls, *args, **kwargs):
            run_calls.append(("open_existing", args, kwargs))
            return cls()

        def __init__(self):
            self.id = 99

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc_val, exc_tb):
            run_calls.append(("drop",))

        def run_phase(self, phase, **kwargs):
            run_calls.append(("phase", phase, kwargs))
            return {"records_changed": 7, "assets_written": 3, "warnings": 1}

        def drain_decisions(self):
            return []

        def drain_warnings(self):
            return []

        def save_target(self, path, **_kwargs):
            save_path = Path(path)
            saved_paths.append(save_path)
            assert save_path != output_path
            save_path.write_bytes(b"updated TES4")

    monkeypatch.setattr(unified.Plugin, "load", staticmethod(fake_load))
    monkeypatch.setattr(run_module, "ConversionRun", FakeConversionRun)
    real_replace = unified.os.replace
    replace_calls = []

    def checked_replace(source, destination):
        assert run_calls[-1] == ("drop",)
        replace_calls.append((Path(source), Path(destination)))
        real_replace(source, destination)

    monkeypatch.setattr(unified.os, "replace", checked_replace)

    ctx = SimpleNamespace(
        mod_path=mod_path,
        output_plugin_name="SeventySix.esm",
        source_game="fo76",
        target_game="fo4",
        source_data_dir=source_dir,
        target_extracted_dir=None,
        target_data_dir=None,
        conversion_workers=20,
        is_whole_plugin=True,
        target_master_handles=[SimpleNamespace(native_handle_id=33)],
        target_record_preflight_rows=[],
        target_record_preflight_master_names=[],
        target_record_preflight_missing_masters=[],
        base_asset_relocation_mesh_roots=[],
        base_asset_namespace="FO76",
        summary=ConversionSummary(mod_path=str(mod_path)),
    )
    runner = StubRunner()

    runtime._run_synthesize_object_lod_existing_output(
        request.source_plugins[0], ctx, runner
    )

    assert loads == []
    assert len(saved_paths) == 1
    temp_output_path = saved_paths[0]
    assert temp_output_path.parent == output_path.parent
    assert temp_output_path.name.startswith(f".{output_path.name}.")
    assert temp_output_path.suffix == ".tmp"
    assert replace_calls == [(temp_output_path, output_path)]
    assert output_path.read_bytes() == b"updated TES4"
    assert temp_output_path.exists() is False
    assert source_plugin_handle.closed is False
    assert target_plugin_handle.closed is False
    assert run_calls[0][0] == "open_existing"
    assert run_calls[1] == (
        "phase",
        "synthesize_object_lod",
        {
            "mod_path": str(mod_path),
            "source_extracted_dir": str(source_dir),
            "params": {"conversion_workers": 20},
        },
    )
    assert run_calls[2] == ("drop",)
    assert runner.logs[-1] == (
        "INFO",
        "synthesize_object_lod: updated existing output plugin SeventySix.esm; "
        "changed=7 assets=3 warnings=1",
    )


def make_wave_ctx(tmp_path: Path):
    from bacup_lib.models import AssetRef

    source_dir = tmp_path / "source"
    (source_dir / "Meshes" / "Terrain" / "World").mkdir(parents=True)
    (source_dir / "Meshes" / "Terrain" / "World" / "tile.bto").write_bytes(b"bto")
    nif_src = source_dir / "Meshes" / "a.nif"
    nif_src.write_bytes(b"nif")
    snd_src = source_dir / "Sound" / "fx" / "s.xwm"
    snd_src.parent.mkdir(parents=True)
    snd_src.write_bytes(b"snd")
    hkx_src = source_dir / "Meshes" / "b.hkx"
    hkx_src.write_bytes(b"hkx")

    assets = [
        AssetRef(
            asset_type="nif", source_path="Meshes/a.nif", resolved_path=str(nif_src)
        ),
        AssetRef(
            asset_type="sound", source_path="Sound/fx/s.xwm", resolved_path=str(snd_src)
        ),
        AssetRef(
            asset_type="behavior",
            source_path="Meshes/b.hkx",
            resolved_path=str(hkx_src),
        ),
    ]
    return SimpleNamespace(
        source_game="fo76",
        target_game="fo4",
        mod_path=tmp_path / "out" / "X",
        output_plugin_name="X.esm",
        is_whole_plugin=True,
        assets=assets,
        summary=ConversionSummary(mod_path=str(tmp_path / "out" / "X")),
        fixups=None,
        formkey_mapper=None,
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=None,
        source_data_dir=source_dir,
        conversion_workers=2,
        overwrite_existing=True,
        addon_index_map={3: 7},
        base_asset_namespace="FO76",
        base_asset_relocation_mesh_roots=(),
        convert_precombined_nifs=False,
        _rust_conversion_run=None,
    )


def test_full_plugin_asset_collection_sweeps_all_source_nifs(tmp_path):
    source_dir = tmp_path / "source"
    character_assets = (
        source_dir / "meshes" / "Actors" / "GraftonMonster" / "CharacterAssets"
    )
    character_assets.mkdir(parents=True)
    skeleton = character_assets / "skeleton.nif"
    skeleton.write_bytes(b"skeleton")
    larm = character_assets / "graftonlarmreplace.nif"
    larm.write_bytes(b"larm")
    landscape = source_dir / "meshes" / "Landscape" / "Rocks" / "conflictrock.nif"
    landscape.parent.mkdir(parents=True)
    landscape.write_bytes(b"landscape")

    class FakeSourceHandle:
        def collect_assets(self, *, asset_kinds=None, signatures=None):
            return [
                {
                    "asset_type": "nif",
                    "source_path": "Actors/GraftonMonster/CharacterAssets/skeleton.nif",
                    "source_form_key": "SeventySix.esm:000800",
                    "source_record_signature": "RACE",
                    "source_subrecord_sig": "MODL",
                    "workshop_wire_point": [3.5, 15.0, 77.0],
                    "workshop_snap_points": [
                        (
                            "P-76-0A7382",
                            (0.0, 128.0, -32.0),
                            (1.0, 0.0, 0.0, 0.0),
                            1.0,
                        )
                    ],
                }
            ]

    request = make_request(tmp_path)
    request.options.convert_nifs = True
    driver = UnifiedDriver(request, sink_id=None)
    ctx = SimpleNamespace(
        source_plugin_handle=FakeSourceHandle(),
        source_data_dir=source_dir,
        is_whole_plugin=True,
    )
    runner = StubRunner()

    assets = driver.record_runtime._collect_assets_native(
        request.source_plugins[0],
        ctx,
        runner,
    )

    by_key = {
        driver.record_runtime._nif_source_path_key(asset.source_path): asset
        for asset in assets
        if asset.asset_type == "nif"
    }
    assert sorted(by_key) == [
        "actors/graftonmonster/characterassets/graftonlarmreplace.nif",
        "actors/graftonmonster/characterassets/skeleton.nif",
        "landscape/rocks/conflictrock.nif",
    ]
    assert by_key[
        "actors/graftonmonster/characterassets/graftonlarmreplace.nif"
    ].source_path == (
        "Meshes/Actors/GraftonMonster/CharacterAssets/graftonlarmreplace.nif"
    )
    assert by_key[
        "actors/graftonmonster/characterassets/graftonlarmreplace.nif"
    ].resolved_path == str(larm)
    assert (
        by_key[
            "actors/graftonmonster/characterassets/graftonlarmreplace.nif"
        ].provenance.walker_pass
        == "full_plugin_nif_sweep"
    )
    assert by_key[
        "actors/graftonmonster/characterassets/skeleton.nif"
    ].workshop_wire_point == (3.5, 15.0, 77.0)
    assert (
        by_key["actors/graftonmonster/characterassets/skeleton.nif"]
        .workshop_snap_points[0]
        .name
        == "P-76-0A7382"
    )
    assert (
        by_key["landscape/rocks/conflictrock.nif"].provenance.walker_pass
        == "full_plugin_nif_sweep"
    )
    assert any(
        level == "INFO"
        and message.startswith("Expanded 2 full-plugin filesystem NIF(s)")
        for level, message in runner.logs
    )


def test_native_asset_collection_adds_fo76_voice_tree(tmp_path):
    source_dir = tmp_path / "source"
    voice_dir = source_dir / "Sound" / "voice" / "seventysix.esm" / "npcf_fs_abbie"
    voice_dir.mkdir(parents=True)
    fuz = voice_dir / "004e315f_1.fuz"
    fuz.write_bytes(b"fuz")
    lip = voice_dir / "004e315f_1.lip"
    lip.write_bytes(b"lip")
    ignored = voice_dir / "notes.txt"
    ignored.write_text("not audio", encoding="utf-8")

    class FakeSourceHandle:
        def collect_assets(self, *, asset_kinds=None, signatures=None):
            assert asset_kinds == ["sound"]
            return []

    request = make_request(tmp_path)
    request.options.copy_sounds = True
    driver = UnifiedDriver(request, sink_id=None)
    ctx = SimpleNamespace(
        source_plugin_handle=FakeSourceHandle(),
        source_data_dir=source_dir,
        output_plugin_name="SeventySix.esm",
    )
    runner = StubRunner()

    assets = driver.record_runtime._collect_assets_native(
        request.source_plugins[0],
        ctx,
        runner,
    )

    by_path = {
        asset.source_path.replace("\\", "/"): asset
        for asset in assets
        if asset.asset_type == "sound"
    }
    assert sorted(by_path) == [
        "Sound/Voice/SeventySix.esm/npcf_fs_abbie/004e315f_1.fuz",
    ]
    assert by_path[
        "Sound/Voice/SeventySix.esm/npcf_fs_abbie/004e315f_1.fuz"
    ].resolved_path == str(fuz)
    assert (
        by_path[
            "Sound/Voice/SeventySix.esm/npcf_fs_abbie/004e315f_1.fuz"
        ].provenance.walker_pass
        == "voice_asset_tree"
    )
    assert any(
        log
        == ("INFO", "Expanded 1 FO76 voice asset(s) from Sound/Voice/SeventySix.esm")
        for log in runner.logs
    )


def test_native_asset_collection_adds_fo76_music_and_sound_fx_trees(tmp_path):
    source_dir = tmp_path / "source"
    music_dir = source_dir / "music" / "76" / "combat"
    music_dir.mkdir(parents=True)
    combat = music_dir / "mus_76_combat_finale.xwm"
    combat.write_bytes(b"combat")
    ignored_music = music_dir / "readme.txt"
    ignored_music.write_text("not audio", encoding="utf-8")

    fx_dir = source_dir / "sound" / "fx" / "ui" / "pipboy"
    fx_dir.mkdir(parents=True)
    radio = fx_dir / "ui_pipboy_radio_static.wav"
    radio.write_bytes(b"radio")

    class FakeSourceHandle:
        def collect_assets(self, *, asset_kinds=None, signatures=None):
            assert asset_kinds == ["sound"]
            return []

    request = make_request(tmp_path)
    request.options.copy_sounds = True
    driver = UnifiedDriver(request, sink_id=None)
    ctx = SimpleNamespace(
        source_plugin_handle=FakeSourceHandle(),
        source_data_dir=source_dir,
        output_plugin_name="SeventySix.esm",
    )
    runner = StubRunner()

    assets = driver.record_runtime._collect_assets_native(
        request.source_plugins[0],
        ctx,
        runner,
    )

    by_path = {
        asset.source_path.replace("\\", "/"): asset
        for asset in assets
        if asset.asset_type == "sound"
    }
    assert sorted(by_path) == [
        "Music/76/combat/mus_76_combat_finale.xwm",
        "Sound/FX/ui/pipboy/ui_pipboy_radio_static.wav",
    ]
    assert by_path["Music/76/combat/mus_76_combat_finale.xwm"].resolved_path == str(
        combat
    )
    assert (
        by_path["Music/76/combat/mus_76_combat_finale.xwm"].provenance.walker_pass
        == "music_asset_tree"
    )
    assert by_path[
        "Sound/FX/ui/pipboy/ui_pipboy_radio_static.wav"
    ].resolved_path == str(radio)
    assert (
        by_path["Sound/FX/ui/pipboy/ui_pipboy_radio_static.wav"].provenance.walker_pass
        == "sound_fx_asset_tree"
    )
    assert any(
        log == ("INFO", "Expanded 1 FO76 music asset(s) from Music")
        for log in runner.logs
    )
    assert any(
        log == ("INFO", "Expanded 1 FO76 sound FX asset(s) from Sound/FX")
        for log in runner.logs
    )


def test_native_asset_collection_expands_characterasset_companion_nifs(tmp_path):
    source_dir = tmp_path / "source"
    character_assets = (
        source_dir / "Meshes" / "Actors" / "GraftonMonster" / "CharacterAssets"
    )
    character_assets.mkdir(parents=True)
    skeleton = character_assets / "skeleton.nif"
    skeleton.write_bytes(b"skeleton")
    larm = character_assets / "graftonlarmreplace.nif"
    larm.write_bytes(b"larm")
    lleg = character_assets / "graftonllegreplace.nif"
    lleg.write_bytes(b"lleg")
    landscape = source_dir / "Meshes" / "Landscape" / "Rocks" / "conflictrock.nif"
    landscape.parent.mkdir(parents=True)
    landscape.write_bytes(b"landscape")

    class FakeSourceHandle:
        def collect_assets(self, *, asset_kinds=None, signatures=None):
            return [
                {
                    "asset_type": "nif",
                    "source_path": "Actors/GraftonMonster/CharacterAssets/skeleton.nif",
                    "source_form_key": "SeventySix.esm:000800",
                    "source_record_signature": "RACE",
                    "source_subrecord_sig": "MODL",
                }
            ]

    request = make_request(tmp_path)
    request.options.convert_nifs = True
    driver = UnifiedDriver(request, sink_id=None)
    ctx = SimpleNamespace(
        source_plugin_handle=FakeSourceHandle(),
        source_data_dir=source_dir,
    )
    runner = StubRunner()

    assets = driver.record_runtime._collect_assets_native(
        request.source_plugins[0],
        ctx,
        runner,
    )

    by_path = {
        asset.source_path.replace("\\", "/").lower(): asset
        for asset in assets
        if asset.asset_type == "nif"
    }
    assert sorted(by_path) == [
        "actors/graftonmonster/characterassets/graftonlarmreplace.nif",
        "actors/graftonmonster/characterassets/graftonllegreplace.nif",
        "actors/graftonmonster/characterassets/skeleton.nif",
    ]
    assert by_path[
        "actors/graftonmonster/characterassets/graftonlarmreplace.nif"
    ].resolved_path == str(larm)
    assert (
        by_path[
            "actors/graftonmonster/characterassets/graftonlarmreplace.nif"
        ].provenance.walker_pass
        == "character_assets"
    )
    assert any(
        log == ("INFO", "Expanded 2 CharacterAssets companion NIF(s)")
        for log in runner.logs
    )


def test_asset_wave_plans(tmp_path):
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
    )

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    terrain_job = {
        "diffuse_path": "source/dirt01_d.dds",
        "normal_path": "source/dirt01_n.dds",
        "reflectivity_path": "source/dirt01_r.dds",
        "lighting_path": "source/dirt01_l.dds",
        "output_prefix": "textures/terrain/appalachia/dirt01",
    }
    driver.terrain_texture_jobs = [terrain_job]
    toggles = AssetWaveToggles()
    runs = AssetRuns(driver.ctx, toggles)
    try:
        builder = AssetWaveBuilder(driver, toggles, runs, StubRunner())

        a1 = builder.build_wave_a1()
        assert [s.phase for s in a1] == ["copy_sounds"]
        assert a1[0].run_id == runs.sounds.id
        assert a1[0].params["sound_paths"][0]["source_path"] == "Sound/fx/s.xwm"

        a2 = builder.build_wave_a2()
        assert [s.phase for s in a2] == [
            "convert_nifs_v2",
            "convert_btos_v2",
        ]
        assert a2[0].run_id == runs.nifs.id
        assert a2[1].run_id == runs.nifs.id  # shared collision memo run
        assert a2[1].after == ("convert_nifs_v2",)
        # The merged addon map + the source_extracted_dir oddity carried
        # verbatim (asset_phases.py helper behavior).
        assert a2[0].params["addon_index_map"] == {"3": 7}
        assert a2[0].source_extracted_dir == str(driver.ctx.target_extracted_dir)
        assert (
            a2[1].params["bto_paths"][0]["source_path"]
            == "Meshes/Terrain/World/tile.bto"
        )
        # convert_animations is gamebryo-only: absent for fo76->fo4.
        assert "convert_animations" not in [s.phase for s in a2]

        # Terrain appended a grass NIF after the A2 snapshot.
        from bacup_lib.models import AssetRef

        grass_src = tmp_path / "source" / "Meshes" / "grass.nif"
        grass_src.write_bytes(b"grass")
        driver.ctx.assets.append(
            AssetRef(
                asset_type="nif",
                source_path="Meshes/grass.nif",
                resolved_path=str(grass_src),
            )
        )

        a3 = builder.build_wave_a3()
        assert [s.phase for s in a3] == [
            "convert_textures_v2",
            "convert_materials_v2",
            "convert_nifs_v2",
        ]
        tex = a3[0]
        assert tex.run_id == runs.textures.id
        assert tex.params["convert_all"] is True
        assert tex.params["terrain_jobs"] == [terrain_job]
        mats = a3[1]
        assert mats.run_id == runs.textures.id

        a4 = builder.build_wave_a4()
        assert [s.phase for s in a4] == [
            "convert_havok",
            "postprocess_havok_assets",
            "synthesize_drivers",
            "copy_materialized_facegen",
        ]
        assert a4[0].run_id == runs.havok.id
        assert a4[1].run_id == runs.havok.id
        assert a4[1].after == ("convert_havok",)
        assert a4[2].run_id == runs.havok.id
        assert a4[2].after == ("postprocess_havok_assets",)
        assert mats.params["convert_all"] is True
        # Grass top-up converts ONLY the delta.
        topup = a3[2]
        assert [e["source_path"] for e in topup.params["nif_paths"]] == [
            "Meshes/grass.nif"
        ]

        # Plan-json shape round-trips through the DAG plan schema.
        stage = a2[1].to_plan_stage()
        assert stage["after"] == ["convert_nifs_v2"]
        assert stage["run_id"] == runs.nifs.id
    finally:
        runs.drop_all()


def test_all_creatures_discovers_and_executes_on_retained_record_run(
    tmp_path: Path,
) -> None:
    from bacup_lib.source_pairs import (
        SKYRIM_MVP_EXCLUDE_SIGNATURES,
        mvp_creature_corpus_policy_payload,
    )
    from bacup_lib.workflows.unified import AssetWaveBuilder, AssetWaveToggles

    request = make_request(tmp_path)
    request.source_game = "skyrimse"
    request.source_data_dir = tmp_path / "source"
    driver = UnifiedDriver(request, sink_id=None)
    policy = mvp_creature_corpus_policy_payload(
        "skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES
    )
    assert policy is not None
    driver.ctx = SimpleNamespace(
        source_game="skyrimse",
        target_game="fo4",
        mod_path=tmp_path / "mod",
        source_data_dir=tmp_path / "source",
        target_extracted_dir=tmp_path / "target",
        target_data_dir=tmp_path / "target_data",
        mvp_creature_corpus_policy=policy,
        mvp_creature_profile="skyrim_wolf",
    )
    calls: list[tuple[str, dict]] = []

    def run_phase(phase: str, **kwargs):
        calls.append((phase, kwargs))
        return {"records_deferred": 44} if phase == "discover_creature_corpus" else {}

    source_run = SimpleNamespace(run_phase=run_phase)
    runtime = _UnifiedRecordRuntime(request)
    runtime._run_mvp_creature_dependency_plan(source_run, driver.ctx, StubRunner())
    runtime._run_mvp_creature_corpus_discovery(source_run, driver.ctx, StubRunner())
    runtime._run_mvp_creature_corpus_execution(source_run, driver.ctx, StubRunner())

    stages = AssetWaveBuilder(
        driver, AssetWaveToggles(), SimpleNamespace(), StubRunner()
    ).build_wave_a4()

    assert [phase for phase, _ in calls] == [
        "plan_creature_dependencies",
        "discover_creature_corpus",
        "execute_creature_corpus",
    ]
    assert calls[0][1]["params"] == {"debug_dir": "debug/creature_corpus"}
    assert calls[0][1]["source_extracted_dir"] == str(tmp_path / "source")
    assert calls[1][1]["params"] == {
        "profile": "all_creatures_v1",
        "debug_dir": "debug/creature_corpus",
    }
    assert calls[1][1]["source_extracted_dir"] == str(tmp_path / "source")
    assert calls[1][1]["target_extracted_dir"] == str(tmp_path / "target")
    assert calls[1][1]["target_data_dir"] == str(tmp_path / "target_data")
    assert calls[2][1]["params"] == {
        "plan_path": "debug/creature_corpus/plan.json",
        "mode": "strict",
    }
    assert calls[2][1]["source_extracted_dir"] == str(tmp_path / "source")
    assert calls[2][1]["target_extracted_dir"] == str(tmp_path / "target")
    assert calls[2][1]["target_data_dir"] == str(tmp_path / "target_data")
    assert stages == []
    assert unified_mod._exact_mvp_creature_profile(driver.ctx) is None
    assert driver.ctx._creature_corpus_executed is True


def test_all_creatures_without_prepared_recipes_stays_discovery_only(
    tmp_path: Path,
) -> None:
    from bacup_lib.source_pairs import (
        FNV_MVP_EXCLUDE_SIGNATURES,
        mvp_creature_corpus_policy_payload,
    )
    from bacup_lib.workflows.unified import AssetWaveBuilder, AssetWaveToggles

    request = make_request(tmp_path)
    request.source_game = "fnv"
    driver = UnifiedDriver(request, sink_id=None)
    policy = mvp_creature_corpus_policy_payload(
        "fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES
    )
    assert policy is not None
    driver.ctx = SimpleNamespace(
        source_game="fnv",
        target_game="fo4",
        mod_path=tmp_path / "mod",
        source_data_dir=tmp_path / "source",
        target_extracted_dir=tmp_path / "target",
        target_data_dir=None,
        mvp_creature_corpus_policy=policy,
    )
    discovery_calls: list[tuple[str, dict]] = []
    source_run = SimpleNamespace(
        run_phase=lambda phase, **kwargs: discovery_calls.append((phase, kwargs)) or {}
    )

    runtime = _UnifiedRecordRuntime(request)
    runtime._run_mvp_creature_corpus_discovery(source_run, driver.ctx, StubRunner())
    runtime._run_mvp_creature_corpus_execution(source_run, driver.ctx, StubRunner())
    stages = AssetWaveBuilder(
        driver,
        AssetWaveToggles(),
        SimpleNamespace(),
        StubRunner(),
    ).build_wave_a4()

    assert "jobs" not in discovery_calls[0][1]["params"]
    assert stages == []
    assert driver.ctx._creature_corpus_discovered is True
    assert driver.ctx._creature_corpus_has_jobs is False
    assert driver.ctx._creature_corpus_executed is True


def test_all_creatures_wave_fails_closed_without_record_run_execution(
    tmp_path: Path,
) -> None:
    from bacup_lib.source_pairs import (
        FNV_MVP_EXCLUDE_SIGNATURES,
        mvp_creature_corpus_policy_payload,
    )
    from bacup_lib.workflows.unified import AssetWaveBuilder, AssetWaveToggles

    request = make_request(tmp_path)
    request.source_game = "fnv"
    driver = UnifiedDriver(request, sink_id=None)
    policy = mvp_creature_corpus_policy_payload(
        "fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES
    )
    assert policy is not None
    driver.ctx = SimpleNamespace(
        source_game="fnv",
        target_game="fo4",
        mod_path=tmp_path / "mod",
        mvp_creature_corpus_policy=policy,
    )
    builder = AssetWaveBuilder(
        driver,
        AssetWaveToggles(),
        SimpleNamespace(),
        StubRunner(),
    )

    with pytest.raises(RuntimeError, match="execution did not complete"):
        builder.build_wave_a4()


def test_all_creatures_discovery_rejects_incomplete_policy(tmp_path: Path) -> None:
    request = make_request(tmp_path)
    request.source_game = "skyrimse"
    ctx = SimpleNamespace(
        mod_path=tmp_path / "mod",
        mvp_creature_corpus_policy={"policy_id": "all_creatures_v1"},
    )

    with pytest.raises(RuntimeError, match="missing required fields"):
        _UnifiedRecordRuntime(request)._run_mvp_creature_corpus_discovery(
            SimpleNamespace(run_phase=lambda *args, **kwargs: {}),
            ctx,
            StubRunner(),
        )


def test_all_creatures_discovery_does_not_consume_external_prepared_manifest(
    tmp_path: Path,
) -> None:
    from bacup_lib.source_pairs import (
        SKYRIM_MVP_EXCLUDE_SIGNATURES,
        mvp_creature_corpus_policy_payload,
    )

    request = make_request(tmp_path)
    request.source_game = "skyrimse"
    policy = mvp_creature_corpus_policy_payload(
        "skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES
    )
    assert policy is not None
    mod_path = tmp_path / "mod"
    prepared_path = mod_path / "debug/creature_corpus/prepared_jobs.json"
    prepared_path.parent.mkdir(parents=True)
    prepared_path.write_text("not valid json", encoding="utf-8")
    ctx = SimpleNamespace(mod_path=mod_path, mvp_creature_corpus_policy=policy)
    calls: list[tuple[str, dict]] = []

    _UnifiedRecordRuntime(request)._run_mvp_creature_corpus_discovery(
        SimpleNamespace(
            run_phase=lambda phase, **kwargs: calls.append((phase, kwargs))
            or {"records_deferred": 1}
        ),
        ctx,
        StubRunner(),
    )

    assert calls == [
        (
            "discover_creature_corpus",
            {
                "mod_path": str(mod_path),
                "source_extracted_dir": "",
                "target_extracted_dir": None,
                "target_data_dir": None,
                "params": {
                    "profile": "all_creatures_v1",
                    "debug_dir": "debug/creature_corpus",
                },
            },
        )
    ]
    assert ctx._creature_corpus_has_jobs is True


def test_asset_run_keeps_output_sink_attachment(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from bacup_lib.workflows.unified import AssetRuns

    attached: list[tuple[int, int]] = []
    monkeypatch.setattr(
        unified_mod,
        "load_native_module",
        lambda: SimpleNamespace(
            sinks_attach_run=lambda run_id, sink_id: attached.append((run_id, sink_id))
        ),
    )
    runs = AssetRuns.__new__(AssetRuns)
    runs.textures = None
    runs.nifs = None
    runs.havok = None
    runs.sounds = None
    runs.textures = SimpleNamespace(id=91)

    runs.attach_sink(7)

    assert attached == [(91, 7)]


def test_asset_wave_pipeline_is_sequential():
    from bacup_lib.workflows.unified import WaveStage, _sequential_wave_plan

    stages = [
        WaveStage("first", 1, "mod", "source", {}),
        WaveStage("second", 2, "mod", "source", {}),
        WaveStage("third", 3, "mod", "source", {}, after=("first",)),
    ]

    assert [stage["after"] for stage in _sequential_wave_plan(stages)] == [
        [],
        ["first"],
        ["first", "second"],
    ]


def test_nif_only_asset_wave_remains_dependency_free(tmp_path):
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
        _sequential_wave_plan,
    )

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    toggles = AssetWaveToggles(
        nifs=True,
        btos=False,
        textures=False,
        materials=False,
        havok=False,
        drivers=False,
        sounds=False,
        animations=False,
    )
    runs = AssetRuns(driver.ctx, toggles)
    try:
        stages = AssetWaveBuilder(driver, toggles, runs, StubRunner()).build_wave_a2()
        plan = _sequential_wave_plan(stages)

        assert [stage["phase"] for stage in plan] == ["convert_nifs_v2"]
        assert plan[0]["after"] == []
    finally:
        runs.drop_all()


def test_harvest_terrain_products_logs_texture_job_handoff(tmp_path, monkeypatch):
    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = SimpleNamespace(_rust_conversion_run=SimpleNamespace(id=17))
    jobs = [
        {
            "diffuse_path": "source/soil_d.dds",
            "normal_path": "source/soil_n.dds",
            "reflectivity_path": "source/soil_r.dds",
            "lighting_path": "source/soil_l.dds",
            "output_prefix": "textures/terrain/appalachia/soil",
        }
    ]
    native = SimpleNamespace(
        conversion_run_terrain_texture_jobs_json=lambda run_id: unified_mod.json.dumps(
            jobs
        )
    )
    monkeypatch.setattr(unified_mod, "load_native_module", lambda: native)
    runner = StubRunner()

    driver._harvest_terrain_products(tmp_path, runner)

    assert driver.terrain_texture_jobs == jobs
    assert runner.logs == [("INFO", "Queued 1 LAND texture bundle(s) for textures_v2")]


def test_harvest_terrain_products_does_not_hide_transfer_failure(tmp_path, monkeypatch):
    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = SimpleNamespace(_rust_conversion_run=SimpleNamespace(id=17))

    def fail_transfer(run_id):
        raise RuntimeError("missing native binding")

    native = SimpleNamespace(conversion_run_terrain_texture_jobs_json=fail_transfer)
    monkeypatch.setattr(unified_mod, "load_native_module", lambda: native)

    with pytest.raises(RuntimeError, match="transfer LAND texture jobs"):
        driver._harvest_terrain_products(tmp_path, StubRunner())


def test_missing_nif_asset_logs_at_warn_not_error(tmp_path):
    """Regen review finding: an unresolved source NIF is a benign skip, not a
    conversion failure — it must log at WARN so the summary doesn't read as
    an error-worthy run."""
    from bacup_lib.models import AssetRef
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
    )

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    driver.ctx.assets.append(
        AssetRef(
            asset_type="nif",
            source_path="Meshes/missing.nif",
            resolved_path=None,
            resolution_error="source path did not resolve",
        )
    )
    toggles = AssetWaveToggles()
    runs = AssetRuns(driver.ctx, toggles)
    runner = StubRunner()
    try:
        builder = AssetWaveBuilder(driver, toggles, runs, runner)
        builder.build_wave_a2()

        assert driver.ctx.summary.nifs_failed == 1
        assert not any(
            level == "ERROR" and "NIF not found" in message
            for level, message in runner.logs
        )
        assert any(
            level == "WARN"
            and message
            == "NIF not found: Meshes/missing.nif: source path did not resolve"
            for level, message in runner.logs
        )
    finally:
        runs.drop_all()


def test_asset_planning_counters_merge_once_after_record_summary(tmp_path):
    from bacup_lib.models import AssetRef, ConversionSummary
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
    )

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    driver.ctx.summary.nifs_failed = 7
    driver.record_runtime._aggregate_summary.nifs_failed = 7
    driver.ctx.assets.append(
        AssetRef(
            asset_type="nif",
            source_path="Meshes/missing.nif",
            resolved_path=None,
            resolution_error="source path did not resolve",
        )
    )
    planning_summary = ConversionSummary(mod_path=str(tmp_path))
    toggles = AssetWaveToggles()
    runs = AssetRuns(driver.ctx, toggles)
    try:
        builder = AssetWaveBuilder(
            driver,
            toggles,
            runs,
            StubRunner(),
            planning_summary=planning_summary,
        )
        builder.build_wave_a2()
        unified_mod._merge_asset_planning_summary(
            driver.record_runtime._aggregate_summary, planning_summary
        )

        assert driver.ctx.summary.nifs_failed == 7
        assert planning_summary.nifs_failed == 1
        assert driver.record_runtime._aggregate_summary.nifs_failed == 8
    finally:
        runs.drop_all()


def test_asset_wave_worker_override_is_used_in_phase_params(tmp_path):
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
    )

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    driver.ctx.conversion_workers = 12
    toggles = AssetWaveToggles()
    runs = AssetRuns(driver.ctx, toggles, conversion_workers=3)
    try:
        builder = AssetWaveBuilder(
            driver,
            toggles,
            runs,
            StubRunner(),
            conversion_workers=3,
        )

        a2 = builder.build_wave_a2()
        nifs = next(s for s in a2 if s.phase == "convert_nifs_v2")
        btos = next(s for s in a2 if s.phase == "convert_btos_v2")
        assert nifs.params["conversion_workers"] == 3
        assert btos.params["conversion_workers"] == 3

        a3 = builder.build_wave_a3()
        textures = next(s for s in a3 if s.phase == "convert_textures_v2")
        assert textures.params["conversion_workers"] == 3
    finally:
        runs.drop_all()


def test_animtext_generation_uses_actual_output_esm_and_meshes_only(
    tmp_path,
    monkeypatch,
):
    from creation_lib.ck import automation
    from bacup_lib.workflows.unified import _run_anim_text_data_generation

    mod_dir = tmp_path / "mods" / "SeventySix"
    game_data_dir = tmp_path / "Fallout 4" / "Data"
    mod_dir.mkdir(parents=True)
    game_data_dir.mkdir(parents=True)
    (mod_dir / "SeventySix.esm").write_bytes(b"plugin")
    stale_anim_text = mod_dir / "data" / "meshes" / "animtextdata" / "stale.txt"
    stale_anim_text.parent.mkdir(parents=True)
    stale_anim_text.write_bytes(b"stale")
    # CreationKit.exe present → CK path is preferred (full-fidelity, all buckets).
    (game_data_dir.parent / "CreationKit.exe").write_bytes(b"")
    calls = []

    def fake_generate_anim_data(mod_name, **kwargs):
        assert not stale_anim_text.exists()
        calls.append((mod_name, kwargs))
        out_dir = mod_dir / "data" / "meshes" / "AnimTextData"
        out_dir.mkdir(parents=True)
        return out_dir

    monkeypatch.setattr(automation, "generate_anim_data", fake_generate_anim_data)

    ctx = SimpleNamespace(
        target_game="fo4",
        target_data_dir=game_data_dir,
        mod_path=mod_dir,
        output_plugin_name="SeventySix.esm",
    )
    runner = StubRunner()
    progress = PhaseProgress(
        phase=0,
        phase_name="Generate AnimTextData",
        status="running",
    )

    _run_anim_text_data_generation(ctx, runner, progress=progress)

    assert len(calls) == 1
    mod_name, kwargs = calls[0]
    assert mod_name == "SeventySix"
    assert kwargs["plugin_name"] == "SeventySix.esm"
    assert kwargs["game_dir"] == game_data_dir.parent
    assert kwargs["game_data_dir"] == game_data_dir
    assert kwargs["mod_dir"] == mod_dir
    assert kwargs["deploy_loose_data"] is True
    assert kwargs["loose_data_roots"] == ("Meshes",)
    assert callable(kwargs["on_progress"])
    assert any(
        level == "INFO" and "CK -GenerateAnimInfo" in message
        for level, message in runner.logs
    )
    assert any(
        level == "INFO" and "removed 1 stale AnimTextData tree" in message
        for level, message in runner.logs
    )


def test_animtext_generation_uses_native_when_ck_absent(tmp_path, monkeypatch):
    from bacup_lib import native_runtime
    from bacup_lib.workflows.unified import _run_anim_text_data_generation

    mod_dir = tmp_path / "mods" / "SeventySix"
    game_data_dir = tmp_path / "Fallout 4" / "Data"
    extracted_dir = tmp_path / "extracted" / "fo4"
    target_catalog = tmp_path / "target_assets.sqlite3"
    target_cache = tmp_path / "target_asset_cache"
    (mod_dir / "data" / "Meshes").mkdir(parents=True)
    stale_anim_text = mod_dir / "data" / "Meshes" / "AnimTextData" / "stale.txt"
    stale_anim_text.parent.mkdir(parents=True)
    stale_anim_text.write_bytes(b"stale")
    game_data_dir.mkdir(parents=True)
    extracted_dir.mkdir(parents=True)
    (mod_dir / "SeventySix.esm").write_bytes(b"plugin")
    (game_data_dir / "Fallout4.esm").write_bytes(b"master")
    # No CreationKit.exe → CK-free native path.

    generate_calls = []

    class FakeNative:
        def conversion_generate_anim_text_data(
            self,
            plugin_path,
            game,
            base_race_plugin_paths,
            src,
            out,
            **kwargs,
        ):
            assert not stale_anim_text.exists()
            generate_calls.append(
                (plugin_path, game, base_race_plugin_paths, src, out, kwargs)
            )
            kwargs["progress_callback"](
                r"derivable race 70/70: actors\wendigocolossus (2 subgraph(s))"
            )
            time.sleep(0.04)
            kwargs["progress_callback"]("AnimationFileData: wrote 4 file(s) in 0.1s")
            return 4, None

    monkeypatch.setattr(
        unified_mod,
        "_ANIM_TEXT_PROGRESS_HEARTBEAT_SECONDS",
        0.01,
    )
    monkeypatch.setattr(native_runtime, "load_native_module", lambda: FakeNative())

    ctx = SimpleNamespace(
        target_game="fo4",
        target_data_dir=game_data_dir,
        target_extracted_dir=extracted_dir,
        target_asset_store=object(),
        target_asset_catalog_path=target_catalog,
        target_asset_cache_dir=target_cache,
        mod_path=mod_dir,
        output_plugin_name="SeventySix.esm",
        mod_prefix="B21",
    )
    runner = StubRunner()
    progress = PhaseProgress(
        phase=0,
        phase_name="Generate AnimTextData",
        status="running",
    )

    _run_anim_text_data_generation(ctx, runner, progress=progress)

    assert len(generate_calls) == 1
    plugin_path, game, base_plugins, src, out, kwargs = generate_calls[0]
    assert plugin_path == str(mod_dir / "SeventySix.esm")
    assert game == "fo4"
    assert base_plugins == [str(game_data_dir / "Fallout4.esm")]
    assert src == str(mod_dir / "data" / "Meshes")
    assert out == str(mod_dir / "data" / "Meshes")
    assert kwargs["base_meshes_root"] is None
    assert kwargs["target_data_dir"] == str(game_data_dir)
    assert kwargs["target_catalog_path"] == str(target_catalog)
    assert kwargs["target_cache_dir"] == str(target_cache)
    assert kwargs["target_overlay_dir"] == str(extracted_dir)
    assert kwargs["mod_prefix"] == "B21"
    assert kwargs["creature_contract_json"] is None
    assert any(
        level == "INFO" and "CK-free generation" in message
        for level, message in runner.logs
    )
    assert any(
        level == "INFO" and "AnimationFileData: wrote 4" in message
        for level, message in runner.logs
    )
    assert any(
        level == "INFO" and "still working" in message for level, message in runner.logs
    )
    derivable_progress = r"derivable race 70/70: actors\wendigocolossus (2 subgraph(s))"
    assert derivable_progress in runner.item_progress
    assert any("still working" in message for message in runner.item_progress)
    assert runner.item_progress[-1] == "AnimationFileData: wrote 4 file(s) in 0.1s"
    assert (
        runner.item_progress.index(derivable_progress) < len(runner.item_progress) - 1
    )
    assert any(
        level == "INFO" and "wrote 4 AnimTextData bucket file(s)" in message
        for level, message in runner.logs
    )


def test_animtext_native_module_exposes_combined_binding():
    from bacup_lib.native_runtime import load_native_module

    native = load_native_module()
    assert callable(native.conversion_prepare_anim_text_data_assets)
    assert callable(native.conversion_generate_anim_text_data)
    assert not hasattr(
        native, "conversion_generate_anim_text_data_with_base_race_handles"
    )


def test_animtext_ck_wrapper_forwards_paths_and_progress(tmp_path, monkeypatch):
    from creation_lib._native import ck_native
    from creation_lib.ck.anim_text_data import generate_anim_text_data

    plugin = tmp_path / "Target.esp"
    source_meshes = tmp_path / "source" / "Meshes"
    output_meshes = tmp_path / "output" / "Meshes"
    base_meshes = tmp_path / "base" / "Meshes"
    base_plugins = [tmp_path / "Fallout4.esm", tmp_path / "DLCRobot.esm"]
    calls = []

    def fake_generate(*args):
        calls.append(args)
        args[-1]("records: decoded")
        return 7

    monkeypatch.setattr(ck_native, "ck_generate_anim_text_data", fake_generate)

    progress_messages = []
    progress_callback = progress_messages.append
    count = generate_anim_text_data(
        plugin,
        game="fo4",
        source_meshes_root=source_meshes,
        output_meshes_root=output_meshes,
        base_meshes_root=base_meshes,
        base_plugin_paths=base_plugins,
        mod_prefix="B21",
        progress_callback=progress_callback,
    )

    assert count == 7
    assert [args[:-1] for args in calls] == [
        (
            str(plugin),
            "fo4",
            str(source_meshes),
            str(output_meshes),
            str(base_meshes),
            [str(path) for path in base_plugins],
            "B21",
        )
    ]
    assert calls[0][-1] is progress_callback
    assert progress_messages == ["records: decoded"]


def test_animtext_ck_wrapper_forwards_worker_limit(tmp_path, monkeypatch):
    from creation_lib._native import ck_native
    from creation_lib.ck.anim_text_data import generate_anim_text_data

    calls = []

    def fake_generate(*args, **kwargs):
        calls.append((args, kwargs))
        return 1

    monkeypatch.setattr(ck_native, "ck_generate_anim_text_data", fake_generate)

    assert (
        generate_anim_text_data(
            tmp_path / "Target.esp",
            game="fo4",
            source_meshes_root=tmp_path / "source" / "Meshes",
            output_meshes_root=tmp_path / "output" / "Meshes",
            workers=3,
        )
        == 1
    )
    assert calls[0][1] == {"workers": 3}


def _write_creature_anim_text_ledgers(mod_dir: Path) -> tuple[Path, Path, Path]:
    debug_dir = mod_dir / "debug" / "creature_corpus"
    debug_dir.mkdir(parents=True)
    plan_path = debug_dir / "plan.json"
    execution_path = debug_dir / "execution_ledger.json"
    record_path = debug_dir / "record_commit_ledger.json"
    plan_path.write_text(
        json.dumps(
            {
                "version": 1,
                "jobs": [
                    {
                        "family_id": "family-canis",
                        "graph_paths": ["Actors/Canis/Behaviors/Canis.hkx"],
                        "artifacts": [],
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    execution_path.write_text(
        json.dumps(
            {
                "version": 1,
                "mode": "strict",
                "strict_aborted": False,
                "families": [
                    {
                        "family_id": "family-canis",
                        "disposition": "published",
                        "records_deferred": 0,
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    record_path.write_text(
        json.dumps(
            {
                "version": 2,
                "intent": {},
                "intent_blake3": "intent",
                "receipt": {
                    "family_ids": ["family-canis"],
                    "record_count": 2,
                    "mapping_count": 1,
                    "reserved_form_keys": [
                        "000800@Target.esp",
                        "01000801@Target.esp",
                    ],
                    "families": [
                        {
                            "family_id": "family-canis",
                            "record_count": 2,
                            "mapping_count": 1,
                            "primary_mappings": [
                                {"source": "Canis", "target": "01000800"}
                            ],
                            "races": [
                                {
                                    "form_key": "000800@Target.esp",
                                    "attack_events": ["attackStart"],
                                    "attack_data_entries": 1,
                                }
                            ],
                        }
                    ],
                },
                "receipt_blake3": "receipt",
            }
        ),
        encoding="utf-8",
    )
    return plan_path, execution_path, record_path


def _creature_anim_text_native_receipt() -> dict:
    return {
        "version": 1,
        "policy_id": "all_creatures_v1",
        "written": 2,
        "families": [
            {
                "family_id": "family-canis",
                "status": "passed",
                "race_form_keys": ["000800@Target.esp"],
                "graph_paths": ["Actors/Canis/Behaviors/Canis.hkx"],
                "emitted_files": [
                    "AnimTextData/AnimEventInfo/Canis.txt",
                    "AnimTextData/AnimationFileData/Canis.txt",
                ],
                "attack_events": [
                    {
                        "event": "attackStart",
                        "race_form_key": "000800@Target.esp",
                        "atkd_index": 0,
                        "source": "emitted_race_atkd_atke",
                        "targets": [
                            {
                                "graph_path": "Actors/Canis/Behaviors/Canis.hkx",
                                "clip_name": "Attack",
                                "annotation": "HitFrame",
                            }
                        ],
                    }
                ],
            }
        ],
    }


def test_creature_animtext_contract_accepts_matching_zero_attack_pair(tmp_path):
    from creation_lib.ck.anim_text_data import (
        _creature_anim_text_receipt,
        _record_commit_attack_contract,
    )

    _, _, record_path = _write_creature_anim_text_ledgers(tmp_path)
    payload = json.loads(record_path.read_text(encoding="utf-8"))
    race = payload["receipt"]["families"][0]["races"][0]
    race["attack_events"] = []
    race["attack_data_entries"] = 0
    record_path.write_text(json.dumps(payload), encoding="utf-8")

    expected_attacks = _record_commit_attack_contract(
        record_path, ("family-canis",)
    )
    assert expected_attacks == {
        "family-canis": {"000800@target.esp": frozenset()}
    }

    native_receipt = _creature_anim_text_native_receipt()
    native_receipt["written"] = 1
    native_receipt["families"][0]["emitted_files"] = [
        "AnimTextData/AnimationFileData/Canis.txt"
    ]
    native_receipt["families"][0]["attack_events"] = []
    receipt = _creature_anim_text_receipt(
        native_receipt,
        expected_family_ids=("family-canis",),
        policy_id="all_creatures_v1",
        expected_attacks=expected_attacks,
        expected_graphs={
            "family-canis": frozenset({"actors/canis/behaviors/canis.hkx"})
        },
    )
    assert receipt.families[0].attack_events == ()


def test_creature_animtext_ck_contract_is_family_local(tmp_path, monkeypatch):
    from creation_lib._native import ck_native
    from creation_lib.ck.anim_text_data import generate_creature_anim_text_closure

    plan_path, execution_path, record_path = _write_creature_anim_text_ledgers(tmp_path)
    calls = []

    def fake_generate(contract_json, progress_callback, **kwargs):
        calls.append((json.loads(contract_json), progress_callback, kwargs))
        return json.dumps(_creature_anim_text_native_receipt())

    monkeypatch.setattr(
        ck_native,
        "ck_generate_creature_anim_text_closure",
        fake_generate,
        raising=False,
    )
    progress = []
    progress_callback = progress.append
    receipt = generate_creature_anim_text_closure(
        tmp_path / "Target.esp",
        game="fo4",
        source_meshes_root=tmp_path / "data" / "Meshes",
        output_meshes_root=tmp_path / "data" / "Meshes",
        corpus_plan_path=plan_path,
        execution_ledger_path=execution_path,
        record_commit_ledger_path=record_path,
        expected_family_ids=("family-canis",),
        policy_id="all_creatures_v1",
        progress_callback=progress_callback,
        workers=3,
    )

    assert receipt.written == 2
    assert receipt.families[0].attack_events[0].targets[0].clip_name == "Attack"
    contract, callback, kwargs = calls[0]
    assert contract["corpus_plan_path"] == str(plan_path)
    assert contract["execution_ledger_path"] == str(execution_path)
    assert contract["record_commit_ledger_path"] == str(record_path)
    assert contract["expected_family_ids"] == ["family-canis"]
    assert contract["event_source"] == "emitted_race_atkd_atke"
    assert contract["resolution_source"] == "emitted_family_graphs"
    assert "donor" not in json.dumps(contract).casefold()
    assert callback is progress_callback
    assert kwargs == {"workers": 3}


@pytest.mark.parametrize(
    ("mutate", "message"),
    [
        (
            lambda payload: payload["families"][0]["attack_events"][0].update(
                source="donor_event"
            ),
            "did not come from emitted RACE ATKD/ATKE",
        ),
        (
            lambda payload: payload["families"][0]["attack_events"][0].update(
                targets=[]
            ),
            "has no graph target",
        ),
    ],
)
def test_creature_animtext_receipt_rejects_non_source_or_unresolved_events(
    tmp_path, monkeypatch, mutate, message
):
    from creation_lib._native import ck_native
    from creation_lib.ck.anim_text_data import generate_creature_anim_text_closure

    plan_path, execution_path, record_path = _write_creature_anim_text_ledgers(tmp_path)
    payload = _creature_anim_text_native_receipt()
    mutate(payload)
    monkeypatch.setattr(
        ck_native,
        "ck_generate_creature_anim_text_closure",
        lambda *_args, **_kwargs: payload,
        raising=False,
    )

    with pytest.raises(RuntimeError, match=message):
        generate_creature_anim_text_closure(
            tmp_path / "Target.esp",
            game="fo4",
            source_meshes_root=tmp_path / "data" / "Meshes",
            output_meshes_root=tmp_path / "data" / "Meshes",
            corpus_plan_path=plan_path,
            execution_ledger_path=execution_path,
            record_commit_ledger_path=record_path,
            expected_family_ids=("family-canis",),
            policy_id="all_creatures_v1",
        )


def test_creature_animtext_contract_fails_when_native_seam_is_missing(
    tmp_path, monkeypatch
):
    from creation_lib._native import ck_native
    from creation_lib.ck.anim_text_data import generate_creature_anim_text_closure

    plan_path, execution_path, record_path = _write_creature_anim_text_ledgers(tmp_path)
    monkeypatch.delattr(
        ck_native, "ck_generate_creature_anim_text_closure", raising=False
    )

    with pytest.raises(RuntimeError, match="missing family-local CK native seam"):
        generate_creature_anim_text_closure(
            tmp_path / "Target.esp",
            game="fo4",
            source_meshes_root=tmp_path / "data" / "Meshes",
            output_meshes_root=tmp_path / "data" / "Meshes",
            corpus_plan_path=plan_path,
            execution_ledger_path=execution_path,
            record_commit_ledger_path=record_path,
            expected_family_ids=("family-canis",),
            policy_id="all_creatures_v1",
        )


def test_creature_animtext_workflow_requires_and_persists_family_receipt(
    tmp_path, monkeypatch
):
    from bacup_lib.workflows.unified import _run_creature_corpus_anim_text_closure

    mod_dir = tmp_path / "mod"
    (mod_dir / "data" / "Meshes").mkdir(parents=True)
    (mod_dir / "Target.esp").write_bytes(b"plugin")
    stale = mod_dir / "data" / "Meshes" / "AnimTextData" / "stale.txt"
    stale.parent.mkdir(parents=True)
    stale.write_text("stale", encoding="utf-8")
    plan_path, execution_path, record_path = _write_creature_anim_text_ledgers(mod_dir)
    receipt_payload = _creature_anim_text_native_receipt()

    class Receipt:
        written = 2

        def to_dict(self):
            return receipt_payload

    calls = []

    def fake_native(ctx, runner, plugin_path, **kwargs):
        calls.append((ctx, runner, plugin_path, kwargs))
        return Receipt()

    monkeypatch.setattr(unified_mod, "_run_anim_text_data_native", fake_native)
    ctx = SimpleNamespace(
        target_game="fo4",
        mod_path=mod_dir,
        output_plugin_name="Target.esp",
        mvp_creature_corpus_policy={
            "policy_id": "all_creatures_v1",
            "execution_mode": "strict",
            "debug_dir": "debug/creature_corpus",
        },
    )

    receipt = _run_creature_corpus_anim_text_closure(ctx, StubRunner())

    assert receipt.written == 2
    assert not stale.exists()
    assert calls[0][2] == mod_dir / "Target.esp"
    assert calls[0][3]["creature_family_ids"] == ("family-canis",)
    assert calls[0][3]["creature_corpus_plan"] == plan_path
    assert calls[0][3]["creature_execution_ledger"] == execution_path
    assert calls[0][3]["creature_record_commit_ledger"] == record_path
    assert (
        json.loads(
            (execution_path.parent / "anim_text_receipt.json").read_text(
                encoding="utf-8"
            )
        )
        == receipt_payload
    )
    assert ctx.creature_anim_text_receipt is receipt


def test_creature_animtext_workflow_fails_without_record_commit_receipt(tmp_path):
    from bacup_lib.workflows.unified import _run_creature_corpus_anim_text_closure

    mod_dir = tmp_path / "mod"
    (mod_dir / "Target.esp").parent.mkdir(parents=True)
    (mod_dir / "Target.esp").write_bytes(b"plugin")
    _, _, record_path = _write_creature_anim_text_ledgers(mod_dir)
    record_path.unlink()
    ctx = SimpleNamespace(
        target_game="fo4",
        mod_path=mod_dir,
        output_plugin_name="Target.esp",
        mvp_creature_corpus_policy={
            "policy_id": "all_creatures_v1",
            "execution_mode": "strict",
            "debug_dir": "debug/creature_corpus",
        },
    )

    with pytest.raises(RuntimeError, match="missing its record-commit ledger"):
        _run_creature_corpus_anim_text_closure(ctx, StubRunner())


def test_creature_animtext_is_mandatory_after_record_mutations_before_packaging():
    import inspect

    source = inspect.getsource(unified_mod.run_unified)
    animtext = source.index('"Generate Creature AnimTextData"')
    assert source.index('"Rebuild Cell Offsets"') < animtext
    assert animtext < source.index('"Pack BA2"')
    assert source.index("all_creatures_v1") < source.index(
        'getattr(request.options, "generate_anim_text_data", False)'
    )


def test_animtext_force_native_overrides_present_ck(tmp_path, monkeypatch):
    from bacup_lib import native_runtime
    from bacup_lib.workflows.unified import _run_anim_text_data_generation

    mod_dir = tmp_path / "mods" / "SeventySix"
    game_data_dir = tmp_path / "Fallout 4" / "Data"
    extracted_dir = tmp_path / "extracted" / "fo4"
    (mod_dir / "data" / "Meshes").mkdir(parents=True)
    game_data_dir.mkdir(parents=True)
    (extracted_dir / "Meshes").mkdir(parents=True)
    (mod_dir / "SeventySix.esm").write_bytes(b"plugin")
    # CK IS present — force_native must still pick the native path.
    (game_data_dir.parent / "CreationKit.exe").write_bytes(b"")

    calls = []

    class FakeNative:
        def conversion_generate_anim_text_data(
            self, plugin_path, game, base_plugins, src, out, **kwargs
        ):
            calls.append((plugin_path, game, base_plugins, src, out, kwargs))
            kwargs["progress_callback"](
                "derivable buckets: AnimationOffsets=2 in 0.1s"
            )
            return 7, None

    monkeypatch.setattr(native_runtime, "load_native_module", lambda: FakeNative())

    ctx = SimpleNamespace(
        target_game="fo4",
        target_data_dir=game_data_dir,
        target_extracted_dir=extracted_dir,
        mod_path=mod_dir,
        output_plugin_name="SeventySix.esm",
    )
    runner = StubRunner()

    _run_anim_text_data_generation(ctx, runner, force_native=True)

    assert len(calls) == 1  # native ran despite CreationKit.exe being present
    assert calls[0][0] == str(mod_dir / "SeventySix.esm")
    assert calls[0][2] == []
    assert calls[0][5]["base_meshes_root"] == str(extracted_dir / "Meshes")


def test_asset_waves_forward_collision_memo_disable(tmp_path):
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
    )

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    driver.ctx.disable_nif_collision_memo = True
    toggles = AssetWaveToggles()
    runs = AssetRuns(driver.ctx, toggles)
    try:
        builder = AssetWaveBuilder(driver, toggles, runs, StubRunner())

        a2 = builder.build_wave_a2()
        nifs = next(s for s in a2 if s.phase == "convert_nifs_v2")
        btos = next(s for s in a2 if s.phase == "convert_btos_v2")
        assert nifs.params["disable_collision_memo"] is True
        assert btos.params["disable_collision_memo"] is True
    finally:
        runs.drop_all()


def test_deferred_a2_shape_converts_terrain_nifs_in_late_a2(tmp_path):
    from bacup_lib.models import AssetRef
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
    )

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    grass_src = tmp_path / "source" / "Meshes" / "grass.nif"
    grass_src.write_bytes(b"grass")
    driver.ctx.assets.append(
        AssetRef(
            asset_type="nif",
            source_path="Meshes/grass.nif",
            resolved_path=str(grass_src),
        )
    )
    toggles = AssetWaveToggles()
    runs = AssetRuns(driver.ctx, toggles)
    try:
        builder = AssetWaveBuilder(driver, toggles, runs, StubRunner())

        a3 = builder.build_wave_a3()
        assert [s.phase for s in a3] == [
            "convert_textures_v2",
            "convert_materials_v2",
        ]

        a2 = builder.build_wave_a2()
        nifs = next(s for s in a2 if s.phase == "convert_nifs_v2")
        assert "Meshes/grass.nif" in [
            e["source_path"] for e in nifs.params["nif_paths"]
        ]
    finally:
        runs.drop_all()


def test_wave_a3_grass_topup_skips_base_game_twins(tmp_path):
    """The A3 grass top-up must apply the legacy _target_has_asset filter
    (asset_phases.py::_phase_convert_nifs_native_impl): terrain-manifest
    spelling twins of base-game NIFs (the delta key does not collapse the
    'meshes/' prefix) are base-game-skipped, not re-converted over wave A2's
    banked bytes."""
    from bacup_lib.models import AssetRef
    from bacup_lib.workflows.unified import (
        AssetRuns,
        AssetWaveBuilder,
        AssetWaveToggles,
    )

    class StubTargetIndex:
        def __init__(self, present: set[str]):
            self.present = present

        def has_asset(self, asset) -> bool:
            return str(asset.source_path).replace("\\", "/").lower() in self.present

    driver = UnifiedDriver(make_request(tmp_path), sink_id=None)
    driver.ctx = make_wave_ctx(tmp_path)
    # Record-graph spelling of a base-game grass NIF, seen by wave A2.
    grass_dir = tmp_path / "source" / "Landscape" / "Grass"
    grass_dir.mkdir(parents=True)
    twin_src = grass_dir / "forestgrassobj01.nif"
    twin_src.write_bytes(b"grass-graph")
    driver.ctx.assets.append(
        AssetRef(
            asset_type="nif",
            source_path="Landscape\\Grass\\ForestGrassObj01.nif",
            resolved_path=str(twin_src),
        )
    )
    driver.ctx.target_asset_index = StubTargetIndex(
        {
            "landscape/grass/forestgrassobj01.nif",
            "meshes/landscape/grass/forestgrassobj01.nif",
        }
    )
    toggles = AssetWaveToggles()
    runs = AssetRuns(driver.ctx, toggles)
    try:
        builder = AssetWaveBuilder(driver, toggles, runs, StubRunner())
        builder.build_wave_a2()
        skipped_after_a2 = driver.ctx.summary.nifs_base_game_skipped

        # Terrain appends the manifest spelling twin (different delta key:
        # 'meshes/' prefix) plus one genuinely-new grass NIF after the A2
        # snapshot.
        new_src = tmp_path / "source" / "Meshes" / "newgrass.nif"
        new_src.write_bytes(b"grass-new")
        driver.ctx.assets.extend(
            [
                AssetRef(
                    asset_type="nif",
                    source_path="meshes/Landscape/Grass/forestgrassobj01.nif",
                    resolved_path=str(twin_src),
                ),
                AssetRef(
                    asset_type="nif",
                    source_path="Meshes/newgrass.nif",
                    resolved_path=str(new_src),
                ),
            ]
        )

        a3 = builder.build_wave_a3()
        topups = [s for s in a3 if s.phase == "convert_nifs_v2"]
        assert len(topups) == 1
        assert [e["source_path"] for e in topups[0].params["nif_paths"]] == [
            "Meshes/newgrass.nif"
        ]
        # Legacy-consistent accounting: the twin lands in base_game_skipped,
        # not converted/failed.
        assert driver.ctx.summary.nifs_base_game_skipped == skipped_after_a2 + 1
        assert driver.ctx.summary.nifs_failed == 0
    finally:
        runs.drop_all()


def _load_regen_module():
    import importlib.util

    path = Path(__file__).resolve().parents[5] / "scripts" / "regen.py"
    spec = importlib.util.spec_from_file_location("regen_plan7", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_regen_cli_surface():
    regen = _load_regen_module()
    import importlib.util

    cli_path = Path(__file__).resolve().parents[5] / "scripts" / "_conversion_cli.py"
    spec = importlib.util.spec_from_file_location("_conversion_cli", cli_path)
    conv_cli = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(conv_cli)

    parser = regen.build_parser(conv_cli)
    # Every CLI lever parses.
    args = parser.parse_args(
        [
            "--mod-name",
            "SeventySix",
            "--workers",
            "8",
            "--deploy",
            "--records-limit",
            "50000",
            "--max-seconds",
            "1500",
            "--max-asset-failures",
            "50",
            "--cpu-textures",
            "--validate-output",
            "--validation-warn-only",
            "--deep-invariants",
            "--no-export-yaml",
            "--memory-budget-probe",
            "--memory-report",
            "--serialize-tracks",
            "--asset-workers",
            "3",
            "--no-scripts",
            "--no-lod",
            "--exclude-record",
            "SCEN",
        ]
    )
    assert args.max_asset_failures == 50
    assert args.serialize_tracks is True
    assert args.asset_workers == 3
    assert args.export_yaml is False
    assert args.memory_report is True

    with pytest.raises(SystemExit):
        parser.parse_args(["--cache"])

    # --undeploy / --deploy / --deploy-only are mutually exclusive.
    with pytest.raises(SystemExit):
        parser.parse_args(["--deploy", "--undeploy"])


def test_record_failure_sets_record_failed_and_releases_waiters(tmp_path, monkeypatch):
    signals = TrackSignals()
    driver = UnifiedDriver(make_request(tmp_path), sink_id=None, signals=signals)
    recorded: list = []
    stub_record_runtime(driver, recorded, monkeypatch)
    monkeypatch.setattr(
        driver.record_runtime,
        "_collect_assets_native",
        lambda sp, ctx, runner: (_ for _ in ()).throw(RuntimeError("boom")),
    )

    with pytest.raises(RuntimeError, match="boom"):
        driver.run_record_track(StubRunner())

    assert signals.record_failed.is_set()
    # Waiters are released so the asset thread can observe the failure.
    assert signals.assets_ready.is_set()
    assert signals.fixups_done.is_set()
    assert signals.terrain_done.is_set()


def test_asset_track_failure_stops_record_track_at_phase_boundary(
    tmp_path, monkeypatch
):
    """A wave failure must abort the record track at its next phase boundary
    even before the rust run exists (the conversion_run_cancel half can't
    reach it yet)."""
    signals = TrackSignals()
    driver = UnifiedDriver(make_request(tmp_path), sink_id=None, signals=signals)
    recorded: list = []
    stub_record_runtime(driver, recorded, monkeypatch)
    driver.asset_track_failed.set()

    with pytest.raises(RuntimeError, match="asset track failed"):
        driver.run_record_track(StubRunner())

    # No phase ran past the boundary check.
    assert [item for kind, item in recorded if kind == "phase"] == []
    assert signals.record_failed.is_set()


def test_script_body_is_hollow_flags_stub_without_events():
    hollow = "Scriptname X Extends ObjectReference\nState waiting\nEndState\n"
    with_event = (
        "Scriptname X Extends ObjectReference\n"
        "Event OnActivate(ObjectReference akActionRef)\nEndEvent\n"
    )
    assert _script_body_is_hollow(hollow) is True
    assert _script_body_is_hollow(with_event) is False


def test_iter_top_level_papyrus_members_skips_in_state_members():
    lines = (
        "Scriptname X Extends ObjectReference\n"
        "Event OnLoad()\nEndEvent\n"
        "State waiting\n"
        "    Event OnActivate(ObjectReference akRef)\n    EndEvent\n"
        "EndState\n"
        "Float Function Helper()\n    return 1.0\nEndFunction\n"
    ).splitlines()
    members = _iter_top_level_papyrus_members(lines)
    names = {(kind, name) for kind, name, _s, _e in members}
    assert ("event", "onload") in names
    assert ("function", "helper") in names
    # The OnActivate inside the named State is NOT top-level.
    assert ("event", "onactivate") not in names


def test_shipped_example_patches_are_method_fragments():
    # The two authored example patches must resolve through the fix-folder API and
    # be method fragments (event bodies only, no whole-script header).
    for name in ("WindChimesActivatorScript", "WaterSourceActivatorScript"):
        source = _script_patch_source(name)
        assert source is not None, name
        assert "Event OnActivate" in source
        # A fragment has no Scriptname declaration line (the skeleton supplies it).
        assert not any(
            line.strip().lower().startswith("scriptname ")
            for line in source.splitlines()
        )


@pytest.mark.parametrize(
    ("name", "expected_members"),
    [
        ("DefaultPlayExplosionOnActivate", {("event", "onactivate")}),
        ("DefaultPlaySoundOnActivate", {("event", "onactivate")}),
        ("OnActivateCastSpell", {("event", "onactivate")}),
        (
            "HazardTriggerScript",
            {
                ("event", "ontriggerenter"),
                ("event", "ontriggerleave"),
                ("event", "ontimer"),
            },
        ),
        ("Quests:_Default:DisableRefOnActivate", {("event", "onactivate")}),
        ("DefaultRefSendStoryEvent", {("function", "sendconfiguredstoryevent")}),
        ("DefaultRefOnActivateSendEvent", {("event", "onactivate")}),
        ("DefaultRefOnTriggerEnterSendEvent", {("event", "ontriggerenter")}),
        (
            "DefaultRefOnDistanceSendEvent",
            {
                ("event", "onload"),
                ("event", "onunload"),
                ("event", "ondistancelessthan"),
                ("event", "ondistancegreaterthan"),
            },
        ),
        ("E09C_PlaySoundOnActivateScript", {("event", "onactivate")}),
        (
            "E08B_RadiationTriggerScript",
            {
                ("event", "ontriggerenter"),
                ("event", "ontriggerleave"),
                ("event", "ontimer"),
            },
        ),
        (
            "DefaultPlayExposionAtNodeOnActivate",
            {
                ("event", "onactivate"),
                ("event", "ontimer"),
                ("function", "playexplosion"),
            },
        ),
        ("SSE_LandmineTrigger_Script", {("event", "ontriggerenter")}),
        (
            "EN02_ExamRoomAVTriggerScript",
            {("event", "ontriggerenter"), ("event", "ontriggerleave")},
        ),
        (
            "MTN_MQ_3rdFloorTriggerScript",
            {("event", "ontriggerenter"), ("event", "ontriggerleave")},
        ),
        ("StormProjectorToggleEnableLinkedRef", {("event", "onactivate")}),
        ("DenizenEnableMarkerScript", {("event", "onload")}),
        (
            "SSE_ReEnableActivatorAfterTimer",
            {("event", "onactivate"), ("event", "ontimer")},
        ),
        (
            "BoSSetStageTriggerScript",
            {
                ("event", "ontriggerenter"),
                ("event", "ontriggerleave"),
                ("function", "trysetstage"),
            },
        ),
        ("BoS01PerPlayerSetStageTriggerScript", {("event", "ontriggerenter")}),
        ("BoSActivateMessageScript", {("event", "onactivate")}),
        ("BoSStartQuestTriggerScript", {("event", "ontriggerenter")}),
        ("FF05_Balance_SensorMessageScript", {("event", "onactivate")}),
        ("W05_RE_BlacklightActivatorScript", {("event", "onactivate")}),
        (
            "defaultonactivategiveitems",
            {
                ("event", "onactivate"),
                ("event", "ontimer"),
                ("function", "giveitems"),
            },
        ),
        ("Storm_SE09_ChickenExplode", {("event", "ondeath")}),
        (
            "DLC03HermitCrabSpawnChildScript",
            {
                ("event", "onload"),
                ("event", "ondeath"),
                ("event", "onunload"),
                ("function", "findmymommy"),
            },
        ),
        (
            "AudioActorPlaySound",
            {
                ("event", "onload"),
                ("event", "oncombatstatechanged"),
                ("event", "ontimer"),
                ("event", "oncelldetach"),
                ("event", "onunload"),
                ("event", "ondying"),
                ("function", "refreshsoundtimers"),
                ("function", "stopsoundtimers"),
            },
        ),
        (
            "Creatures:EyebotSuiciderScript",
            {
                ("event", "onload"),
                ("event", "oncombatstatechanged"),
                ("event", "ondistancelessthan"),
                ("event", "ondeath"),
                ("event", "onunload"),
                ("function", "registerforplayerproximity"),
                ("function", "unregisterforplayerproximity"),
            },
        ),
        ("DefaultActorIgnoreFriendlyHitsScript", {("event", "oninit")}),
        (
            "Creatures:ScorchbeastRaceScript",
            {
                ("event", "oneffectstart"),
                ("event", "onanimationevent"),
                ("event", "ontimer"),
                ("event", "oneffectfinish"),
                ("function", "registerscorchbeastanimationevents"),
                ("function", "unregisterscorchbeastanimationevents"),
                ("function", "updatestrafeweaponforstate"),
                ("function", "restoresonicweapon"),
                ("function", "placeconfiguredexplosion"),
                ("function", "startsonicattackcooldown"),
            },
        ),
        (
            "Creatures:MothmanCombatantScript",
            {
                ("event", "oneffectstart"),
                ("event", "actor.oncombatstatechanged"),
                ("event", "onanimationevent"),
                ("event", "ontimer"),
                ("event", "oneffectfinish"),
                ("function", "registercombatantevents"),
                ("function", "unregistercombatantevents"),
                ("function", "startaoeweapontimer"),
            },
        ),
        (
            "Creatures:MothmanDefenderScript",
            {
                ("event", "oneffectstart"),
                ("event", "actor.oncombatstatechanged"),
                ("event", "ondistancelessthan"),
                ("event", "ontimer"),
                ("event", "onanimationevent"),
                ("event", "oneffectfinish"),
                ("function", "registerdefenderevents"),
                ("function", "unregisterdefenderevents"),
                ("function", "entercombatantstate"),
            },
        ),
        (
            "Creatures:MothmanWatcherScript",
            {
                ("event", "oneffectstart"),
                ("event", "ondistancelessthan"),
                ("event", "onanimationevent"),
                ("event", "oneffectfinish"),
                ("function", "registerwatcherdistance"),
                ("function", "unregisterwatcherevents"),
            },
        ),
        (
            "Creatures:FlatwoodsMonsterRaceScript",
            {
                ("event", "oneffectstart"),
                ("event", "onanimationevent"),
                ("event", "oneffectfinish"),
            },
        ),
        (
            "Creatures:FlatwoodsMonsterWatcherScript",
            {
                ("event", "oneffectstart"),
                ("event", "ondistancelessthan"),
                ("event", "oneffectfinish"),
                ("function", "registerwatcherdistance"),
                ("function", "unregisterwatcherevents"),
            },
        ),
        (
            "Creatures:WendigoRaceScript",
            {
                ("event", "oneffectstart"),
                ("event", "onanimationevent"),
                ("event", "oneffectfinish"),
                ("function", "registerwendigoanimationevents"),
                ("function", "unregisterwendigoanimationevents"),
            },
        ),
        (
            "crOguaRaceScript",
            {
                ("event", "oneffectstart"),
                ("event", "onanimationevent"),
                ("event", "oneffectfinish"),
                ("function", "registeroguashellevents"),
                ("function", "unregisteroguashellevents"),
                ("function", "entershell"),
                ("function", "exitshell"),
            },
        ),
        (
            "Creatures:SheepsquatchRaceScript",
            {
                ("event", "oneffectstart"),
                ("event", "onhit"),
                ("event", "oneffectfinish"),
                ("function", "applysheepsquatchstage"),
                ("function", "updatesheepsquatchstage"),
            },
        ),
        (
            "Creatures:FloaterRaceScript",
            {
                ("event", "oneffectstart"),
                ("event", "onanimationevent"),
                ("event", "oneffectfinish"),
            },
        ),
        (
            "Creatures:FloaterGnasherBiteScript",
            {("event", "oneffectstart")},
        ),
        (
            "Creatures:FloaterScript",
            {("event", "ondying")},
        ),
    ],
)
def test_core_activator_patches_are_method_fragments(name, expected_members):
    source = _script_patch_source(name)

    assert source is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in source.splitlines()
    )
    members = {
        (kind, member_name)
        for kind, member_name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }
    assert expected_members <= members


def test_scorchbeast_patch_uses_fo4_weapon_fallback_and_all_animation_events():
    source = _script_patch_source("Creatures:ScorchbeastRaceScript")

    assert source is not None
    assert "SetEquippedWeaponAttacksEnabled" not in source
    assert "selfRef.UnequipItem(currentData.SonicAttackWeapon" in source
    assert "selfRef.EquipItem(currentData.SonicAttackWeapon" in source
    for event_variable in (
        "animEventStartCloakVFX",
        "animEventFlightLandingAttack",
        "animEventFlightLanded",
        "animEventGroundAreaAttack",
        "animEventGroundTakeoffAttack",
        "animEventGroundTakeoff",
        "animEventSonicAttack",
    ):
        assert f"RegisterForAnimationEvent(selfRef, {event_variable})" in source
        assert f"UnregisterForAnimationEvent(selfRef, {event_variable})" in source


def test_cryptid_watcher_patches_do_not_force_combat():
    for name in (
        "Creatures:MothmanWatcherScript",
        "Creatures:FlatwoodsMonsterWatcherScript",
    ):
        source = _script_patch_source(name)

        assert source is not None
        assert "RegisterForDistanceLessThanEvent" in source
        assert 'GoToState("disappear")' in source
        assert "StartCombat(" not in source
        assert "SetEnemy(" not in source


def test_cryptid_disappear_paths_disable_actor_after_teleport_event():
    for name in (
        "Creatures:MothmanCombatantScript",
        "Creatures:MothmanDefenderScript",
        "Creatures:MothmanWatcherScript",
        "Creatures:FlatwoodsMonsterWatcherScript",
    ):
        source = _script_patch_source(name)

        assert source is not None
        assert ".Disable()" in source

    flatwoods_patch = _script_patch_source("Creatures:FlatwoodsMonsterWatcherScript")
    assert flatwoods_patch is not None
    skeleton = (
        "Scriptname Creatures:FlatwoodsMonsterWatcherScript "
        "Extends ActiveMagicEffect\n"
        "actor selfRef\n"
        'String animEventTeleportStart = "TurnInvisible"\n'
        "sound Property DisappearSound Auto mandatory\n"
        "State disappear\n"
        "    Event OnAnimationEvent(ObjectReference akSource, String asEventName)\n"
        "        DisappearSound.Play(selfRef)\n"
        "    EndEvent\n"
        "EndState\n"
    )

    merged = _merge_script_method_patches(skeleton, flatwoods_patch)

    assert merged.count("State disappear") == 1
    assert merged.count("Event OnAnimationEvent") == 1
    assert "selfRef.Disable()" in merged
    assert "If DisappearSound != None" in merged


def test_mothman_aoe_weapon_cooldown_starts_after_attack_completion():
    source = _script_patch_source("Creatures:MothmanCombatantScript")

    assert source is not None
    aoe_start = source.index("If asEventName == animEventAoEAttackStart")
    attack_finished = source.index('ElseIf asEventName == "AttackEnd"', aoe_start)
    teleport_start = source.index(
        "ElseIf asEventName == animEventTeleportStart", attack_finished
    )
    aoe_start_branch = source[aoe_start:attack_finished]
    attack_finished_branch = source[attack_finished:teleport_start]
    assert "UnequipItem" not in aoe_start_branch
    assert 'RegisterForAnimationEvent(selfRef, "AttackEnd")' in aoe_start_branch
    assert 'RegisterForAnimationEvent(selfRef, "AttackStop")' in aoe_start_branch
    assert 'RegisterForAnimationEvent(selfRef, "AttackInterrupt")' in aoe_start_branch
    assert "selfRef.UnequipItem(AoEAttackWeapon, False, True)" in attack_finished_branch
    assert (
        "StartAoEWeaponTimer(AoEAttackDelayTime, AoEAttackDelayTime)"
        in attack_finished_branch
    )

    effect_finish = source[source.index("Event OnEffectFinish") :]
    assert "selfRef.UnequipItem(AoEAttackWeapon, False, True)" in effect_finish
    assert "selfRef.EquipItem(AoEAttackWeapon, False, True)" not in effect_finish


def test_p1_creature_patches_use_verified_fo4_fallbacks():
    wendigo = _script_patch_source("Creatures:WendigoRaceScript")
    ogua = _script_patch_source("crOguaRaceScript")
    sheepsquatch = _script_patch_source("Creatures:SheepsquatchRaceScript")
    floater = _script_patch_source("Creatures:FloaterRaceScript")
    gnasher_bite = _script_patch_source("Creatures:FloaterGnasherBiteScript")
    floater_actor = _script_patch_source("Creatures:FloaterScript")

    assert wendigo is not None
    assert "PlaceAtNode(sExplosionSpawnLocation, ScreamAttackExplosion)" in wendigo
    assert "PlaceAtMe(ScreamAttackExplosion)" in wendigo

    assert ogua is not None
    assert 'RegisterForAnimationEvent(mySelf, "TurnInvulnerable")' in ogua
    assert 'RegisterForAnimationEvent(mySelf, "TurnVulnerable")' in ogua
    assert "timesShelled >= ShellLimit" in ogua
    assert "ShellSpell.Cast(mySelf, mySelf)" in ogua
    assert "ShellExit" not in ogua

    assert sheepsquatch is not None
    assert "selfRef.GetValuePercentage(Health)" in sheepsquatch
    assert 'ApplySheepsquatchStage("stage2", Sheepsquatch_Stage2)' in sheepsquatch
    assert 'ApplySheepsquatchStage("stage3", Sheepsquatch_Stage3)' in sheepsquatch

    assert floater is not None
    assert "VampiricBiteSpell.Cast(floater, floater)" in floater
    assert gnasher_bite is not None
    assert "VampiricBiteSpell.Cast(akCaster, akCaster)" in gnasher_bite
    assert floater_actor is not None
    assert "Event OnDying(Actor akKiller)" in floater_actor
    assert "PlaceAtMe(DeathExplosion)" in floater_actor


def test_radio_general_patch_supplies_station_scheduler():
    source = _script_patch_source("RadioGeneral_MasterScript")

    assert source is not None
    assert "Event OnInit()" in source
    assert "Event Scene.OnEnd(Scene akSender)" in source
    assert "Function QueueNextScene()" in source
    assert "Scene Function PickNextScene()" in source
    assert "Scene Function ResolveScene(Int index)" in source
    assert (
        'Game.GetFormFromFile(songFormIDs[index], "SeventySix.esm") as Scene' in source
    )
    assert 'RegisterForRemoteEvent(nextScene, "OnEnd")' in source
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in source.splitlines()
    )

    skeleton = (
        "Scriptname RadioGeneral_MasterScript Extends QuestInstance\n"
        "songsDatum[] Property songsData Auto Mandatory\n"
    )
    augmented = _augment_fo76_to_fo4_script_skeleton(
        "RadioGeneral_MasterScript", skeleton
    )
    merged = _merge_script_method_patches(augmented, source)
    assert "Int[] Property songFormIDs Auto Const Mandatory" in merged
    assert "Event Scene.OnEnd(Scene akSender)" in merged
    assert "Scene Function PickNextScene()" in merged
    assert "Scene candidate = ResolveScene(index)" in merged
    assert (
        _augment_fo76_to_fo4_script_skeleton("RadioGeneral_MasterScript", augmented)
        == augmented
    )


def test_radio_general_merged_source_native_compiles_for_fo4():
    repo_root = Path(__file__).resolve().parents[5]
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = repo_root / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                candidates.append(Path(line.split("=", 1)[1].strip().strip('"')))
                break
    base_source = next(
        (
            game_root / "Data" / "Scripts" / "Source" / "Base"
            for game_root in candidates
            if (game_root / "Data" / "Scripts" / "Source" / "Base").is_dir()
        ),
        None,
    )
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    source_root = repo_root / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
    skeleton = (source_root / "RadioGeneral_MasterScript.psc").read_text(
        encoding="utf-8"
    )
    augmented = _augment_fo76_to_fo4_script_skeleton(
        "RadioGeneral_MasterScript", skeleton
    )
    patch = _script_patch_source("RadioGeneral_MasterScript")
    assert patch is not None
    merged = _merge_script_method_patches(augmented, patch)
    result = compile_psc(
        merged,
        imports=[str(source_root), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="RadioGeneral_MasterScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def _fo4_base_source_root() -> Path | None:
    repo_root = Path(__file__).resolve().parents[5]
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = repo_root / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                candidates.append(Path(line.split("=", 1)[1].strip().strip('"')))
                break
    return next(
        (
            game_root / "Data" / "Scripts" / "Source" / "Base"
            for game_root in candidates
            if (game_root / "Data" / "Scripts" / "Source" / "Base").is_dir()
        ),
        None,
    )


def _merge_and_compile_patch(script_name: str) -> str:
    """Merge a patch into its generated skeleton, compile it, return the source.

    Note the compiler does not resolve methods across scripts, so a successful
    compile says nothing about calls into another script; assert on the returned
    source for those.
    """
    base_source = _fo4_base_source_root()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    repo_root = Path(__file__).resolve().parents[5]
    source_root = repo_root / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
    relative = unified_mod._script_relative_path(script_name, ".psc")
    skeleton_path = source_root / relative
    if not skeleton_path.is_file():
        pytest.skip(f"generated skeleton unavailable: {relative}")

    augmented = _augment_fo76_to_fo4_script_skeleton(
        script_name, skeleton_path.read_text(encoding="utf-8")
    )
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(augmented, patch)
    result = compile_psc(
        merged,
        imports=[str(source_root), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=relative.name,
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
    return merged


def test_mothman_combatant_merged_source_native_compiles_for_fo4():
    merged = _merge_and_compile_patch("Creatures:MothmanCombatantScript")

    assert merged.count("Event OnAnimationEvent") == 1
    assert merged.count("Event OnEffectFinish") == 1


def test_radio_drama_patch_supplies_rotating_station_scheduler():
    source = _script_patch_source("RadioDramaRadio_MasterScript")

    assert source is not None
    assert "Event OnInit()" in source
    assert "Event Scene.OnEnd(Scene akSender)" in source
    assert "Function QueueNextScene()" in source
    assert "Scene Function PickNextScene()" in source
    # All three track lists must be reachable, not just songs.
    assert "PickFrom(dramasData, dramaFormIDs, lastDramaScenePlayed)" in source
    assert (
        "PickFrom(commercialsData, commercialFormIDs, lastCommercialScenePlayed)"
        in source
    )
    assert "PickFrom(songsData, songFormIDs, lastSongScenePlayed)" in source
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in source.splitlines()
    )

    skeleton = (
        "Scriptname RadioDramaRadio_MasterScript Extends QuestInstance\n"
        "tracksDatum[] Property songsData Auto Mandatory\n"
    )
    augmented = _augment_fo76_to_fo4_script_skeleton(
        "RadioDramaRadio_MasterScript", skeleton
    )
    for declaration in (
        "Int[] Property songFormIDs Auto Const Mandatory",
        "Int[] Property dramaFormIDs Auto Const Mandatory",
        "Int[] Property commercialFormIDs Auto Const Mandatory",
    ):
        assert declaration in augmented
    assert (
        _augment_fo76_to_fo4_script_skeleton("RadioDramaRadio_MasterScript", augmented)
        == augmented
    )


def test_muzak_radio_patches_drive_conditional_song_variable():
    quest_patch = _script_patch_source("SQRadio76QuestScript")
    assert quest_patch is not None
    assert "Function UpdateRadio()" in quest_patch
    assert "LastSong01 = CurrentSong" in quest_patch

    fragment_patch = _script_patch_source(
        "Fragments:Scenes:SF_SQ_Radio76_MuzakA_Scene01_004F7AF7"
    )
    assert fragment_patch is not None
    # Phase index 0 is the ident phase, ahead of the CurrentSong-gated phases.
    assert "Function Fragment_Phase_01_Begin()" in fragment_patch
    assert "radioQuest.CurrentSong = nextSong" in fragment_patch
    assert "radioQuest.UpdateRadio()" in fragment_patch


def test_radio_drama_merged_source_native_compiles_for_fo4():
    _merge_and_compile_patch("RadioDramaRadio_MasterScript")


def test_muzak_radio_merged_sources_native_compile_for_fo4():
    quest_source = _merge_and_compile_patch("SQRadio76QuestScript")
    fragment_source = _merge_and_compile_patch(
        "Fragments:Scenes:SF_SQ_Radio76_MuzakA_Scene01_004F7AF7"
    )

    # The compiler ignores unresolved cross-script methods, so pair the fragment's
    # call with the declaration it depends on rather than trusting the compile.
    assert "radioQuest.UpdateRadio()" in fragment_source
    assert "Function UpdateRadio()" in quest_source
    # The scene conditions read ::CurrentSong_var, which only exists when the
    # property survives decompilation as Conditional.
    assert "Int Property CurrentSong" in quest_source
    current_song = next(
        line
        for line in quest_source.splitlines()
        if line.strip().startswith("Int Property CurrentSong")
    )
    assert "conditional" in current_song.lower(), current_song


@pytest.mark.parametrize(
    ("source_type", "expected"),
    (
        ("QuestInstance", "Quest"),
        ("questinstance", "Quest"),
        ("QUESTINSTANCE", "Quest"),
        ("QuestInstance[]", "Quest[]"),
        ("questinstance[][]", "Quest[][]"),
    ),
)
def test_fo76_questinstance_type_adapts_to_fo4_quest(
    source_type: str,
    expected: str,
) -> None:
    assert unified_mod._fo76_to_fo4_script_type(source_type) == expected


def test_decompile_adapts_questinstance_extends_header(tmp_path, monkeypatch):
    import creation_lib.pex as pex_mod

    def fake_decompile(*_args, type_adapter=None, **_kwargs):
        assert type_adapter is not None
        return f"Scriptname W05_TestQuest Extends {type_adapter('QuestInstance')}\n"

    monkeypatch.setattr(pex_mod, "decompile_pex", fake_decompile)
    patch_dir = tmp_path / "patches"
    patch_dir.mkdir()
    monkeypatch.setattr(unified_mod, "_SCRIPT_PATCH_DIR", patch_dir)

    runtime = _UnifiedRecordRuntime(make_request(tmp_path))
    result = runtime._decompile_script_source_for_fo4(
        "W05_TestQuest",
        tmp_path / "W05_TestQuest.pex",
        SimpleNamespace(mod_path=tmp_path / "mod"),
        StubRunner(),
    )

    assert result is None
    written = (
        tmp_path / "mod" / "Scripts" / "Source" / "User" / "W05_TestQuest.psc"
    ).read_text(encoding="utf-8")
    assert written.startswith("Scriptname W05_TestQuest Extends Quest\n")


def test_skyrim_decompile_uses_fo4_type_and_api_adapters(tmp_path, monkeypatch):
    import dataclasses
    import creation_lib.pex as pex_mod

    def fake_decompile(
        *_args,
        type_adapter=None,
        skip_internal_functions=False,
        fo4_api_compat=False,
        **_kwargs,
    ):
        assert type_adapter is not None
        assert type_adapter("ArmorAddon") == "Form"
        assert type_adapter("SoundDescriptor[]") == "Sound[]"
        assert skip_internal_functions
        assert fo4_api_compat
        return "Scriptname SkyrimObjectScript Extends ObjectReference\n"

    monkeypatch.setattr(pex_mod, "decompile_pex", fake_decompile)
    patch_dir = tmp_path / "patches"
    patch_dir.mkdir()
    monkeypatch.setattr(unified_mod, "_SCRIPT_PATCH_DIR", patch_dir)
    request = dataclasses.replace(make_request(tmp_path), source_game="skyrimse")

    runtime = _UnifiedRecordRuntime(request)
    result = runtime._decompile_script_source_for_fo4(
        "SkyrimObjectScript",
        tmp_path / "SkyrimObjectScript.pex",
        SimpleNamespace(mod_path=tmp_path / "mod"),
        StubRunner(),
    )

    assert result is None
    written = (
        tmp_path
        / "mod"
        / "Scripts"
        / "Source"
        / "User"
        / "SkyrimObjectScript.psc"
    ).read_text(encoding="utf-8")
    assert written.startswith("Scriptname SkyrimObjectScript Extends ObjectReference\n")


def test_merge_appends_missing_event_and_keeps_skeleton():
    skeleton = (
        "Scriptname WindChimesActivatorScript Extends ObjectReference\n"
        "Form Property ResourceToGive Auto Mandatory\n"
        "State waitingforactivate\nEndState\n"
    )
    patch = (
        "Event OnActivate(ObjectReference akActionRef)\n"
        "    akActionRef.AddItem(ResourceToGive, 1)\n"
        "EndEvent\n"
    )
    merged = _merge_script_method_patches(skeleton, patch)
    # Skeleton declarations are preserved and the event is injected exactly once.
    assert "Scriptname WindChimesActivatorScript" in merged
    assert "Form Property ResourceToGive" in merged
    assert merged.count("Event OnActivate") == 1
    assert "akActionRef.AddItem(ResourceToGive, 1)" in merged


def test_merge_replaces_matching_top_level_stub():
    skeleton = (
        "Scriptname X Extends ObjectReference\n"
        "Event OnActivate(ObjectReference akRef)\n"
        "    ; stub — does nothing\n"
        "EndEvent\n"
    )
    patch = "Event OnActivate(ObjectReference akRef)\n    akRef.Disable()\nEndEvent\n"
    merged = _merge_script_method_patches(skeleton, patch)
    assert merged.count("Event OnActivate") == 1
    assert "akRef.Disable()" in merged
    assert "stub — does nothing" not in merged


def test_merge_replaces_member_inside_existing_named_state():
    skeleton = (
        "Scriptname X Extends ObjectReference\n"
        "Bool Property Enabled Auto\n"
        "Auto State Ready\n"
        "    Event OnActivate(ObjectReference akRef)\n"
        '        Debug.Trace("old")\n'
        "    EndEvent\n"
        "    Event OnLoad()\n"
        "        Enabled = True\n"
        "    EndEvent\n"
        "EndState\n"
    )
    patch = (
        "State Ready\n"
        "    Event OnActivate(ObjectReference akRef)\n"
        "        akRef.Disable()\n"
        "    EndEvent\n"
        "EndState\n"
    )

    merged = _merge_script_method_patches(skeleton, patch)

    assert merged.count("State Ready") == 1
    assert merged.count("Event OnActivate") == 1
    assert "akRef.Disable()" in merged
    assert 'Debug.Trace("old")' not in merged
    assert "Event OnLoad()" in merged
    assert "Bool Property Enabled Auto" in merged


def test_merge_state_rename_updates_declaration_and_exact_gotostate_target():
    skeleton = (
        "Scriptname X Extends ObjectReference\n"
        'String label = "default"\n'
        "Function Restore()\n"
        '    Self.GoToState("default")\n'
        "EndFunction\n"
        "Auto State default\n"
        "    Event OnLoad()\n"
        '        PlayAnimation("Reset")\n'
        "    EndEvent\n"
        "EndState\n"
    )

    merged = _merge_script_method_patches(
        skeleton, "; @state-rename default operational\n"
    )

    assert "Auto State operational" in merged
    assert 'Self.GoToState("operational")' in merged
    assert 'String label = "default"' in merged
    assert "Event OnLoad()" in merged
    assert "State default" not in merged


def test_merge_top_level_fragment_remains_backward_compatible_with_states():
    skeleton = (
        "Scriptname X Extends ObjectReference\n"
        "State Waiting\n"
        "    Event OnLoad()\n"
        "    EndEvent\n"
        "EndState\n"
    )
    patch = "Event OnActivate(ObjectReference akRef)\nEndEvent\n"

    merged = _merge_script_method_patches(skeleton, patch)

    assert merged.count("State Waiting") == 1
    assert merged.count("Event OnLoad()") == 1
    assert merged.count("Event OnActivate") == 1


def test_decompile_merges_patch_into_skeleton(tmp_path, monkeypatch):
    import creation_lib.pex as pex_mod

    skeleton = (
        "Scriptname WindChimesActivatorScript Extends ObjectReference\n"
        "Form Property ResourceToGive Auto Mandatory\n"
        "State waitingforactivate\nEndState\n"
    )
    monkeypatch.setattr(pex_mod, "decompile_pex", lambda *a, **k: skeleton)

    patch_dir = tmp_path / "patches"
    patch_dir.mkdir()
    (patch_dir / "WindChimesActivatorScript.psc").write_text(
        "Event OnActivate(ObjectReference akActionRef)\n"
        "    akActionRef.AddItem(ResourceToGive, 1)\nEndEvent\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(unified_mod, "_SCRIPT_PATCH_DIR", patch_dir)

    runtime = _UnifiedRecordRuntime(make_request(tmp_path))
    runner = StubRunner()
    ctx = SimpleNamespace(mod_path=tmp_path / "mod")

    result = runtime._decompile_script_source_for_fo4(
        "WindChimesActivatorScript", tmp_path / "src.pex", ctx, runner
    )

    assert result is None
    written = (
        tmp_path
        / "mod"
        / "Scripts"
        / "Source"
        / "User"
        / "WindChimesActivatorScript.psc"
    ).read_text(encoding="utf-8")
    assert "Scriptname WindChimesActivatorScript" in written
    assert "Form Property ResourceToGive" in written
    assert "akActionRef.AddItem(ResourceToGive, 1)" in written
    assert any("merged fix-folder method patch" in msg for _, msg in runner.logs)
