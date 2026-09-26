package {
    import flash.display.DisplayObjectContainer;
    import flash.display.MovieClip;
    import flash.geom.ColorTransform;
    import flash.text.TextField;
    import flash.text.TextFormat;

    // FO76's challenge progress row ("Kill a Mutant Hound  3/3"). Its frame scripts lived in the class this
    // replaces, so they are re-added here; the idle hold counts Tales' gameplay time instead of a Timer.
    public dynamic class ChallengeUpdateFlyout extends MovieClip {
        private var current:uint = 0;
        private var done:uint = 0;
        private var progress:Number = -1;
        private var hold:Number = -1;
        private var cooldown:Number = 0;

        public function ChallengeUpdateFlyout() {
            super();
            addFrameScript(0, frame1, 43, frame44, 155, frame156);
            mouseEnabled = false;
            mouseChildren = false;
            gotoAndStop("off");
            visible = false;
        }

        // Returns the id of the last notice that finished rolling off.
        public function B21SetUpdate(data:Object):uint {
            var delta:Number = Number(data.delta);
            if (!(delta > 0)) delta = 0;
            if (cooldown > 0) cooldown -= delta;
            visible = Boolean(data.visible);
            if (!visible) return done;
            // FO76 docks the flyouts to the left edge of the screen at (-1,-6).
            x = Number(data.left) - 1;
            y = -6;
            tint(uint(data.color));
            if (uint(data.id) != current) {
                // FO76 meant to leave a second between rows; its shipped cooldown timer never fired.
                if (cooldown > 0) return done;
                current = uint(data.id);
                progress = Number(data.progress);
                hold = -1;
                texts(data);
                gotoAndPlay("rollOn");
                return done;
            }
            if (Number(data.progress) != progress) {
                // The same challenge advanced while its row is up: FO76 refreshes it and holds 2 s longer.
                progress = Number(data.progress);
                texts(data);
                if (hold >= 0) hold += 2;
            }
            if (hold >= 0) {
                hold -= delta;
                if (hold < 0) {
                    hold = -1;
                    gotoAndPlay("rollOff");
                }
            }
            return done;
        }

        private function frame1():void { stop(); }

        // Fully shown: FO76 stops at idle and holds about 3 s (a 1 s timer counting down from 2).
        private function frame44():void {
            gotoAndStop("idle");
            hold = 3;
        }

        private function frame156():void {
            done = current;
            gotoAndStop("off");
            cooldown = 1;
        }

        private function texts(data:Object):void {
            setText(field(child("Name_mc")), String(data.name));
            setText(field(child("Count_mc")), uint(data.progress) + "/" + uint(data.count));
        }

        private function child(name:String):DisplayObjectContainer {
            var value:DisplayObjectContainer = getChildByName(name) as DisplayObjectContainer;
            if (value == null) throw new Error("Missing challenge update part " + name);
            return value;
        }

        private function field(container:DisplayObjectContainer):TextField {
            for (var i:int = 0; i < container.numChildren; ++i) {
                var text:TextField = container.getChildAt(i) as TextField;
                if (text != null) return text;
            }
            return null;
        }

        private function setText(target:TextField, value:String):void {
            if (target == null) throw new Error("Missing challenge update text");
            target.text = value;
            if (!target.multiline && target.textWidth > target.width - 4) {
                var format:TextFormat = target.getTextFormat();
                format.size = Math.max(1, Math.floor(Number(format.size) * (target.width - 4) / target.textWidth));
                target.setTextFormat(format);
            }
        }

        // FO4 HUD Color, as the status HUD applies it to FO76's cream art.
        private function tint(color:uint):void {
            var base:uint = 0xFFFFCB;
            transform.colorTransform = new ColorTransform(
                Math.max(2, (color >> 16) & 255) / Math.max(2, (base >> 16) & 255),
                Math.max(2, (color >> 8) & 255) / Math.max(2, (base >> 8) & 255),
                Math.max(2, color & 255) / Math.max(2, base & 255));
        }
    }
}
