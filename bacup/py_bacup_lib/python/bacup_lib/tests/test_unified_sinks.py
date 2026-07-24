"""Sink join equivalence vs pack_mod + cross-language classifier pin. The
fixture exercises the reconcile path end-to-end: a sink with NOTHING streamed
must reproduce pack_mod's archives exactly."""
from __future__ import annotations

import json
import shutil
from pathlib import Path

from bacup_lib import _native as bacup_native
from creation_lib import _native as creation_native
from creation_lib.build.archive_plan import classify_archive_family
from creation_lib.build.packer import pack_mod
from bacup_lib.workflows.unified import finalize_sinks_for_mod

bsarchive = creation_native.bsarchive_native
directxtex = creation_native.directxtex_native
conversion = bacup_native.conversion_native


# SHARED FIXTURE TABLE — duplicated in
# bacup/py_bacup_lib/native/conversion/src/sinks/mod.rs (CLASSIFY_TABLE,
# Rust-side pin). Keep both lists in lockstep.
CLASSIFY_TABLE = [
    ("data/Textures/x.dds", "Textures"),
    ("Textures/x.dds", "Textures"),
    ("textures/sub/dir/y.DDS", "Textures"),
    ("Interface/i.swf", "Interface"),
    ("materials/a.bgsm", "Materials"),
    ("Misc/stray.bgem", "Materials"),
    ("Strings/SeventySix_en.STRINGS", "Strings"),
    ("Strings/SeventySix_en.dlstrings", "Strings"),
    ("Strings/SeventySix_en.ilstrings", "Strings"),
    ("sound/fx/a.xwm", "Sounds"),
    ("music/m.wav", "Sounds"),
    ("meshes/animations/a.hkx", "Animations"),
    ("misc/animations/note.txt", "Animations"),
    ("scripts/a.pex", "Scripts"),
    ("Scripts/Source/a.psc", "Scripts"),
    ("terrain/x.bto", "LOD"),
    ("meshes/terrain/world/x.btr", "LOD"),
    ("lod x.btr", "LOD"),
    ("meshes/a.nif", "Meshes"),
    ("stray.nif", "Meshes"),
    ("misc/readme.txt", "Main"),
    ("data/misc/readme.txt", "Main"),
    ("Vis/uvd/file.uvd", "Main"),
    ("noextension", "Main"),
]


def test_classifier_python_side_of_the_cross_language_pin():
    for rel, expected in CLASSIFY_TABLE:
        assert classify_archive_family(rel) == expected, f"classify({rel!r})"


def build_fixture_mod(project_root: Path, mod_name: str) -> Path:
    mod = project_root / "mods" / mod_name
    data = mod / "data"
    (data / "Meshes").mkdir(parents=True)
    (data / "Meshes" / "a.nif").write_bytes(b"NIF-ish bytes " * 200)
    (data / "Sound" / "fx").mkdir(parents=True)
    (data / "Sound" / "fx" / "s.xwm").write_bytes(b"X" * 4096)
    (data / "Scripts").mkdir(parents=True)
    (data / "Scripts" / "p.pex").write_bytes(b"PEX bytes " * 50)
    (data / "Textures").mkdir(parents=True)
    rgba = bytes([128, 64, 32, 255]) * (16 * 16)
    directxtex.write_dds_rgba(
        str(data / "Textures" / "t.dds"), 16, 16, rgba, "R8G8B8A8_UNORM", True, False
    )
    (mod / "Strings").mkdir(parents=True)
    (mod / "Strings" / f"{mod_name}_en.STRINGS").write_bytes(b"\x00" * 64)
    (mod / "Terrain").mkdir(parents=True)
    (mod / "Terrain" / "W.btd4").write_bytes(b"BTD4 sidecar")
    return mod


def archive_listing(path: Path) -> list[str]:
    return sorted(bsarchive.list_archive(str(path)))


def extracted_all(path: Path) -> dict[str, bytes]:
    return {name: bsarchive.extract_one(str(path), name) for name in archive_listing(path)}


