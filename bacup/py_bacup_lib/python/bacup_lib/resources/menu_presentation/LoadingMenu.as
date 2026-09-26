package {
    import flash.display.Loader;
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.events.SecurityErrorEvent;
    import flash.geom.Matrix;
    import flash.net.URLRequest;

    public class LoadingMenu extends MovieClip {
        private var b21Photo:Loader;
        private var b21Frame:MovieClip;
        private var b21Text:MovieClip;
        private var b21PendingUrl:String;
        private var b21TextAlpha:Number;
        private var b21SpinnerAlpha:Number;
        private var b21TextHidden:Boolean;
        private var b21Level:Number = 0;
        private var b21Url:String;
        // Images still to try if the current one fails, and Tales' logger for each attempt.
        private var b21Queue:Array;
        public var B21LoadingReport:Function;
        public function get B21PhotoHostVersion():uint { return 2; }

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
            addChildAt(b21Photo, BlackRect_mc && contains(BlackRect_mc) ? getChildIndex(BlackRect_mc) + 1 : 0);
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
                b21Frame = new B21_LoadingBox();
                b21Text = new B21_LoadingText();
                b21Frame.mouseEnabled = b21Text.mouseEnabled = false;
                b21Frame.mouseChildren = b21Text.mouseChildren = false;
                addChildAt(b21Frame, getChildIndex(b21Photo) + 1);
                addChildAt(b21Text, getChildIndex(b21Frame) + 1);
                b21Text.Dots_mc.play();
                // A failed fit has already moved on to the next image.
                if (!B21FitPhoto()) return;
                b21TextAlpha = LeftText_mc.alpha;
                b21SpinnerAlpha = VaultTecLogo_mc.alpha;
                b21TextHidden = true;
                LeftText_mc.alpha = 0;
                addEventListener(Event.ENTER_FRAME, B21SyncText);
                stage.addEventListener(Event.RESIZE, B21PhotoResized);
                B21SyncText(null);
                var size:Array = B21PhotoSize();
                B21Report(true, size[0] + "x" + size[1]);
            } catch (error:Error) { B21TryNext(error.message); }
        }

        public function B21LevelChanged(fraction:Number):void {
            b21Level = Math.max(0, Math.min(1, fraction));
        }

        private function B21SyncText(event:Event):void {
            if (!b21Text || !b21Photo) return;
            try {
                b21Photo.visible = !InMinimalMode;
                VaultTecLogo_mc.alpha = InMinimalMode ? b21SpinnerAlpha : 0;
                b21Frame.visible = !InMinimalMode;
                b21Text.visible = !InMinimalMode;
                b21Text.LoadScreenText_tf.text = LeftText_mc.LoadScreenText_tf.text;
                b21Text.LevelText_tf.text = LeftText_mc.LevelText_tf.text;
                b21Text.LoadScreenText_tf.visible = LeftText_mc.LoadScreenText_tf.visible;
                b21Text.LevelText_tf.visible = LeftText_mc.LevelText_tf.visible;
                b21Text.PlayerLevelMeter_mc.visible = LeftText_mc.LevelMeter_mc.visible;
                b21Text.PlayerLevelMeter_mc.gotoAndStop(Math.max(1, Math.round(b21Level * 300)));
            } catch (error:Error) { B21ClearPhoto(); }
        }

        private function B21PhotoResized(event:Event):void { B21FitPhoto(); }

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
            B21Place(b21Frame, B21_MenuLayout.loading.Box_mc);
            B21Place(b21Text, B21_MenuLayout.loading.LeftText_mc);
            return true;
        }

        private function B21PhotoFailed(event:Event):void {
            if (b21Photo && event.target == b21Photo.contentLoaderInfo)
                B21TryNext(event is IOErrorEvent ? IOErrorEvent(event).text : event.type);
        }

        public function B21ClearPhoto():void {
            removeEventListener(Event.ADDED_TO_STAGE, B21PhotoStage);
            removeEventListener(Event.ENTER_FRAME, B21SyncText);
            if (stage) stage.removeEventListener(Event.RESIZE, B21PhotoResized);
            b21PendingUrl = null;
            b21Queue = null;
            if (b21TextHidden && LeftText_mc) LeftText_mc.alpha = b21TextAlpha;
            if (b21TextHidden && VaultTecLogo_mc) VaultTecLogo_mc.alpha = b21SpinnerAlpha;
            b21TextHidden = false;
            if (b21Frame && contains(b21Frame)) removeChild(b21Frame);
            if (b21Text && contains(b21Text)) removeChild(b21Text);
            b21Frame = b21Text = null;
            if (!b21Photo) return;
            b21Photo.contentLoaderInfo.removeEventListener(Event.COMPLETE, B21PhotoLoaded);
            b21Photo.contentLoaderInfo.removeEventListener(IOErrorEvent.IO_ERROR, B21PhotoFailed);
            b21Photo.contentLoaderInfo.removeEventListener(SecurityErrorEvent.SECURITY_ERROR, B21PhotoFailed);
            try { b21Photo.close(); } catch (error:Error) {}
            if (contains(b21Photo)) removeChild(b21Photo);
            b21Photo.unloadAndStop(true);
            b21Photo = null;
        }
    }
}
