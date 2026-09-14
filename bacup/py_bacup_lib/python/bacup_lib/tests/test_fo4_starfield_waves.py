"""fo4:starfield wave configuration — registry, wave plan, and param shapes.

Covers the pair's integration surface without a live conversion: the native phase
registry, the pair's asset-wave plan, the three param builders unified.py hands
the Starfield phases, and regen.py's pair guards.
"""
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

import bacup_lib.workflows.unified as unified
from bacup_lib.native_runtime import load_native_module


def _asset(source_path: str, resolved_path: str = "", asset_type: str = "nif"):
    return SimpleNamespace(
        source_path=source_path,
        resolved_path=resolved_path or source_path,
        asset_type=asset_type,
    )


# ---------------------------------------------------------------------------
# Native registry
# ---------------------------------------------------------------------------


def test_registry_exposes_every_fo4_starfield_phase() -> None:
    names = load_native_module().conversion_run_list_phases()
    for phase in [
        "terrain_btd_write",
        "starfield_meshes",
        "starfield_materials",
        "starfield_textures",
        "wwise_audio",
        "audio_rewire",
        "starfield_worldspace_gate",
    ]:
        assert phase in names, phase


def test_registry_exposes_the_structured_cell_phase() -> None:
    # Separate from the list above so an older native build without this phase
    # skips only this check. Registration is asserted Rust-side by
    # `phase::mod::dispatcher_tests::fo4_starfield_phases_are_registered`;
    # this checks only whether the built extension carries it.
    names = load_native_module().conversion_run_list_phases()
    if "starfield_cells" not in names:
        pytest.skip(
            "the installed bacup_lib._native predates phase/starfield_cells.rs; "
            "run `uv run python scripts/ensure_native.py --package bacup`"
        )


# ---------------------------------------------------------------------------
# Wave plan
# ---------------------------------------------------------------------------


def test_wave_plan_for_starfield_target_replaces_every_fo4_asset_phase() -> None:
    plan = unified._wave_plan_for("fo4", "starfield")
    assert plan.starfield_target is True
    assert plan.nif_phase == "starfield_meshes"
    assert plan.material_phase == "starfield_materials"
    assert plan.texture_phase == "starfield_textures"
    # No FO4-target legs: no .bto port, no wave A4 havok/anim, no grass top-up.
    assert plan.bto_phase is None
    assert plan.wave_a4 is False
    assert plan.grass_topup is False


@pytest.mark.parametrize(
    "source_game,expected_nif_phase",
    [("fo76", "convert_nifs_v2"), ("skyrimse", "convert_nifs_v2"), ("fnv", "convert_gamebryo_nifs")],
)
def test_wave_plan_for_fo4_target_pairs_is_unchanged(
    source_game: str, expected_nif_phase: str
) -> None:
    plan = unified._wave_plan_for(source_game, "fo4")
    assert plan.starfield_target is False
    assert plan.nif_phase == expected_nif_phase


def test_starfield_fo4_required_mvp_fence_excludes_facegen_assets() -> None:
    from bacup_lib.source_pairs import STARFIELD_MVP_EXCLUDE_SIGNATURES

    driver = SimpleNamespace(
        ctx=SimpleNamespace(source_game="starfield", target_game="fo4"),
        _req=SimpleNamespace(options=SimpleNamespace(exclude_signatures=())),
    )

    excluded = unified._mvp_exclude_signatures(driver)

    assert excluded == STARFIELD_MVP_EXCLUDE_SIGNATURES
    assert not unified._is_world_only_mvp_asset_path(
        "Meshes/actors/character/facegendata/facegeom/starfield.esm/0022eeeb.nif",
        "nif",
        excluded,
    )
    assert unified._is_world_only_mvp_asset_path(
        "Meshes/Architecture/Outpost/wall01.nif", "nif", excluded
    )


def test_is_fo4_starfield_only_matches_the_forward_direction() -> None:
    assert unified._is_fo4_starfield(
        SimpleNamespace(source_game="fo4", target_game="starfield")
    )
    assert not unified._is_fo4_starfield(
        SimpleNamespace(source_game="starfield", target_game="fo4")
    )
    assert not unified._is_fo4_starfield(
        SimpleNamespace(source_game="fo76", target_game="fo4")
    )


# ---------------------------------------------------------------------------
# Param builders
# ---------------------------------------------------------------------------


