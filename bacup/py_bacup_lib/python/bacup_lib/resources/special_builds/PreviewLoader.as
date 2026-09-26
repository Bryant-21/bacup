package {
    import flash.display.MovieClip;
    import flash.display.Loader;
    import flash.events.Event;
    import flash.net.URLRequest;
    import flash.system.ApplicationDomain;
    import flash.system.LoaderContext;

    public class PreviewLoader extends MovieClip {
        private var paths:Array;
        private var loader:Loader;
        public function PreviewLoader() {
            paths = ["fonts_en.swf", "bsbuttonhintbar.swf", "perkslibrary_small.swf", "preview.swf"];
            next();
        }
        private function next(event:Event = null):void {
            if (paths.length == 0) { addChild(loader); return; }
            loader = new Loader();
            loader.contentLoaderInfo.addEventListener(Event.COMPLETE, next);
            loader.load(new URLRequest(paths.shift()), new LoaderContext(false, ApplicationDomain.currentDomain));
        }
    }
}
