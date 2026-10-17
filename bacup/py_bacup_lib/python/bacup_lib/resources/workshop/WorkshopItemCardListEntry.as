package {
    public class WorkshopItemCardListEntry extends Shared.AS3.BSScrollingListEntry {
        // TEMP: how many times the row took data and what icon it held, for Tales' card-row log.
        public var B21Debug:String;
        public var B21Calls:int;

        // SetEntryText starts by clearing the row's icon with removeChildren(), which FO4's Scaleform throws on when
        // there is nothing to remove. BSScrollingList.SetEntry swallows the error, so a row stopped taking data once it
        // held no icon: Produces kept the first row it drew ("Defense 2" on Water Pump and Generator - Fusion,
        // 2026-10-12), and reused requirement rows kept the previous card's materials.
        public function B21ClearIcon():void {
            B21Calls = B21Calls + 1;
            B21Debug = B21Calls + ": cleared " + Icon_mc.numChildren + " icon children";
            while (Icon_mc.numChildren > 0) Icon_mc.removeChildAt(0);
        }
    }
}
