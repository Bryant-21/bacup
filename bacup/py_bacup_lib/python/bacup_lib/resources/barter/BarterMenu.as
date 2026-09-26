package
{
    import Shared.IMenu;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.display.Loader;
    import flash.display.MovieClip;
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.events.MouseEvent;
    import flash.events.KeyboardEvent;
    import flash.geom.Rectangle;
    import flash.geom.ColorTransform;
    import flash.net.URLRequest;
    import flash.system.ApplicationDomain;
    import flash.system.LoaderContext;
    import flash.text.TextField;
    import flash.text.Font;
    import flash.text.TextFormat;

    public class ContainerMenu extends IMenu
    {
        public var B21Ready:Boolean;
        public var B21Error:String;
        public var B21Decor:MovieClip;
        public var B21Card:Loader;
        private var B21ArtLoader:Loader;
        private var B21Tabs:Array;
        private var B21Selector:DisplayObject;
        private var B21LastFilter:int;
        private var B21HasCard:Boolean;
        private var B21Headings:Array;
        private var B21FontsReady:Boolean;
        private var B21Tint:uint = 0xffffff;
        private var B21CardTint:uint = 0xffffffff;
        private var B21Hover:int = -1;
        private var B21ActiveList:ItemList;
        private var B21Footers:Array = [];
        private var B21Hints:Array = [];
        private var B21WheelX:Number = NaN;
        private var B21WheelY:Number = NaN;

        public function B21Initialize():void
        {
            B21Ready = false;
            B21Error = "";
            B21LastFilter = -1;
            // Class augmentation drops field initializers; without these the menu starts black until native tints it.
            B21Tint = 0xffffff;
            B21CardTint = 0xffffffff;
            B21Hover = -1;
            B21Tabs = [];
            B21Headings = [];
            B21Footers = [];
            B21Hints = [];
            B21WheelX = B21WheelY = NaN;
            addEventListener(Event.ADDED_TO_STAGE, B21Load);
        }

        private function B21Path(path:String):String
        {
            var url:String = loaderInfo.url;
            if (url.toLowerCase().indexOf("interface/") == 0 || url.toLowerCase().indexOf("interface\\") == 0)
                url = url.substring(10);
            var slash:int = Math.max(url.lastIndexOf("/"), url.lastIndexOf("\\"));
            return url.substring(0, slash + 1) + "B21/TalesFromAppalachia/" + path;
        }

        private function B21Load(event:Event):void
        {
            removeEventListener(Event.ADDED_TO_STAGE, B21Load);
            B21ArtLoader = new Loader();
            B21ArtLoader.contentLoaderInfo.addEventListener(Event.COMPLETE, B21Loaded);
            B21ArtLoader.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, B21Failed);
            B21ArtLoader.load(new URLRequest(B21Path("Barter/securetrade.swf")),
                new LoaderContext(false, new ApplicationDomain(ApplicationDomain.currentDomain)));
            B21Card = new Loader();
            addEventListener("GetPlatform", B21CardPlatform, true, 100);
            B21Card.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, B21Failed);
            B21Card.load(new URLRequest(B21Path("Inspect/bartercard.swf")),
                new LoaderContext(false, new ApplicationDomain(ApplicationDomain.currentDomain)));
        }

        private function B21Failed(event:IOErrorEvent):void
        {
            if (B21Error.length > 0) B21Error += "; ";
            B21Error += (event.target == B21ArtLoader.contentLoaderInfo ? "panels: " : "item card: ") + event.text;
        }

        private function B21Freeze(object:DisplayObject):void
        {
            if (object is MovieClip) MovieClip(object).stop();
            if (object is TextField) TextField(object).text = "";
            if (object is DisplayObjectContainer)
                for (var i:int = 0; i < DisplayObjectContainer(object).numChildren; i++)
                    B21Freeze(DisplayObjectContainer(object).getChildAt(i));
        }

        private function B21Art(name:String):MovieClip
        {
            var type:Class = B21ArtLoader.contentLoaderInfo.applicationDomain.getDefinition(name) as Class;
            var clip:MovieClip = new type();
            B21Freeze(clip);
            return clip;
        }

        private function B21Place(object:DisplayObject, left:Number, top:Number, width:Number, height:Number):void
        {
            object.scaleX = object.scaleY = 1;
            var bounds:Rectangle = object.getBounds(object);
            object.scaleX = width / bounds.width;
            object.scaleY = height / bounds.height;
            object.x = left - bounds.x * object.scaleX;
            object.y = top - bounds.y * object.scaleY;
        }

        private function B21Panel(name:String, left:Number):MovieClip
        {
            var panel:MovieClip = B21Art(name);
            for (var i:int = panel.numChildren - 1; i >= 0; i--)
            {
                var child:DisplayObject = panel.getChildAt(i);
                if (child.name.indexOf("instance") != 0 && child.name != "OfferWeightIcon") panel.removeChildAt(i);
            }
            panel.mouseEnabled = panel.mouseChildren = false;
            panel.alpha = 0.45;
            B21Decor.addChild(panel);
            B21Place(panel, left, 152, 316, 454);
            return panel;
        }

        private function B21Move(object:DisplayObject, left:Number, top:Number):void
        {
            var bounds:Rectangle = object.getBounds(this);
            object.x += (left - bounds.x) / object.parent.scaleX;
            object.y += (top - bounds.y) / object.parent.scaleY;
        }

        private function B21Footer(inventory:MovieClip, left:Number):void
        {
            // Barter draws the footer as one backer; the container menu splits it into weight and caps halves.
            var row:Rectangle = null;
            for (var i:int = 0; i < inventory.numChildren; i++)
            {
                var bounds:Rectangle = inventory.getChildAt(i).getBounds(this);
                if (bounds.y >= 480 && bounds.bottom <= 516) row = row == null ? bounds : row.union(bounds);
            }
            if (row == null || row.width <= 280) return;
            var icon:DisplayObject;
            for (var j:int = 0; j < inventory.numChildren; j++)
            {
                var part:DisplayObject = inventory.getChildAt(j);
                var position:Rectangle = part.getBounds(this);
                if (position.y >= 480 && position.bottom <= 516)
                {
                    if (!(part is TextField) && position.width < 40 && (icon == null || position.x > icon.getBounds(this).x)) icon = part;
                    part.visible = false;
                }
            }
            if (icon != null)
            {
                icon.visible = true;
                B21Place(icon, 0, 0, 14, 14);
                B21Move(icon, left + 282, 584);
            }
            var currency:TextField = new TextField();
            currency.defaultTextFormat = new TextFormat("$MAIN_Font", 16, 0xffffff, false, false, false, null, null, "right");
            currency.x = left + 148; currency.y = 580; currency.width = 130; currency.height = 25;
            currency.mouseEnabled = currency.selectable = false;
            B21Decor.addChild(currency);
            var weight:TextField = new TextField();
            weight.defaultTextFormat = new TextFormat("$MAIN_Font", 16, 0xffffff);
            weight.x = left + 13; weight.y = 580; weight.width = 128; weight.height = 25;
            weight.mouseEnabled = weight.selectable = false;
            weight.text = inventory == PlayerInventory_mc ? "" : "0 / 0";
            B21Decor.addChild(weight);
            B21Footers.push({source:inventory == PlayerInventory_mc ? inventory.PlayerCaps_tf : inventory.VendorCaps_tf,
                currency:currency,weight:weight});
        }

        private function B21CardPlatform(event:Event):void
        {
            if (B21Card == null || !B21Card.contains(event.target as DisplayObject)) return;
            // The converted child has a private PlatformRequestEvent class.
            if ("RespondToRequest" in event) Object(event).RespondToRequest(uiPlatform, bPS3Switch, 0, 0);
            event.stopImmediatePropagation();
        }

        public function B21SetCarryWeight(current:Number, maximum:Number):void
        {
            if (B21Footers.length > 0) B21Footers[0].weight.text = Math.floor(current) + " / " + Math.floor(maximum);
        }

        public function B21SortHint(button:Object):Object
        {
            if (B21Ready) button.SetButtons("Q", button.PSNButton, button.XenonButton);
            return button;
        }

        public function B21InvestHint(button:Object):Object
        {
            if (B21Ready) button.SetButtons("I", button.PSNButton, button.XenonButton);
            return button;
        }

        private function B21ClearTint(target:DisplayObject):void
        {
            target.filters = [];
            var color:ColorTransform = target.transform.colorTransform;
            color.redMultiplier = color.greenMultiplier = color.blueMultiplier = 1;
            color.redOffset = color.greenOffset = color.blueOffset = 0;
            target.transform.colorTransform = color;
        }

        private function B21Style(target:DisplayObjectContainer):void
        {
            B21ClearTint(target);
            for (var i:int = 0; i < target.numChildren; i++)
            {
                var child:DisplayObject = target.getChildAt(i);
                B21ClearTint(child);
                if (child is TextField)
                {
                    var field:TextField = child as TextField;
                    field.embedFonts = true;
                    var format:TextFormat = field.getTextFormat();
                    format.font = "Roboto Condensed";
                    format.bold = false; format.italic = false;
                    field.defaultTextFormat = format;
                    field.setTextFormat(format);
                }
                else if (child is DisplayObjectContainer) B21Style(child as DisplayObjectContainer);
            }
        }

        private function B21Loaded(event:Event):void
        {
            B21Decor = new MovieClip();
            B21Decor.mouseEnabled = false;
            B21Decor.mouseChildren = true;
            addChildAt(B21Decor, 0);
            B21Panel("/* ART_0 */", 88);
            var offer:MovieClip = B21Panel("/* ART_1 */", 876);
            var sourceBar:MovieClip = B21Art("/* ART_2 */");
            var backer:DisplayObject = sourceBar.getChildByName("BackerBar_mc");
            B21Selector = sourceBar.getChildByName("Selector_mc");
            B21Decor.addChild(backer);
            backer.alpha = 0.55;
            B21Place(backer, 40, 22, 1200, 38);
            B21Decor.addChild(B21Selector);
            for (var i:int = 0; i < FilterInfoA.length; i++)
            {
                var tab:Sprite = new Sprite();
                tab.x = 164 + i * 120;
                tab.y = 25;
                tab.name = String(i);
                tab.graphics.beginFill(0, 0.01);
                tab.graphics.drawRect(0, 0, 112, 32);
                tab.graphics.endFill();
                tab.buttonMode = true;
                var field:TextField = new TextField();
                field.defaultTextFormat = new TextFormat("$MAIN_Font", 18, 0xffffff, false, false, false, null, null, "center");
                field.text = FilterInfoA[i].text;
                field.setTextFormat(field.defaultTextFormat);
                field.width = 112;
                field.height = 32;
                field.mouseEnabled = field.selectable = false;
                tab.addChild(field);
                tab.addEventListener(MouseEvent.CLICK, B21FilterClick);
                tab.addEventListener(MouseEvent.ROLL_OVER, B21FilterOver);
                tab.addEventListener(MouseEvent.ROLL_OUT, B21FilterOut);
                B21Decor.addChild(tab);
                B21Tabs.push(tab);
            }
            for (var hintIndex:int = 0; hintIndex < 2; hintIndex++)
            {
                var hint:TextField = new TextField();
                hint.defaultTextFormat = new TextFormat("$MAIN_Font", 18, 0xffffff, false, false, false, null, null, "center");
                hint.x = hintIndex == 0 ? 48 : 1184; hint.y = 29; hint.width = 48; hint.height = 30;
                hint.mouseEnabled = hint.selectable = false;
                B21Decor.addChild(hint); B21Hints.push(hint);
            }
            addEventListener(KeyboardEvent.KEY_DOWN, B21CategoryKey, true, 100);
            addEventListener(KeyboardEvent.KEY_UP, B21CategoryKey, true, 100);
            addEventListener(MouseEvent.MOUSE_WHEEL, B21Wheel, true, 100);
            addEventListener(MouseEvent.MOUSE_OVER, B21GuardHover, true, 100);
            addEventListener(MouseEvent.ROLL_OVER, B21GuardHover, true, 100);
            addEventListener(MouseEvent.MOUSE_MOVE, B21ResumePointer, true, 100);
            PlayerInventory_mc.x += 100 - PlayerInventory_mc.PlayerList_mc.getBounds(this).x;
            ContainerInventory_mc.x += 888 - ContainerInventory_mc.getBounds(this).x;
            ContainerList_mc.x += 888 - ContainerList_mc.getBounds(this).x;
            B21SizeList(PlayerInventory_mc.PlayerList_mc, 100);
            B21SizeList(ContainerList_mc, 888);
            var column:int = 0;
            for each (var inventory:MovieClip in [PlayerInventory_mc, ContainerInventory_mc])
            {
                for each (var part:String in ["PlayerBracketBackground_mc", "ContainerBracketBackground_mc", "lines",
                    "PlayerListHeader", "ContainerListHeader", "PlayerSwitchButton_tf", "ContainerSwitchButton_tf", "LeftHitBox_tf", "RightHitBox_tf"])
                {
                    var object:DisplayObject = inventory.getChildByName(part);
                    if (object != null) object.visible = false;
                }
                B21Footer(inventory, column == 0 ? 100 : 888);
                // FO76 draws the title on a darker band than the list; the panel artwork has none of its own.
                var band:Sprite = new Sprite();
                band.graphics.beginFill(0, 0.45);
                band.graphics.drawRect(column == 0 ? 88 : 876, 152, 316, 40);
                band.graphics.endFill();
                band.mouseEnabled = false;
                B21Decor.addChild(band);
                var heading:TextField = new TextField();
                heading.defaultTextFormat = new TextFormat("$MAIN_Font", 24, 0xffffff);
                heading.x = column == 0 ? 98 : 886;
                heading.y = 157;
                heading.width = 296;
                heading.height = 34;
                heading.mouseEnabled = heading.selectable = false;
                B21Decor.addChild(heading);
                B21Headings.push(heading);
                column++;
            }
            // A container has no footer of its own, so the FO76 offer panel's weight icon would stand alone.
            var offerWeight:DisplayObject = offer.getChildByName("OfferWeightIcon");
            if (offerWeight != null && B21Footers.length < 2) offerWeight.visible = false;
            var transfer:DisplayObject = getChildByName("CapsTransferInfo_mc");
            if (transfer != null) B21Place(transfer, 580, 590, 120, 49);
            addChild(B21Card);
            B21Card.mouseEnabled = B21Card.mouseChildren = false;
            setChildIndex(QuantityMenu_mc, numChildren - 1);
            setChildIndex(ButtonHintBar_mc, numChildren - 1);
            B21Ready = true;
            UpdateButtonHints();
            addEventListener(Event.ENTER_FRAME, B21Refresh);
        }

        private function B21SizeList(list:ItemList, left:Number):void
        {
            list.scaleX = list.scaleY = 290 / list.border.width;
            list.border.height = 350 / list.scaleY;
            for (var i:int = 0; i < list.numListItems; i++)
            {
                var row:ItemListEntry = list.GetClipByIndex(i) as ItemListEntry;
                if (row == null) continue;
                row.ORIG_BORDER_HEIGHT = 26 / list.scaleY;
                row.border.height = row.ORIG_BORDER_HEIGHT;
                row.textField.height = 25 / list.scaleY;
                row.textField.defaultTextFormat = new TextFormat("Roboto Condensed", 20 / list.scaleY, 0xffffff, false, false);
                row.textField.setTextFormat(row.textField.defaultTextFormat);
            }
            if (list.ScrollDown != null) list.ScrollDown.y = list.border.y + list.border.height;
            B21Move(list, left, 202);
            list.InvalidateData();
        }

        private function B21FilterOver(event:MouseEvent):void
        {
            if (QuantityMenu_mc.opened || MessageBoxIsActive || InspectingFeaturedItem) return;
            B21Hover = int(event.currentTarget.name);
            B21PaintTabs();
        }

        private function B21FilterOut(event:MouseEvent):void
        {
            if (B21Hover == int(event.currentTarget.name)) B21Hover = -1;
            B21PaintTabs();
        }

        private function B21PaintTabs():void
        {
            for (var i:int = 0; i < B21Tabs.length; i++)
            {
                var tab:Sprite = B21Tabs[i];
                var highlighted:Boolean = i == B21LastFilter || i == B21Hover;
                tab.graphics.clear();
                tab.graphics.beginFill(0xffffff, i == B21Hover && i != B21LastFilter ? 0.8 : 0.01);
                tab.graphics.drawRect(0, 0, 112, 32);
                tab.graphics.endFill();
                TextField(tab.getChildAt(0)).textColor = highlighted ? 0 : 0xffffff;
            }
        }

        public function B21SelectFilter(index:int):void
        {
            if (!B21Ready || index < 0 || index >= FilterInfoA.length || QuantityMenu_mc.opened || MessageBoxIsActive || InspectingFeaturedItem) return;
            if (stage.focus != PlayerInventory_mc.PlayerList_mc && stage.focus != ContainerList_mc)
                stage.focus = B21ActiveList != null ? B21ActiveList : PlayerInventory_mc.PlayerList_mc;
            uiPlayerFilterIndex = uiContainerFilterIndex = index;
            PlayerInventory_mc.PlayerList_mc.filterer.itemFilter = FilterInfoA[index].flag;
            ContainerList_mc.filterer.itemFilter = FilterInfoA[index].flag;
            for each (var list:ItemList in [PlayerInventory_mc.PlayerList_mc, ContainerList_mc])
            {
                BGSCodeObj.sortItems(list == ContainerList_mc, index, false);
                UpdateHeaderText(list);
                list.InvalidateData();
                list.selectedClipIndex = list.filterer.IsFilterEmpty(FilterInfoA[index].flag) ? -1 : 0;
            }
            BGSCodeObj.updateSortButtonLabel(containerIsSelected, index);
            UpdateItemDisplay(containerIsSelected ? ContainerList_mc : PlayerInventory_mc.PlayerList_mc, false);
            B21Refresh(null);
        }

        public function B21ValidateListHighlight():void
        {
            if (!B21Ready) { ValidateListHighlight(); return; }
            for each (var list:ItemList in [PlayerInventory_mc.PlayerList_mc, ContainerList_mc])
                if (list.itemsShown == 0) list.selectedIndex = -1;
            if (stage.focus == null)
            {
                if (ContainerList_mc.itemsShown > 0) SwitchToContainerList(!InitialValidation);
                else SwitchToPlayerList(!InitialValidation);
            }
            InitialValidation = false;
            var active:ItemList = containerIsSelected ? ContainerList_mc : PlayerInventory_mc.PlayerList_mc;
            if (active.itemsShown == 0) B21SetItem(null);
        }

        public function B21SetHUDColor(color:uint):void
        {
            B21Tint = color;
            B21ApplyTint();
        }

        // The tint goes on each child rather than the root. GFx applies an inherited tint before a
        // child's own filter, and the card's red-channel normalization then drew it white
        // (2026-09-23 screenshot: panels 236,210,151, card 255,255,255), so the card tints itself.
        // Setting each child's colour also replaces, rather than stacks on, the engine's own
        // per-clip HUD tint. Alpha is kept: stock menus hide themselves with it.
        private function B21ApplyTint():void
        {
            filters = [];
            var plain:ColorTransform = transform.colorTransform;
            plain.redMultiplier = plain.greenMultiplier = plain.blueMultiplier = plain.alphaMultiplier = 1;
            plain.redOffset = plain.greenOffset = plain.blueOffset = plain.alphaOffset = 0;
            transform.colorTransform = plain;
            for (var i:int = 0; i < numChildren; i++)
            {
                var child:DisplayObject = getChildAt(i);
                var own:ColorTransform = child.transform.colorTransform;
                own.redMultiplier = child == B21Card ? 1 : (B21Tint >> 16 & 255) / 255;
                own.greenMultiplier = child == B21Card ? 1 : (B21Tint >> 8 & 255) / 255;
                own.blueMultiplier = child == B21Card ? 1 : (B21Tint & 255) / 255;
                own.redOffset = own.greenOffset = own.blueOffset = 0;
                child.transform.colorTransform = own;
            }
            var bridge:Object = B21Card != null ? B21Card.content : null;
            if (bridge != null && "SetHUDColor" in bridge && B21CardTint != B21Tint)
            {
                bridge.SetHUDColor(B21Tint);
                B21CardTint = B21Tint;
            }
        }

        private function B21PositionCost():void
        {
            // Stock UpdatePickpocketInfo only centers it horizontally; its authored height sits under the panels.
            var pickpocket:DisplayObject = getChildByName("PickpocketInfo_mc");
            if (pickpocket != null) B21Move(pickpocket, pickpocket.getBounds(this).x, 590);
            var transfer:MovieClip = getChildByName("CapsTransferInfo_mc") as MovieClip;
            if (transfer == null) return;
            var field:TextField = transfer.TransferCaps_tf;
            var icon:DisplayObject = transfer.TransferCapsIcon_mc;
            var gap:Number = field.x - icon.x;
            var center:Number = (640 - transfer.x) / transfer.scaleX;
            icon.x = center - (gap + field.textWidth) / 2;
            field.x = icon.x + gap;
            var backer:DisplayObject = transfer.Background_mc;
            var bounds:Rectangle = backer.getBounds(this);
            B21Move(backer, 640 - bounds.width / 2, bounds.y);
            B21Move(transfer, transfer.getBounds(this).x, 590);
        }

        private function B21FilterClick(event:MouseEvent):void
        {
            B21SelectFilter(int(event.currentTarget.name));
            event.stopImmediatePropagation();
        }

        private function B21CategoryKey(event:KeyboardEvent):void
        {
            if (!B21Ready || !visible || QuantityMenu_mc.opened || MessageBoxIsActive || InspectingFeaturedItem ||
                (event.keyCode != 90 && event.keyCode != 67)) return;
            if (event.type == KeyboardEvent.KEY_UP)
            {
                var index:int = containerIsSelected ? uiContainerFilterIndex : uiPlayerFilterIndex;
                B21SelectFilter((index + FilterInfoA.length + (event.keyCode == 90 ? -1 : 1)) % FilterInfoA.length);
            }
            event.preventDefault();
            event.stopImmediatePropagation();
        }

        private function B21PointerList(target:DisplayObject):ItemList
        {
            for each (var list:ItemList in [PlayerInventory_mc.PlayerList_mc, ContainerList_mc])
                if (list == target || list.contains(target)) return list;
            return null;
        }

        private function B21Wheel(event:MouseEvent):void
        {
            if (QuantityMenu_mc.opened || MessageBoxIsActive || InspectingFeaturedItem || event.delta == 0) return;
            var list:ItemList = B21PointerList(event.target as DisplayObject);
            if (list == null || list.disableInput || list.disableSelection) return;
            B21WheelX = stage.mouseX; B21WheelY = stage.mouseY;
            if (list == ContainerList_mc) SwitchToContainerList(false);
            else SwitchToPlayerList(false);
            list.dispatchEvent(new KeyboardEvent(KeyboardEvent.KEY_DOWN, true, true, 0, event.delta < 0 ? 40 : 38));
            event.stopImmediatePropagation();
        }

        private function B21GuardHover(event:MouseEvent):void
        {
            if (stage.mouseX == B21WheelX && stage.mouseY == B21WheelY && B21PointerList(event.target as DisplayObject) != null)
                event.stopImmediatePropagation();
        }

        private function B21ResumePointer(event:MouseEvent):void
        {
            if (isNaN(B21WheelX) || stage.mouseX == B21WheelX && stage.mouseY == B21WheelY ||
                QuantityMenu_mc.opened || MessageBoxIsActive || InspectingFeaturedItem) return;
            var list:ItemList = B21PointerList(event.target as DisplayObject);
            if (list == null || list.disableInput || list.disableSelection) return;
            B21WheelX = B21WheelY = NaN;
            for (var i:int = 0; i < list.itemsShown; i++)
            {
                var row:ItemListEntry = list.GetClipByIndex(i) as ItemListEntry;
                if (row != null && row.hitTestPoint(stage.mouseX, stage.mouseY, false)) list.selectedIndex = row.itemIndex;
            }
        }

        public function B21HideStockChrome():void
        {
            if (!B21Ready) return;
            PlayerInventory_mc.PlayerListHeader.visible = false;
            ContainerInventory_mc.ContainerListHeader.visible = false;
            PlayerInventory_mc.PlayerSwitchButton_tf.visible = false;
            ContainerInventory_mc.ContainerSwitchButton_tf.visible = false;
            for each (var inventory:MovieClip in [PlayerInventory_mc, ContainerInventory_mc])
            {
                var lines:DisplayObject = inventory.getChildByName("lines");
                if (lines != null) lines.visible = false;
            }
        }

        private function B21Refresh(event:Event):void
        {
            if (stage.focus == PlayerInventory_mc.PlayerList_mc || stage.focus == ContainerList_mc)
                B21ActiveList = stage.focus as ItemList;
            var bridge:Object = B21Card != null ? B21Card.content : null;
            ItemCard_mc.visible = (bridge == null || !("SetCard" in bridge)) && !InspectingFeaturedItem;
            B21Card.visible = B21HasCard && !QuantityMenu_mc.opened && !InspectingFeaturedItem;
            B21PositionCost();
            B21HideStockChrome();
            if (!B21FontsReady)
                for each (var font:Font in Font.enumerateFonts(true))
                    if (font.fontName == "Roboto Condensed") B21FontsReady = true;
            if (B21FontsReady)
                for each (var target:DisplayObjectContainer in [B21Decor, PlayerInventory_mc, ContainerInventory_mc, ContainerList_mc, ButtonHintBar_mc])
                    B21Style(target);
            var index:int = containerIsSelected ? uiContainerFilterIndex : uiPlayerFilterIndex;
            if (B21Hover != -1 && (QuantityMenu_mc.opened || MessageBoxIsActive || InspectingFeaturedItem))
            {
                B21Hover = -1;
                B21PaintTabs();
            }
            if (index != B21LastFilter)
            {
                B21LastFilter = index;
                B21Place(B21Selector, 164 + index * 120, 25, 112, 32);
                B21PaintTabs();
            }
            B21Hints[0].text = uiPlatform == 0 ? "Z)" : "LB)";
            B21Hints[1].text = uiPlatform == 0 ? "C)" : "RB)";
            for each (var footer:Object in B21Footers) footer.currency.text = footer.source != null ? footer.source.text : "";
            B21Headings[0].text = FilterInfoA[uiPlayerFilterIndex].text + "Mine";
            B21Headings[1].text = uiContainerFilterIndex == 0 ? String(FilterInfoA[0].containerText || FilterInfoA[0].text) : FilterInfoA[uiContainerFilterIndex].text;
            for each (var heading:TextField in B21Headings)
                heading.setTextFormat(new TextFormat(B21FontsReady ? "Roboto Condensed" : "$MAIN_Font", 24, 0xffffff));
            // B21Style clears the panels' tints along with the engine's; put the HUD tint back.
            B21ApplyTint();
        }

        public function B21SetItem(value:Object):Boolean
        {
            var bridge:Object = B21Card != null ? B21Card.content : null;
            if (!B21Ready || bridge == null || !("SetCard" in bridge)) return false;
            B21HasCard = value != null;
            if (value == null) { bridge.Clear(); B21Refresh(null); return true; }
            bridge.SetLayout({barter:true, statsLeft:582, statsBottom:576, statsScale:0.7, stageWidth:1280,
                headerLeft:424, headerTop:105, headerWidth:432, descriptionTop:144, hintsTop:675, log:value.log});
            bridge.SetHUDColor(B21Tint);
            B21CardTint = B21Tint;
            bridge.SetCard(value);
            ItemCard_mc.visible = false;
            B21Card.visible = !QuantityMenu_mc.opened && !InspectingFeaturedItem;
            return true;
        }

        public function B21FilterState():Object
        {
            return {player:uiPlayerFilterIndex, vendor:uiContainerFilterIndex};
        }

        public function B21CardState():String
        {
            var bridge:Object = B21Card != null ? B21Card.content : null;
            if (bridge == null) return "not loaded";
            var bounds:Rectangle = bridge.stats.getBounds(this);
            return "item=" + B21HasCard + " loader=" + B21Card.visible + " movie=" + bridge.visible +
                " content=" + bridge.content.visible + " staged=" + (bridge.stage != null) +
                " entries=" + bridge.card.numChildren + " bounds=" + bounds +
                " rootAlpha=" + alpha + " parentAlpha=" + (parent != null ? parent.alpha : 1) +
                " tint=" + B21Tint.toString(16) + " playerGreen=" + PlayerInventory_mc.transform.colorTransform.greenMultiplier +
                " cardGreen=" + B21Card.transform.colorTransform.greenMultiplier + " fonts=" + B21FontsReady +
                " quantity=" + QuantityMenu_mc.opened + " inspecting=" + InspectingFeaturedItem;
        }
    }
}
