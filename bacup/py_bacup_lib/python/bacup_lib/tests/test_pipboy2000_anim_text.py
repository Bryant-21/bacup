from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import pipboy2000_anim_text as step

CORE = "meshes/actors/character/_1stperson/behaviors/pipboy2000.hkx"
ANIMS = "meshes/actors/character/_1stperson/animations"

MINI_GRAPH = """<?xml version="1.0" encoding="ascii"?>
<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1" toplevelobject="#0090">
  <hksection name="__data__">
    <hkobject name="#0090" class="hkRootLevelContainer" signature="0x2772c11e">
      <hkparam name="namedVariants" numelements="1">
        <hkobject>
          <hkparam name="name">hkbBehaviorGraph</hkparam>
          <hkparam name="className">hkbBehaviorGraph</hkparam>
          <hkparam name="variant">#0091</hkparam>
        </hkobject>
      </hkparam>
    </hkobject>
    <hkobject name="#0091" class="hkbBehaviorGraph" signature="0xb1218f86">
      <hkparam name="name">Pipboy2000.hkb</hkparam>
      <hkparam name="rootGenerator">#0092</hkparam>
      <hkparam name="data">#0093</hkparam>
    </hkobject>
    <hkobject name="#0092" class="hkbClipGenerator" signature="0xd4cc9f6">
      <hkparam name="name">pipboyIdle</hkparam>
      <hkparam name="animationName">Animations\\Pipboy\\pipboyIdle.hkt</hkparam>
      <hkparam name="mode">MODE_LOOPING</hkparam>
    </hkobject>
    <hkobject name="#0093" class="hkbBehaviorGraphData" signature="0x907f9e6d">
      <hkparam name="stringData">#0094</hkparam>
    </hkobject>
    <hkobject name="#0094" class="hkbBehaviorGraphStringData" signature="0xc2d6f3b0">
    </hkobject>
  </hksection>
</hkpackfile>
"""


def _write(root: Path, relative: str, data: bytes = b"clip") -> Path:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def _layout(tmp_path: Path, graph: bytes = b"graph") -> tuple[Path, Path]:
    converted, base = tmp_path / "converted", tmp_path / "fo4"
    _write(converted, CORE, graph)
    _write(converted, f"{ANIMS}/pipboy2000/pipboyidle.hkx")
    _write(converted, "meshes/actors/character/_1stperson/behaviors/other.hkx")
    _write(base, f"{ANIMS}/pipboy/pipboyidle.hkx")
    _write(base, f"{ANIMS}/common/sneakoffset.hkx")
    return converted, base


def test_subgraph_id_is_the_engine_hash_of_graph_and_sapt_chain():
    from creation_lib.inspection.animation import subgraph_id

    assert subgraph_id(step.CORE_BEHAVIOR, step.SAPT_CHAIN) == step.SUBGRAPH_ID
    # Chain order is part of the id; keywords are not an input at all.
    assert subgraph_id(step.CORE_BEHAVIOR, step.SAPT_CHAIN[::-1]) != step.SUBGRAPH_ID


def test_emit_stages_all_inputs_as_base_and_publishes_only_this_subgraph(tmp_path, monkeypatch):
    converted, base = _layout(tmp_path)
    mod = tmp_path / "mod"
    atd = mod / "data" / "Meshes" / "AnimTextData"
    stale = _write(atd, f"AnimationOffsets/{step.SUBGRAPH_ID}.txt", b"stale")
    unrelated = _write(atd, "AnimationFileData/1.txt", b"keep")
    calls = []

    def generate(subgraphs_json, src, out, *, base_meshes_root, progress_callback):
        calls.append((subgraphs_json, Path(src), Path(base_meshes_root)))
        assert not any(Path(src).iterdir()), "nothing may sit where the generator rewrites graphs"
        staged = Path(base_meshes_root)
        assert (staged / CORE.removeprefix("meshes/")).is_file()
        assert (staged / "actors/character/_1stperson/animations/pipboy2000/pipboyidle.hkx").is_file()
        assert (staged / "actors/character/_1stperson/animations/common/sneakoffset.hkx").is_file()
        assert not (staged / "actors/character/_1stperson/behaviors/other.hkx").exists()
        _write(Path(out), f"AnimTextData/AnimationFileData/{step.SUBGRAPH_ID}.txt", b"files")
        _write(Path(out), "AnimTextData/SyncAnimData/ResolvedSyncAnimData_1stPerson.txt")
        progress_callback("done")
        return 2

    monkeypatch.setattr("bacup_lib.native_runtime.load_native_module",
                        lambda: SimpleNamespace(conversion_generate_subgraph_anim_text_data=generate))
    logs = []
    written = step.emit_pipboy2000_anim_text(mod, step.DataSource(converted), step.DataSource(base),
                                             log=logs.append)

    assert written == [atd / "AnimationFileData" / f"{step.SUBGRAPH_ID}.txt"]
    assert written[0].read_bytes() == b"files"
    assert not stale.exists()
    assert unrelated.read_bytes() == b"keep"
    assert not (atd / "SyncAnimData").exists()
    assert logs == ["done"]
    assert '"sapt_chain"' in calls[0][0] and "Pipboy2000.hkx" in calls[0][0]


