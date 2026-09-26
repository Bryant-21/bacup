package
{
    import flash.display.MovieClip;
    import flash.events.MouseEvent;
    import flash.filters.DropShadowFilter;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.system.ApplicationDomain;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class RadialMenu extends MovieClip
    {
        public var BGSCodeObj:Object = {};
        public var Cross_mc:Object;
        public var WheelVersion:uint = 2;
        public var WeaponIconRevision:uint = 0;
        public var WeaponIconAlpha:Boolean = true;
        public var SafeX:Number = 0;
        public var SafeY:Number = 0;
        public var InnerRing:RadialMenuRingInner;
        public var CenterInfo_mc:MovieClip;
        private var entries:Array = [];
        private var items:Array = [];
        private var icons:Array = [];
        private var weaponIcons:Array = [];
        private var selected:int = -1;
        private var hints:TextField;
        private var gamepad:Boolean = false;
        private var ringOrigin:Point = new Point();
        private var centerIcon:MovieClip;
        private var keywordIcons:Object = {/* BACUP_ICON_KEYWORDS */};

        public function RadialMenu()
        {
            super();
            Cross_mc = this;
            x = 640;
            y = 360;
            var background:MovieClip = new __B21_RADIAL_BACKGROUND_CLASS__();
            background.gotoAndStop(background.totalFrames);
            background.alpha = 0.55;
            background.mouseEnabled = false;
            background.mouseChildren = false;
            addChild(background);
            InnerRing = new RadialMenuRingInner();
            addChild(InnerRing);
            InnerRing.scaleX = 0.95;
            InnerRing.scaleY = 0.95;
            CenterInfo_mc = new __B21_RADIAL_CENTER_CLASS__();
            CenterInfo_mc.gotoAndStop(CenterInfo_mc.totalFrames);
            CenterInfo_mc.scaleX = 0.72;
            CenterInfo_mc.scaleY = 0.72;
            CenterInfo_mc.ConditionBar_mc.visible = false;
            CenterInfo_mc.IconContainer_mc.visible = false;
            CenterInfo_mc.mouseEnabled = false;
            CenterInfo_mc.mouseChildren = false;
            addChild(CenterInfo_mc);
            for (var i:int = 0; i < 12; i++)
            {
                var entry:RadialMenuEntryInner = InnerRing.getChildByName("RadialEntry" + i + "_mc") as RadialMenuEntryInner;
                entry.Configure(i);
                entry.addEventListener(MouseEvent.ROLL_OVER, Hover);
                entry.addEventListener(MouseEvent.MOUSE_MOVE, Hover);
                entry.addEventListener(MouseEvent.CLICK, Click);
                entries.push(entry);
                var point:Point = InnerRing.globalToLocal(entry.localToGlobal(new Point(entry.Icon_mc.x, entry.Icon_mc.y)));
                ringOrigin.x += point.x / 12;
                ringOrigin.y += point.y / 12;
            }
            InnerRing.x = -ringOrigin.x * InnerRing.scaleX;
            InnerRing.y = -ringOrigin.y * InnerRing.scaleY;
            CenterInfo_mc.x = -(CenterInfo_mc.emoteTitleText_tf.x + CenterInfo_mc.emoteTitleText_tf.width / 2) * CenterInfo_mc.scaleX;
            CenterInfo_mc.y = 42 - CenterInfo_mc.emoteTitleText_tf.y * CenterInfo_mc.scaleY;
            CenterInfo_mc.emoteTitleText_tf.width = 340;
            CenterInfo_mc.emoteTitleText_tf.x = -170 - CenterInfo_mc.x / CenterInfo_mc.scaleX;
            CenterInfo_mc.emoteTitleText_tf.wordWrap = true;
            CenterInfo_mc.emoteTitleText_tf.multiline = true;
            CenterInfo_mc.emoteTitleText_tf.height = 90;
            CenterInfo_mc.ammoInfo_mc.y = CenterInfo_mc.emoteTitleText_tf.y + 88;
            var condition:MovieClip = CenterInfo_mc.ConditionBar_mc;
            var conditionBounds:Rectangle = condition.getBounds(condition);
            condition.scaleX = condition.scaleY = 180 / (conditionBounds.width * CenterInfo_mc.scaleX);
            condition.x = -CenterInfo_mc.x / CenterInfo_mc.scaleX - (conditionBounds.x + conditionBounds.width / 2) * condition.scaleX;
            condition.y = (26 - CenterInfo_mc.y) / CenterInfo_mc.scaleY - (conditionBounds.y + conditionBounds.height / 2) * condition.scaleY;
            hints = new TextField();
            hints.defaultTextFormat = new TextFormat("$MAIN_Font", 18, 0xFFFFFF, false, null, null, null, null, "center");
            hints.x = -530;
            hints.name = "ControlsHint";
            hints.y = -310;
            hints.width = 1060;
            hints.height = 35;
            hints.mouseEnabled = false;
            addChild(hints);
            addEventListener(MouseEvent.MOUSE_WHEEL, Scroll);
            SetPlatform(0, false, 0, 0);
            Refresh();
        }

        public function set favInfoArray(value:Array):void
        {
            items = value == null ? [] : value;
            weaponIcons = [];
            WeaponIconRevision++;
            Refresh();
        }

        public function set selectedIndex(value:uint):void { Select(value < 12 ? int(value) : -1); }
        public function get selectedIndex():uint { return selected < 0 ? 0xFFFFFFFF : uint(selected); }
        public function GetEntryClip(index:uint):MovieClip
        {
            return index < entries.length ? entries[index] as MovieClip : null;
        }
        public function SetDetails(value:Array):void
        {
            icons = [];
            for each (var data:Object in value)
                icons.push({icon: ResolveIcon(data), equipped: data.equipped,
                    conditionHealth: data.conditionHealth, conditionDurability: data.conditionDurability});
            Refresh();
        }

        public function SetWeaponIcon(slot:int, runs:Array):void
        {
            if (slot < 0 || slot >= 12) return;
            weaponIcons[slot] = runs != null && runs.length > 0 ? runs : null;
            Refresh();
        }

        public function SetWeaponIcons(value:Array):void
        {
            weaponIcons = value == null ? [] : value;
            Refresh();
        }

        /* BACUP_ICON_RESOLVER */
        public function onSetSafeRect():void {}
        public function SetSafeRect(...args):void {}

        public function SetPlatform(platform:uint, swap:Boolean = false, controller:uint = 0, keyboard:uint = 0):void
        {
            SetInputMode(platform != 0);
        }

        public function get ControllerMode():Boolean { return gamepad; }
        public function SetInputMode(value:Boolean):void
        {
            gamepad = value;
            mouseEnabled = mouseChildren = !gamepad;
            for each (var entry:RadialMenuEntryInner in entries) entry.SetGamepad(gamepad);
            hints.text = gamepad ? "RIGHT STICK / D-PAD: SELECT     A: USE     B: CLOSE" : "MOUSE / ARROWS: SELECT     CLICK / ENTER: USE     TAB: CLOSE";
        }

        private function Refresh():void
        {
            for (var i:int = 0; i < entries.length; i++)
            {
                var data:Object = i < items.length ? items[i] : null;
                if (data != null && i < icons.length) data.equipped = icons[i].equipped;
                entries[i].SetItem(data, i < icons.length ? String(icons[i].icon) : "UnknownIcon",
                    weaponIcons[i] as Array);
            }
            Center();
        }

        private function Center():void
        {
            var item:Object = selected >= 0 && selected < items.length ? items[selected] : null;
            CenterInfo_mc.emoteTitleText_tf.text = item != null ? String(item.text) + (item.count > 1 ? " (" + item.count + ")" : "") : "";
            var title:TextField = CenterInfo_mc.emoteTitleText_tf;
            var format:TextFormat = title.defaultTextFormat;
            title.setTextFormat(format);
            while (Number(format.size) > 18 && title.textHeight > title.height - 4)
            {
                format.size = Number(format.size) - 1;
                title.setTextFormat(format);
            }
            CenterInfo_mc.radialCategoryText_tf.text = item == null ? "FAVORITES" : "";
            CenterInfo_mc.ammoInfo_mc.ammoInfo_tf.text = item != null && item.ammoText != null ? item.ammoText + " (" + item.ammoCount + ")" : "";
            var detail:Object = selected >= 0 && selected < icons.length ? icons[selected] : null;
            var condition:MovieClip = CenterInfo_mc.ConditionBar_mc;
            condition.visible = item != null && detail != null && detail.conditionHealth >= 0 && detail.conditionDurability > 0;
            if (condition.visible)
            {
                var bar:MovieClip = condition.Bar_mc;
                var durability:Number = Math.max(0, Math.min(100, detail.conditionDurability));
                // Fractional frame numbers can be interpreted as labels; match the inspect adapter.
                bar.gotoAndStop(int(bar.totalFrames - (bar.totalFrames - 1) * durability / 100));
                var meter:MovieClip = bar.MeterClip_mc;
                var health:Number = Math.max(0, Math.min(2, detail.conditionHealth));
                meter.gotoAndStop(health > 0 ? int(meter.totalFrames - (meter.totalFrames - 2) * health / 2) : 1);
            }
            if (centerIcon != null) removeChild(centerIcon);
            centerIcon = null;
            if (item != null)
            {
                var icon:String = selected < icons.length ? icons[selected].icon : "UnknownIcon";
                var runs:Array = weaponIcons[selected] as Array;
                if (runs != null && runs.length > 0) centerIcon = RadialMenuEntryInner.DrawStencil(runs);
                else
                {
                    var domain:ApplicationDomain = ApplicationDomain.currentDomain;
                    if (!domain.hasDefinition(icon)) icon = "UnknownIcon";
                    var type:Class = domain.getDefinition(icon) as Class;
                    centerIcon = new type() as MovieClip;
                }
                var bounds:Rectangle = centerIcon.getBounds(centerIcon);
                var scale:Number = Math.min(110 / Math.max(1, bounds.width), 100 / Math.max(1, bounds.height));
                centerIcon.scaleX = scale;
                centerIcon.scaleY = scale;
                centerIcon.x = -(bounds.x + bounds.width / 2) * scale;
                centerIcon.y = -38 - (bounds.y + bounds.height / 2) * scale;
                centerIcon.filters = [new DropShadowFilter(1,45,0,0.8,2,2,1,1,false,false,false)];
                centerIcon.mouseEnabled = false;
                centerIcon.mouseChildren = false;
                addChild(centerIcon);
            }
        }

        private function Select(index:int):void
        {
            if (index < -1 || index >= 12 || index == selected) return;
            if (selected >= 0) entries[selected].SetSelected(false);
            selected = index;
            if (selected >= 0) entries[selected].SetSelected(true);
            Center();
        }

        public function PointAt(horizontal:Number, vertical:Number):void
        {
            if (horizontal * horizontal + vertical * vertical < 0.0625) return;
            // The source has twelve 30-degree sectors, with quickslot 1 at -60 degrees.
            var index:int = (int(Math.floor(Math.atan2(vertical, horizontal) * 6 / Math.PI + 2.5)) + 12) % 12;
            Select(index);
        }

        private function Hover(event:MouseEvent):void
        {
            if (gamepad) return;
            Select(RadialMenuEntryInner(event.currentTarget).slot);
        }

        private function Click(event:MouseEvent):void
        {
            if (gamepad) return;
            Hover(event);
            Use();
        }

        private function Scroll(event:MouseEvent):void
        {
            if (!gamepad && event.delta != 0) Navigate(event.delta > 0 ? "Previous" : "Next");
        }

        public function Navigate(action:String):void
        {
            if (action == "Accept") Use();
            else if (action == "Cancel") { if (BGSCodeObj.closeMenu != null) BGSCodeObj.closeMenu(); }
            else if (action == "Next") Select((selected + 1 + 12) % 12);
            else if (action == "Previous") Select((selected - 1 + 12) % 12);
        }

        public function ProcessUserEvent(action:String, down:Boolean):Boolean
        {
            if (action == "Cancel" || action == "Quickkeys")
            {
                // FO4 opens favorites on release; closing on press lets that release reopen it.
                if (!down) Navigate("Cancel");
                return true;
            }
            if (!down) return false;
            if (action == "Accept" || action == "Activate") Navigate("Accept");
            else if (action == "Right" || action == "Down") Navigate("Next");
            else if (action == "Left" || action == "Up") Navigate("Previous");
            else return false;
            return true;
        }

        private function Use():void
        {
            if (selected >= 0 && selected < items.length && items[selected] != null && items[selected].count > 0 && BGSCodeObj.useQuickkey != null)
                // The native FavoritesMenu callback accepts GFx UInt, not AS3 int.
                BGSCodeObj.useQuickkey(uint(selected));
        }
    }
}
