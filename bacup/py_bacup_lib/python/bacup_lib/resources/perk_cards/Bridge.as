package {
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.events.KeyboardEvent;
    import flash.display.Loader;
    import flash.display.Sprite;
    import flash.display.DisplayObject;
    import flash.geom.ColorTransform;
    import flash.geom.Rectangle;
    import flash.net.URLRequest;
    import Shared.AS3.IMenu;
    import Shared.AS3.Data.BSUIDataManager;
    import Shared.AS3.Data.BSUIEventDispatcherBackend;
    import Shared.AS3.Events.CustomEvent;

    public class PerksMenu extends IMenu {
        public var BGSCodeObj:Object;
        public var B21BridgeVersion:uint;
        public var B21LastError:String = "";
        private var B21Revision:String;
        private var B21Generation:String;
        private var B21Presented:Boolean = false;
        private var B21BonusPoints:uint;
        private var B21Allocation:Object;
        private var B21PackPrompt:Loader;
        private var B21PackDarkener:Sprite;
        private var B21PromptMenu:Object;
        private var B21SuppressPrompt:Boolean;
        private var B21Gamepad:Boolean;

        public function B21Initialize():void {
            BGSCodeObj = new Object();
            B21Revision = "";
            B21Generation = "";
            var backend:BSUIEventDispatcherBackend = new BSUIEventDispatcherBackend();
            backend.DispatchEventToGame = B21Event;
            BSUIDataManager.InitDataManager(backend);
            B21Publish("PerkCardGameModeFilterUIData", {gameModeFilter: 0});
            B21Publish("HUDColors", {hue: 0, saturation: 0, value: 0, contrast: 0});
            B21BridgeVersion = 2;
            focusRect = false;
            tabEnabled = false;
            tabChildren = false;
            addEventListener(Event.ENTER_FRAME, B21FitFooter);
        }

        public function B21SetData(frame:Object, gamepad:Boolean):Boolean {
            try {
                B21LastError = "";
                B21Revision = frame.revision;
                B21Generation = frame.generation;
                B21Gamepad = gamepad;
                B21SuppressPrompt = Boolean(frame.packs.suppressPrompt);
                stage.stageFocusRect = false;
                // The first FO4 host used the multiplayer inspection flag for read-only mode.
                frame.perks.isLimitedViewMode = false;
                if (!frame.perks.hasOwnProperty("equippedCardIDs")) {
                    frame.perks.equippedCardIDs = [];
                    for each (var card:Object in frame.perks.perkCardDataA) {
                        if (card.equipped) frame.perks.equippedCardIDs.push(card.uniqueID);
                    }
                }
                frame.character.playerRace = 1;
                frame.character.hasSeenPVPTutorial = true;
                for each (var pick:Object in frame.perks.levelUpPerkCardDataA) pick.unequippedDuplicateCount = 0;
                for each (var cards:Array in [frame.perks.perkCardDataA, frame.perks.unownedPerkCardDataA,
                        frame.perks.levelUpPerkCardDataA, frame.packs.openedPackCardData]) {
                    if (cards != null) {
                        for each (var entry:Object in cards) entry.supportedGameMode = 0;
                    }
                }
                for each (var stat:String in ["STR", "PER", "END", "CHA", "INT", "AGI", "LCK"]) {
                    frame.character[stat] = uint(frame.character[stat + "_base"]) + uint(frame.character[stat + "_bonus"]);
                }
                B21BonusPoints = uint(frame.perks.permanentBonusPoints);
                B21Allocation = new Object();
                for each (var allocationStat:String in ["STR", "PER", "END", "CHA", "INT", "AGI", "LCK"]) {
                    B21Allocation[allocationStat + "_base"] = B21BonusPoints > 0 ? 0 : uint(frame.character[allocationStat + "_base"]);
                }
                this._NumLevelUpPoints = frame.perks.numLevelUpPoints;
                this._NumSPECIALPoints = frame.perks.specialPointsAvailable;
                this.SetPlatform(gamepad ? 1 : 0, false, gamepad ? 1 : 0, 0);
                if (!B21Presented) {
                    B21Publish("ScreenResolutionData", {ScreenWidth: 1920, ScreenHeight: 1080});
                    if (!frame.perks.allowEditsToCardsAndDecks) {
                        this._ViewUnownedCards = true;
                        this._InitialShowQueue = [];
                    }
                }
                B21Publish("CharacterInfoData", frame.character);
                B21Publish("SharedPerksUIData", frame.shared);
                B21Publish("LegendaryPerksMenuData", frame.legendary);
                B21Publish("CardPacksUIData", frame.packs);
                B21Publish("PerksUIData", frame.perks);
                this._NumLevelUpPoints = frame.perks.numLevelUpPoints;
                this._NumSPECIALPoints = frame.perks.specialPointsAvailable;
                if (((stage.focus == this.PickAPerk_mc || stage.focus == this.PerkLevelUpOption_mc) && this._NumLevelUpPoints == 0) ||
                        (stage.focus == this.PickSpecial_mc && this._NumSPECIALPoints == 0)) this.FocusBase();
                if (this._NumLevelUpPoints == 0) this.PerkLevelUpOption_mc.visible = false;
                this.SetButtons();
                B21Presented = true;
                return true;
            } catch (error:Error) {
                B21LastError = error.message;
                return false;
            }
        }

        public function B21Publish(name:String, payload:Object):void {
            var provider:Object = BSUIDataManager.GetDataFromClient(name);
            for (var key:String in payload) provider.data[key] = payload[key];
            provider.SetReady(false);
            provider.DispatchChange();
        }

        public function B21SetButtons():Boolean {
            var base:Boolean = stage.focus == this.Collection_mc || stage.focus == this.EquipDeckHolder_mc;
            this.ShareButton.ButtonVisible = false;
            this.ShareButton.ButtonEnabled = false;
            this.LevelBoostPurchaseButton.ButtonVisible = base && this._AllowEditsToCardsAndDecks && this._NumSPECIALPoints > 0;
            this.LevelBoostPurchaseButton.ButtonEnabled = this._AllowEditsToCardsAndDecks && this._NumSPECIALPoints > 0;
            this.LevelBoostPurchaseButton.ButtonText = B21BonusPoints > 0 ? "$B21_TFA_AssignPermanentSPECIAL" :
                this._NumSPECIALPoints > 0 ? "$B21_TFA_AssignSPECIAL" : "$B21_TFA_MoveSPECIAL";
            if (!this._AllowEditsToCardsAndDecks) {
                this.AcceptButton.ButtonVisible = base;
                this.AcceptButton.ButtonEnabled = false;
                this.RankUpButton.ButtonVisible = base;
                this.RankUpButton.ButtonEnabled = false;
                this.LevelUpButton.ButtonVisible = base && this._NumLevelUpPoints > 0;
                this.LevelUpButton.ButtonEnabled = false;
                this.ShareButton.ButtonVisible = false;
                this.ShareButton.ButtonEnabled = false;
                this.OpenCardPackButton.ButtonEnabled = false;
            } else if (base) {
                this.LevelUpButton.ButtonVisible = this._NumLevelUpPoints > 0;
                this.LevelUpButton.ButtonEnabled = this._NumLevelUpPoints > 0;
            }
            if (this._NumLevelUpPoints == 0) this.LevelUpButton.ButtonFlashing = false;
            return true;
        }

        public function B21QueryOpenCardPack(allowed:Boolean):void {
            if (B21PackPrompt != null) return;
            if (!allowed || B21SuppressPrompt) { this.OnQueryOpenCardPackResult(false); return; }
            B21PackDarkener = new Sprite();
            B21PackDarkener.graphics.beginFill(0, 0.75);
            B21PackDarkener.graphics.drawRect(-stage.stageWidth, -stage.stageHeight,
                stage.stageWidth * 3, stage.stageHeight * 3);
            B21PackDarkener.graphics.endFill();
            addChild(B21PackDarkener);
            B21PackPrompt = new Loader();
            addChild(B21PackPrompt);
            stage.addEventListener(KeyboardEvent.KEY_DOWN, B21PromptKey, true, 1000);
            stage.addEventListener(KeyboardEvent.KEY_UP, B21PromptKey, true, 1000);
            B21PackPrompt.contentLoaderInfo.addEventListener(Event.COMPLETE, B21PromptLoaded);
            B21PackPrompt.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, B21PromptFailed);
            B21PackPrompt.load(new URLRequest("Interface/B21/TalesFromAppalachia/PerkCards/packprompt.swf"));
        }

        private function B21PromptKey(event:KeyboardEvent):void {
            if (B21PackPrompt != null) event.stopImmediatePropagation();
        }

        private function B21PromptLoaded(event:Event):void {
            try {
                B21PromptMenu = Object(B21PackPrompt.content).Menu_mc;
                B21PromptMenu.B21Show(B21PromptAnswer, B21Gamepad);
                B21Tint(B21PromptMenu as DisplayObject);
            } catch (error:Error) { B21LastError = error.message; B21PromptAnswer(1); }
        }

        private function B21PromptFailed(event:IOErrorEvent):void {
            B21LastError = event.text;
            B21PromptAnswer(1);
        }

        public function B21PromptAnswer(index:uint):void {
            if (B21PackPrompt == null) return;
            stage.removeEventListener(KeyboardEvent.KEY_DOWN, B21PromptKey, true);
            stage.removeEventListener(KeyboardEvent.KEY_UP, B21PromptKey, true);
            removeChild(B21PackPrompt);
            removeChild(B21PackDarkener);
            B21PackPrompt.unload();
            B21PackPrompt = null;
            B21PackDarkener = null;
            B21PromptMenu = null;
            if (index == 2) {
                B21SuppressPrompt = true;
                B21Event(new CustomEvent("SuppressPackPrompt", {}));
            }
            this.FocusBase();
            this.OnQueryOpenCardPackResult(index == 0);
        }

        public function B21ProcessUserEvent(action:String, pressed:Boolean):Boolean {
            if (B21PackPrompt != null) {
                if (B21PromptMenu != null) B21PromptMenu.B21Input(action, pressed);
                return true;
            }
            if (this.hasOwnProperty("QuantityModal_mc") && this.QuantityModal_mc != null && this.QuantityModal_mc.opened) {
                if (action == "Accept" || action == "Activate") {
                    if (!pressed) this.QuantityModal_mc.onConfirm();
                } else if (action == "Cancel" || action == "ForceClose") {
                    if (!pressed) this.QuantityModal_mc.onCancel();
                } else if (action == "Left" || action == "Right") {
                    if (!pressed) this.QuantityModal_mc.modifyQuantity(action == "Left" ? -1 : 1);
                } else {
                    this.QuantityModal_mc.ProcessUserEvent(action == "Boost" ? "RTrigger" : this.ConvertEventString(action), pressed);
                }
                return true;
            }
            return this.ProcessUserEvent(action, pressed);
        }

        public function get B21PromptActive():Boolean {
            return B21PackPrompt != null || (this.hasOwnProperty("QuantityModal_mc") && this.QuantityModal_mc != null && this.QuantityModal_mc.opened);
        }

        private var B21HUDColor:uint = 0xFFFFCB;

        public function B21SetHUDColor(color:uint):void {
            B21HUDColor = color;
            for each (var name:String in ["ButtonHintBar_mc", "FloatingButtonHintBar_mc", "Header_mc",
                    "TotalPoints_tf", "PerkCoinIcon_mc", "LegendaryPerksButton_mc", "ModeButton_mc", "QuantityModal_mc"]) {
                if (this.hasOwnProperty(name)) B21Tint(this[name] as DisplayObject);
            }
            if (B21PromptMenu != null) B21Tint(B21PromptMenu as DisplayObject);
        }

        private function B21Tint(clip:DisplayObject):void {
            if (clip == null) return;
            var color:ColorTransform = clip.transform.colorTransform;
            color.redMultiplier = ((B21HUDColor >> 16) & 255) / 255;
            color.greenMultiplier = ((B21HUDColor >> 8) & 255) / 255;
            color.blueMultiplier = (B21HUDColor & 255) / 203;
            clip.transform.colorTransform = color;
        }

        private function B21FitFooter(event:Event):void {
            if (!B21Presented || !this.hasOwnProperty("ButtonHintBar_mc")) return;
            var bar:Object = this.ButtonHintBar_mc;
            if (bar == null || !bar.visible) return;
            var bounds:Rectangle = bar.getBounds(bar);
            if (bounds.width <= 0) return;
            var left:Number = Math.max(48, bar.StartingXPos);
            if (this.Header_mc != null) left = Math.max(left, this.Header_mc.getBounds(this).right + 24);
            var scale:Number = Math.min(1, (1872 - left) / bounds.width);
            bar.scaleX = bar.scaleY = scale;
            bar.x = left - bounds.left * scale;
        }

        public function B21AllocationValues():Object {
            return B21Allocation;
        }

        public function B21PicksAfterSpecial():uint {
            return 0;
        }

        public function B21PickPerkOnly():Boolean {
            this._SPECIALToLevel = -1;
            return false;
        }

        public function B21OpenSPECIAL():void {
            if (!this._AllowEditsToCardsAndDecks || this._NumSPECIALPoints == 0) return;
            this.SetStageFocus(this.PickSpecial_mc);
        }

        public function B21OpenPerks():void {
            if (!this._AllowEditsToCardsAndDecks || this._NumLevelUpPoints == 0) return;
            this.onLevelUpButtonPressed();
        }

        public function B21PerksHandler():Function {
            return B21OpenPerks;
        }

        public function B21SPECIALHandler():Function {
            return B21OpenSPECIAL;
        }

        public function B21FreeRespec():void {
            this._NumLevelUpPoints += 1;
        }

        public function B21RankCandidates(card:Object):Array {
            var result:Array = [];
            if (card == null || !card.canRankUp) return result;
            for each (var candidate:Object in this._PerkCardData) {
                if (candidate.uniqueID != card.uniqueID && candidate.basePerkID == card.basePerkID &&
                        candidate.rank == 0 && candidate.canConsume) result.push(candidate);
            }
            return result;
        }

        public function B21Event(event:Event):void {
            if (BGSCodeObj == null || !BGSCodeObj.hasOwnProperty("PerkCardEvent")) return;
            var params:Object = event is CustomEvent ? CustomEvent(event).params : new Object();
            var payload:Object = new Object();
            for (var key:String in params) payload[key] = params[key];
            payload.revision = B21Revision;
            payload.generation = B21Generation;
            BGSCodeObj.PerkCardEvent(event.type, payload);
        }
    }
}
