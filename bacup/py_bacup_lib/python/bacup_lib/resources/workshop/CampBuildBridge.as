package {
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.display.MovieClip;
    import flash.display.Shape;
    import flash.display.Stage;
    import flash.display.InteractiveObject;
    import flash.filters.ColorMatrixFilter;
    import flash.ui.Keyboard;
    import flash.geom.ColorTransform;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.text.TextField;
    import flash.text.TextFieldAutoSize;
    import flash.utils.Dictionary;
    import flash.utils.getTimer;
    import Shared.AS3.BSScrollingListEntry;
    import Shared.AS3.Data.BSUIDataManager;
    import Shared.AS3.Data.BSUIEventDispatcherBackend;
    import Shared.AS3.Data.FromClientDataEvent;
    import Shared.AS3.Events.CustomEvent;

    public class WorkshopMenuNew extends Shared.AS3.IMenu {
        private var B21Code:Object;
        private var B21Backend:BSUIEventDispatcherBackend;
        private var B21Live:Boolean;
        private var B21Generation:String = "";
        private var B21Revision:String = "";
        private var B21Platform:uint;
        private var B21Sequence:uint;
        private var B21Painting:Boolean;
        private var B21Target:uint;
        private var B21Holding:Boolean;
        private var B21Wire:Boolean;
        private var B21CanWire:Boolean;
        private var B21CanRepair:Boolean;
        private var B21CanBuild:Boolean;
        private var B21CameraHelp:Object;
        private var B21Mappings:Object;
        private var B21Labels:Object;
        private var B21Filter:uint;
        private var B21CameraControls:Boolean;
        private var B21Center:Object;
        private var B21CameraDot:Shape;
        private var B21Stage:Stage;
        private var B21Focus:InteractiveObject;
        private var B21Tint:uint;
        private var B21Tinted:Dictionary;
        private var B21TintedText:Dictionary;
        private var B21SubMode:uint;
        private var B21CanReplace:Boolean;
        private var B21Replacing:Boolean;
        private var B21CanLock:Boolean;
        private var B21CanWallpaper:Boolean;
        private var B21GroupCount:uint;
        private var B21GroupSelected:Boolean;
        private var B21CanGroupSelect:Boolean;
        private var B21Blueprint:Boolean;
        private var B21LastSave:int;
        public var B21LastError:String = "";

        public function get B21CampBuildVersion():uint { return 14; }

        public function B21GetPreview():Object {
            if (!B21Live || B21CameraControls || stage == null || m_MainMode != MAINMODE_BUILD || !ItemCard_mc.visible) return null;
            // The model shows the item being chosen; the category panel isn't choosing one.
            if (m_CurrentHandler != itemListHandler && m_CurrentHandler != variantsPanelHandler) return null;
            var card:Rectangle = ItemCard_mc.Background_mc.getBounds(stage);
            var buttons:Rectangle = ButtonHintBar_mc.getBounds(stage);
            var bottom:Number = Math.min(stage.stageHeight, buttons.top) - 20;
            var top:Number = card.bottom + 20;
            if (card.width <= 0 || bottom <= top) return null;
            return {left:card.left, right:card.right, top:top, bottom:bottom};
        }

        public function B21SetPlatform(platform:uint):void {
            B21Platform = platform;
            SetPlatform(platform, false, platform, 0);
        }

        public function B21ProcessInput(name:String, pressed:Boolean):Boolean {
            if (!B21Live) return false;
            if (B21CameraControls && (name == "Up" || name == "Down" || name == "Left" || name == "Right")) return true;
            // FO76's options panel acts on its own rows; ours replace them, so its keys are handled here.
            if (m_CurrentHandler == quickMenuHandler) {
                if (name == "Accept" || name == "Activate") { if (!pressed) B21ChooseOption(); return true; }
                if (name == "Cancel" || name == "XButton" || name == "B21Options" || name == "CloseMenu") {
                    if (!pressed) onQuickMenuClose(null);
                    return true;
                }
                if (B21Platform != 0 && (name == "Up" || name == "Down")) {
                    if (!pressed && stage.focus != null)
                        stage.focus.dispatchEvent(new KeyboardEvent(KeyboardEvent.KEY_DOWN, true, false, 0,
                            name == "Up" ? Keyboard.UP : Keyboard.DOWN));
                    return true;
                }
                if (name == "RShoulder" || name.indexOf("B21") == 0) return true;
                return ProcessUserEvent(name, pressed);
            }
            if (name == "CloseMenu") {
                if (!pressed) onForceClose();
                return true;
            }
            if (name == "B21PreviousCategory" || name == "B21NextCategory") {
                if (!B21Holding && !B21Replacing)
                    CategoryList_mc.processUserEvent(name == "B21NextCategory" ? "NextCategory" : "PrevCategory", pressed);
                return true;
            }
            if (name == "B21Options") {
                if (!pressed) B21OpenOptions();
                return true;
            }
            if (m_MainMode == MAINMODE_BUILD && !B21CanBuild && (name == "Accept" || name == "Activate") &&
                m_CurrentHandler != categoryPanelHandler) return true;
            if (name == "RShoulder") {
                if (!pressed) requestToggleMainMode();
                return true;
            }
            if (m_MainMode == MAINMODE_MODIFY && B21SubMode == MODE_MODIFY_BLUEPRINTSAVE) {
                // FO76's save list: E saves into the row, a second E overwrites an existing blueprint, TAB goes back.
                if (name == "Accept" || name == "Activate") { if (!pressed) B21SaveBlueprint(); return true; }
                if (name == "Cancel") { if (!pressed && m_CurrentHandler == saveBlueprintHandler) m_CurrentHandler(CANCEL_BTN); return true; }
                if (B21Platform != 0 && (name == "Up" || name == "Down") && !pressed && stage.focus != null)
                    stage.focus.dispatchEvent(new KeyboardEvent(KeyboardEvent.KEY_DOWN, true, false, 0, name == "Up" ? Keyboard.UP : Keyboard.DOWN));
                return true;
            }
            if (m_MainMode == MAINMODE_MODIFY) {
                if (name == "LShoulder") {
                    if (!B21Holding && !B21Replacing) CategoryList_mc.processUserEvent("PrevCategory", pressed);
                    return true;
                }
                if (B21SubMode == MODE_MODIFY_GROUPSELECT) {
                    // FO76's group select: E selects or deselects the object in view, R adds its structure, T (pad LT) grows
                    // the selection by what touches it and SPACE saves it. TAB clears a selection, then exits.
                    if (name == "Up" || name == "Down" || name == "Left" || name == "Right") return ProcessUserEvent(name, pressed);
                    if (!pressed) {
                        if (name == "Accept" || name == "Activate") B21Group("B21::BlueprintToggle");
                        else if (name == "XButton") B21Group("B21::BlueprintStructure");
                        else if (name == "B21Grow") B21Group("B21::BlueprintGrow");
                        else if (name == "YButton") B21Group("B21::BlueprintCreate");
                        else if (name == "Cancel") { if (B21GroupCount > 0) B21Group("B21::BlueprintClear"); else onForceClose(); }
                    }
                    return true;
                }
                if (B21ModifyListed()) {
                    // Replace, Lock and Wallpapers browse their lists as Build does; E on the list applies the choice.
                    if (B21Replacing || name == "YButton") return true;
                    if ((name == "Accept" || name == "Activate") && m_CurrentHandler == itemListHandler) {
                        if (!pressed) B21ModifyApply();
                        return true;
                    }
                } else {
                    // As in FO76: E moves, R asks store or scrap, Z/C (pad LB) changes category.
                    if (name == "Up" || name == "Down" || name == "Left" || name == "Right")
                        return !B21Holding && ProcessUserEvent(name, pressed);
                    if (!pressed) {
                        if (name == "Accept" || name == "Activate") { if (B21CanRepair) B21ModifyRepair(); else B21ModifyAccept(); }
                        else if (name == "YButton") { if (B21Wire) B21ModifyAccept(); else B21ModifyWire(); }
                        else if (name == "XButton") B21OpenStoreScrap();
                        else if (name == "Cancel") { if (B21Holding) B21ModifyCancel(); else onForceClose(); }
                    }
                    return true;
                }
            }
            if (name == "XButton") {
                if (!pressed) B21OpenOptions();
                return true;
            }
            if (name == "Cancel") {
                if (!pressed && !ProcessUserEvent(name, false)) {
                    if (m_CurrentHandler == itemListHandler || m_CurrentHandler == variantsPanelHandler)
                        m_CurrentHandler(CANCEL_BTN);
                    else onForceClose();
                }
                return true;
            }
            if (B21Platform != 0 && ((name == "Up" || name == "Down") && m_CurrentHandler == itemListHandler ||
                (name == "Up" || name == "Down" || name == "Left" || name == "Right") && m_CurrentHandler == variantsPanelHandler)) {
                if (!pressed && stage.focus != null)
                    stage.focus.dispatchEvent(new KeyboardEvent(KeyboardEvent.KEY_DOWN, true, false, 0,
                        name == "Up" ? Keyboard.UP : name == "Down" ? Keyboard.DOWN :
                        name == "Left" ? Keyboard.LEFT : Keyboard.RIGHT));
                return true;
            }
            return ProcessUserEvent(name, pressed);
        }

        public function B21Start(code:Object):Boolean {
            if (code == null) return false;
            try {
                B21LastError = "";
                B21Generation = "";
                B21Revision = "";
                B21Sequence = 0;
                B21Painting = false;
                // Initialisers on bridge fields never run: the bridge is merged into FO76's class.
                B21Tint = 0xFFFFCB;
                B21Tinted = new Dictionary(true);
                B21TintedText = new Dictionary(true);
                B21SubMode = MODE_BUILD_DEFAULT;
                B21Code = code;
                B21Backend = new BSUIEventDispatcherBackend();
                B21Backend.DispatchEventToGame = B21Dispatch;
                B21Backend.InitDataManager();
                B21Live = true;
                addEventListener(Event.ENTER_FRAME, B21StopCategoryEntries);
                addEventListener(Event.ENTER_FRAME, B21FitSubcategories);
                B21Stage = stage;
                if (B21Stage != null) {
                    B21Stage.addEventListener(KeyboardEvent.KEY_DOWN, B21GateInput, true, 1000);
                    B21Stage.addEventListener(KeyboardEvent.KEY_UP, B21GateInput, true, 1000);
                    B21Stage.addEventListener(MouseEvent.MOUSE_WHEEL, B21GateInput, true, 1000);
                }
                // In a Tales-created menu a clip-depth mask only clips under a filtered ancestor; otherwise GFx
                // draws FO76's red mask shapes behind the category list and its icons. GFx skips an identity
                // matrix, so this one darkens by 1/255 (invisible) and never changes.
                for each (var masked:DisplayObject in [getChildByName("LeftContainer_mc"), ItemCard_mc])
                    if (masked != null)
                        masked.filters = [new ColorMatrixFilter([254/255,0,0,0,0,0,254/255,0,0,0,0,0,254/255,0,0,0,0,0,1,0])];
                onBuildModeEnter();
                return true;
            } catch (error:Error) {
                B21LastError = error.message;
                B21Live = false;
                B21Code = null;
                return false;
            }
        }

        private function B21Dispatch(event:Event):void {
            var category:Boolean = event.type == "WorkshopMenu::CategorySelect";
            var modify:Boolean = m_MainMode == MAINMODE_MODIFY;
            if (!B21Live || B21Painting || B21Code == null || B21Generation.length == 0 ||
                event.type.indexOf("ImageFixtureManager::") == 0 || event.type == "WorkshopMenu::ToggleMoreOptions") return;
            // Only Replace, Lock and Wallpapers have Modify lists; FO76 places from a list in Modify too, which applies the choice.
            if (modify && (event.type == "ModifyMode::ItemChanged" || event.type == "BuildMode::ItemPlace")) {
                var choice:Object = B21ModifyListed() && ItemList_mc.selectedIndex >= 0 &&
                    ItemList_mc.selectedIndex < ItemList_mc.entryList.length ? ItemList_mc.entryList[ItemList_mc.selectedIndex] : null;
                if (choice == null) return;
                if (event.type == "BuildMode::ItemPlace") { B21ModifyApply(); return; }
                var select:String = B21SubMode == MODE_MODIFY_LOCK ? "B21::LockSelect" :
                    B21SubMode == MODE_MODIFY_WALLPAPER ? "B21::WallpaperSelect" : "B21::ReplaceSelect";
                B21Code.Dispatch(select, {generation:B21Generation, revision:B21Revision,
                    request:++B21Sequence, params:{recipeID:choice.RecipeID}});
                return;
            }
            if (event.type == "ModifyMode::BlueprintSelected") {
                // The save list's Enter press arrives here as well as through E; one press saves once.
                if (getTimer() - B21LastSave < 200) return;
                B21LastSave = getTimer();
                var row:Object = CustomEvent(event).params;
                B21Code.Dispatch("B21::BlueprintSave", {generation:B21Generation, revision:B21Revision,
                    request:++B21Sequence, params:{recipeID:row.blueprintSelected, targetID:row.idLow}});
                return;
            }
            if (B21CameraControls && (category || event.type == "BuildMode::ItemChanged" || event.type == "BuildMode::CategoryChanged")) return;
            if (event.type == "BuildMode::ItemPlace" && !B21CanBuild) return;
            var source:Object = event is CustomEvent ? CustomEvent(event).params : {};
            var params:Object = {};
            for (var key:String in source) params[key] = source[key];
            if ((event.type == "BuildMode::ItemChanged" || event.type == "BuildMode::ItemPlace") &&
                ItemList_mc.selectedIndex >= 0 && ItemList_mc.selectedIndex < ItemList_mc.entryList.length) {
                params.recipeID = ItemList_mc.entryList[ItemList_mc.selectedIndex].RecipeID;
                params.itemRecipeID = params.recipeID;
                var variant:int = params.hasOwnProperty("activeVariantIndex") ? int(params.activeVariantIndex) :
                    event.type == "BuildMode::ItemPlace" ? m_LastSelectedVariantIndex : -1;
                if (m_CurrentHandler == variantsPanelHandler) variant = VariantList_mc.selectedIndex;
                if (variant < 0) variant = m_LastSelectedVariantIndex;
                if (m_VariantsParentID == params.itemRecipeID && VariantList_mc.entryData != null &&
                    variant >= 0 && variant < VariantList_mc.entryData.length)
                    params.recipeID = VariantList_mc.entryData[variant].RecipeID;
            }
            // FO76 reports a Build category whose sublist is empty this way; native takes it as the whole category.
            // In Modify, each category is a sub-mode, and native decides whether to enter it.
            if (category && !modify) params = {activeCategoryID:params.id, activeSubcategoryID:int.MAX_VALUE, navFromInput:params.navFromInput};
            var action:String = category ? (modify ? "B21::ModifyCategory" : "BuildMode::CategoryChanged") :
                event.type == EVENT_SWITCH_MODE ? "B21::ModeChange" : event.type;
            B21Code.Dispatch(action, {generation:B21Generation, revision:B21Revision,
                request:++B21Sequence, params:params});
        }

        public function B21Publish(channel:String, payload:Object):void {
            if (B21Live) BSUIDataManager.GetDataFromClient(channel).B21SetData(payload);
            if (channel == "WorkshopStatusData") FreeCamControlsText_mc.visible = false;
        }

        public function B21ApplyFrame(frame:Object):Boolean {
            if (!B21Live || B21LastError.length != 0 || frame == null || !(frame.generation is String) ||
                !(frame.revision is String) || !(frame.channels is Array)) return false;
            B21Generation = frame.generation;
            B21Revision = frame.revision;
            B21Painting = true;
            try {
                B21Target = frame.targetID;
                B21Holding = frame.holding;
                B21Wire = frame.wire;
                B21CanWire = frame.canWire;
                B21CanRepair = frame.canRepair;
                B21CanBuild = frame.canBuild;
                B21CanReplace = frame.canReplace == true;
                B21Replacing = frame.replacing == true;
                B21CanLock = frame.canLock == true;
                B21CanWallpaper = frame.canWallpaper == true;
                B21GroupCount = frame.groupCount is Number ? uint(frame.groupCount) : 0;
                B21GroupSelected = frame.groupSelected == true;
                B21CanGroupSelect = frame.canGroupSelect == true;
                B21Blueprint = frame.blueprint == true;
                B21CameraHelp = frame.cameraHelp;
                B21Center = frame.center;
                B21Mappings = frame.buttonMappings;
                B21Labels = frame.labels;
                B21Filter = frame.filter;
                if (frame.hudColor is Number) B21Tint = uint(frame.hudColor);
                B21SetMode(frame.mode);
                // Native owns the sub-mode, as FO76's server does: the movie only reports the category chosen.
                B21SubMode = m_MainMode == MAINMODE_MODIFY && frame.subMode is Number ? uint(frame.subMode) : MODE_BUILD_DEFAULT;
                setSubMode(B21SubMode);
                var asynchronous:Boolean = false;
                for each (var pending:Object in frame.channels) {
                    if (pending.name != "WorkshopCategoriesData") continue;
                    m_BuildModeCachedHandler = categoryPanelHandler;
                    m_ReturnToItemList = false;
                    m_ReturnToVariants = false;
                    resetCurrentHandler();
                    break;
                }
                for each (var channel:Object in frame.channels) {
                    if (channel.name == "WorkshopCategoriesData") asynchronous = true;
                    B21Publish(channel.name, channel.payload);
                    if (channel.name == "WorkshopItemCard") B21ShowBuildCount(channel.payload);
                }
                if (asynchronous) addEventListener(Event.ENTER_FRAME, B21FinishPainting, false, -1000);
                else B21Painting = false;
                CategoryList_mc.visible = true;
                if (m_MainMode == MAINMODE_MODIFY) {
                    ItemList_mc.visible = B21ModifyListed() && ItemList_mc.entryList != null && ItemList_mc.entryList.length > 0;
                    if (!ItemList_mc.visible && m_CurrentHandler == itemListHandler) m_CurrentHandler(CANCEL_BTN);
                }
                return true;
            } catch (error:Error) {
                B21LastError = error.message;
                B21Painting = false;
                return false;
            }
        }

        // FO4 has no per-object build limit. Native sends a maximum of 0, and BUILT shows the count alone instead of
        // FO76's "count/limit". The card writes its rows as the data arrives, so this follows each publish.
        private function B21ShowBuildCount(card:Object):void {
            if (card == null || !card.hasBuildCounts || card.maxBuildCount != 0) return;
            var inner:DisplayObjectContainer = ItemCard_mc.getChildByName("Internal_mc") as DisplayObjectContainer;
            var content:DisplayObjectContainer = inner != null ? inner.getChildByName("Content_mc") as DisplayObjectContainer : null;
            var row:DisplayObjectContainer = content != null ? content.getChildByName("BuildCount_mc") as DisplayObjectContainer : null;
            var count:TextField = row != null ? row.getChildByName("BuildCount_tf") as TextField : null;
            if (count != null) count.text = String(card.currentBuildCount);
        }

        // FO76's library stops each category entry on its *_Minimized and *_Expanded frames with frame scripts, but
        // the entries take the menu's script-less copy of the class, so an expand or minimize played on through every
        // state (seen in the Ruffle preview). Build hid it by re-stopping entries on each input; Modify's lists don't.
        private function B21StopCategoryEntries(event:Event):void {
            for (var i:int = 0; i < CategoryList_mc.numListItems_Inspectable; ++i) {
                var entry:MovieClip = CategoryList_mc.GetClipByIndex(i) as MovieClip;
                var label:String = entry != null ? entry.currentLabel : null;
                if (label != null && (label.indexOf("_Minimized") > 0 || label.indexOf("_Expanded") > 0)) entry.stop();
            }
        }

        // FO76's sub-category list shows 15 rows and scrolls only past the 15th, which fits its one-line names. Tales'
        // breadcrumb names ("Wall Decorations / Flags") wrap to two lines, so the last rows sat under the panel's mask
        // and the selection walked off it. Showing only the rows that fit lets the list scroll to keep the selection
        // among them. FO76 sets 15 rows again whenever the category list redraws.
        private function B21FitSubcategories(event:Event):void {
            var list:Object = CategoryList_mc.Sublist_mc;
            if (stage == null || list == null || !list.visible || list.displayedItemCount == 0 || CategoryList_mc.Mask_mc == null)
                return;
            // FO76's categoryPanelHandler_Exit shows the sub-list without laying the categories out again, and nothing
            // later does. In game (2026-10-12) the categories after the selected one were drawn over its sub-list.
            var parent:Object = CategoryList_mc.FindClipForEntry(CategoryList_mc.selectedIndex);
            var listTop:Number = parent != null ? parent.y + (parent.Sizer_mc != null ? parent.Sizer_mc.height : parent.height) : list.y;
            var unlaid:Boolean = Math.abs(list.y - listTop) > 1;
            for (var i:int = 0; i < CategoryList_mc.numListItems_Inspectable && !unlaid; ++i) {
                var category:Object = CategoryList_mc.GetClipByIndex(i);
                unlaid = category != null && category.visible && category.itemIndex > CategoryList_mc.selectedIndex &&
                    category.y < list.y + list.displayHeight - 1;
            }
            if (unlaid) {
                CategoryList_mc.UpdateList();
                return;
            }
            var bottom:Number = DisplayObject(CategoryList_mc.Mask_mc).getBounds(stage).bottom + 1;
            var fit:uint = 0;
            while (fit < list.displayedItemCount) {
                var clip:Object = list.getClipByIndex(fit);
                var row:DisplayObject = clip.Sizer_mc != null ? clip.Sizer_mc : DisplayObject(clip);
                if (row.getBounds(stage).bottom > bottom) break;
                ++fit;
            }
            if (fit < list.displayedItemCount) {
                list.maxRows = Math.max(1, fit);
                return;
            }
            // Fewer rows don't move the scroll on their own, so a selection past the last one wouldn't be drawn.
            var first:int = list.rowScrollPos;
            var last:int = first + int(list.maxDisplayedItems) - 1;
            if (list.selectedIndex > last) list.rowScrollPos = list.selectedIndex - (last - first);
            else if (list.selectedIndex >= 0 && list.selectedIndex < first) list.rowScrollPos = list.selectedIndex;
        }

        private function B21FinishPainting(event:Event):void {
            removeEventListener(Event.ENTER_FRAME, B21FinishPainting);
            B21Painting = false;
        }

        public function B21CategoriesUpdate(event:FromClientDataEvent):void {
            try {
                if (event.data == null || !(event.data.mainList is Array)) return;
                CategoryList_mc.entryList = event.data.mainList.slice(0, event.data.count);
                CategoryList_mc.InvalidateData();
                addEventListener(Event.ENTER_FRAME, B21CategoriesReady, false, -500);
            } catch (error:Error) { B21LastError = error.message; }
        }

        private function B21CategoriesReady(event:Event):void {
            removeEventListener(Event.ENTER_FRAME, B21CategoriesReady);
            try {
                if (CategoryList_mc.entryList.length > 0) {
                    CategoryList_mc.selectedIndex = 0;
                    CategoryList_mc.refreshSublistData();
                    CategoryList_mc.selectedSublistIndex = 0;
                }
                onCategoryChange(null);
                B21PaintTint();
            } catch (error:Error) { B21LastError = error.message; }
        }

        public function B21SetMode(mode:uint):void {
            if (m_MainMode == mode) return;
            setMainMode(mode);
        }

        public function B21SetCoreHints(camera:Boolean = false, controls:Boolean = false, key:String = "F10"):void {
            var owned:Boolean = camera && !controls;
            if (owned && !B21CameraControls && B21Stage != null) B21Focus = B21Stage.focus;
            if (!owned && B21CameraControls && B21Stage != null) {
                B21Stage.focus = B21Focus != null && B21Focus.stage == B21Stage ? B21Focus :
                    m_CurrentHandler == variantsPanelHandler ? VariantList_mc :
                    m_CurrentHandler == itemListHandler ? ItemList_mc : CategoryList_mc;
                B21Focus = null;
            }
            B21CameraControls = owned;
            mouseChildren = !B21CameraControls;
            B21ShowCameraHelp();
            B21ShowCameraDot(camera);
            var entries:Array = [];
            for (var i:uint = 0; i <= SWITCHMODE_BUTTON_INDEX; ++i)
                entries.push(B21Hint("", "", "", "", "", false));
            var modify:Boolean = m_MainMode == MAINMODE_MODIFY;
            entries[SWITCHMODE_BUTTON_INDEX] = B21MappedHint("RShoulder", modify ? "$BUILD" : "$MODIFY", "Q", "RB", "requestToggleMainMode", true);
            if (modify && B21SubMode == MODE_MODIFY_BLUEPRINTSAVE) {
                entries.push(B21MappedHint("Accept", "$SAVE BLUEPRINT", "E", "A", "", true));
                entries.push(B21Hint("$NAVIGATE", "Arrows", "LS", "LS", "", true));
                entries.push(B21MappedHint("Cancel", "$CANCEL", "TAB", "B", "", true));
            } else if (modify && B21SubMode == MODE_MODIFY_GROUPSELECT) {
                // The object in view can join when native allows it; a selected one can always leave.
                var looking:Boolean = B21Target != 0 && !B21Replacing;
                var pick:Object = B21MappedHint("Accept", B21GroupSelected ? "$B21_TFA_Deselect" : "$SELECT", "E", "A", "", true);
                pick.IsButtonEnabled = looking && (B21GroupSelected || B21CanGroupSelect);
                entries.push(pick);
                var structure:Object = B21MappedHint("XButton", "$SELECT STRUCTURE", "R", "X", "", true);
                structure.IsButtonEnabled = looking && B21CanGroupSelect;
                entries.push(structure);
                var grow:Object = B21Hint("$GROW SELECTION", "T", "LT", "LT", "", true);
                grow.IsButtonEnabled = B21GroupCount > 0;
                entries.push(grow);
                var create:Object = B21MappedHint("YButton", "$CREATE BLUEPRINT", "SPACE", "Y", "", true);
                create.IsButtonEnabled = B21GroupCount > 0;
                entries.push(create);
                if (B21GroupCount > 0) entries.push(B21MappedHint("Cancel", "$CLEAR", "TAB", "B", "", true));
                entries.push(B21Hint("$CATEGORIES", "Z/C", "LB", "LB", "", true));
                entries.push(B21Hint("$WORKSHOPTOGGLEFREECAM", "V", "Back", "Back", "", true));
                entries.push(B21Hint("$EXIT", "Esc", "B", "B", "onForceClose", true));
            } else if (modify && B21ModifyListed()) {
                // E enters the list, then applies its selection: a replacement, a lock or wallpaper, or Remove taking
                // the lock (017F84) or wallpaper (B21Remove) off.
                var list:Boolean = m_CurrentHandler == itemListHandler;
                var selection:Object = ItemList_mc.selectedIndex >= 0 && ItemList_mc.selectedIndex < ItemList_mc.entryList.length ?
                    ItemList_mc.entryList[ItemList_mc.selectedIndex] : null;
                var apply:String = "$REPLACE";
                if (B21SubMode == MODE_MODIFY_LOCK) apply = selection != null && selection.RecipeID == 0x017F84 ? "$REMOVE LOCK" : "$LOCK";
                else if (B21SubMode == MODE_MODIFY_WALLPAPER) apply = selection != null && selection.B21Remove == true ? "$REMOVE" : "$APPLY";
                var swap:Object = B21MappedHint("Accept", list ? apply : "$SELECT", "E", "A", list ? "B21ModifyApply" : "", true);
                swap.IsButtonEnabled = !B21Replacing && (!list || B21CanApply());
                entries.push(swap);
                entries.push(B21Hint("$NAVIGATE", "Arrows", "LS", "LS", "", !B21Replacing));
                var switching:Object = B21Hint("$CATEGORIES", "Z/C", "LB", "LB", "", true);
                switching.IsButtonEnabled = !B21Replacing;
                entries.push(switching);
                entries.push(B21Hint("$WORKSHOPTOGGLEFREECAM", "V", "Back", "Back", "", true));
                entries.push(B21Hint("$OPTIONS", "X", "X", "X", "B21OpenOptions", !B21Replacing));
                entries.push(B21Hint("$EXIT", "Esc", "B", "B", "onForceClose", !B21Replacing));
            } else if (modify) {
                // FO76's Modify bar. Its actions grey out without a target instead of disappearing.
                var target:Boolean = B21Editing() && B21Target != 0;
                var move:Object = B21MappedHint("Accept", B21CanRepair ? "$REPAIR" : B21Wire ? "$ATTACH WIRE" : B21Holding ? "$PLACE" : "$MOVE",
                    "E", "A", B21CanRepair ? "B21ModifyRepair" : "B21ModifyAccept", true);
                move.IsButtonEnabled = target;
                entries.push(move);
                var storeScrap:Object = B21MappedHint("XButton", "$STORE/SCRAP", "R", "X", "B21OpenStoreScrap", true);
                storeScrap.IsButtonEnabled = target && !B21Wire;
                entries.push(storeScrap);
                if (B21CanWire) entries.push(B21MappedHint("YButton", "$ATTACH WIRE", "SPACE", "Y", "B21ModifyWire", true));
                if (B21Holding) entries.push(B21MappedHint("Cancel", "$CANCEL", "TAB", "B", "B21ModifyCancel", true));
                var categories:Object = B21Hint("$CATEGORIES", "Z/C", "LB", "LB", "", true);
                categories.IsButtonEnabled = !B21Holding;
                entries.push(categories);
                var tag:Object = B21Hint("$TAG FOR SEARCH", "T", "Y", "Y", "", true);
                tag.IsButtonEnabled = false;
                entries.push(tag);
                entries.push(B21Hint("$WORKSHOPTOGGLEFREECAM", "V", "Back", "Back", "", true));
                // The pad's X is Store/Scrap here, so OPTIONS is a keyboard key in Modify.
                entries.push(B21Hint("$OPTIONS", "X", "X", "X", "B21OpenOptions", B21Platform == 0));
                entries.push(B21Hint("$EXIT", "Esc", "B", "B", "onForceClose", !B21Holding));
            } else {
                // FO76's bar drops a hint whose IsEnabled is false; IsButtonEnabled greys it out instead.
                var build:Object = B21MappedHint("Accept", "$BUILD", "E", "A", "onAccept", true);
                build.IsButtonEnabled = B21CanBuild;
                entries.push(build);
                entries.push(B21Hint("$NAVIGATE", "Arrows", "LS", "LS", "", true));
                entries.push(B21Hint("$CATEGORIES", "Z/C", "LB", "LB", "", true));
                entries.push(B21Hint("$TAG FOR SEARCH", "T", "Y", "Y", "", true));
                entries.push(B21Hint("$WORKSHOPTOGGLEFREECAM", "V", "Back", "Back", "", true));
                entries.push(B21Hint("$OPTIONS", "X", "X", "X", "B21OpenOptions", true));
                entries.push(B21Hint("$EXIT", "Esc", "B", "B", "onForceClose", true));
            }
            entries.push(B21Hint(controls ? "$B21_TFA_BuildCameraControls" : "$B21_TFA_BuildMenuControls",
                key, "LS", "LS", "B21ToggleControls", camera));
            updateButtonBar({EntryList:entries});
            ModeHeader_mc.ModeButton_mc.visible = true;
            B21PaintTint();
        }

        // The recipe filters, RECENT and variants belong to Build; in Modify only the card expands.
        public function B21OpenOptions():void {
            if (B21CameraControls || B21Holding) return;
            var building:Boolean = m_MainMode == MAINMODE_BUILD;
            B21ShowPanel([
                B21Option(B21Filter == 1 ? "$FilterKnown_DISABLE" : "$FilterKnown", "b21Filter", building, B21Filter == 1 ? 0 : 1),
                B21Option(B21Filter == 2 ? "$FilterBuildable_DISABLE" : "$FilterBuildable", "b21Filter", building, B21Filter == 2 ? 0 : 2),
                B21Option(B21Filter == 3 ? "$FilterUnlockable_DISABLE" : "$FilterUnlockable", "b21Filter", building, B21Filter == 3 ? 0 : 3),
                B21Option("$RECENT", "b21Recent", building && B21RecentIndex() >= 0, 0),
                B21Option("$ToggleVariants", "toggleVariants", building, 0),
                B21Option("$ExpandInfoCard", "expandInfoCard", ItemCard_mc.visible && Object(ItemCard_mc).expandable, 0),
                B21Option("$DELETE BLUEPRINT", "b21DeleteBlueprint", building && B21Blueprint, 0)]);
        }

        // FO76 asks whether to store or scrap the object, but its movie has no panel for that, so this one asks.
        public function B21OpenStoreScrap():void {
            if (B21CameraControls || !B21Editing() || B21Target == 0 || B21Wire) return;
            B21ShowPanel([B21Option("$SCRAP", "b21Scrap", true, 0), B21Option("$STORE", "b21Store", true, 0)]);
        }

        private function B21Editing():Boolean {
            return m_MainMode == MAINMODE_MODIFY && (B21SubMode == MODE_MODIFY_EDIT || B21SubMode == MODE_MODIFY_OBJECTMOVE);
        }

        private function B21ModifyListed():Boolean {
            return m_MainMode == MAINMODE_MODIFY &&
                (B21SubMode == MODE_MODIFY_REPLACE || B21SubMode == MODE_MODIFY_LOCK || B21SubMode == MODE_MODIFY_WALLPAPER);
        }

        private function B21CanApply():Boolean {
            return B21SubMode == MODE_MODIFY_LOCK ? B21CanLock : B21SubMode == MODE_MODIFY_WALLPAPER ? B21CanWallpaper : B21CanReplace;
        }

        private function B21ShowPanel(rows:Array):void {
            if (m_CurrentHandler == quickMenuHandler) { onQuickMenuClose(null); return; }
            onMoreOptions();
            if (m_CurrentHandler != quickMenuHandler) return;
            var list:Object = Object(QuickMenuPanel_mc).List_mc;
            // FO76 makes one clip per row of its own five; each status update re-measures every row's clip.
            if (list.numListItems_Inspectable != rows.length) {
                list.numListItems_Inspectable = rows.length;
                list.SetNumListItems(rows.length);
            }
            list.entryList = rows;
            list.InvalidateData();
            // FO76's art fits its five rows. It ends below our last row, padded as it is above the first.
            var panel:DisplayObject = QuickMenuPanel_mc;
            var art:DisplayObject = Object(QuickMenuPanel_mc).MainBG_mc.BGInner_mc;
            var first:Rectangle = DisplayObject(list.GetClipByIndex(0)).getBounds(panel);
            var last:Rectangle = DisplayObject(list.GetClipByIndex(rows.length - 1)).getBounds(panel);
            var bounds:Rectangle = art.getBounds(panel);
            art.height *= (last.bottom + first.top - 2 * bounds.top) / bounds.height;
            art.y += bounds.top - art.getBounds(panel).top;
            list.selectedIndex = 0;
            B21PaintTint();
        }

        private function B21Option(label:String, action:String, enabled:Boolean, filter:uint):Object {
            if (B21Labels != null && B21Labels.hasOwnProperty(label)) label = B21Labels[label];
            return {labelKey:label, eventName:action, currentState:"", actionEnabled:enabled, b21Filter:filter};
        }

        private function B21ChooseOption():void {
            var list:Object = Object(QuickMenuPanel_mc).List_mc;
            var index:int = list.selectedIndex;
            var row:Object = index >= 0 && index < list.entryList.length ? list.entryList[index] : null;
            if (row == null || !row.actionEnabled) return;
            // FO76's own handler leaves the panel itself.
            if (row.eventName == "expandInfoCard") { onQuickMenuExpandInfoCard(null); return; }
            onQuickMenuClose(null);
            if (row.eventName == "toggleVariants") toggleMinimizedVariantsMode();
            else if (row.eventName == "b21Filter") B21Dispatch(new CustomEvent("B21::FilterChange", {newMode:row.b21Filter}));
            else if (row.eventName == "b21Recent") CategoryList_mc.selectedIndex = B21RecentIndex();
            else if (row.eventName == "b21Store") B21ModifyStore();
            else if (row.eventName == "b21Scrap") B21ModifyScrap();
            else if (row.eventName == "b21DeleteBlueprint") B21DeleteBlueprint();
        }

        // Native pins the recently built recipes as the first category, under this id.
        private function B21RecentIndex():int {
            var entries:Object = CategoryList_mc.entryList;
            for (var i:int = 0; entries != null && i < entries.length; ++i)
                if (entries[i].formID == 4294967294) return i;
            return -1;
        }

        // FO4's HUD colour over FO76's #FFFFCB art, as in the other converted menus. Thumbnails and the deliberately
        // red, green and blue parts keep their colours, so the tint goes on the largest subtrees without one.
        private function B21PaintTint():void {
            for (var i:int = 0; i < numChildren; ++i) {
                var clip:DisplayObject = getChildAt(i);
                var exempt:Dictionary = new Dictionary(true);
                B21FindExempt(clip, exempt);
                B21PaintClip(clip, exempt, false);
            }
        }

        private function B21Exempt(clip:DisplayObject):Boolean {
            if (clip == B21CameraDot) return true;
            var name:String = clip.name;
            if (name == "Image_mc" || name == "ErrorMessage_mc" || name == "BlueprintBudget_mc" ||
                name == "SaveBlueprint_mc" || name == "MeterStored_mc") return true;
            var label:String = clip is MovieClip ? MovieClip(clip).currentLabel : null;
            return label != null && (label.indexOf("Red") == 0 || label.indexOf("red") == 0 ||
                label.indexOf("Green") == 0 || label.indexOf("green") == 0);
        }

        // Marks an exempt clip 2 and each ancestor of one 1. A child can read back as null while its timeline
        // frame is being rebuilt.
        private function B21FindExempt(clip:DisplayObject, exempt:Dictionary):Boolean {
            if (clip == null) return false;
            if (B21Exempt(clip)) { exempt[clip] = 2; return true; }
            var found:Boolean = false;
            var container:DisplayObjectContainer = clip as DisplayObjectContainer;
            if (container != null)
                for (var i:int = 0; i < container.numChildren; ++i)
                    if (B21FindExempt(container.getChildAt(i), exempt)) found = true;
            if (found) exempt[clip] = 1;
            return found;
        }

        // Below a tinted or an exempt clip, every clip keeps its own colour. Once a script writes a clip's colour, its
        // parent's timeline stops animating it, which froze FO76's selection highlight and grey states. So only clips
        // no timeline colours are written: list entries, which scripts create, and children of single-frame clips.
        // Other text, such as the mode header's, takes the tint through its text colour, which no timeline animates.
        private function B21PaintClip(clip:DisplayObject, exempt:Dictionary, covered:Boolean):void {
            if (clip == null) return;
            var parent:MovieClip = clip.parent as MovieClip;
            var writable:Boolean = clip is BSScrollingListEntry || parent == null || parent.totalFrames <= 1;
            var whole:Boolean = !covered && exempt[clip] == null && writable;
            if (clip is TextField && !writable) B21TintText(TextField(clip), !covered && exempt[clip] == null);
            else B21TintClip(clip, whole);
            var container:DisplayObjectContainer = clip as DisplayObjectContainer;
            if (container != null)
                for (var i:int = 0; i < container.numChildren; ++i)
                    B21PaintClip(container.getChildAt(i), exempt, covered || whole || exempt[clip] == 2);
        }

        // The clip's own multipliers are kept, and its alpha and offsets are never touched.
        private function B21TintClip(clip:DisplayObject, on:Boolean):void {
            var own:Array = B21Tinted[clip] as Array;
            if (!on && own == null) return;
            var color:ColorTransform = clip.transform.colorTransform;
            if (own == null) B21Tinted[clip] = own = [color.redMultiplier, color.greenMultiplier, color.blueMultiplier];
            color.redMultiplier = own[0] * (on ? (B21Tint >> 16 & 255) / 255 : 1);
            color.greenMultiplier = own[1] * (on ? (B21Tint >> 8 & 255) / 255 : 1);
            color.blueMultiplier = own[2] * (on ? (B21Tint & 255) / 203 : 1);
            clip.transform.colorTransform = color;
            if (!on) B21Tinted[clip] = null;
        }

        // Keeps the text's own colour and what was written over it; a colour FO76 sets later becomes its own.
        private function B21TintText(text:TextField, on:Boolean):void {
            var record:Array = B21TintedText[text] as Array;
            if (record != null && text.textColor != record[1]) record = null;
            if (!on && record == null) return;
            var own:uint = record != null ? uint(record[0]) : text.textColor;
            text.textColor = !on ? own : B21TintChannel(own >> 16 & 255, B21Tint >> 16 & 255, 255) << 16 |
                B21TintChannel(own >> 8 & 255, B21Tint >> 8 & 255, 255) << 8 | B21TintChannel(own & 255, B21Tint & 255, 203);
            B21TintedText[text] = on ? [own, text.textColor] : null;
        }

        private function B21TintChannel(own:uint, tint:uint, base:uint):uint {
            return uint(Math.min(255, Math.round(own * tint / base)));
        }

        private function B21ShowCameraHelp():void {
            FreeCamControlsText_mc.visible = B21CameraControls && B21CameraHelp != null;
            if (!FreeCamControlsText_mc.visible) return;
            var lines:Array = B21Platform == 0 ? B21CameraHelp.pc : B21CameraHelp.pad;
            if (lines == null || lines.length != 2) { FreeCamControlsText_mc.visible = false; return; }
            clearCreatedTextFields();
            var upper:TextField = FreeCamControlsText_mc.getChildByName("ControlsTextUpper_tf") as TextField;
            var lower:TextField = FreeCamControlsText_mc.getChildByName("ControlsTextLower_tf") as TextField;
            var backer:Object = FreeCamControlsText_mc.getChildByName("Backer_mc");
            if (upper == null || lower == null || backer == null) { FreeCamControlsText_mc.visible = false; return; }
            if (FreeCamControlsText_mc.parent != this) addChild(FreeCamControlsText_mc);
            var card:Rectangle = ItemCard_mc.Background_mc.getBounds(this);
            FreeCamControlsText_mc.x = card.left;
            FreeCamControlsText_mc.y = card.bottom + 20;
            backer.width = card.width;
            for (var i:int = 0; i < FreeCamControlsText_mc.numChildren; ++i) {
                var child:Object = FreeCamControlsText_mc.getChildAt(i);
                child.visible = child == upper || child == lower || child == backer;
            }
            for each (var field:TextField in [upper, lower]) {
                field.x = 20;
                field.width = backer.width - 40;
                field.multiline = field.wordWrap = true;
                field.autoSize = TextFieldAutoSize.LEFT;
            }
            upper.y = 20;
            upper.text = lines[0];
            lower.y = upper.y + upper.textHeight + 12;
            lower.text = lines[1];
            backer.height = lower.y + lower.textHeight + 20;
        }

        // The stock HUD crosshair is not drawn under the free camera; FO76 keeps a centre dot there.
        private function B21ShowCameraDot(camera:Boolean):void {
            if (B21CameraDot == null) {
                // GFx draws translucent strokes opaque, so the outline is a second opaque fill.
                B21CameraDot = new Shape();
                B21CameraDot.graphics.beginFill(0x000000);
                B21CameraDot.graphics.drawCircle(0, 0, 4);
                B21CameraDot.graphics.endFill();
                B21CameraDot.graphics.beginFill(0xFFFFFF);
                B21CameraDot.graphics.drawCircle(0, 0, 2.5);
                B21CameraDot.graphics.endFill();
            }
            B21CameraDot.visible = camera && B21Center != null && B21Center.x is Number && B21Center.y is Number;
            if (!B21CameraDot.visible) return;
            addChild(B21CameraDot);
            var centre:Point = globalToLocal(new Point(B21Center.x, B21Center.y));
            B21CameraDot.x = centre.x;
            B21CameraDot.y = centre.y;
        }

        private function B21Edit(action:String):void {
            if (m_MainMode != MAINMODE_MODIFY || (!B21Target || !B21Editing()) && action != "B21::ModifyCancel") return;
            B21Dispatch(new CustomEvent(action, {targetID:B21Target, holding:B21Holding, wire:B21Wire}));
        }

        public function B21ModifyAccept():void { B21Edit("B21::ModifyAccept"); }
        public function B21ModifyScrap():void { if (!B21Wire) B21Edit("B21::ModifyScrap"); }
        public function B21ModifyStore():void { if (!B21Wire) B21Edit("B21::ModifyStore"); }
        public function B21ModifyCancel():void { B21Edit("B21::ModifyCancel"); }
        public function B21ModifyWire():void { if (B21CanWire) B21Edit("B21::ModifyWire"); }
        public function B21ModifyRepair():void { if (B21CanRepair) B21Edit("B21::ModifyRepair"); }

        public function B21ModifyReplace():void {
            if (m_MainMode != MAINMODE_MODIFY || B21SubMode != MODE_MODIFY_REPLACE || !B21CanReplace || B21Replacing ||
                B21Target == 0 || ItemList_mc.selectedIndex < 0 || ItemList_mc.selectedIndex >= ItemList_mc.entryList.length) return;
            B21Dispatch(new CustomEvent("B21::ModifyReplace",
                {targetID:B21Target, recipeID:ItemList_mc.entryList[ItemList_mc.selectedIndex].RecipeID}));
        }

        public function B21ModifyLock():void {
            if (m_MainMode != MAINMODE_MODIFY || B21SubMode != MODE_MODIFY_LOCK || !B21CanLock || B21Replacing ||
                B21Target == 0 || ItemList_mc.selectedIndex < 0 || ItemList_mc.selectedIndex >= ItemList_mc.entryList.length) return;
            B21Dispatch(new CustomEvent("B21::ModifyLock",
                {targetID:B21Target, recipeID:ItemList_mc.entryList[ItemList_mc.selectedIndex].RecipeID}));
        }

        public function B21ModifyWallpaper():void {
            if (m_MainMode != MAINMODE_MODIFY || B21SubMode != MODE_MODIFY_WALLPAPER || !B21CanWallpaper || B21Replacing ||
                B21Target == 0 || ItemList_mc.selectedIndex < 0 || ItemList_mc.selectedIndex >= ItemList_mc.entryList.length) return;
            B21Dispatch(new CustomEvent("B21::ModifyWallpaper",
                {targetID:B21Target, recipeID:ItemList_mc.entryList[ItemList_mc.selectedIndex].RecipeID}));
        }

        public function B21ModifyApply():void {
            if (B21SubMode == MODE_MODIFY_LOCK) B21ModifyLock();
            else if (B21SubMode == MODE_MODIFY_WALLPAPER) B21ModifyWallpaper();
            else B21ModifyReplace();
        }

        private function B21Group(action:String):void {
            if (!B21Replacing) B21Dispatch(new CustomEvent(action, {targetID:B21Target}));
        }

        public function B21SaveBlueprint():void {
            if (B21SubMode == MODE_MODIFY_BLUEPRINTSAVE && SaveBlueprint_mc.selectedEntry != null) onBlueprintPress(null);
        }

        public function B21DeleteBlueprint():void {
            if (m_MainMode != MAINMODE_BUILD || !B21Blueprint || ItemList_mc.selectedIndex < 0 ||
                ItemList_mc.selectedIndex >= ItemList_mc.entryList.length) return;
            B21Dispatch(new CustomEvent("B21::BlueprintDelete", {recipeID:ItemList_mc.entryList[ItemList_mc.selectedIndex].RecipeID}));
        }

        // FO76 strings native shows; naming them here keeps them in the converted translation table.
        private function B21NativeStrings():Array {
            return ["$BLUEPRINTS", "$BlueprintOverwrite", "$BlueprintTooBig", "$BlueprintInvalidItem",
                "$WorkshopBlueprintRequirementError", "$Workshop_CantPlaceBlueprint_BadConditions"];
        }

        public function B21ToggleControls():void {
            B21Dispatch(new Event("B21::CameraControlsToggle"));
        }

        private function B21GateInput(event:Event):void {
            if (B21Live && B21CameraControls) event.stopImmediatePropagation();
        }

        private function B21Hint(label:String, pc:String, xbox:String, psn:String,
            callback:String, enabled:Boolean):Object {
            // FO4 draws `$` keys in these fields as boxes; native sends the translated text.
            if (B21Labels != null && B21Labels.hasOwnProperty(label)) label = B21Labels[label];
            return {Name:label, Mappings:{PCButton:pc, XboxButton:xbox, PSNButton:psn},
                ScriptFunc:callback, Event:"", IsEnabled:enabled, IsVisible:enabled,
                IsButtonEnabled:enabled, IsFlashing:false, IsHold:false};
        }

        private function B21MappedHint(event:String, label:String, pc:String, controller:String,
            callback:String, enabled:Boolean):Object {
            var hint:Object = B21Hint(label, pc, controller, controller, callback, enabled);
            if (B21Mappings != null && B21Mappings.hasOwnProperty(event)) hint.Mappings = B21Mappings[event];
            return hint;
        }

        public function B21Stop():void {
            if (B21Stage != null) {
                B21Stage.removeEventListener(KeyboardEvent.KEY_DOWN, B21GateInput, true);
                B21Stage.removeEventListener(KeyboardEvent.KEY_UP, B21GateInput, true);
                B21Stage.removeEventListener(MouseEvent.MOUSE_WHEEL, B21GateInput, true);
                B21Stage = null;
            }
            B21CameraControls = false;
            B21CanBuild = false;
            B21CameraHelp = null;
            B21Center = null;
            if (B21CameraDot != null) B21CameraDot.visible = false;
            FreeCamControlsText_mc.visible = false;
            B21Focus = null;
            mouseChildren = true;
            removeEventListener(Event.ENTER_FRAME, B21FinishPainting);
            removeEventListener(Event.ENTER_FRAME, B21CategoriesReady);
            removeEventListener(Event.ENTER_FRAME, B21StopCategoryEntries);
            removeEventListener(Event.ENTER_FRAME, B21FitSubcategories);
            B21Live = false;
            B21Generation = "";
            B21Revision = "";
            var subscriptions:Array = [
                ["ScreenResolutionData", onScreenResolutionData],
                ["WorkshopCategoriesData", B21CategoriesUpdate],
                ["WorkshopItemsData", onItemsDataUpdate],
                ["WorkshopVariantsData", onVariantsDataUpdate],
                ["WorkshopStatusData", onStatusDataUpdate],
                ["WorkshopItemCard", onItemCardUpdate],
                ["WorkshopButtonBarData", onWorkshopButtonBarUpdate],
                ["WorkshopBlueprintData", onBlueprintDataUpdate],
                ["WorkshopUpdateItem", onUpdateItemEntry],
                ["WorkshopUpdateCategory", onUpdateCategoryEntry],
                ["FireForgetEvent", onFireForgetEventUpdate]
            ];
            for each (var subscription:Array in subscriptions)
                BSUIDataManager.Unsubscribe(subscription[0], subscription[1]);
            if (B21Backend != null) B21Backend.DispatchEventToGame = null;
            B21Code = null;
        }
    }
}
