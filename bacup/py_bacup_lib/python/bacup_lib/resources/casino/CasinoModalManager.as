package {
    import flash.events.Event;
    import Shared.AS3.IMenu;
    import Shared.AS3.Data.BSUIDataManager;
    import Shared.AS3.Data.BSUIEventDispatcherBackend;
    import Shared.AS3.Events.CustomEvent;

    public class CasinoModalManager extends IMenu {
        public var BGSCodeObj:Object;
        public var B21CasinoBridgeVersion:uint;
        private var B21Revision:String;
        private var B21Generation:String;

        public function B21Initialize():void {
            BGSCodeObj = new Object();
            B21Revision = "";
            B21Generation = "";
            var backend:BSUIEventDispatcherBackend = new BSUIEventDispatcherBackend();
            backend.DispatchEventToGame = B21Event;
            BSUIDataManager.InitDataManager(backend);
            B21CasinoBridgeVersion = 1;
        }

        public function B21SetData(frame:Object, gamepad:Boolean):void {
            B21Revision = frame.revision;
            B21Generation = frame.generation;
            this.SetPlatform(gamepad ? 1 : 0, false, gamepad ? 1 : 0, 0);
            B21Publish("ScreenResolutionData", {ScreenWidth: 1920, ScreenHeight: 1080});
            B21Publish("CasinoData", {
                casinoGameType: frame.casinoGameType,
                availableChips: frame.availableChips,
                possiblePayout: frame.possiblePayout
            });
        }

        public function B21Publish(name:String, payload:Object):void {
            var provider:Object = BSUIDataManager.GetDataFromClient(name);
            for (var key:String in payload) provider.data[key] = payload[key];
            provider.SetReady(false);
            provider.DispatchChange();
        }

        public function B21Event(event:Event):void {
            if (BGSCodeObj == null || !BGSCodeObj.hasOwnProperty("CasinoAction")) return;
            var params:Object = event is CustomEvent ? CustomEvent(event).params : new Object();
            var payload:Object = new Object();
            for (var key:String in params) payload[key] = params[key];
            payload.revision = B21Revision;
            payload.generation = B21Generation;
            BGSCodeObj.CasinoAction(event.type, payload);
        }
    }
}
