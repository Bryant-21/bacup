package {
    import flash.display.DisplayObject;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.utils.Dictionary;
    public class ItemsListEntry extends Shared.AS3.BSScrollingListEntry {
        private var B21Label:TextField;
        private var B21StateColors:Object;

        // FO4's Scaleform keeps a child's colour when gotoAndStop goes back to an earlier frame; Flash and Ruffle
        // reset it. FO76 hides Selector_mc from RollOnGray on and shows it only by going back to RollOn, so in game
        // a tile once deselected was never highlighted again (2026-10-12). Forward steps apply properly, so each
        // state's child colours are read once, stepping forward from frame 1, and every state change sets them.
        public function B21GotoState(label:String):void {
            if (B21StateColors == null && currentFrame == 1) {
                B21StateColors = {};
                for (var frame:int = 1; frame <= totalFrames; ++frame) {
                    gotoAndStop(frame);
                    var colors:Dictionary = new Dictionary(true);
                    for (var i:int = 0; i < numChildren; ++i) colors[getChildAt(i)] = getChildAt(i).transform.colorTransform;
                    if (currentLabel != null) B21StateColors[currentLabel] = colors;
                }
            }
            gotoAndStop(label);
            var state:Dictionary = B21StateColors != null ? B21StateColors[label] as Dictionary : null;
            if (state == null) return;
            for (var j:int = 0; j < numChildren; ++j) {
                var child:DisplayObject = getChildAt(j);
                if (state[child] != undefined) child.transform.colorTransform = state[child];
            }
        }

        public function B21ShowName(entry:Object):void {
            if (B21Label == null) {
                B21Label = new TextField();
                B21Label.mouseEnabled = false;
                B21Label.multiline = true;
                B21Label.wordWrap = true;
                B21Label.defaultTextFormat = new TextFormat("$MAIN_Font_Bold", 12, 0xFFFFFF);
                B21Label.x = Image_mc.x;
                B21Label.y = Image_mc.y;
                B21Label.width = Image_mc.width;
                B21Label.height = Image_mc.height;
                addChild(B21Label);
            }
            B21Label.text = entry.text;
            B21Label.visible = entry.image.fixtureType == -1;
        }
    }
}
