package SpecialBuilds {
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import Shared.AS3.IMenu;
    import Shared.AS3.BSButtonHintData;
    import Shared.AS3.Data.BSUIDataManager;
    import Shared.AS3.Data.BSUIEventDispatcherBackend;
    import Shared.AS3.Events.CustomEvent;

    public class SpecialBuildsMenu extends IMenu {
        public var BGSCodeObj:Object;
        public var B21BridgeVersion:uint;
        public var B21LastError:String;
        private var B21Revision:String;
        private var B21Generation:String;
        private var B21RenamePanel:Sprite;
        private var B21Name:TextField;
        private var B21RenameID:uint;
        private var B21Presented:Boolean;

        public function B21Initialize():void {
            BGSCodeObj = new Object();
            B21LastError = "";
            var backend:BSUIEventDispatcherBackend = new BSUIEventDispatcherBackend();
            backend.DispatchEventToGame = B21Event;
            BSUIDataManager.InitDataManager(backend);
            B21BridgeVersion = 2;
            focusRect = false;
            tabEnabled = false;
            tabChildren = false;
            this.m_UnlockButton.ButtonText = "$B21_TFA_AddLoadout";
        }

        public function B21Publish(name:String, payload:Object):void {
            var provider:Object = BSUIDataManager.GetDataFromClient(name);
            for (var key:String in payload) provider.data[key] = payload[key];
            provider.SetReady(false);
            provider.DispatchChange();
        }

        public function B21SetData(frame:Object, gamepad:Boolean):Boolean {
            var step:String = "frame tokens";
            try {
                B21LastError = "";
                B21Revision = frame.revision;
                B21Generation = frame.generation;
                stage.stageFocusRect = false;
                step = "platform";
                this.SetPlatform(gamepad ? 1 : 0, false, gamepad ? 1 : 0, 0);
                step = "editor platform";
                this.EditSpecialModal_mc.B21SetPlatform(gamepad);
                step = "screen resolution";
                if (!B21Presented) B21Publish("ScreenResolutionData", {ScreenWidth: 1920, ScreenHeight: 1080});
                step = "menu stack";
                B21Publish("MenuStackData", {menuStackA: []});
                step = "loadouts";
                B21Publish("SpecialBuildsData", frame);
                if (B21RenamePanel != null) {
                    setChildIndex(B21RenamePanel, numChildren - 1);
                    stage.focus = B21Name;
                }
                B21Presented = true;
                return true;
            } catch (error:Error) {
                B21LastError = step + ": " + error.message;
                return false;
            }
        }

        public function B21Input(action:String):void {
            if (B21RenamePanel != null) {
                if (action == "Accept" || action == "Cancel") B21EndRename(action == "Accept");
                return;
            }
            if (this.EditSpecialModal_mc.isActive) {
                this.EditSpecialModal_mc.B21Input(action);
                return;
            }
            if (action == "Up" || action == "Down") {
                var list:Object = this.BuildsWidget_mc.List_mc;
                list.selectedIndex = Math.max(0, Math.min(list.entryData.length - 1,
                    list.selectedIndex + (action == "Down" ? 1 : -1)));
            } else if (action == "Accept") this.onBuildPress(new Event("B21Accept"));
            else this.ProcessUserEvent(action, false);
        }

        public function B21StartRename(id:uint):void {
            if (B21RenamePanel != null) return;
            B21RenameID = id;
            B21RenamePanel = new Sprite();
            B21RenamePanel.focusRect = false;
            B21RenamePanel.graphics.beginFill(0, 0.65);
            B21RenamePanel.graphics.drawRect(-stage.stageWidth, -stage.stageHeight,
                stage.stageWidth * 3, stage.stageHeight * 3);
            B21RenamePanel.graphics.endFill();
            B21RenamePanel.graphics.beginFill(0x080808, 0.98);
            B21RenamePanel.graphics.drawRect(510, 360, 900, 270);
            B21RenamePanel.graphics.endFill();
            var title:TextField = new TextField();
            title.defaultTextFormat = new TextFormat("$MAIN_Font", 32, 0xfff4b0);
            title.x = 555; title.y = 385; title.width = 810; title.height = 55;
            title.text = "$RENAME";
            title.selectable = false;
            title.mouseEnabled = false;
            B21RenamePanel.addChild(title);
            B21Name = new TextField();
            B21Name.defaultTextFormat = new TextFormat("$MAIN_Font", 32, 0xfff4b0);
            B21Name.type = "input";
            B21Name.multiline = false;
            B21Name.focusRect = false;
            B21Name.maxChars = 40;
            B21Name.x = 555; B21Name.y = 455; B21Name.width = 810; B21Name.height = 55;
            B21Name.border = true; B21Name.borderColor = 0xffd75b;
            B21Name.text = this.BuildsWidget_mc.List_mc.selectedEntry.name;
            B21RenamePanel.addChild(B21Name);
            addChild(B21RenamePanel);
            stage.focus = B21Name;
            B21Name.setSelection(0, B21Name.text.length);
            this.m_UnlockButton.ButtonVisible = false;
            this.m_LoadButton.ButtonVisible = false;
            this.m_EditActivePerksButton.ButtonVisible = false;
            this.m_RenameButton.ButtonVisible = false;
            this.m_EditSpecialButton.ButtonVisible = false;
            this.m_ExitButton.ButtonVisible = false;
            var buttons:Vector.<BSButtonHintData> = new Vector.<BSButtonHintData>();
            buttons.push(new BSButtonHintData("$ACCEPT", "ENTER", "PSN_A", "Xenon_A", 1, B21ConfirmRename));
            buttons.push(new BSButtonHintData("$CANCEL", "TAB", "PSN_B", "Xenon_B", 1, B21CancelRename));
            this.ButtonHintBar_mc.SetButtonHintData(buttons);
            B21Send("B21TextEntry", {enabled: true});
            stage.focus = B21Name;
            B21Name.setSelection(0, B21Name.text.length);
        }

        public function B21ConfirmRename():void { B21EndRename(true); }
        public function B21CancelRename():void { B21EndRename(false); }

        public function B21EndRename(accept:Boolean):void {
            if (B21RenamePanel == null) return;
            var name:String = B21Name.text;
            while (name.length > 0 && name.charAt(0) == " ") name = name.substr(1);
            while (name.length > 0 && name.charAt(name.length - 1) == " ") name = name.substr(0, name.length - 1);
            if (accept && name.length == 0) return;
            removeChild(B21RenamePanel);
            B21RenamePanel = null;
            B21Name = null;
            B21Send("B21TextEntry", {enabled: false});
            this.ButtonHintBar_mc.SetButtonHintData(this.m_LoadoutsButtonsVector);
            stage.focus = this.BuildsWidget_mc.List_mc;
            this.updateButtonBar();
            if (accept) B21Send("SpecialBuilds::RenameBuild", {id: B21RenameID, name: name});
        }

        public function B21Event(event:Event):void {
            var params:Object = event is CustomEvent ? CustomEvent(event).params : new Object();
            if (event.type == "SpecialBuilds::RenameBuild") { B21StartRename(params.id); return; }
            B21Send(event.type, params);
        }

        public function B21Send(action:String, params:Object):void {
            if (BGSCodeObj == null || !BGSCodeObj.hasOwnProperty("PunchCardEvent")) return;
            var payload:Object = new Object();
            for (var key:String in params) payload[key] = params[key];
            payload.revision = B21Revision;
            payload.generation = B21Generation;
            BGSCodeObj.PunchCardEvent(action, payload);
        }
    }
}
