package
{
    import flash.display.MovieClip;
    import flash.filters.DropShadowFilter;
    import flash.geom.Rectangle;
    import flash.system.ApplicationDomain;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public dynamic class RadialMenuEntryInner extends MovieClip
    {
        public var Icon_mc:MovieClip;
        public var Hotkey_mc:MovieClip;
        public var EquippedState_mc:MovieClip;
        public var EquippedStateTop_mc:MovieClip;
        public var Backer_mc:MovieClip;
        public var Fill_mc:MovieClip;
        public var HitArea_mc:MovieClip;
        public var Expanded_mc:MovieClip;
        public var BackgroundHighlight_mc:MovieClip;
        public var slot:int = -1;
        private var iconClip:MovieClip;
        private var label:TextField;
        private var iconName:String = "";
        private var selectedValue:Boolean = false;
        private var stencil:Array;

        public function RadialMenuEntryInner()
        {
            super();
            addFrameScript(1, stop, 2, stop, 14, stop, 20, stop);
            gotoAndStop(3);
            mouseChildren = false;
        }

        public function Configure(index:int):void
        {
            slot = index;
            hitArea = HitArea_mc;
            HitArea_mc.mouseEnabled = false;
            HitArea_mc.visible = false;
            Backer_mc.alpha = 0.55;
            Icon_mc.rotation = -rotation;
            Hotkey_mc.visible = false;
            EquippedStateTop_mc.rotation = -rotation;
            label = new TextField();
            label.defaultTextFormat = new TextFormat("$MAIN_Font", 19, 0xFFFFFF, true, null, null, null, null, "center");
            label.width = 48;
            label.height = 28;
            label.x = Hotkey_mc.x;
            label.y = Hotkey_mc.y;
            label.rotation = -rotation;
            label.mouseEnabled = false;
            label.text = index < 9 ? String(index + 1) : index == 9 ? "0" : index == 10 ? "-" : "=";
            addChild(label);
        }

        public static function DrawStencil(runs:Array):MovieClip
        {
            var clip:MovieClip = new MovieClip();
            clip.name = "WeaponStencil";
            var groups:Array = [];
            var i:int;
            for (i = 0; i + 3 < runs.length; i += 4)
            {
                var alpha:int = int(runs[i + 3]);
                if (alpha == 2) alpha = 255;
                else if (alpha == 1) alpha = 51;
                if (groups[alpha] == null) groups[alpha] = [];
                groups[alpha].push(i);
            }
            // A shared fill joins adjacent rows without antialiasing each row boundary.
            for (alpha = 1; alpha <= 255; alpha++)
            {
                var group:Array = groups[alpha] as Array;
                if (group == null) continue;
                clip.graphics.beginFill(0xFFFFFF, alpha / 255);
                for (var j:int = 0; j < group.length; j++)
                {
                    i = int(group[j]);
                    clip.graphics.drawRect(Number(runs[i]), Number(runs[i + 1]), Number(runs[i + 2]), 1);
                }
                clip.graphics.endFill();
            }
            return clip;
        }

        public function SetItem(data:Object, icon:String, runs:Array = null):void
        {
            var occupied:Boolean = data != null && data.count > 0;
            Backer_mc.gotoAndStop(occupied ? 1 : 2);
            if (!occupied) icon = "radialIconEmpty";
            if (!occupied) runs = null;
            if (icon != iconName || runs !== stencil)
            {
                if (iconClip != null && iconClip.parent != null) iconClip.parent.removeChild(iconClip);
                iconClip = null;
                iconName = icon;
                stencil = runs;
                if (occupied)
                {
                    if (runs != null && runs.length > 0) iconClip = DrawStencil(runs);
                    else
                    {
                        var domain:ApplicationDomain = ApplicationDomain.currentDomain;
                        if (!domain.hasDefinition(icon)) icon = "UnknownIcon";
                        var type:Class = domain.getDefinition(icon) as Class;
                        iconClip = new type() as MovieClip;
                    }
                    var bounds:Rectangle = iconClip.getBounds(iconClip);
                    var scale:Number = Math.min(84 / Math.max(1, bounds.width), 84 / Math.max(1, bounds.height));
                    iconClip.scaleX = scale;
                    iconClip.scaleY = scale;
                    iconClip.x = -(bounds.x + bounds.width / 2) * scale;
                    iconClip.y = -(bounds.y + bounds.height / 2) * scale;
                    iconClip.filters = [new DropShadowFilter(1,45,0,0.8,2,2,1,1,false,false,false)];
                    Icon_mc.Body.addChild(iconClip);
                }
            }
            if (iconClip != null) iconClip.alpha = occupied ? 1 : 0.35;
            EquippedState_mc.visible = data != null && data.equipped === true;
            EquippedStateTop_mc.visible = EquippedState_mc.visible;
        }

        public function SetSelected(value:Boolean):void
        {
            if (selectedValue == value) return;
            selectedValue = value;
            gotoAndStop(value ? 15 : 3);
        }

        public function SetGamepad(value:Boolean):void
        {
            if (label != null) label.visible = !value;
        }
    }
}
