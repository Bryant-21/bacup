package {
    import flash.display.MovieClip;

    // FO4's HUD loader sets SwfLoaderURL on this root and stops it on its last frame, so the
    // root must be dynamic and its timeline must last as long as the art.
    public dynamic class B21PerkVaultBoy extends MovieClip {
        public var Art_mc:MovieClip;

        public function B21PerkVaultBoy() {
            super();
            addFrameScript(totalFrames - 1, holdArt);
        }

        // The loader only stops the root; without this the art loops back to frame 1 while the HUD fades it out.
        internal function holdArt():* {
            if (Art_mc) Art_mc.stop();
        }
    }
}
