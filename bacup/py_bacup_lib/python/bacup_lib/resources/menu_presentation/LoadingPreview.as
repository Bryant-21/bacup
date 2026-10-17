package {
    import flash.display.MovieClip;
    import flash.display.StageScaleMode;
    import flash.display.StageAlign;
    import flash.events.Event;
    import flash.external.ExternalInterface;
    public class LoadingMenu extends MovieClip {
        public function B21PreviewBoot():void { addEventListener(Event.ENTER_FRAME, B21PreviewStart); }
        public function B21PreviewNothing():void {}
        private var b21PreviewReports:Array;
        private function B21PreviewReport(shown:Boolean, url:String, detail:String):void {
            b21PreviewReports.push({shown:shown, url:url, detail:detail});
        }
        private function B21PreviewStart(event:Event):void {
            removeEventListener(Event.ENTER_FRAME, B21PreviewStart);
            stage.scaleMode = StageScaleMode.NO_SCALE;
            stage.align = StageAlign.TOP_LEFT;
            B21Layout();
            BGSCodeObj = {requestLoadingText:B21PreviewNothing};
            SetLoadingText(1, "", B21_PreviewStrings.tip);
            SetLevel(25, 0.6);
            LeftText_mc.LevelText_tf.text = B21_PreviewStrings.values["$LEVEL"] + " 25";
            addEventListener(Event.ENTER_FRAME, B21TranslateLoading);
            b21PreviewReports = [];
            B21LoadingReport = B21PreviewReport;
            B21ShowPhoto("photo-background.png");
            ExternalInterface.addCallback("loadingShow", B21ShowPhoto);
            ExternalInterface.addCallback("loadingShowAll", B21ShowPhotos);
            ExternalInterface.addCallback("loadingClear", B21ClearPhoto);
            ExternalInterface.addCallback("loadingMinimal", SetMinimalMode);
            ExternalInterface.addCallback("loadingSpinner", SpinnerOnly);
            ExternalInterface.addCallback("loadingTip", B21PreviewTip);
            ExternalInterface.addCallback("loadingState", B21PreviewState);
            ExternalInterface.call("report", {ready:true});
        }
        public function B21PreviewTip(text:String):void { SetLoadingText(1, "", text); }
        private function B21TranslateLoading(event:Event):void {
            LeftText_mc.LoadingArea_tf.text = B21_PreviewStrings.values["$LOADING"];
        }
        public function B21PreviewState():Object {
            var bounds:Object = b21Photo && b21Photo.content ? b21Photo.getBounds(stage) : null;
            return {loaded:b21Photo != null && b21Photo.content != null,
                photoBounds:bounds ? {x:bounds.x, y:bounds.y, width:bounds.width, height:bounds.height} : null,
                photoInMenu:b21Photo != null && contains(b21Photo),
                frameVisible:Box_mc.visible, textVisible:LeftText_mc.visible, version:B21PhotoHostVersion,
                spinnerVisible:VaultTecLogo_mc.visible,
                logoFrames:MovieClip(VaultTecLogo_mc.getChildAt(0)).totalFrames,
                logoFrame:MovieClip(VaultTecLogo_mc.getChildAt(0)).currentFrame,
                visible:b21Photo != null && b21Photo.visible,
                photoWidth:b21Photo && b21Photo.content ? b21Photo.content.width : 0,
                photoHeight:b21Photo && b21Photo.content ? b21Photo.content.height : 0,
                text:LeftText_mc.LoadScreenText_tf.text,
                level:LeftText_mc.PlayerLevelMeter_mc.currentFrame, reports:b21PreviewReports};
        }
    }
}
