package {
    import flash.display.MovieClip;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class B21_DailyOpsActive extends MovieClip {
        private var radio:B21_DailyOpsRadio = new B21_DailyOpsRadio();
        private var subtitle:TextField = new TextField();

        public function B21_DailyOpsActive() {
            stop();
            mouseEnabled = false;
            mouseChildren = false;
            addChild(radio);
            subtitle.defaultTextFormat = new TextFormat("$MAIN_Font",25,0xffffff,false,false,false,null,null,"center");
            subtitle.x = 410;
            subtitle.y = 840;
            subtitle.width = 1100;
            subtitle.height = 120;
            subtitle.multiline = true;
            subtitle.wordWrap = true;
            subtitle.selectable = false;
            addChild(subtitle);
            visible = false;
        }

        public function B21SetHUD(data:Object):void {
            visible = data.visible;
            radio.B21SetSpeaker(data.speaker);
            subtitle.text = data.subtitle;
        }
    }
}