def test_join_produces_legacy_identical_archives(tmp_path):
    mod_name = "X"
    proj_new = tmp_path / "proj_new"
    proj_legacy = tmp_path / "proj_legacy"
    mod = build_fixture_mod(proj_new, mod_name)
    legacy_mod = proj_legacy / "mods" / mod_name
    shutil.copytree(mod, legacy_mod)

    # Legacy oracle: pack_mod over the same tree.
    pack_mod(
        mod_name,
        pc=True,
        game="fo4",
        project_root=proj_legacy,
        archive_max_bytes=16 * 1024**3,
        expanded_archives=True,
    )

    # Full-run path: no BA2 spill, direct-pack the loose tree.
    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name=mod_name,
            archive_max_bytes=16 * 1024**3,
            direct_pack_all=True,
        )
        assert plans, "planner produced no archives"

        legacy = sorted(p.name for p in legacy_mod.glob("*.ba2"))
        new = sorted(p.name for p in mod.glob("*.ba2"))
        assert new == legacy, f"shard names differ: {new} vs {legacy}"
        for name in new:
            assert archive_listing(mod / name) == archive_listing(legacy_mod / name), name
            assert extracted_all(mod / name) == extracted_all(legacy_mod / name), name
        # Terrain sidecars must never be packed.
        assert not any(
            "terrain" in entry.lower() for n in new for entry in archive_listing(mod / n)
        )
        # Spills are deleted after a successful join.
        assert not (mod / "_sink_tmp" / "GNRL.spill").exists()
        assert not (mod / "_sink_tmp" / "DX10.spill").exists()
    finally:
        conversion.sinks_drop(sink_id)


def test_finalize_can_write_archives_to_deploy_dir(tmp_path):
    mod_name = "X"
    mod = build_fixture_mod(tmp_path, mod_name)
    (mod / "X - Meshes.ba2").write_bytes(b"old local archive")
    deploy_dir = tmp_path / "Fallout4" / "Data"
    deploy_dir.mkdir(parents=True)
    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name=mod_name,
            archive_max_bytes=16 * 1024**3,
            direct_pack_all=True,
            archive_output_dir=deploy_dir,
        )
    finally:
        conversion.sinks_drop(sink_id)

    expected_names = sorted(plan.output_name for plan in plans)
    assert expected_names
    assert sorted(path.name for path in deploy_dir.glob("*.ba2")) == expected_names
    assert not list(mod.glob("*.ba2"))
    assert not list(deploy_dir.glob("*.tmp"))


def test_finalize_archive_labels_only_packs_selected_families(tmp_path):
    mod_name = "SeventySix"
    mod = tmp_path / "mods" / mod_name
    meshes = mod / "data" / "Meshes"
    animations = meshes / "Actors" / "Fixture" / "Animations"
    materials = mod / "data" / "Materials"
    animations.mkdir(parents=True)
    materials.mkdir(parents=True)
    (meshes / "fixture.nif").write_bytes(b"nif")
    (animations / "idle.hkx").write_bytes(b"hkx")
    (materials / "fixture.bgsm").write_bytes(b"bgsm")
    stale_selected = mod / f"{mod_name} - MeshesExtra.ba2"
    untouched = mod / f"{mod_name} - Materials.ba2"
    stale_selected.write_bytes(b"stale")
    untouched.write_bytes(b"keep")

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name=mod_name,
            direct_pack_all=True,
            archive_labels=("Meshes", "MeshesExtra", "Animations"),
        )
    finally:
        conversion.sinks_drop(sink_id)

    assert {plan.family for plan in plans} == {"Meshes", "Animations"}
    assert (mod / f"{mod_name} - Meshes.ba2").is_file()
    assert (mod / f"{mod_name} - Animations.ba2").is_file()
    assert not stale_selected.exists()
    assert untouched.read_bytes() == b"keep"


