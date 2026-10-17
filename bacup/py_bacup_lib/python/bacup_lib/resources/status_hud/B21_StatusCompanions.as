package {
    import flash.display.MovieClip;
    import flash.display.Bitmap;
    import flash.display.Graphics;
    import flash.display.Loader;
    import flash.display.Shape;
    import flash.events.Event;
    import flash.events.IOErrorEvent;
    import flash.net.URLRequest;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.geom.ColorTransform;
    import flash.filters.ColorMatrixFilter;
    import flash.filters.DropShadowFilter;

    public class B21_StatusCompanions extends MovieClip {
        private var rows:Array = [];
        // TEMP diagnostic (10-05): Tales logs this so a missing portrait shows whether the image loaded.
        public var b21PictureState:String = "";
        // FO76's row art has a clip-depth mask, which GFx applies only under a filtered ancestor (see the
        // masked groups in B21_StatusHUD). Portraits stay outside that filter: under it they did not draw in game.
        // Their shadows, backgrounds and outlines are drawn without filters for the same reason.
        private var rowLayer:MovieClip = new MovieClip();
        private var shadows:MovieClip = new MovieClip();
        private var backs:MovieClip = new MovieClip();
        private var pictures:MovieClip = new MovieClip();
        private var frames:MovieClip = new MovieClip();
        private var meters:MovieClip = new MovieClip();
        private var levels:MovieClip = new MovieClip();
        // iCompanionPortraitSize: 56 at 100%.
        private var pictureSize:Number = 56;
        // The compact meters' radiation, darker than the downed red.
        private var radsColor:uint = 0xD9472B;

        public function B21_StatusCompanions() {
            mouseEnabled = mouseChildren = false;
            rowLayer.filters = [new ColorMatrixFilter([254/255,0,0,0,0,0,254/255,0,0,0,0,0,254/255,0,0,0,0,0,1,0])];
            rowLayer.name = "rows";
            shadows.name = "shadows";
            backs.name = "backs";
            pictures.name = "pictures";
            frames.name = "frames";
            meters.name = "meters";
            levels.name = "levels";
            addChild(rowLayer);
            addChild(shadows);
            addChild(backs);
            addChild(pictures);
            addChild(frames);
            addChild(meters);
            addChild(levels);
        }

        // Bar-aligned and shadowed like FO76's nameField_mc (its PlaceObject filter), which we hide because its
        // field is right-aligned for FO76's own layout.
        private function label(font:String, size:Number, y:Number):TextField {
            var result:TextField = new TextField();
            result.defaultTextFormat = new TextFormat(font,size,0xFFFFCB);
            result.x = 74;
            result.y = y;
            result.width = 230;
            result.height = size+8;
            result.selectable = result.mouseEnabled = false;
            result.filters = [new DropShadowFilter(2,45,0,1,2,2,1,3)];
            return result;
        }

        private function row():Object {
            var art:MovieClip = new B21_StatusCompanionRow();
            var holder:MovieClip = new MovieClip();
            holder.addChild(art);
            art.x = 64;
            art.nameField_mc.visible = false;
            var name:TextField = label("$MAIN_Font_Bold",22,4);
            var perk:TextField = label("$MAIN_Font",14,38);
            holder.addChild(name);
            holder.addChild(perk);
            var picture:Loader = new Loader();
            picture.visible = false;
            pictures.addChild(picture);
            picture.contentLoaderInfo.addEventListener(Event.COMPLETE,pictureLoaded);
            picture.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR,pictureFailed);
            var shade:Loader = new Loader();
            shade.visible = false;
            // The portrait's own pixels in black: a silhouette shadow for the transparent realistic portraits.
            shade.transform.colorTransform = new ColorTransform(0,0,0,.8);
            shadows.addChild(shade);
            shade.contentLoaderInfo.addEventListener(Event.COMPLETE,shadeLoaded);
            var back:Shape = new Shape();
            back.visible = false;
            backs.addChild(back);
            var frame:Shape = new Shape();
            frame.visible = false;
            frames.addChild(frame);
            // Compact meter: track, health, radiation.
            var gauge:MovieClip = new MovieClip();
            gauge.visible = false;
            gauge.addChild(new Shape());
            gauge.addChild(new Shape());
            gauge.addChild(new Shape());
            meters.addChild(gauge);
            var progression:MovieClip = new MovieClip();
            var chevrons:Shape = new Shape();
            progression.addChild(chevrons);
            var badge:MovieClip = new MovieClip();
            progression.addChild(badge);
            var level:TextField = label("$MAIN_Font_Bold",14,-2);
            level.x = 0;
            level.width = 28;
            level.height = 22;
            level.defaultTextFormat = new TextFormat("$MAIN_Font_Bold",14,0xFFFFCB,null,null,null,null,null,"center");
            badge.addChild(level);
            levels.addChild(progression);
            rowLayer.addChild(holder);
            return {holder:holder,art:art,name:name,perk:perk,picture:picture,url:"",shade:shade,shadeUrl:"",
                back:back,backKey:"",frame:frame,frameKey:"",gauge:gauge,gaugeKey:"",
                progression:progression,badge:badge,level:level,chevrons:chevrons};
        }

        private function updateProgression(view:Object, member:Object, tint:uint, clearance:Number):void {
            var level:Number = Number(member.level);
            view.progression.visible = isFinite(level) && level > 0;
            if (!view.progression.visible) return;
            view.progression.x = view.picture.x;
            view.progression.y = view.picture.y;
            var size:Number = pictureSize/56;
            view.badge.scaleX = view.badge.scaleY = size;
            view.badge.x = -34*size-clearance;
            view.badge.y = (pictureSize-18*size)/2;
            view.badge.graphics.clear();
            view.badge.graphics.beginFill(0,.8);
            view.badge.graphics.drawRect(0,0,28,18);
            view.badge.graphics.endFill();
            view.level.text = String(Math.floor(level));
            view.level.textColor = tint;
            var phase:Number = Number(member.levelUp);
            view.chevrons.visible = view.picture.visible && isFinite(phase) && phase >= 0 && phase < 1;
            if (!view.chevrons.visible) return;
            view.chevrons.scaleX = view.chevrons.scaleY = size*.85;
            view.chevrons.x = (pictureSize-24*size*.85)/2;
            view.chevrons.y = pictureSize*(.34-.22*phase);
            view.chevrons.alpha = Math.min(1,phase/.12,(1-phase)/.25);
            var graphics:Graphics = view.chevrons.graphics;
            graphics.clear();
            for (var i:int = 0; i < 2; ++i) {
                var y:Number = i*9;
                graphics.beginFill(tint);
                graphics.moveTo(0,y+8);
                graphics.lineTo(12,y);
                graphics.lineTo(24,y+8);
                graphics.lineTo(24,y+13);
                graphics.lineTo(12,y+5);
                graphics.lineTo(0,y+13);
                graphics.lineTo(0,y+8);
                graphics.endFill();
            }
        }

        private function pictureLoaded(event:Event):void {
            var picture:Loader = event.target.loader as Loader;
            // Portraits are 128 px and drawn at 56; unsmoothed, the thin lines of the drawn styles break up.
            var bitmap:Bitmap = picture.content as Bitmap;
            if (bitmap != null) bitmap.smoothing = true;
            picture.width = picture.height = pictureSize;
            b21PictureState = "loaded " + event.target.url + " size " + picture.width + "x" + picture.height + " visible " + picture.visible;
        }
        private function pictureFailed(event:IOErrorEvent):void {
            event.target.loader.visible = false;
            b21PictureState = "failed " + event.text;
        }
        private function shadeLoaded(event:Event):void {
            var shade:Loader = event.target.loader as Loader;
            var bitmap:Bitmap = shade.content as Bitmap;
            if (bitmap != null) bitmap.smoothing = true;
            shade.width = shade.height = pictureSize;
        }

        // The shape the portrait is cut to (sCompanionPortraitShape), `offset` down and right. A rounded square's
        // corner radius matches kRoundedCorner in CompanionPortraitRules.h, so the tile hugs the cut capture.
        private function tile(graphics:Graphics, offset:Number, shape:String):void {
            if (shape == "circle") graphics.drawCircle(offset+pictureSize/2,offset+pictureSize/2,pictureSize/2);
            else if (shape == "rounded") roundedTile(graphics,offset);
            else graphics.drawRect(offset,offset,pictureSize,pictureSize);
        }
        private function roundedTile(graphics:Graphics, offset:Number):void {
            var radius:Number = pictureSize*.15;
            var near:Number = offset+radius;
            var far:Number = offset+pictureSize-radius;
            var centres:Array = [[far,near],[far,far],[near,far],[near,near]];
            for (var corner:int = 0; corner < 4; ++corner)
                for (var step:int = 0; step <= 8; ++step) {
                    var turn:Number = (corner+step/8)*Math.PI/2;
                    var x:Number = centres[corner][0]+radius*Math.sin(turn);
                    var y:Number = centres[corner][1]-radius*Math.cos(turn);
                    if (corner == 0 && step == 0) graphics.moveTo(x,y);
                    else graphics.lineTo(x,y);
                }
            graphics.lineTo(far,offset);
        }
        private function fill(graphics:Graphics, color:uint, alpha:Number, offset:Number, shape:String):void {
            graphics.beginFill(color,alpha);
            tile(graphics,offset,shape);
            graphics.endFill();
        }

        // sCompanionPortraitBackground: grey or the row colour, see-through or solid; null for "none".
        private function backing(background:String, tint:uint):Object {
            if (background == "grey") return {color:uint(0x4B4B4B),alpha:Number(.55)};
            if (background == "hud") return {color:tint,alpha:Number(.35)};
            if (background == "solidgrey") return {color:uint(0x4B4B4B),alpha:Number(1)};
            if (background == "solidhud") return {color:tint,alpha:Number(1)};
            return null;
        }

        // Part of the circle around the tile, from `from` to `to` (fractions of a turn clockwise from 12 o'clock).
        private function arc(graphics:Graphics, radius:Number, from:Number, to:Number):void {
            if (to <= from) return;
            var centre:Number = pictureSize/2;
            var steps:Number = Math.max(2,Math.ceil((to-from)*72));
            for (var step:int = 0; step <= steps; ++step) {
                var turn:Number = (from+(to-from)*step/steps)*2*Math.PI;
                var x:Number = centre+radius*Math.sin(turn);
                var y:Number = centre-radius*Math.cos(turn);
                if (step == 0) graphics.moveTo(x,y);
                else graphics.lineTo(x,y);
            }
        }

        // Ring around a round portrait or bar beside a square one; reach() is how far either extends past the tile.
        private function thickness(circle:Boolean):Number {
            return circle ? Math.max(3,pictureSize*.07) : Math.max(4,pictureSize*.09);
        }
        private function reach(circle:Boolean):Number {
            return (circle ? 2 : 4)+thickness(circle);
        }

        // Health fills clockwise from 12 o'clock, or up from the bottom. Radiation takes the far end, as on FO76's HP bar.
        private function drawGauge(gauge:MovieClip, circle:Boolean, color:uint, health:Number, rads:Number):void {
            var track:Graphics = Shape(gauge.getChildAt(0)).graphics;
            var life:Graphics = Shape(gauge.getChildAt(1)).graphics;
            var radiation:Graphics = Shape(gauge.getChildAt(2)).graphics;
            track.clear();
            life.clear();
            radiation.clear();
            var width:Number = thickness(circle);
            if (circle) {
                var radius:Number = pictureSize/2+2+width/2;
                track.lineStyle(width,0,.5,false,"normal","none");
                arc(track,radius,0,1);
                life.lineStyle(width,color,1,false,"normal","none");
                arc(life,radius,0,health);
                radiation.lineStyle(width,radsColor,1,false,"normal","none");
                arc(radiation,radius,1-rads,1);
                return;
            }
            var left:Number = pictureSize+4;
            track.beginFill(0,.5);
            track.drawRect(left,0,width,pictureSize);
            track.endFill();
            if (health > 0) {
                life.beginFill(color,1);
                life.drawRect(left,pictureSize*(1-health),width,pictureSize*health);
                life.endFill();
            }
            if (rads > 0) {
                radiation.beginFill(radsColor,1);
                radiation.drawRect(left,0,width,pictureSize*rads);
                radiation.endFill();
            }
        }

        // scale: iCompanionPortraitSize / 100; NaN from a DLL that sends none. border, shadow, shape, background and
        // compact: bCompanionPortraitBorder, bCompanionPortraitShadow, sCompanionPortraitShape (square, rounded or
        // circle), sCompanionPortraitBackground and bCompanionCompactHUD; false or undefined ("none") from an older DLL.
        // A rounded portrait takes the square's bar in the compact HUD.
        public function update(members:Array, color:uint, scale:Number, border:Boolean, shadow:Boolean,
                shape:String, background:String, compact:Boolean):void {
            var circle:Boolean = shape == "circle";
            // Our compiler emits no coercion, so GFx types this register from the stored value; with an int
            // 0 on one branch it read the length as 0 in arithmetic and every row sat at y = 0.
            var count:Number = Number(members == null ? 0 : members.length);
            while (rows.length < count) rows.push(row());
            while (rows.length > count) {
                var removed:Object = rows.pop();
                removed.picture.unload();
                removed.shade.unload();
                pictures.removeChild(removed.picture);
                shadows.removeChild(removed.shade);
                backs.removeChild(removed.back);
                frames.removeChild(removed.frame);
                meters.removeChild(removed.gauge);
                levels.removeChild(removed.progression);
                rowLayer.removeChild(removed.holder);
            }
            visible = count > 0;
            pictureSize = 56*(isFinite(scale) && scale > 0 ? scale : 1);
            // The bar moves aside by the portrait's extra width, and the two stay centred on each other. A row is
            // the portrait plus a 6-unit gap once it outgrows the 76-unit bar row: the character fills only the lower
            // three quarters of its tile, so rows of 76 plus the growth left 200% faces 45 units apart (10-05).
            // A column holds as many rows as fit in the eight rows of 100%.
            // bCompanionCompactHUD: only the portraits, each with its meter, and no name, bar or perk. A companion
            // without a portrait would be a bare meter, so until every portrait is in the team keeps its full rows.
            var compactLayout:Boolean = compact && count > 0;
            for (var j:int = 0; j < count; ++j)
                if (members[j].portrait == null || String(members[j].portrait).length == 0) compactLayout = false;
            var extra:Number = pictureSize-56;
            var outside:Number = compactLayout ? reach(circle) : 0;
            var span:Number = pictureSize+outside+(circle ? outside : 0);
            var levelGutter:Number = 0;
            for (j = 0; j < count; ++j)
                if (isFinite(Number(members[j].level)) && Number(members[j].level) > 0) levelGutter = 34*pictureSize/56;
            var pitch:Number = compactLayout ? (circle ? span : pictureSize)+6 : Math.max(76,pictureSize+6);
            var perColumn:Number = Math.max(1,Math.floor(8*76/pitch));
            for (var i:int = 0; i < count; ++i) {
                var view:Object = rows[i];
                var member:Object = members[i];
                var column:Number = Math.floor(i / perColumn);
                var left:Number = levelGutter+column*((compactLayout ? span+16 : 320+extra)+levelGutter);
                var top:Number = (i-column*perColumn-Math.min(perColumn,count-column*perColumn))*pitch;
                view.holder.visible = !compactLayout;
                view.holder.x = left+extra;
                view.holder.y = top+(pitch-76)/2;
                view.picture.x = left-8+(compactLayout && circle ? outside : 0);
                view.picture.y = top+(pitch-pictureSize)/2;
                if (view.picture.content != null) view.picture.width = view.picture.height = pictureSize;
                var tint:uint = member.downed ? 0xF67259 : color;
                view.name.text = String(member.name);
                view.name.textColor = view.perk.textColor = tint;
                view.perk.text = member.shared == null ? "" : String(member.shared);
                // FO76 draws the lead's star where the shared-card icon sits.
                view.art.LeaderIcon_mc.visible = member.leader == true;
                view.art.SharedPerkCardIcon_mc.visible = view.perk.text.length > 0 && !view.art.LeaderIcon_mc.visible;
                var meter:MovieClip = view.art.Meter_mc;
                var health:Number = Number(member.health);
                meter.visible = isFinite(health) && health >= 0;
                if (meter.visible) meter.gotoAndStop(1+Math.round(Math.min(1,health)*(meter.totalFrames-1)));
                view.art.transform.colorTransform = new ColorTransform(
                    (tint >> 16 & 255)/255,(tint >> 8 & 255)/255,(tint & 255)/203);
                // Glyph and outline portraits are drawn in greys for the row colour, like FO76's HUD icons.
                view.picture.transform.colorTransform = member.tinted == true ? new ColorTransform(
                    (tint >> 16 & 255)/255,(tint >> 8 & 255)/255,(tint & 255)/255) : new ColorTransform();
                var url:String = member.portrait == null ? "" : String(member.portrait);
                if (url != view.url) {
                    view.picture.unload();
                    view.url = url;
                    view.picture.visible = url.length > 0;
                    if (url.length > 0) view.picture.load(new URLRequest(url));
                    b21PictureState = "requested " + url;
                }
                var back:Object = view.picture.visible ? backing(background,tint) : null;
                view.back.visible = back != null;
                view.back.x = view.picture.x;
                view.back.y = view.picture.y;
                var backKey:String = back == null ? "" : pictureSize+" "+back.color+" "+back.alpha+" "+shape+" "+shadow;
                if (backKey != view.backKey) {
                    view.backKey = backKey;
                    view.back.graphics.clear();
                    // A backed portrait's tile casts the shadow (bCompanionPortraitShadow), 3 units down and right.
                    if (back != null && shadow) fill(view.back.graphics,0,.8,3,shape);
                    if (back != null) fill(view.back.graphics,uint(back.color),Number(back.alpha),0,shape);
                }
                // An unbacked portrait casts its own silhouette: the portrait in black, loaded only while needed.
                var shaded:Boolean = shadow && view.picture.visible && back == null;
                var shadeUrl:String = shaded ? url : "";
                if (shadeUrl != view.shadeUrl) {
                    view.shade.unload();
                    view.shadeUrl = shadeUrl;
                    if (shadeUrl.length > 0) view.shade.load(new URLRequest(shadeUrl));
                }
                view.shade.visible = shaded;
                view.shade.x = view.picture.x+3;
                view.shade.y = view.picture.y+3;
                if (view.shade.content != null) view.shade.width = view.shade.height = pictureSize;
                // bCompanionPortraitBorder: a 2-unit outline in the row colour.
                var framed:Boolean = border && view.picture.visible;
                view.frame.visible = framed;
                view.frame.x = view.picture.x;
                view.frame.y = view.picture.y;
                var frameKey:String = framed ? pictureSize+" "+tint+" "+shape : "";
                if (frameKey != view.frameKey) {
                    view.frameKey = frameKey;
                    view.frame.graphics.clear();
                    if (framed) {
                        view.frame.graphics.lineStyle(2,tint,1,true);
                        tile(view.frame.graphics,0,shape);
                    }
                }
                var life:Number = isFinite(health) ? Math.min(1,Math.max(0,health)) : 0;
                var rads:Number = Number(member.rads);
                rads = isFinite(rads) ? Math.min(1,Math.max(0,rads)) : 0;
                view.gauge.visible = compactLayout;
                view.gauge.x = view.picture.x;
                view.gauge.y = view.picture.y;
                var gaugeKey:String = compactLayout ? pictureSize+" "+circle+" "+tint+" "+life+" "+rads : "";
                if (gaugeKey != view.gaugeKey) {
                    view.gaugeKey = gaugeKey;
                    if (compactLayout) drawGauge(view.gauge,circle,tint,life,rads);
                }
                updateProgression(view,member,tint,compactLayout && circle ? outside : 0);
            }
        }
    }
}
