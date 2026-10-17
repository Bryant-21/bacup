package {
    import flash.display.MovieClip;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.text.TextFormatAlign;

    public class B21_RevivePrompt extends MovieClip {
        public var BGSCodeObj:Object;
        private var title:B21_ReviveTitleArt;
        private var timer:B21_ReviveTimerArt;
        private var hint:TextField;

        public function B21_RevivePrompt() {
            super();
            title = new B21_ReviveTitleArt();
            timer = new B21_ReviveTimerArt();
            // The FO76 art's text boxes centre about 140 px right of their registration point.
            title.x = 818;
            title.y = 700;
            timer.x = 818;
            timer.y = 750;
            addChild(title);
            addChild(timer);
            hint = new TextField();
            hint.defaultTextFormat = new TextFormat("$MAIN_Font_Bold", 24, 0xFFFFCB, null, null, null, null, null,
                TextFormatAlign.CENTER);
            hint.selectable = false;
            hint.mouseEnabled = false;
            hint.width = 800;
            hint.height = 36;
            hint.x = 560;
            hint.y = 800;
            addChild(hint);
            visible = false;
        }

        public function B21SetRevive(data:Object):void {
            if (data == null || !data.visible) {
                visible = false;
                return;
            }
            setText(title, String(data.title));
            setText(timer, String(data.seconds) + "s");
            hint.text = String(data.hint);
            hint.alpha = 0.5 + 0.5 * Math.max(0, Math.min(1, Number(data.hold)));
            visible = true;
        }

        private function setText(part:MovieClip, value:String):void {
            for (var i:int = 0; i < part.numChildren; ++i) {
                var field:TextField = part.getChildAt(i) as TextField;
                if (field != null) {
                    field.text = value;
                    return;
                }
            }
        }
    }
}