def test_finalize_external_archive_root_packs_to_target_temp_then_replaces(
    tmp_path, monkeypatch
):
    from bacup_lib.workflows import unified

    mod = tmp_path / "mods" / "SeventySix"
    textures = mod / "data" / "Textures"
    textures.mkdir(parents=True)
    texture = textures / "a.dds"
    texture.write_bytes(b"dds")
    deploy_dir = tmp_path / "MO2" / "mods" / "SeventySix"
    captured_outputs: list[Path] = []

    class Native:
        def sinks_streamed(self, sink_id):
            return []

        def sinks_add_files(self, sink_id, items, workers):
            return len(items)

        def sinks_abort(self, sink_id):
            raise AssertionError("unexpected abort")

        def sinks_cleanup_spills(self, sink_id):
            return None

    class Entry:
        source_path = texture
        relative_path = "Textures/a.dds"
        size = 3

    class Plan:
        output_name = "SeventySix - Textures.ba2"
        texture_archive = True
        entries = (Entry(),)

    def fake_run_native_pack_plans(plans, *_args, **_kwargs):
        for _planned, output_path in plans:
            captured_outputs.append(Path(output_path))
            Path(output_path).write_bytes(b"BA2")
        return len(plans)

    monkeypatch.setattr(unified, "load_native_module", lambda: Native())
    monkeypatch.setattr(unified, "plan_archive_outputs", lambda *a, **k: [Plan()])
    monkeypatch.setattr(unified, "_run_native_pack_plans", fake_run_native_pack_plans)
    monkeypatch.setattr(unified, "_validate_archive_size", lambda *a, **k: None)

    unified.finalize_sinks_for_mod(
        1,
        mod,
        mod_name="SeventySix",
        direct_pack_all=True,
        archive_output_dir=deploy_dir,
    )

    final_archive = deploy_dir / "SeventySix - Textures.ba2"
    assert captured_outputs == [deploy_dir / "SeventySix - Textures.ba2.tmp"]
    assert final_archive.read_bytes() == b"BA2"
    assert not list(deploy_dir.glob("*.tmp"))
    assert not list(mod.glob("*.ba2"))


def test_join_mixed_streamed_and_reconciled(tmp_path):
    """Half the tree pre-streamed via the sink, half left for reconcile —
    the BA2 membership must still equal the full packable inventory."""
    mod_name = "Y"
    proj = tmp_path / "proj"
    mod = build_fixture_mod(proj, mod_name)

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": True,
            }
        )
    )
    try:
        # Pre-stream the NIF (as a phase would) — reconcile gets the rest.
        nif = mod / "data" / "Meshes" / "a.nif"
        added = conversion.sinks_add_files(sink_id, [(str(nif), "Meshes/a.nif")], None)
        assert added == 1
        assert conversion.sinks_streamed(sink_id) == ["meshes/a.nif"]

        finalize_sinks_for_mod(sink_id, mod, mod_name=mod_name)

        listed = set()
        for ba2 in mod.glob("*.ba2"):
            listed.update(n.replace("\\", "/").lower() for n in archive_listing(ba2))
        assert listed == {
            "meshes/a.nif",
            "sound/fx/s.xwm",
            "scripts/p.pex",
            "textures/t.dds",
            f"strings/{mod_name.lower()}_en.strings",
        }
    finally:
        conversion.sinks_drop(sink_id)


