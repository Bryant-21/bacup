package {
    import flash.display.MovieClip;
    import flash.display.Loader;
    import flash.events.Event;
    import flash.external.ExternalInterface;
    import flash.net.URLRequest;

    public class B21_ReputationPreview extends MovieClip {
        private var loader:Loader = new Loader();
        private var hud:Object;
        private var completed:uint = 0;
        private var completions:uint = 0;

        public function B21_ReputationPreview() {
            ExternalInterface.addCallback("sample", sample);
            ExternalInterface.addCallback("state", state);
            loader.contentLoaderInfo.addEventListener(Event.COMPLETE, ready);
            addChild(loader);
            loader.load(new URLRequest("reputationhud.swf"));
        }

        private function ready(event:Event):void {
            hud = loader.content;
            hud.BGSCodeObj = {ReputationDone:done};
        }

        private function done(sequence:uint):void { completed = sequence; completions++; }

        public function sample(data:Object):String {
            try { hud.B21SetReputation(data); return ""; }
            catch (error:Error) { return error.toString(); }
        }

        public function state():Object {
            if (hud == null) return {ready:false};
            return {ready:true, visible:hud.visible, done:completed, completions:completions,
                meter:hud.ReputationMeter_mc.meterPercent, reported:hud.B21Done,
                header:hud.ReputationMeter_mc.Header_mc.Header_tf.text,
                left:hud.ReputationMeter_mc.LeftStatusIcon_mc.Face_mc == null ? "" : hud.ReputationMeter_mc.LeftStatusIcon_mc.Face_mc.currentLabel,
                right:hud.ReputationMeter_mc.RightStatusIcon_mc.Face_mc == null ? "" : hud.ReputationMeter_mc.RightStatusIcon_mc.Face_mc.currentLabel};
        }
    }
}
