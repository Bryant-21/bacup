"""fo4:starfield packaging tail.

Starfield auto-loads ONLY `<plugin> - Main.ba2`, `- Textures.ba2`,
`- Voices_<lang>.ba2`, `- Localization.ba2` for a mod plugin
(bacup/docs/starfield_target/R1-esm-wrld-btd.md). These tests cover the pure
pack-plan classifier (`build_pack_plan`) and the packaging orchestrator
(`pack_fo4_starfield_archives`) with the native archive writer stubbed out.
"""
from __future__ import annotations

from pathlib import Path

import pytest

import bacup_lib.workflows.unified as unified


def _touch(path: Path, content: bytes = b"x") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)
    return path


def _seed_fixture_tree(data_root: Path) -> None:
    """One file per asset class the pair produces, plus a `.mat` for the loose case."""
    _touch(data_root / "meshes" / "a.nif")
    _touch(data_root / "geometries" / "x.mesh")
    _touch(data_root / "terrain" / "w.btd")
    _touch(data_root / "textures" / "t_color.dds")
    _touch(data_root / "sound" / "soundbanks" / "bank.bnk")
    _touch(data_root / "materials" / "foo.mat")


# ---------------------------------------------------------------------------
# build_pack_plan — pure classifier
# ---------------------------------------------------------------------------


def test_build_pack_plan_classifies_each_asset_kind(tmp_path: Path) -> None:
    _seed_fixture_tree(tmp_path)

    plan = unified.build_pack_plan(tmp_path)
    by_rel = {job.relative_path: job for job in plan}

    assert set(by_rel) == {
        "meshes/a.nif",
        "geometries/x.mesh",
        "terrain/w.btd",
        "textures/t_color.dds",
        "sound/soundbanks/bank.bnk",
        "materials/foo.mat",
    }

    main_archive = "Fallout4_SF - Main.ba2"
    textures_archive = "Fallout4_SF - Textures.ba2"

    for rel in ("meshes/a.nif", "geometries/x.mesh", "terrain/w.btd", "sound/soundbanks/bank.bnk"):
        job = by_rel[rel]
        assert job.destination == main_archive, rel
        assert job.pack_kind == "starfield", rel
        # The whole Main archive ships uncompressed, like vanilla SFBGS00D - Main.ba2.
        assert job.compress is False, rel

    tex_job = by_rel["textures/t_color.dds"]
    assert tex_job.destination == textures_archive
    assert tex_job.pack_kind == "starfielddds"
    assert tex_job.compress is True

    mat_job = by_rel["materials/foo.mat"]
    assert mat_job.destination == "loose"
    assert mat_job.pack_kind is None


def test_build_pack_plan_honors_a_custom_mod_name(tmp_path: Path) -> None:
    _touch(tmp_path / "terrain" / "w.btd")
    plan = unified.build_pack_plan(
        tmp_path, unified.Fo4StarfieldPackFlags(mod_name="SomeOtherMod")
    )
    assert plan[0].destination == "SomeOtherMod - Main.ba2"


def test_build_pack_plan_fails_loud_on_an_unrecognized_asset_class(tmp_path: Path) -> None:
    _touch(tmp_path / "scripts" / "foo.pex")
    with pytest.raises(ValueError, match="scripts/foo.pex"):
        unified.build_pack_plan(tmp_path)


# ---------------------------------------------------------------------------
# pack_fo4_starfield_archives — orchestrator (native packer stubbed)
# ---------------------------------------------------------------------------


@pytest.fixture
def fake_native_pack(monkeypatch):
    """Stub `_run_native_pack_entries`: records every call and writes a
    placeholder file so downstream size/discovery checks have something real
    to look at, without needing bsarchive_native or genuine archive bytes."""
    calls: list[dict] = []

    def fake(entries, output_path, game, *, texture_archive=False, compress=True, **_kwargs):
        calls.append(
            {
                "entries": entries,
                "output_path": output_path,
                "game": game,
                "texture_archive": texture_archive,
                "compress": compress,
            }
        )
        Path(output_path).write_bytes(b"BA2")

    monkeypatch.setattr(unified, "_run_native_pack_entries", fake)
    return calls


