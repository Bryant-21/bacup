package {
    import flash.display.DisplayObject;
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.MouseEvent;
    import flash.geom.Point;
    import flash.utils.getDefinitionByName;
    import flash.utils.getQualifiedClassName;
    import Shared.AS3.BSScrollingList;

    public class MainMenu extends MovieClip {
        private var b21Rows:B21_MainRows;
        private var b21Hidden:Array;
        private var b21Version:Object;
        private var b21Placed:Array = [];
        private var b21Grown:Array = [];
        public function get B21MainHostVersion():uint { return 2; }
        public function get B21MainHostFamily():String { return "__B21_HOST_FAMILY__"; }
        public var B21PhotoModeAction:Function;
        // Tales-owned settings: ("list") returns their option rows, ("set", id, value) applies one.
        public var B21SettingsAction:Function;

        public function B21InstallSettings():void {
            // Capture on the root runs before the stock handlers, which would hand the engine an id it
            // does not own.
            addEventListener(SettingsOptionItem.VALUE_CHANGE, B21SettingChanged, true, 100);
        }

        public function B21AppendDisplayOptions(options:Array):void {
            if (B21SettingsAction == null || !options) return;
            for each (var option:Object in B21SettingsAction("list") as Array) options.push(option);
        }

        private function B21SettingChanged(event:Event):void {
            var item:Object = event.target;
            var list:Object = OptionsPanel_mc.Fader_mc.List_mc;
            var entry:Object = item is SettingsOptionItem && list.entryList ? list.entryList[item.itemIndex] : null;
            if (B21SettingsAction == null || !entry || !entry.b21Setting) return;
            event.stopImmediatePropagation();
            // Filling the list sets each row's value, which also reports a change.
            if (entry.value == item.value) return;
            entry.value = item.value;
            B21SettingsAction("set", entry.ID, item.value);
        }

        public function B21InstallPhotoEntry():void {
            if (!PauseMode || B21PhotoModeAction == null || !MainPanel_mc || !MainPanel_mc.List_mc) return;
            var list:Object = MainPanel_mc.List_mc;
            for each (var row:Object in list.entryList) if (row.b21PhotoMode) return;
            var position:int = list.entryList.length;
            for (var i:int = 0; i < list.entryList.length; i++) {
                if (list.entryList[i].index == SETTINGS_INDEX) { position = i + 1; break; }
            }
            list.entryList.splice(position, 0, {text:"$B21_PM_Menu", index:-210076,
                disabled:!B21PhotoModeAction(false), b21PhotoMode:true});
            list.InvalidateData();
            addEventListener(Event.ENTER_FRAME, B21RefreshPhotoEntry);
        }

        private function B21RefreshPhotoEntry(event:Event):void {
            if (!PauseMode || B21PhotoModeAction == null) return;
            var list:Object = MainPanel_mc.List_mc;
            for each (var row:Object in list.entryList) {
                if (!row || !row.b21PhotoMode) continue;
                var disabled:Boolean = !B21PhotoModeAction(false);
                if (row.disabled != disabled) { row.disabled = disabled; list.InvalidateData(); }
            }
        }

        public function B21MainPress():void {
            var item:Object = MainPanel_mc.List_mc.selectedEntry;
            if (item && item.b21PhotoMode) {
                if (!item.disabled && B21PhotoModeAction != null && !shouldIgnoreInput)
                    B21PhotoModeAction(true);
                return;
            }
            onMainListItemPress();
        }

        public function B21ApplySourceStyle():void {
            if (b21Rows) return;
            b21Rows = new B21_MainRows();
            b21Rows.select = B21Select;
            b21Rows.accept = B21Accept;
            b21Rows.visible = false;
            // Field initialisers never run: this class is merged into the stock one, whose constructor stays.
            b21Hidden = [];
            b21Placed = [];
            b21Grown = [];
            addChild(b21Rows);
            addEventListener(Event.ENTER_FRAME, B21Refresh);
        }

        private function B21Select(index:int):void {
            if (currentState != MAIN_STATE || PauseMode || shouldIgnoreInput) return;
            var list:BSScrollingList = MainPanel_mc.List_mc;
            if (list.selectedIndex == index) return;
            list.selectedIndex = index;
            // Mirrors BSScrollingList.onEntryRollover, which the stock menu turns into its focus sound.
            if (list.selectedIndex == index) list.dispatchEvent(new Event(BSScrollingList.PLAY_FOCUS_SOUND, true, true));
        }

        private function B21Accept():void {
            if (currentState != MAIN_STATE || PauseMode || shouldIgnoreInput) return;
            MainPanel_mc.List_mc.dispatchEvent(new Event(BSScrollingList.ITEM_PRESS, true));
        }

        private function B21Refresh(event:Event):void {
            if (!stage || !b21Rows || !MainPanel_mc || !MainPanel_mc.List_mc) return;
            var saves:Boolean = !PauseMode && (currentState == SAVE_LOAD_STATE ||
                currentState == SAVE_LOAD_CONFIRM_STATE || currentState == DELETE_SAVE_CONFIRM_STATE ||
                currentState == CHARACTER_SELECT_STATE);
            var showing:Boolean = !PauseMode && MainPanel_mc.visible && (currentState == MAIN_STATE ||
                currentState == CONTINUE_CONFIRM_STATE || currentState == NEW_CONFIRM_STATE ||
                currentState == QUIT_CONFIRM_STATE || saves);
            var settings:Boolean = (currentState == SETTINGS_CATEGORY_STATE ||
                currentState == OPTIONS_LISTS_STATE || currentState == DEFAULT_SETTINGS_CONFIRM_STATE ||
                currentState == INPUT_MAPPING_STATE);
            if (!showing && !settings) {
                b21Rows.visible = false;
                B21Restore();
                return;
            }
            try {
                var scale:Number = Math.min(stage.stageWidth / 1920, stage.stageHeight / 1080);
                var top:Number = (stage.stageHeight - 1080 * scale) / 2;
                if (!b21Hidden.length) {
                    B21CoverStage();
                    B21Hide(MainPanel_mc);
                    B21Hide(BackgroundAndBrackets_mc);
                    B21Hide(BethesdaLogo_mc);
                    B21Hide(ButtonHintBar_mc.ButtonBracket_Left_mc);
                    B21Hide(ButtonHintBar_mc.ButtonBracket_Right_mc);
                }
                B21KeepCover();
                if (settings) {
                    b21Rows.visible = false;
                    var panels:Object = B21_MenuLayout.main.panels;
                    var rows:Number = scale * panels.rowRatio;
                    B21GrowList(OptionsPanel_mc.Fader_mc.List_mc, panels.options[1]);
                    B21GrowList(ControlsPanel_mc.Fader_mc.List_mc, panels.controls[1]);
                    B21Place(SettingsPanel_mc, SettingsPanel_mc.SettingsList_mc, panels.categories, rows, scale, top);
                    B21Place(OptionsPanel_mc, OptionsPanel_mc.Fader_mc.List_mc, panels.options, rows, scale, top);
                    B21Place(ControlsPanel_mc, ControlsPanel_mc.Fader_mc.List_mc, panels.controls, rows, scale, top);
                    B21Place(ButtonHintBar_mc, ButtonHintBar_mc, panels.hints, scale * 1920 / loaderInfo.width,
                        scale, top);
                    return;
                }
                if (!saves) B21Unplace();
                var entries:Array = [];
                for each (var item:Object in MainPanel_mc.List_mc.entryList) {
                    entries.push({text:item.text, index:item.index, disabled:item.disabled,
                        waitingForLoad:item.waitingForLoad,
                        b21Small:item.index == SETTINGS_INDEX || item.index == QUIT_INDEX ||
                            item.index == CREDITS_INDEX || item.index == HELP_INDEX || item.index == PLATFORM_HELP,
                        b21Image:item.index == LOAD_INDEX ? "character" : "play"});
                }
                b21Rows.Render(entries, int(MainPanel_mc.List_mc.selectedIndex));
                b21Rows.scaleX = b21Rows.scaleY = scale;
                b21Rows.x = B21_MenuLayout.main.x * scale;
                b21Rows.y = top + B21_MenuLayout.main.y * scale;
                b21Rows.visible = true;
                if (saves) {
                    panels = B21_MenuLayout.main.panels;
                    rows = scale * panels.rowRatio;
                    B21Place(SaveLoadHolder_mc, SaveLoadHolder_mc.Panel_mc.PlayerInfo_tf, panels.saves, rows, scale, top);
                    B21Place(CharacterSelectList_mc, CharacterSelectList_mc, panels.saves, rows, scale, top);
                    B21Place(ButtonHintBar_mc, ButtonHintBar_mc, panels.hints, scale * 1920 / loaderInfo.width,
                        scale, top);
                }
            } catch (error:Error) {
                b21Rows.visible = false;
                B21Restore();
            }
        }

        // The stock lists show 8 rows. FO76's run down to the button hints, so this fits as many as the
        // scaled panel leaves room for. SetNumListItems is not public, so this adds the extra rows the
        // way it does: same entry class, clip index, platform and mouse handlers, in the same holder.
        private function B21GrowList(list:Object, top:Number):void {
            if (!list || b21Grown.indexOf(list) >= 0 || !list.GetClipByIndex(0)) return;
            b21Grown.push(list);
            var panels:Object = B21_MenuLayout.main.panels;
            var shown:int = int(list.numListItems);
            var pitch:Number = list.border.height / shown;
            var rows:int = Math.floor((panels.hints[1] - top) / (pitch * panels.rowRatio)) - 1;
            if (rows <= shown) return;
            var first:MovieClip = list.GetClipByIndex(0);
            var entryClass:Class = getDefinitionByName(getQualifiedClassName(first)) as Class;
            for (var i:int = shown; i < rows; i++) {
                var clip:Object = new entryClass();
                clip.iPlatform = first.iPlatform;
                clip.clipIndex = i;
                clip.addEventListener(MouseEvent.MOUSE_OVER, list.onEntryRollover);
                clip.addEventListener(MouseEvent.CLICK, list.onEntryPress);
                first.parent.addChild(clip as DisplayObject);
            }
            var extra:Number = pitch * (rows - shown);
            list.border.height += extra;
            if (list.ScrollDown) list.ScrollDown.y += extra;
            list.numListItems = rows;
            list.InvalidateData();
        }

        // Moves a stock panel so its anchor sits where the FO76 menu puts the matching list (a 1920x1080
        // layout); size is the panel's scale in this movie.
        private function B21Place(panel:MovieClip, anchor:DisplayObject, at:Array, size:Number, scale:Number,
                                  top:Number):void {
            if (!panel || !anchor) return;
            var saved:Boolean = false;
            for each (var entry:Object in b21Placed) if (entry.clip == panel) saved = true;
            if (!saved) b21Placed.push({clip:panel, x:panel.x, y:panel.y, scaleX:panel.scaleX, scaleY:panel.scaleY});
            panel.scaleX = panel.scaleY = size;
            var origin:Point = globalToLocal(anchor.localToGlobal(new Point(0, 0)));
            panel.x += at[0] * scale - origin.x;
            panel.y += top + at[1] * scale - origin.y;
        }

        private function B21Unplace():void {
            for each (var entry:Object in b21Placed)
                for (var key:String in entry) if (key != "clip") entry.clip[key] = entry[key];
            b21Placed = [];
        }

        // In game this movie draws only inside regions covered by its original content. Clips created
        // later (the source rows) never extend that region, and the faded stock panel no longer covers
        // it, so the rows were invisible. Stretching an existing, emptied text field over the stage
        // does extend it (verified in game; root graphics and new children do not).
        // UpdateButtons hides the version field outside the main state (and SetVersionText refills it),
        // which drops the drawable region again: the rows and the moved save screenshot vanished there.
        private function B21KeepCover():void {
            if (!b21Version || !VersionText_tf) return;
            if (!VersionText_tf.visible) {
                b21Version.visible = false;
                VersionText_tf.visible = true;
            }
            if (VersionText_tf.text != "") {
                b21Version.text = VersionText_tf.text;
                VersionText_tf.text = "";
            }
        }

        private function B21CoverStage():void {
            if (!VersionText_tf) return;
            b21Version = {text:VersionText_tf.text, visible:VersionText_tf.visible, x:VersionText_tf.x,
                y:VersionText_tf.y, width:VersionText_tf.width, height:VersionText_tf.height};
            var origin:Point = VersionText_tf.parent.globalToLocal(new Point(0, 0));
            var corner:Point = VersionText_tf.parent.globalToLocal(new Point(stage.stageWidth, stage.stageHeight));
            VersionText_tf.text = "";
            VersionText_tf.visible = true;
            VersionText_tf.x = origin.x;
            VersionText_tf.y = origin.y;
            VersionText_tf.width = corner.x - origin.x;
            VersionText_tf.height = corner.y - origin.y;
        }

        private function B21Hide(clip:MovieClip):void {
            if (!clip) return;
            b21Hidden.push({clip:clip, alpha:clip.alpha, mouse:clip.mouseChildren});
            clip.alpha = 0;
            clip.mouseChildren = false;
        }

        private function B21Restore():void {
            B21Unplace();
            for each (var entry:Object in b21Hidden) {
                entry.clip.alpha = entry.alpha;
                entry.clip.mouseChildren = entry.mouse;
            }
            b21Hidden = [];
            if (b21Version) {
                for (var key:String in b21Version) VersionText_tf[key] = b21Version[key];
                b21Version = null;
            }
        }

        public function B21ClearSourceStyle():void {
            removeEventListener(Event.ENTER_FRAME, B21Refresh);
            B21Restore();
            if (b21Rows && contains(b21Rows)) removeChild(b21Rows);
            b21Rows = null;
        }
    }
}
