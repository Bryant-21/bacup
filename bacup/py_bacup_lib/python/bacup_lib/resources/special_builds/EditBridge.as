package SpecialBuilds {
    import flash.display.MovieClip;

    public class EditSpecialModal extends MovieClip {
        public function B21SetPlatform(gamepad:Boolean):void {
            this.m_SpecialEditLeftButton.ButtonVisible = !gamepad;
        }

        public function B21Input(action:String):void {
            if (currentLabel != "rollOn") return;
            if (action == "Up" || action == "Down") {
                this.List_mc.selectedIndex = Math.max(0, Math.min(6,
                    this.List_mc.selectedIndex + (action == "Down" ? 1 : -1)));
            } else if (action == "Left" || action == "Right") this.updateSelectedSpecial(action == "Right");
            else this.ProcessUserEvent(action, false);
        }

    }
}
