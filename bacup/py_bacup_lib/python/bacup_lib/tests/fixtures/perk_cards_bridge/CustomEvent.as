package Shared.AS3.Events {
    import flash.events.Event;
    public class CustomEvent extends Event {
        public var params:Object;
        public function CustomEvent(name:String, payload:Object) {
            super(name);
            params = payload;
        }
    }
}
