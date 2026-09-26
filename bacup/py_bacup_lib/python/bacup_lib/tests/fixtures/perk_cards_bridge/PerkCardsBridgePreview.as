package {
    import flash.events.Event;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.external.ExternalInterface;
    import flash.text.TextField;
    import Shared.AS3.Data.BSUIDataManager;
    import Shared.AS3.Events.CustomEvent;

    public class PerkCardsBridgePreview extends PerksMenu {
        private var passed:uint = 0;
        private var failures:Array = [];
        private var calls:Array = [];
        private var deckUpdates:uint = 0;

        public function PerkCardsBridgePreview() {
            super();
            B21Initialize();
            check(BGSCodeObj != null, "code object constructed");
            check(B21BridgeVersion == 2, "initialized bridge handshake");
            BGSCodeObj.PerkCardEvent = capture;
            ExternalInterface.addCallback("command", command);
            BSUIDataManager.GetDataFromClient("PerksUIData").listener = applyDeckData;
            var frame:Object = {revision: "18446744073709551600", generation: "19",
                character: {INT_base: 7}, shared: {sharedPerkCardDataA: []},
                legendary: {perkCoins: 42}, packs: {itemData: [{count: 3}]},
                perks: {perkCardDataA: [{uniqueID: 12}], numLevelUpPoints: 8,
                    unownedPerkCardDataA: [{uniqueID: 0, SPECIALIndex: 1}],
                    levelUpPerkCardDataA: [{uniqueID: 0, SPECIALIndex: 2}],
                    isLimitedViewMode: false, equippedCardIDs: [12], allowEditsToCardsAndDecks: false}};
            check(B21SetData(frame, true), "first frame completes all provider consumers");
            check(platform == 1, "controller platform");
            check(BSUIDataManager.GetDataFromClient("CharacterInfoData").data.INT_base == 7, "character provider");
            check(BSUIDataManager.GetDataFromClient("PerksUIData").data.numLevelUpPoints == 8, "pick balance provider");
            check(BSUIDataManager.GetDataFromClient("PerksUIData").data.perkCardDataA[0].uniqueID == 12, "stable instance id");
            check(BSUIDataManager.GetDataFromClient("CardPacksUIData").data.itemData[0].count == 3, "pack provider");
            check(BSUIDataManager.GetDataFromClient("LegendaryPerksMenuData").data.perkCoins == 42, "legendary provider");
            check(BSUIDataManager.GetDataFromClient("PerksUIData").changes == 1, "one provider refresh");
            check(BSUIDataManager.GetDataFromClient("PerksUIData").ready, "SetReady false marks ready without double dispatch");
            check(deckUpdates == 1, "normal deck data applied");
            check(_ViewUnownedCards, "unmigrated read-only menu shows the unowned catalog");
            check(frame.character.INT == 7, "SPECIAL picker receives base and total fields");
            var visibleCards:Array = frame.perks.perkCardDataA.concat(frame.perks.unownedPerkCardDataA);
            check(visibleCards.filter(sourceGameModeFilter, this).length == 2,
                "source game-mode predicate retains owned and unowned cards");
            check(frame.perks.levelUpPerkCardDataA[0].supportedGameMode == 0, "pick candidates use offline game mode");
            check(_InitialShowQueue.length == 0, "read-only rewards do not start uncommittable pick modals");
            addChild(Collection_mc);
            stage.focus = Collection_mc;
            B21SetButtons();
            check(AcceptButton.ButtonVisible && !AcceptButton.ButtonEnabled, "read-only equip hint remains visible and disabled");
            check(RankUpButton.ButtonVisible && !RankUpButton.ButtonEnabled, "read-only rank hint remains visible and disabled");
            check(LevelUpButton.ButtonVisible && !LevelUpButton.ButtonEnabled, "pending pick hint remains visible and disabled");
            check(!OpenCardPackButton.ButtonEnabled, "unimplemented pack opening remains disabled");
            _AllowEditsToCardsAndDecks = true;
            frame.character.level = 80;
            frame.perks.allowEditsToCardsAndDecks = true;
            frame.perks.numLevelUpPoints = 0;
            frame.perks.specialPointsAvailable = 3;
            frame.perks.permanentBonusPoints = 0;
            check(B21SetData(frame, false), "live SPECIAL-only frame applies");
            B21SetButtons();
            check(LevelBoostPurchaseButton.ButtonVisible && LevelBoostPurchaseButton.ButtonEnabled && !LevelUpButton.ButtonEnabled, "SPECIAL has its own action without picks");
            check(!B21PickPerkOnly(), "banked SPECIAL never redirects card selection after level 50");
            addChild(PickSpecial_mc);
            B21OpenSPECIAL();
            check(stage.focus == PickSpecial_mc, "separate SPECIAL action opens its picker after level 50");
            stage.focus = Collection_mc;
            check(LevelBoostPurchaseButton.ButtonText == "$B21_TFA_AssignSPECIAL", "SPECIAL-only action label");
            check(B21AllocationValues().INT_base == 7, "allocation cap excludes permanent and legendary bonuses");
            frame.perks.numLevelUpPoints = 8;
            frame.perks.permanentBonusPoints = 1;
            check(B21SetData(frame, false), "permanent bonus frame applies");
            B21SetButtons();
            check(B21PicksAfterSpecial() == 0, "free permanent bonus assignment does not consume a card pick");
            check(B21AllocationValues().INT_base == 0, "permanent bonus assignment bypasses allocation cap");
            check(LevelBoostPurchaseButton.ButtonText == "$B21_TFA_AssignPermanentSPECIAL", "permanent bonus action label");
            frame.perks.permanentBonusPoints = 0;
            frame.perks.specialPointsAvailable = 0;
            check(B21SetData(frame, false), "live card-only frame applies");
            check(!B21PickPerkOnly() && B21PicksAfterSpecial() == 0, "SPECIAL allocation never consumes a card pick");
            _SPECIALToLevel = 4;
            check(!B21PickPerkOnly() && _SPECIALToLevel == -1, "picking any card never forces a SPECIAL choice");
            check(!ShareButton.ButtonVisible, "multiplayer share remains hidden");
            addChild(PickAPerk_mc);
            B21OpenPerks();
            check(stage.focus == PickAPerk_mc, "banked picks open the card picker");
            frame.perks.numLevelUpPoints = 0;
            check(B21SetData(frame, false), "exhausted credits frame applies");
            check(stage.focus == Collection_mc, "last card pick closes its chooser");
            check(!LevelUpButton.ButtonVisible && !LevelUpButton.ButtonFlashing,
                "exhausted picks remove the level-up notification");
            addChild(PerkLevelUpOption_mc);
            stage.focus = PerkLevelUpOption_mc;
            PerkLevelUpOption_mc.visible = true;
            check(B21SetData(frame, false) && !PerkLevelUpOption_mc.visible && stage.focus == Collection_mc,
                "exhausted legacy level-up modal is dismissed");
            B21OpenPerks();
            B21OpenSPECIAL();
            check(stage.focus == Collection_mc, "exhausted keyboard actions cannot reopen either chooser");
            check(!LevelBoostPurchaseButton.ButtonVisible && !LevelBoostPurchaseButton.ButtonEnabled,
                "exhausted SPECIAL hides its increase action");
            stage.focus = PickSpecial_mc;
            check(B21SetData(frame, false) && stage.focus == Collection_mc, "last SPECIAL credit closes its chooser");
            var carousel:FilteredCarousel = new FilteredCarousel();
            check(carousel.B21FilterText("$FILTER") == "$B21_TFA_PerkCards_FilterAll", "all filter has an explicit label");
            check(carousel.B21FilterText("$FILTER_PERCEPTION") == "$FILTER_PERCEPTION", "SPECIAL filter label is preserved");
            visibleCards[0] = {uniqueID: 12};
            check(visibleCards.filter(sourceGameModeFilter, this).length == 1,
                "missing game mode reproduces the source collection rejection");
            var payload:Object = {uniqueID: 12};
            BSUIDataManager.backend.DispatchEventToGame(new CustomEvent("EquipCard", payload));
            check(calls.length == 1 && calls[0].action == "EquipCard", "backend callback");
            check(calls[0].payload.uniqueID == 12, "callback instance id");
            check(calls[0].payload.revision == "18446744073709551600", "64-bit revision remains string");
            check(calls[0].payload.generation == "19", "load generation");
            check(!payload.hasOwnProperty("revision"), "original event payload unchanged");
            BSUIDataManager.backend.DispatchEventToGame(new CustomEvent("QueryOpenCardPack", {callback: "OnQueryOpenCardPackResult"}));
            check(calls.length == 2 && calls[1].action == "QueryOpenCardPack", "source pack query callback");
            BSUIDataManager.backend.DispatchEventToGame(new CustomEvent("OpenCardPack", {itemServerHandleID: 1, count: 2}));
            check(calls.length == 3 && calls[2].payload.count == 2, "source multi-pack open intent");
            BSUIDataManager.backend.DispatchEventToGame(new Event("ViewedCardPack"));
            check(calls.length == 4 && calls[3].action == "ViewedCardPack", "source reveal completion intent");
            B21Event(new Event("CloseMenu"));
            check(calls.length == 5 && calls[4].action == "CloseMenu", "close without custom payload");
            BGSCodeObj = null;
            B21Event(new Event("CloseMenu"));
            check(calls.length == 5, "removed native handler ignored");
            BGSCodeObj = {};
            B21Event(new Event("CloseMenu"));
            check(calls.length == 5, "missing callback ignored");
            check(B21SetData(frame, false), "repeat frame completes");
            check(platform == 0, "keyboard platform");
            check(BSUIDataManager.GetDataFromClient("ScreenResolutionData").changes == 1, "darkeners created only once");
            frame.perks = {isLimitedViewMode: true, allowEditsToCardsAndDecks: false,
                perkCardDataA: [{uniqueID: 12, equipped: true}]};
            var oldFailed:Boolean = false;
            try { applyDeckData(frame.perks); } catch (oldError:Error) { oldFailed = true; }
            check(oldFailed, "old preview flag reproduces missing limited-view-data failure");
            check(B21SetData(frame, false), "installed host frame adapts to the normal player layout");
            check(frame.perks.equippedCardIDs.length == 1 && frame.perks.equippedCardIDs[0] == 12,
                "installed host equipped IDs derive from the actual cards");
            frame.perks.equippedCardIDs = null;
            check(!B21SetData(frame, false), "missing equipped IDs rejects the deck frame");
            check(B21LastError.length > 0, "provider failure is reported to native host");
            frame.perks.equippedCardIDs = [];
            check(B21SetData(frame, false), "empty collection applies successfully");
            check(B21LastError == "", "successful frame clears the diagnostic");
            _NumLevelUpPoints = 0;
            B21FreeRespec();
            --_NumLevelUpPoints;
            check(_NumLevelUpPoints == 0, "free respec preserves zero pick balance");
            _NumLevelUpPoints = 8;
            B21FreeRespec();
            --_NumLevelUpPoints;
            check(_NumLevelUpPoints == 8, "free respec preserves pending picks");
            var target:Object = {uniqueID: 1, basePerkID: 12, rank: 1, canRankUp: true};
            _PerkCardData = [target,
                {uniqueID: 2, basePerkID: 12, rank: 0, canConsume: true},
                {uniqueID: 3, basePerkID: 12, rank: 1, canConsume: true},
                {uniqueID: 4, basePerkID: 12, rank: 0, canConsume: false},
                {uniqueID: 5, basePerkID: 14, rank: 0, canConsume: true}];
            var ingredients:Array = B21RankCandidates(target);
            check(ingredients.length == 1 && ingredients[0].uniqueID == 2, "only unassigned rank-one duplicate is consumed");
            target.canRankUp = false;
            check(B21RankCandidates(target).length == 0, "unavailable next rank cannot merge");
            var confirmation:PerkCardRankConfirmation = new PerkCardRankConfirmation();
            confirmation.B21SetData({isAnimatedPerk: false}, {isAnimatedPerk: true});
            check(!confirmation.ProducedCard_mc.dataObj.isAnimatedPerk, "foil ingredient does not change preserved target");
            confirmation.B21SetData({isAnimatedPerk: true}, {isAnimatedPerk: false});
            check(confirmation.ProducedCard_mc.dataObj.isAnimatedPerk, "foil target stays foil");
            QuantityModal_mc.opened = true;
            check(B21PromptActive, "quantity dialog uses native modal navigation");
            B21ProcessUserEvent("Left",true);
            check(QuantityModal_mc.quantity == 202, "quantity changes once on release");
            B21ProcessUserEvent("Left",false);
            check(QuantityModal_mc.quantity == 201, "quantity decreases from the reported 202 packs");
            B21ProcessUserEvent("Right",false);
            check(QuantityModal_mc.quantity == 202, "quantity increases");
            B21ProcessUserEvent("Accept",true);
            check(QuantityModal_mc.confirmed == 0, "press does not confirm twice");
            B21ProcessUserEvent("Accept",false);
            check(QuantityModal_mc.confirmed == 1 && !B21PromptActive, "release reaches the source confirmation callback");
            QuantityModal_mc.opened = true;
            B21ProcessUserEvent("Cancel",true);
            B21ProcessUserEvent("Cancel",false);
            check(QuantityModal_mc.confirmed == 1 && QuantityModal_mc.canceled == 1 && !B21PromptActive,
                "cancel closes quantity without opening packs");
            addChild(Header_mc);
            Header_mc.graphics.beginFill(0xFFFFCB);
            Header_mc.graphics.drawRect(0,0,216,40);
            Header_mc.graphics.endFill();
            addChild(ButtonHintBar_mc);
            ButtonHintBar_mc.graphics.beginFill(0xFFFFCB);
            ButtonHintBar_mc.graphics.drawRect(-1000,0,2100,30);
            ButtonHintBar_mc.graphics.endFill();
            dispatchEvent(new Event(Event.ENTER_FRAME));
            check(ButtonHintBar_mc.getBounds(this).left >= 360 && ButtonHintBar_mc.getBounds(this).right <= 1872.1,
                "long footer fits between source left anchor and right margin");
            ButtonHintBar_mc.graphics.clear();
            ButtonHintBar_mc.graphics.beginFill(0xFFFFCB);
            ButtonHintBar_mc.graphics.drawRect(-100,0,200,30);
            ButtonHintBar_mc.graphics.endFill();
            dispatchEvent(new Event(Event.ENTER_FRAME));
            check(ButtonHintBar_mc.scaleX == 1 && ButtonHintBar_mc.scaleY == 1,
                "short modal footer restores natural text size");
            for each (var color:uint in [0x00FF00,0xFFB642,0x60BFFF]) {
                B21SetHUDColor(color);
                check(Math.abs(ButtonHintBar_mc.transform.colorTransform.redMultiplier-((color>>16)&255)/255)<0.005,
                    "footer follows HUD color");
                check(Collection_mc.transform.colorTransform.redMultiplier == 1 && Collection_mc.transform.colorTransform.blueMultiplier == 1,
                    "card art remains untinted");
            }
            removeChild(Header_mc);
            removeChild(ButtonHintBar_mc);
            ButtonHintBar_mc.visible = false;
            var status:TextField = new TextField();
            status.width = 1000;
            status.height = 400;
            status.textColor = 0xffffff;
            status.text = "Perk card Scaleform bridge: " + passed + " passed, " + failures.length + " failed\n" + failures.join("\n");
            addChild(status);
            if (ExternalInterface.available) ExternalInterface.call("perkBridgeResult", passed, failures.join("|"));
            BGSCodeObj = {PerkCardEvent: capture};
        }

        private function findPrompt(node:DisplayObject):Object {
            if (node.hasOwnProperty("B21Show")) return node;
            if (node is DisplayObjectContainer) {
                var container:DisplayObjectContainer = DisplayObjectContainer(node);
                for (var i:uint = 0; i < container.numChildren; ++i) {
                    var found:Object = findPrompt(container.getChildAt(i));
                    if (found != null) return found;
                }
            }
            return null;
        }

        public function command(action:String):Object {
            if (action == "Query") B21QueryOpenCardPack(true);
            else if (action == "Unavailable") B21QueryOpenCardPack(false);
            else if (action != "Read") B21ProcessUserEvent(action, false);
            var prompt:Object = findPrompt(this);
            return {prompt:prompt != null, active:B21PromptActive, index:prompt == null ? -1 : prompt.List_mc.selectedIndex,
                results:queryResults, calls:calls, children:numChildren, error:B21LastError,
                focusRect:stage.stageFocusRect};
        }

        private function capture(action:String, payload:Object):void { calls.push({action: action, payload: payload}); }
        private function sourceGameModeFilter(card:Object, index:int, cards:Array):Boolean {
            return card.supportedGameMode == BSUIDataManager.GetDataFromClient("PerkCardGameModeFilterUIData").data.gameModeFilter;
        }
        private function applyDeckData(data:Object):void {
            var ids:Array = data.isLimitedViewMode ? data.limitedViewData.equippedCardIDs : data.equippedCardIDs;
            var count:uint = ids.length;
            ++deckUpdates;
        }
        private function check(value:Boolean, name:String):void {
            if (value) ++passed;
            else failures.push(name);
        }
    }
}
