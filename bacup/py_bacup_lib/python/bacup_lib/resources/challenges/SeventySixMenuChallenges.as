package {
    import flash.events.Event;
    import flash.display.MovieClip;
    import flash.text.TextField;
    import Shared.AS3.Events.MenuActionEvent;
    import Shared.AS3.BSScrollingList;

    public class SeventySixMenuChallenges extends MovieClip {
        public var CategoryList_mc:MenuListComponent;
        public var ItemList_mc:BSScrollingList;
        public var SubItemList_mc:BSScrollingList;
        public var TimerText_tf:TextField;
        public var DisabledText_tf:TextField;
        public var ChallengeRerollText_mc:MovieClip;
        public var Tooltip_mc:MovieClip;
        public var ScoreWidgetManager_mc:ScoreWidgetManager;
        private var B21Categories:Array;
        private var B21Updating:Boolean = false;

        public function SeventySixMenuChallenges() {
            super();
            CategoryList_mc.itemRendererClassName_Inspectable = "B21TFA_Challenges_StoreMenuListEntry";
            CategoryList_mc.numItems_Inspectable = 14;
            CategoryList_mc.useBackground = false;
            CategoryList_mc.verticalSpacing_Inspectable = 5;
            CategoryList_mc.x = 0;
            CategoryList_mc.y = 0;
            CategoryList_mc.visible = true;
            ItemList_mc.x = 430;
            ItemList_mc.y = 0;
            ItemList_mc.visible = true;
            SubItemList_mc.x = 1160;
            SubItemList_mc.y = 0;
            TimerText_tf.x = 430;
            TimerText_tf.y = -64;
            DisabledText_tf.x = 430;
            DisabledText_tf.y = 40;
            ItemList_mc.listEntryClass_Inspectable = "B21TFA_Challenges_ChallengeListEntry";
            ItemList_mc.textOption_Inspectable = BSScrollingList.TEXT_OPTION_MULTILINE;
            ItemList_mc.numListItems_Inspectable = 10;
            SubItemList_mc.listEntryClass_Inspectable = "B21TFA_Challenges_ChallengeListEntry";
            SubItemList_mc.textOption_Inspectable = BSScrollingList.TEXT_OPTION_MULTILINE;
            SubItemList_mc.numListItems_Inspectable = 5;
            SubItemList_mc.visible = false;
            CategoryList_mc.addEventListener(MenuActionEvent.MENU_ACCEPT, CategoryChanged);
            CategoryList_mc.addEventListener(MenuActionEvent.MENU_HOVER, CategoryChanged);
            ItemList_mc.addEventListener(BSScrollingList.SELECTION_CHANGE, ChallengeHovered);
            ItemList_mc.addEventListener(BSScrollingList.ITEM_PRESS, ChallengePressed);
            SubItemList_mc.addEventListener(BSScrollingList.ITEM_PRESS, SubChallengePressed);
            updateButtonsAndTooltips();
        }

        private function CategoryChanged(event:Event):void { B21SelectCategory(); }
        private function ChallengeHovered(event:Event):void { onChallengeHover(); }
        private function ChallengePressed(event:Event):void { stage.focus = ItemList_mc; B21Navigate("Accept"); }
        private function SubChallengePressed(event:Event):void { stage.focus = SubItemList_mc; toggleTrackSelectedChallenge(); }

        public function onChallengeHover():void {
            var entry:Object = ItemList_mc.selectedEntry;
            SubItemList_mc.visible = entry != null && entry.subChallenges.length > 0;
            if (SubItemList_mc.visible) {
                SubItemList_mc.entryList = entry.subChallenges;
                SubItemList_mc.InvalidateData();
            }
        }

        public function B21SetSnapshot(snapshot:Object, completed:Boolean, gamepad:Boolean):void {
            var oldCategory:int = Math.max(0, CategoryList_mc.selectedIndex);
            var oldCategoryEntry:Object = CategoryList_mc.List_mc.selectedEntry;
            var oldKey:String = oldCategoryEntry == null ? "" : String(oldCategoryEntry.key);
            var oldEntry:Object = ItemList_mc.selectedEntry;
            var oldID:String = oldEntry == null ? "" : String(oldEntry.ID);
            var oldChild:Object = SubItemList_mc.selectedEntry;
            var oldChildID:String = oldChild == null ? "" : String(oldChild.ID);
            var subFocused:Boolean = stage.focus == SubItemList_mc;
            var itemScroll:uint = ItemList_mc.scrollPosition;
            var childScroll:uint = SubItemList_mc.scrollPosition;
            B21Categories = [];
            if (snapshot.daily != null) B21AddCategory("daily", "DAILY", snapshot.daily.entries, snapshot.daily.resetAt, completed);
            if (snapshot.weekly != null) B21AddCategory("weekly", "WEEKLY", snapshot.weekly.entries, snapshot.weekly.resetAt, completed);
            if (snapshot.lifetime != null) {
                for each (var category:Object in snapshot.lifetime.categories) {
                    B21AddCategory(category.key, "LIFETIME: " + category.label.toUpperCase(), category.entries, 0, completed);
                }
            }
            B21Updating = true;
            CategoryList_mc.List_mc.entryList = B21Categories;
            CategoryList_mc.List_mc.InvalidateData();
            for (var c:int = 0; c < B21Categories.length; ++c) {
                if (B21Categories[c].key == oldKey) oldCategory = c;
            }
            CategoryList_mc.setSelectedIndex(Math.min(oldCategory, B21Categories.length - 1));
            B21Updating = false;
            B21SelectCategory();
            for (var i:int = 0; i < ItemList_mc.entryList.length; ++i) {
                if (ItemList_mc.entryList[i].ID == oldID) ItemList_mc.selectedIndex = i;
            }
            ItemList_mc.scrollPosition = Math.min(itemScroll, ItemList_mc.maxScrollPosition);
            onChallengeHover();
            if (SubItemList_mc.visible) {
                for (var s:int = 0; s < SubItemList_mc.entryList.length; ++s) {
                    if (SubItemList_mc.entryList[s].ID == oldChildID) SubItemList_mc.selectedIndex = s;
                }
                SubItemList_mc.scrollPosition = Math.min(childScroll, SubItemList_mc.maxScrollPosition);
                if (subFocused) stage.focus = SubItemList_mc;
            } else if (subFocused) stage.focus = ItemList_mc;
            ItemList_mc.SetPlatform(gamepad ? 1 : 0, false, 0, 0);
            SubItemList_mc.SetPlatform(gamepad ? 1 : 0, false, 0, 0);
            CategoryList_mc.List_mc.SetPlatform(gamepad ? 1 : 0, false, 0, 0);
            updateButtonsAndTooltips();
        }

        private function B21Rows(entries:Array, completed:Boolean):Array {
            var rows:Array = [];
            for each (var entry:Object in entries) {
                if (!completed && entry.completed) continue;
                rows.push({ID: entry.id, text: entry.name, currentValue: entry.progress,
                    thresholdValue: entry.count, completed: entry.completed, isTracked: entry.tracked,
                    reward: entry.reward, subChallenges: B21Rows(entry.sub, true), isEnabled: true});
            }
            rows.sort(B21Compare);
            return rows;
        }

        private function B21Compare(a:Object, b:Object):int {
            if (a.isTracked != b.isTracked) return a.isTracked ? -1 : 1;
            if (a.completed != b.completed) return a.completed ? 1 : -1;
            return String(a.text).localeCompare(String(b.text));
        }

        private function B21AddCategory(key:String, text:String, entries:Array, resetAt:Number, completed:Boolean):void {
            if (resetAt > 0) {
                var total:uint = 0;
                for each (var entry:Object in entries) if (entry.completed) ++total;
                text += " (" + total + "/" + entries.length + ")";
            }
            B21Categories.push({key: key, text: text, challenges: B21Rows(entries, completed),
                resetAt: resetAt, isDisplayed: true, isEnabled: true});
        }

        private function B21SelectCategory():void {
            if (B21Updating) return;
            var category:Object = CategoryList_mc.List_mc.selectedEntry;
            ItemList_mc.entryList = category == null ? [] : category.challenges;
            ItemList_mc.InvalidateData();
            ItemList_mc.selectedIndex = ItemList_mc.entryList.length > 0 ? 0 : -1;
            SubItemList_mc.visible = false;
            UpdateTimerText();
            DisabledText_tf.visible = ItemList_mc.entryList.length == 0;
            DisabledText_tf.text = "No challenges in this category";
        }

        public function B21Navigate(action:String):void {
            var list:BSScrollingList = stage.focus == SubItemList_mc ? SubItemList_mc : ItemList_mc;
            if (action == "Next" || action == "Previous") {
                var index:int = CategoryList_mc.selectedIndex + (action == "Next" ? 1 : -1);
                if (B21Categories.length > 0) {
                    CategoryList_mc.setSelectedIndex((index + B21Categories.length) % B21Categories.length);
                    B21SelectCategory();
                }
                stage.focus = ItemList_mc;
            } else if (action == "Up" || action == "Down") {
                if (stage.focus == CategoryList_mc || stage.focus == CategoryList_mc.List_mc) {
                    B21Navigate(action == "Up" ? "Previous" : "Next");
                } else {
                    list.selectedIndex = Math.max(0, Math.min(list.entryList.length - 1,
                        list.selectedIndex + (action == "Up" ? -1 : 1)));
                    if (list == ItemList_mc) onChallengeHover();
                }
            } else if (action == "Left") {
                stage.focus = list == SubItemList_mc ? ItemList_mc : CategoryList_mc.List_mc;
            } else if (action == "Right" || action == "Accept") {
                if (stage.focus == CategoryList_mc || stage.focus == CategoryList_mc.List_mc) {
                    stage.focus = ItemList_mc;
                } else if (list == ItemList_mc && list.selectedEntry != null && list.selectedEntry.subChallenges.length > 0) {
                    onChallengeHover();
                    stage.focus = SubItemList_mc;
                    SubItemList_mc.selectedIndex = 0;
                } else toggleTrackSelectedChallenge();
            } else if (action == "Track") toggleTrackSelectedChallenge();
            ItemList_mc.alpha = 1;
            SubItemList_mc.alpha = 1;
            CategoryList_mc.alpha = 1;
        }

        public function toggleTrackSelectedChallenge():void {
            var entry:Object = stage.focus == SubItemList_mc ? SubItemList_mc.selectedEntry : ItemList_mc.selectedEntry;
            if (entry != null && !entry.completed) Object(root).Track(String(entry.ID), !entry.isTracked);
        }

        public function updateButtonsAndTooltips():void {
            ChallengeRerollText_mc.visible = false;
            if (ScoreWidgetManager_mc != null) ScoreWidgetManager_mc.visible = false;
            Tooltip_mc.visible = false;
        }

        public function UpdateTimerText():* {
            var category:Object = CategoryList_mc.List_mc.selectedEntry;
            TimerText_tf.visible = category != null && category.resetAt > 0;
            if (TimerText_tf.visible) {
                var seconds:Number = Math.max(0, category.resetAt - new Date().getTime() / 1000);
                if (seconds <= 0) TimerText_tf.text = "Refreshing...";
                else if (category.key == "weekly") TimerText_tf.text = "ENDING IN: " + Math.floor(seconds / 86400) + "d " + Math.floor(seconds % 86400 / 3600) + "h (UTC)";
                else TimerText_tf.text = "ENDING IN: " + Math.floor(seconds / 3600) + "h " + Math.floor(seconds % 3600 / 60) + "m (UTC)";
            }
        }

        public function onCategoryChanged():* { B21SelectCategory(); }
        public function onCategoryAccept():* { B21SelectCategory(); stage.focus = ItemList_mc; }
        public function onCategoryHover():* { B21SelectCategory(); }
        public function onCancel():Boolean { Object(root).Navigate("Cancel"); return true; }
        public function onGotoSeason():void {}
        public function onRerollChallenge():void {}
        public function unlockZeus():void {}
    }
}
