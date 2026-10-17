"""Native authored-stage pixels and existing embedded method editing."""
from dataclasses import replace
from io import BytesIO
import json
import struct
from types import SimpleNamespace
from unittest.mock import Mock

from PIL import Image
import pytest

from creation_lib.swf import native_runtime as native
from creation_lib.swf.project import build_document
from creation_lib.swf.writer import write_swf
from creation_lib.swf.types import MATRIX
from creation_lib.swf.svg_io import svg_to_shapes
from creation_lib.swf.tags import DefineShapeTag, PlaceObject2Tag
from ui.swf_editor.document import SwfEditingDocument
from ui.swf_editor.movie import Movie
from ui.swf_editor.stage import CompiledStage
from ui.tests.test_swf_editor_workflow import artwork, compiled
from ui.tests.test_swf_stage import placement, tag, timeline


def pixels(data,frame=1,owner=0):
    result=native.render_frame_image(data,frame,owner)
    image=Image.open(BytesIO(result['png'])).convert('RGBA')
    return image,result


def test_frame_moves_real_pixels_and_keeps_stage_canvas():
    data=artwork().build()
    model=CompiledStage(data)
    before,_=pixels(data)
    node=model.nodes()[0]
    changed=model.set_matrix(node.address,replace(node.matrix,translate_x=node.matrix.translate_x+400))
    after,result=pixels(changed)
    assert before.size==after.size==(550,400)
    assert not result['warnings']
    assert before.getbbox()!=after.getbbox()
    box=before.getbbox()
    assert after.getbbox()==(box[0]+20,box[1],box[2]+20,box[3])


def test_nested_sprite_advances_and_partial_modification_holds():
    base=Movie(artwork().build())
    shape=next(t for t in base.tags if t.character_id==1)
    nested=[placement(1,1,MATRIX(translate_x=1000,translate_y=1000)),tag(1),
            placement(1,matrix=MATRIX(translate_x=1400,translate_y=1000),move=True),tag(1),tag(0)]
    sprite=tag(39,struct.pack('<HH',20,2)+b''.join(t.encoded for t in nested))
    data=timeline([sprite,placement(2,20),tag(1),placement(2,name='renamed',move=True),tag(1)],2)
    movie=Movie(data)
    data=movie.assemble([shape]+movie.tags[1:])
    first,_=pixels(data,1)
    second,_=pixels(data,2)
    assert first.getbbox()[0]+20==second.getbbox()[0]
    assert CompiledStage(data).nodes(frame=2)[0].matrix==CompiledStage(data).nodes(frame=1)[0].matrix


def test_clip_mask_ignores_paint_alpha_and_ends_at_clip_depth():
    mask=svg_to_shapes('<svg xmlns="http://www.w3.org/2000/svg"><rect width="20" height="30" fill="#ffffff" fill-opacity="0.1"/></svg>')[0]
    mask.shape_id=1
    content=svg_to_shapes('<svg xmlns="http://www.w3.org/2000/svg"><rect width="40" height="30" fill="#ffffff"/></svg>')[0]
    content.shape_id=2
    def definition(shape):
        item=DefineShapeTag(shape,tag_id=32)
        return tag(item.tag_id,item.to_bytes())
    place=PlaceObject2Tag(depth=1,character_id=1,matrix=MATRIX(translate_x=1000,translate_y=1000),clip_depth=2)
    data=timeline([definition(mask),definition(content),tag(26,place.to_bytes()),
                   placement(2,2,MATRIX(translate_x=1000,translate_y=1000)),
                   placement(3,2,MATRIX(translate_x=2400,translate_y=1000)),tag(1)])
    image,result=pixels(data)
    assert not result['warnings']
    assert image.getpixel((60,60))[3]==255
    assert image.getpixel((80,60))[3]==0
    assert image.getpixel((150,60))[3]==255


def test_fill_rule_controls_same_direction_nested_contours():
    svg='<svg xmlns="http://www.w3.org/2000/svg"><path fill="#ffffff" d="M0 0L40 0L40 40L0 40Z M10 10L30 10L30 30L10 30Z"/></svg>'
    shape=svg_to_shapes(svg)[0]
    shape.shape_id=1
    shape.shape_version=4
    shape.edge_bounds=shape.bounds
    def image(flags):
        shape.shape_flags=flags
        item=DefineShapeTag(shape,tag_id=83)
        return pixels(timeline([tag(83,item.to_bytes()),placement(1,1,MATRIX(translate_x=1000,translate_y=1000)),tag(1)]))[0]
    assert image(0).getpixel((70,70))[3]==0
    assert image(4).getpixel((70,70))[3]==255