def test_mesh_entries_strip_the_meshes_prefix() -> None:
    entries = unified.starfield_mesh_entries(
        [
            _asset("meshes/landscape/rock01.nif", "X:/fo4/meshes/landscape/rock01.nif"),
            _asset("Meshes\\Architecture\\wall.nif", "X:/fo4/Meshes/Architecture/wall.nif"),
        ]
    )
    assert [entry["nif_rel"] for entry in entries] == [
        "landscape/rock01.nif",
        "Architecture/wall.nif",
    ]
    assert entries[0]["resolved_path"] == "X:/fo4/meshes/landscape/rock01.nif"


def test_material_entries_keep_the_materials_prefix() -> None:
    # The prefix is load-bearing: `mat_out_rel(bgsm_rel)` is both the output
    # path under data/ and the MaterialID CRC input stamped into the NIF.
    entries = unified.starfield_material_entries(
        [_asset("Materials\\Weapons\\Foo.BGSM", "X:/fo4/Materials/Weapons/Foo.BGSM", "material")]
    )
    assert entries == [
        {
            "bgsm_rel": "Materials/Weapons/Foo.BGSM",
            "resolved_path": "X:/fo4/Materials/Weapons/Foo.BGSM",
        }
    ]


@pytest.fixture
def fake_bgsm(monkeypatch, tmp_path):
    """Stub `read_bgsm` so the set builder is tested, not the BGSM binary codec.

    `starfield_texture_sets` imports `read_bgsm` inside the function body, so
    patching the module attribute is enough.
    """
    from creation_lib.material_tools import bgsm_bin

    materials: dict[str, SimpleNamespace] = {}

    def make(
        rel: str,
        *,
        diffuse: str,
        normal: str = "",
        spec: str = "",
        glow: str = "",
        env_mapping: bool = False,
    ):
        path = tmp_path / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"BGSM")
        materials[str(path)] = SimpleNamespace(
            header=SimpleNamespace(env_mapping=env_mapping),
            DiffuseTexture=diffuse,
            NormalTexture=normal,
            SmoothSpecTexture=spec,
            GlowTexture=glow,
        )
        return _asset(rel.replace("\\", "/"), str(path), "material")

    def _read(handle):
        return materials[str(Path(handle.name))]

    monkeypatch.setattr(bgsm_bin, "read_bgsm", _read)
    return make


def test_texture_sets_are_named_from_the_diffuse_reference(fake_bgsm) -> None:
    asset = fake_bgsm(
        "Materials/Architecture/wall01.bgsm",
        diffuse="Architecture\\wall01_d.dds",
        normal="Architecture\\wall01_n.dds",
    )
    sets, warnings = unified.starfield_texture_sets([asset], {})

    assert warnings == []
    assert len(sets) == 1
    entry = sets[0]
    # `map_bgsm_to_mat_inputs` spells the .mat's color path
    # `Architecture\wall01_color.dds`; the texture phase writes
    # data/Textures/<output_subdir>/<base_name>_color.dds — they must agree.
    assert entry["base_name"] == "wall01"
    assert entry["output_subdir"] == "Architecture"
    assert entry["color"] == "Textures/Architecture/wall01_d.dds"
    assert entry["normal"] == "Textures/Architecture/wall01_n.dds"
    assert "spec_gloss" not in entry
    assert entry["environment_mapping"] is False


def test_texture_sets_prefer_the_graphs_resolved_texture_path(fake_bgsm) -> None:
    asset = fake_bgsm(
        "Materials/Architecture/wall03.bgsm", diffuse="Architecture\\wall03_d.dds"
    )
    resolved = {"textures/architecture/wall03_d.dds": "X:/fo4x/Textures/Architecture/wall03_d.dds"}
    sets, _ = unified.starfield_texture_sets([asset], resolved)
    assert sets[0]["color"] == "X:/fo4x/Textures/Architecture/wall03_d.dds"


def test_texture_sets_carry_the_env_mapping_flag_for_the_metal_channel(fake_bgsm) -> None:
    # A non-zero `_metal` map is emitted only when the source BGSM has
    # environment mapping enabled.
    asset = fake_bgsm(
        "Materials/Weapons/gun01.bgsm",
        diffuse="Weapons\\gun01_d.dds",
        spec="Weapons\\gun01_s.dds",
        env_mapping=True,
    )
    sets, warnings = unified.starfield_texture_sets([asset], {})
    assert warnings == []
    assert sets[0]["environment_mapping"] is True
    assert sets[0]["spec_gloss"] == "Textures/Weapons/gun01_s.dds"


