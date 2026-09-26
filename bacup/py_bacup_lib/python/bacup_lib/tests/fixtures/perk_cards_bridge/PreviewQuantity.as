package {
    import flash.display.MovieClip;
    public class PreviewQuantity extends MovieClip {
        public var opened:Boolean = false;
        public var quantity:int = 202;
        public var confirmed:uint = 0;
        public var canceled:uint = 0;
        public function onConfirm():void { ++confirmed; opened = false; }
        public function onCancel():void { ++canceled; opened = false; }
        public function modifyQuantity(step:int):void { quantity = Math.max(1,Math.min(202,quantity+step)); }
        public function ProcessUserEvent(action:String, pressed:Boolean):Boolean { return false; }
    }
}
