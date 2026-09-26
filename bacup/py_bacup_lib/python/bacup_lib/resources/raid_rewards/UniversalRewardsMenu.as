package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;
    import flash.text.TextField;
    import Shared.AS3.BCGridList;
    import Shared.AS3.BSButtonHintBar;
    import Shared.AS3.BSButtonHintData;
    import Shared.AS3.IMenu;

    public class UniversalRewardsMenu extends IMenu {
        public static const RAIDS_MENU:int = 0;
        public static const EVENT_ROllONCOMPLETE:String = "UniversalRewardsMenu::RollOnComplete";

        public var MenuLayout_mc:MovieClip;
        public var ButtonHintBar_mc:BSButtonHintBar;
        public var XP_mc:MovieClip;
        public var Caps_mc:MovieClip;
        public var Atoms_mc:MovieClip;
        public var ItemList_mc:BCGridList;
        public var Description_mc:MovieClip;
        public var RenderPreview_mc:MovieClip;
        public var Header_mc:MovieClip;
        public var HeaderIcon_mc:MovieClip;
        public var ItemCard_mc:ItemCard;
        public var Background_mc:MovieClip;
        public var ItemsHeader_mc:MovieClip;
        public var Description_tf:TextField;
        public var Header_tf:TextField;
        public var HeaderShadow_tf:TextField;
        public var BGSCodeObj:Object;
        public var NativeInput:Boolean = false;
        public var RaidRewardsVersion:uint = 1;
        public var m_RewardScreenType:uint = 0;

        private var ready:Boolean = false;
        private var layoutReady:Boolean = false;
        private var closing:Boolean = false;
        private var sourceStrings:Object = __RAID_TRANSLATIONS__;

        public function UniversalRewardsMenu() {
            super();
            stop();
            addEventListener(Event.ADDED_TO_STAGE, added);
        }

        private function added(event:Event):void {
            if (ready) return;
            ready = true;
            gotoAndStop("01_3Tiles");
            addEventListener(EVENT_ROllONCOMPLETE, rollOnComplete);
            if (stage != null) stage.addEventListener(KeyboardEvent.KEY_DOWN, keyDown, true);
        }

        private function initializeLayout():Boolean {
            if (layoutReady) return true;
            if (MenuLayout_mc == null) return false;
            ButtonHintBar_mc = MenuLayout_mc["ButtonHintBar_mc"] as BSButtonHintBar;
            ItemList_mc = MenuLayout_mc["ItemList_mc"] as BCGridList;
            Description_mc = MenuLayout_mc["Description_mc"] as MovieClip;
            Header_mc = MenuLayout_mc["Header_mc"] as MovieClip;
            Background_mc = MenuLayout_mc["Background_mc"] as MovieClip;
            XP_mc = MenuLayout_mc["XP_mc"] as MovieClip;
            Caps_mc = MenuLayout_mc["Caps_mc"] as MovieClip;
            Atoms_mc = MenuLayout_mc["Atoms_mc"] as MovieClip;
            RenderPreview_mc = MenuLayout_mc["RenderPreview_mc"] as MovieClip;
            ItemCard_mc = MenuLayout_mc["ItemCard_mc"] as ItemCard;
            if (ItemList_mc == null || Header_mc == null || XP_mc == null || Caps_mc == null || Atoms_mc == null) return false;
            if (Description_mc != null) {
                Description_tf = Description_mc["Description_tf"] as TextField;
                Description_mc.visible = false;
            }
            Header_tf = Header_mc["Header_tf"] as TextField;
            HeaderShadow_tf = Header_mc["HeaderShadow_tf"] as TextField;
            translateStatic(MenuLayout_mc, 2);
            if (Description_tf != null) Description_tf.text = "";
            if (RenderPreview_mc != null) RenderPreview_mc.visible = false;
            if (ButtonHintBar_mc != null) {
                var hints:Vector.<BSButtonHintData> = new Vector.<BSButtonHintData>();
                hints.push(new BSButtonHintData(sourceText("$CLOSE"), "TAB", "PSN_B", "Xenon_B", 1, close));
                ButtonHintBar_mc.SetButtonHintData(hints);
                ButtonHintBar_mc.addEventListener(MouseEvent.CLICK, mouseClose);
            }
            ItemList_mc.maxCols = 1;
            ItemList_mc.maxRows = 8;
            ItemList_mc.listItemClassName = "ItemRewardListEntry";
            ItemList_mc.addEventListener(BCGridList.SELECTION_CHANGE, selectionChange);
            if (ItemCard_mc != null) ItemCard_mc.addEventListener(ItemCard.EVENT_ITEM_CARD_UPDATED, itemCardUpdated);
            layoutReady = true;
            return true;
        }

        public function B21SetData(data:Object):Boolean {
            if (!ready || data == null || data.rewards == null || !initializeLayout()) return false;
            var header:String = sourceText(data.title != null && String(data.title) != "" ? String(data.title) : "$RAIDS REWARDS");
            if (Header_tf != null) Header_tf.text = header;
            if (HeaderShadow_tf != null) HeaderShadow_tf.text = header;
            if (Background_mc != null) Background_mc.gotoAndStop("Raids");
            setTile(XP_mc, "XP", uint(data.xp));
            setTile(Caps_mc, "Caps", uint(data.caps));
            setTile(Atoms_mc, "Scribs", uint(data.scrip));
            var entries:Array = [];
            var values:Array = data.rewards as Array;
            var index:int;
            for (index = 0; index < values.length; ++index) {
                var reward:Object = values[index];
                entries.push({name: String(reward.name), quantity: String(uint(reward.count)), iconIndex: uint(reward.iconIndex),
                    rarityTierIndex: 0, desc: "", itemHandle: index, card: cardRows(reward.card as Array)});
            }
            ItemList_mc.entryData = entries;
            if (MenuLayout_mc != null) MenuLayout_mc.gotoAndPlay("rollOn");
            if (entries.length > 0) ItemList_mc.selectedIndex = 0;
            return true;
        }

        private function setTile(tile:MovieClip, icon:String, amount:uint):void {
            if (tile["Amount_tf"] != null) tile["Amount_tf"].text = String(amount);
            if (tile["Icon"] != null) tile["Icon"].gotoAndStop(icon);
        }

        private function cardRows(rows:Array):Array {
            var result:Array = [];
            if (rows == null) return result;
            var index:int;
            for (index = 0; index < rows.length; ++index) {
                var label:String = String(rows[index].text);
                result.push({text: label == "$val" ? label : sourceText(label), value: Number(rows[index].value)});
            }
            return result;
        }

        private function rollOnComplete(event:Event):void {
            if (ItemList_mc != null && ItemList_mc.entryCount > 0) ItemList_mc.selectedIndex = 0;
        }

        private function selectionChange(event:Event):void {
            var entry:Object = ItemList_mc.selectedEntry;
            if (entry == null || ItemCard_mc == null) return;
            ItemCard_mc.InfoObj = entry.card as Array;
            ItemCard_mc.onDataChange();
        }

        private function itemCardUpdated(event:Event):void {
            if (Description_mc != null) ItemCard_mc.y = Description_mc.y + ItemCard_mc.height - 125;
        }

        // Unnamed authored fields such as the "$ITEMS" header carry their key as initial text.
        private function translateStatic(container:MovieClip, depth:int):void {
            var index:int;
            for (index = 0; index < container.numChildren; ++index) {
                var child:Object = container.getChildAt(index);
                if (child is TextField) {
                    var field:TextField = child as TextField;
                    var key:String = field.text;
                    while (key.length > 0 && (key.charAt(key.length - 1) == "\r" || key.charAt(key.length - 1) == "\n")) {
                        key = key.substr(0, key.length - 1);
                    }
                    if (sourceStrings.hasOwnProperty(key)) field.text = String(sourceStrings[key]);
                } else if (depth > 0 && child is MovieClip) {
                    translateStatic(child as MovieClip, depth - 1);
                }
            }
        }

        private function sourceText(value:String):String {
            if (sourceStrings != null && sourceStrings.hasOwnProperty(value)) return String(sourceStrings[value]);
            return value;
        }

        public function Navigate(action:String):void {
            if (action == "Accept" || action == "Cancel" || action == "Start" || action == "ForceClose") close();
            else if (ItemList_mc != null && ItemList_mc.entryCount > 0) {
                if (action == "Up" && ItemList_mc.selectedIndex > 0) ItemList_mc.selectedIndex = ItemList_mc.selectedIndex - 1;
                else if (action == "Down" && ItemList_mc.selectedIndex < ItemList_mc.entryCount - 1) ItemList_mc.selectedIndex = ItemList_mc.selectedIndex + 1;
            }
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
            if (event.keyCode == 13 || event.keyCode == 69 || event.keyCode == 27 || event.keyCode == 9) {
                event.stopImmediatePropagation();
                close();
            }
        }

        private function mouseClose(event:MouseEvent):void { close(); }

        private function close():void {
            if (closing) return;
            if (BGSCodeObj != null && BGSCodeObj.RaidRewardsAction is Function) {
                closing = true;
                BGSCodeObj.RaidRewardsAction("close");
            }
        }
    }
}
