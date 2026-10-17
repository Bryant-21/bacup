package {
    import flash.display.DisplayObjectContainer;
    import flash.display.Loader;
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.events.SecurityErrorEvent;
    import flash.events.TimerEvent;
    import flash.geom.Matrix;
    import flash.net.URLRequest;
    import flash.utils.Timer;

    public class LoadingMenu extends MovieClip {
        public var LeftText_mc:MovieClip;
        public var VaultTecLogo_mc:MovieClip;
        public var Vault51Logo_mc:MovieClip;
        public var BlackRect_mc:MovieClip;
        public var Box_mc:MovieClip;
        public var Info_mc:MovieClip;
        public var BGSCodeObj:Object;
        public var InMinimalMode:Boolean = false;
        private var b21Photo:Loader;
        private var b21PendingUrl:String;
        private var b21Url:String;
        private var b21Queue:Array;
        private var b21Timer:Timer;
        public var B21LoadingReport:Function;
        public function get B21PhotoHostVersion():uint { return 3; }

        public function LoadingMenu() {
            super();
            BGSCodeObj = {};
            Info_mc.visible = Vault51Logo_mc.visible = BlackRect_mc.visible = false;
            VaultTecLogo_mc.visible = true;
            LeftText_mc.VDSGText_tf.visible = false;
            LeftText_mc.LoadScreenText_tf.text = "";
            b21Timer = new Timer(3000);
            b21Timer.addEventListener(TimerEvent.TIMER, refreshLoadingText);
            addEventListener(Event.ADDED_TO_STAGE, B21Stage);
            addEventListener(Event.REMOVED_FROM_STAGE, B21Removed);
        }

        private function B21Stage(event:Event):void {
            if (event.target != this) return;
            b21Timer.start();
            stage.addEventListener(Event.RESIZE, B21PhotoResized);
            B21Layout();
        }

        private function B21Removed(event:Event):void {
            if (event.target != this) return;
            b21Timer.stop();
            B21ClearPhoto();
            if (stage) stage.removeEventListener(Event.RESIZE, B21PhotoResized);
        }

        public function SetPlatform(platform:uint, swap:Boolean):void {}
        public function SetSafeRect(x:Number, y:Number):void { B21Layout(); }

        public function SetLoadingText(slide:uint, title:String, tip:String):void {
            InMinimalMode = false;
            BlackRect_mc.visible = false;
            VaultTecLogo_mc.visible = true;
            Box_mc.visible = LeftText_mc.visible = true;
            LeftText_mc.LoadScreenText_tf.text = tip;
            LeftText_mc.LoadScreenText_tf.visible = true;
            LeftText_mc.Dots_mc.play();
            if (b21Photo) b21Photo.visible = true;
        }

        public function SetLevel(level:uint, fraction:Number):void {
            LeftText_mc.LevelText_tf.text = "$LEVEL";
            LeftText_mc.LevelText_tf.appendText(" " + level);
            LeftText_mc.PlayerLevelMeter_mc.gotoAndStop(Math.max(1, Math.round(Math.max(0, Math.min(1, fraction)) * 300)));
            LeftText_mc.LevelText_tf.visible = LeftText_mc.PlayerLevelMeter_mc.visible = level > 0;
        }

        public function SetMinimalMode():void {
            InMinimalMode = true;
            Box_mc.visible = LeftText_mc.visible = false;
            BlackRect_mc.visible = VaultTecLogo_mc.visible = true;
            if (b21Photo) b21Photo.visible = false;
        }

        public function SpinnerOnly():void {
            SetMinimalMode();
            BlackRect_mc.visible = false;
        }

        public function refreshLoadingText(event:Event = null):void {
            if (BGSCodeObj && BGSCodeObj.requestLoadingText != null) BGSCodeObj.requestLoadingText();
        }

        public function B21ShowPhotos(urls:Array):void {
            B21ShowPhoto(urls && urls.length ? String(urls[0]) : null);
            b21Queue = urls ? urls.slice(1) : null;
        }

        private function B21Report(shown:Boolean, detail:String):void {
            if (B21LoadingReport == null) return;
            try { B21LoadingReport(shown, b21Url, detail); } catch (error:Error) {}
        }

        private function B21TryNext(detail:String):void {
            B21Report(false, detail);
            var rest:Array = b21Queue;
            B21ClearPhoto();
            if (rest && rest.length) B21ShowPhotos(rest);
        }

        public function B21ShowPhoto(url:String):void {
            B21ClearPhoto();
            if (!url) return;
            if (!stage) {
                b21PendingUrl = url;
                addEventListener(Event.ADDED_TO_STAGE, B21PhotoStage);
                return;
            }
            b21Url = url;
            b21Photo = new Loader();
            b21Photo.visible = false;
            b21Photo.mouseEnabled = false;
            b21Photo.contentLoaderInfo.addEventListener(Event.COMPLETE, B21PhotoLoaded);
            b21Photo.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, B21PhotoFailed);
            b21Photo.contentLoaderInfo.addEventListener(SecurityErrorEvent.SECURITY_ERROR, B21PhotoFailed);
            // Under FilterHolder_mc, in stage coordinates: FO76 places this menu 10px down inside it,
            // and FO4 tints that holder with the HUD colour. FO76 draws its backgrounds untinted.
            DisplayObjectContainer(root).addChildAt(b21Photo, 0);
            try { b21Photo.load(new URLRequest(url)); }
            catch (error:Error) { B21TryNext(error.message); }
        }

        private function B21PhotoStage(event:Event):void {
            var url:String = b21PendingUrl;
            var rest:Array = b21Queue;
            B21ShowPhoto(url);
            b21Queue = rest;
        }

        private function B21PhotoLoaded(event:Event):void {
            if (!b21Photo || event.target != b21Photo.contentLoaderInfo) return;
            try {
                if (!B21FitPhoto()) return;
                b21Photo.visible = !InMinimalMode;
                var size:Array = B21PhotoSize();
                B21Report(true, size[0] + "x" + size[1]);
            } catch (error:Error) { B21TryNext(error.message); }
        }

        private function B21PhotoResized(event:Event):void { B21Layout(); B21FitPhoto(); }

        private function B21Layout():void {
            if (!stage) return;
            B21Place(Box_mc, B21_MenuLayout.loading.Box_mc);
            B21Place(LeftText_mc, B21_MenuLayout.loading.LeftText_mc);
            B21Place(VaultTecLogo_mc, B21_MenuLayout.loading.VaultTecLogo_mc);
            BlackRect_mc.width = stage.stageWidth;
            BlackRect_mc.height = stage.stageHeight;
        }

        private function B21Place(clip:MovieClip, source:Array):void {
            var scale:Number = Math.min(stage.stageWidth / 1920, stage.stageHeight / 1080);
            clip.transform.matrix = new Matrix(source[0] * scale, source[1] * scale,
                source[2] * scale, source[3] * scale, source[4] * scale,
                stage.stageHeight - 1080 * scale + source[5] * scale);
        }

        // Scaleform leaves LoaderInfo's size at 0 for images, so the size comes from the content.
        private function B21PhotoSize():Array {
            var content:Object = b21Photo ? b21Photo.content : null;
            if (!content || !content.scaleX || !content.scaleY) return [0, 0];
            return [content.width / content.scaleX, content.height / content.scaleY];
        }

        private function B21FitPhoto():Boolean {
            if (!b21Photo || !stage || !b21Photo.content) return false;
            var size:Array = B21PhotoSize();
            var width:Number = size[0];
            var height:Number = size[1];
            if (width <= 0 || height <= 0) { B21TryNext("image has no size"); return false; }
            var scale:Number = Math.max(stage.stageWidth / width, stage.stageHeight / height);
            b21Photo.content.scaleX = b21Photo.content.scaleY = scale;
            b21Photo.x = (stage.stageWidth - width * scale) / 2;
            b21Photo.y = (stage.stageHeight - height * scale) / 2;
            return true;
        }

        private function B21PhotoFailed(event:Event):void {
            if (b21Photo && event.target == b21Photo.contentLoaderInfo)
                B21TryNext(event is IOErrorEvent ? IOErrorEvent(event).text : event.type);
        }

        public function B21ClearPhoto():void {
            removeEventListener(Event.ADDED_TO_STAGE, B21PhotoStage);
            b21PendingUrl = null;
            b21Queue = null;
            if (!b21Photo) return;
            b21Photo.contentLoaderInfo.removeEventListener(Event.COMPLETE, B21PhotoLoaded);
            b21Photo.contentLoaderInfo.removeEventListener(IOErrorEvent.IO_ERROR, B21PhotoFailed);
            b21Photo.contentLoaderInfo.removeEventListener(SecurityErrorEvent.SECURITY_ERROR, B21PhotoFailed);
            try { b21Photo.close(); } catch (error:Error) {}
            if (b21Photo.parent) b21Photo.parent.removeChild(b21Photo);
            b21Photo.unloadAndStop(true);
            b21Photo = null;
        }
    }
}
