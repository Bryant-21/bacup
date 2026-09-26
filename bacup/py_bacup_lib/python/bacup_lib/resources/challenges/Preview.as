package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.external.ExternalInterface;

    public dynamic class B21TFA_Challenges_SeventySixMenu extends MovieClip {
        private var B21PreviewFrames:int = 0;
        private var B21PreviewActions:Array;

        public function B21PreviewInit():void {
            B21PreviewActions = [];
            addEventListener(Event.ENTER_FRAME, B21PreviewFrame);
        }

        private function B21PreviewEntry(id:String, title:String, progress:uint, total:uint, done:Boolean, tracked:Boolean):Object {
            return {id:id, name:title, progress:progress, count:total, completed:done, tracked:tracked,
                reward:{caps:100, xp:250, perkCardPacks:0}, sub:[]};
        }

        private function B21PreviewAction(action:String, id:String, tracked:Boolean):void {
            B21PreviewActions.push({action:action, id:id, tracked:tracked});
            ExternalInterface.call("report", JSON.stringify({actions:B21PreviewActions}));
        }

        private function B21PreviewFrame(event:Event):void {
            ++B21PreviewFrames;
            if (B21PreviewFrames == 8) {
                BGSCodeObj = {ChallengeAction:B21PreviewAction};
                var daily:Array = [B21PreviewEntry("000001", "Complete an Event", 1, 3, false, true),
                    B21PreviewEntry("000002", "Collect Caps", 650, 1000, false, false),
                    B21PreviewEntry("000003", "Cook a Meal", 5, 5, true, false)];
                var weekly:Array = [B21PreviewEntry("000004", "Explore Appalachia", 3, 7, false, false)];
                weekly[0].reward = {caps:300, xp:750, perkCardPacks:1};
                var parent:Object = B21PreviewEntry("000005", "Survive Appalachia", 1, 2, false, false);
                parent.sub = [B21PreviewEntry("000006", "Harvest Wild Plants", 4, 10, false, false),
                    B21PreviewEntry("000007", "Drink Clean Water", 5, 5, true, false)];
                B21SetData({version:1, available:true, daily:{resetAt:new Date().getTime() / 1000 + 7500, entries:daily},
                    weekly:{resetAt:new Date().getTime() / 1000 + 200000, entries:weekly}, lifetime:{categories:[
                        {key:"survival", label:"Survival", entries:[parent]},
                        {key:"combat", label:"Combat", entries:[B21PreviewEntry("000008", "Defeat Scorched", 12, 30, false, false)]}]},
                    tracked:["000001"]}, false);
            }
            if (B21PreviewFrames == 14) {
                var screen:Object = Challenges_mc;
                var results:Object = {categories:screen.CategoryList_mc.List_mc.entryList.length,
                    dailyRows:screen.ItemList_mc.entryList.length,
                    timer:screen.TimerText_tf.text,
                    offline:screen.ScoreWidgetManager_mc == null && !screen.ChallengeRerollText_mc.visible};
                Navigate("Track");
                results.trackAction = B21PreviewActions[0];
                Navigate("Completed");
                results.completedRows = screen.ItemList_mc.entryList.length;
                Navigate("Next");
                results.weeklyReward = screen.ItemList_mc.entryList[0].reward;
                results.weeklyTimer = screen.TimerText_tf.text;
                Navigate("Next");
                Navigate("Accept");
                results.children = screen.SubItemList_mc.entryList.length;
                var childID:String = screen.SubItemList_mc.selectedEntry.ID;
                B21SetData(Snapshot, false);
                results.childRefresh = screen.SubItemList_mc.visible && screen.SubItemList_mc.selectedEntry.ID == childID && stage.focus == screen.SubItemList_mc;
                Navigate("Cancel");
                results.closeAction = B21PreviewActions[B21PreviewActions.length - 1];
                Navigate("Previous");
                Navigate("Previous");
                var reset:Number = Snapshot.daily.resetAt;
                Snapshot.daily.resetAt = new Date().getTime() / 1000 - 1;
                B21SetData(Snapshot, false);
                results.expiredTimer = screen.TimerText_tf.text;
                Snapshot.daily.resetAt = reset;
                B21SetData(Snapshot, false);
                results.passed = results.categories == 4 && results.dailyRows == 2 && results.completedRows == 3 &&
                    results.trackAction.id == "000001" && !results.trackAction.tracked &&
                    results.closeAction.action == "close" && results.children == 2 && results.childRefresh &&
                    results.offline && results.weeklyReward.caps == 300 && results.weeklyReward.xp == 750 &&
                    results.weeklyReward.perkCardPacks == 1 && results.expiredTimer == "Refreshing...";
                ExternalInterface.call("report", JSON.stringify(results));
                removeEventListener(Event.ENTER_FRAME, B21PreviewFrame);
            }
        }
    }
}
