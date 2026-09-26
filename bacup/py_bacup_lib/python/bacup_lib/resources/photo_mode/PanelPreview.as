package {
    import flash.display.MovieClip;
    import flash.display.DisplayObject;
    public class Panel extends MovieClip {
        public function B21PreviewControls(root:DisplayObject):Object {
            var controls:Array = [];
            for each (var control:Object in Controls) {
                var bounds:Object = control.getBounds(root);
                controls.push({name:control.B21ControlName(),active:control.active,
                    x:bounds.x,y:bounds.y,width:bounds.width,height:bounds.height,hit:control.B21PreviewBounds(root)});
            }
            return {selected:SelectedControl,controls:controls};
        }
    }
}
