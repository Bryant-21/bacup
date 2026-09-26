package
{
    import Shared.IMenu;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.display.Loader;
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.geom.Rectangle;
    import flash.net.URLRequest;
    import flash.system.ApplicationDomain;
    import flash.system.LoaderContext;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class Workshop extends IMenu
    {
        public var B21PrototypeVersion:int;
        public var B21ArtReady:Boolean;
        public var B21ArtError:String;
        public var B21Panel:MovieClip;
        private var B21Loader:Loader;
        private var B21Path:TextField;
        private var B21Name:TextField;
        private var B21Materials:TextField;
        private var B21LastText:String;

        public function B21Initialize():void
        {
            B21PrototypeVersion = 1;
            B21ArtReady = false;
            B21ArtError = "";
            B21Panel = new MovieClip();
            B21Panel.mouseEnabled = false;
            B21Panel.mouseChildren = false;
            addChildAt(B21Panel, 0);
            addEventListener(Event.ADDED_TO_STAGE, B21LoadArt);
        }

        private function B21LoadArt(event:Event):void
        {
            removeEventListener(Event.ADDED_TO_STAGE, B21LoadArt);
            B21Loader = new Loader();
            B21Loader.contentLoaderInfo.addEventListener(Event.COMPLETE, B21Loaded);
            B21Loader.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, B21Failed);
            var url:String = root.loaderInfo.url;
            var slash:int = Math.max(url.lastIndexOf("/"), url.lastIndexOf("\\"));
            B21Loader.load(new URLRequest(url.substring(0, slash + 1) +
                "B21/TalesFromAppalachia/WorkshopPrototype/workshopnewlibrary.swf"),
                new LoaderContext(false, new ApplicationDomain(ApplicationDomain.currentDomain)));
        }

        private function B21Failed(event:IOErrorEvent):void
        {
            B21ArtError = event.text;
        }

        private function B21Freeze(clip:DisplayObject):void
        {
            if (clip is MovieClip) MovieClip(clip).stop();
            if (clip is TextField) TextField(clip).text = "";
            if (clip is DisplayObjectContainer)
            {
                var container:DisplayObjectContainer = DisplayObjectContainer(clip);
                for (var i:int = 0; i < container.numChildren; i++) B21Freeze(container.getChildAt(i));
            }
        }

        private function B21Art(name:String, left:Number, top:Number, width:Number, height:Number):MovieClip
        {
            var type:Class = B21Loader.contentLoaderInfo.applicationDomain.getDefinition(name) as Class;
            var clip:MovieClip = new type();
            B21Panel.addChild(clip);
            B21Freeze(clip);
            var bounds:Rectangle = clip.getBounds(clip);
            clip.scaleX = width / bounds.width;
            clip.scaleY = height / bounds.height;
            clip.x = left - bounds.x * clip.scaleX;
            clip.y = top - bounds.y * clip.scaleY;
            return clip;
        }

        private function B21Text(left:Number, top:Number, width:Number, height:Number, size:int):TextField
        {
            var field:TextField = new TextField();
            field.defaultTextFormat = new TextFormat("$MAIN_Font", size, 0xFFFFCB);
            field.x = left;
            field.y = top;
            field.width = width;
            field.height = height;
            field.multiline = true;
            field.wordWrap = true;
            field.selectable = false;
            B21Panel.addChild(field);
            return field;
        }

        private function B21Loaded(event:Event):void
        {
            B21Art("/* ART_0 */", 24, 112, 280, 324);
            B21Art("/* ART_1 */", 24, 112, 280, 44);
            B21Art("/* ART_2 */", 24, 466, 1232, 190);
            var heading:TextField = B21Text(36, 119, 250, 34, 25);
            heading.text = "$BUILD";
            B21Path = B21Text(38, 171, 250, 90, 18);
            B21Name = B21Text(38, 264, 250, 64, 23);
            B21Materials = B21Text(38, 338, 250, 90, 17);
            B21ArtReady = true;
            addEventListener(Event.ENTER_FRAME, B21Refresh);
            B21Refresh(null);
        }

        public function B21Refresh(event:Event):void
        {
            if (!B21ArtReady) return;
            var path:String = DisplayPathBase_mc.DisplayPath_tf.text;
            var name:String = ItemNameBase_mc.ItemName_tf.text;
            var lines:Array = [];
            for each (var row:Object in requirements)
            {
                var label:String = row.hasOwnProperty("text") ? String(row.text) : "";
                var count:String = row.hasOwnProperty("counts") ? String(row.counts) : "";
                if (label.length > 0) lines.push(label + (count.length > 0 ? "  " + count : ""));
            }
            var materials:String = lines.join("\n");
            var key:String = path + "\t" + name + "\t" + materials;
            if (key == B21LastText) return;
            B21LastText = key;
            B21Path.text = path.split(" >  ").join("\n");
            B21Name.text = name;
            B21Materials.text = materials;
        }
    }
}
