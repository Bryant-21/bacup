package
{
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.display.Loader;
    import flash.display.MovieClip;
    import flash.display.Shape;
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;
    import flash.filters.ColorMatrixFilter;
    import flash.geom.Rectangle;
    import flash.net.URLRequest;
    import flash.system.ApplicationDomain;
    import flash.system.LoaderContext;
    import flash.text.Font;
    import flash.text.TextField;
    import flash.text.TextFieldAutoSize;
    import flash.text.TextFormat;
    import flash.text.TextLineMetrics;
    import flash.utils.getQualifiedClassName;

    // Tales Inspect overlay. This movie is its own menu, drawn over FO4's Inspect screen, and
    // native code drives it through SetLayout, SetHUDColor, SetCard and Clear.
    public class ExamineMenu extends MovieClip
    {
        // FO76's card rows are drawn at FO4's row size (36 vs 37 px), so they take FO4's card scale unchanged.
        public static const STAR_POINTS:Array = [0, -1, 0.2245, -0.309, 0.9511, -0.309, 0.3633, 0.118,
            0.5878, 0.809, 0, 0.382, -0.5878, 0.809, -0.3633, 0.118, -0.9511, -0.309, -0.2245, -0.309];

        public var content:Sprite = new Sprite();
        public var header:Sprite = new Sprite();
        public var stats:Sprite = new Sprite();
        public var mods:Sprite = new Sprite();
        public var typeLine:Sprite = new Sprite();
        public var inspectTab:MovieClip = new InspectTab();
        public var footer:B21ButtonHints = new B21ButtonHints(true);
        public var repairHints:B21ButtonHints = new B21ButtonHints();
        private var repairGamepad:Boolean = false;
        public var card:ItemCard = new ItemCard();
        // FO76's own card panel: the box, the heading strip and the heading, one per column.
        public var statsPanel:MovieClip = new StatsPanel();
        public var modsPanel:MovieClip = new StatsPanel();
        public var legendaryRow:ItemCard_StandardEntry = null;
        // FO4's stock ExamineMenu positions, used until native code sends the installed movie's own.
        public var layout:Object = {statsLeft: 273, statsBottom: 611, statsScale: 0.7, stageWidth: 1280,
            headerLeft: 234, headerTop: 28, headerWidth: 812, descriptionTop: 56, hintsTop: 687};
        public var current:Object = null;
        public var BGSCodeObj:Object;
        public var NativeInput:Boolean = false;
        public var repairState:Object = null;
        public var repairSelection:int = 0;
        public var repairConfirm:Boolean = false;
        public var repairConfirmSelection:int = 0;
        public var repairPending:Boolean = false;
        public var repairContent:Sprite = new Sprite();
        private var repairPointerX:Number = NaN;
        private var repairPointerY:Number = NaN;
        // Static so the shared text helper reaches them.
        public static var fontName:String = "Roboto Condensed";
        public static var boldFontName:String = "Roboto Condensed Bold";
        public static var fontsReady:Boolean = false;
        public static const BODY_COLOR:uint = 0xFFFFCB;
        public static const GOLD_COLOR:uint = 0xF5CB5B;
        // Marks the fields the card draws itself, which are already styled and must not be restyled.
        public static const OURS:String = "b21CardText";
        // FO76's box sits this far outside the rows it frames.
        public static const PANEL_PAD:Number = 2;
        private var icons:Loader = new Loader();
        private var iconSupplied:Boolean = false;
        private var fontsLoader:Loader = new Loader();
        private var logger:Function = null;
        private var pending:String = "";
        // What the panel art measured on the last fit, reported with the card.
        private var note:String = "";

        public function ExamineMenu()
        {
            super();
            mouseEnabled = false;
            mouseChildren = true;
            card.bottomUp = true;
            card.entrySpacing = 2.5;
            card.showItemDesc = false;
            card.showValueEntry = true;
            card.blankEntryFillTarget = 0;
            card.addEventListener(ItemCard.EVENT_ITEM_CARD_UPDATED, onCardUpdated);
            Heading(modsPanel).text = "$CURRENT MODS";
            stats.addChild(statsPanel);
            stats.addChild(card);
            mods.addChild(modsPanel);
            content.addChild(header);
            content.addChild(stats);
            content.addChild(mods);
            content.addChild(typeLine);
            content.addChild(inspectTab);
            content.addChild(footer);
            addChild(content);
            addChild(repairContent);
            addChild(repairHints);
            repairHints.visible = false;
            footer.addEventListener(MouseEvent.CLICK, onFooterAction);
            repairHints.addEventListener(MouseEvent.CLICK, onRepairHint);
            addEventListener(Event.ADDED_TO_STAGE, onAddedToStage);
        }

        private function onAddedToStage(event:Event):void
        {
            removeEventListener(Event.ADDED_TO_STAGE, onAddedToStage);
            stage.addEventListener(KeyboardEvent.KEY_DOWN, onRepairKey, false, 0, true);
            fontsLoader.contentLoaderInfo.addEventListener(Event.COMPLETE, onFontsLoaded);
            fontsLoader.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, onFontsFailed);
            fontsLoader.load(new URLRequest(Sibling("b21_inspect_fonts.swf")), new LoaderContext(false, ApplicationDomain.currentDomain));
            icons.contentLoaderInfo.addEventListener(Event.COMPLETE, onIconsLoaded);
            icons.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, onIconsFailed);
            icons.load(new URLRequest(Sibling("currencyiconlibrary.swf")), new LoaderContext(false, ApplicationDomain.currentDomain));
            if (current != null)
            {
                SetCard(current);
            }
            if (repairState != null)
            {
                DrawRepair();
            }
            var problem:String = ClassProblem();
            if (problem.length > 0)
            {
                Log(problem);
            }
        }

        private function onFontsLoaded(event:Event):void
        {
            for each (var name:String in ["B21TFA_InspectFonts_$MAIN_Font", "B21TFA_InspectFonts_$MAIN_Font_Bold"])
            {
                var type:Class = ApplicationDomain.currentDomain.getDefinition(name) as Class;
                Font.registerFont(type);
            }
            fontsReady = true;
            PickFonts();
            if (current != null) SetCard(current);
            if (repairState != null) DrawRepair();
        }

        private function onFontsFailed(event:IOErrorEvent):void
        {
            Log("FO76 font library failed to load: " + event.text);
        }

        public function Sibling(file:String):String
        {
            // Native menu movies resolve requests from their own folder; nested cards use their host's folder.
            if (loaderInfo.loader == null) return file;
            var url:String = loaderInfo.url;
            // GFx already resolves Loader requests under Interface; preview URLs remain absolute.
            if (url.toLowerCase().indexOf("interface/") == 0) url = url.substring(10);
            if (url.toLowerCase().indexOf("interface\\") == 0) url = url.substring(10);
            var slash:int = Math.max(url.lastIndexOf("/"), url.lastIndexOf("\\"));
            return url.substring(0, slash + 1) + file;
        }

        // Native code hands the card a log function with its layout; anything said before that
        // arrives waits here, because the fonts are picked the moment the card reaches the stage.
        public function Log(text:String):void
        {
            if (logger == null)
            {
                pending = pending.length > 0 ? pending + "; " + text : text;
                return;
            }
            logger(text);
        }

        // A loaded movie resolves real font names only: the $ aliases the menus use are wired up
        // for a menu movie, and every field asking for one draws boxes here (2026-09-17 log).
        public function PickFonts():void
        {
            var names:Object = {};
            var found:String = "";
            var fonts:Array = Font.enumerateFonts(true);
            for each (var font:Font in fonts)
            {
                names[font.fontName] = true;
                found += (found.length > 0 ? "," : "") + font.fontName;
            }
            fontName = Pick(names, ["Roboto Condensed"], fontName);
            boldFontName = Pick(names, ["Roboto Condensed Bold"], boldFontName);
            Log("fonts " + fonts.length + " (" + fontName + " / " + boldFontName + "): " + found.substring(0, 240));
        }

        private static function Pick(names:Object, wanted:Array, fallback:String):String
        {
            for each (var name:String in wanted)
            {
                if (names[name] != null)
                {
                    return name;
                }
            }
            return fallback;
        }

        // FO76's own row fields name the aliases too, so restyle the ones the card drew for us.
        public function RestyleText(target:DisplayObjectContainer):void
        {
            if (target is B21ButtonHints) return;
            for (var i:int = 0; i < target.numChildren; i++)
            {
                var child:DisplayObject = target.getChildAt(i);
                if (child is TextField && child.name != OURS)
                {
                    var field:TextField = child as TextField;
                    field.embedFonts = fontsReady;
                    var base:TextFormat = field.defaultTextFormat;
                    var bold:Boolean = base != null && (base.bold == true || String(base.font).indexOf("Bold") >= 0);
                    // Only the font: a format read back here carries none of the field's own size or colour,
                    // so applying a whole one leaves every row 12pt black.
                    field.setTextFormat(new TextFormat(bold ? boldFontName : fontName));
                }
                else if (child is DisplayObjectContainer)
                {
                    RestyleText(child as DisplayObjectContainer);
                }
            }
        }

        private function onIconsLoaded(event:Event):void
        {
            if (current != null)
            {
                SetCard(current);
            }
        }

        private function onIconsFailed(event:IOErrorEvent):void
        {
            Log("currency icons failed to load: " + event.text);
        }

        public function ClassProblem():String
        {
            // The converter renames every class this movie defines, so resolving a stock
            // FO4 name means the host menu's classes took ours over.
            var entry:String = getQualifiedClassName(new ItemCard_StandardEntry());
            return entry == "ItemCard_StandardEntry" ? "item card classes resolved to the host's " + entry : "";
        }

        public function SetLayout(value:Object):void
        {
            layout = value;
            if (value.log != null && logger == null)
            {
                logger = value.log as Function;
                if (pending.length > 0)
                {
                    logger(pending);
                    pending = "";
                }
            }
            if (current != null)
            {
                SetCard(current);
            }
        }

        public function SetHUDColor(color:uint):void
        {
            // FO76's source art contains yellow fills as well as text. Normalize its red
            // channel before tinting so a blue HUD does not turn those fills green.
            var red:Number = (color >> 16 & 255) / 255;
            var green:Number = (color >> 8 & 255) / 255;
            var blue:Number = (color & 255) / 255;
            var tint:ColorMatrixFilter = new ColorMatrixFilter([
                red, 0, 0, 0, 0,
                green, 0, 0, 0, 0,
                blue, 0, 0, 0, 0,
                0, 0, 0, 1, 0]);
            content.filters = [tint];
            repairContent.filters = [tint];
            repairHints.filters = [tint];
        }

        public function Clear():void
        {
            current = null;
            visible = false;
        }

        public function SetCard(value:Object):void
        {
            current = value;
            visible = true;
            if (stage == null)
            {
                return;
            }
            var scale:Number = Number(layout.statsScale);
            header.visible = layout.workbench != true;
            typeLine.visible = layout.workbench != true && layout.barter != true;
            inspectTab.visible = layout.workbench != true && layout.barter != true;
            DrawFooter();
            var tabLabel:TextField = inspectTab.getChildByName("HeaderText_tf") as TextField;
            tabLabel.text = "$INSPECT";
            tabLabel.embedFonts = fontsReady;
            tabLabel.setTextFormat(new TextFormat(boldFontName));
            inspectTab.scaleX = inspectTab.scaleY = 0.7;
            inspectTab.x = -inspectTab.getBounds(inspectTab).x * inspectTab.scaleX;
            inspectTab.y = 645 - inspectTab.getBounds(inspectTab).y * inspectTab.scaleY;
            DrawHeader(value);
            DrawMods(value.mods as Array, scale);
            if (layout.workbench == true || layout.barter == true)
            {
                mods.visible = false;
            }
            DrawTypeLine(value.typeLine as Array);
            if (legendaryRow != null)
            {
                stats.removeChild(legendaryRow);
                legendaryRow = null;
            }
            var stars:int = LegendaryStars(value.rows as Array);
            if (stars > 0)
            {
                legendaryRow = new ItemCard_StandardEntry();
                legendaryRow.PopulateEntry({text: "$B21_TFA_Legendary", value: ""});
                AddStars(legendaryRow, stars, legendaryRow.Value_tf.x + legendaryRow.Value_tf.width,
                    legendaryRow.height / 2, legendaryRow.height * 0.22, true);
                stats.addChild(legendaryRow);
            }
            stats.scaleX = scale;
            stats.scaleY = scale;
            // Above one, FO76's card adds its own stack weight row from the weight entry.
            card.Count = value.count != null ? uint(value.count) : 1;
            card.InfoObj = InfoRows(value.rows as Array);
            card.redrawUIComponent();
            PlacePanels();
            Log("card \"" + String(value.title) + "\": " + (value.rows as Array).length + " rows, " +
                card.numChildren + " entries, " + (mods.numChildren - 1) + " mods, panel" + note);
        }

        public function SetRepair(value:Object):void
        {
            repairState = value;
            repairSelection = 0;
            repairConfirmSelection = 0;
            repairConfirm = false;
            repairPending = false;
            repairPointerX = NaN;
            repairPointerY = NaN;
            focusRect = false;
            tabEnabled = false;
            tabChildren = false;
            content.visible = false;
            mouseEnabled = true;
            mouseChildren = true;
            visible = true;
            if (stage != null)
            {
                stage.stageFocusRect = false;
                stage.focus = NativeInput ? null : this;
                DrawRepair();
            }
        }

        public function DrawFooter():void
        {
            footer.visible = layout.workbench != true && layout.hints != null;
            if (!footer.visible) return;
            footer.SetHints(layout.hints as Array, 0xFFFFFF, Number(layout.stageWidth) - 80);
            footer.x = Number(layout.stageWidth) / 2;
            footer.y = 685;
        }

        private function onFooterAction(event:MouseEvent):void
        {
            var action:String = footer.ActionAt(content.mouseX, content.mouseY);
            if (action.length > 0 && BGSCodeObj != null && BGSCodeObj.FooterAction != null)
                BGSCodeObj.FooterAction(action);
        }

        public function SetInputDevice(gamepad:Boolean):void
        {
            if (repairGamepad == gamepad) return;
            repairGamepad = gamepad;
            if (repairState != null) DrawRepairHints();
        }

        private function DrawRepairHints():void
        {
            repairHints.visible = repairState != null;
            repairHints.x = 640;
            repairHints.y = 660;
            repairHints.SetHints([
                {id:"select", key:repairGamepad ? "_DPad_Up / _DPad_Down" : "Up / Down", label:"$B21_TFA_PromptSelect"},
                {id:"accept", key:repairGamepad ? "Xenon_A" : "ENTER", label:"$B21_TFA_PromptConfirm", enabled:!repairPending && repairState.methods[repairSelection].available},
                {id:"cancel", key:repairGamepad ? "Xenon_B" : "TAB", label:"$B21_TFA_RepairCancel"}
            ]);
        }

        private function onRepairHint(event:MouseEvent):void
        {
            if (!NativeInput) PointRepair(mouseX, mouseY, true);
        }

        public function WantsPointer():Boolean
        {
            return footer.visible && stage != null && footer.hitTestPoint(stage.mouseX, stage.mouseY, true);
        }

        public function DrawRepair():void
        {
            while (repairContent.numChildren > 0)
            {
                repairContent.removeChildAt(0);
            }
            var shade:Shape = new Shape();
            shade.graphics.beginFill(0, 0.9);
            shade.graphics.drawRect(0, 0, 1280, 720);
            shade.graphics.endFill();
            repairContent.addChild(shade);
            var panel:MovieClip = new StatsPanel();
            repairContent.addChild(panel);
            Heading(panel).text = "$B21_TFA_Repair";
            FitPanel(panel, new Rectangle(360, 180, 560, 310));
            var panelBottom:Number = 490;
            AddText(repairContent, String(repairState.title), 28, true, 375, 190, 530, "center", true);
            AddText(repairContent, "$ItemInfo_CND", 21, false, 380, 240, 120, "left", false);
            AddText(repairContent, Math.round(Number(repairState.health) * 100) + "%", 21, false, 760, 240, 140, "right", true);
            var methods:Array = repairState.methods as Array;
            if (repairConfirm)
            {
                var method:Object = methods[repairSelection];
                AddText(repairContent, String(method.label), 24, true, 385, 285, 510, "center", true);
                AddText(repairContent, Math.round(Number(repairState.health) * 100) + "%  >  " + method.target + "%",
                    22, false, 385, 323, 510, "center", true);
                if (method.details != null && String(method.details).length > 0)
                {
                    var details:TextField = AddText(repairContent, String(method.details), 18, false, 370, 348, 540, "center", true);
                }
                var confirmY:Number = details != null ? Math.max(380, details.y + details.height + 10) : 380;
                RepairButton("$B21_TFA_ConfirmRepair", 0, confirmY, true, onRepairAccept);
                RepairButton("$B21_TFA_RepairCancel", 1, confirmY + 50, true, onRepairCancel);
                panelBottom = Math.max(panelBottom, confirmY + 110);
            }
            else
            {
                for (var i:int = 0; i < methods.length; i++)
                {
                    var entry:Object = methods[i];
                    var button:Sprite = RepairButton(String(entry.label), i, 280 + i * 48, entry.available == true, onRepairChoose);
                    AddText(button, entry.target + "%" + (entry.id == "workbench" ? "" : "  (" + entry.count + ")"),
                        20, false, 310, 5, 190, "right", true);
                    if (!entry.available)
                    {
                        button.alpha = i == repairSelection ? 1 : 0.75;
                    }
                }
                var chosen:Object = methods[repairSelection];
                if (chosen.details != null && String(chosen.details).length > 0)
                {
                    details = AddText(repairContent, String(chosen.details), 18, false, 370, 438, 540, "center", true);
                    panelBottom = Math.max(panelBottom, details.y + details.height + 16);
                }
            }
            FitPanel(panel, new Rectangle(360, 180, 560, panelBottom - 180));
            if (String(repairState.notice).length > 0)
            {
                AddText(repairContent, String(repairState.notice), 20, false, 370, panelBottom + 15, 540, "center", true);
            }
            DrawRepairHints();
            RestyleText(repairContent);
            var selected:DisplayObjectContainer = repairContent.getChildByName(String(repairConfirm ? repairConfirmSelection : repairSelection)) as DisplayObjectContainer;
            if (selected != null)
                for (var j:int = 0; j < selected.numChildren; j++)
                    if (selected.getChildAt(j) is TextField) TextField(selected.getChildAt(j)).textColor = 0;
            if (!NativeInput && stage != null) stage.focus = this;
        }

        public function RepairButton(label:String, index:int, y:Number, enabled:Boolean, callback:Function):Sprite
        {
            var button:Sprite = new Sprite();
            button.name = String(index);
            button.x = 380;
            button.y = y;
            var selected:Boolean = index == (repairConfirm ? repairConfirmSelection : repairSelection);
            button.graphics.beginFill(0xFFFFFF, selected ? 0.85 : 0.07);
            button.graphics.drawRect(0, 0, 520, 42);
            button.graphics.endFill();
            button.mouseChildren = false;
            button.tabEnabled = false;
            button.tabChildren = false;
            button.focusRect = false;
            button.buttonMode = enabled && !repairPending;
            AddText(button, label, 21, true, 8, 5, 305, "left", true);
            button.addEventListener(MouseEvent.CLICK, callback);
            button.addEventListener(MouseEvent.MOUSE_MOVE, onRepairHover);
            repairContent.addChild(button);
            return button;
        }

        private function onRepairChoose(event:MouseEvent):void
        {
            if (NativeInput || repairPending) return;
            PointRepair(mouseX, mouseY, true);
        }

        private function onRepairAccept(event:MouseEvent):void
        {
            if (NativeInput) return;
            repairConfirmSelection = 0;
            NavigateRepair("accept");
        }
        private function onRepairCancel(event:MouseEvent):void { if (!NativeInput) NavigateRepair("cancel"); }

        private function onRepairHover(event:MouseEvent):void
        {
            if (!NativeInput) PointRepair(mouseX, mouseY, false);
        }

        public function PointRepair(x:Number, y:Number, click:Boolean):void
        {
            if (click && repairState != null) {
                var action:String = repairHints.ActionAt(x, y);
                if (action == "accept" || action == "cancel") {
                    NavigateRepair(action);
                    return;
                }
            }
            if (!click && x == repairPointerX && y == repairPointerY) return;
            repairPointerX = x;
            repairPointerY = y;
            if (repairState == null || repairPending || x < 380 || x > 900) return;
            var first:DisplayObject = repairContent.getChildByName("0");
            if (first == null) return;
            var top:Number = first.y;
            var step:Number = repairConfirm ? 50 : 48;
            var index:int = Math.floor((y - top) / step);
            if (index < 0 || index >= (repairConfirm ? 2 : repairState.methods.length) || y - top - index * step > 42) return;
            var previous:int = repairConfirm ? repairConfirmSelection : repairSelection;
            if (repairConfirm) repairConfirmSelection = index;
            else repairSelection = index;
            if (previous != index) DrawRepair();
            if (click) NavigateRepair("accept");
        }

        private function onRepairKey(event:KeyboardEvent):void
        {
            if (repairState == null) return;
            event.stopImmediatePropagation();
            event.preventDefault();
            if (NativeInput) return;
            if (event.keyCode == 38 || event.keyCode == 87) NavigateRepair("up");
            else if (event.keyCode == 40 || event.keyCode == 83) NavigateRepair("down");
            else if (event.keyCode == 13 || event.keyCode == 69) NavigateRepair("accept");
            else if (event.keyCode == 27 || event.keyCode == 9) NavigateRepair("cancel");
        }

        public function NavigateRepair(action:String):void
        {
            if (repairState == null) return;
            if (action == "cancel")
            {
                if (repairConfirm && !repairPending)
                {
                    repairConfirm = false;
                    DrawRepair();
                }
                else if (BGSCodeObj != null && BGSCodeObj.RepairAction != null)
                {
                    BGSCodeObj.RepairAction("close", String(repairState.revision));
                }
                return;
            }
            if (repairPending) return;
            var methods:Array = repairState.methods as Array;
            if (action == "up" || action == "down")
            {
                if (repairConfirm) repairConfirmSelection = 1 - repairConfirmSelection;
                else
                {
                    repairSelection += action == "up" ? -1 : 1;
                    if (repairSelection < 0) repairSelection = methods.length - 1;
                    else if (repairSelection >= methods.length) repairSelection = 0;
                }
                DrawRepair();
            }
            else if (action == "accept" && methods[repairSelection].available == true)
            {
                if (!repairConfirm)
                {
                    repairConfirm = true;
                    repairConfirmSelection = 0;
                    DrawRepair();
                }
                else if (repairConfirmSelection == 1) NavigateRepair("cancel");
                else if (BGSCodeObj != null && BGSCodeObj.RepairAction != null)
                {
                    repairPending = true;
                    DrawRepairHints();
                    BGSCodeObj.RepairAction(String(methods[repairSelection].id), String(repairState.revision));
                }
            }
        }

        private function onCardUpdated(event:Event):void
        {
            RestyleText(content);
            CurrencyIcons();
            PlacePanels();
        }

        // FO76 finds the caps icon by class name in ApplicationDomain.currentDomain. Nested in the
        // barter movie, GFx misses it there (2026-09-23 game screenshots; Ruffle finds it), so any
        // value row left without one gets it from the library this card loaded.
        public function CurrencyIcons():void
        {
            var domain:ApplicationDomain = icons.content != null ? icons.contentLoaderInfo.applicationDomain : null;
            if (domain == null || !domain.hasDefinition("IconCu_Caps")) return;
            for (var i:int = 0; i < card.numChildren; i++)
            {
                var row:ItemCard_ValueEntry = card.getChildAt(i) as ItemCard_ValueEntry;
                var holder:MovieClip = row != null ? row["Icon_mc"] as MovieClip : null;
                if (holder == null || HasCurrencyIcon(holder)) continue;
                var icon:MovieClip = new (domain.getDefinition("IconCu_Caps") as Class)() as MovieClip;
                holder.addChild(icon);
                // SWFLoaderClip.setContainerIconClip's own sizing.
                icon.scaleX = icon.scaleY = Number(holder["clipScale"]) || 1;
                if (Number(holder["clipWidth"]) > 0) icon.width = Number(holder["clipWidth"]);
                if (Number(holder["clipHeight"]) > 0) icon.height = Number(holder["clipHeight"]);
                icon.x += Number(holder["clipXOffset"]) || 0;
                icon.y += Number(holder["clipYOffset"]) || 0;
                if (!iconSupplied)
                {
                    iconSupplied = true;
                    Log("caps icon supplied from the card's currency library");
                }
            }
        }

        private static function HasCurrencyIcon(holder:MovieClip):Boolean
        {
            for (var i:int = 0; i < holder.numChildren; i++)
                if (getQualifiedClassName(holder.getChildAt(i)).indexOf("IconCu_") == 0) return true;
            return false;
        }

        public function PlacePanels():void
        {
            note = "";
            if (legendaryRow != null)
            {
                legendaryRow.y = card.getBounds(stats).y - card.entrySpacing - legendaryRow.height;
            }
            FitPanel(statsPanel, Rows(stats, statsPanel));
            FitPanel(modsPanel, Rows(mods, modsPanel));
            stats.x = layout.workbench == true || layout.barter == true ? Number(layout.statsLeft) :
                Number(layout.stageWidth) * 0.1625 - stats.getBounds(stats).x * stats.scaleX;
            stats.y = layout.workbench == true || layout.barter == true ? Number(layout.statsBottom) : 620;
            var bounds:Rectangle = mods.getBounds(mods);
            mods.x = Number(layout.stageWidth) - (stats.x + stats.getBounds(stats).x * stats.scaleX) - bounds.right * mods.scaleX;
            mods.y = stats.y - bounds.bottom * mods.scaleY;
        }

        // What a column drew, without the panel about to be sized around it. The card measures
        // wider than the rows it lays out, so its entries are measured one by one instead.
        public function Rows(target:Sprite, panel:MovieClip):Rectangle
        {
            var union:Rectangle = null;
            for (var i:int = 0; i < target.numChildren; i++)
            {
                var child:DisplayObject = target.getChildAt(i);
                if (child == panel || !child.visible)
                {
                    continue;
                }
                var parts:DisplayObjectContainer = child == card ? card : null;
                for (var j:int = 0; j < (parts != null ? parts.numChildren : 1); j++)
                {
                    var part:DisplayObject = parts != null ? parts.getChildAt(j) : child;
                    if (!part.visible)
                    {
                        continue;
                    }
                    var background:DisplayObject = part is DisplayObjectContainer ?
                        DisplayObjectContainer(part).getChildByName("Background_mc") : null;
                    var box:Rectangle = background != null ? background.getBounds(target) : part.getBounds(target);
                    union = union == null ? box : union.union(box);
                }
            }
            if (union != null)
            {
                var sourceRow:ItemCard_StandardEntry = new ItemCard_StandardEntry();
                union.width = sourceRow.getBounds(sourceRow).width;
            }
            return union == null ? new Rectangle() : union;
        }

        // FO76 grows the panel from the card's entry count; ours wraps whatever the column drew,
        // which is the same shape for any row count and survives FO4's narrower stage.
        public function FitPanel(panel:MovieClip, rows:Rectangle):void
        {
            panel.visible = rows.width > 0;
            if (!panel.visible)
            {
                return;
            }
            var box:DisplayObject = panel.getChildByName("Box_mc");
            var strip:DisplayObject = panel.getChildByName("StatsLabelBG_mc");
            var label:TextField = Heading(panel);
            if (box == null || strip == null || label == null)
            {
                Log("card panel art missing: box " + (box != null) + ", strip " + (strip != null) +
                    ", heading " + (label != null));
                return;
            }
            var art:Rectangle = strip.getBounds(strip);
            var top:Number = rows.y - art.height;
            Stretch(strip, rows.x, top, rows.width, art.height);
            Stretch(box, rows.x - PANEL_PAD, top - PANEL_PAD, rows.width + 2 * PANEL_PAD,
                rows.bottom - top + 2 * PANEL_PAD);
            label.setTextFormat(new TextFormat(boldFontName));
            label.embedFonts = fontsReady;
            label.x = rows.x + (rows.width - label.width) / 2;
            label.y = top + (art.height - label.height) / 2;
            note += " rows " + int(rows.width) + "x" + int(rows.height);
        }

        // Not the width and height setters: those scale a clip by the size GFx has measured for it,
        // and a clip it has not drawn yet measures zero, which scales the art away for good.
        // Reading the art's own box back at scale 1 makes every fit start from the same place.
        public function Stretch(art:DisplayObject, left:Number, top:Number, width:Number, height:Number):void
        {
            art.scaleX = 1;
            art.scaleY = 1;
            var box:Rectangle = art.getBounds(art);
            note += " " + art.name + " " + int(box.width) + "x" + int(box.height);
            if (box.width <= 0 || box.height <= 0)
            {
                return;
            }
            art.scaleX = width / box.width;
            art.scaleY = height / box.height;
            art.x = left - box.x * art.scaleX;
            art.y = top - box.y * art.scaleY;
        }

        public static function Heading(panel:MovieClip):TextField
        {
            return panel.getChildByName("itemStatsLabel_tf") as TextField;
        }

        public static function LegendaryStars(rows:Array):int
        {
            for each (var row:Object in rows)
            {
                if (row.kind == "legendary")
                {
                    return int(row.stars);
                }
            }
            return 0;
        }

        // Card rows in FO76 ItemCard's own data contract.
        public static function InfoRows(rows:Array):Array
        {
            var info:Array = [];
            for each (var row:Object in rows)
            {
                var kind:String = String(row.kind);
                if (kind == "legendary")
                {
                    continue;
                }
                if (kind == "condition")
                {
                    info.push({text: "$health", currentHealth: Number(row.fillPct) * 100, maximumHealth: 100});
                    info.push({text: "durability", value: row.maximumDurability != null ? Number(row.maximumDurability) : 100});
                }
                else if (kind == "multi")
                {
                    for each (var part:Object in row.parts as Array)
                    {
                        info.push({text: row.label, value: part.value, damageType: part.damageType, difference: part.difference});
                    }
                }
                else if (kind == "ammo")
                {
                    // These entries switch to their highlighted frame whenever difference is not 0.
                    info.push({text: row.label, value: row.value, damageType: 10, difference: Number(row.difference) || 0});
                }
                else if (kind == "fireMode")
                {
                    info.push({text: "$ATTACKMODE", value: row.value, difference: Number(row.difference) || 0});
                }
                else if (kind == "value")
                {
                    info.push({text: "$val", value: row.value});
                }
                else
                {
                    var entry:Object = {text: row.label, value: row.value, difference: row.difference};
                    // FO76 draws a timer on timed effects and a component list for junk from these alone.
                    if (Number(row.duration) > 0) entry.duration = Number(row.duration);
                    if (row.components is Array && (row.components as Array).length > 0) entry.components = row.components;
                    info.push(entry);
                }
            }
            return info;
        }

        public function DrawHeader(value:Object):void
        {
            while (header.numChildren > 0)
            {
                header.removeChildAt(0);
            }
            var width:Number = Number(layout.headerWidth);
            var left:Number = Number(layout.headerLeft);
            var title:TextField = AddText(header, String(value.title), 26, true, left, Number(layout.headerTop), width, "center", true);
            title.textColor = GOLD_COLOR;
            if (layout.barter != true) TrailingStars(header, title, int(value.stars), 9, GOLD_COLOR);
            var y:Number = Math.max(Number(layout.descriptionTop), title.y + title.height);
            if (layout.barter == true) { title.visible = false; y = Number(layout.headerTop); }
            for each (var effect:Object in value.effects as Array)
            {
                var line:TextField = AddText(header, String(effect.text), 18, false, left, y, width, "center", true);
                TrailingStars(header, line, int(effect.stars), 5.5);
                y += line.height;
            }
        }

        public function DrawMods(list:Array, scale:Number):void
        {
            for (var i:int = mods.numChildren - 1; i >= 0; i--)
            {
                if (mods.getChildAt(i) != modsPanel)
                {
                    mods.removeChildAt(i);
                }
            }
            mods.visible = list != null && list.length > 0;
            if (!mods.visible)
            {
                return;
            }
            var y:Number = 0;
            for each (var mod:Object in list)
            {
                var row:ItemCard_StandardEntry = new ItemCard_StandardEntry();
                row.PopulateEntry({text: String(mod.text), value: ""});
                RestyleText(row);
                row.y = y;
                if (int(mod.stars) > 0)
                {
                    TrailingStars(row, row.Label_tf, int(mod.stars), row.height * 0.22);
                }
                mods.addChild(row);
                y += row.height + card.entrySpacing;
            }
            mods.scaleX = scale;
            mods.scaleY = scale;
        }

        public function DrawTypeLine(words:Array):void
        {
            while (typeLine.numChildren > 0)
            {
                typeLine.removeChildAt(0);
            }
            if (words == null || words.length == 0)
            {
                return;
            }
            var x:Number = 0;
            // Each field keeps a 2px gutter on both sides; overlap them so the comma sits on its word.
            for (var i:int = 0; i < words.length; i++)
            {
                var word:TextField = AddText(typeLine, String(words[i]), 16, false, x, 0, 400, "left", false);
                x += word.width - 4;
                if (i + 1 < words.length)
                {
                    var comma:TextField = AddText(typeLine, ", ", 16, false, x, 0, 40, "left", false);
                    x += comma.width - 4;
                }
            }
            typeLine.x = Number(layout.headerLeft) + (Number(layout.headerWidth) - x) / 2;
            typeLine.y = Number(layout.hintsTop) - typeLine.height - 8;
        }

        public static function AddText(target:Sprite, text:String, size:Number, bold:Boolean, x:Number, y:Number,
                                       width:Number, align:String, wrap:Boolean):TextField
        {
            var field:TextField = new TextField();
            var format:TextFormat = new TextFormat(bold ? boldFontName : fontName, size, BODY_COLOR,
                bold, false, false, null, null, align);
            field.defaultTextFormat = format;
            field.embedFonts = fontsReady;
            field.name = OURS;
            field.selectable = false;
            field.mouseEnabled = false;
            field.multiline = wrap;
            field.wordWrap = wrap;
            field.width = width;
            field.autoSize = wrap ? TextFieldAutoSize.NONE : TextFieldAutoSize.LEFT;
            field.text = text;
            // A $ key is swapped for its translation as the text lands, and the substituted run
            // comes back 12pt black, so the size and colour have to be set over the top of it.
            field.setTextFormat(format);
            field.textColor = BODY_COLOR;
            if (wrap)
            {
                field.height = field.textHeight + 4;
            }
            field.x = x;
            field.y = y;
            target.addChild(field);
            return field;
        }

        // Stars after the last line a wrapped, centred field actually laid out.
        public static function TrailingStars(target:Sprite, field:TextField, count:int, radius:Number, color:uint = 0xFFFFCB):void
        {
            if (count <= 0 || field.numLines == 0)
            {
                return;
            }
            var last:TextLineMetrics = field.getLineMetrics(field.numLines - 1);
            AddStars(target, count, field.x + 2 + last.x + last.width + radius * 1.6,
                field.y + field.height - 2 - last.height / 2, radius, false, color);
        }

        // Filled five-point stars traced with moveTo/lineTo; FO4 crashes on GraphicsPath.
        public static function AddStars(target:Sprite, count:int, x:Number, y:Number, radius:Number, alignRight:Boolean, color:uint = 0xFFFFCB):Shape
        {
            var shape:Shape = new Shape();
            var spacing:Number = radius * 2.4;
            var first:Number = alignRight ? x - radius - (count - 1) * spacing : x + radius;
            shape.graphics.beginFill(color, 1);
            for (var star:int = 0; star < count; star++)
            {
                for (var point:int = 0; point <= 10; point++)
                {
                    var px:Number = first + star * spacing + Number(STAR_POINTS[(point % 10) * 2]) * radius;
                    var py:Number = y + Number(STAR_POINTS[(point % 10) * 2 + 1]) * radius;
                    if (point == 0)
                    {
                        shape.graphics.moveTo(px, py);
                    }
                    else
                    {
                        shape.graphics.lineTo(px, py);
                    }
                }
            }
            shape.graphics.endFill();
            target.addChild(shape);
            return shape;
        }
    }
}
