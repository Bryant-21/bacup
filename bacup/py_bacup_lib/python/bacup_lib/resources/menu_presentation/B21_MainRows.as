package {
    import flash.display.MovieClip;
    import flash.events.MouseEvent;
    import flash.geom.Matrix;
    import flash.text.TextFormat;

    public class B21_MainRows extends MovieClip {
        private var rows:Array = [];
        private var signature:String = "";
        private var lastSelection:int = -2;
        public var select:Function;
        public var accept:Function;

        public function Render(entries:Array, selected:int):void {
            var key:String = "";
            for each (var entry:Object in entries)
                key += String(entry.text) + ":" + Boolean(entry.disabled) + ":" + Boolean(entry.b21Small) + ":" +
                    Boolean(entry.waitingForLoad) + ":" + String(entry.b21Image) + "|";
            if (key != signature) {
                signature = key;
                lastSelection = -2;
                while (numChildren) removeChildAt(0);
                rows = [];
                var yOffset:Number = 0;
                for (var i:int = 0; i < entries.length; i++) {
                    entry = entries[i];
                    var small:Boolean = Boolean(entry.b21Small);
                    var clip:MovieClip = small ? new B21_MainSmall() : new B21_MainLarge();
                    var pose:Array = small ? B21_MenuLayout.main.small : B21_MenuLayout.main.large;
                    clip.transform.matrix = new Matrix(pose[0], pose[1], pose[2], pose[3], pose[4], pose[5] + yOffset);
                    clip.b21Index = i;
                    clip.b21Small = small;
                    clip.b21Disabled = Boolean(entry.disabled);
                    clip.mouseChildren = false;
                    clip.buttonMode = !clip.b21Disabled;
                    clip.Spinner_mc.visible = Boolean(entry.waitingForLoad);
                    if (!small) {
                        clip.hitArea = clip.HitArea_mc;
                        clip.HitArea_mc.visible = false;
                        clip.Image_mc.ImageBody_mc.gotoAndStop(entry.b21Image || "play");
                    }
                    clip.Sizer_mc.visible = false;
                    addChild(clip);
                    clip.Text_mc.Text_tf.text = String(entry.text);
                    clip.addEventListener(MouseEvent.ROLL_OVER, Hover);
                    clip.addEventListener(MouseEvent.CLICK, Click);
                    rows.push(clip);
                    yOffset += clip.Sizer_mc.height;
                }
            }
            if (selected == lastSelection) return;
            lastSelection = selected;
            for (i = 0; i < rows.length; i++) {
                clip = rows[i];
                var active:Boolean = i == selected;
                var label:String = active ? "rollOn" : "off";
                if (clip.b21Disabled) label += "Disabled";
                if (clip.b21Small) {
                    clip.gotoAndStop(label);
                    var format:TextFormat = new TextFormat();
                    format.underline = active;
                    clip.Text_mc.Text_tf.setTextFormat(format);
                } else {
                    if (active) clip.gotoAndPlay(label);
                    else clip.gotoAndStop(label);
                    if (active && !clip.b21Disabled) clip.Image_mc.gotoAndPlay("rollOn");
                    else clip.Image_mc.gotoAndStop("off");
                }
            }
        }

        private function Hover(event:MouseEvent):void {
            var row:MovieClip = event.currentTarget as MovieClip;
            if (select != null) select(int(row.b21Index));
        }
        private function Click(event:MouseEvent):void {
            var row:MovieClip = event.currentTarget as MovieClip;
            if (select != null) select(int(row.b21Index));
            if (!row.b21Disabled && accept != null) accept();
        }
    }
}
