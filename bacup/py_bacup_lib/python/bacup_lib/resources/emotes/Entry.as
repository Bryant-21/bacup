package {
    import flash.display.MovieClip;
    import flash.geom.Rectangle;
    import flash.system.ApplicationDomain;
    public dynamic class CLASS_NAME extends MovieClip {
        public var Icon_mc:MovieClip;
        public var Hotkey_mc:MovieClip;
        public var EquippedState_mc:MovieClip;
        public var EquippedStateTop_mc:MovieClip;
        public var Backer_mc:MovieClip;
        public var Fill_mc:MovieClip;
        public var HitArea_mc:MovieClip;
        public var Expanded_mc:MovieClip;
        public var BackgroundHighlight_mc:MovieClip;
        public var slot:int;
        public var occupied:Boolean;
        public var icon:String = "";
        private var artwork:MovieClip;
        private var iconBounds:Rectangle;
        private var selectedValue:Boolean;
        public function CLASS_NAME() {
            super();
            addFrameScript(1, stop, 2, stop, 10, stop, 14, stop, 20, stop, 21, stop);
            gotoAndStop(IDLE_FRAME);
            mouseChildren = false;
        }
        public function Configure(index:int):void {
            slot = index;
            if (HitArea_mc != null) {
                hitArea = HitArea_mc;
                HitArea_mc.visible = false;
                HitArea_mc.mouseEnabled = false;
            }
            if (Hotkey_mc != null) Hotkey_mc.visible = false;
            if (EquippedState_mc != null) EquippedState_mc.visible = false;
            if (EquippedStateTop_mc != null) EquippedStateTop_mc.visible = false;
            if (Backer_mc != null) Backer_mc.gotoAndStop(occupied ? 1 : 2);
            UpdateRotation();
            iconBounds = Icon_mc.Body.getBounds(Icon_mc.Body);
        }
        // FO76 RadialMenuEntry.updateRotation: keep the icon upright whatever the sector and ring turn.
        public function UpdateRotation():void {
            Icon_mc.rotation = -rotation - parent.rotation;
        }
        public function SetBackerAlpha(value:Number):void {
            if (Backer_mc != null) Backer_mc.alpha = value;
        }
        // Empty sectors keep their dimmed source backer (frame 2) rather than disappearing.
        public function SetItem(value:String, available:Boolean):void {
            occupied = value.length > 0 && available;
            visible = true;
            mouseEnabled = true;
            if (Backer_mc != null) Backer_mc.gotoAndStop(occupied ? 1 : 2);
            if (value == icon && artwork != null) {
                artwork.alpha = occupied ? 1 : 0.35;
                return;
            }
            icon = value;
            if (artwork != null && artwork.parent != null) artwork.parent.removeChild(artwork);
            artwork = null;
            if (value.length == 0) return;
            var type:Class = ApplicationDomain.currentDomain.getDefinition("B21Emotes_" + value) as Class;
            artwork = new type() as MovieClip;
            var bounds:Rectangle = artwork.getBounds(artwork);
            var scale:Number = Math.min(iconBounds.width / Math.max(1, bounds.width), iconBounds.height / Math.max(1, bounds.height));
            artwork.scaleX = artwork.scaleY = scale;
            artwork.x = -(bounds.x + bounds.width / 2) * scale;
            artwork.y = -(bounds.y + bounds.height / 2) * scale;
            artwork.alpha = occupied ? 1 : 0.35;
            Icon_mc.Body.addChild(artwork);
        }
        public function SetSelected(value:Boolean):void {
            if (selectedValue == value) return;
            selectedValue = value;
            gotoAndStop(value ? SELECTED_FRAME : IDLE_FRAME);
            Configure(slot);
            Icon_mc.gotoAndPlay(value ? "selected" : "unselected");
        }
        public function get selected():Boolean { return selectedValue; }
    }
}
