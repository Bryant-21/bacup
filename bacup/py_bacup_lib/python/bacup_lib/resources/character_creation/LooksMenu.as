package {
    import Shared.AS3.IMenu;
    import Shared.GlobalFunc;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.text.TextFieldType;
    import flash.geom.ColorTransform;

    public class LooksMenu extends IMenu {
        private var b21CurrentActor:uint = 0;
        private var b21NativeReady:Boolean = false;
        private var b21Features:Function;
        private var b21Intensity:Function;
        private var b21SetIntensity:Function;
        private var b21Confirm:Function;
        private var b21NamePanel:Sprite;
        private var b21NameField:TextField;
        private var b21Name:String = "";
        private var b21NameEventConsumed:Boolean = false;
        public var B21CharacterAction:Function;
        public var B21CharacterLabels:Object = {};

        public function get B21CharacterCreationVersion():uint { return 3; }

        public function B21InitializeFO4():void {
            // FO4 reads the unsuffixed flag before acquiring the source button bar.
            Object(ButtonHintBar_mc)["bRedirectToButtonBarMenu"] = false;
            ButtonHintBar_mc.bRedirectToButtonBarMenu_Inspectable = false;
            // FO4's LooksMenu constructor requires this path before it can initialize.
            FeaturePanel_mc.Brackets_mc = {BracketExtents_mc:FeaturePanel_mc.List_mc.border};
            // FO4 controls root visibility itself and never sends FO76's ShowUI callback.
            ShowUI();
            addEventListener(Event.ENTER_FRAME, B21StyleFrame);
        }

        override public function SetPlatform(platform:uint, ps3Switch:Boolean, controller:uint = 0, keyboard:uint = 0):* {
            B21SetFO4Platform(platform);
        }

        public function set currentActor(value:uint):void {
            b21CurrentActor = value;
            UpdateButtons();
        }

        public function B21BoneRegions():Array {
            return FacialBoneRegions[b21CurrentActor] as Array || [];
        }

        public function B21Native():Object {
            if (!b21NativeReady && BGSCodeObj.GetFeatureData is Function) {
                b21NativeReady = true;
                b21Features = BGSCodeObj.GetFeatureData;
                b21Intensity = BGSCodeObj.GetDetailIntensity;
                b21SetIntensity = BGSCodeObj.SetDetailIntensity;
                b21Confirm = BGSCodeObj.ConfirmAndCloseMenu;
                BGSCodeObj.GetFeatureData = B21Features;
                BGSCodeObj.GetDetailIntensity = B21Intensity;
                BGSCodeObj.SetDetailIntensity = B21SetIntensity;
                BGSCodeObj.NotifyForWittyBanter = B21Banter;
                BGSCodeObj["ChangeSex"] = B21ChangeSex;
                BGSCodeObj.ConfirmAndCloseMenu = B21AskName;
            }
            return BGSCodeObj;
        }

        public function onCodeObjDestruction():void {
            B21CancelName();
            removeEventListener(Event.ENTER_FRAME, B21StyleFrame);
            // These function values retain FO4's LooksMenu and its input contexts.
            b21Features = null;
            b21Intensity = null;
            b21SetIntensity = null;
            b21Confirm = null;
            B21CharacterAction = null;
            b21NativeReady = true;
        }

        private function B21Features(entries:Array, category:uint, region:uint):* {
            var selected:* = b21Features.apply(BGSCodeObj, [entries, category, region]);
            for each (var entry:Object in entries) entry.owned = true;
            return selected;
        }

        private function B21Intensity(group:uint, index:uint):Number {
            return Number(b21Intensity.apply(BGSCodeObj, [group, index])) * 100;
        }

        private function B21SetIntensity(...args):void {
            args[2] = Number(args[2]) / 100;
            b21SetIntensity.apply(BGSCodeObj, args);
        }

        private function B21Banter(flavor:uint):void {}

        private function B21ChangeSex():void {
            if (B21CharacterAction != null) B21CharacterAction("sex");
        }

        private function B21StyleFrame(event:Event):void {
            if (B21CharacterAction != null) B21SourceColors();
        }

        public function B21SourceColors():void {
            B21ClearFilters(this);
            B21ClearFilters(ButtonHintBar_mc);
            B21ClearFilters(FeaturePanel_mc.Brackets_mc.BracketExtents_mc);
            B21HintColors(ButtonHintBar_mc);
        }

        private function B21ClearFilters(item:DisplayObject):void {
            if (item == null || item.transform == null) return;
            item.filters = [];
            var color:ColorTransform = item.transform.colorTransform;
            if (color == null) return;
            color.redMultiplier = color.greenMultiplier = color.blueMultiplier = 1;
            color.redOffset = color.greenOffset = color.blueOffset = 0;
            item.transform.colorTransform = color;
        }

        private function B21HintColors(group:DisplayObjectContainer):void {
            for (var i:int = 0; i < group.numChildren; i++) {
                var child:DisplayObject = group.getChildAt(i);
                B21ClearFilters(child);
                if (child is TextField) TextField(child).textColor = 0xFFFFCB;
                else if (child is DisplayObjectContainer) B21HintColors(DisplayObjectContainer(child));
            }
        }

        private function B21Label(text:String, x:Number, y:Number, width:Number, size:uint):TextField {
            var label:TextField = new TextField();
            var format:TextFormat = new TextFormat("$MAIN_Font", size, 0xFFFFCB);
            label.defaultTextFormat = format;
            label.x = x; label.y = y; label.width = width; label.height = 55;
            label.selectable = false;
            GlobalFunc.SetText(label, B21CharacterLabels[text] || text, false);
            label.setTextFormat(format);
            b21NamePanel.addChild(label);
            return label;
        }

        private function B21AskName():Boolean {
            if (b21NamePanel != null || B21CharacterAction == null) return false;
            confirmClose = true;
            b21NamePanel = new Sprite();
            b21NamePanel.graphics.beginFill(0, 0.75);
            b21NamePanel.graphics.drawRect(0, 0, 1920, 1080);
            b21NamePanel.graphics.endFill();
            b21NamePanel.graphics.beginFill(0x20262B);
            b21NamePanel.graphics.lineStyle(2, 0xFFFFCB);
            b21NamePanel.graphics.drawRect(510, 340, 900, 350);
            b21NamePanel.graphics.endFill();
            addChild(b21NamePanel);
            B21Label("$B21_TFA_CharacterName", 555, 375, 810, 36);
            b21NameField = B21Label(b21Name, 555, 450, 810, 32);
            b21NameField.type = TextFieldType.INPUT;
            b21NameField.selectable = true;
            b21NameField.maxChars = 40;
            b21NameField.background = true;
            b21NameField.backgroundColor = 0x0F1418;
            b21NameField.border = true;
            b21NameField.borderColor = 0xFFFFCB;
            var accept:TextField = B21Label("$B21_TFA_CharacterNameAccept", 555, 575, 405, 28);
            var cancel:TextField = B21Label("$B21_TFA_CharacterNameCancel", 990, 575, 360, 28);
            accept.addEventListener(MouseEvent.CLICK, B21NameAcceptClick);
            cancel.addEventListener(MouseEvent.CLICK, B21NameCancelClick);
            stage.addEventListener(KeyboardEvent.KEY_DOWN, B21NameKey, true, 100);
            B21CharacterAction("textEntry", true);
            stage.focus = b21NameField;
            b21NameField.setSelection(0, b21NameField.length);
            return false;
        }

        private function B21NameKey(event:KeyboardEvent):void {
            if (b21NamePanel == null) return;
            event.stopImmediatePropagation();
            if (event.keyCode == 13) { event.preventDefault(); B21AcceptName(); }
            else if (event.keyCode == 27) { event.preventDefault(); B21CancelName(); }
        }

        private function B21NameAcceptClick(event:MouseEvent):void { B21AcceptName(); }
        private function B21NameCancelClick(event:MouseEvent):void { B21CancelName(); }

        public function B21FilterNameInput(action:String, pressed:Boolean):Boolean {
            b21NameEventConsumed = b21NamePanel != null;
            if (b21NameEventConsumed && !pressed) {
                if (action == "Accept") B21AcceptName();
                else if (action == "Cancel") B21CancelName();
            }
            return UIHidden;
        }

        public function B21InputFunctions(action:String, pressed:Boolean):Array {
            if (!b21NameEventConsumed) return InputFunctionsA;
            var blocked:Array = InputFunctionsA.concat();
            blocked[eMode] = [];
            return blocked;
        }

        public function B21AcceptName():void {
            if (b21NamePanel == null) return;
            var name:String = b21NameField.text;
            while (name.length && name.charCodeAt(0) <= 32) name = name.substr(1);
            while (name.length && name.charCodeAt(name.length - 1) <= 32) name = name.substr(0, name.length - 1);
            if (!name.length || !B21CharacterAction("name", name)) return;
            b21Name = name;
            B21CancelName();
            confirmClose = Boolean(b21Confirm.apply(BGSCodeObj, []));
        }

        private function B21CancelName():void {
            if (b21NamePanel == null) return;
            stage.removeEventListener(KeyboardEvent.KEY_DOWN, B21NameKey, true);
            B21CharacterAction("textEntry", false);
            removeChild(b21NamePanel);
            b21NamePanel = null;
            b21NameField = null;
            confirmClose = false;
            stage.focus = this;
        }
    }
}
