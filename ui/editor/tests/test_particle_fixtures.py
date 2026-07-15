import os
from pathlib import Path

import pytest

from creation_lib.nif.nif_file import NifFile
from ui.editor.particles.model import build_particle_models


FIXTURE_RELATIVE_PATHS = (
    Path("Meshes/Effects/AttachFXMist01.nif"),
    Path("Meshes/Effects/MPSPlasmaTrail.nif"),
    Path("Meshes/Effects/ShaderParticles/CritLaserFXSP.nif"),
    Path("Meshes/Effects/ShaderParticles/DetectLifeFXSParticles.nif"),
)
def _candidate_fixture_roots() -> tuple[Path, ...]:
    roots = []
    if env_root := os.environ.get("FO4_EXTRACTED_DIR"):
        roots.append(Path(env_root))

    repo_root = Path(__file__).resolve().parents[3]
    roots.append(repo_root / "extracted" / "fo4")

    return tuple(dict.fromkeys(roots))


def _resolve_fixture_path(fixture_relative_path: Path) -> Path:
    candidate_paths = [root / fixture_relative_path for root in _candidate_fixture_roots()]
    for candidate_path in candidate_paths:
        if candidate_path.exists():
            return candidate_path

    searched_paths = ", ".join(str(candidate_path) for candidate_path in candidate_paths)
    pytest.skip(f"FO4 particle fixture is missing: {fixture_relative_path}; searched: {searched_paths}")


@pytest.mark.parametrize("fixture_relative_path", FIXTURE_RELATIVE_PATHS, ids=lambda path: path.name)
def test_particle_fixtures_extract_models(fixture_relative_path):
    fixture_path = _resolve_fixture_path(fixture_relative_path)

    nif = NifFile.load(str(fixture_path))
    models = build_particle_models(nif, nif_id=fixture_path.name)

    assert models, f"{fixture_path} produced no particle models"
    for model in models:
        assert model.system_block_id >= 0, f"{fixture_path} produced a model with invalid block id"
        assert model.nif_id == fixture_path.name, f"{fixture_path} produced a model with missing source identity"
        system_block = nif.get_block(model.system_block_id)
        assert system_block is not None, f"{fixture_path} model points to a missing particle system block"
        assert system_block.type_name, f"{fixture_path} model has no particle system type"

    assert any(
        model.emitter_block_id is not None
        or model.modifier_block_ids
        or model.controller_block_id is not None
        or model.emitter_mesh_block_ids
        for model in models
    ), f"{fixture_path} models did not expose emitter, modifier, controller, or shape metadata"


def test_mps_plasma_trail_extracts_particle_material_and_emitter_fields():
    fixture_path = _resolve_fixture_path(Path("Meshes/Effects/MPSPlasmaTrail.nif"))

    nif = NifFile.load(str(fixture_path))
    models = build_particle_models(nif, nif_id=fixture_path.name)
    plasma = next(model for model in models if model.name == "PlasmaTrail001")

    assert plasma.source_texture == r"textures\Effects\PlasmaProjectileAtlas_d.dds"
    assert plasma.base_color is not None
    assert plasma.base_color[1] == 1.0
    assert plasma.emitter_speed == 90.0
    assert plasma.emitter_declination == 1.5707963705062866
    assert plasma.emitter_planar_angle == 1.5707963705062866
    assert plasma.emitter_lifetime == 0.800000011920929
    assert plasma.emitter_initial_radius == 16.0
    assert plasma.emitter_radius == 0.20000000298023224
    assert 19 in plasma.modifier_block_ids
    assert plasma.modifier_parent_block_ids[plasma.modifier_block_ids.index(19)] == 16
    assert plasma.modifier_depths[plasma.modifier_block_ids.index(19)] == 1
    assert len(plasma.atlas_offsets) == 16
    assert plasma.atlas_offsets[1] == (0.25, 0.5, 0.0, 0.25)
