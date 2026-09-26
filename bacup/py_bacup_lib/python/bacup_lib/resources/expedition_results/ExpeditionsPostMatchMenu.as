package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;
    import Shared.AS3.BSButtonHintBar;
    import Shared.AS3.BSButtonHintData;
    import Shared.AS3.IMenu;

    public class ExpeditionsPostMatchMenu extends IMenu {
        public var Internal_mc:MovieClip;
        public var ButtonHintBar_mc:BSButtonHintBar;
        public var ExScreenshotContainer_mc:MovieClip;
        public var MainHeader_mc:MovieClip;
        public var ExpeditionName_mc:MovieClip;
        public var ObjectiveCompleteHeader_mc:MovieClip;
        public var RewardsListHeader_mc:MovieClip;
        public var PrimaryCheckbox_mc:MovieClip;
        public var ObjectivesContainer_mc:MovieClip;
        public var RewardsContainer_mc:MovieClip;
        public var BGSCodeObj:Object;
        public var NativeInput:Boolean = false;
        public var ExpeditionResultsVersion:uint = 1;

        private var objectives:Array = [];
        private var rewards:Array = [];
        private var ready:Boolean = false;
        private var sourceStrings:Object = __EXPEDITION_TRANSLATIONS__;

        public function ExpeditionsPostMatchMenu() {
            super();
            addEventListener(Event.ADDED_TO_STAGE, added);
        }

        private function added(event:Event):void {
            if (ready) return;
            ready = true;
            gotoAndStop(30);
            collectRows();
            if (ButtonHintBar_mc != null) {
                ButtonHintBar_mc.SetButtonHintData(new <BSButtonHintData>[
                    new BSButtonHintData("$EXIT", "TAB", "PSN_B", "Xenon_B", 1, close)
                ]);
                ButtonHintBar_mc.addEventListener(MouseEvent.CLICK, mouseClose);
            }
            if (stage != null) stage.addEventListener(KeyboardEvent.KEY_DOWN, keyDown, true);
        }

        private function collectRows():void {
            var index:int;
            if (ObjectivesContainer_mc != null) {
                for (index = 0; index < ObjectivesContainer_mc.numChildren; ++index) {
                    objectives.push(ObjectivesContainer_mc.getChildAt(index));
                }
            }
            if (RewardsContainer_mc != null) {
                for (index = 0; index < RewardsContainer_mc.numChildren; ++index) {
                    rewards.push(RewardsContainer_mc.getChildAt(index));
                }
            }
        }

        public function B21SetData(data:Object):Boolean {
            if (!ready || data == null || data.objectives == null || data.rewards == null) return false;
            setText(ExpeditionName_mc, sourceText(String(data.expeditionName)).toUpperCase());
            setText(MainHeader_mc, sourceText("$Expedition"));
            setText(RewardsListHeader_mc, sourceText("$REWARDS"));
            if (PrimaryCheckbox_mc != null) PrimaryCheckbox_mc.gotoAndStop("pass");
            populateObjectives(data.objectives);
            populateRewards(data.rewards);
            return true;
        }

        private function setText(clip:MovieClip, value:String):void {
            if (clip != null && clip["textField_tf"] != null) clip["textField_tf"].text = value;
            else if (clip != null && clip["MainHeader_tf"] != null) clip["MainHeader_tf"].text = value;
        }

        private function sourceText(value:String):String {
            if (sourceStrings != null && sourceStrings.hasOwnProperty(value)) return String(sourceStrings[value]);
            return value;
        }

        private function populateObjectives(values:Array):void {
            var index:int;
            for (index = 0; index < objectives.length; ++index) {
                var row:Object = objectives[index];
                row.visible = index < values.length;
                if (row.visible && row.hasOwnProperty("SetEntryText")) {
                    row.SetEntryText({objective: sourceText(String(values[index].name)), isComplete: Boolean(values[index].complete)}, "");
                }
            }
        }

        private function populateRewards(values:Array):void {
            var index:int;
            for (index = 0; index < rewards.length; ++index) {
                var row:Object = rewards[index];
                row.visible = index < values.length;
                if (row.visible && row.hasOwnProperty("SetEntryText")) {
                    row.SetEntryText({rewardName: sourceText(String(values[index].name)), count: uint(values[index].count),
                        type: uint(values[index].type), isSpecial: Boolean(values[index].special)}, "");
                }
            }
        }

        public function Navigate(action:String):void {
            if (action == "Accept" || action == "Cancel" || action == "Start" || action == "ForceClose") close();
        }

        public function ProcessUserEvent(action:String, pressed:Boolean):Boolean {
            if (!pressed && (action == "Accept" || action == "Cancel" || action == "Start" || action == "ForceClose")) {
                Navigate(action);
                return true;
            }
            return false;
        }

        private function keyDown(event:KeyboardEvent):void {
            if (NativeInput) return;
            if (event.keyCode == 13 || event.keyCode == 32 || event.keyCode == 27 || event.keyCode == 9) {
                event.stopImmediatePropagation();
                close();
            }
        }

        private function mouseClose(event:MouseEvent):void { close(); }

        private function close():void {
            if (BGSCodeObj != null && BGSCodeObj.ExpeditionResultsAction is Function) {
                BGSCodeObj.ExpeditionResultsAction("close");
            }
        }
    }
}
