package {
    import flash.display.Loader;
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.net.URLRequest;

    public dynamic class B21_DailyOpsRadio extends MovieClip {
        public var AppalachiaVOFlyoutGraphic_mc:MovieClip;
        public var XPDVOFlyoutGraphic_mc:MovieClip;
        public var DOVOFlyoutGraphic_mc:MovieClip;
        public var DOVOWaveForm_mc:MovieClip;
        public var SlasherVOFlyoutGraphic_mc:MovieClip;
        private var portraitLoader:Loader;
        private var portrait:MovieClip;
        private var speaker:String = "";
        private var displayed:String = "";

        public function B21_DailyOpsRadio() {
            super();
            gotoAndStop("dailyOps");
            addEventListener(Event.ENTER_FRAME, update);
            portraitLoader = new Loader();
            portraitLoader.contentLoaderInfo.addEventListener(Event.COMPLETE, loaded);
            portraitLoader.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, failed);
            portraitLoader.load(new URLRequest("vocharacteranim.swf"));
        }

        private function loaded(event:Event):void {
            portrait = portraitLoader.content as MovieClip;
            portrait.x = 1686;
            portrait.y = 617;
            portrait.VOCharacterAnim_mc.gotoAndStop("BS01_NPCM_DailyOps_Dodge");
        }

        private function failed(event:IOErrorEvent):void {
            trace("B21 Daily Ops: converted radio portrait is missing");
        }

        public function B21SetSpeaker(name:String):void {
            speaker = name;
        }

        private function update(event:Event):void {
            if (!DOVOFlyoutGraphic_mc) return;
            if (portrait && portrait.parent != DOVOFlyoutGraphic_mc.VOCharacter_mc)
                DOVOFlyoutGraphic_mc.VOCharacter_mc.addChild(portrait);
            if (speaker == displayed) {
                if (speaker == "" && DOVOFlyoutGraphic_mc.currentLabel == "off") visible = false;
                return;
            }
            displayed = speaker;
            visible = true;
            DOVOFlyoutGraphic_mc.VOCharName_mc.textField_tf.text = speaker;
            DOVOFlyoutGraphic_mc.gotoAndPlay(speaker == "" ? "rollOff" : "rollOn");
        }
    }
}