def test_join_compact_archives_remove_obsolete_generated_ba2s(tmp_path):
    mod_name = "Compact"
    proj = tmp_path / "proj"
    mod = build_fixture_mod(proj, mod_name)
    terrain_textures = mod / "data" / "Textures" / "Terrain" / "Appalachia"
    terrain_textures.mkdir(parents=True)
    shutil.copyfile(
        mod / "data" / "Textures" / "t.dds",
        terrain_textures / "Appalachia.4.0.0.dds",
    )
    shutil.copyfile(
        mod / "data" / "Textures" / "t.dds",
        terrain_textures / "lswamprocks01_d.dds",
    )
    stale_names = [
        f"{mod_name} - Meshes.ba2",
        f"{mod_name} - Materials.ba2",
        f"{mod_name} - LODTextures.ba2",
        f"{mod_name} - TerrainTextures.ba2",
        f"{mod_name} - Textures1.ba2",
    ]
    for stale_name in stale_names:
        (mod / stale_name).write_bytes(b"old archive")

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name=mod_name,
            archive_max_bytes=128 * 1024**3,
            direct_pack_all=True,
            expanded_archives=False,
        )

        assert [plan.output_name for plan in plans] == [
            f"{mod_name} - Main.ba2",
            f"{mod_name} - Textures.ba2",
        ]
        assert sorted(path.name for path in mod.glob("*.ba2")) == [
            f"{mod_name} - Main.ba2",
            f"{mod_name} - Textures.ba2",
        ]
        assert archive_listing(mod / f"{mod_name} - Main.ba2") == [
            "meshes/a.nif",
            "scripts/p.pex",
            "sound/fx/s.xwm",
            f"strings/{mod_name.lower()}_en.strings",
        ]
        assert archive_listing(mod / f"{mod_name} - Textures.ba2") == [
            "textures/t.dds",
            "textures/terrain/appalachia/appalachia.4.0.0.dds",
            "textures/terrain/appalachia/lswamprocks01_d.dds",
        ]
    finally:
        conversion.sinks_drop(sink_id)


def test_finalize_no_loose_reconciles_unstreamed_packable_files(tmp_path):
    mod = tmp_path / "mods" / "SeventySix"
    mesh_dir = (
        mod / "data" / "Meshes" / "actors" / "character" / "animations" / "common"
    )
    mesh_dir.mkdir(parents=True)
    (mesh_dir / "fx.nif").write_bytes(b"nif")

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": False,
                "enable_ba2": True,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name="SeventySix",
        )

        listed = set()
        for ba2 in mod.glob("*.ba2"):
            listed.update(n.replace("\\", "/").lower() for n in archive_listing(ba2))
        assert plans
        assert listed == {"meshes/actors/character/animations/common/fx.nif"}
    finally:
        conversion.sinks_drop(sink_id)


def test_finalize_direct_texture_pack_uses_worker_budget(tmp_path, monkeypatch):
    from bacup_lib.workflows import unified

    mod = tmp_path / "mods" / "SeventySix"
    textures = mod / "data" / "Textures"
    textures.mkdir(parents=True)
    texture = textures / "a.dds"
    texture.write_bytes(b"dds")
    calls: list[dict] = []

    class Native:
        def sinks_streamed(self, sink_id):
            return []

        def sinks_add_files(self, sink_id, items, workers):
            return len(items)

        def sinks_abort(self, sink_id):
            raise AssertionError("unexpected abort")

        def sinks_cleanup_spills(self, sink_id):
            return None

    class Entry:
        source_path = texture
        relative_path = "Textures/a.dds"
        size = 3

    class Plan:
        output_name = "SeventySix - Textures.ba2"
        texture_archive = True
        entries = (Entry(),)

    monkeypatch.setattr(unified, "load_native_module", lambda: Native())
    monkeypatch.setattr(unified, "plan_archive_outputs", lambda *a, **k: [Plan()])
    monkeypatch.setattr(unified, "discover_mod_archives", lambda *a, **k: [])
    monkeypatch.setattr(unified, "_validate_archive_size", lambda *a, **k: None)

    pack_events = []

    def pack_progress(event):
        pack_events.append(event)

    def fake_run_native_pack_plans(plans, game, **kwargs):
        calls.append({"game": game, "plans": len(plans), **kwargs})
        kwargs["progress"]({"completed": 1, "total": 1})
        for _planned, output_path in plans:
            Path(output_path).write_bytes(b"BA2")
        return len(plans)

    monkeypatch.setattr(
        unified,
        "_run_native_pack_plans",
        fake_run_native_pack_plans,
    )

    plans = unified.finalize_sinks_for_mod(
        1,
        mod,
        mod_name="SeventySix",
        texture_pack_workers=12,
        pack_progress=pack_progress,
    )

    assert plans
    assert calls[0].pop("progress") is pack_progress
    assert calls == [
        {
            "game": "fo4",
            "plans": 1,
            "og": False,
            "total_workers": 12,
        }
    ]
    assert pack_events == [{"completed": 1, "total": 1}]


