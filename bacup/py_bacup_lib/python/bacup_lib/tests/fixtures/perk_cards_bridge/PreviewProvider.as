package Shared.AS3.Data {
    public class PreviewProvider {
        public var data:Object = new Object();
        public var changes:uint = 0;
        public var ready:Boolean = false;
        public var listener:Function;
        public function SetReady(notify:Boolean):void {
            if (!ready) {
                ready = true;
                if (notify) DispatchChange();
            }
        }
        public function DispatchChange():void {
            ++changes;
            if (listener != null) listener(data);
        }
    }
}
