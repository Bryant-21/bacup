package
{
    import flash.display.DisplayObject;
    import flash.display.MovieClip;
    import flash.display.Shape;
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.MouseEvent;
    import flash.filters.DropShadowFilter;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.system.ApplicationDomain;
    import flash.text.TextField;
    import flash.text.TextFieldAutoSize;
    import flash.text.TextFormat;

    public class RadialMenu extends MovieClip
    {
        // FO76 lays the tab, hint bar and inventory out on a 1920x1080 stage; FO4 menus use 1280x720.
        private static const STAGE_SCALE:Number = 2 / 3;
        // radialInventory frame: the wheel layers drop to alpha 177/256 while the list is open.
        private static const DIM_ALPHA:Number = 0.69140625;
        private static const LIST_ROWS:int = 13;
        private static const LIST_SPACING:Number = 7.5;
        // Hint line top at stage y 688 clears FO4's compass (emote wheel, in game 09-23).
        private static const HINT_TOP:Number = 328;
        private static const HINT_RIGHT:Number = 624;
        // A hint line measures about 30; anything far taller is a bar whose text did not lay out.
        private static const MAX_HINT_LINE:Number = 60;
        private static const HINT_RETRIES:int = 30;
        // A Scaleform CLICK and a native "Click" for the same press must not both act.
        private static const CLICK_WINDOW:Number = 400;

        public var BGSCodeObj:Object = {};
        public var Cross_mc:Object;
        public var WheelVersion:uint = 3;
        public var WeaponIconRevision:uint = 0;
        public var WeaponIconAlpha:Boolean = true;
        public var SafeX:Number = 0;
        public var SafeY:Number = 0;
        public var InnerRing:RadialMenuRingInner;
        public var CenterInfo_mc:MovieClip;
        public var Background_mc:MovieClip;
        public var RingDisk:DisplayObject;
        public var radialTab:MovieClip;
        public var ButtonHintBar_mc:B21ButtonHints;
        public var PlayerInventory_mc:SecureTradePlayerInventory;
        public var HitLayer_mc:Sprite;
        private var entries:Array = [];
        private var items:Array = [];
        private var icons:Array = [];
        private var weaponIcons:Array = [];
        private var selected:int = -1;
        private var gamepad:Boolean = false;
        private var ringOrigin:Point = new Point();
        private var centerIcon:MovieClip;
        private var keywordIcons:Object = {/* BACUP_ICON_KEYWORDS */};
        private var hintPlate:Sprite = new Sprite();
        private var hintLeft:Number = -624;
        private var tentAllowed:Boolean = false;
        private var rebuildHints:Boolean = false;
        private var hintRetries:int = 0;
        private var slotInner:Number = 0;
        private var slotOuter:Number = 0;
        private var listOpen:Boolean = false;
        private var listRows:Array = [];
        private var inventory:Array;
        private var listIndex:int = -1;
        private var listTop:int = 0;
        private var listId:Number = -1;
        private var listMoved:Boolean = false;
        private var noItemsText:String = "";
        private var pointerClickAt:Number = -1e9;
        private var nativeClickAt:Number = -1e9;

        public function RadialMenu()
        {
            super();
            Cross_mc = this;
            x = 640;
            y = 360;
            // The converter keeps FO76's wheel disk shape as this sprite's only timeline child.
            if (numChildren > 0 && getChildAt(0) is Shape) RingDisk = getChildAt(0);
            // Mouse wheel events need an interactive target anywhere on screen to bubble here.
            HitLayer_mc = new Sprite();
            HitLayer_mc.name = "B21WheelHitLayer";
            HitLayer_mc.graphics.beginFill(0, 0);
            HitLayer_mc.graphics.drawRect(-1280, -720, 2560, 1440);
            HitLayer_mc.graphics.endFill();
            addChildAt(HitLayer_mc, 0);
            Background_mc = new __B21_RADIAL_BACKGROUND_CLASS__();
            // Frame 5 ends FO76's roll-on with the filled center disc; its roll-off frames remove it.
            Background_mc.gotoAndStop(5);
            Background_mc.mouseEnabled = false;
            Background_mc.mouseChildren = false;
            addChild(Background_mc);
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
            // FO76 centers both disks on the ring; keep that geometry at the ring's scale.
            CenterOn(RingDisk, InnerRing.scaleX);
            CenterOn(Background_mc, InnerRing.scaleX);
            MeasureSlots();
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
            BuildTab();
            hintPlate.name = "B21HintPlate";
            hintPlate.mouseEnabled = false;
            addChild(hintPlate);
            ButtonHintBar_mc = new B21ButtonHints();
            ButtonHintBar_mc.name = "ButtonHintBar_mc";
            hintLeft = radialTab.ModeBlade_mc.BladeFill_mc.getBounds(this).right + 16;
            ButtonHintBar_mc.addEventListener(Event.CHANGE, LayoutHints);
            addChild(ButtonHintBar_mc);
            addEventListener(Event.ENTER_FRAME, RetryHints);
            BuildInventory();
            addEventListener(MouseEvent.MOUSE_WHEEL, Scroll);
            SetPlatform(0, false, 0, 0);
            Refresh();
        }

        private static function CenterOn(clip:DisplayObject, scale:Number):void
        {
            if (clip == null) return;
            var bounds:Rectangle = clip.getBounds(clip);
            clip.scaleX = scale;
            clip.scaleY = scale;
            clip.x = -(bounds.x + bounds.width / 2) * scale;
            clip.y = -(bounds.y + bounds.height / 2) * scale;
        }

        private function MeasureSlots():void
        {
            slotInner = 1e9;
            for each (var entry:RadialMenuEntryInner in entries)
            {
                var bounds:Rectangle = entry.HitArea_mc.getBounds(this);
                var nearX:Number = Math.max(bounds.left, Math.min(0, bounds.right));
                var nearY:Number = Math.max(bounds.top, Math.min(0, bounds.bottom));
                slotInner = Math.min(slotInner, Math.sqrt(nearX * nearX + nearY * nearY));
                var farX:Number = Math.max(Math.abs(bounds.left), Math.abs(bounds.right));
                var farY:Number = Math.max(Math.abs(bounds.top), Math.abs(bounds.bottom));
                slotOuter = Math.max(slotOuter, Math.sqrt(farX * farX + farY * farY));
            }
        }

        private function BuildTab():void
        {
            radialTab = new __B21_RADIAL_TAB_CLASS__();
            radialTab.gotoAndStop(1);
            radialTab.mouseEnabled = false;
            radialTab.mouseChildren = false;
            radialTab.scaleX = STAGE_SCALE;
            radialTab.scaleY = STAGE_SCALE;
            // FO76 places the tab at (0.9, 990.5) on its 1920x1080 stage.
            radialTab.x = -640 + 0.9 * STAGE_SCALE;
            radialTab.y = -360 + 990.5 * STAGE_SCALE;
            var field:TextField = radialTab.tabText_tf;
            field.autoSize = TextFieldAutoSize.LEFT;
            field.text = B21ButtonHints.Translate("$FAVORITES");
            // FO76 RadialMenu.updateFillWidth: the blade spans the label plus its end caps.
            radialTab.ModeBlade_mc.BladeFill_mc.width = field.textWidth + field.textHeight + 70;
            addChild(radialTab);
        }

        private function BuildInventory():void
        {
            noItemsText = B21ButtonHints.Translate("$NO_ITEMS");
            PlayerInventory_mc = new SecureTradePlayerInventory();
            PlayerInventory_mc.name = "PlayerInventory_mc";
            PlayerInventory_mc.visible = false;
            // The panel's first child is its backing; the header's text box reaches past it.
            var panel:Rectangle = PlayerInventory_mc.getChildAt(0).getBounds(PlayerInventory_mc);
            if (PlayerInventory_mc.Header_mc != null)
                PlayerInventory_mc.Header_mc.Header_tf.text = B21ButtonHints.Translate("$QUICKINVENTORY");
            // FO76 anchors its ItemList_mc here inside the panel; the converter drops that list.
            var holder:MovieClip = new MovieClip();
            holder.name = "B21ListRows";
            holder.x = 176.3;
            holder.y = 253.2;
            PlayerInventory_mc.addChild(holder);
            var pitch:Number = 0;
            var available:Number = panel.bottom - holder.y;
            for (var i:int = 0; i < LIST_ROWS; i++)
            {
                var row:PlayerListEntry = new PlayerListEntry();
                if (pitch == 0) pitch = row.RowHeight() + LIST_SPACING;
                if (i > 0 && (i + 1) * pitch - LIST_SPACING > available) break;
                row.RowIndex = i;
                row.y = i * pitch;
                row.visible = false;
                row.addEventListener(MouseEvent.ROLL_OVER, RowHover);
                row.addEventListener(MouseEvent.MOUSE_MOVE, RowHover);
                row.addEventListener(MouseEvent.CLICK, RowClick);
                holder.addChild(row);
                listRows.push(row);
            }
            PlayerInventory_mc.scaleX = STAGE_SCALE;
            PlayerInventory_mc.scaleY = STAGE_SCALE;
            var ring:Rectangle = InnerRing.getBounds(this);
            var left:Number = Math.min(ring.right + 24, 624 - panel.width * STAGE_SCALE);
            PlayerInventory_mc.x = left - panel.x * STAGE_SCALE;
            PlayerInventory_mc.y = -(panel.y + panel.height / 2) * STAGE_SCALE;
            addChild(PlayerInventory_mc);
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
        public function get InventoryOpen():Boolean { return listOpen; }
        public function get InventoryIndex():int { return listIndex; }
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

        public function SetActions(value:Object):void
        {
            tentAllowed = value != null && value.tent == true;
            RefreshHints();
        }

        public function SetInventory(rows:Array):void
        {
            var sorted:Array = rows == null ? [] : rows.concat();
            sorted.sort(CompareRows);
            inventory = sorted;
            if (listOpen && !listMoved) PickSlotRow();
            else
            {
                listIndex = -1;
                for (var i:int = 0; i < sorted.length; i++)
                    if (Number(sorted[i].id) == listId) listIndex = i;
                if (listIndex < 0 && sorted.length > 0) listIndex = 0;
            }
            if (listOpen) RenderList();
            RefreshHints();
        }

        private static function CompareRows(a:Object, b:Object):int
        {
            var left:String = String(a.text).toLowerCase();
            var right:String = String(b.text).toLowerCase();
            if (left < right) return -1;
            if (left > right) return 1;
            return Number(a.id) < Number(b.id) ? -1 : Number(a.id) > Number(b.id) ? 1 : 0;
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
            RefreshHints();
        }

        private function Occupied(index:int):Boolean
        {
            return index >= 0 && index < items.length && items[index] != null && items[index].count > 0;
        }

        private function TentAvailable():Boolean { return tentAllowed && BGSCodeObj.placeSurvivalTent != null; }
        private function ChangeAvailable():Boolean
        {
            return BGSCodeObj.requestInventory != null && BGSCodeObj.assignQuickkey != null;
        }
        private function InspectAvailable():Boolean { return BGSCodeObj.inspectQuickkey != null && Occupied(selected); }

        // FO76 RadialMenu.updateButtonBar for the favorites wheel, in its populateButtonBar order.
        private function RefreshHints():void
        {
            if (ButtonHintBar_mc == null) return;
            var rows:Array = [];
            if (!listOpen && TentAvailable())
                rows.push({id:"tent", key:gamepad ? "Xenon_Y" : "T", label:"$B21_TFA_SurvivalTent", callback:TentHint});
            if (!listOpen && !gamepad) rows.push({id:"scroll", key:"Mousewheel", label:"$SCROLL"});
            rows.push({id:"select", key:gamepad ? "Xenon_RS" : "Space", label:"$SELECT"});
            rows.push({id:"use", key:gamepad ? "Xenon_A" : "Space", label:"$RADIAL_MENU_USE", callback:UseHint});
            if (!listOpen && ChangeAvailable())
                rows.push({id:"change", key:gamepad ? "Xenon_R1" : "C", label:"$CHANGE", callback:ChangeHint});
            if (!listOpen && InspectAvailable())
            {
                var detail:Object = selected < icons.length ? icons[selected] : null;
                var repairable:Boolean = detail != null && detail.conditionHealth >= 0 && detail.conditionDurability > 0;
                rows.push({id:"inspect", key:gamepad ? "Xenon_R3" : "X", label:repairable ? "$INSPECT/REPAIR" : "$INSPECT",
                    callback:InspectHint});
            }
            // FO76 has no visible way back from its list; Tales adds one so a mouse alone can leave it.
            if (listOpen) rows.push({id:"back", key:gamepad ? "Xenon_B" : "Tab", label:"$BACK", callback:BackHint});
            ButtonHintBar_mc.SetHints(rows, 0xFFFFFF, HINT_RIGHT - hintLeft);
        }

        // In game (10-04) a freshly built bar sometimes drew no text yet measured about 160 x 350, and
        // its plate became a grey box over the wheel. Such a bar stays hidden and is rebuilt next frame.
        private function HintsMeasured(bar:B21ButtonHints):Boolean
        {
            var rows:int = bar.numChildren;
            var line:Number = rows > 0 ? bar.getChildAt(0).height : 0;
            return isFinite(line) && isFinite(bar.width) && isFinite(bar.height) && rows <= 3 &&
                line <= MAX_HINT_LINE && bar.height <= rows * (line + 8) && bar.width <= HINT_RIGHT - hintLeft + 1;
        }

        private function RetryHints(event:Event):void
        {
            if (!rebuildHints) return;
            rebuildHints = false;
            ButtonHintBar_mc.Invalidate();
            RefreshHints();
        }

        // Bottom center like FO76, right of the FAVORITES tab. Extra lines grow upward from the bottom line.
        private function LayoutHints(event:Event = null):void
        {
            var bar:B21ButtonHints = ButtonHintBar_mc;
            hintPlate.graphics.clear();
            bar.visible = HintsMeasured(bar);
            if (!bar.visible)
            {
                hintRetries++;
                rebuildHints = hintRetries <= HINT_RETRIES;
                return;
            }
            hintRetries = 0;
            if (bar.numChildren > 0) bar.y = HINT_TOP + bar.getChildAt(0).height - bar.height;
            var half:Number = bar.width / 2;
            bar.x = Math.min(HINT_RIGHT - half, Math.max(0, hintLeft + half));
            // FO76 draws its hint bar on a dark translucent plate; bare text washes out on bright scenes.
            var bounds:Rectangle = bar.getBounds(this);
            if (bounds.width <= 0) return;
            hintPlate.graphics.beginFill(0x000000, 0.65);
            hintPlate.graphics.drawRect(bounds.x - 14, bounds.y - 5, bounds.width + 28, bounds.height + 10);
            hintPlate.graphics.endFill();
        }

        private function HintAction(id:String):void
        {
            if (id == "tent") Navigate("Tent");
            else if (id == "use") Navigate("Accept");
            else if (id == "change") Navigate("Change");
            else if (id == "inspect") Navigate("Inspect");
            else if (id == "back") Navigate("Cancel");
        }
        private function TentHint():void { if (PointerClick()) HintAction("tent"); }
        private function UseHint():void { if (PointerClick()) HintAction("use"); }
        private function ChangeHint():void { if (PointerClick()) HintAction("change"); }
        private function InspectHint():void { if (PointerClick()) HintAction("inspect"); }
        private function BackHint():void { if (PointerClick()) HintAction("back"); }

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
                centerIcon.alpha = listOpen ? DIM_ALPHA : 1;
                addChild(centerIcon);
            }
            RefreshHints();
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
            if (listOpen) return;
            if (horizontal * horizontal + vertical * vertical < 0.0625) return;
            Select(Sector(horizontal, vertical));
        }

        // The source has twelve 30-degree sectors, with quickslot 1 at -60 degrees.
        private static function Sector(horizontal:Number, vertical:Number):int
        {
            return (int(Math.floor(Math.atan2(vertical, horizontal) * 6 / Math.PI + 2.5)) + 12) % 12;
        }

        private function SlotAt(px:Number, py:Number):int
        {
            var radius:Number = Math.sqrt(px * px + py * py);
            return radius < slotInner || radius > slotOuter ? -1 : Sector(px, py);
        }

        private function Hover(event:MouseEvent):void
        {
            if (gamepad || listOpen) return;
            Select(RadialMenuEntryInner(event.currentTarget).slot);
        }

        private function Click(event:MouseEvent):void
        {
            if (gamepad || listOpen || !PointerClick()) return;
            Hover(event);
            Use();
        }

        private function Scroll(event:MouseEvent):void
        {
            if (!gamepad && event.delta != 0) Navigate(event.delta > 0 ? "Previous" : "Next");
        }

        private static function Now():Number { return new Date().getTime(); }

        private function PointerClick():Boolean
        {
            if (Now() - nativeClickAt < CLICK_WINDOW) return false;
            pointerClickAt = Now();
            return true;
        }

        // FO4's FavoritesMenu never delivers Scaleform CLICK, so native reports the release here.
        private function NativeClick():void
        {
            if (Now() - pointerClickAt < CLICK_WINDOW) return;
            nativeClickAt = Now();
            if (gamepad)
            {
                Navigate("Accept");
                return;
            }
            var hint:String = ButtonHintBar_mc.visible ? ButtonHintBar_mc.ActionAt(mouseX, mouseY) : "";
            if (hint != "")
            {
                HintAction(hint);
                return;
            }
            if (listOpen)
            {
                var row:PlayerListEntry = RowAt(mouseX, mouseY);
                if (row == null) Navigate("Accept");
                else if (ChooseRow(row)) AssignSelected();
                return;
            }
            var slot:int = SlotAt(mouseX, mouseY);
            if (slot >= 0) Select(slot);
            Use();
        }

        public function Navigate(action:String):void
        {
            if (action == "Click") NativeClick();
            else if (listOpen)
            {
                if (action == "Accept") AssignSelected();
                else if (action == "Cancel" || action == "Change") CloseList();
                else if (action == "Next") MoveList(1);
                else if (action == "Previous") MoveList(-1);
            }
            else if (action == "Accept") Use();
            else if (action == "Cancel") Close();
            else if (action == "Next") Select((selected + 1 + 12) % 12);
            else if (action == "Previous") Select((selected - 1 + 12) % 12);
            else if (action == "Change") OpenList();
            else if (action == "Inspect") Inspect();
            else if (action == "Tent") { if (TentAvailable()) BGSCodeObj.placeSurvivalTent(); }
        }

        public function ProcessUserEvent(action:String, down:Boolean):Boolean
        {
            // FO4 opens favorites on release; closing on press lets that release reopen it.
            if (action == "Quickkeys")
            {
                if (!down) Close();
                return true;
            }
            if (action == "Cancel")
            {
                if (!down) Navigate("Cancel");
                return true;
            }
            if (!down) return false;
            if (action == "Accept" || action == "Activate") Navigate("Accept");
            else if (listOpen && action == "Up") Navigate("Previous");
            else if (listOpen && action == "Down") Navigate("Next");
            else if (listOpen && (action == "Left" || action == "Right")) return true;
            else if (action == "Right" || action == "Down") Navigate("Next");
            else if (action == "Left" || action == "Up") Navigate("Previous");
            else return false;
            return true;
        }

        private function Close():void
        {
            if (BGSCodeObj.closeMenu != null) BGSCodeObj.closeMenu();
        }

        private function Sound(name:String):void
        {
            if (BGSCodeObj.PlaySound != null) BGSCodeObj.PlaySound(name);
        }

        private function Use():void
        {
            if (Occupied(selected) && BGSCodeObj.useQuickkey != null)
                // The native FavoritesMenu callback accepts GFx UInt, not AS3 int.
                BGSCodeObj.useQuickkey(uint(selected));
        }

        private function Inspect():void
        {
            if (InspectAvailable()) BGSCodeObj.inspectQuickkey(uint(selected));
        }

        private function SetWheelDim(value:Boolean):void
        {
            var alpha:Number = value ? DIM_ALPHA : 1;
            Background_mc.alpha = alpha;
            InnerRing.alpha = alpha;
            CenterInfo_mc.alpha = alpha;
            if (centerIcon != null) centerIcon.alpha = alpha;
        }

        // FO76 onShowInventory: the list opens beside the wheel for the highlighted slot.
        private function OpenList():void
        {
            if (selected < 0 || !ChangeAvailable()) return;
            listOpen = true;
            listMoved = false;
            PickSlotRow();
            SetWheelDim(true);
            PlayerInventory_mc.visible = true;
            RenderList();
            RefreshHints();
            Sound("UIMenuOK");
            BGSCodeObj.requestInventory();
        }

        private function CloseList():void
        {
            LeaveList();
            Sound("UIMenuCancel");
        }

        private function LeaveList():void
        {
            listOpen = false;
            PlayerInventory_mc.visible = false;
            SetWheelDim(false);
            RefreshHints();
        }

        // FO76 onSlotItem: assign the highlighted item to the highlighted slot, then show the wheel.
        private function AssignSelected():void
        {
            if (inventory == null || listIndex < 0 || listIndex >= inventory.length || selected < 0) return;
            if (BGSCodeObj.assignQuickkey == null) return;
            var id:uint = uint(inventory[listIndex].id);
            var slot:uint = uint(selected);
            LeaveList();
            Sound("UIMenuOK");
            BGSCodeObj.assignQuickkey(uint(id), uint(slot));
        }

        private function PickSlotRow():void
        {
            listIndex = inventory != null && inventory.length > 0 ? 0 : -1;
            listId = listIndex >= 0 ? Number(inventory[0].id) : -1;
            listTop = 0;
            if (inventory == null) return;
            for (var i:int = 0; i < inventory.length; i++)
                if (int(inventory[i].slot) == selected)
                {
                    listIndex = i;
                    listId = Number(inventory[i].id);
                }
        }

        private function MoveList(delta:int):void
        {
            if (inventory == null || inventory.length == 0) return;
            var next:int = int(Math.max(0, Math.min(inventory.length - 1, listIndex + delta)));
            if (next == listIndex) return;
            listIndex = next;
            listId = Number(inventory[next].id);
            listMoved = true;
            RenderList();
        }

        private function ChooseRow(row:PlayerListEntry):Boolean
        {
            var index:int = listTop + row.RowIndex;
            if (row.RowIndex < 0 || inventory == null || index >= inventory.length) return false;
            if (index != listIndex)
            {
                listIndex = index;
                listId = Number(inventory[index].id);
                listMoved = true;
                RenderList();
            }
            return true;
        }

        private function RowAt(px:Number, py:Number):PlayerListEntry
        {
            for each (var row:PlayerListEntry in listRows)
                if (row.visible && row.getBounds(this).contains(px, py)) return row;
            return null;
        }

        private function RowHover(event:MouseEvent):void
        {
            if (!gamepad && listOpen) ChooseRow(PlayerListEntry(event.currentTarget));
        }

        private function RowClick(event:MouseEvent):void
        {
            if (gamepad || !listOpen || !PointerClick()) return;
            if (ChooseRow(PlayerListEntry(event.currentTarget))) AssignSelected();
        }

        private function RenderList():void
        {
            var count:int = inventory == null ? 0 : inventory.length;
            if (listIndex >= count) listIndex = count - 1;
            if (count > 0 && listIndex < 0) listIndex = 0;
            if (listIndex >= 0 && listIndex < listTop) listTop = listIndex;
            if (listIndex >= listTop + listRows.length) listTop = listIndex - listRows.length + 1;
            listTop = int(Math.max(0, Math.min(listTop, count - listRows.length)));
            for (var i:int = 0; i < listRows.length; i++)
            {
                var row:PlayerListEntry = listRows[i];
                var index:int = listTop + i;
                if (index < count) row.SetRow(inventory[index], index == listIndex);
                else if (i == 0 && inventory != null) row.SetMessage(noItemsText);
                else row.visible = false;
            }
        }
    }
}
