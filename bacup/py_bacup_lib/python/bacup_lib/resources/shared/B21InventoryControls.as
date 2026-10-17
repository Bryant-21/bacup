package
{
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.MouseEvent;
    import flash.events.KeyboardEvent;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.text.TextField;
    import flash.text.TextFieldType;
    import flash.text.TextFormat;

    public class B21InventoryControls extends Sprite
    {
        private static const LABELS:Array = ["WT", "VAL", "V/W"];
        private var list:Object;
        private var search:TextField;
        private var heading:Sprite;
        private var color:uint = 0xffffff;
        private var rows:Array = [];
        private var searchEnabled:Boolean;
        private var headingSignature:String = "";
        private var searchSignature:String = "";
        private var metrics:Object;
        private var metricsSignature:String = "";
        private var layoutVersion:uint;
        public var editing:Boolean;

        public function B21InventoryControls(owner:Object)
        {
            list = owner;
            heading = new Sprite();
            addChild(heading);
            search = field("", 120, 14, null);
            search.type = TextFieldType.DYNAMIC;
            search.maxChars = 96;
            search.visible = false;
            search.addEventListener(Event.CHANGE, changed);
            search.addEventListener(MouseEvent.CLICK, focusSearch);
            search.addEventListener(KeyboardEvent.KEY_DOWN, searchKey);
            addChild(search);
        }

        private function field(value:String, width:Number, size:Number, font:String, align:String = "left"):TextField
        {
            var result:TextField = new TextField();
            result.defaultTextFormat = new TextFormat(font == null ? "$MAIN_Font" : font, size, color, false, false, false, null, null, align);
            result.text = value;
            result.width = width;
            result.height = size * 1.3 + 4;
            result.selectable = false;
            result.mouseEnabled = false;
            return result;
        }

        // Sizes follow the stock row instead of stage pixels: GFx renders the Pip-Boy page into its
        // own target, so the stage scale does not say how large a row's text appears.
        private function measure(row:Object):Object
        {
            var bounds:Rectangle = row.border.getBounds(row);
            var format:TextFormat = row.textField.defaultTextFormat;
            var size:Number = Number(format.size);
            if (!(size > 0)) size = bounds.height * 0.55;
            var columnSize:Number = Math.round(size * 0.72);
            var probe:TextField = field("000.0", 400, columnSize, format.font);
            var name:TextField = field("Wg", 400, size, format.font);
            return {font:format.font, size:size, columnSize:columnSize,
                columnWidth:Math.ceil(probe.textWidth + columnSize * 0.45),
                headingSize:Math.max(8, Math.round(size * 0.62)),
                icon:Math.round(Math.min(bounds.height * 0.78, size * 1.15)),
                gap:Math.round(size * 0.3),
                // Clears the selection chevron the converted container rows draw inside their border.
                right:bounds.right - Math.round(size * 0.9),
                baseline:name.getLineMetrics(0).ascent - probe.getLineMetrics(0).ascent};
        }

        public function configure(tint:uint, allowSearch:Boolean):void
        {
            color = tint;
            searchEnabled = allowSearch;
            if (!list.B21Enabled) { visible = false; if (editing) endSearch(false); paintRows(); return; }
            visible = true;
            var row:Object = list.GetClipByIndex(0);
            if (row == null || row.border == null || row.textField == null) return;
            // Headings and search sit above the first row in this sprite's space; the row may be scaled.
            var bounds:Rectangle = row.border.getBounds(row);
            var origin:Point = globalToLocal(row.localToGlobal(new Point(bounds.x, bounds.y)));
            var unit:Number = (globalToLocal(row.localToGlobal(new Point(bounds.x + 100, bounds.y))).x - origin.x) / 100;
            if (!(unit > 0)) unit = 1;
            var geometry:String = bounds.x + ":" + bounds.y + ":" + bounds.width + ":" + bounds.height + ":" +
                row.textField.defaultTextFormat.size + ":" + origin.x + ":" + origin.y + ":" + unit + ":" + list.B21HeadingCenter;
            if (metricsSignature != geometry)
            {
                metricsSignature = geometry;
                metrics = measure(row);
                layoutVersion++;
            }
            var band:Number = (metrics.headingSize * 1.5 + 6) * unit;
            var top:Number = list.B21HeadingCenter > 0 ? origin.y - list.B21HeadingCenter - band / 2 : origin.y - band - 2 * unit;
            var columnsLeft:Number = origin.x + (metrics.right - 3 * metrics.columnWidth - bounds.x) * unit;
            var signature:String = layoutVersion + ":" + color + ":" + list.B21Columns + ":" + list.B21Mode + ":" + list.B21Descending;
            if (headingSignature != signature)
            {
                headingSignature = signature;
                while (heading.numChildren) heading.removeChildAt(0);
                if (list.B21Columns) for (var c:int = 0; c < 3; c++)
                {
                    var width:Number = metrics.columnWidth * unit;
                    var button:Sprite = new Sprite();
                    button.x = columnsLeft + c * width;
                    button.y = top;
                    button.name = String(c + 2);
                    button.buttonMode = true;
                    button.graphics.beginFill(color, 0.01);
                    button.graphics.drawRect(0, 0, width, band);
                    button.graphics.endFill();
                    var label:TextField = field(LABELS[c], width, metrics.headingSize * unit, metrics.font, "right");
                    label.height = band;
                    label.y = (band - label.textHeight - 4) / 2;
                    button.addChild(label);
                    button.addEventListener(MouseEvent.CLICK, sortClick);
                    heading.addChild(button);
                    if (list.B21Mode == c + 2)
                    {
                        var arrow:Number = metrics.headingSize * unit * 0.5;
                        var right:Number = width - label.textWidth - 4 - arrow * 0.5;
                        var middle:Number = band / 2;
                        button.graphics.beginFill(color);
                        button.graphics.moveTo(right - arrow, middle - arrow * (list.B21Descending ? 0.3 : -0.3));
                        button.graphics.lineTo(right, middle - arrow * (list.B21Descending ? 0.3 : -0.3));
                        button.graphics.lineTo(right - arrow / 2, middle + arrow * (list.B21Descending ? 0.3 : -0.3));
                        button.graphics.endFill();
                    }
                }
            }
            var nextSearch:String = signature + ":" + searchEnabled + ":" + editing + ":" + list.B21Query;
            if (searchEnabled && searchSignature != nextSearch)
            {
                search.visible = true;
                search.mouseEnabled = search.selectable = true;
                search.x = origin.x;
                search.y = top;
                search.width = Math.max(64 * unit, (list.B21Columns ? columnsLeft : origin.x + bounds.width * unit) - origin.x - metrics.gap * unit);
                search.background = true;
                search.backgroundColor = 0x000000;
                search.border = true;
                search.borderColor = color;
                search.textColor = color;
                var searchFormat:TextFormat = search.defaultTextFormat;
                searchFormat.font = metrics.font == null ? "$MAIN_Font" : metrics.font;
                searchFormat.size = metrics.headingSize * unit * 1.15;
                search.defaultTextFormat = searchFormat;
                search.setTextFormat(searchFormat);
                search.height = band;
                if (!editing && !list.B21Query) search.text = "Search...";
                else if (!editing) search.text = list.B21Query;
            }
            else if (!searchEnabled) { search.visible = false; if (editing) endSearch(false); }
            searchSignature = nextSearch;
            paintRows();
        }

        private function sortClick(event:MouseEvent):void
        {
            list.B21Sort(uint(event.currentTarget.name));
            event.stopImmediatePropagation();
        }

        private function focusSearch(event:MouseEvent):void { beginSearch(); event.stopImmediatePropagation(); }
        public function beginSearch():void
        {
            if (!searchEnabled || !stage) return;
            editing = true;
            search.text = list.B21Query == null ? "" : list.B21Query;
            stage.focus = search;
            search.setSelection(search.length, search.length);
            // Native text entry blocks the menu's own keys until editing ends, so a click
            // elsewhere must end it as well.
            stage.addEventListener(MouseEvent.MOUSE_DOWN, outsideClick, true);
        }
        private function outsideClick(event:MouseEvent):void { if (event.target != search) endSearch(false); }
        private function endSearch(clear:Boolean):void
        {
            if (clear) { search.text = ""; list.B21Search(""); }
            editing = false;
            if (stage) { stage.removeEventListener(MouseEvent.MOUSE_DOWN, outsideClick, true); stage.focus = list; }
        }
        private function changed(event:Event):void { list.B21Search(search.text); }
        private function searchKey(event:KeyboardEvent):void
        {
            if (event.keyCode == 27 || event.keyCode == 13) endSearch(event.keyCode == 27);
            event.stopImmediatePropagation();
        }

        public function textInput(value:String):void
        {
            if (!editing) return;
            search.text = (search.text + value).substr(0, 96);
            list.B21Search(search.text);
        }

        public function searchInput(action:String):void
        {
            if (action == "Search") { beginSearch(); return; }
            if (!editing) return;
            if (action == "Backspace") { search.text = search.text.substr(0, Math.max(0, search.length - 1)); list.B21Search(search.text); }
            if (action == "Accept" || action == "Cancel") endSearch(action == "Cancel");
        }

        private function paintRows():void
        {
            var entries:Array = list.B21ViewEntries;
            for (var i:int = 0; i < list.numListItems; i++)
            {
                var row:Object = list.GetClipByIndex(i);
                if (row == null) continue;
                var old:Object = rows[i];
                if (old == null)
                {
                    var layer:Sprite = new Sprite();
                    layer.name = "B21InventoryRow";
                    layer.mouseEnabled = layer.mouseChildren = false;
                    row.addChild(layer);
                    var icon:Sprite = new Sprite();
                    icon.name = "B21InventoryGlyph";
                    layer.addChild(icon);
                    var columns:Array = [];
                    for (var columnIndex:int = 0; columnIndex < 3; columnIndex++)
                    {
                        var columnField:TextField = field("", 36, 15, null, "right");
                        columns.push(columnField);
                        layer.addChild(columnField);
                    }
                    var widthMember:String = "B21TextFieldWidth" in row ? "B21TextFieldWidth" : "BaseTextFieldWidth";
                    old = {layer:layer, icon:icon, columns:columns, field:row.textField,
                        width:row.textField.width, x:row.textField.x, base:row[widthMember], widthMember:widthMember,
                        enabled:false, signature:"", entry:null, runs:null};
                    rows[i] = old;
                }
                old.layer.visible = list.B21Enabled && row.visible;
                if (!list.B21Enabled)
                {
                    if (old.enabled) { old.field.x = old.x; old.field.width = old.width; row[old.widthMember] = old.base; }
                    old.enabled = false;
                    old.signature = "";
                    continue;
                }
                if (metrics == null || !row.visible || row.itemIndex < 0 || row.itemIndex >= entries.length) { old.signature = ""; continue; }
                var entry:Object = entries[row.itemIndex];
                var tint:uint = list.B21ViewIndex == row.itemIndex ? 0x000000 : color;
                var signature:String = layoutVersion + ":" + tint + ":" + list.B21Columns + ":" + list.B21Icons + ":" + entry.text + ":" + entry.count +
                    ":" + entry.B21Weight + ":" + entry.B21Value + ":" + entry.B21IconType + ":" + entry.B21Category + ":" + entry.B21IconColors;
                if (old.signature == signature && old.entry === entry && old.runs === entry.B21IconRuns) continue;
                old.enabled = true;
                old.signature = signature;
                old.entry = entry;
                old.runs = entry.B21IconRuns;
                var m:Object = metrics;
                var left:Number = old.x + (list.B21Icons ? m.icon + m.gap : 0);
                var limit:Number = (list.B21Columns ? m.right - 3 * m.columnWidth : m.right) - m.gap;
                row[old.widthMember] = Math.max(m.size * 2, Math.min(old.base - (left - old.x), limit - left));
                old.field.x = left;
                row.SetEntryText(entry, "Shrink To Fit");
                for (var c:int = 0; c < 3; c++)
                {
                    var column:TextField = old.columns[c];
                    column.visible = list.B21Columns;
                    if (!list.B21Columns) continue;
                    var format:TextFormat = new TextFormat(m.font == null ? "$MAIN_Font" : m.font, m.columnSize, tint, false, false, false, null, null, "right");
                    column.defaultTextFormat = format;
                    column.text = B21InventoryRules.number(B21InventoryRules.key(entry, c + 2), c == 2 ? 1 : c == 0 ? 2 : 0);
                    // Assigning unchanged text keeps the old runs' format (the previous selection tint).
                    column.setTextFormat(format);
                    column.width = m.columnWidth;
                    column.height = m.columnSize * 1.3 + 4;
                    if (column.textWidth > m.columnWidth - 4) { format.size = Math.floor(m.columnSize * (m.columnWidth - 4) / column.textWidth); column.setTextFormat(format); }
                    column.x = m.right - (3 - c) * m.columnWidth;
                    column.y = old.field.y + m.baseline;
                }
                old.icon.visible = list.B21Icons;
                if (list.B21Icons)
                {
                    old.icon.graphics.clear();
                    while (old.icon.numChildren) old.icon.removeChildAt(0);
                    var bounds:Rectangle = row.border.getBounds(row);
                    B21InventoryIcon.draw(old.icon, entry, old.x, bounds.y + (bounds.height - m.icon) / 2, m.icon, m.icon,
                        B21InventoryIcon.colorOf(entry, tint, list.B21ViewIndex == row.itemIndex));
                }
            }
        }
    }
}
