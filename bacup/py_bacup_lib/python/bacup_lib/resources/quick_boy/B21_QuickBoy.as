package {
    import flash.display.MovieClip;

    public class B21_QuickBoy extends MovieClip {
        public var B21QuickBoyVersion:int = 1;

        public function B21_QuickBoy() {
            mouseEnabled = false;
            mouseChildren = false;
            var art:B21_QuickBoyArt = new B21_QuickBoyArt();
            addChild(art);
            // The converted screen mesh supplies the world gradient; this panel would double its shadow.
            art.MainBackground_mc.visible = false;
        }
    }
}
