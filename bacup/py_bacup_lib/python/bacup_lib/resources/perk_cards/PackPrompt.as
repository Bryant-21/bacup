package {
    import Shared.AS3.IMenu;
    import Shared.AS3.Styles.MessageBoxButtonListStyle;

    public class MessageBoxMenu extends IMenu {
        private var B21Answer:Function;

        public function B21ListStyle():Class {
            // Class renaming leaves string-based renderer lookups unchanged.
            MessageBoxButtonListStyle.listEntryClass_Inspectable = "B21PerkPackPrompt_MessageBoxButtonEntry";
            return MessageBoxButtonListStyle;
        }

        public function B21Show(answer:Function, gamepad:Boolean):void {
            B21Answer = answer;
            BGSCodeObj = {onButtonPress: B21Choose, onBackButton: B21Back};
            this.SetPlatform(gamepad ? 1 : 0, false, gamepad ? 1 : 0, 0);
            if (this.List_mc.numListItems_Inspectable == 0) {
                this.List_mc.listEntryClass_Inspectable = "B21PerkPackPrompt_MessageBoxButtonEntry";
                this.List_mc.numListItems_Inspectable = 4;
                this.List_mc.onComponentInit(null);
            }
            this.bodyText = "$B21_TFA_Perks_OpenPackPrompt";
            this.buttonArray = [{text: "$YES", buttonIndex: 0}, {text: "$NO", buttonIndex: 1},
                {text: "$B21_TFA_Perks_NoPrompt", buttonIndex: 2}];
            this.InvalidateMenu();
            this.List_mc.selectedIndex = 1;
            focusRect = false;
            this.List_mc.focusRect = false;
            stage.stageFocusRect = false;
        }

        public function B21Input(action:String, pressed:Boolean):Boolean {
            if (pressed) return true;
            if (action == "Up" || action == "Down") {
                this.List_mc.selectedIndex = (this.List_mc.selectedIndex + (action == "Down" ? 1 : 2)) % 3;
            } else if (action == "Accept") B21Choose(this.List_mc.selectedIndex);
            else if (action == "Cancel") B21Back();
            return true;
        }

        public function B21Back():void { B21Choose(1); }

        public function B21Choose(index:uint):void {
            if (B21Answer == null || index > 2) return;
            var answer:Function = B21Answer;
            B21Answer = null;
            answer.call(null, index);
        }
    }
}