def test_finalize_no_loose_reconciles_existing_packable_assets(tmp_path):
    mod = tmp_path / "mods" / "SeventySix"
    meshes = mod / "data" / "Meshes"
    strings = mod / "Strings"
    meshes.mkdir(parents=True)
    strings.mkdir(parents=True)
    (meshes / "reused.nif").write_bytes(b"nif")
    (strings / "SeventySix_en.STRINGS").write_bytes(b"strings")

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": False,
                "enable_ba2": True,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name="SeventySix",
        )

        listed = set()
        for ba2 in mod.glob("*.ba2"):
            listed.update(n.replace("\\", "/").lower() for n in archive_listing(ba2))
        assert plans
        assert listed == {
            "meshes/reused.nif",
            "strings/seventysix_en.strings",
        }
        assert not (mod / "_sink_tmp" / "GNRL.spill").exists()
        assert not (mod / "_sink_tmp" / "DX10.spill").exists()
    finally:
        conversion.sinks_drop(sink_id)


def test_direct_pack_all_covers_scripts_and_strings(tmp_path):
    mod = tmp_path / "mods" / "SeventySix"
    scripts = mod / "data" / "Scripts"
    strings = mod / "Strings"
    scripts.mkdir(parents=True)
    strings.mkdir(parents=True)
    (scripts / "p.pex").write_bytes(b"pex")
    (strings / "SeventySix_en.STRINGS").write_bytes(b"strings")

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name="SeventySix",
            direct_pack_all=True,
        )

        listed = set()
        for ba2 in mod.glob("*.ba2"):
            listed.update(n.replace("\\", "/").lower() for n in archive_listing(ba2))
        assert plans
        assert listed == {
            "scripts/p.pex",
            "strings/seventysix_en.strings",
        }
    finally:
        conversion.sinks_drop(sink_id)


def test_direct_pack_all_covers_ck_animtextdata(tmp_path):
    mod = tmp_path / "mods" / "SeventySix"
    animtext = mod / "data" / "Meshes" / "AnimTextData" / "AnimEventInfo"
    animtext.mkdir(parents=True)
    (animtext / "4152054059.txt").write_bytes(b"events")

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name="SeventySix",
            direct_pack_all=True,
        )

        listed = set()
        for ba2 in mod.glob("*.ba2"):
            listed.update(n.replace("\\", "/").lower() for n in archive_listing(ba2))
        assert plans
        assert listed == {
            "meshes/animtextdata/animeventinfo/4152054059.txt",
        }
    finally:
        conversion.sinks_drop(sink_id)


def test_direct_pack_all_covers_lodsettings(tmp_path):
    mod = tmp_path / "mods" / "SeventySix"
    lodsettings = mod / "data" / "LODSettings"
    lodsettings.mkdir(parents=True)
    (lodsettings / "APPALACHIA.lod").write_bytes(b"lod")

    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        plans = finalize_sinks_for_mod(
            sink_id,
            mod,
            mod_name="SeventySix",
            direct_pack_all=True,
        )

        listed = set()
        for ba2 in mod.glob("*.ba2"):
            listed.update(n.replace("\\", "/").lower() for n in archive_listing(ba2))
        assert plans
        assert listed == {"lodsettings/appalachia.lod"}
    finally:
        conversion.sinks_drop(sink_id)


