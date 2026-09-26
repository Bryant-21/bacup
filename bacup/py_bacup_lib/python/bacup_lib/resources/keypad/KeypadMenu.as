package
{
    import flash.display.MovieClip;
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class KeypadMenu extends MovieClip
    {
        public var KeyRects_mc:MovieClip;
        public var BGSCodeObj:Object;
        public var NativeInput:Boolean = false;
        public var KeypadVersion:int = 1;
        private var digits:int = 4;
        private var entered:String = "";
        private var selected:int = 0;
        private var pending:Boolean = false;
        private var built:Boolean = false;
        private var readout:TextField;
        private var heading:TextField;
        private var hint:TextField;
        private var buttons:Array = [];
        private var labels:Array = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "DEL", "0", "ENTER"];

        public function KeypadMenu()
        {
            super();
            stop();
            addEventListener(Event.ADDED_TO_STAGE, onAdded);
        }

        private function text(value:String, xPos:Number, yPos:Number, width:Number, size:int):TextField
        {
            var field:TextField = new TextField();
            field.defaultTextFormat = new TextFormat("$MAIN_Font", size, 0xFFFDCD, false, false, false, null, null, "center");
            field.text = value;
            field.x = xPos;
            field.y = yPos;
            field.width = width;
            field.height = size * 1.6;
            field.selectable = false;
            field.mouseEnabled = false;
            addChild(field);
            return field;
        }

        private function onAdded(event:Event):void
        {
            build();
            stage.addEventListener(KeyboardEvent.KEY_DOWN, onKey);
        }

        private function build():void
        {
            if (built || !KeyRects_mc) return;
            built = true;
            graphics.beginFill(0, 0.65);
            graphics.drawRect(0, 0, 1920, 1080);
            graphics.endFill();
            graphics.lineStyle(2, 0xFFFDCD, 0.7);
            graphics.beginFill(0x101714, 0.96);
            graphics.drawRect(610, 65, 700, 950);
            graphics.endFill();
            heading = text("KEYPAD", 650, 90, 620, 38);
            readout = text("", 650, 165, 620, 60);
            hint = text("", 635, 940, 650, 24);
            KeyRects_mc.x = 0;
            KeyRects_mc.y = 0;
            KeyRects_mc.scaleX = KeyRects_mc.scaleY = 1;
            KeyRects_mc.mouseEnabled = false;
            KeyRects_mc.mouseChildren = false;
            for (var i:int = 0; i < 12; ++i)
            {
                var column:int = i % 3;
                var row:int = int(i / 3);
                var xPos:Number = 710 + column * 170;
                var yPos:Number = 280 + row * 158;
                var button:Sprite = new Sprite();
                button.name = String(i);
                button.graphics.lineStyle(1, 0xFFFDCD, 0.25);
                button.graphics.beginFill(0x28382E, 0.95);
                button.graphics.drawRect(0, 0, 140, 140);
                button.graphics.endFill();
                button.x = xPos;
                button.y = yPos;
                button.buttonMode = true;
                button.addEventListener(MouseEvent.MOUSE_OVER, onHover);
                button.addEventListener(MouseEvent.CLICK, onClick);
                addChild(button);
                buttons.push(button);
                text(labels[i], xPos, yPos + 44, 140, i == 9 || i == 11 ? 28 : 40);
                var selector:MovieClip = KeyRects_mc["Selector" + row + column + "_mc"];
                selector.x = xPos;
                selector.y = yPos;
                selector.scaleX = selector.scaleY = 1;
            }
            addChild(KeyRects_mc);
            paint();
        }

        public function Configure(count:int, title:String):void
        {
            build();
            digits = Math.max(1, Math.min(8, count));
            entered = "";
            pending = false;
            heading.text = title.length ? title : "KEYPAD";
            paint();
        }

        private function paint():void
        {
            if (!built) return;
            var display:String = entered;
            while (display.length < digits) display += "_";
            readout.text = display.split("").join(" ");
            hint.text = pending ? "Submitting..." : "Enter: submit    Backspace: delete    Tab / B: exit";
            for (var i:int = 0; i < 12; ++i)
                KeyRects_mc["Selector" + int(i / 3) + (i % 3) + "_mc"].visible = i == selected;
        }

        private function onHover(event:MouseEvent):void
        {
            selected = int(event.currentTarget.name);
            paint();
        }

        private function onClick(event:MouseEvent):void
        {
            selected = int(event.currentTarget.name);
            Navigate("Select");
        }

        public function Navigate(action:String):void
        {
            if (action == "Cancel") { send("cancel", ""); return; }
            if (pending) return;
            if (action == "Up") selected = (selected + 9) % 12;
            else if (action == "Down") selected = (selected + 3) % 12;
            else if (action == "Left") selected = int(selected / 3) * 3 + (selected + 2) % 3;
            else if (action == "Right") selected = int(selected / 3) * 3 + (selected + 1) % 3;
            else if (action == "Select")
            {
                Navigate(selected == 9 ? "Delete" : selected == 11 ? "Submit" : labels[selected]);
                return;
            }
            else if (action == "Delete") entered = entered.substr(0, Math.max(0, entered.length - 1));
            else if (action == "Clear") entered = "";
            else if (action == "Submit")
            {
                if (entered.length == digits) send("submit", entered);
            }
            else if (action.length == 1 && action >= "0" && action <= "9" && entered.length < digits) entered += action;
            paint();
        }

        private function send(action:String, value:String):void
        {
            if (BGSCodeObj && BGSCodeObj.KeypadAction is Function)
            {
                pending = action == "submit";
                BGSCodeObj.KeypadAction(action, value);
            }
        }

        private function onKey(event:KeyboardEvent):void
        {
            if (NativeInput) return;
            var key:int = event.keyCode;
            if (key >= 48 && key <= 57) Navigate(String(key - 48));
            else if (key >= 96 && key <= 105) Navigate(String(key - 96));
            else if (key == 13) Navigate("Submit");
            else if (key == 8) Navigate("Delete");
            else if (key == 46) Navigate("Clear");
            else if (key == 27 || key == 9) Navigate("Cancel");
            else if (key == 37) Navigate("Left");
            else if (key == 38) Navigate("Up");
            else if (key == 39) Navigate("Right");
            else if (key == 40) Navigate("Down");
            else if (key == 32) Navigate("Select");
        }
    }
}
