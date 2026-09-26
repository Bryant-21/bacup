package {
    import flash.display.MovieClip;
    import flash.display.DisplayObjectContainer;
    import flash.display.Loader;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.geom.Matrix;
    import flash.geom.Point;
    import flash.net.URLRequest;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class B21_StatusRewards extends MovieClip {
        public var done:uint = 0;
        public var activeID:uint = 0;
        public var model:Boolean = false;
        public var questLease:Boolean = false;
        public var noticeSeen:uint = 0;
        public var questArt:Boolean = false;
        public var modelX:Number = 0.5;
        public var modelY:Number = 0.4;
        public var modelScale:Number = 0.2;
        private var pending:Array = [];
        private var active:Object;
        private var clip:B21_StatusRewardTimeline;
        private var clock:Number = 0;
        private var frames:int = 0;
        private var leaving:Boolean = false;
        private var lastNative:uint = 0;
        private var lastQuest:String = "";
        private var leasedQuest:String = "";
        private var art:Loader;
        private var generation:uint = 0;

        public function B21_StatusRewards() {
            addEventListener("HUDAnnounce::ShowModel",onModel);
            addEventListener("HUDAnnounce::ClearModel",onClearModel);
        }
        private function onModel(event:Event):void { model = active != null && active.kind == "item"; }
        private function onClearModel(event:Event):void { model = false; }
        private function onArtError(event:IOErrorEvent):void {}
        private function onArtLoaded(event:Event):void {
            if (art == null || clip == null || active == null || active.kind != "quest") return;
            var movie:MovieClip = art.content as MovieClip;
            if (movie == null) return;
            var sizing:Object = B21_StatusRewardSource.data.loader;
            movie.graphics.clear();
            movie.graphics.beginFill(0,0);
            movie.graphics.drawRect(0,0,sizing.questAnimStageWidth,sizing.questAnimStageHeight);
            movie.graphics.endFill();
            var ratio:Number = Number(sizing.maxClipHeight)/movie.height;
            movie.scaleX = movie.scaleY = ratio;
            movie.x = -Number(sizing.questAnimStageWidth)*0.5*ratio;
            movie.y = -Number(sizing.questAnimStageHeight)*0.5*ratio;
            questArt = true;
        }
        private function field(object:DisplayObjectContainer):TextField {
            if (object == null) return null;
            for (var i:int = 0; i < object.numChildren; ++i) {
                var text:TextField = object.getChildAt(i) as TextField;
                if (text != null) return text;
                var child:DisplayObjectContainer = object.getChildAt(i) as DisplayObjectContainer;
                if (child != null) { text = field(child); if (text != null) return text; }
            }
            return null;
        }
        private function text(object:DisplayObjectContainer,value:String):void {
            var target:TextField = field(object);
            if (target == null) throw new Error("Missing source reward text");
            // Keep FO76's two tones, as the quest tracker does: header gold (#F5CB5B) names and titles,
            // cream (#FFFFCB) descriptions and rows, which HUD Color turns darker and lighter.
            var color:uint = target.textColor;
            target.text = value;
            target.textColor = color;
            if (!target.multiline && target.textWidth > target.width-4) {
                var format:TextFormat = target.defaultTextFormat;
                format.size = Math.max(1,Math.floor(Number(format.size)*(target.width-4)/target.textWidth));
                target.setTextFormat(format);
            }
        }
        private function close():void {
            if (art != null) { art.unload(); art = null; }
            if (clip != null && contains(clip)) removeChild(clip);
            clip = null;
            active = null;
            activeID = 0;
            model = false;
            questArt = false;
        }
        private function begin(row:Object):void {
            close();
            active = row;
            activeID = uint(row.id);
            clock = 0;
            frames = 0;
            leaving = false;
            if (row.kind == "item") clip = new B21_StatusRewardItem();
            else if (row.kind == "list") clip = new B21_StatusRewardList();
            else clip = new B21_StatusRewardQuest();
            addChild(clip);
            if (row.kind == "item") {
                text(clip.FanfareInternal_mc.Name_mc,String(row.title));
                text(clip.FanfareDescription_mc,String(row.description));
                clip.NewAnim_mc.visible = Boolean(row.isNew);
                for (var star:int = 1; star <= 4; ++star)
                    clip.FanfareInternal_mc["LegendaryStar0"+star+"_mc"].visible = star <= int(row.stars);
            } else if (row.kind == "list") {
                text(clip.FanfareType_mc,"$ITEMREWARD");
                clip.BonusFanfareType_mc.visible = false;
                var rows:Array = row.rows as Array;
                for (var i:int = 1; i <= 6; ++i) {
                    clip["BonusFanfareName_mc"+i].visible = false;
                    text(clip["FanfareName_mc"+i],rows != null && i <= rows.length ? String(rows[i-1]) : "");
                }
            } else {
                text(clip.FanfareType_mc,String(row.header));
                text(clip.FanfareName_mc,String(row.title));
                text(clip.FanfareDescription_mc,"");
                if (row.swf != null && String(row.swf).length > 0) {
                    art = new Loader();
                    art.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR,onArtError);
                    art.contentLoaderInfo.addEventListener(Event.COMPLETE,onArtLoaded);
                    clip.FanfareQuestCompleted_mc.QuestAnimCatcher_mc.ClipContainer_mc.VaultBoyImageInternal_mc.addChild(art);
                    art.load(new URLRequest(String(row.swf)));
                }
            }
            clip.B21Goto("rollOn",true);
        }
        // quest: FO4's quest-update text as captured from HUDMenu ({visible,alpha,name,type}), origin: the
        // stage point of HUDMenu's top-centre group, viewport: the stage rect the model's screen fractions use.
        public function update(data:Object,quest:Object,origin:Point,scale:Number,viewport:Object):void {
            questLease = false;
            if (data == null) { close(); pending = []; lastNative = 0; lastQuest = ""; leasedQuest = ""; return; }
            if (generation != uint(data.generation)) {
                close(); pending = []; lastNative = 0; lastQuest = ""; leasedQuest = ""; done = noticeSeen = 0;
                generation = uint(data.generation);
            }
            var key:String = "";
            if (Boolean(data.quests) && quest != null && Boolean(quest.visible) && Number(quest.alpha) > 0) {
                key = String(quest.name)+"|"+String(quest.type);
                questLease = key == leasedQuest;
                for each (var notice:Object in data.notices) {
                    if (String(notice.title) != String(quest.name)) continue;
                    questLease = true;
                    leasedQuest = key;
                    noticeSeen = uint(notice.token);
                    if (key != lastQuest) pending.push(notice);
                    break;
                }
            }
            lastQuest = key;
            if (key.length == 0) leasedQuest = "";
            var next:Object = data.next;
            if (next != null && uint(next.id) != 0 && uint(next.id) != lastNative) {
                pending.push(next);
                lastNative = uint(next.id);
            }
            if (active != null && ((active.kind == "item" && !Boolean(data.items)) ||
                (active.kind != "item" && !Boolean(data.quests)))) close();
            for (var p:int = pending.length-1; p >= 0; --p) {
                if ((pending[p].kind == "item" && !Boolean(data.items)) ||
                    (pending[p].kind != "item" && !Boolean(data.quests))) pending.splice(p,1);
            }
            if (active == null && pending.length > 0 && Boolean(data.advance)) begin(pending.shift());
            if (active == null) { visible = false; return; }
            visible = Boolean(data.advance);
            var source:Array = B21_StatusRewardSource.data.layout[String(active.kind)];
            var at:Point = globalToLocal(origin);
            clip.transform.matrix = new Matrix(source[0]*scale,source[1]*scale,source[2]*scale,source[3]*scale,
                at.x+(source[4]-960)*scale,at.y+(source[5]-54)*scale);
            if (Boolean(data.advance) && !(model && !Boolean(data.modelReady))) {
                clock += Math.max(0,Number(data.delta))*B21_StatusRewardSource.data.fps;
                while (frames < int(clock)) {
                    if (model && !Boolean(data.modelReady)) { clock = frames; break; }
                    clip.B21Tick(); ++frames;
                }
            }
            if (!leaving && clock >= (active.kind == "list" ? 234 : active.kind == "item" ? 119 : 120)) {
                leaving = true;
                clip.B21Goto("rollOff",true);
            }
            if (leaving && clip.currentFrame == (active.kind == "quest" ? 60 : clip.totalFrames)) {
                if (uint(active.id) != 0) done = uint(active.id);
                close();
                return;
            }
            if (active.kind == "item") {
                var catcher:Object = clip.FanfareItem_mc.FanfareItemCatcher_mc;
                if (catcher == null) throw new Error("Missing reward model catcher at "+clip.currentFrame);
                var position:Point = catcher.localToGlobal(new Point(0,0));
                if (viewport == null) viewport = {x:0,y:0,width:stage.stageWidth,height:stage.stageHeight};
                modelX = (position.x-Number(viewport.x))/Number(viewport.width);
                modelY = (position.y-Number(viewport.y))/Number(viewport.height);
            }
        }
        public function diagnostic():Object {
            var stars:Array = [];
            if (active != null && active.kind == "item") for (var i:int = 1; i <= 4; ++i) {
                var star:Object = clip.FanfareInternal_mc["LegendaryStar0"+i+"_mc"];
                var bounds:Object = star.getBounds(this);
                stars.push({frame:star.currentFrame,visible:star.visible,alpha:star.alpha,children:star.numChildren,
                    x:bounds.x,y:bounds.y,width:bounds.width,height:bounds.height});
            }
            var catcher:Object = active != null && active.kind == "item" ? clip.FanfareItem_mc.FanfareItemCatcher_mc : null;
            var catcherBounds:Object = catcher == null ? null : catcher.getBounds(stage);
            return {kind:active == null ? "" : active.kind,frame:clip == null ? 0 : clip.currentFrame,stars:stars,
                catcher:catcherBounds == null ? null : {x:catcherBounds.x,y:catcherBounds.y,width:catcherBounds.width,height:catcherBounds.height},
                nameColor:active != null && active.kind == "item" ? field(clip.FanfareInternal_mc.Name_mc).textColor : 0,
                descriptionColor:active != null && active.kind == "item" ? field(clip.FanfareDescription_mc).textColor : 0,
                done:done,model:model,questLease:questLease,questArt:questArt,pending:pending.length,
                title:active == null ? "" : active.title};
        }
    }
}
