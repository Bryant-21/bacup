package LegendaryPerksMenu_fla {
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.geom.Point;
    import Shared.AS3.Data.BSUIDataManager;
    import Shared.AS3.Data.BSUIEventDispatcherBackend;
    import Shared.AS3.Events.CustomEvent;

    public dynamic class MainTimeline extends MovieClip {
        public var LegendaryPerksMenu_mc:LegendaryPerksMenu;
        public var BGSCodeObj:Object;
        private var connected:Boolean = false;

        public function MainTimeline() {
            super();
        }

        public function B21SetData(payload:Object, gamepad:Boolean):void {
            if (!connected) {
                connected = true;
                root.transform.perspectiveProjection.fieldOfView = 122.353662;
                root.transform.perspectiveProjection.projectionCenter = new Point(960, 540);
                var backend:BSUIEventDispatcherBackend = new BSUIEventDispatcherBackend();
                backend.DispatchEventToGame = B21Event;
                BSUIDataManager.InitDataManager(backend);
                var screen:Object = new Object();
                screen.ScreenWidth = 1920;
                screen.ScreenHeight = 1080;
                B21Publish("ScreenResolutionData", screen);
                var character:Object = new Object();
                character.playerRace = 1;
                B21Publish("CharacterInfoData", character);
                var colors:Object = new Object();
                colors.hue = 0;
                colors.saturation = 0;
                colors.value = 0;
                colors.contrast = 0;
                B21Publish("HUDColors", colors);
            }
            this.LegendaryPerksMenu_mc.SetPlatform(gamepad ? 1 : 0, false, 0, 0);
            B21Publish("LegendaryPerksMenuData", payload);
        }

        public function B21Publish(providerName:String, payload:Object):void {
            var provider:Object = BSUIDataManager.GetDataFromClient(providerName);
            for (var key:String in payload) {
                provider.data[key] = payload[key];
            }
            provider.SetReady(false);
            provider.DispatchChange();
        }

        public function B21Event(event:Event):void {
            if (this.BGSCodeObj == null) return;
            var payload:Object = event is CustomEvent ? CustomEvent(event).params : new Object();
            this.BGSCodeObj.LegendaryEvent(event.type, payload);
        }

        public function ProcessUserEvent(eventName:String, pressed:Boolean):Boolean {
            return this.LegendaryPerksMenu_mc.ProcessUserEvent(eventName, pressed);
        }
    }
}
