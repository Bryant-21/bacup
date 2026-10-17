package {
    import flash.display.MovieClip;
    import flash.display.Sprite;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.geom.ColorTransform;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.utils.Dictionary;
    import flash.utils.getDefinitionByName;
    import flash.utils.getQualifiedClassName;
    // HUDMenu's glass renderer made this panel's translucency opaque and gave its text black shadows,
    // so the HUDMenu instance only captures FO4's widget (capture) and Tales' HUD-depth menu draws it
    // from the snapshot (update).
    public class B21_StatusLoot extends MovieClip {
        // QuickContainerItemData's declared members; Tales' B21 members are dynamic and enumerate.
        private static const STOCK_FIELDS:Array = ["text","count","equipState","filterFlag","favorite","isLegendary",
            "taggedForSearch","isBetterThanEquippedItem","isQuestItem"];
        // Rendered icon runs are large: a change rides the snapshot for this many frames, then only its revision.
        private static const RUNS_FRAMES:int = 30;
        // The HUD tints FO76's cream art onto the HUD colour (B21_StatusHUD.tint). Drawing text and icons
        // white made them paler than the meters, and item-type icon colours landed on the darker title shade.
        private static const CREAM:uint = 0xFFFFCB;
        // FO76's condition meter is gold, as on the HUD's own meter: this maps it onto cream first.
        private static const GOLD_TO_CREAM:ColorTransform = new ColorTransform(255/245,255/203,203/91,1);
        private static const TITLE_GOLD:uint = 0xF5CB5B;
        private static const TITLE_SIZE:Number = 20;
        public var art:MovieClip = new B21_StatusQuickLoot();
        private var buttons:MovieClip = new MovieClip();
        private var elapsed:Number = 0;
        private var opened:Boolean = false;
        private var diagnosticsRows:Array = [];
        private var columns:Sprite = new Sprite();
        private var captions:Array = [];
        private var cndCaption:TextField;
        // FO76's title is an HTML field on a two-frame clip whose frame scripts conversion drops. It never
        // drew in game, while script-made fields (the column captions) do, so the title is our own field.
        public var titleField:TextField = new TextField();
        private var titleCorner:Point;
        private var titleKey:String = "";
        private var authored:Dictionary = new Dictionary();
        private var metrics:Object;
        private var sentRuns:Array = [null,null,null,null,null];
        private var runsRevision:int = 0;
        private var runsFrames:int = 0;
        private var shownRuns:Array = [null,null,null,null,null];
        private var shownRevision:int = -1;
        // Why the last capture fell back to FO4's widget; the native bridge logs changes.
        public var issue:String = "";
        public function B21_StatusLoot() {
            addChild(art);
            addChild(buttons);
            buttons.y = B21_StatusSource.layout.quickButtons[5];
            art.Spinner_mc.visible = false;
            var header:MovieClip = art.ListHeaderAndBracket_mc;
            var source:TextField = MovieClip(header.getChildByName("ContainerName_mc")).getChildByName("textField_tf") as TextField;
            titleCorner = new Point(source.x,source.y);
            // FO76's field values (20 px gold), but not its format: that carries the resolved "Roboto Condensed Bold"
            // name, while the $ alias maps to the bold face through fontconfig.
            titleField.defaultTextFormat = new TextFormat("$MAIN_Font_Bold",TITLE_SIZE,TITLE_GOLD);
            titleField.height = TITLE_SIZE*1.3+4;
            titleField.mouseEnabled = titleField.selectable = titleField.multiline = titleField.wordWrap = false;
        }
        // Names keep the row's size and lose their tail, as FallUI's do; shrinking them to fit made them unreadable.
        private function clip(field:TextField, value:String, size:Number):void {
            var format:TextFormat = field.defaultTextFormat;
            format.size = size;
            field.text = value;
            field.setTextFormat(format);
            if (field.textWidth <= field.width-4) return;
            var low:Number = Number(0);
            var high:Number = Number(value.length);
            while (low < high) {
                // GFx's shifted midpoint stayed at 1 for bounds 1 and 21, hanging the HUD update.
                var middle:Number = Math.floor((low+high+1)/2);
                if (middle <= low || middle > high) break;
                field.text = value.substr(0,middle) + "...";
                field.setTextFormat(format);
                if (field.textWidth <= field.width-4) low = middle;
                else high = middle-1;
            }
            while (low > 0 && value.charAt(low-1) == " ") --low;
            field.text = value.substr(0,low) + "...";
            field.setTextFormat(format);
        }
        // Widens an art clip to the right by extra parent units from its authored size.
        private function stretch(clip:DisplayObject, extra:Number):void {
            var base:Object = authored[clip];
            if (base == null) authored[clip] = base = {scale:clip.scaleX,x:clip.x,width:clip.width,left:clip.getBounds(clip.parent).x,extra:0};
            if (base.extra == extra) return;
            base.extra = extra;
            clip.scaleX = base.scale*(base.width+extra)/base.width;
            clip.x = base.x;
            clip.x += base.left - clip.getBounds(clip.parent).x;
        }
        private function recolor(clip:DisplayObject, selected:Boolean):void {
            clip.transform.colorTransform = new ColorTransform(0,0,0,1,selected ? 0 : 255,selected ? 0 : 255,selected ? 0 : 203,0);
        }
        // Stock class members (the bar's ButtonPoolV and ButtonHintBarInternal_mc, a hint's
        // textField_tf) read as missing from this movie in game, so walk the display list: the bar
        // adds its displayed hints to an unnamed child clip, and each hint names its own parts.
        public static function stockHints(bar:DisplayObjectContainer):Array {
            var hints:Array = [];
            for (var i:int = 0; i < bar.numChildren; ++i) {
                var holder:DisplayObjectContainer = bar.getChildAt(i) as DisplayObjectContainer;
                for (var j:int = 0; holder != null && j < holder.numChildren; ++j) {
                    var hint:DisplayObjectContainer = holder.getChildAt(j) as DisplayObjectContainer;
                    if (hint != null && hint.getChildByName("textField_tf") is TextField && hint.getChildByName("IconHolderInstance") is DisplayObjectContainer)
                        hints.push(hint);
                }
            }
            return hints;
        }
        public static function hintGlyph(hint:DisplayObjectContainer):TextField {
            var holder:DisplayObjectContainer = hint.getChildByName("IconHolderInstance") as DisplayObjectContainer;
            var anim:DisplayObjectContainer = holder.getChildByName("IconAnimInstance") as DisplayObjectContainer;
            return anim == null ? null : anim.getChildByName("Icon_tf") as TextField;
        }
        private function captureHints(stock:Object):Array {
            var bar:DisplayObjectContainer = stock.ButtonHintBar_mc as DisplayObjectContainer;
            if (bar == null) { issue = "stock button bar is " + getQualifiedClassName(stock.ButtonHintBar_mc); return null; }
            var hints:Array = [];
            for each (var hint:DisplayObjectContainer in stockHints(bar)) {
                if (!hint.visible) continue;
                var glyph:TextField = hintGlyph(hint);
                if (glyph == null) { issue = "stock button glyph missing"; return null; }
                hints.push({glyph:glyph.text,font:glyph.getTextFormat().font,pcKey:Boolean(Object(hint).UsePCKey),alpha:hint.alpha,
                    label:TextField(hint.getChildByName("textField_tf")).text.toUpperCase()});
            }
            return hints;
        }
        private function drawButtons(hints:Array, center:Number):void {
            var offset:Number = 0;
            for (var i:int = 0; i < hints.length; ++i) {
                var hint:Object = hints[i];
                var button:MovieClip;
                if (i == buttons.numChildren) {
                    button = new B21_StatusHoldButton();
                    button.mouseChildren = false;
                    buttons.addChild(button);
                } else button = buttons.getChildAt(i) as MovieClip;
                button.visible = true;
                button.alpha = Number(hint.alpha);
                button.SecondaryIconHolderInstance.visible = false;
                button.Highlight_mc.visible = button.Sizer_mc.visible = button.HoldMeter_mc.visible = false;
                var glyph:TextField = button.IconHolderInstance.IconAnimInstance.Icon_tf;
                glyph.text = String(hint.glyph);
                var format:TextFormat = glyph.defaultTextFormat;
                format.font = String(hint.font);
                glyph.setTextFormat(format);
                glyph.textColor = CREAM;
                glyph.autoSize = "left";
                glyph.y = hint.pcKey ? 1.25 : 2.25;
                button.textField_tf.text = String(hint.label);
                button.textField_tf.textColor = CREAM;
                button.textField_tf.autoSize = "left";
                button.textField_tf.x = button.IconHolderInstance.width+10;
                button.x = offset;
                offset += button.textField_tf.x+button.textField_tf.width+20;
            }
            while (buttons.numChildren > hints.length) buttons.removeChildAt(buttons.numChildren-1);
            buttons.x = center-Math.max(0,offset-20)/2;
        }
        public function reset():void { opened = false; elapsed = 0; }
        private function fail(reason:String):Boolean { issue = reason; return false; }
        // Sizes follow the converted row's own name font, as the full inventory lists do.
        private function measure(row:Object):Object {
            var format:TextFormat = row.ItemName_tf.defaultTextFormat;
            var size:Number = Number(format.size) > 0 ? Number(format.size) : 20;
            var columnSize:Number = Math.round(size*0.72);
            var probe:TextField = new TextField();
            probe.defaultTextFormat = new TextFormat(format.font,columnSize);
            probe.text = "000.0";
            var name:TextField = new TextField();
            name.defaultTextFormat = new TextFormat(format.font,size);
            name.text = "Wg";
            var select:Rectangle = row.SelectionIndicator_mc.getBounds(row);
            return {font:format.font,columnSize:columnSize,
                slack:Math.max(0,select.right-Number(row.B21TextX)-Number(row.B21TextWidth)-(Number(row.B21TextX)-select.x)),
                columnWidth:Math.min(Math.ceil(probe.textWidth+columnSize*0.45),Math.floor(Number(row.B21TextWidth)*0.15)),
                headingSize:Math.max(8,Math.round(size*0.55)),icon:Math.round(size*1.05),gap:Math.round(size*0.3),
                baseline:name.getLineMetrics(0).ascent-probe.getLineMetrics(0).ascent};
        }
        // HUDMenu side: FO4's widget as plain data, which is all that can cross into the menu's movie.
        public function capture(stock:Object, data:Object):Object {
            issue = "";
            if (stock.ListItems_mc == null || stock.ListHeaderAndBracket_mc == null || stock.ButtonHintBar_mc == null) { issue = "stock widget parts missing"; return null; }
            var header:Object = stock.ListHeaderAndBracket_mc;
            if (header.ContainerName_mc == null || header.ContainerName_mc.textField_tf == null || header.BracketPairHolder_mc == null) { issue = "stock header parts missing"; return null; }
            var rows:Array = [];
            var runs:Array = [];
            for (var i:int = 0; i < 5; ++i) {
                var source:Object = stock.ListItems_mc.getChildByName("ItemText"+i);
                if (source == null) { issue = "row "+i+" missing"; return null; }
                if (!("data" in source)) { issue = "stock row "+i+" has no data property"; return null; }
                var entry:Object = source.data;
                var shown:Boolean = source.visible && entry != null;
                var copy:Object = null;
                if (shown) {
                    copy = {};
                    for each (var key:String in STOCK_FIELDS) if (entry[key] !== undefined) copy[key] = entry[key];
                    for (var member:String in entry)
                        if (entry[member] is String || entry[member] is Number || entry[member] is Boolean) copy[member] = entry[member];
                }
                rows.push({shown:shown,selected:Boolean(source.selected),entry:copy});
                runs.push(shown ? entry.B21IconRuns as Array : null);
                if (runs[i] !== sentRuns[i]) { sentRuns[i] = runs[i]; ++runsRevision; runsFrames = RUNS_FRAMES; }
            }
            var hints:Array = captureHints(stock);
            if (hints == null) return null;
            var state:Object = {title:String(header.ContainerName_mc.textField_tf.text),brackets:Boolean(header.BracketPairHolder_mc.visible),
                rows:rows,hints:hints,runsRevision:runsRevision,carryWeight:Number(data.carryWeight),maxCarryWeight:Number(data.maxCarryWeight)};
            if (runsFrames > 0) { --runsFrames; state.runs = runs; }
            // Logged by the native bridge when it changes; quick loot still draws.
            if (state.title == "" && rows[0].shown) issue = "not a fallback: Fallout 4's container title is empty";
            return state;
        }
        // Menu side: draws a capture.
        public function update(state:Object, data:Object):Boolean {
            issue = "";
            if (!(state.rows is Array) || !(state.hints is Array)) return fail("capture incomplete");
            if (!opened) { opened = true; elapsed = 0; }
            else elapsed += Math.max(0,Number(data.delta));
            if (state.runs is Array && int(state.runsRevision) != shownRevision) { shownRuns = state.runs as Array; shownRevision = int(state.runsRevision); }
            var runs:Array = int(state.runsRevision) == shownRevision ? shownRuns : [null,null,null,null,null];
            var brackets:Boolean = Boolean(state.brackets);
            art.gotoAndStop(brackets ? Math.min(12,3+Math.floor(elapsed*B21_StatusSource.layout.holdFPS)) : 23);
            art.Spinner_mc.visible = false;
            var header:MovieClip = art.ListHeaderAndBracket_mc;
            var titleClip:DisplayObject = header.getChildByName("ContainerName_mc");
            if (titleClip != null) titleClip.visible = false;
            // Our script-made captions draw in the list clip; the title never drew inside FO76's header clip, so it
            // lives beside them and follows the header, which moves when the brackets close.
            if (titleField.parent != art.ListItems_mc) art.ListItems_mc.addChild(titleField);
            var titleAt:Point = art.ListItems_mc.globalToLocal(header.localToGlobal(titleCorner));
            titleField.x = titleAt.x; titleField.y = titleAt.y;
            var firstRow:Object = art.ListItems_mc.getChildByName("ItemText0");
            if (firstRow == null) return fail("converted row art missing");
            if (!firstRow.hasOwnProperty("B21TextWidth")) firstRow.B21TextWidth = firstRow.ItemName_tf.width;
            if (!firstRow.hasOwnProperty("B21TextX")) firstRow.B21TextX = firstRow.ItemName_tf.x;
            if (metrics == null) metrics = measure(firstRow);
            var span:Number = 3*metrics.columnWidth;
            var firstData:Object = state.rows[0] == null ? null : state.rows[0].entry;
            var showColumns:Boolean = firstData != null && Boolean(firstData.B21Columns);
            // With columns on, condition gets its own column before WT; FO76's meter sat where the columns now are.
            var conditions:Boolean = false;
            for (var shown:int = 0; shown < 5; ++shown) {
                var shownEntry:Object = state.rows[shown] != null && state.rows[shown].shown ? state.rows[shown].entry : null;
                var shownHealth:Number = shownEntry != null && shownEntry.hasOwnProperty("B21Condition") ? Number(shownEntry.B21Condition) : -1;
                if (isFinite(shownHealth) && shownHealth >= 0 && shownHealth <= 2) conditions = true;
            }
            var cnd:Number = showColumns && conditions ? Math.round(metrics.columnWidth*1.3) : 0;
            // Columns take the highlight's spare right margin, then widen the panel to the right, instead of taking the names' room.
            var wide:Number = showColumns ? Math.max(0,span+cnd-metrics.slack) : 0;
            var grow:Number = showColumns ? metrics.slack+wide : 0;
            var textWidth:Number = Number(firstRow.B21TextWidth)+grow;
            var artScale:Number = art.ListItems_mc.scaleX*firstRow.scaleX;
            stretch(art.getChildAt(0),wide*artScale);
            var columnsLeft:Number = firstRow.x+firstRow.B21TextX+textWidth-span-cnd;
            titleField.width = Math.max(24,(showColumns ? columnsLeft-metrics.gap : firstRow.x+Number(firstRow.B21TextX)+textWidth)-titleField.x);
            art.ListItems_mc.addChild(columns);
            columns.visible = showColumns;
            if (showColumns) {
                if (cndCaption == null) {
                    cndCaption = new TextField();
                    cndCaption.defaultTextFormat = new TextFormat(metrics.font,metrics.headingSize,CREAM,false,false,false,null,null,"center");
                    cndCaption.text = "CND";
                    cndCaption.height = metrics.headingSize*1.3+4;
                    cndCaption.mouseEnabled = cndCaption.selectable = false;
                    columns.addChild(cndCaption);
                }
                cndCaption.visible = cnd > 0;
                cndCaption.width = cnd;
                cndCaption.x = columnsLeft;
                cndCaption.y = firstRow.y + firstRow.ItemName_tf.y - cndCaption.height;
                for (var heading:int = 0; heading < 3; heading++) {
                    var caption:TextField;
                    if (heading >= captions.length) {
                        caption = new TextField();
                        caption.defaultTextFormat = new TextFormat(metrics.font,metrics.headingSize,CREAM,false,false,false,null,null,"right");
                        caption.text = ["WT","VAL","V/W"][heading];
                        caption.width = metrics.columnWidth; caption.height = metrics.headingSize*1.3+4;
                        caption.mouseEnabled = caption.selectable = false;
                        columns.addChild(caption);
                        captions.push(caption);
                    } else caption = captions[heading] as TextField;
                    caption.x = columnsLeft + cnd + heading*metrics.columnWidth;
                    caption.y = firstRow.y + firstRow.ItemName_tf.y - caption.height;
                }
            }
            var titleFormat:TextFormat = titleField.defaultTextFormat;
            titleFormat.align = brackets ? "left" : "center";
            titleField.defaultTextFormat = titleFormat;
            var nextTitle:String = String(state.title) + ":" + titleField.width + ":" + titleFormat.align;
            if (titleKey != nextTitle) { titleKey = nextTitle; clip(titleField,String(state.title),TITLE_SIZE); }
            art.ListHeaderAndBracket_mc.BracketPairHolder_mc.visible = brackets;
            diagnosticsRows = [];
            for (var i:int = 0; i < 5; ++i) {
                var source:Object = state.rows[i];
                var row:MovieClip = art.ListItems_mc.getChildByName("ItemText"+i) as MovieClip;
                if (source == null || row == null) return fail("row "+i+" missing");
                var entry:Object = source.entry;
                row.visible = Boolean(source.shown) && entry != null;
                if (!row.visible) continue;
                entry.B21IconRuns = runs[i];
                var selected:Boolean = Boolean(source.selected);
                var label:String = String(entry.text);
                if (label.substr(label.length-2) == " ◆") label = label.substr(0,label.length-2);
                var stars:int = entry.hasOwnProperty("B21LSRank") ? int(entry.B21LSRank) : entry.isLegendary ? 1 : 0;
                if (entry.hasOwnProperty("B21LSName") && entry.B21LSName == label) label = String(entry.B21LSName);
                if (label.charAt(label.length-1) == "★") {
                    stars = 0;
                    while (label.charAt(label.length-1) == "★") { ++stars; label = label.substr(0,label.length-1); }
                    if (label.charAt(label.length-1) == " ") label = label.substr(0,label.length-1);
                }
                stars = Math.max(0,Math.min(5,stars));
                if (int(entry.count) > 1) label += " ("+entry.count+")";
                row.RaritySelector_mc.visible = row.RarityIndicator_mc.visible = row.RarityBorder_mc.visible = false;
                row.conditionMeter.visible = false;
                var health:Number = entry.hasOwnProperty("B21Condition") ? Number(entry.B21Condition) : -1;
                var meter:MovieClip = row.ConditionMeter_mc;
                if (!row.hasOwnProperty("B21MeterScale")) {
                    row.B21MeterScale = meter.scaleX; row.B21MeterX = meter.x; row.B21MeterY = meter.y; row.B21MeterWidth = meter.width;
                }
                meter.visible = isFinite(health) && health >= 0 && health <= 2;
                if (meter.visible) {
                    meter.gotoAndStop(1);
                    // FO76's GlobalFunc.updateConditionMeter: frame 1 is broken, 111-220 run empty to full up
                    // to 100%, and 2-110 add the over-repair overlay up to 200%.
                    var frames:int = meter.MeterClip_mc.totalFrames;
                    meter.MeterClip_mc.gotoAndStop(health <= 0 ? 1 : Math.round(frames-(frames-2)/2*Math.min(2,health)));
                    meter.MeterClip_mc.transform.colorTransform = selected ? new ColorTransform(0,0,0,1) : GOLD_TO_CREAM;
                }
                if (!row.hasOwnProperty("B21TextWidth")) row.B21TextWidth = row.ItemName_tf.width;
                if (!row.hasOwnProperty("B21TextX")) row.B21TextX = row.ItemName_tf.x;
                var rowWidth:Number = Number(row.B21TextWidth)+grow;
                stretch(row.SelectionIndicator_mc,wide);
                if (meter.visible && cnd > 0) {
                    meter.scaleX = meter.scaleY = Number(row.B21MeterScale)*Math.min(1,(cnd-metrics.gap)/Number(row.B21MeterWidth));
                    var meterBox:Rectangle = meter.getBounds(row);
                    meter.x += row.B21TextX + rowWidth - span - cnd + (cnd-meterBox.width)/2 - meterBox.x;
                    meter.y += row.ItemName_tf.y + (row.ItemName_tf.height-meterBox.height)/2 - meterBox.y;
                } else {
                    meter.scaleX = meter.scaleY = Number(row.B21MeterScale);
                    meter.x = Number(row.B21MeterX); meter.y = Number(row.B21MeterY);
                }
                row.LegendaryIcon_mc.visible = false;
                row.SelectionIndicator_mc.alpha = selected ? 1 : 0;
                var bonusShape:Sprite = row.getChildByName("B21UniqueBonus") as Sprite;
                if (bonusShape == null) {
                    bonusShape = new Sprite(); bonusShape.name = "B21UniqueBonus"; row.addChild(bonusShape);
                }
                bonusShape.graphics.clear();
                var unique:Boolean = entry.B21Unique == true && entry.B21UniqueIcon is Array;
                if (unique) {
                    bonusShape.graphics.beginFill(0xFFFFFF);
                    for each (var command:Array in entry.B21UniqueIcon) {
                        if (int(command[0]) == 0) bonusShape.graphics.moveTo(Number(command[1]),Number(command[2]));
                        else if (int(command[0]) == 1) bonusShape.graphics.lineTo(Number(command[1]),Number(command[2]));
                        else bonusShape.graphics.curveTo(Number(command[1]),Number(command[2]),Number(command[3]),Number(command[4]));
                    }
                    bonusShape.graphics.endFill();
                    bonusShape.scaleX = bonusShape.scaleY = row.LegendaryIcon_mc.height;
                    bonusShape.y = row.LegendaryIcon_mc.y;
                }
                var icons:Array = [bonusShape,row.FavoriteIcon_mc,row.questItemIcon_mc,row.BetterIcon_mc,row.TaggedForSearchIcon_mc,row.SetBonusIcon_mc];
                var flags:Array = [unique,Boolean(entry.favorite),entry.hasOwnProperty("isQuestItem") && Boolean(entry.isQuestItem),
                    Boolean(entry.isBetterThanEquippedItem),Boolean(entry.taggedForSearch),false];
                row.questItemIcon_mc.gotoAndStop("local");
                var starHolder:MovieClip = row.getChildByName("B21Stars") as MovieClip;
                if (starHolder == null) { starHolder = new MovieClip(); starHolder.name = "B21Stars"; row.addChild(starHolder); }
                while (starHolder.numChildren > stars) starHolder.removeChildAt(starHolder.numChildren-1);
                while (starHolder.numChildren < stars) {
                    var type:Class = getDefinitionByName(getQualifiedClassName(row.LegendaryIcon_mc)) as Class;
                    var star:MovieClip = new type(); star.stop(); starHolder.addChild(star);
                }
                var iconWidth:Number = stars*(row.LegendaryIcon_mc.width+2)+(meter.visible && cnd == 0 ? meter.width : 0);
                for (var j:int = 0; j < icons.length; ++j) if (flags[j]) iconWidth += icons[j].width+2;
                var extras:Sprite = row.getChildByName("B21InventoryExtras") as Sprite;
                if (extras == null) { extras = new Sprite(); extras.name = "B21InventoryExtras"; extras.mouseEnabled = extras.mouseChildren = false; row.addChild(extras); }
                var columnWidth:Number = entry.B21Columns ? span : 0;
                var itemIconWidth:Number = entry.B21Icons ? metrics.icon+metrics.gap : 0;
                row.ItemName_tf.x = row.B21TextX + itemIconWidth;
                row.ItemName_tf.width = Math.max(24,rowWidth-iconWidth-columnWidth-cnd-itemIconWidth);
                var extraSignature:String = row.B21TextX + ":" + rowWidth + ":" + row.ItemName_tf.y + ":" + selected + ":" +
                    entry.B21Columns + ":" + entry.B21Icons + ":" + entry.B21Weight + ":" + entry.B21Value + ":" + entry.B21IconType + ":" + entry.B21Category + ":" + entry.B21IconColors;
                if (!row.hasOwnProperty("B21ExtraSignature") || row.B21ExtraSignature != extraSignature || row.B21ExtraRuns !== entry.B21IconRuns) {
                row.B21ExtraSignature = extraSignature;
                row.B21ExtraRuns = entry.B21IconRuns;
                extras.graphics.clear();
                while (extras.numChildren) extras.removeChildAt(0);
                if (entry.B21Columns)
                    for (var c:int = 0; c < 3; c++) {
                        var value:TextField = new TextField();
                        value.defaultTextFormat = new TextFormat(metrics.font,metrics.columnSize,selected ? 0 : CREAM,false,false,false,null,null,"right");
                        value.text = B21InventoryRules.number(B21InventoryRules.key(entry,c+2),c == 0 ? 2 : c == 2 ? 1 : 0);
                        value.width = metrics.columnWidth; value.height = metrics.columnSize*1.3+4;
                        value.x = row.B21TextX + rowWidth - span + c*metrics.columnWidth; value.y = row.ItemName_tf.y + metrics.baseline;
                        value.mouseEnabled = value.selectable = false; extras.addChild(value);
                    }
                if (entry.B21Icons) B21InventoryIcon.draw(extras,entry,row.B21TextX,row.ItemName_tf.y+(row.ItemName_tf.height-metrics.icon)/2,
                    metrics.icon,metrics.icon,selected ? 0 : CREAM);
                }
                var nameSize:Number = Number(row.ItemName_tf.defaultTextFormat.size);
                var nameKey:String = label + ":" + row.ItemName_tf.width + ":" + nameSize;
                if (row.B21NameKey != nameKey) { row.B21NameKey = nameKey; clip(row.ItemName_tf,label,nameSize); }
                row.ItemName_tf.textColor = selected ? 0 : CREAM;
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
            var weight:Number = Number(state.carryWeight);
            var maximum:Number = Number(state.maxCarryWeight);
            art.WeightText_mc.visible = art.WeightIcon_mc.visible = isFinite(weight) && weight >= 0 && isFinite(maximum) && maximum >= 0;
            if (art.WeightText_mc.visible) {
                art.WeightText_mc.WeightText_tf.text = Math.floor(weight)+"/"+Math.floor(maximum);
                art.WeightText_mc.WeightText_tf.textColor = CREAM;
                art.WeightIcon_mc.gotoAndStop(weight > maximum ? "warning" : "normal");
            }
            drawButtons(state.hints as Array,wide*artScale/2);
            return true;
        }
        public function diagnostics():Object {
            return {rows:diagnosticsRows,frame:art.currentFrame,buttons:buttons.numChildren,
                    title:titleField.text,titleAlign:titleField.getTextFormat().align,
                    weight:art.WeightText_mc.WeightText_tf.text,weightVisible:art.WeightText_mc.visible};
        }
    }
}
