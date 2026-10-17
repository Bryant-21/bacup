package {
    import flash.display.MovieClip;
    import flash.text.TextField;
    import Shared.AS3.BSButtonHintBar;
    import Shared.AS3.BSButtonHintData;
    import Shared.AS3.IMenu;

    public class DeathRespawnMenu extends IMenu {
        public var RespawnSecondsField:TextField;
        public var ButtonHintBar_mc:BSButtonHintBar;
        public var ModalSetBounty_mc:MovieClip;
        public var Leaderboard_mc:MovieClip;
        public var BGSCodeObj:Object;

        private var respawnButton:BSButtonHintData;
        private var seconds:int = 0;

        public function DeathRespawnMenu() {
            super();
            addFrameScript(0, holdFrame, 1, holdFrame);
            respawnButton = new BSButtonHintData("$RESPAWN", "Space", "PSN_A", "Xenon_A", 1, onRespawnAccept);
        }

        private function holdFrame():void {
            stop();
        }

        override public function onAddedToStage():void {
            if (ModalSetBounty_mc != null) ModalSetBounty_mc.visible = false;
            if (Leaderboard_mc != null) Leaderboard_mc.visible = false;
            if (ButtonHintBar_mc != null) ButtonHintBar_mc.SetButtonHintData(new <BSButtonHintData>[respawnButton]);
            B21SetDeathData(seconds);
        }

        public function B21SetDeathData(value:int):void {
            seconds = value < 0 ? 0 : value;
            if (seconds > 0) {
                gotoAndStop("Countdown");
                if (RespawnSecondsField != null) {
                    RespawnSecondsField.text = seconds < 10 ? "0" + seconds : String(seconds);
                    RespawnSecondsField.visible = true;
                }
            } else {
                gotoAndStop("List");
                if (RespawnSecondsField != null) RespawnSecondsField.visible = false;
            }
            respawnButton.ButtonVisible = seconds == 0;
        }

        private function onRespawnAccept():void {
            if (seconds != 0) return;
            if (BGSCodeObj != null && BGSCodeObj.RespawnAccept is Function) BGSCodeObj.RespawnAccept();
        }
    }
}