def test_pack_fo4_starfield_archives_produces_exactly_main_and_textures(
    tmp_path: Path, fake_native_pack
) -> None:
    mod_root = tmp_path / "Fallout4_SF"
    _seed_fixture_tree(mod_root / "data")

    planned = unified.pack_fo4_starfield_archives(mod_root)

    assert {p.output_name for p in planned} == {
        "Fallout4_SF - Main.ba2",
        "Fallout4_SF - Textures.ba2",
    }
    assert (mod_root / "Fallout4_SF - Main.ba2").is_file()
    assert (mod_root / "Fallout4_SF - Textures.ba2").is_file()

    by_output = {Path(call["output_path"]).name: call for call in fake_native_pack}
    main_call = by_output["Fallout4_SF - Main.ba2"]
    assert main_call["game"] == "starfield"
    assert main_call["texture_archive"] is False
    assert main_call["compress"] is False  # uncompressed Main

    textures_call = by_output["Fallout4_SF - Textures.ba2"]
    assert textures_call["game"] == "starfield"
    assert textures_call["texture_archive"] is True
    assert textures_call["compress"] is True

    # materials/**.mat never gets packed.
    mat_rels = {
        entry.relative_path
        for call in fake_native_pack
        for entry in call["entries"]
    }
    assert "materials/foo.mat" not in mat_rels
    assert (mod_root / "data" / "materials" / "foo.mat").is_file()


def test_pack_fo4_starfield_archives_pins_expanded_archives_false(
    tmp_path: Path, fake_native_pack
) -> None:
    mod_root = tmp_path / "Fallout4_SF"
    _seed_fixture_tree(mod_root / "data")

    with pytest.raises(RuntimeError, match="expanded_archives"):
        unified.pack_fo4_starfield_archives(mod_root, expanded_archives=True)
    # The pin is checked before any archive write.
    assert fake_native_pack == []


def test_pack_fo4_starfield_archives_fails_loudly_instead_of_sharding(
    tmp_path: Path, fake_native_pack
) -> None:
    mod_root = tmp_path / "Fallout4_SF"
    _seed_fixture_tree(mod_root / "data")

    with pytest.raises(RuntimeError, match="terrain-extent-cells|deploy-loose"):
        unified.pack_fo4_starfield_archives(mod_root, archive_max_bytes=1)
    # No archive should have been written for the oversize plan.
    assert fake_native_pack == []


def test_pack_fo4_starfield_archives_removes_stale_family_archives(
    tmp_path: Path, fake_native_pack
) -> None:
    mod_root = tmp_path / "Fallout4_SF"
    _seed_fixture_tree(mod_root / "data")
    # Simulate leftover output from a prior expanded_archives=True run.
    stale = _touch(mod_root / "Fallout4_SF - Terrain.ba2")

    unified.pack_fo4_starfield_archives(mod_root)

    assert not stale.exists()


# ---------------------------------------------------------------------------
# regen.py deploy guard — --deploy still writes Fallout4Custom.ini
# (scripts/regen.py's own _resolve_regen_paths() never made a Starfield INI
# path; --deploy/-only/--undeploy fall through to the FO4 one).
# ---------------------------------------------------------------------------


def _regen_parser():
    import sys

    scripts_dir = Path(__file__).resolve().parents[5] / "scripts"
    if str(scripts_dir) not in sys.path:
        sys.path.insert(0, str(scripts_dir))
    import _conversion_cli as conv_cli
    import regen

    return regen.build_parser(conv_cli)


@pytest.mark.parametrize("flag", ["--deploy", "--deploy-only", "--undeploy"])
def test_regen_rejects_bare_deploy_for_fo4_starfield(flag: str) -> None:
    parser = _regen_parser()
    with pytest.raises(SystemExit):
        parser.parse_args(["--pair", "fo4:starfield", flag])


def test_regen_allows_deploy_to_mo2_for_fo4_starfield(tmp_path: Path) -> None:
    parser = _regen_parser()
    args = parser.parse_args(
        [
            "--pair",
            "fo4:starfield",
            "--deploy",
            "--deploy-to-mo2",
            str(tmp_path),
        ]
    )
    assert args.deploy_to_mo2 == str(tmp_path)
