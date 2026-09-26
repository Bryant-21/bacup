package SpecialBuilds {
    import Shared.AS3.Data.BSUIDataManager;
    public class SpecialBuildsShared {
        public static const SPECIAL_NAME_ARRAY:Array = ["$STRENGTH", "$PERCEPTION", "$ENDURANCE", "$CHARISMA", "$INTELLIGENCE", "$AGILITY", "$LUCK"];
        public static const SPECIAL_ABBREV_ARRAY:Array = ["STR", "PER", "END", "CHA", "INT", "AGI", "LCK"];
        public static const MAX_SPECIAL_VALUE:uint = 15;
        public static const MIN_SPECIAL_VALUE:uint = 1;
        public static const MAX_SPECIAL_POINTS:uint = 49;
        public static const MAX_DISPLAYED_PERKS:uint = 8;
        public static const NUM_OF_SPECIALS:uint = 7;
        public static const EVENT_EDIT_SPECIAL_MODAL_CLOSE:String = "SpecialBuilds::EditSpecialModalClose";
        public static const EVENT_SPECIAL_RIGHT_ARROW_CLICK:String = "SpecialBuilds::SpecialRightArrowClick";
        public static const EVENT_SPECIAL_LEFT_ARROW_CLICK:String = "SpecialBuilds::SpecialLeftArrowClick";
        public static const EVENT_SPECIAL_BUILDS_DATA:String = "SpecialBuildsData";
        public static const EVENT_LOAD_BUILD:String = "SpecialBuilds::LoadBuild";
        public static const EVENT_RESET_BUILD:String = "SpecialBuilds::ResetBuild";
        public static const EVENT_RENAME_BUILD:String = "SpecialBuilds::RenameBuild";
        public static const EVENT_SAVE_BUILD:String = "SpecialBuilds::SaveBuild";
        public static const EVENT_UNLOCK_SLOT:String = "SpecialBuilds::UnlockSlot";
        public static const EVENT_CLOSE_MENU:String = "SpecialBuilds::CloseMenu";
        public static const EVENT_EDIT_ACTIVE_PERKS:String = "SpecialBuilds::EditActivePerks";
        public static const EVENT_RESPEC_SPECIAL:String = "SpecialBuilds::RespecSpecial";
        public static const LINKAGE_EDIT_SPECIAL_LIST_ENTRY:String = "SpecialBuilds.EditSpecialListEntry";
        public static const LINKAGE_SPECIAL_BUILDS_ENTRY:String = "SpecialBuilds.SpecialBuildsListEntry";
        public static const LINKAGE_PERK_LIST_ENTRY:String = "SpecialBuilds.PerkListEntry";
        public static function getSpecialNameFromIndex(index:uint):String {
            return index < 7 ? SPECIAL_NAME_ARRAY[index] : "";
        }
        public static function getSpecialAbbrevFromIndex(index:uint):String {
            return index < 7 ? SPECIAL_ABBREV_ARRAY[index] : "";
        }
        public static function getMaxPointsAvailableForCharacter():uint {
            var data:Object = BSUIDataManager.GetDataFromClient("SpecialBuildsData").data;
            return Math.max(0, Math.min(49, int(data.totalPoints) - 7));
        }
        public static function calculatePointsAvailable(specials:Array):uint {
            var points:int = getMaxPointsAvailableForCharacter();
            if (specials == null || specials.length != 7) return 0;
            for each (var stat:Object in specials) points -= int(stat.value) - 1;
            return Math.max(0, points);
        }
    }
}
