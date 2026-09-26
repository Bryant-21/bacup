package {
    import flash.display.MovieClip;
    public class PerkCardRankConfirmation extends MovieClip {
        public var ProducedCard_mc:Object = {dataObj: {}};
        public function SetData(target:Object, consume:Object):void {
            ProducedCard_mc.dataObj = {isAnimatedPerk: target.isAnimatedPerk || consume.isAnimatedPerk};
        }
    }
}
