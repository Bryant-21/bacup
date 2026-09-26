package {
    import flash.display.MovieClip;
    import flash.events.MouseEvent;
    public class OptionStepperWithLabel extends MovieClip {
        public function B21BindMouse():void {
            var bounds:Object = Selector_mc.getBounds(this);
            graphics.beginFill(0, 0);
            graphics.drawRect(bounds.x, bounds.y, bounds.width, bounds.height);
            graphics.endFill();
            Selector_mc.mouseEnabled = false;
            mouseChildren = false;
            addEventListener(MouseEvent.CLICK, B21MouseClick);
        }
        private function B21MouseClick(event:MouseEvent):void {
            if (!active) return;
            if (Stepper_mc.LeftArrow_mc.hitTestPoint(event.stageX, event.stageY)) onLeft();
            else if (Stepper_mc.RightArrow_mc.hitTestPoint(event.stageX, event.stageY)) onRight();
        }
    }
}
