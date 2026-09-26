package Shared.AS3.Data {
    public class BSUIDataManager {
        public static var backend:BSUIEventDispatcherBackend;
        public static var providers:Object = new Object();
        public static function InitDataManager(value:BSUIEventDispatcherBackend):void { backend = value; }
        public static function GetDataFromClient(name:String):PreviewProvider {
            if (!providers.hasOwnProperty(name)) providers[name] = new PreviewProvider();
            return providers[name];
        }
    }
}
