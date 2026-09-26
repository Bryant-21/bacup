package {
    import flash.display.MovieClip;
    import flash.events.MouseEvent;
    public class SelfieMenu extends Shared.AS3.IMenu {
        public function get B21PhotoModeVersion():uint { return 2; }
        override public function SetPlatform(platform:uint, ps3Switch:Boolean, controller:uint = 0, keyboard:uint = 0):* {
            B21SetFO4Platform(platform);
        }
        public function B21Start(code:Object):void {
            BGSCodeObj = code;
            focusRect = false;
            tabChildren = false;
            Initialize();
            AddStepper(Panel2_Cinematic_mc, "Freeze Time", "$B21_PM_FreezeTime");
            SetControlActive(false, "Expression", "Player");
            SetControlActive(false, "Vanity Light Style", "Player");
            SetControlActive(false, "Vanity Light Strength", "Player");
            SetControlActive(false, "Texture Category", "Filters");
            SetControlActive(false, "Texture", "Filters");
            for each (var panel:Object in Panels) panel.B21BindMouse();
            for (var i:int = 0; i < Panels.length; i++)
                PanelBackground_mc.TabSelector_mc.getChildByName("Icon" + i + "_mc").addEventListener(MouseEvent.CLICK, B21MouseTab);
        }
        private function B21MouseTab(event:MouseEvent):void { SetPanel(int(event.currentTarget.name.charAt(4))); }
        public function B21Snapshot():void { BGSCodeObj.Action("capture"); }
        public function B21Exit():void { BGSCodeObj.Action("exit"); }
        public function B21Toggle():void { BGSCodeObj.Action("toggle"); }
        public function B21SnapshotHandler():Function { return B21Snapshot; }
        public function B21ExitHandler():Function { return B21Exit; }
        public function B21ToggleHandler():Function { return B21Toggle; }
        public function B21BindHints(events:Array):void {
            var hints:Array = [buttonHint_Snapshot, buttonHint_Cancel, buttonHint_Toggle,
                buttonHint_Reset, buttonHint_Prev, buttonHint_Next];
            for (var i:uint = 0; i < hints.length; i++) {
                hints[i].SetButtons("", hints[i].PSNButton, hints[i].XenonButton);
                hints[i].userEventMapping = events[i];
            }
        }
    }
}
