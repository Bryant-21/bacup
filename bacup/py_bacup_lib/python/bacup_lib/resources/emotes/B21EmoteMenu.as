package {
    import flash.display.MovieClip;
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.MouseEvent;
    import flash.filters.GlowFilter;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.system.ApplicationDomain;
    import flash.text.TextField;
    import flash.text.TextFieldAutoSize;
    import flash.text.TextFormat;
    public class B21EmoteMenu extends MovieClip {
        public static const INNER_SLOTS:int = 12;
        public static const OUTER_SLOTS:int = 16;
        public var BGSCodeObj:Object = {};
        public var EmoteVersion:uint = 3;
        public var expanded:Boolean = false;
        // Collapsed: the category sector. Expanded: the emote's index within its category.
        public var selected:int = 0;
        public var category:int = 0;
        public var windowStart:int = 0;
        public var categories:Array = [];
        private var content:MovieClip;
        private var inner:MovieClip;
        private var outer:MovieClip;
        private var center:MovieClip;
        private var back:MovieClip;
        private var tab:MovieClip;
        private var innerEntries:Array = [];
        private var outerEntries:Array = [];
        private var title:TextField;
        private var titleY:Number;
        private var categoryLabel:TextField;
        private var indicator:MovieClip;
        private var indicatorIcon:String = "";
        private var indicatorValue:TextField;
        private var centerIcon:MovieClip;
        private var centerIconName:String = "";
        private var centerBounds:Rectangle;
        private var hints:B21ButtonHints = new B21ButtonHints();
        private var hintPlate:Sprite = new Sprite();
        private var hudColor:uint = 0xFFFFFF;
        private var gamepad:Boolean = false;
        private var pending:Boolean = false;
        public function B21EmoteMenu() {
            super();
            visible = false;
            content = new RadialMenu();
            content.stop();
            content.scaleX = content.scaleY = 2 / 3;
            addChild(content);
            back = content.Background_mc;
            back.gotoAndStop(5);
            back.mouseEnabled = back.mouseChildren = false;
            inner = content.InnerRing;
            outer = content.OuterRing;
            center = content.CenterInfo_mc;
            center.gotoAndStop(15);
            center.ConditionBar_mc.visible = false;
            centerBounds = center.IconContainer_mc.Body.getBounds(center.IconContainer_mc.Body);
            center.ammoInfo_mc.visible = false;
            center.mouseEnabled = center.mouseChildren = false;
            title = center.emoteTitleText_tf;
            categoryLabel = center.radialCategoryText_tf;
            title.text = categoryLabel.text = "";
            titleY = title.y;
            tab = content.radialTab;
            tab.gotoAndStop(1);
            tab.mouseEnabled = tab.mouseChildren = false;
            SetTab(B21ButtonHints.Translate("$B21_TFA_EmoteTab"));
            Collect(inner, INNER_SLOTS, innerEntries, HoverInner, ClickInner);
            Collect(outer, OUTER_SLOTS, outerEntries, HoverOuter, ClickOuter);
            outer.visible = false;
            hints.name = "EmoteHints";
            hints.x = 640;
            hints.y = 688;
            hintPlate.mouseEnabled = false;
            addChild(hintPlate);
            addChild(hints);
            hints.addEventListener(Event.CHANGE, DrawHintPlate);
            addEventListener(MouseEvent.MOUSE_WHEEL, Scroll);
        }
        private function Collect(ring:MovieClip, count:int, entries:Array, hover:Function, click:Function):void {
            for (var i:int = 0; i < count; i++) {
                var entry:MovieClip = ring.getChildByName("RadialEntry" + i + "_mc") as MovieClip;
                if (entry == null) throw new Error("Emote ring lost RadialEntry" + i + "_mc");
                entry.Configure(i);
                entry.addEventListener(MouseEvent.ROLL_OVER, hover);
                entry.addEventListener(MouseEvent.MOUSE_MOVE, hover);
                entry.addEventListener(MouseEvent.CLICK, click);
                entries.push(entry);
            }
        }
        // FO76 RadialMenu.updateFillWidth: the blade spans the label plus its end caps.
        private function SetTab(value:String):void {
            var field:TextField = tab.tabText_tf;
            field.autoSize = TextFieldAutoSize.LEFT;
            field.text = value;
            tab.ModeBlade_mc.BladeFill_mc.width = field.textWidth + field.textHeight + 70;
        }
        public function SetPlatform(platform:uint, swap:Boolean = false, controller:uint = 0, keyboard:uint = 0):void {
            SetInputMode(platform != 0);
        }
        public function get ControllerMode():Boolean { return gamepad; }
        public function SetInputMode(value:Boolean):void {
            gamepad = value;
            mouseEnabled = mouseChildren = !gamepad;
            RefreshHints();
        }
        public function SetHUDColor(color:uint):void {
            hudColor = color;
            RefreshHints();
        }
        private function AcceptHint():void { Navigate("Accept"); }
        private function ExpandHint():void { Navigate("Expand"); }
        private function CancelHint():void { Navigate("Cancel"); }
        private function RefreshHints():void {
            var usable:Boolean = !pending && Emotes(category).length > 0;
            var rows:Array = [];
            if (gamepad) rows.push({id:"navigate", key:"Xenon_RS", label:"$B21_TFA_PromptSelect"});
            else rows.push({id:"scroll", key:"Mousewheel", label:"$B21_TFA_EmoteScroll"});
            rows.push({id:"accept", key:gamepad ? "Xenon_A" : "ENTER", label:expanded ? "$B21_TFA_EmoteSay" : "$B21_TFA_PromptSelect", enabled:usable, callback:AcceptHint});
            rows.push({id:"expand", key:gamepad ? "Xenon_R1" : "Q", label:expanded ? "$B21_TFA_EmoteCollapse" : "$B21_TFA_EmoteExpand", enabled:usable, callback:ExpandHint});
            rows.push({id:"cancel", key:gamepad ? "Xenon_B" : "TAB", label:expanded ? "$B21_TFA_EmoteBack" : "$B21_TFA_EmoteClose", callback:CancelHint});
            hints.SetHints(rows, hudColor, 1200);
        }
        // FO76 draws its hint bar on a dark translucent plate; bare white text washes out on bright scenes.
        private function DrawHintPlate(event:Event = null):void {
            var bounds:Rectangle = hints.getBounds(this);
            hintPlate.graphics.clear();
            if (bounds.width <= 0) return;
            hintPlate.graphics.beginFill(0x000000, 0.65);
            hintPlate.graphics.drawRect(bounds.x - 14, bounds.y - 5, bounds.width + 28, bounds.height + 10);
            hintPlate.graphics.endFill();
        }
        private function Emotes(index:int):Array {
            return index >= 0 && index < categories.length ? categories[index].emotes : [];
        }
        public function SetData(categoryRows:Array, entryRows:Array):void {
            categories = [];
            var byId:Object = {};
            for (var c:int = 0; c < categoryRows.length && c < INNER_SLOTS; c++) {
                var row:Object = {id:String(categoryRows[c].id), name:String(categoryRows[c].name),
                    icon:String(categoryRows[c].icon), emotes:[]};
                categories.push(row);
                byId[row.id] = row;
            }
            for each (var entry:Object in entryRows) {
                var owner:Object = byId[String(entry.category_id)];
                if (owner != null) owner.emotes.push({source_id:String(entry.source_id), name:String(entry.name), icon:String(entry.icon)});
            }
            for (var i:int = 0; i < INNER_SLOTS; i++) {
                var known:Boolean = i < categories.length;
                innerEntries[i].SetItem(known ? categories[i].icon : "", known && categories[i].emotes.length > 0);
            }
            visible = true;
            content.visible = tab.visible = hints.visible = hintPlate.visible = true;
            pending = false;
            expanded = false;
            windowStart = 0;
            category = 0;
            while (category < categories.length - 1 && Emotes(category).length == 0) category++;
            selected = category;
            Refresh();
        }
        private function Refresh():void {
            var list:Array = Emotes(category);
            outer.visible = expanded;
            for (var i:int = 0; i < INNER_SLOTS; i++) {
                innerEntries[i].SetSelected(i == category);
                innerEntries[i].SetBackerAlpha(expanded ? 0.3 : 0.8);
            }
            if (expanded) {
                // FO76 RadialMenu.processStateUpdate: OuterRing.updateRotation(30 * (inner + 1)).
                outer.rotation = 30 * (category + 1);
                for (var s:int = 0; s < OUTER_SLOTS; s++) {
                    var index:int = windowStart + s;
                    var slot:MovieClip = outerEntries[s];
                    slot.UpdateRotation();
                    slot.SetItem(index < list.length ? list[index].icon : "", index < list.length);
                    slot.SetSelected(index == selected);
                    slot.SetBackerAlpha(0.8);
                }
            }
            var item:Object = expanded ? list[selected] : (category < categories.length ? categories[category] : null);
            title.text = item == null ? "" : String(item.name);
            // The source title sits inside the expanded ring; drop it below the outer sectors.
            title.y = titleY + (expanded ? 75 : 0);
            categoryLabel.text = "";
            ShowCenter(item == null ? "" : String(item.icon));
            RefreshHints();
        }
        private function ShowCenter(icon:String):void {
            if (icon == centerIconName) return;
            if (centerIcon != null && centerIcon.parent != null) centerIcon.parent.removeChild(centerIcon);
            centerIcon = icon.length > 0 ? Artwork(icon, centerBounds.width, centerBounds.height, false) : null;
            centerIconName = icon;
            if (centerIcon != null) center.IconContainer_mc.Body.addChild(centerIcon);
        }
        private static function Artwork(icon:String, width:Number, height:Number, bottom:Boolean):MovieClip {
            var type:Class = ApplicationDomain.currentDomain.getDefinition("B21Emotes_" + icon) as Class;
            var clip:MovieClip = new type() as MovieClip;
            var bounds:Rectangle = clip.getBounds(clip);
            var scale:Number = Math.min(width / Math.max(1, bounds.width), height / Math.max(1, bounds.height));
            clip.scaleX = clip.scaleY = scale;
            clip.x = -(bounds.x + bounds.width / 2) * scale;
            clip.y = -(bottom ? bounds.y + bounds.height : bounds.y + bounds.height / 2) * scale;
            return clip;
        }
        private function Expand(value:Boolean):void {
            if (value && Emotes(category).length == 0) return;
            expanded = value;
            selected = expanded ? 0 : category;
            windowStart = 0;
            Refresh();
        }
        private function SelectCategory(index:int):void {
            if (index < 0 || index >= INNER_SLOTS || index == category) return;
            category = index;
            if (expanded && Emotes(category).length == 0) expanded = false;
            selected = expanded ? 0 : category;
            windowStart = 0;
            Refresh();
        }
        // Moving past either end of a category longer than the ring shifts a 16-wide window.
        private function SelectEmote(index:int):void {
            var count:int = Emotes(category).length;
            if (count == 0) return;
            index = (index % count + count) % count;
            if (index < windowStart) windowStart = index;
            else if (index >= windowStart + OUTER_SLOTS) windowStart = index - OUTER_SLOTS + 1;
            if (index == selected) return;
            selected = index;
            Refresh();
        }
        private function Step(direction:int):void {
            if (expanded) SelectEmote(selected + direction);
            else SelectCategory((category + direction + INNER_SLOTS) % INNER_SLOTS);
        }
        private function HoverInner(event:MouseEvent):void {
            if (gamepad || pending) return;
            SelectCategory(int(event.currentTarget.slot));
        }
        private function ClickInner(event:MouseEvent):void {
            if (gamepad || pending) return;
            var slot:int = int(event.currentTarget.slot);
            if (slot != category) SelectCategory(slot);
            else Expand(!expanded);
        }
        private function HoverOuter(event:MouseEvent):void {
            if (gamepad || pending || !expanded) return;
            var index:int = windowStart + int(event.currentTarget.slot);
            if (index < Emotes(category).length) SelectEmote(index);
        }
        private function ClickOuter(event:MouseEvent):void {
            if (gamepad || !expanded || !event.currentTarget.occupied) return;
            SelectEmote(windowStart + int(event.currentTarget.slot));
            Navigate("Accept");
        }
        private function Scroll(event:MouseEvent):void {
            if (!gamepad && event.delta != 0) Navigate(event.delta > 0 ? "Left" : "Right");
        }
        public function PointAt(dx:Number, dy:Number):void {
            if (dx * dx + dy * dy < 0.09 || pending) return;
            var origin:Point = outer.localToGlobal(new Point(0, 0));
            var entries:Array = expanded ? outerEntries : innerEntries;
            var count:int = expanded ? Math.min(OUTER_SLOTS, Emotes(category).length - windowStart) : Math.min(INNER_SLOTS, categories.length);
            var best:Number = -2;
            var chosen:int = -1;
            for (var i:int = 0; i < count; i++) {
                var icon:MovieClip = entries[i].Icon_mc;
                var p:Point = icon.localToGlobal(new Point(0, 0)).subtract(origin);
                var dot:Number = (p.x * dx + p.y * dy) / Math.max(0.001, p.length);
                if (dot > best) { best = dot; chosen = i; }
            }
            if (chosen < 0) return;
            if (expanded) SelectEmote(windowStart + chosen);
            else SelectCategory(chosen);
        }
        public function Navigate(action:String):void {
            if (action == "Cancel") {
                if (expanded && !pending) Expand(false);
                else if (BGSCodeObj.EmoteAction != null) BGSCodeObj.EmoteAction("close", "");
                return;
            }
            if (pending || categories.length == 0) return;
            if (action == "Expand") Expand(!expanded);
            else if (action == "Accept") {
                if (!expanded) Expand(true);
                else if (BGSCodeObj.EmoteAction != null && Emotes(category).length > 0) {
                    pending = true;
                    RefreshHints();
                    BGSCodeObj.EmoteAction("play", String(Emotes(category)[selected].source_id));
                }
            }
            else if (action == "Left" || action == "Up") Step(-1);
            else if (action == "Right" || action == "Down") Step(1);
        }
        // Overhead bubble, driven every frame by the native indicator menu.
        public function Present(icon:String, sx:Number, sy:Number, value:String = ""):void {
            visible = true;
            content.visible = tab.visible = hints.visible = hintPlate.visible = false;
            mouseEnabled = mouseChildren = false;
            if (indicator == null) {
                indicator = new MovieClip();
                indicator.name = "Indicator";
                indicatorValue = new TextField();
                indicatorValue.name = "IndicatorValue";
                indicatorValue.defaultTextFormat = new TextFormat("$MAIN_Font_Bold", 30, 0xFFFFFF, true);
                indicatorValue.autoSize = TextFieldAutoSize.CENTER;
                indicatorValue.selectable = false;
                indicatorValue.mouseEnabled = false;
                // The dice number sits on the icon art, as FO76's DiceValue_mc does; the outline keeps it legible.
                indicatorValue.filters = [new GlowFilter(0x000000, 1, 5, 5, 6)];
                indicator.addChild(indicatorValue);
                addChild(indicator);
            }
            if (icon != indicatorIcon) {
                if (indicator.numChildren > 1) indicator.removeChildAt(0);
                indicator.addChildAt(Artwork(icon, 100, 100, true), 0);
                indicatorIcon = icon;
            }
            if (indicatorValue.text != value) {
                indicatorValue.text = value;
                indicatorValue.x = -indicatorValue.width / 2;
                indicatorValue.y = -50 - indicatorValue.height / 2;
            }
            indicatorValue.visible = value.length > 0;
            x = sx;
            y = sy;
        }
    }
}