def test_cache_manifest_write_and_consult(tmp_path):
    from bacup_lib.workflows.unified import (
        CacheAssetEntry,
        consult_cache,
        params_digest,
        write_cache_manifest,
    )

    mod_root = tmp_path / "mods" / "C"
    (mod_root / "data" / "Textures").mkdir(parents=True)
    src_dir = tmp_path / "source"
    src_dir.mkdir()
    src_a = src_dir / "a_d.dds"
    src_b = src_dir / "b_d.dds"
    src_a.write_bytes(b"texture A source")
    src_b.write_bytes(b"texture B source")
    out_a = mod_root / "data" / "Textures" / "a_d.dds"
    out_a.write_bytes(b"converted A")
    # b's output intentionally MISSING.

    digest = params_digest({"use_gpu": True, "textures": ["ignored-entry-list"]})
    # The entries list must not affect the digest.
    assert digest == params_digest({"use_gpu": True})
    assert digest != params_digest({"use_gpu": False})

    entries = [
        CacheAssetEntry(str(src_a), "textures", digest, ("Textures/a_d.dds",)),
        CacheAssetEntry(str(src_b), "textures", digest, ("Textures/b_d.dds",)),
    ]
    manifest_path = write_cache_manifest(mod_root, entries)
    assert manifest_path == mod_root / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    assert manifest["version"] == 1
    assert manifest["converters"]["textures"] == "1"
    assert set(manifest["assets"]) == {str(src_a), str(src_b)}
    rec = manifest["assets"][str(src_a)]
    assert rec["phase"] == "textures"
    assert rec["params_digest"] == digest
    assert rec["outputs"] == ["Textures/a_d.dds"]
    assert isinstance(rec["blake3"], str) and len(rec["blake3"]) == 64
    assert manifest["written_at"]

    # Consult: a hits (hash + version + digest match, output exists);
    # b misses (output absent).
    skip = consult_cache(manifest_path, entries, mod_root=mod_root)
    assert skip == {str(src_a)}

    # A changed source byte re-converts.
    src_a.write_bytes(b"texture A source CHANGED")
    assert consult_cache(manifest_path, entries, mod_root=mod_root) == set()
    src_a.write_bytes(b"texture A source")
    assert consult_cache(manifest_path, entries, mod_root=mod_root) == {str(src_a)}

    # A params-knob change re-converts.
    other = [
        CacheAssetEntry(str(src_a), "textures", params_digest({"use_gpu": False}), ("Textures/a_d.dds",)),
    ]
    assert consult_cache(manifest_path, other, mod_root=mod_root) == set()

    # A missing output re-converts.
    out_a.unlink()
    assert consult_cache(manifest_path, entries, mod_root=mod_root) == set()


def test_sidecar_registry_roundtrip(tmp_path):
    mod = tmp_path / "mods" / "Z"
    mod.mkdir(parents=True)
    sink_id = conversion.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod),
                "spill_dir": str(mod / "_sink_tmp"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        conversion.sinks_register_sidecar(sink_id, "Terrain/APPALACHIA.btd4")
        conversion.sinks_register_sidecar(sink_id, "Terrain/APPALACHIA.btd4")
        assert conversion.sinks_sidecars(sink_id) == ["Terrain/APPALACHIA.btd4"]
    finally:
        conversion.sinks_drop(sink_id)


def test_cleanup_temp_save_strings_removes_only_that_temp_stem(tmp_path):
    from bacup_lib.workflows.unified import _cleanup_temp_save_strings

    strings = tmp_path / "Strings"
    strings.mkdir()
    (strings / "SeventySix_en.STRINGS").write_bytes(b"real")
    (strings / ".SeventySix.esm.abc12345_en.STRINGS").write_bytes(b"orphan")
    (strings / ".SeventySix.esm.abc12345_ru.DLSTRINGS").write_bytes(b"orphan")
    (strings / ".SeventySix.esm.zzz99999_en.STRINGS").write_bytes(b"other run")

    _cleanup_temp_save_strings(tmp_path / ".SeventySix.esm.abc12345.tmp")

    assert (strings / "SeventySix_en.STRINGS").is_file()
    assert not (strings / ".SeventySix.esm.abc12345_en.STRINGS").exists()
    assert not (strings / ".SeventySix.esm.abc12345_ru.DLSTRINGS").exists()
    assert (strings / ".SeventySix.esm.zzz99999_en.STRINGS").is_file()
