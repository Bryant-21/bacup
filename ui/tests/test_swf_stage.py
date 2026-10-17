"""Compiled placement edits retain all opaque data and timeline semantics."""
from dataclasses import replace
import struct
import threading

import pytest

from creation_lib.swf.types import BitWriter, MATRIX
from ui.swf_editor.document import SwfEditingDocument, RevisionJob
from ui.swf_editor.movie import Movie, MovieTag, encode_tag
from ui.swf_editor.stage import CompiledStage, compose, flatten, read_placement
from ui.tests.test_swf_editor_workflow import artwork, compiled


def tag(code, payload=b''):
    return MovieTag(code,payload,encode_tag(code,payload))


def placement(depth, character=None, matrix=None, name=None, move=False, suffix=b'', code=26):
    flags = int(move)|(2 if character is not None else 0)|(4 if matrix else 0)|(32 if name else 0)
    writer = BitWriter()
    writer.write_ui8(flags)
    if code in {70,94}:
        writer.write_ui8(8)
    writer.write_ui16(depth)
    if code in {70,94}:
        writer.write_string('ClassBeforeMatrix')
    if character is not None:
        writer.write_ui16(character)
    if matrix:
        matrix.write(writer)
    if name:
        writer.write_string(name)
    return tag(code,writer.getvalue()+suffix)


def timeline(tags, frames=1):
    base = Movie(artwork().build())
    base.header = base.header[:-2]+struct.pack('<H',frames)
    return base.assemble([base.tags[0],*tags,tag(0)])


def test_partial_updates_inherit_matrix_and_insert_only_missing_matrix():
    data = timeline([placement(7,1,MATRIX(translate_x=200),name='Meter'),tag(1),
                     placement(7,name='Renamed',move=True,suffix=b'opaque'),tag(1),tag(1),
                     placement(7,matrix=MATRIX(translate_x=800),move=True),tag(1)],4)
    stage = CompiledStage(data)
    node = stage.nodes(frame=2)[0]
    assert node.name == 'Renamed' and node.matrix.translate_x == 200
    modified = stage.set_matrix(node.address,replace(node.matrix,translate_x=600))
    after = CompiledStage(modified)
    assert after.nodes(frame=1)[0].matrix.translate_x == 200
    assert after.nodes(frame=2)[0].matrix.translate_x == 600
    assert after.nodes(frame=3)[0].matrix.translate_x == 600
    assert after.nodes(frame=4)[0].matrix.translate_x == 800
    before_tag,after_tag = Movie(data).tags[3],Movie(modified).tags[3]
    p,q = read_placement(before_tag),read_placement(after_tag)
    assert before_tag.payload[p.matrix_span[1]:] == after_tag.payload[q.matrix_span[1]:]
    assert after_tag.payload.endswith(b'opaque')


@pytest.mark.parametrize('code',[4,26,70,94])
def test_matrix_patch_keeps_class_names_actions_and_unknown_suffix(code):
    if code == 4:
        writer = BitWriter()
        writer.write_ui16(1)
        writer.write_ui16(3)
        MATRIX().write(writer)
        item = tag(4,writer.getvalue()+b'opaque rgb transform')
    else:
        item = placement(3,1,MATRIX(),name='Named',suffix=b'\xffopaque actions',code=code)
    data = timeline([item,tag(1)])
    stage = CompiledStage(data)
    node = stage.nodes()[0]
    after = Movie(stage.set_matrix(node.address,MATRIX(translate_x=-203,translate_y=408)))
    p,q = read_placement(item),read_placement(after.tags[1])
    assert item.payload[:p.matrix_span[0]] == after.tags[1].payload[:q.matrix_span[0]]
    assert item.payload[p.matrix_span[1]:] == after.tags[1].payload[q.matrix_span[1]:]
    assert q.matrix.translate_x == -203


def test_nested_sprite_patch_retains_abc_and_all_siblings_and_save_reopen(tmp_path):
    raw = Movie(compiled())
    nested = [placement(2,1,MATRIX(),name='Child'),tag(222,b'nested opaque'),tag(1),tag(0)]
    sprite = tag(39,struct.pack('<HH',40,1)+b''.join(t.encoded for t in nested))
    data = raw.assemble([item for item in raw.tags if item.code not in {0,1}]+
                        [sprite,placement(5,40,MATRIX(scale_x=2,translate_x=1000)),tag(1),tag(0)])
    stage = CompiledStage(data)
    child = list(flatten(stage.nodes()))[-1]
    assert child.owner == 40 and child.key == (5,2)
    changed = stage.set_matrix(child.address,MATRIX(translate_x=180,translate_y=80))
    before,after = Movie(data),Movie(changed)
    sprite_index = child.address[0]
    assert all(a.encoded == b.encoded for i,(a,b) in enumerate(zip(before.tags,after.tags)) if i != sprite_index)
    assert before.header == after.header
    assert before.tags[sprite_index].payload.endswith(nested[1].encoded+nested[2].encoded+nested[3].encoded)
    source = tmp_path/'original.swf'
    source.write_bytes(data)
    doc = SwfEditingDocument.open(source)
    doc.movie = changed
    doc.save(tmp_path/'session.swfproj')
    reopened = SwfEditingDocument.open(tmp_path/'session.swfproj')
    assert reopened.build() == changed and not reopened.dirty
    assert source.read_bytes() == data


def test_parent_rotation_composition_and_removal():
    parent = MATRIX(scale_x=0,scale_y=0,rotate_skew_0=2,rotate_skew_1=-2,translate_x=400,translate_y=600)
    result = compose(parent,MATRIX(translate_x=100,translate_y=200))
    assert (result.translate_x,result.translate_y) == (0,800)
    stage = CompiledStage(timeline([placement(4,1,MATRIX()),tag(1),tag(28,struct.pack('<H',4)),tag(1)],2))
    assert len(stage.nodes(frame=1)) == 1 and not stage.nodes(frame=2)


def test_external_job_cancellation_keeps_its_own_token():
    doc,jobs = SwfEditingDocument(),RevisionJob()
    started = threading.Event()
    def operation(snapshot,cancel):
        started.set()
        assert cancel.wait(2)
        return b'old'
    try:
        jobs.start(doc,operation,cancellable=True)
        assert started.wait(2)
        old = jobs.future
        jobs.cancel()
        jobs.start(doc,lambda snapshot,cancel: not cancel.is_set(),cancellable=True)
        assert old.result(timeout=3) == b'old'
        assert jobs.future.result(timeout=3) is True
        assert jobs.poll(doc) == ('build',True)
    finally:
        jobs.close()


def test_matrix_invalid_values_fail_before_change():
    stage = CompiledStage(timeline([placement(1,1,MATRIX()),tag(1)]))
    for value in (float('nan'),float('inf'),2**31):
        with pytest.raises(ValueError):
            stage.set_matrix(stage.nodes()[0].address,MATRIX(translate_x=value))
