package {
    import flash.display.MovieClip;
    import flash.events.Event;

    public dynamic class HUDReputationUpdatesWidget extends MovieClip {
        public var ReputationMeter_mc:HUDReputationUpdateMeter;
        public var LevelUpAnimation_mc:MovieClip;
        public var BGSCodeObj:Object;
        public var B21Done:uint = 0;
        private var sequence:uint = 0;
        private var active:Boolean = false;

        public function HUDReputationUpdatesWidget() {
            super();
            mouseEnabled = false;
            mouseChildren = false;
            visible = false;
            addEventListener(HUDReputationUpdateMeter.DISPLAY_COMPLETE, onMeterDisplayed);
            addEventListener(HUDReputationUpdateMeter.FADEOUT_COMPLETE, onFinished);
        }

        public function B21SetReputation(data:Object):void {
            if (!data.visible) {
                visible = false;
                return;
            }
            visible = true;
            if (active && sequence == uint(data.sequence)) return;
            sequence = uint(data.sequence);
            active = true;
            ReputationMeter_mc.gotoAndStop(1);
            LevelUpAnimation_mc.gotoAndStop(1);
            if (data.levelUp) {
                LevelUpAnimation_mc.RepLevelUpBoy_mc.gotoAndStop(String(data.factionCode));
                LevelUpAnimation_mc.gotoAndPlay("rollOn");
            } else {
                if (data.tierStart == data.tierEnd && data.tierEnd == Shared.AS3.Factions.THRESHOLD_TIER_ALLY) {
                    // The source's completed-tier branch shows two Ally faces instead of requesting tier 7.
                    var maximum:Object = {};
                    for (var key:String in data) maximum[key] = data[key];
                    maximum.tierStart = data.tierEnd - 1;
                    data = maximum;
                }
                ReputationMeter_mc.fadeIn();
                ReputationMeter_mc.data = data;
            }
        }

        private function onMeterDisplayed(event:Event):void {
            if (active) ReputationMeter_mc.fadeOut();
        }

        private function onFinished(event:Event):void {
            if (!active) return;
            active = false;
            visible = false;
            B21Done = sequence;
            if (BGSCodeObj != null && BGSCodeObj.ReputationDone != null)
                BGSCodeObj.ReputationDone(sequence);
        }
    }
}