def test_texture_sets_report_a_normal_map_outside_the_diffuse_set(fake_bgsm) -> None:
    asset = fake_bgsm(
        "Materials/Architecture/wall02.bgsm",
        diffuse="Architecture\\wall02_d.dds",
        normal="Shared\\flat_n.dds",
    )
    sets, warnings = unified.starfield_texture_sets([asset], {})
    assert len(sets) == 1
    assert any("does not share the diffuse set" in warning for warning in warnings)


def test_texture_sets_skip_a_bgsm_with_no_diffuse(fake_bgsm) -> None:
    asset = fake_bgsm("Materials/empty.bgsm", diffuse="")
    sets, warnings = unified.starfield_texture_sets([asset], {})
    assert sets == []
    assert any("no DiffuseTexture" in warning for warning in warnings)


def test_texture_sets_dedupe_two_bgsms_sharing_one_texture_family(fake_bgsm) -> None:
    first = fake_bgsm("Materials/A/shared.bgsm", diffuse="Shared\\rock_d.dds")
    second = fake_bgsm("Materials/B/shared.bgsm", diffuse="Shared\\rock_d.dds")
    sets, _ = unified.starfield_texture_sets([first, second], {})
    assert len(sets) == 1


# ---------------------------------------------------------------------------
# Record-track order
# ---------------------------------------------------------------------------


class _FakeDriver:
    """Just enough of UnifiedDriver to exercise the record tail's ordering."""

    def __init__(self, *, source_data_dir="X:/fo4x"):
        self.dispatched: list[tuple[str, dict, str]] = []
        self.labels: list[str] = []
        self.terrain_done = 0
        self._req = SimpleNamespace(source_data_dir=source_data_dir)

    def _record_phase(self, phase_no, label, body, runner, **kwargs):
        self.labels.append(label)
        body(SimpleNamespace())

    def _mark_terrain_done(self):
        self.terrain_done += 1

    def run_phase(self, name, mod_path="", params=None, **kwargs):
        self.dispatched.append((name, dict(params or {}), mod_path))
        return {}


def _record_tail(driver, *, convert_terrain=True, placeholder_audio=True, extent_cells=None):
    ctx = SimpleNamespace(
        source_game="fo4",
        target_game="starfield",
        mod_path=Path("X:/mods/Fallout4_SF"),
        source_data_dir="X:/fo4x",
        target_data_dir="X:/Starfield/Data",
        _rust_conversion_run=driver,
    )
    opts = SimpleNamespace(
        convert_terrain=convert_terrain,
        placeholder_audio=placeholder_audio,
        terrain=SimpleNamespace(worldspace_editor_id="", extent_cells=extent_cells),
    )
    runner = SimpleNamespace(emit_log=lambda *a, **k: None)
    return unified.UnifiedDriver._run_fo4_starfield_record_tail(
        driver, ctx, object(), runner, opts, 0
    )


def test_record_tail_order_is_btd_then_cells_then_wwise_then_rewire() -> None:
    driver = _FakeDriver()
    used = _record_tail(driver)

    assert used == 4
    assert [name for name, _, _ in driver.dispatched] == [
        "terrain_btd_write",
        "starfield_cells",
        "wwise_audio",
        "audio_rewire",
    ]
    # terrain_btd_write, starfield_cells and wwise_audio all resolve FormKeys
    # through mapper_state; audio_rewire consumes wwise_audio's manifest AND
    # patches XCMO/XCAS on the CELL records starfield_cells creates, so it
    # must stay last. starfield_cells sits after terrain_btd_write only so the
    # asset track's A3 gate is released first -- they have no data dependency.
    assert driver.dispatched[0][1] == {"worldspace_editor_id": "Commonwealth"}
    assert driver.dispatched[1][1] == {}
    assert driver.dispatched[2][1] == {"placeholder_audio": True}
    assert driver.terrain_done == 1


def test_record_tail_crops_the_btd_when_terrain_extent_cells_is_set() -> None:
    driver = _FakeDriver()
    _record_tail(driver, extent_cells=32)
    assert driver.dispatched[0][1]["terrain_extent_cells"] == 32


def test_record_tail_still_releases_the_terrain_gate_without_terrain() -> None:
    driver = _FakeDriver()
    used = _record_tail(driver, convert_terrain=False)
    assert [name for name, _, _ in driver.dispatched] == [
        "starfield_cells",
        "wwise_audio",
        "audio_rewire",
    ]
    assert used == 3
    # The asset track's A3 wave blocks on this signal.
    assert driver.terrain_done == 1


