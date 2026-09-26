package {
    import flash.display.MovieClip;
    import flash.display.DisplayObject;
    import flash.display.Sprite;
    import flash.text.TextField;
    import flash.events.Event;
    import flash.external.ExternalInterface;
    import flash.geom.ColorTransform;

    public class LooksMenu extends MovieClip {
        private var b21PreviewCalls:Array;
        private var b21PreviewPreset:int = 0;
        private var b21PreviewIntensity:Number = 0.5;

        public function B21PreviewInit():void {
            addEventListener(Event.ENTER_FRAME, B21PreviewFrame);
        }

        private function B21PreviewFrame(event:Event):void {
            removeEventListener(Event.ENTER_FRAME, B21PreviewFrame);
            b21PreviewCalls = [];
            b21PreviewPreset = 0;
            b21PreviewIntensity = 0.5;
            BGSCodeObj = {GetFeatureData:B21PreviewFeatures, GetDetailIntensity:B21PreviewGetIntensity,
                SetDetailIntensity:B21PreviewSetIntensity, GetLastCharacterPreset:B21PreviewPreset,
                ChangeCharacterPreset:B21PreviewChangePreset, ChangeSex:B21PreviewNoop,
                GetHasDetailsApplied:B21PreviewFalse, SetSculptMode:B21PreviewNoop,
                SetFeatureMode:B21PreviewNoop, SetBumpersRepeat:B21PreviewNoop,
                ClearBoneRegionTint:B21PreviewNoop, SetBoneRegionTint:B21PreviewNoop,
                ClearPickData:B21PreviewNoop, CreateUndoPoint:B21PreviewNoop,
                SetHairHighlight:B21PreviewNoop,
                StartBodyEdit:B21PreviewTrue, EndBodyEdit:B21PreviewNoop,
                GetExtraGroupName:B21PreviewGroup, GetDetailColorCount:B21PreviewZero,
                GetDetailColor:B21PreviewZero, ConfirmAndCloseMenu:B21PreviewTrue,
                NotifyForWittyBanter:B21PreviewBanter, PlaySound:B21PreviewNoop};
            B21CharacterAction = B21PreviewAction;
            B21CharacterLabels = {"$B21_TFA_CharacterName":"CHARACTER NAME",
                "$B21_TFA_CharacterNameAccept":"CONFIRM", "$B21_TFA_CharacterNameCancel":"BACK"};
            FacialBoneRegions = [[{regionID:7, name:"Nose", headPart:0, isSlider:false,
                presetIntensity:0.5, axisArray:[{axis:0, isSlider:true, axisSliderValue:0.5}]}],
                [{regionID:9, name:"Eyes", headPart:2, isSlider:false, presetIntensity:0.5, axisArray:[]}]];
            characterPresetCount = 15;
            currentActor = 0;
            editMode = 0;
            enableControls = true;
            SetPlatform(0, false);
            stage.stageFocusRect = false;
            focusRect = false;
            tabChildren = false;
            stage.focus = this;
            ExternalInterface.addCallback("creatorCall", B21PreviewCall);
            ExternalInterface.addCallback("creatorState", B21PreviewState);
            ExternalInterface.addCallback("creatorSet", B21PreviewSet);
            ExternalInterface.addCallback("creatorPlatform", B21PreviewPlatform);
            ExternalInterface.addCallback("creatorIntensity", B21PreviewIntensity);
            ExternalInterface.addCallback("creatorTree", B21PreviewTree);
            ExternalInterface.addCallback("creatorNativePath", B21PreviewNativePath);
            ExternalInterface.addCallback("creatorName", B21PreviewName);
            ExternalInterface.addCallback("creatorNativeTint", B21PreviewNativeTint);
            ExternalInterface.addCallback("creatorDestroy", B21PreviewDestroy);
            ExternalInterface.addCallback("creatorAcquireHints", B21PreviewAcquireHints);
            ExternalInterface.call("report", B21PreviewState());
        }

        public function B21PreviewFeatures(entries:Array, category:uint, region:uint):uint {
            entries.length = 0;
            entries.push({text:"Option 1", applied:true}, {text:"Option 2", applied:false});
            b21PreviewCalls.push({method:"GetFeatureData", category:category, region:region});
            return 0;
        }
        public function B21PreviewGetIntensity(group:uint, index:uint):Number { return b21PreviewIntensity; }
        public function B21PreviewSetIntensity(...args):void { b21PreviewIntensity = args[2]; }
        public function B21PreviewPreset():int { return b21PreviewPreset; }
        public function B21PreviewChangePreset(previous:Boolean):void {
            b21PreviewPreset = (b21PreviewPreset + (previous ? 14 : 1)) % 15;
            b21PreviewCalls.push({method:"ChangeCharacterPreset", previous:previous});
            onCommitCharacterPresetChange(b21PreviewPreset);
        }
        public function B21PreviewAction(action:String, value:* = null):Boolean {
            b21PreviewCalls.push({method:action, value:value});
            if (action == "sex") ToggleSex();
            return true;
        }
        public function B21PreviewName(value:String):void { b21NameField.text = value; }
        public function B21PreviewDestroy():void {
            var root:Object = this;
            if ("onCodeObjDestruction" in root) root.onCodeObjDestruction();
            BGSCodeObj = null;
        }
        public function B21PreviewAcquireHints():Boolean {
            ButtonHintBar_mc.onAcquiredByNativeCode();
            return ButtonHintBar_mc.visible;
        }
        public function B21PreviewNativeTint():Boolean {
            var nested:Sprite = new Sprite();
            var label:TextField = new TextField();
            label.text = "Nested hint";
            label.textColor = 0;
            nested.addChild(label);
            ButtonHintBar_mc.addChild(nested);
            nested.transform.colorTransform = new ColorTransform(0, 0, 0, 0.4);
            label.transform.colorTransform = new ColorTransform(0, 0, 0, 0.6);
            ButtonHintBar_mc.transform.colorTransform = new ColorTransform(0, 0, 0, 0.5);
            B21SourceColors();
            var color:ColorTransform = ButtonHintBar_mc.transform.colorTransform;
            var restored:Boolean = color.redMultiplier == 1 && color.greenMultiplier == 1 &&
                color.blueMultiplier == 1 && color.alphaMultiplier == 0.5 &&
                nested.transform.colorTransform.redMultiplier == 1 &&
                Math.abs(nested.transform.colorTransform.alphaMultiplier - 0.4) < 0.01 &&
                label.transform.colorTransform.redMultiplier == 1 &&
                Math.abs(label.transform.colorTransform.alphaMultiplier - 0.6) < 0.01 && label.textColor == 0xFFFFCB;
            ButtonHintBar_mc.removeChild(nested);
            color.alphaMultiplier = 1;
            ButtonHintBar_mc.transform.colorTransform = color;
            return restored;
        }
        public function B21PreviewBanter(...args):void { b21PreviewCalls.push({method:"banter"}); }
        public function B21PreviewGroup(...args):String { return "Markings"; }
        public function B21PreviewNoop(...args):void {}
        public function B21PreviewFalse(...args):Boolean { return false; }
        public function B21PreviewTrue(...args):Boolean { return true; }
        public function B21PreviewZero(...args):uint { return 0; }
        public function B21PreviewSelectionHandler():Function { return B21PreviewSelection; }
        private function B21PreviewSelection(event:Event):void { onFeatureSelectionChange(); }
        public function B21PreviewFocusHandler():Function { return B21PreviewFocus; }
        private function B21PreviewFocus(event:Event):void { onListPlayFocus(); }
        public function B21PreviewCall(name:String, args:Array):* {
            var methods:Object = {CharacterPresetRight:CharacterPresetRight, CharacterPresetLeft:CharacterPresetLeft,
                ChangeSex:ChangeSex, FaceMode:FaceMode, SculptMode:SculptMode, StartMode:StartMode,
                BodyMode:BodyMode, ExtrasMode:ExtrasMode, FeatureMode:FeatureMode,
                ConfirmCloseMenu:ConfirmCloseMenu, ProcessUserEvent:ProcessUserEvent, B21AcceptName:B21AcceptName};
            return methods[name].apply(this, args);
        }
        public function B21PreviewSet(name:String, value:*):void { var target:Object = this; target[name] = value; }
        public function B21PreviewPlatform(value:uint):void { SetPlatform(value, false); }
        public function B21PreviewInputFunctions():Array {
            var result:Array = InputFunctionsA.concat();
            var start:Array = StartModeFunctionsReleased.concat();
            start[5] = B21PreviewLeft;
            start[6] = B21PreviewRight;
            result[START_MODE] = start;
            return result;
        }
        private function B21PreviewLeft(released:Boolean):void { CharacterPresetLeft(); }
        private function B21PreviewRight(released:Boolean):void { CharacterPresetRight(); }
        public function B21PreviewIntensity(value:Number):Number {
            B21Native().SetDetailIntensity(0, 0, value, true);
            return B21Native().GetDetailIntensity(0, 0);
        }
        public function B21PreviewState():Object {
            var labels:Array = [];
            if (b21NamePanel != null) for (var i:int = 0; i < b21NamePanel.numChildren; i++) {
                var label:TextField = b21NamePanel.getChildAt(i) as TextField;
                if (label != null) labels.push({text:label.text, color:label.textColor,
                    font:label.getTextFormat().font, size:label.getTextFormat().size});
            }
            return {version:B21CharacterCreationVersion, hidden:UIHidden, platform:uiPlatform, mode:eMode,
                feature:eFeature, preset:b21PreviewPreset, female:_bisFemale, intensity:b21PreviewIntensity,
                bone:GetBoneRegionIndexFromCurrentID(), regions:B21BoneRegions(),
                owned:!MustPurchase, entries:FeaturePanel_mc.List_mc.entryList, calls:b21PreviewCalls,
                start:START_MODE, face:FACE_MODE, body:BODY_MODE, sculpt:SCULPT_MODE,
                categories:FEATURE_CATEGORY_MODE, features:FEATURE_MODE, extras:AST_EXTRAS,
                hair:AST_HAIR, morphs:AST_MORPHS, closed:confirmClose && b21NamePanel == null,
                nameOpen:b21NamePanel != null, name:b21Name, nameLabels:labels,
                nativeCallbacks:[b21Features, b21Intensity, b21SetIntensity, b21Confirm].filter(B21PreviewHasCallback).length,
                nativeAction:B21CharacterAction != null,
                redirectsHints:Object(ButtonHintBar_mc)["bRedirectToButtonBarMenu"],
                redirectsSourceHints:ButtonHintBar_mc.bRedirectToButtonBarMenu_Inspectable,
                nameInput:b21NameField != null ? b21NameField.text : ""};
        }
        private function B21PreviewHasCallback(value:*, index:int, list:Array):Boolean { return value != null; }
        public function B21PreviewNativePath(path:String):Object {
            var value:Object = this;
            for each (var part:String in path.split(".")) {
                if (value == null || !(part in value)) return {display:false};
                value = value[part];
            }
            return {display:value is DisplayObject, width:value.width, height:value.height,
                featureBounds:value == FeaturePanel_mc.List_mc.border};
        }
        public function B21PreviewTree():Object {
            var items:Array = [];
            for (var i:int=0; i<numChildren; i++) {
                var c:Object=getChildAt(i);
                items.push({name:c.name,x:c.x,y:c.y,width:c.width,height:c.height,alpha:c.alpha,visible:c.visible});
            }
            var hints:Array=[];
            for each (var h:Object in _buttonHintDataV) hints.push({text:h.ButtonText,visible:h.ButtonVisible});
            return {x:x,y:y,width:width,height:height,alpha:alpha,visible:visible,children:items,hints:hints};
        }
    }
}
