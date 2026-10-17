package {
    import flash.display.MovieClip;
    import Shared.GlobalFunc;

    public class MinigameDial extends MovieClip {
        public function B21SliceColor():uint {
            var data:Object = this.MenuBase.FishingData;
            return data.hasOwnProperty("hudColor") ? uint(data.hudColor) : GlobalFunc.COLOR_TEXT_BODY;
        }
    }
}
