package {
    import flash.display.MovieClip;

    public class PerkCardRankConfirmation extends MovieClip {
        public function B21SetData(target:Object, consume:Object):void {
            this.SetData(target, consume);
            var result:Object = this.ProducedCard_mc.dataObj;
            result.isAnimatedPerk = target.isAnimatedPerk;
            this.ProducedCard_mc.dataObj = result;
        }
    }
}
