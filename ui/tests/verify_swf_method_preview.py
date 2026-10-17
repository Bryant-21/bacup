"""Execute edited embedded methods in disposable Ruffle previews."""
import argparse
import json
from pathlib import Path
import time

from creation_lib.swf import native_runtime as native
from creation_lib.swf.project import build_document
from creation_lib.swf.writer import write_swf
from ui.swf_editor.swf_editor_app import SwfEditorApp


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--output',type=Path,required=True)
    output=parser.parse_args().output.resolve()
    output.relative_to(Path.cwd().resolve())
    output.mkdir(parents=True,exist_ok=True)
    source='''package { import flash.display.MovieClip;
        public class B21_MethodRuntime extends MovieClip {
            private var _amount:Number = 7;
            public function B21_MethodRuntime() { trace("B21_METHOD_" + value() + "_PRIVATE_" + amount); }
            public function value():int { return 7; }
            public function get amount():Number { return this._amount; }
        } }'''
    before=write_swf(build_document({'canvas':[100,100],'scripts':['fixture.as'],
                                  'script_sources':{'fixture.as':source},
                                  'exports':[{'character':0,'class':'B21_MethodRuntime'}]},'.'))
    after=native.replace_as3_method_source(before,'B21_MethodRuntime','value','return 42;')
    after=native.replace_as3_method_source(after,'B21_MethodRuntime','get amount','return this._amount * 2;')
    app=SwfEditorApp()
    evidence=[]
    try:
        for iteration,(data,marker) in enumerate(((before,'B21_METHOD_7_PRIVATE_7'),
                                                (after,'B21_METHOD_42_PRIVATE_14'),
                                                (after,'B21_METHOD_42_PRIVATE_14')),1):
            app.launch_preview(data)
            directory=Path(app._preview_directory.name)
            deadline=time.monotonic()+8
            while time.monotonic()<deadline and marker not in app.preview_output():
                app.poll()
                if app._preview_process.poll() is not None:
                    break
                time.sleep(.05)
            log=app.preview_output()
            evidence.append({'iteration':iteration,'expected':marker,'executed':marker in log,'log':log})
            assert app._preview_process.poll() is None and marker in log,log
            app.close_preview()
            app.close_preview()
            assert not directory.exists()
        print('Original and edited instance/getter methods executed; repeated previews and close cleanup passed')
    finally:
        app.cleanup()
        (output/'method-preview.json').write_text(json.dumps(evidence,indent=2),encoding='utf-8')


if __name__=='__main__':
    main()
