package {
    import Shared.AS3.BSButtonHintBar;
    import Shared.AS3.BSButtonHintData;
    import Shared.AS3.IMenu;
    import flash.display.MovieClip;

    public class DailyOpsModalManager extends IMenu {
        public var BGSCodeObj:Object;
        public var MissionRanking_mc:DOMissionRankingModal;
        public var MissionRewards_mc:DOMissionRewardsModal;
        public var MissionTally_mc:MovieClip;
        public var ButtonHintBar_mc:BSButtonHintBar;
        private var rewardsButton:BSButtonHintData;
        private var closeButton:BSButtonHintData;
        private var report:Object;

        public function DailyOpsModalManager() {
            super();
            addFrameScript(0, stopFrame, 1, stopFrame, 2, stopFrame);
            rewardsButton = new BSButtonHintData("$VIEW_REWARDS", "ENTER", "PSN_A", "Xenon_A", 1, showRewards);
            closeButton = new BSButtonHintData("$EXIT", "ESC", "PSN_B", "Xenon_B", 1, closeReport);
            ButtonHintBar_mc.SetButtonHintData(new <BSButtonHintData>[rewardsButton, closeButton]);
        }

        override public function onAddedToStage():void {
            gotoAndStop("MissionRanking");
            MissionRanking_mc.show();
        }

        public function B21SetData(data:Object):void {
            report = data;
            gotoAndStop("MissionRanking");
            MissionRanking_mc.populateRankData(data.rankDataA, data.completionTime);
            MissionRanking_mc.populateDOData(data.location, data.gameMode, data.enemy, data.mutator);
            MissionRanking_mc.show();
            rewardsButton.ButtonVisible = true;
        }

        public function ProcessUserEvent(name:String, pressed:Boolean):Boolean {
            if (pressed) return false;
            if (name == "Accept" || name == "Activate") {
                showRewards();
                return true;
            }
            if (name == "Cancel" || name == "Start" || name == "ForceClose") {
                closeReport();
                return true;
            }
            return false;
        }

        private function showRewards():void {
            if (report == null || !rewardsButton.ButtonVisible) return;
            gotoAndStop("MissionRewards");
            MissionRewards_mc.setRewardsData(report.rankDataA, report.rewardData, report.mutationMode);
            MissionRewards_mc.setTooltip(report.mutationMode);
            MissionRewards_mc.show();
            rewardsButton.ButtonVisible = false;
        }

        private function closeReport():void {
            if (BGSCodeObj != null) BGSCodeObj.DailyOpsEvent("close");
        }

        private function stopFrame():void { stop(); }
    }
}
