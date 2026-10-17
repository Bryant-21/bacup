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
    import flash.geom.ColorTransform;
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
                    SetInputMappingMode:B21Nothing,SaveSettings:onSettingsSaved,
                    StartState:B21Nothing,EndState:B21Nothing,UpdateInputContext:B21Nothing,
                    RegisterSaveLoadPanel:B21Nothing,InitialPopulateLoadList:B21Nothing,SetBackgroundVisible:B21Nothing};
                PauseMode = false;
                b21HasSaves = b21LauncherAvailable = true;
                InitMenu();
                // Ruffle enforces the Event argument; Scaleform accepts the stock zero-argument listener.
                removeEventListener(BSScrollingList.PLAY_FOCUS_SOUND, onListPlayFocus);
                addEventListener(BSScrollingList.PLAY_FOCUS_SOUND, B21PreviewFocus);
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
                ExternalInterface.addCallback("menuInspect", B21PreviewInspect);
                ExternalInterface.addCallback("menuColor", B21PreviewColor);
                ExternalInterface.addCallback("menuBack", B21PreviewBack);
                ExternalInterface.addCallback("menuDefaults", ResetSettingsToDefaults);
                ExternalInterface.addCallback("menuRejectSave", B21PreviewRejectSave);
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
            BGSCodeObj.onSettingsValueChange = B21PreviewNativeSetting;
            BGSCodeObj.onPipboyColorUpdate = B21PreviewPipboyColor;
            BGSCodeObj.onHUDColorUpdate = B21Nothing;
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
        private function B21PreviewFocus(event:Event):void { onListPlayFocus(); }
        private var b21NativeChanges:int;
        private var b21PipboyUpdates:int;
        private var b21RejectSave:Boolean;
        private var b21PreviewFeatures:Array;
        private function B21PreviewRejectSave():void { b21RejectSave = true; }
        public function B21PreviewBack():void {
            onCancelPress();
            if (bSavingSettings) SaveSettings();
        }
        private function B21PreviewNativeSetting(id:Object, value:Object):void { b21NativeChanges++; }
        private function B21PreviewPipboyColor():void {
            b21PipboyUpdates++;
            var rgb:Array = [];
            for each (var row:Object in OptionsPanel_mc.Fader_mc.List_mc.entryList)
                if (row.pipboyColorUpdate) rgb.push(row.value);
            if (rgb.length == 3) ColorReference_mc.ColorSwatch_mc.transform.colorTransform = new ColorTransform(rgb[0], rgb[1], rgb[2]);
        }
        public function B21PreviewColor(value:Number):void {
            var list:Object = OptionsPanel_mc.Fader_mc.List_mc;
            var clip:Object = list.GetClipByIndex(list.selectedClipIndex);
            clip.value = value;
            clip.dispatchEvent(new Event(SettingsOptionItem.VALUE_CHANGE, true));
        }
        private function B21PreviewBox(clip:Object):Object {
            if (!clip) return null;
            var box:Object = clip.getBounds(this);
            return {x:box.x, y:box.y, width:box.width, height:box.height, visible:clip.visible, alpha:clip.alpha};
        }
        public function B21PreviewInspect():Object {
            var list:Object = OptionsPanel_mc.Fader_mc.List_mc;
            var preview:Object = ColorReference_mc;
            var rgb:Object = preview.ColorSwatch_mc.transform.colorTransform;
            var gaps:Array = [];
            for (var i:int = 1; i < list.itemsShown; i++) {
                var previous:Object = list.GetClipByIndex(i - 1).border.getBounds(this);
                gaps.push(list.GetClipByIndex(i).border.getBounds(this).top - previous.bottom);
            }
            return {state:currentState, pause:PauseMode, main:B21PreviewBox(MainPanel_mc.List_mc),
                categories:B21PreviewBox(SettingsPanel_mc.SettingsList_mc), options:B21PreviewBox(list),
                categoryPanel:B21PreviewBox(SettingsPanel_mc), categoryRow:B21PreviewBox(SettingsPanel_mc.SettingsList_mc.GetClipByIndex(0)),
                rowGaps:gaps,
                optionRows:list.entryList, selected:list.selectedIndex, preview:B21PreviewBox(preview),
                rgb:rgb ? [rgb.redMultiplier, rgb.greenMultiplier, rgb.blueMultiplier] : null,
                nativeChanges:b21NativeChanges, pipboyUpdates:b21PipboyUpdates};
        }
        private function B21PreviewSettingsAction(operation:String, id:Object = null, value:Object = null):Object {
            if (operation == "defaults") b21PreviewFeatures = null;
            if (!b21PreviewFeatures) {
                b21PreviewFeatures = [
                    {text:"Quick-Boy", movieType:2, value:1},
                    {text:"Pip-Boy View", movieType:1, options:["Pip-Boy", "Quick-Boy"], value:1},
                    {text:"Quick-Boy Color Mode", movieType:1, options:["Match Pip-Boy", "Match HUD", "Own Color"], value:1},
                    {text:"Quick-Boy Color R", movieType:0, value:1, b21QuickColor:true},
                    {text:"Quick-Boy Color G", movieType:0, value:181 / 255, b21QuickColor:true},
                    {text:"Quick-Boy Color B", movieType:0, value:66 / 255, b21QuickColor:true},
                    {text:"Quick-Boy Size", movieType:1, options:B21PreviewSteps(0.75, 1.5, 0.05), value:5},
                    {text:"Quick-Boy Brightness", movieType:1, options:B21PreviewSteps(0.5, 3, 0.1), value:10},
                    {text:"Show Damage Numbers", movieType:2, value:1},
                    {text:"Show Weapon Condition", movieType:2, value:1},
                    {text:"Always Show Hunger and Thirst", movieType:2, value:1},
                    {text:"Highlight Corpses", movieType:2, value:1},
                    {text:"Stop Highlighting Corpses", movieType:1, options:["After Taking Loot", "After Inspecting", "When Empty"], value:1},
                    {text:"Only Highlight Your Kills", movieType:2, value:1}];
                for (var i:int = 0; i < b21PreviewFeatures.length; i++) {
                    b21PreviewFeatures[i].ID = 0xB2200 + i;
                    b21PreviewFeatures[i].b21Setting = b21PreviewFeatures[i].b21Feature = true;
                }
            }
            if (operation == "set") {
                if (b21RejectSave) { b21RejectSave = false; return false; }
                b21PreviewChanges.push([id, value]);
                for each (var entry:Object in b21PreviewFeatures) if (entry.ID == id) {
                    entry.value = entry.b21QuickColor ? Math.round(Number(value) * 255) / 255 : value;
                    if (entry.b21QuickColor) b21PreviewFeatures[2].value = 2;
                }
                return true;
            }
            if (operation == "defaults") return true;
            var features:Array = [];
            for each (var savedFeature:Object in b21PreviewFeatures) {
                var copy:Object = {};
                for (var key:String in savedFeature) copy[key] = savedFeature[key];
                features.push(copy);
            }
            if (operation == "features") return features;
            var degrees:Array = [];
            for (var d:int = 60; d <= 130; d += 5) degrees.push(String(d));
            return [{text:"Field of View", movieType:1, options:degrees, ID:0xB2100, value:6, b21Setting:true},
                {text:"First-Person Field of View", movieType:1, options:degrees, ID:0xB2101, value:4, b21Setting:true},
                {text:"Depth of Field", movieType:2, ID:0xB2102, value:1, b21Setting:true},
                {text:"Shadow Distance", movieType:1, options:["Low", "Medium", "High", "Ultra"], ID:0xB2105, value:2, b21Setting:true},
                {text:"Window Mode (Restart)", movieType:1, options:["Fullscreen", "Borderless", "Windowed"], ID:0xB2108, value:1, b21Setting:true}].concat(features);
        }
        private function B21PreviewSteps(low:Number, high:Number, step:Number):Array {
            var values:Array = [];
            for (var i:int = 0; i <= Math.round((high - low) / step); i++) values.push(String(Math.round((low + i * step) * 100) / 100) + "x");
            return values;
        }
        private function B21PreviewOptions(options:Array):void {
            for (var i:int = 0; i < options.length; i++) {
                var option:Object = options[i];
                if (String(option.text).indexOf("$Settings_FrameCap") == 0 || option.text == "$Settings_VisualFidelity") continue;
                option.ID = i;
                option.value = option.movieType == 0 ? 0.6 : option.movieType == 2 ? i % 2 : 0;
                option.pipboyColorUpdate = String(option.text).indexOf("$PipboyColor") == 0;
                option.hudColorUpdate = String(option.text).indexOf("$HUDColor") == 0;
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
