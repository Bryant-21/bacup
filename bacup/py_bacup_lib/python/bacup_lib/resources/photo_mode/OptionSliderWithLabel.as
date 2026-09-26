package {
    import flash.display.MovieClip;
    import flash.display.Stage;
    import flash.events.Event;
    import flash.events.MouseEvent;
    public class OptionSliderWithLabel extends MovieClip {
        private var b21DragStage:Stage;
        public function B21BindMouse():void {
            var bounds:Object = Selector_mc.getBounds(this);
            graphics.beginFill(0, 0);
            graphics.drawRect(bounds.x, bounds.y, bounds.width, bounds.height);
            graphics.endFill();
            Selector_mc.mouseEnabled = false;
            mouseChildren = false;
            addEventListener(MouseEvent.MOUSE_DOWN, B21MouseDown);
            addEventListener(Event.REMOVED_FROM_STAGE, B21MouseEnd);
        }
        private function B21MouseDown(event:MouseEvent):void {
            if (!active || !stage) return;
            var range:Object = Slider_mc.Slider_Selected_mc.Slider_Range_Selected_mc.getBounds(this);
            if (mouseX < range.x - 8 || mouseX > range.right + 8) return;
            b21DragStage = stage;
            b21DragStage.addEventListener(MouseEvent.MOUSE_MOVE, B21MouseDrag);
            b21DragStage.addEventListener(MouseEvent.MOUSE_UP, B21MouseEnd);
            B21MouseDrag(event);
        }
        private function B21MouseDrag(event:MouseEvent):void {
            var range:Object = Slider_mc.Slider_Selected_mc.Slider_Range_Selected_mc.getBounds(this);
            var knob:Object = Slider_mc.Slider_Selected_mc.Slider_Bar_Selected_mc.getBounds(this);
            var fraction:Number = Math.max(0, Math.min(1, (mouseX - range.x - knob.width / 2) / (range.width - knob.width)));
            value = Min + fraction * (Max - Min);
        }
        private function B21MouseEnd(event:Event):void {
            if (!b21DragStage) return;
            b21DragStage.removeEventListener(MouseEvent.MOUSE_MOVE, B21MouseDrag);
            b21DragStage.removeEventListener(MouseEvent.MOUSE_UP, B21MouseEnd);
            b21DragStage = null;
        }
    }
}
