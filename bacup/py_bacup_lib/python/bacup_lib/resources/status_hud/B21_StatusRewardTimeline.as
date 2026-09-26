package {
    import flash.display.MovieClip;
    import flash.events.Event;

    public dynamic class B21_StatusRewardTimeline extends MovieClip {
        private var source:Object;
        private var running:Boolean = true;
        public function B21_StatusRewardTimeline(id:String) {
            source = B21_StatusRewardSource.data.timelines[id];
            stop();
            actions();
        }
        private function actions():void {
            var list:Array = source.actions[String(currentFrame)] as Array;
            if (list == null) return;
            for each (var command:Array in list) {
                if (command[0] == "event") {
                    dispatchEvent(new Event(String(command[1]),true));
                    continue;
                }
                var target:Object = this;
                if (command[1] != "") for each (var key:String in String(command[1]).split(".")) {
                    target = target[key];
                    if (target == null) throw new Error("Missing reward timeline target "+command[1]+" at "+currentFrame);
                }
                if (command[0] == "stop") target.B21Stop();
                else target.B21Goto(command[2],Boolean(command[3]));
            }
        }
        public function B21Stop():void { running = false; }
        public function B21Goto(frame:Object,play:Boolean):void {
            var value:int = frame is String ? int(source.labels[String(frame)]) : int(frame);
            gotoAndStop(value);
            running = play;
            actions();
        }
        public function B21Tick():void {
            if (running && totalFrames > 1) {
                gotoAndStop(currentFrame == totalFrames ? 1 : currentFrame+1);
                actions();
            }
            for (var i:int = 0; i < numChildren; ++i) {
                var child:Object = getChildAt(i);
                if ("B21Tick" in child) child.B21Tick();
            }
        }
    }
}
