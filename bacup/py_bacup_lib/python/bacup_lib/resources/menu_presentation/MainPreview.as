package {
    import flash.display.MovieClip;
    import flash.display.Loader;
    import flash.display.StageScaleMode;
    import flash.display.StageAlign;
    import flash.net.URLRequest;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.ui.Keyboard;
    import flash.external.ExternalInterface;
    import Shared.AS3.BSScrollingList;
    public class MainMenu extends MovieClip {
        private var b21PreviewTicks:int = 0;
        private var b21PhotoFixture:Loader;
        private var b21PhotoLaunches:int = 0;
        private var b21Continues:int = 0;
        private var b21HasSaves:Boolean;
        private var b21LauncherAvailable:Boolean;
        public function B21PreviewBoot():void { addEventListener(Event.ENTER_FRAME, B21PreviewStart); }
        public function B21True(...args):Boolean { return true; }
        public function B21False(...args):Boolean { return false; }
        public function B21Nothing(...args):void {}
        public function B21Continue():void { b21Continues++; }
        public function B21HasSaves():Boolean { return b21HasSaves; }
        public function B21LaunchPhoto(launch:Boolean):Boolean { if (launch) b21PhotoLaunches++; return b21LauncherAvailable; }
        public function B21PreviewAvailability(available:Boolean):void { b21LauncherAvailable = available; }
        private function B21PreviewStart(event:Event):void {
            if (++b21PreviewTicks < 3) return;
            removeEventListener(Event.ENTER_FRAME, B21PreviewStart);
            try {
                stage.scaleMode = StageScaleMode.NO_SCALE;
                stage.align = StageAlign.TOP_LEFT;
                BGSCodeObj = {GetHasSavedGames:B21HasSaves,GetHasInstalledContent:B21True,
                    GetShowCreationClubOption:B21True,GetShowBethesdaNetOption:B21True,
                    IsMainMenuReady:B21True,PlayOKSound:B21Nothing,PlayCancelSound:B21Nothing,
                    AreModsLoaded:B21False,onContinuePress:B21Continue,
                    PlayFocusSound:B21Nothing,SetGamepadCursorVisible:B21Nothing,
                    SetInputMappingMode:B21Nothing,SaveSettings:B21Nothing,
                    StartState:B21Nothing,EndState:B21Nothing,UpdateInputContext:B21Nothing,
                    RegisterSaveLoadPanel:B21Nothing,InitialPopulateLoadList:B21Nothing,SetBackgroundVisible:B21Nothing};
                PauseMode = false;
                b21HasSaves = b21LauncherAvailable = true;
                InitMenu();
                InitList(__B21_INIT_LIST_ARGS__);
                SetToMenu();
                B21TranslateEntries();
                b21PhotoFixture = new Loader();
                b21PhotoFixture.contentLoaderInfo.addEventListener(Event.COMPLETE, B21FitBackdrop);
                stage.addEventListener(Event.RESIZE, B21FitBackdrop);
                addChildAt(b21PhotoFixture, 0);
                b21PhotoFixture.load(new URLRequest("photo-background.png"));
                B21ApplySourceStyle();
                visible = true;
                alpha = 1;
                ExternalInterface.addCallback("menuSelect", B21PreviewSelect);
                ExternalInterface.addCallback("menuState", B21PreviewState);
                ExternalInterface.addCallback("menuMode", B21PreviewMode);
                ExternalInterface.addCallback("menuPress", B21PreviewPress);
                ExternalInterface.addCallback("menuFault", B21PreviewFault);
                ExternalInterface.addCallback("menuClear", B21ClearSourceStyle);
                ExternalInterface.addCallback("menuAvailability", B21PreviewAvailability);
                ExternalInterface.addCallback("menuSettings", B21PreviewSettings);
                ExternalInterface.addCallback("menuLoad", B21PreviewLoad);
                B21PhotoModeAction = B21LaunchPhoto;
                ExternalInterface.call("report", {ready:true});
            } catch (error:Error) { ExternalInterface.call("report", {error:String(error)}); }
        }
        public function B21PreviewLoad(saves:Array):Object {
            var entries:Array = MainPanel_mc.List_mc.entryList;
            for (var i:int = 0; i < entries.length; i++) if (entries[i].index == LOAD_INDEX) B21PreviewPress(i);
            if (currentState == MAIN_STATE) {
                BGSCodeObj.ShowContinueSecondPanel = B21False;
                EndState();
                StartState(SAVE_LOAD_STATE);
            }
            var list:Object = SaveLoadHolder_mc.Panel_mc.List_mc;
            list.entryList = saves;
            list.InvalidateData();
            list.selectedIndex = 0;
            return {state:currentState, saves:list.entryList.length};
        }
        public function B21PreviewSettings(category:int, row:int):Object {
            BGSCodeObj.RequestGameplayOptions = BGSCodeObj.RequestDisplayOptions = BGSCodeObj.RequestAudioOptions = B21PreviewOptions;
            BGSCodeObj.onSettingsValueChange = B21Nothing;
            if (!b21PreviewChanges) b21PreviewChanges = [];
            if (B21SettingsAction == null) {
                B21SettingsAction = B21PreviewSettingsAction;
                B21InstallSettings();
            }
            if (currentState == MAIN_STATE) {
                var entries:Array = MainPanel_mc.List_mc.entryList;
                for (var i:int = 0; i < entries.length; i++) if (entries[i].index == SETTINGS_INDEX) B21PreviewPress(i);
                var categories:Object = SettingsPanel_mc.SettingsList_mc;
                for each (var group:Object in categories.entryList)
                    if (B21_PreviewStrings.values[group.text]) group.text = B21_PreviewStrings.values[group.text];
                categories.InvalidateData();
            }
            try {
                if (category >= 0) {
                    SettingsPanel_mc.SettingsList_mc.selectedIndex = category;
                    onSettingsListItemPress();
                    OptionsPanel_mc.Fader_mc.List_mc.selectedIndex = row;
                } else if (category == -3) {
                    OptionsPanel_mc.Fader_mc.List_mc.selectedIndex = row;
                    OptionsPanel_mc.Fader_mc.List_mc.dispatchEvent(new KeyboardEvent(KeyboardEvent.KEY_DOWN, true, false, 0, Keyboard.RIGHT));
                } else if (category == -1) SettingsPanel_mc.SettingsList_mc.selectedIndex = row;
                else OptionsPanel_mc.Fader_mc.List_mc.selectedIndex = row;
            } catch (error:Error) { return {error:error.getStackTrace() || String(error)}; }
            return {state:currentState, categories:SettingsPanel_mc.SettingsList_mc.entryList.length,
                options:OptionsPanel_mc.Fader_mc.List_mc.entryList.length, changes:b21PreviewChanges.length,
                shown:OptionsPanel_mc.Fader_mc.List_mc.numListItems,
                lastChange:b21PreviewChanges.length ? b21PreviewChanges[b21PreviewChanges.length - 1] : null};
        }
        public var b21PreviewChanges:Array;
        private function B21PreviewSettingsAction(operation:String, id:Object = null, value:Object = null):Object {
            if (operation == "set") { b21PreviewChanges.push([id, value]); return null; }
            var degrees:Array = [];
            for (var d:int = 60; d <= 130; d += 5) degrees.push(String(d));
            return [{text:"Field of View", movieType:1, options:degrees, ID:0xB2100, value:6, b21Setting:true},
                {text:"First-Person Field of View", movieType:1, options:degrees, ID:0xB2101, value:4, b21Setting:true},
                {text:"Depth of Field", movieType:2, ID:0xB2102, value:1, b21Setting:true},
                {text:"Shadow Distance", movieType:1, options:["Low", "Medium", "High", "Ultra"], ID:0xB2105, value:2, b21Setting:true},
                {text:"Window Mode (Restart)", movieType:1, options:["Fullscreen", "Borderless", "Windowed"], ID:0xB2108, value:1, b21Setting:true}];
        }
        private function B21PreviewOptions(options:Array):void {
            for (var i:int = 0; i < options.length; i++) {
                var option:Object = options[i];
                option.ID = i;
                option.value = option.movieType == 0 ? 0.6 : option.movieType == 2 ? i % 2 : 0;
                if (B21_PreviewStrings.values[option.text]) option.text = B21_PreviewStrings.values[option.text];
                if (option.options) for (var k:int = 0; k < option.options.length; k++)
                    if (B21_PreviewStrings.values[option.options[k]]) option.options[k] = B21_PreviewStrings.values[option.options[k]];
            }
        }
        public function B21PreviewSelect(index:int):void { MainPanel_mc.List_mc.selectedIndex = index; }
        private function B21TranslateEntries():void {
            for each (var entry:Object in MainPanel_mc.List_mc.entryList) {
                var key:String = String(entry.text);
                if (B21_PreviewStrings.values[key]) entry.text = B21_PreviewStrings.values[key];
            }
            MainPanel_mc.List_mc.InvalidateData();
        }
        private function B21FitBackdrop(event:Event):void {
            if (!b21PhotoFixture || !b21PhotoFixture.content) return;
            var scale:Number = Math.max(stage.stageWidth / b21PhotoFixture.contentLoaderInfo.width,
                stage.stageHeight / b21PhotoFixture.contentLoaderInfo.height);
            b21PhotoFixture.content.scaleX = b21PhotoFixture.content.scaleY = scale;
            b21PhotoFixture.x = (stage.stageWidth - b21PhotoFixture.content.width) / 2;
            b21PhotoFixture.y = (stage.stageHeight - b21PhotoFixture.content.height) / 2;
        }
        public function B21PreviewMode(paused:Boolean, saved:Boolean):void {
            PauseMode = paused;
            b21HasSaves = saved;
            InitList(__B21_INIT_LIST_ARGS__);
            B21TranslateEntries();
        }
        public function B21PreviewPress(index:int):void {
            MainPanel_mc.List_mc.selectedIndex = index;
            MainPanel_mc.List_mc.dispatchEvent(new Event(BSScrollingList.ITEM_PRESS, true));
        }
        public function B21PreviewFault(broken:Boolean):void {
            if (broken) MainPanel_mc.List_mc.entryList.push(null);
            else MainPanel_mc.List_mc.entryList.pop();
        }
        public function B21PreviewState():Object {
            var bounds:Array = [];
            if (b21Rows) for (var i:int = 0; i < b21Rows.numChildren; i++) {
                var clip:Object = b21Rows.getChildAt(i);
                var box:Object = clip.Text_mc.getBounds(this);
                bounds.push({x:box.x,y:box.y,width:box.width,height:box.height});
            }
            return {state:currentState, entries:MainPanel_mc.List_mc.entryList,
                selected:MainPanel_mc.List_mc.selectedIndex, visible:MainPanel_mc.visible,
                alpha:MainPanel_mc.alpha, children:numChildren, rootVisible:visible, rootAlpha:alpha,
                photoLaunches:b21PhotoLaunches,continues:b21Continues,rowBounds:bounds,
                version:B21MainHostVersion, logoAlpha:BethesdaLogo_mc.alpha,
                rows:b21Rows ? {count:b21Rows.numChildren,visible:b21Rows.visible,x:b21Rows.x,y:b21Rows.y,width:b21Rows.width,height:b21Rows.height} : null,
                parent:{name:parent.name,visible:parent.visible,alpha:parent.alpha}};
        }
    }
}
