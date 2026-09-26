package {
   import flash.events.Event;
   import Shared.AS3.IMenu;
   import Shared.AS3.Data.BSUIDataManager;
   import Shared.AS3.Data.FromClientDataEvent;

   public class FishingMenu extends IMenu {
      public var BGSCodeObj:Object;
      private var B21Connected:Boolean = false;

      public function B21Initialize():void {
         this.FishingData = {selectedBaitIndex: 0, sliceSize: 0, gameState: 2, fishInSlice: false,
            fishAngle: 0, fishProgress: 0, sliceAngle: 0, joystickDotAngle: 0, joystickDotProgress: 0};
      }

      public function B21HasSelectedBait():Boolean {
         return this.FishingData != null && uint(this.FishingData.selectedBaitIndex) < this.ItemDataA.length;
      }

      public function B21SetData(payload:Object, bait:Array, platform:uint, elapsed:Number) : Boolean
      {
         if(!this.B21Connected)
         {
            this.B21Connected = true;
            var backend:Shared.AS3.Data.BSUIEventDispatcherBackend = new Shared.AS3.Data.BSUIEventDispatcherBackend();
            backend.DispatchEventToGame = this.B21Event;
            BSUIDataManager.InitDataManager(backend);
         }
         this.deltaTime = elapsed;
         this.SetPlatform(platform, false, platform, 0);
         this.ItemDataA.length = 0;
         for each(var entry:Object in bait)
         {
            var item:FishingItemData = new FishingItemData();
            item.text = entry.text;
            item.count = entry.count;
            item.isRegionSpecific = entry.isRegionSpecific;
            item.formID = entry.formID;
            this.ItemDataA.push(item);
         }
         var provider:Shared.AS3.Data.UIDataFromClient = BSUIDataManager.GetDataFromClient(EVENT_FISHING_DATA);
         for(var key:String in payload)
         {
            provider.data[key] = payload[key];
         }
         provider.SetReady(false);
         this.onFishingData(new FromClientDataEvent(provider));
         this.updateBaitList();
         if(this.m_GameState == GAME_STATE_BOBBERING && payload.hasOwnProperty("canHook"))
         {
            this.SelectBtn.ButtonText = payload.canHook ? "$B21_FISHING_HOOK_NOW" : "$B21_FISHING_WAIT_FOR_BITE";
         }
         return this.m_GameState == payload.gameState && this.MinigameDial_mc.visible == (payload.gameState == GAME_STATE_MINIGAME);
      }

      public function B21Event(event:Event) : void
      {
         if(this.BGSCodeObj != null)
         {
            var payload:Object = event is Shared.AS3.Events.CustomEvent ? Shared.AS3.Events.CustomEvent(event).params : new Object();
            this.BGSCodeObj.FishingEvent(event.type, payload);
         }
      }
   }
}
