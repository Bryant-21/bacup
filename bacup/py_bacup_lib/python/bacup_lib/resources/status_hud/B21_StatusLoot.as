package {
    import flash.display.MovieClip;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.events.MouseEvent;
    import flash.filters.DropShadowFilter;
    import flash.geom.ColorTransform;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.utils.getDefinitionByName;
    import flash.utils.getQualifiedClassName;
    public class B21_StatusLoot extends MovieClip {
        public var art:MovieClip = new B21_StatusQuickLoot();
        private var buttons:MovieClip = new MovieClip();
        private var elapsed:Number = 0;
        private var opened:Boolean = false;
        private var diagnosticsRows:Array = [];
        public function B21_StatusLoot() {
            addChild(art);
            addChild(buttons);
            buttons.y = B21_StatusSource.layout.quickButtons[5];
            art.Spinner_mc.visible = false;
        }
        private function fit(field:TextField, value:String, size:Number):void {
            field.text = value;
            var format:TextFormat = field.defaultTextFormat;
            format.size = size;
            field.setTextFormat(format);
            if (field.textWidth > field.width-4) {
                format.size = Math.max(1,Math.floor(size*(field.width-4)/field.textWidth));
                field.setTextFormat(format);
            }
        }
        private function recolor(clip:DisplayObject, selected:Boolean):void {
            clip.transform.colorTransform = new ColorTransform(0,0,0,1,selected ? 0 : 255,selected ? 0 : 255,selected ? 0 : 255,0);
        }
        private function forward(event:MouseEvent):void {
            var button:Object = event.currentTarget;
            if (button.sourceHint != null) button.sourceHint.dispatchEvent(event.clone());
        }
        private function updateButtons(stock:Object):Boolean {
            var holder:DisplayObjectContainer = stock.ButtonHintBar_mc.ButtonHintBarInternal_mc as DisplayObjectContainer;
            if (holder == null) return false;
            var count:int = 0;
            var offset:Number = 0;
            for (var i:int = 0; i < holder.numChildren; ++i) {
                var hint:Object = holder.getChildAt(i);
                if (!hint.visible || !hint.hasOwnProperty("IconHolderInstance") || !hint.hasOwnProperty("textField_tf")) continue;
                var source:TextField = hint.IconHolderInstance.IconAnimInstance.Icon_tf as TextField;
                if (source == null) return false;
                var button:MovieClip;
                if (count == buttons.numChildren) {
                    button = new B21_StatusHoldButton();
                    button.mouseChildren = false;
                    buttons.addChild(button);
                    button.addEventListener(MouseEvent.CLICK,forward);
                    button.addEventListener(MouseEvent.MOUSE_DOWN,forward);
                    button.addEventListener(MouseEvent.MOUSE_UP,forward);
                } else button = buttons.getChildAt(count) as MovieClip;
                ++count;
                button.sourceHint = hint;
                button.visible = true;
                button.alpha = hint.alpha;
                button.SecondaryIconHolderInstance.visible = false;
                button.Highlight_mc.visible = button.Sizer_mc.visible = button.HoldMeter_mc.visible = false;
                var glyph:TextField = button.IconHolderInstance.IconAnimInstance.Icon_tf;
                glyph.text = source.text;
                var format:TextFormat = glyph.defaultTextFormat;
                format.font = source.getTextFormat().font;
                glyph.setTextFormat(format);
                glyph.textColor = 0xffffff;
                glyph.autoSize = "left";
                glyph.y = hint.UsePCKey ? 1.25 : 2.25;
                button.textField_tf.text = hint.textField_tf.text.toUpperCase();
                button.textField_tf.textColor = 0xffffff;
                button.textField_tf.autoSize = "left";
                button.textField_tf.x = button.IconHolderInstance.width+10;
                button.x = offset;
                offset += button.textField_tf.x+button.textField_tf.width+20;
            }
            while (buttons.numChildren > count) buttons.removeChildAt(buttons.numChildren-1);
            buttons.x = -Math.max(0,offset-20)/2;
            return true;
        }
        public function reset():void { opened = false; elapsed = 0; }
        public function update(stock:Object, data:Object):Boolean {
            if (stock.ListItems_mc == null || stock.ListHeaderAndBracket_mc == null || stock.ButtonHintBar_mc == null) return false;
            var header:Object = stock.ListHeaderAndBracket_mc;
            if (header.ContainerName_mc == null || header.ContainerName_mc.textField_tf == null || header.BracketPairHolder_mc == null) return false;
            if (!opened) { opened = true; elapsed = 0; }
            else elapsed += Math.max(0,Number(data.delta));
            art.gotoAndStop(header.BracketPairHolder_mc.visible ? Math.min(12,3+Math.floor(elapsed*B21_StatusSource.layout.holdFPS)) : 23);
            art.Spinner_mc.visible = false;
            var title:TextField = art.ListHeaderAndBracket_mc.ContainerName_mc.textField_tf;
            var titleFormat:TextFormat = title.defaultTextFormat;
            titleFormat.align = header.BracketPairHolder_mc.visible ? "left" : "center";
            title.defaultTextFormat = titleFormat;
            fit(art.ListHeaderAndBracket_mc.ContainerName_mc.textField_tf,header.ContainerName_mc.textField_tf.text,
                Number(art.ListHeaderAndBracket_mc.ContainerName_mc.textField_tf.defaultTextFormat.size));
            art.ListHeaderAndBracket_mc.BracketPairHolder_mc.visible = header.BracketPairHolder_mc.visible;
            diagnosticsRows = [];
            for (var i:int = 0; i < 5; ++i) {
                var source:Object = stock.ListItems_mc.getChildByName("ItemText"+i);
                var row:MovieClip = art.ListItems_mc.getChildByName("ItemText"+i) as MovieClip;
                if (source == null || row == null || !source.hasOwnProperty("data")) return false;
                var entry:Object = source.data;
                row.visible = source.visible && entry != null;
                if (!row.visible) continue;
                var selected:Boolean = Boolean(source.selected);
                var label:String = String(entry.text);
                var stars:int = entry.hasOwnProperty("B21LSRank") ? int(entry.B21LSRank) : entry.isLegendary ? 1 : 0;
                if (entry.hasOwnProperty("B21LSName") && entry.B21LSName == label) label = String(entry.B21LSName);
                if (label.charAt(label.length-1) == "\u2605") {
                    stars = 0;
                    while (label.charAt(label.length-1) == "\u2605") { ++stars; label = label.substr(0,label.length-1); }
                    if (label.charAt(label.length-1) == " ") label = label.substr(0,label.length-1);
                }
                stars = Math.max(0,Math.min(5,stars));
                if (int(entry.count) > 1) label += " ("+entry.count+")";
                row.RaritySelector_mc.visible = row.RarityIndicator_mc.visible = row.RarityBorder_mc.visible = false;
                row.conditionMeter.visible = false;
                var health:Number = entry.hasOwnProperty("B21Condition") ? Number(entry.B21Condition) : -1;
                var meter:MovieClip = row.ConditionMeter_mc;
                meter.visible = isFinite(health) && health >= 0 && health <= 2;
                if (meter.visible) {
                    meter.gotoAndStop(1);
                    // FO76's GlobalFunc.updateConditionMeter: frame 1 is broken, 111-220 run empty to full up
                    // to 100%, and 2-110 add the over-repair overlay up to 200%.
                    var frames:int = meter.MeterClip_mc.totalFrames;
                    meter.MeterClip_mc.gotoAndStop(health <= 0 ? 1 : Math.round(frames-(frames-2)/2*Math.min(2,health)));
                }
                row.LegendaryIcon_mc.visible = false;
                row.SelectionIndicator_mc.alpha = selected ? 1 : 0;
                var icons:Array = [row.FavoriteIcon_mc,row.questItemIcon_mc,row.BetterIcon_mc,row.TaggedForSearchIcon_mc,row.SetBonusIcon_mc];
                var flags:Array = [Boolean(entry.favorite),entry.hasOwnProperty("isQuestItem") && Boolean(entry.isQuestItem),
                    Boolean(entry.isBetterThanEquippedItem),Boolean(entry.taggedForSearch),false];
                row.questItemIcon_mc.gotoAndStop("local");
                var starHolder:MovieClip = row.getChildByName("B21Stars") as MovieClip;
                if (starHolder == null) { starHolder = new MovieClip(); starHolder.name = "B21Stars"; row.addChild(starHolder); }
                while (starHolder.numChildren > stars) starHolder.removeChildAt(starHolder.numChildren-1);
                while (starHolder.numChildren < stars) {
                    var type:Class = getDefinitionByName(getQualifiedClassName(row.LegendaryIcon_mc)) as Class;
                    var star:MovieClip = new type(); star.stop(); starHolder.addChild(star);
                }
                var iconWidth:Number = stars*(row.LegendaryIcon_mc.width+2)+(meter.visible ? meter.width : 0);
                for (var j:int = 0; j < icons.length; ++j) if (flags[j]) iconWidth += icons[j].width+2;
                if (!row.hasOwnProperty("B21TextWidth")) row.B21TextWidth = row.ItemName_tf.width;
                row.ItemName_tf.width = Math.max(24,row.B21TextWidth-iconWidth);
                fit(row.ItemName_tf,label,Number(row.ItemName_tf.defaultTextFormat.size));
                row.ItemName_tf.textColor = selected ? 0 : 0xffffff;
                row.ItemName_tf.filters = selected ? [] : [new DropShadowFilter(2,45,0,1,0,0,1,3)];
                var x:Number = row.ItemName_tf.x+row.ItemName_tf.getLineMetrics(0).x+row.ItemName_tf.getLineMetrics(0).width+2;
                starHolder.x = x; starHolder.y = row.LegendaryIcon_mc.y;
                for (j = 0; j < starHolder.numChildren; ++j) {
                    star = starHolder.getChildAt(j) as MovieClip;
                    star.transform.matrix = row.LegendaryIcon_mc.transform.matrix;
                    star.x = j*(row.LegendaryIcon_mc.width+2); star.y = 0;
                    recolor(star,selected);
                }
                x += stars*(row.LegendaryIcon_mc.width+2);
                for (j = 0; j < icons.length; ++j) {
                    icons[j].visible = flags[j];
                    if (flags[j]) { icons[j].x = x; recolor(icons[j],selected); x += icons[j].width+2; }
                }
                diagnosticsRows.push({text:row.ItemName_tf.text,selected:selected,stars:stars,
                    conditionVisible:meter.visible,conditionFrame:meter.MeterClip_mc.currentFrame,conditionFrames:meter.MeterClip_mc.totalFrames});
            }
            var weight:Number = Number(data.carryWeight);
            var maximum:Number = Number(data.maxCarryWeight);
            art.WeightText_mc.visible = art.WeightIcon_mc.visible = isFinite(weight) && weight >= 0 && isFinite(maximum) && maximum >= 0;
            if (art.WeightText_mc.visible) {
                art.WeightText_mc.WeightText_tf.text = Math.floor(weight)+"/"+Math.floor(maximum);
                art.WeightText_mc.WeightText_tf.textColor = 0xffffff;
                art.WeightIcon_mc.gotoAndStop(weight > maximum ? "warning" : "normal");
            }
            return updateButtons(stock);
        }
        public function diagnostics():Object {
            return {rows:diagnosticsRows,frame:art.currentFrame,buttons:buttons.numChildren,
                    title:art.ListHeaderAndBracket_mc.ContainerName_mc.textField_tf.text,
                    titleAlign:art.ListHeaderAndBracket_mc.ContainerName_mc.textField_tf.getTextFormat().align,
                    weight:art.WeightText_mc.WeightText_tf.text,weightVisible:art.WeightText_mc.visible};
        }
    }
}
