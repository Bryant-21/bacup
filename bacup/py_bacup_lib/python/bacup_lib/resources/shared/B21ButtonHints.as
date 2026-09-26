package {
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.MouseEvent;
    import flash.filters.DropShadowFilter;
    import flash.geom.ColorTransform;
    import flash.geom.Rectangle;
    import flash.text.AntiAliasType;
    import flash.text.TextField;
    import flash.text.TextFieldAutoSize;
    import flash.text.TextFormat;

    // Rebuilds FO4's Shared.AS3.BSButtonHint (ButtonBarMenu.swf) inside the host movie. A
    // Loader-loaded copy of the stock component renders nothing in game: FO4 GFx gives child
    // movies neither the fontconfig aliases nor vector art. Geometry and glyph tables below are
    // the decompiled stock values; keep them in sync with the 1.11 component.
    public class B21ButtonHints extends Sprite {
        private static const DISABLED_ALPHA:Number = 0.5;
        private static const HINT_SPACING:Number = 20;
        private static const ICON_Y:Number = 3.5;
        private static const ICON_INSET:Number = 2;
        private static const LABEL_Y:Number = 5.25;
        private static const PC_KEY_Y:Number = -1.5;
        private static const GLYPHS:Object = {
            Xenon_A:"A", Xenon_B:"B", Xenon_X:"C", Xenon_Y:"D", Xenon_Select:"E", Xenon_LS:"F",
            Xenon_L1:"G", Xenon_L3:"H", Xenon_L2:"I", Xenon_L2R2:"J", Xenon_RS:"K", Xenon_R1:"L",
            Xenon_R3:"M", Xenon_R2:"N", Xenon_Start:"O", _Positive:"P", _Negative:"Q", _Question:"R",
            _Neutral:"S", Left:"T", Right:"U", Down:"V", Up:"W", Xenon_R2_Alt:"X", Xenon_L2_Alt:"Y",
            PSN_A:"a", PSN_Y:"b", PSN_X:"c", PSN_B:"d", PSN_Select:"z", PSN_L3:"f", PSN_L1:"g",
            PSN_L1R1:"h", PSN_LS:"i", PSN_L2:"j", PSN_L2R2:"k", PSN_R3:"l", PSN_R1:"m", PSN_RS:"n",
            PSN_R2:"o", PSN_Start:"p", _DPad_LR:"q", _DPad_UD:"r", _DPad_Left:"t", _DPad_Right:"u",
            _DPad_Down:"v", _DPad_Up:"w", PSN_R2_Alt:"x", PSN_L2_Alt:"y"};

        private var entries:Array = [];
        private var disabled:Object = {};
        private var callbacks:Object = {};
        private var tint:uint = 0xFFFFFF;
        private var availableWidth:Number = 1200;
        private var signature:String = "";
        private var pipboy:Boolean;
        public var ready:Boolean = true;

        public function B21ButtonHints(pipboyStyle:Boolean = false) {
            pipboy = pipboyStyle;
        }

        public function SetHints(value:Array, color:uint = 0xFFFFFF, maxWidth:Number = 1200):void {
            var key:String = String(color) + ":" + String(maxWidth);
            for each (var entry:Object in value)
                key += "|" + entry.id + ":" + entry.key + ":" + entry.label + ":" + entry.enabled + ":" + entry.color;
            entries = value == null ? [] : value;
            if (key == signature) return;
            signature = key;
            tint = color;
            availableWidth = maxWidth - (pipboy ? 13.1 : 0);
            draw();
        }

        public function ActionAt(x:Number, y:Number):String {
            for (var r:int = 0; r < numChildren; r++) {
                var row:Sprite = getChildAt(r) as Sprite;
                for (var i:int = 0; i < row.numChildren; i++) {
                    var hint:Sprite = row.getChildAt(i) as Sprite;
                    if (hint.visible && disabled[hint.name] != true && hint.getBounds(parent).contains(x, y))
                        return hint.name;
                }
            }
            return "";
        }

        // GFx translates a $ key when it is assigned; reading it back yields the plain string, which
        // is also what vanilla GlobalFunc.SetText uppercases.
        public static function Translate(key:String):String {
            if (key.charAt(0) != "$") return key;
            var scratch:TextField = new TextField();
            scratch.defaultTextFormat = new TextFormat("$MAIN_Font", 18, 0xFFFFFF);
            scratch.text = key;
            return scratch.text;
        }

        private static function Field(font:String, size:Number, bold:Boolean):TextField {
            var field:TextField = new TextField();
            field.defaultTextFormat = new TextFormat(font, size, 0xFFFFFF, bold);
            field.autoSize = TextFieldAutoSize.LEFT;
            field.antiAliasType = AntiAliasType.NORMAL;
            field.selectable = false;
            field.mouseEnabled = false;
            return field;
        }

        private static function Icon(key:String, enabled:Boolean):TextField {
            var glyph:String = GLYPHS[key];
            var field:TextField = glyph != null ? Field("$Controller_Buttons_inverted", 20, false) : Field("$MAIN_Font", 20, false);
            field.text = glyph != null ? glyph : key + ")";
            field.x = ICON_INSET;
            field.y = ICON_Y + (glyph != null ? 0 : PC_KEY_Y);
            field.alpha = enabled ? 1 : DISABLED_ALPHA;
            return field;
        }

        private function Hint(entry:Object):Sprite {
            var enabled:Boolean = entry.enabled != false;
            var keys:Array = String(entry.key).split(" / ");
            var hint:Sprite = new Sprite();
            hint.mouseChildren = false;
            var color:uint = entry.color == null ? tint : uint(entry.color);
            hint.transform.colorTransform = new ColorTransform(
                (color >> 16 & 255) / 255, (color >> 8 & 255) / 255, (color & 255) / 255);
            if (pipboy) hint.filters = [new DropShadowFilter(2, 45, 0, 1, 2, 2, 1, 1)];
            var icon:TextField = Icon(String(keys[0]), enabled);
            hint.addChild(icon);
            var label:TextField = Field("$MAIN_Font_Bold", 18, true);
            // A $ key translated by GFx in this aliased font drew as missing glyphs in game (09-23),
            // while plain text in the same field rendered, so keys are resolved off-screen first.
            label.text = Translate(String(entry.label)).toUpperCase();
            label.x = icon.width;
            label.y = LABEL_Y;
            label.alpha = enabled ? 1 : DISABLED_ALPHA;
            hint.addChild(label);
            if (keys.length > 1) {
                var secondary:TextField = Icon(String(keys[1]), enabled);
                secondary.x = label.x + label.width + ICON_INSET;
                hint.addChild(secondary);
            }
            return hint;
        }

        private function clicked(event:MouseEvent):void {
            var name:String = Sprite(event.currentTarget).name;
            var callback:Function = callbacks[name] as Function;
            if (callback != null && disabled[name] != true) callback.call(null);
        }

        private function draw():void {
            while (numChildren > 0) removeChildAt(0);
            disabled = {};
            callbacks = {};
            var nextX:Number = 0;
            var nextY:Number = 0;
            var lineHeight:Number = 0;
            var rows:Array = [];
            var row:Sprite = new Sprite();
            addChild(row);
            rows.push(row);
            for each (var entry:Object in entries) {
                var hint:Sprite = Hint(entry);
                hint.name = entry.id == null ? String(row.numChildren) : String(entry.id);
                if (entry.enabled == false) disabled[hint.name] = true;
                if (entry.callback is Function) {
                    callbacks[hint.name] = entry.callback;
                    hint.addEventListener(MouseEvent.CLICK, clicked);
                }
                var bounds:Rectangle = hint.getBounds(hint);
                if (nextX > 0 && nextX + bounds.width > availableWidth) {
                    nextY += lineHeight + 8;
                    row = new Sprite();
                    row.y = nextY;
                    addChild(row);
                    rows.push(row);
                    nextX = 0;
                    lineHeight = 0;
                }
                row.addChild(hint);
                hint.x = nextX - bounds.x;
                hint.y = -bounds.y;
                nextX += bounds.width + HINT_SPACING;
                lineHeight = Math.max(lineHeight, bounds.height);
            }
            for each (var line:Sprite in rows) {
                var width:Number = line.width;
                if (pipboy && line.numChildren > 0) {
                    // Stock ButtonBarMenu brackets are 6.55 x 32 with 2 px strokes.
                    var y:Number = (line.height - 32) / 2;
                    var background:uint = uint((tint >> 16 & 255) * 56 / 255) << 16 |
                        uint((tint >> 8 & 255) * 56 / 255) << 8 | uint((tint & 255) * 56 / 255);
                    line.graphics.beginFill(background, 0.5);
                    line.graphics.drawRect(-6.55, y, width + 13.1, 32);
                    line.graphics.endFill();
                    line.graphics.beginFill(tint);
                    line.graphics.drawRect(-6.55, y, 2, 32);
                    line.graphics.drawRect(-4.55, y, 4.55, 2);
                    line.graphics.drawRect(-4.55, y + 30, 4.55, 2);
                    line.graphics.drawRect(width + 4.55, y, 2, 32);
                    line.graphics.drawRect(width, y, 4.55, 2);
                    line.graphics.drawRect(width, y + 30, 4.55, 2);
                    line.graphics.endFill();
                }
                line.x = -width / 2;
            }
            dispatchEvent(new Event(Event.CHANGE));
        }
    }
}
