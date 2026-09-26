package {
    import flash.display.MovieClip;
    public class B21_StatusReticle extends MovieClip {
        public var art:MovieClip = new B21_StatusCrosshair();
        private var state:String = "None";
        private var requested:String = "None";
        private var finish:String = "None";
        private var animation:MovieClip;
        private var elapsed:Number = 0;
        private var radius:Number = 19;
        private var fromRadius:Number = 19;
        private var targetRadius:Number = 19;
        private var tweenTime:Number = .15;
        private var collapsing:Boolean = false;
        private var collapseDelay:Number = -1;
        public function B21_StatusReticle() { addChild(art); }
        private function begin(next:String):void {
            var clips:MovieClip = art.CrosshairBase_mc.CrosshairClips_mc;
            for (var i:int = 0; i < clips.numChildren; ++i) clips.getChildAt(i).visible = false;
            animation = clips[state+"_"+next] as MovieClip;
            finish = next;
            elapsed = 0;
            if (animation != null) { animation.visible = true; animation.gotoAndStop(Math.min(2,animation.totalFrames)); }
        }
        private function tween(value:Number):void {
            if (value == targetRadius) return;
            fromRadius = radius;
            targetRadius = value;
            tweenTime = 0;
        }
        public function reset():void {
            state = requested = finish = "None";
            animation = null;
            radius = fromRadius = targetRadius = 19;
            tweenTime = .15;
            collapsing = false;
            collapseDelay = -1;
        }
        public function update(next:String, spread:Number, delta:Number):Boolean {
            if (["None","Dot","Standard","Activate","Command"].indexOf(next) < 0 || !isFinite(spread) || spread < 0) return false;
            delta = isFinite(delta) ? Math.max(0,delta) : 0;
            if (next != requested) {
                requested = next;
                collapseDelay = -1;
                if (state == "Standard" && animation == null) {
                    collapsing = next != "Standard";
                    if (collapsing) {
                        // FO76's Timer receives 0.3 milliseconds, despite its constant's SEC suffix.
                        collapseDelay = next == "Activate" ? .0003 : 0;
                    }
                }
            }
            if (animation != null) {
                elapsed += delta;
                animation.gotoAndStop(Math.min(animation.totalFrames,2+Math.floor(elapsed*B21_StatusSource.layout.holdFPS)));
                if (animation.currentFrame == animation.totalFrames) {
                    state = finish;
                    animation = null;
                }
            }
            if (animation == null) {
                if (state == "Standard") {
                    if (requested == "Standard") {
                        collapsing = false;
                        collapseDelay = -1;
                        tween(Math.max(19,spread));
                    } else {
                        if (!collapsing) { collapsing = true; collapseDelay = requested == "Activate" ? .0003 : 0; }
                        collapseDelay -= delta;
                        if (collapseDelay <= 0) tween(19);
                    }
                    tweenTime = Math.min(.15,tweenTime+delta);
                    var fraction:Number = tweenTime/.15;
                    radius = fromRadius+(targetRadius-fromRadius)*fraction*fraction;
                    if (collapsing && collapseDelay <= 0 && tweenTime >= .15) begin(requested);
                } else if (state != requested || finish != requested) begin(requested);
                else if (art.CrosshairBase_mc.CrosshairClips_mc[state+"_"+state] != null) {
                    var clips:MovieClip = art.CrosshairBase_mc.CrosshairClips_mc;
                    for (var i:int = 0; i < clips.numChildren; ++i) clips.getChildAt(i).visible = false;
                    MovieClip(clips[state+"_"+state]).visible = true;
                    MovieClip(clips[state+"_"+state]).gotoAndStop(2);
                }
            }
            var ticks:MovieClip = art.CrosshairBase_mc.CrosshairTicks_mc;
            ticks.visible = state == "Standard" && animation == null;
            art.CrosshairBase_mc.CrosshairClips_mc.visible = !ticks.visible;
            ticks.Up.y = -radius; ticks.Down.y = radius;
            ticks.Left.x = -radius; ticks.Right.x = radius;
            return true;
        }
        public function diagnostics():Object {
            return {state:state,requested:requested,radius:radius,target:targetRadius,
                    animation:animation == null ? "" : animation.name,frame:animation == null ? 0 : animation.currentFrame};
        }
    }
}
