package {
    import flash.display.MovieClip;
    import flash.events.MouseEvent;
    public class Panel extends MovieClip {
        public function B21BindMouse():void {
            for each (var control:Object in Controls) {
                control.addEventListener(MouseEvent.ROLL_OVER, B21MouseSelect);
                control.B21BindMouse();
            }
        }
        private function B21MouseSelect(event:MouseEvent):void {
            var index:int = Controls.indexOf(event.currentTarget);
            if (index >= 0 && Controls[index].active) selectControl(index);
        }
    }
}
