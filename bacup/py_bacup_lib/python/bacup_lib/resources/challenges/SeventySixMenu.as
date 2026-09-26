package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import Shared.AS3.Data.BSUIDataManager;
    import Shared.AS3.Data.BSUIEventDispatcherBackend;

    public dynamic class SeventySixMenu extends MovieClip {
        public var BGSCodeObj:Object;
        public var NativeInput:Boolean = false;
        public var ChallengeMenuVersion:uint = 1;
        public var Challenges_mc:SeventySixMenuChallenges;
        public var ShowCompleted:Boolean = false;
        public var Snapshot:Object;
        private var footer:TextField;
        private var initialized:Boolean = false;
        private var gamepadActive:Boolean = false;

        public function SeventySixMenu() {
            super();
            var backend:BSUIEventDispatcherBackend = new BSUIEventDispatcherBackend();
            backend.DispatchEventToGame = IgnoreOnlineEvent;
            BSUIDataManager.InitDataManager(backend);
            graphics.beginFill(0x111C24, 0.97);
            graphics.drawRect(0, 0, 1920, 1080);
            graphics.endFill();
            var heading:TextField = Label("CHALLENGES", 84, 48, 44);
            Challenges_mc = new SeventySixMenuChallenges();
            Challenges_mc.x = 90;
            Challenges_mc.y = 168;
            addChild(Challenges_mc);
            footer = Label("", 84, 972, 24);
            var back:TextField = Label("[Tab / B] BACK", 1580, 972, 24);
            back.addEventListener(MouseEvent.CLICK, BackClicked);
            footer.addEventListener(MouseEvent.CLICK, FooterClicked);
            addEventListener(Event.ADDED_TO_STAGE, Added);
        }

        private function Label(value:String, px:Number, py:Number, size:Number):TextField {
            var field:TextField = new TextField();
            field.defaultTextFormat = new TextFormat("$MAIN_Font", size, 0xEEE8C9);
            field.text = value;
            field.selectable = false;
            field.x = px;
            field.y = py;
            field.width = 1500;
            field.height = size + 18;
            addChild(field);
            return field;
        }

        private function Added(event:Event):void {
            stage.addEventListener(KeyboardEvent.KEY_DOWN, KeyDown, true);
        }

        private function IgnoreOnlineEvent(event:Event):void {}

        public function B21SetData(payload:Object, gamepad:Boolean):void {
            Snapshot = payload;
            gamepadActive = gamepad;
            Challenges_mc.B21SetSnapshot(payload, ShowCompleted, gamepad);
            footer.text = gamepad ? "[Y] TRACK / UNTRACK     [X] SHOW COMPLETED     [LB / RB] CATEGORY" :
                "[T] TRACK / UNTRACK     [X] SHOW COMPLETED     [Q / E] CATEGORY";
            if (ShowCompleted) footer.appendText("  (ON)");
            if (!initialized) {
                initialized = true;
                stage.focus = Challenges_mc.ItemList_mc;
            }
        }

        public function Navigate(action:String):void {
            if (action == "Cancel") {
                if (BGSCodeObj != null) BGSCodeObj.ChallengeAction("close", "", false);
            } else if (action == "Completed") {
                ShowCompleted = !ShowCompleted;
                if (Snapshot != null) B21SetData(Snapshot, gamepadActive);
            } else {
                Challenges_mc.B21Navigate(action);
            }
        }

        public function Track(id:String, tracked:Boolean):void {
            if (BGSCodeObj != null) BGSCodeObj.ChallengeAction("track", id, tracked);
        }

        private function KeyDown(event:KeyboardEvent):void {
            if (NativeInput) return;
            var action:String = "";
            if (event.keyCode == 27 || event.keyCode == 9) action = "Cancel";
            else if (event.keyCode == 84) action = "Track";
            else if (event.keyCode == 88) action = "Completed";
            else if (event.keyCode == 81) action = "Previous";
            else if (event.keyCode == 69) action = "Next";
            else if (event.keyCode == 38 || event.keyCode == 87) action = "Up";
            else if (event.keyCode == 40 || event.keyCode == 83) action = "Down";
            else if (event.keyCode == 37 || event.keyCode == 65) action = "Left";
            else if (event.keyCode == 39 || event.keyCode == 68) action = "Right";
            else if (event.keyCode == 13 || event.keyCode == 32) action = "Accept";
            if (action.length > 0) { event.stopImmediatePropagation(); Navigate(action); }
        }

        private function BackClicked(event:MouseEvent):void { Navigate("Cancel"); }
        private function FooterClicked(event:MouseEvent):void {
            Navigate(event.localX < 320 ? "Track" : "Completed");
        }
    }
}