def test_archive_entries_are_read_when_the_graph_is_not_loose(tmp_path, monkeypatch):
    converted, base = _layout(tmp_path)
    archived = {CORE: b"graph", f"{ANIMS}/pipboy2000/pipboyidle.hkx": b"clip"}
    for name in archived:
        (converted / name).unlink()
    fake_archive = SimpleNamespace(
        list_files=lambda prefix: [n for n in archived if n.startswith(prefix)],
        extract=archived.get, close=lambda: None)
    source = step.DataSource(converted)
    source.archives = [fake_archive]
    seen = []

    def generate(_json, _src, out, *, base_meshes_root, progress_callback):
        seen.append((Path(base_meshes_root) / CORE.removeprefix("meshes/")).read_bytes())
        _write(Path(out), f"AnimTextData/AnimationFileData/{step.SUBGRAPH_ID}.txt")

    monkeypatch.setattr("bacup_lib.native_runtime.load_native_module",
                        lambda: SimpleNamespace(conversion_generate_subgraph_anim_text_data=generate))
    step.emit_pipboy2000_anim_text(tmp_path / "mod", source, step.DataSource(base))
    assert seen == [b"graph"]


def test_missing_converted_graph_fails_after_removing_stale_output(tmp_path):
    converted, base = _layout(tmp_path)
    (converted / CORE).unlink()
    stale = _write(tmp_path / "mod/data/Meshes/AnimTextData",
                   f"AnimationFileData/{step.SUBGRAPH_ID}.txt")
    with pytest.raises(FileNotFoundError, match="arm graph"):
        step.emit_pipboy2000_anim_text(tmp_path / "mod", step.DataSource(converted),
                                       step.DataSource(base))
    assert not stale.exists()


def test_native_generator_resolves_clips_through_the_sapt_chain(tmp_path):
    from creation_lib.hkxpack import pack_xml_to_hkx

    xml = tmp_path / "graph.xml"
    xml.write_text(MINI_GRAPH, encoding="ascii")
    graph = tmp_path / "graph.hkx"
    pack_xml_to_hkx(str(xml), str(graph))
    converted, base = _layout(tmp_path, graph.read_bytes())

    written = step.emit_pipboy2000_anim_text(tmp_path / "mod", step.DataSource(converted),
                                             step.DataSource(base))

    file_data = tmp_path / f"mod/data/Meshes/AnimTextData/AnimationFileData/{step.SUBGRAPH_ID}.txt"
    assert file_data in written
    lines = file_data.read_text(encoding="ascii").splitlines()
    assert lines[:3] == ["3", "1", str(step.SUBGRAPH_ID)]
    clips = [line.lower() for line in lines[4:4 + int(lines[3])]]
    # The converted Pip-Boy 2000 clip shadows the vanilla one of the same name.
    assert r"actors\character\_1stperson\animations\pipboy2000\pipboyidle.hkx" in clips
    assert r"actors\character\_1stperson\animations\pipboy\pipboyidle.hkx" not in clips


def test_workflow_step_is_fo76_only_and_skips_without_fo4_data(tmp_path, monkeypatch):
    from bacup_lib.workflows.unified import _convert_fo76_pipboy2000_anim_text

    monkeypatch.setattr(step, "emit_pipboy2000_anim_text",
                        lambda *_args, **_kwargs: pytest.fail("must not run"))
    ctx = SimpleNamespace(mod_path=str(tmp_path), target_data_dir=None)
    skyrim = SimpleNamespace(source_game="skyrimse", target_game="fo4", target_data_dir=tmp_path)
    assert _convert_fo76_pipboy2000_anim_text(skyrim, ctx) == 0
    fo76 = SimpleNamespace(source_game="fo76", target_game="fo4", target_data_dir=None)
    assert _convert_fo76_pipboy2000_anim_text(fo76, ctx) == 0


def test_workflow_step_reads_the_converted_mod_and_fo4_data(tmp_path, monkeypatch):
    from bacup_lib.workflows.unified import _convert_fo76_pipboy2000_anim_text

    seen = {}

    def emit(mod, converted, base):
        seen.update(mod=mod, converted=converted.data_dir, base=base.data_dir)
        return [mod / "x.txt"]

    monkeypatch.setattr(step, "emit_pipboy2000_anim_text", emit)
    ctx = SimpleNamespace(mod_path=str(tmp_path / "mod"), target_data_dir=str(tmp_path / "fo4"))
    fo76 = SimpleNamespace(source_game="fo76", target_game="fo4", target_data_dir=None)
    assert _convert_fo76_pipboy2000_anim_text(fo76, ctx) == 1
    assert seen == {"mod": tmp_path / "mod", "converted": tmp_path / "mod" / "data",
                    "base": tmp_path / "fo4"}
