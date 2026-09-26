package {
    import flash.display.MovieClip;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public dynamic class ChallengeTracker extends MovieClip {
        private var heading:TextField;
        private var rows:TextField;

        public function ChallengeTracker() {
            super();
            stop();
            mouseEnabled = false;
            mouseChildren = false;
            heading = field(1380, 125, 430, 32, 24, 0xFFFFFF);
            rows = field(1280, 165, 530, 250, 20, 0xE6E6E6);
            rows.multiline = true;
            rows.wordWrap = true;
            visible = false;
        }

        private function field(x:Number, y:Number, width:Number, height:Number, size:uint, color:uint):TextField {
            var value:TextField = new TextField();
            value.defaultTextFormat = new TextFormat("$MAIN_Font", size, color, false, false, false, null, null, "right");
            value.x = x;
            value.y = y;
            value.width = width;
            value.height = height;
            value.selectable = false;
            addChild(value);
            return value;
        }

        public function B21SetTracker(data:Object):void {
            visible = data.visible;
            if (!visible) return;
            heading.text = "CHALLENGES";
            var lines:Array = [];
            for each (var entry:Object in data.entries) {
                lines.push(entry.name + "  " + entry.progress + "/" + entry.count + (entry.completed ? "  COMPLETE" : ""));
            }
            rows.text = lines.join("\n");
        }
    }
}
