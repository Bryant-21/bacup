package {
    import flash.display.MovieClip;
    import flash.display.DisplayObjectContainer;
    import flash.events.Event;
    import flash.external.ExternalInterface;
    import flash.text.TextField;
    public class SelfieMenu extends MovieClip {
        private var b21PreviewStarted:Boolean = false;
        private var b21Actions:Array = [];
        private var b21Changes:Array = [];
        public function B21PreviewInit():void {
            b21Actions = [];
            b21Changes = [];
            addEventListener(Event.ENTER_FRAME, B21PreviewFrame);
        }
        private function B21PreviewFrame(event:Event):void {
            if (b21PreviewStarted) return;
            b21PreviewStarted = true;
            removeEventListener(Event.ENTER_FRAME, B21PreviewFrame);
            B21Start({GetDefaultSliderMin:B21Min, GetDefaultSliderMax:B21Max,
                GetDefaultSliderValue:B21Default, GetSliderSpeed:B21Speed, ResetSliderValue:B21Default,
                FillStepper:B21Fill, OnSliderChanged:B21Changed, OnStepperChanged:B21Changed,
                Action:B21Action, PlayMenuSound:B21Sound, RegisterImage:B21Sound, UnregisterImage:B21Sound});
            ExternalInterface.addCallback("photoTest", B21PreviewTest);
            ExternalInterface.addCallback("photoPanel", SetPanel);
            ExternalInterface.addCallback("photoState", B21PreviewState);
            ExternalInterface.addCallback("photoInput", B21PreviewInput);
            ExternalInterface.addCallback("photoPlatform", B21PreviewPlatform);
            ExternalInterface.addCallback("photoHints", B21PreviewHints);
            ExternalInterface.addCallback("photoMembers", B21PreviewMembers);
            addEventListener(Event.ENTER_FRAME, B21PreviewTranslate);
            SetPlatform(0, false);
            ExternalInterface.call("report", B21PreviewTest());
        }
        public function B21Min(panel:String,name:String):Number {
            return name == "View Roll" ? -180 : name == "Field of View" ? 5 : name == "Camera Speed" ? 0.1 : 0;
        }
        public function B21Max(panel:String,name:String):Number {
            return name == "View Roll" ? 180 : name == "Field of View" ? 150 : name == "Camera Speed" ? 50 :
                name == "Distance" || name == "Range" ? 5000 : name == "Strength" ? 1 : 3;
        }
        public function B21Default(panel:String,name:String):Number {
            return name == "Field of View" ? 80 : name == "View Roll" ? 0 :
                name == "Distance" || name == "Range" ? 500 : name == "Strength" ? 0.5 : 1;
        }
        public function B21Speed():Number { return 1; }
        public function B21Fill(control:Object,name:String):void {
            var options:Array = B21_PhotoStrings.options[name] || ["$NONE"];
            for each (var option:String in options) control.addOption(B21_PhotoStrings.values[option] || option);
            var selected:String = options[name == "Show Player" || name == "Freeze Time" ? 1 : 0];
            control.setDefault(B21_PhotoStrings.values[selected] || selected);
        }
        public function B21Changed(panel:String,name:String,value:Number):void { b21Changes.push({panel:panel,name:name,value:value}); }
        public function B21Action(action:String):void { b21Actions.push(action); ExternalInterface.call("action",action); }
        public function B21Sound(name:String):void {}
        public function B21PreviewPlatform(platform:uint):void { SetPlatform(platform, false); }
        // Mirrors PhotoMode.cpp PublishControlMap: the FO76 engine's ControlMapData, which the hints subscribe to
        // through userEventMapping. Public dynamic lookups only, as native GetMember/Invoke. Returns the drawn text.
        public function B21PreviewHints(events:Array, keys:Array):Array {
            B21BindHints(events);
            var domain:Object = this.loaderInfo.applicationDomain;
            var manager:Object = domain.getDefinition("Shared.AS3.Data.BSUIDataManager");
            var platforms:Object = domain.getDefinition("Shared.AS3.Events.PlatformChangeEvent");
            var provider:Object = manager.GetDataFromClient("ControlMapData");
            var mappings:Array = [];
            for (var i:uint = 0; i < events.length; i++) mappings.push({userEventName:events[i], buttonName:keys[i]});
            provider.data.buttonMappings = mappings;
            provider.data.uiController = platforms.PLATFORM_PC_KB_MOUSE;
            provider.DispatchChange();
            var text:Array = [];
            B21CollectText(ButtonBackground_mc, text);
            return text;
        }
        // What native GetMember sees: a public dynamic lookup on the menu object.
        public function B21PreviewMembers(names:Array):Object {
            var found:Object = {};
            var own:Array = [buttonHint_Snapshot, buttonHint_Cancel, buttonHint_Toggle, buttonHint_Reset, buttonHint_Prev, buttonHint_Next];
            var userEvents:Array = [];
            for each (var data:Object in own) userEvents.push(data.UserEvent + "|" + data.userEventMapping + "|" + data.ButtonText);
            found["userEvents"] = userEvents;
            for each (var name:String in names) {
                try { found[name] = this[name] != null; } catch (error:Error) { found[name] = "error " + error.errorID; }
            }
            return found;
        }
        private function B21CollectText(container:DisplayObjectContainer, text:Array):void {
            for (var i:int = 0; i < container.numChildren; i++) {
                var child:Object = container.getChildAt(i);
                if (child is TextField && child.visible && child.text) text.push(child.text);
                else if (child is DisplayObjectContainer) B21CollectText(DisplayObjectContainer(child), text);
            }
        }
        public function B21PreviewInput(action:String):void {
            var methods:Object = {up:"OnUp", down:"OnDown", left:"OnLeft", right:"OnRight",
                previous:"OnLShoulder", next:"OnRShoulder", reset:"OnReset", hide:"HideMenu", show:"ShowMenu"};
            // Native GFx Invoke resolves public methods by name, without SelfieMenu's lexical scope.
            var target:Object = this;
            var callback:Function = target[methods[action]];
            callback.call(target);
        }
        public function B21PreviewState():Object {
            var state:Object = Panels[SelectedPanel].B21PreviewControls(this);
            var tabs:Array = [];
            for (var i:int = 0; i < Panels.length; i++) {
                var bounds:Object = PanelBackground_mc.TabSelector_mc.getChildByName("Icon" + i + "_mc").getBounds(this);
                tabs.push({x:bounds.x,y:bounds.y,width:bounds.width,height:bounds.height});
            }
            return {panel:SelectedPanel,selected:state.selected,controls:state.controls,
                version:B21PhotoModeVersion,visible:MenuVisible,platform:uiPlatform,changes:b21Changes,actions:b21Actions,tabs:tabs};
        }
        private function B21PreviewTranslate(event:Event):void { B21TranslateTree(this); }
        private function B21TranslateTree(container:DisplayObjectContainer):void {
            for (var i:int = 0; i < container.numChildren; i++) {
                var child:Object = container.getChildAt(i);
                if (child is TextField && B21_PhotoStrings.values[child.text]) child.text = B21_PhotoStrings.values[child.text];
                else if (child is DisplayObjectContainer) B21TranslateTree(child);
            }
        }
        public function B21PreviewTest():Object {
            var checks:uint = 0;
            if (Panels.length == 5) checks++;
            if (SelectedPanel == 0) checks++;
            SetPanel(1); if (SelectedPanel == 1) checks++;
            SetPanel(4); if (SelectedPanel == 4) checks++;
            B21Snapshot(); B21Toggle(); B21Exit();
            if (b21Actions[b21Actions.length - 3] == "capture" &&
                b21Actions[b21Actions.length - 2] == "toggle" &&
                b21Actions[b21Actions.length - 1] == "exit") checks++;
            SetPanel(0);
            var before:int = b21Changes.length;
            OnRight();
            if (b21Changes.length > before && b21Changes[b21Changes.length - 1].name == "Field of View") checks++;
            SetPanel(1);
            before = b21Changes.length;
            OnRight();
            if (b21Changes.length > before && b21Changes[b21Changes.length - 1].name == "Show Player") checks++;
            SetPanel(0);
            return {passed: checks, expected: 7, panels: Panels.length, changes: b21Changes};
        }
    }
}