def test_method_recovery_recompile_preserves_original_class_and_tags():
    before=compiled()
    source=native.recover_as3_method_source(before,'B21_Fixture','value')
    assert source['body']=='return 7;'
    constructor=native.abc_disassemble(before,'B21_Fixture','constructor')
    after=native.replace_as3_method_source(before,'B21_Fixture','value','return 42;')
    assert native.abc_class_outline(before,'B21_Fixture')==native.abc_class_outline(after,'B21_Fixture')
    assert constructor==native.abc_disassemble(after,'B21_Fixture','constructor')
    assert '42' in json.dumps(native.abc_disassemble(after,'B21_Fixture','value'))
    a,b=Movie(before),Movie(after)
    assert a.header==b.header
    assert all(x.encoded==y.encoded for x,y in zip(a.tags,b.tags) if x.code not in {72,82})


def test_method_buffer_save_reopen_build_and_invalid_diagnostics(tmp_path):
    original=tmp_path/'original.swf'
    before=compiled()
    original.write_bytes(before)
    doc=SwfEditingDocument.open(original)
    recovered=native.recover_as3_method_source(before,'B21_Fixture','value')
    key='B21_Fixture::value'
    doc.sources[key]='return 42;'
    doc.manifest['native_method_sources']={key:{'class':'B21_Fixture','method':'value','baseline':recovered['body']}}
    doc.save(tmp_path/'edit.swfproj')
    reopened=SwfEditingDocument.open(tmp_path/'edit.swfproj')
    assert not reopened.dirty
    after=reopened.build()
    assert '42' in json.dumps(native.abc_disassemble(after,'B21_Fixture','value'))
    reopened.sources[key]='if ('
    reopened.save(tmp_path/'invalid.swfproj')
    invalid=SwfEditingDocument.open(tmp_path/'invalid.swfproj')
    with pytest.raises(ValueError):
        invalid.build()
    assert invalid.sources[key]=='if (' and original.read_bytes()==before


def test_getter_namespace_and_other_members_stay_exact():
    source='''package { public class B21_MethodFixture {
        private var _amount:Number = 7;
        public function B21_MethodFixture() {}
        public function get amount():Number { return this._amount; }
        public function set amount(value:Number):void { this._amount = value; }
        public function other():int { return 33; }
    } }'''
    data=write_swf(build_document({'canvas':[100,100],'scripts':['fixture.as'],
                                 'script_sources':{'fixture.as':source}},'.'))
    method=native.recover_as3_method_source(data,'B21_MethodFixture','get amount')
    assert 'this._amount' in method['body']
    after=native.replace_as3_method_source(data,'B21_MethodFixture','get amount','return this._amount * 2;')
    assert native.abc_class_outline(data,'B21_MethodFixture')==native.abc_class_outline(after,'B21_MethodFixture')
    for name in ('constructor','static initializer','set amount','other'):
        assert native.abc_disassemble(data,'B21_MethodFixture',name)==native.abc_disassemble(after,'B21_MethodFixture',name)
    with pytest.raises(ValueError,match='receiver'):
        native.replace_as3_method_source(data,'B21_MethodFixture','get amount','var other:Object = {}; return other._amount;')


def test_constructor_and_unsupported_exception_recovery_refused():
    with pytest.raises(ValueError,match='Constructor'):
        native.recover_as3_method_source(compiled(),'B21_Fixture','constructor')
    with pytest.raises(ValueError,match='Constructor'):
        native.replace_as3_method_source(compiled(),'B21_Fixture','constructor','return;')
    data=write_swf(build_document({'canvas':[100,100],'scripts':['x.as'],'script_sources':{'x.as':
        'package { public class B21_Exceptions { public function test():void { try { throw new Error(); } catch (e:Error) {} } } }'}},'.'))
    with pytest.raises(ValueError,match='Exception'):
        native.recover_as3_method_source(data,'B21_Exceptions','test')


def test_reopening_method_selects_edited_buffer_without_recovery():
    from ui.swf_editor.panels.document_panel import DocumentPanel
    panel=DocumentPanel.__new__(DocumentPanel)
    panel.class_name,panel.method='B21_Fixture','value'
    key='B21_Fixture::value'
    panel.app=SimpleNamespace(document=SimpleNamespace(sources={key:'return 42;'}),jobs=Mock())
    panel.recover_method()
    panel.app.jobs.start.assert_not_called()
    assert panel.app.document.sources[key]=='return 42;'
    assert panel.source_name==key and panel._select_source_tab and panel._focus_next
