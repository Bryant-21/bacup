package {
    import flash.display.MovieClip;
    import flash.filters.DropShadowFilter;
    import flash.geom.ColorTransform;
    import flash.geom.Rectangle;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class B21_StatusReadout extends MovieClip {
        public var text:String = "";
        public var icons:Array = [];
        private var lastKey:String = "";

        public static function makeText():TextField {
            var field:TextField = new TextField();
            field.defaultTextFormat = new TextFormat("$MAIN_Font",24,0xFFFFCB);
            field.selectable = field.mouseEnabled = false;
            field.height = 28;
            field.filters = [new DropShadowFilter(1,45,0,1,2,2,1,1)];
            return field;
        }

        private function valueText(value:String, color:uint):TextField {
            var field:TextField = makeText();
            field.text = value;
            field.textColor = color;
            field.width = field.textWidth+5;
            field.height = field.textHeight+4;
            field.y = (28-field.textHeight)/2-2;
            return field;
        }

        public function update(row:Object, color:uint):void {
            var key:String = String(color)+"|";
            var supplied:Array = row == null ? null : row.resistances as Array;
            var values:Array = [];
            if (supplied != null) for each (var resistance:Object in supplied)
                if (Math.round(Number(resistance.value)) != 0) values.push(resistance);
            var health:String = row != null && Boolean(row.healthNumbers)
                ? Math.round(Number(row.current))+"/"+Math.round(Number(row.maximum)) : "";
            key += health;
            for each (var visibleResistance:Object in values)
                key += "|"+visibleResistance.icon+":"+visibleResistance.label+":"+Math.round(Number(visibleResistance.value));
            if (key == lastKey) return;
            lastKey = key;
            while (numChildren > 0) removeChildAt(numChildren-1);
            graphics.clear();
            icons = [];
            var labels:Array = [];
            var at:Number = 0;
            var field:TextField;
            if (health.length > 0) {
                field = valueText(health,color);
                addChild(field);
                at = field.width+18;
                labels.push(health);
            }
            for each (var item:Object in values) {
                var frame:int = int(item.icon);
                if (frame > 0 && frame <= 6) {
                    var icon:MovieClip = new B21_StatusResistanceIcon();
                    icon.gotoAndStop(frame);
                    icon.scaleX = icon.scaleY = 21/icon.height;
                    addChild(icon);
                    var bounds:Rectangle = icon.getBounds(this);
                    icon.x = at-bounds.x;
                    icon.y = 3-bounds.y;
                    icon.transform.colorTransform = new ColorTransform(0,0,0,1,color >> 16 & 255,color >> 8 & 255,color & 255);
                    icon.filters = [new DropShadowFilter(1,45,0,1,2,2,1,1)];
                    at += icon.width+6;
                    icons.push(frame);
                }
                var caption:String = String(item.label);
                var amount:String = Math.round(Number(item.value)).toString();
                field = valueText((frame > 0 ? "" : caption+" ")+amount,color);
                field.x = at;
                addChild(field);
                at += field.width+18;
                labels.push(caption+" "+amount);
            }
            var width:Number = Math.max(0,at-18);
            for (var i:int = 0; i < numChildren; ++i) getChildAt(i).x -= width/2;
            if (numChildren > 0) {
                var left:Number = -width/2-10;
                width += 20;
                // EnemyHealthMeter.HealthBarFrame uses this fill and a one-pixel black edge.
                graphics.beginFill(0x181C1D,114/255);
                graphics.drawRect(left,0,width,28);
                graphics.endFill();
                graphics.beginFill(0);
                graphics.drawRect(left-1,-1,width+2,1);
                graphics.drawRect(left-1,28,width+2,1);
                graphics.drawRect(left-1,0,1,28);
                graphics.drawRect(left+width,0,1,28);
                graphics.endFill();
            }
            text = labels.join("    ");
            visible = text.length > 0;
        }
    }
}
