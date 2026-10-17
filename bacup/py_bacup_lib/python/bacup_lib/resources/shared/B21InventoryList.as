package Shared.AS3
{
    import flash.display.MovieClip;
    import flash.events.Event;

    public class BSScrollingList extends MovieClip
    {
        public var B21Enabled:Boolean;
        public var B21Mode:uint;
        public var B21Descending:Boolean;
        public var B21Query:String;
        public var B21Columns:Boolean;
        public var B21Icons:Boolean;
        public var B21Options:String;
        public var B21LayoutEnabled:Boolean;
        // Set by hosts whose art reserves a cell for the headings: its center's height above the first row.
        public var B21HeadingCenter:Number;
        public var B21DataRevision:uint;
        public var B21MetadataRevision:Number;
        private var B21Source:Array;
        private var B21Controls:B21InventoryControls;
        private var B21RestoreListIndex:Boolean;
        private var B21ProjectionPending:Boolean;
        private var B21PendingIndex:int;
        private var B21PreviousIndex:int;
        private var B21Refreshing:Boolean;
        public function get B21SearchEditing():Boolean { return B21Controls != null && B21Controls.editing; }

        public function B21Presentation(color:uint, search:Boolean):void
        {
            if (B21Controls == null && B21Enabled)
            {
                B21Controls = new B21InventoryControls(this);
                addChild(B21Controls);
            }
            if (B21Controls != null) B21Controls.configure(color, search);
        }

        public function B21TextInput(text:String):void { if (B21Controls != null) B21Controls.textInput(text); }
        public function B21SearchInput(action:String):void { if (B21Controls != null) B21Controls.searchInput(action); }

        public function get B21ViewIndex():int { return iSelectedIndex; }
        public function set B21ViewIndex(index:int):void { doSetSelectedIndex(index); }
        public function get B21ViewEntries():Array { return EntriesA; }

        public function B21GetSelectedIndex():int
        {
            return B21Enabled ? B21InventoryRules.nativeIndex(EntriesA, iSelectedIndex) : iSelectedIndex;
        }

        public function B21SelectEngineIndex(index:int):void
        {
            doSetSelectedIndex(B21Enabled ? B21InventoryRules.viewIndex(EntriesA, index) : index);
        }

        public function B21ClampNativeIndex(index:int):int
        {
            if (!B21Enabled) return _filterer.ClampIndex(index);
            var view:int = B21InventoryRules.viewIndex(EntriesA, index);
            return B21InventoryRules.nativeIndex(EntriesA, _filterer.ClampIndex(view < 0 ? 0 : view));
        }

        public function B21GetSource():Array { return B21Source == null ? EntriesA : B21Source; }

        public function B21SetSource(source:Array):Array
        {
            if (source == null) source = [];
            B21Source = source;
            B21DataRevision++;
            return B21Enabled ? B21InventoryRules.project(source, B21Mode, B21Descending, B21Query) : source;
        }

        public function B21Reproject():void
        {
            if (!B21Refreshing) B21DataRevision++;
            if (B21Source == null || !B21Enabled && B21Source === EntriesA) return;
            var selected:Object = iSelectedIndex >= 0 && iSelectedIndex < EntriesA.length ? EntriesA[iSelectedIndex] : null;
            B21PreviousIndex = B21GetSelectedIndex();
            EntriesA = B21Enabled ? B21InventoryRules.project(B21Source, B21Mode, B21Descending, B21Query) : B21Source;
            _filterer.filterArray = EntriesA;
            B21PendingIndex = selected == null ? -1 : EntriesA.indexOf(selected);
            B21ProjectionPending = true;
            iSelectedIndex = -1;
        }

        public function B21FinishProjection():void
        {
            if (!B21ProjectionPending) return;
            B21ProjectionPending = false;
            doSetSelectedIndex(B21PendingIndex);
            if (B21PendingIndex < 0 && B21PreviousIndex >= 0) dispatchEvent(new Event(SELECTION_CHANGE, true, true));
        }

        public function B21Configure(enabled:Boolean, columns:Boolean, icons:Boolean, mode:uint, descending:Boolean):void
        {
            if (B21Enabled == enabled && B21Columns == columns && B21Icons == icons &&
                B21Mode == mode && B21Descending == descending) return;
            B21Source = B21GetSource();
            if (enabled && !B21Enabled) { B21RestoreListIndex = restoreListIndex; restoreListIndex = false; }
            if (!enabled && B21Enabled) restoreListIndex = B21RestoreListIndex;
            B21Enabled = enabled;
            if (!enabled) B21Query = "";
            B21Columns = enabled && columns;
            B21Icons = enabled && icons;
            B21Mode = mode;
            B21Descending = descending;
            B21RefreshView();
        }

        public function B21RefreshView():void
        {
            B21Refreshing = true;
            InvalidateData();
            B21Refreshing = false;
            if (iSelectedIndex < 0 && EntriesA.length) doSetSelectedIndex(_filterer.ClampIndex(0));
        }

        public function B21Search(query:String):void
        {
            B21Query = query;
            B21RefreshView();
        }

        public function B21Sort(mode:uint):void
        {
            B21Descending = B21Mode == mode ? !B21Descending : mode >= 2;
            B21Mode = mode;
            B21RefreshView();
        }
    }
}
