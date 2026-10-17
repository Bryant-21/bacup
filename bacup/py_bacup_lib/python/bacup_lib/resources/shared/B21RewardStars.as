package {
    import flash.display.DisplayObject;
    import flash.display.Sprite;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class B21RewardStars {
        public static function setText(target:TextField,value:String):void {
            var positions:Array = [];
            var label:String = "";
            for (var i:int = 0; i < value.length; ++i) {
                var character:String = value.charAt(i);
                // FO76's font maps the not-sign to a star; FO4's remapped font does not.
                if (character == "\u00ac" || character == "\u2605") {
                    positions.push(label.length);
                    label += "  ";
                } else label += character;
            }
            var color:uint = target.textColor;
            target.text = label;
            target.textColor = color;
            if (!target.multiline && target.textWidth > target.width-4) {
                var format:TextFormat = target.defaultTextFormat;
                format.size = Math.max(1,Math.floor(Number(format.size)*(target.width-4)/target.textWidth));
                target.setTextFormat(format);
            }
            var previous:DisplayObject = target.parent.getChildByName("B21RankGlyphs_"+target.name);
            if (previous != null) target.parent.removeChild(previous);
            if (positions.length == 0) return;
            var glyphs:Sprite = new Sprite();
            glyphs.name = "B21RankGlyphs_"+target.name;
            glyphs.mouseEnabled = false;
            glyphs.mouseChildren = false;
            target.parent.addChild(glyphs);
            for (var index:int = 0; index < positions.length; ++index) {
                var first:Object = target.getCharBoundaries(int(positions[index]));
                var last:Object = target.getCharBoundaries(int(positions[index])+1);
                if (first == null || last == null) continue;
                var star:Sprite = new Sprite();
                star.name = "B21RewardStar"+index;
                star.transform.matrix = target.transform.matrix;
                var radius:Number = Math.min(Number(first.height)*0.35,(Number(last.x)+Number(last.width)-Number(first.x))*0.48);
                var x:Number = (Number(first.x)+Number(last.x)+Number(last.width))*0.5;
                var y:Number = Number(first.y)+Number(first.height)*0.5;
                star.graphics.beginFill(target.textColor);
                for (var point:int = 0; point <= 10; ++point) {
                    var angle:Number = -Math.PI/2+point*Math.PI/5;
                    var r:Number = point%2 == 0 ? radius : radius*0.4;
                    if (point == 0) star.graphics.moveTo(x+Math.cos(angle)*r,y+Math.sin(angle)*r);
                    else star.graphics.lineTo(x+Math.cos(angle)*r,y+Math.sin(angle)*r);
                }
                star.graphics.endFill();
                glyphs.addChild(star);
            }
        }
    }
}