def test_record_tail_refuses_real_audio_without_a_wwise_toolchain(monkeypatch) -> None:
    monkeypatch.delenv("WWISEROOT", raising=False)
    driver = _FakeDriver()
    with pytest.raises(RuntimeError, match="placeholder-audio"):
        _record_tail(driver, placeholder_audio=False)


def test_record_tail_builds_wwise_tools_from_wwiseroot(monkeypatch) -> None:
    monkeypatch.setenv("WWISEROOT", str(Path("X:/Wwise2021")))
    driver = _FakeDriver()
    _record_tail(driver, placeholder_audio=False)
    tools = driver.dispatched[2][1]["tools"]
    assert tools["wwise_console"].endswith("WwiseConsole.exe")
    assert tools["copy_streamed_files"].endswith("CopyStreamedFiles.exe")
    assert tools["ffmpeg_cmd"][0]
    assert "wwise" in tools["wwise_project_src"].lower()


# ---------------------------------------------------------------------------
# regen.py pair guards
# ---------------------------------------------------------------------------


def _regen_parser():
    import sys

    scripts_dir = Path(__file__).resolve().parents[5] / "scripts"
    if str(scripts_dir) not in sys.path:
        sys.path.insert(0, str(scripts_dir))
    import _conversion_cli as conv_cli
    import regen

    return regen.build_parser(conv_cli)


def test_regen_rejects_expanded_archives_for_fo4_starfield() -> None:
    parser = _regen_parser()
    with pytest.raises(SystemExit):
        parser.parse_args(["--pair", "fo4:starfield", "--expanded-archives"])


def test_regen_accepts_the_fo4_starfield_mvp_placeholder_audio_run() -> None:
    parser = _regen_parser()
    args = parser.parse_args(
        [
            "--pair",
            "fo4:starfield",
            "--mvp",
            "--placeholder-audio",
            "--terrain-extent-cells",
            "32",
        ]
    )
    assert args.placeholder_audio is True
    assert args.terrain_extent_cells == 32
    assert args.mod_name == "Fallout4_SF"
    # The pair has no LOD pipeline yet; the generic non-default-pair branch
    # would otherwise leave lod_mode="generate".
    assert args.lod_mode == "none"


def test_regen_rejects_the_starfield_only_flags_on_other_pairs() -> None:
    parser = _regen_parser()
    with pytest.raises(SystemExit):
        parser.parse_args(["--pair", "fo76:fo4", "--placeholder-audio"])
    with pytest.raises(SystemExit):
        parser.parse_args(["--pair", "skyrimse:fo4", "--terrain-extent-cells", "32"])


def test_regen_paths_resolve_the_starfield_target_side(monkeypatch, tmp_path) -> None:
    import regen  # noqa: F401  (path set up by _regen_parser)

    _regen_parser()
    import regen as regen_module
    from bacup_lib.source_pairs import get_pair

    env = {
        "FO4_DIR": str(tmp_path / "Fallout4"),
        "FO4_DATA_DIR": str(tmp_path / "Fallout4" / "Data"),
        "FO4_EXTRACTED_DIR": str(tmp_path / "fo4_extracted"),
        "STARFIELD_DIR": str(tmp_path / "Starfield"),
        "STARFIELD_DATA_DIR": str(tmp_path / "Starfield" / "Data"),
        "STARFIELD_EXTRACTED_DIR": str(tmp_path / "sf_extracted"),
    }
    monkeypatch.setattr(
        regen_module, "_read_env_path", lambda name: Path(env[name]) if name in env else None
    )
    fo4_esm = tmp_path / "Fallout4" / "Data" / "Fallout4.esm"
    fo4_esm.parent.mkdir(parents=True, exist_ok=True)
    fo4_esm.write_bytes(b"TES4")

    paths = regen_module._resolve_regen_paths(
        "Fallout4_SF", pair=get_pair("fo4:starfield")
    )
    assert paths.target_data_dir == Path(env["STARFIELD_DATA_DIR"])
    assert paths.target_extracted_dir == Path(env["STARFIELD_EXTRACTED_DIR"])
    assert paths.source_data_dir == Path(env["FO4_DATA_DIR"])
    assert paths.target_asset_catalog_path.name == "starfield_target_assets.sqlite3"
