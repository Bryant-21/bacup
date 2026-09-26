package {
    import flash.display.MovieClip;

    public dynamic class B21_QuestAreaCompass extends MovieClip {
        // FO76's within-area bracket spans x -24.9..479.4 with its bar bottom at y 55.05;
        // FO4's CompassBar_mc spans x -152..152 with its bottom at y 0.
        private static const SCALE:Number = 304 / 504.3;

        public var AreaQuest_WithinClip_mc:B21_QuestAreaWithinClip;
        public var WithinClipVisibility:Boolean = false;

        public function B21_QuestAreaCompass() {
            super();
            mouseEnabled = false;
            mouseChildren = false;
            AreaQuest_WithinClip_mc = new B21_QuestAreaWithinClip();
            AreaQuest_WithinClip_mc.scaleX = AreaQuest_WithinClip_mc.scaleY = SCALE;
            AreaQuest_WithinClip_mc.x = -227.25 * SCALE;
            AreaQuest_WithinClip_mc.y = -55.05 * SCALE;
            addChild(AreaQuest_WithinClip_mc);
            visible = false;
        }

        // FO76 HUDCompassWidget.onDataChanged, driven by the native withinAreaMarker state.
        public function B21SetWithinArea(within:Boolean):void {
            if (within) {
                if (!WithinClipVisibility) AreaQuest_WithinClip_mc.gotoAndPlay("rollOn");
                WithinClipVisibility = true;
            } else {
                if (WithinClipVisibility) AreaQuest_WithinClip_mc.gotoAndPlay("rollOut");
                WithinClipVisibility = false;
            }
            // FO76's resting "off" frame repeats its own compass bar, which FO4 already draws.
            visible = WithinClipVisibility || AreaQuest_WithinClip_mc.currentFrame > 1;
        }
    }
}
