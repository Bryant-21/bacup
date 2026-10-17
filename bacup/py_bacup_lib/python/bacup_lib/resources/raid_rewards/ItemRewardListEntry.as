package {
    import Shared.AS3.BSScrollingListEntry;

    public class ItemRewardListEntry extends BSScrollingListEntry {
        public function B21SetRewardName(value:String):void {
            B21RewardStars.setText(RewardName_mc.Name_tf,value);
        }
    }
}
