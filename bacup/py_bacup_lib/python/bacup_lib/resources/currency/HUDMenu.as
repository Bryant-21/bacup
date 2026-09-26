package {
    import flash.display.MovieClip;
    import flash.display.Loader;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.geom.Rectangle;
    import flash.net.URLRequest;
    import flash.system.ApplicationDomain;
    import flash.system.LoaderContext;

    public dynamic class HUDMenu extends MovieClip {
        private var currency:HUDCurrencyUpdatesWidget;
        private var icons:Loader;
        private var packIcons:Loader;
        private var loadedLibraries:uint = 0;
        public var B21Ready:Boolean = false;
        public var B21Failed:Boolean = false;

        public function HUDMenu() {
            super();
            stop();
            mouseEnabled = false;
            mouseChildren = false;
            currency = new HUDCurrencyUpdatesWidget();
            addChild(currency);
            visible = false;
            icons = new Loader();
            icons.contentLoaderInfo.addEventListener(Event.COMPLETE, iconsLoaded);
            icons.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, iconsFailed);
            icons.load(new URLRequest("currencyiconlibrary.swf"),
                new LoaderContext(false, ApplicationDomain.currentDomain));
            packIcons = new Loader();
            packIcons.contentLoaderInfo.addEventListener(Event.COMPLETE, iconsLoaded);
            packIcons.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, iconsFailed);
            packIcons.load(new URLRequest("challengerewardiconlibrary.swf"),
                new LoaderContext(false, ApplicationDomain.currentDomain));
        }

        private function iconsLoaded(event:Event):void {
            B21Ready = ++loadedLibraries == 2;
        }

        private function iconsFailed(event:IOErrorEvent):void {
            B21Failed = true;
        }

        public function B21SetCurrency(data:Object):void {
            visible = B21Ready && data.visible;
            if (visible) {
                currency.B21Update(data);
                var bounds:Rectangle = currency.CurrencyBase_mc.getBounds(currency)
                    .union(currency.CurrencyIcon_mc.getBounds(currency));
                if (currency.CurrencyChange_mc.visible && currency.CurrencyChange_mc.alpha > 0)
                    bounds = bounds.union(currency.CurrencyChange_mc.getBounds(currency));
                currency.x = 960 - bounds.x - bounds.width / 2;
                currency.y = 860 - bounds.bottom;
            }
        }
    }
}
