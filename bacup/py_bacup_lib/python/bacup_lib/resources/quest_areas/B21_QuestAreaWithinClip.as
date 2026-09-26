package {
    import flash.display.MovieClip;

    // FO76 exports this clip under a generated class name that FO4's HUD does not define.
    public dynamic class B21_QuestAreaWithinClip extends MovieClip {
        public var InsideAreaText_mc:MovieClip;

        public function B21_QuestAreaWithinClip() {
            super();
            addFrameScript(0, frame1, 139, frame140);
        }

        internal function frame1():* {
            stop();
        }

        internal function frame140():* {
            gotoAndPlay(50);
        }
    }
}
