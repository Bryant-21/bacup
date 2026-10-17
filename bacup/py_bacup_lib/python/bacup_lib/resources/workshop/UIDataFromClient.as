package Shared.AS3.Data {
    public class UIDataFromClient extends flash.events.EventDispatcher {
        public function B21SetData(payload:Object):void {
            m_Payload = payload;
            if (dataReady) DispatchChange();
            else SetReady(true);
        }
    }
}
