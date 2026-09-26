package {
    import flash.display.MovieClip;
    import flash.display.Loader;
    import flash.events.Event;
    import flash.external.ExternalInterface;
    import flash.net.URLLoader;
    import flash.net.URLRequest;

    public class PerkCardsMoviePreview extends MovieClip {
        private var loader:Loader;
        private var frameLoader:URLLoader;
        private var menu:Object;
        private var frame:Object;
        private var ticks:uint = 0;

        public function PerkCardsMoviePreview() {
            loader = new Loader();
            addChild(loader);
            loader.contentLoaderInfo.addEventListener(Event.COMPLETE, loaded);
            loader.load(new URLRequest("../ui/data/Interface/B21/TalesFromAppalachia/PerkCards/perksmenu.swf"));
            frameLoader = new URLLoader();
            frameLoader.addEventListener(Event.COMPLETE, frameLoaded);
            frameLoader.load(new URLRequest("frame.json"));
            addEventListener(Event.ENTER_FRAME, tick);
        }

        private function loaded(event:Event):void { menu = loader.content; }
        private function frameLoaded(event:Event):void { frame = JSON.parse(frameLoader.data); }
        private function capture(action:String, payload:Object):void {
            if (ExternalInterface.available) ExternalInterface.call("perkMovieEvent", action, JSON.stringify(payload));
        }
        private function tick(event:Event):void {
            if (menu == null || frame == null || ++ticks < 4) return;
            removeEventListener(Event.ENTER_FRAME, tick);
            menu.BGSCodeObj.PerkCardEvent = capture;
            menu.B21SetData(frame, false);
            if (ExternalInterface.available) ExternalInterface.call("perkMovieReady", frame.perks.levelUpPerkCardDataA.length);
        }
    }
}
