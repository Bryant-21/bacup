package {
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.display.MovieClip;
    import flash.filters.ColorMatrixFilter;
    import flash.geom.ColorTransform;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.utils.getTimer;

    public class B21_StatusNameplates extends MovieClip {
        private static const CREAM:uint = 0xFFFFCB;
        private static const RED:uint = 0xF67259;
        private var plates:Object = {};

        public function B21_StatusNameplates() {
            mouseChildren = mouseEnabled = false;
            visible = false;
            // GFx only clips the source meter's masks under a non-identity filtered ancestor.
            filters = [new ColorMatrixFilter([254/255,0,0,0,0,0,254/255,0,0,0,0,0,254/255,0,0,0,0,0,1,0])];
        }

        private function freeze(node:DisplayObject):void {
            if (node is MovieClip) MovieClip(node).stop();
            var container:DisplayObjectContainer = node as DisplayObjectContainer;
            if (container != null) for (var i:int = 0; i < container.numChildren; ++i) freeze(container.getChildAt(i));
        }

        private function field(node:DisplayObject):TextField {
            if (node is TextField) return TextField(node);
            var container:DisplayObjectContainer = node as DisplayObjectContainer;
            if (container != null) for (var i:int = 0; i < container.numChildren; ++i) {
                var found:TextField = field(container.getChildAt(i));
                if (found != null) return found;
            }
            return null;
        }

        private function label(node:DisplayObject, value:String):void {
            var target:TextField = field(node);
            if (target == null) return;
            var format:TextFormat = target.defaultTextFormat;
            target.text = value;
            target.setTextFormat(format);
            if (target.textWidth > target.width-4) {
                format.size = Math.max(1,Math.floor(Number(format.size)*(target.width-4)/target.textWidth));
                target.setTextFormat(format);
            }
        }

        private function tint(node:DisplayObject, color:uint, source:uint = 0xFFFFCB):void {
            node.transform.colorTransform = new ColorTransform(
                Number(color >> 16 & 255)/Number(source >> 16 & 255),
                Number(color >> 8 & 255)/Number(source >> 8 & 255),
                Number(color & 255)/Number(source & 255));
        }

        private function fill(meter:Object, percent:Number):void {
            var bar:MovieClip = meter.MeterBarInternal_mc.Contents.Fill as MovieClip;
            bar.gotoAndStop(1+Math.floor(300*Math.max(0,Math.min(1,percent))));
            if (bar.healthBarLeader != null) bar.healthBarLeader.visible = false;
        }

        private function shownColor(node:DisplayObject, base:uint = 0xFFFFCB):Array {
            var transform:ColorTransform = node.transform.colorTransform;
            return [Math.round(transform.redMultiplier*(base >> 16 & 255)),
                Math.round(transform.greenMultiplier*(base >> 8 & 255)),Math.round(transform.blueMultiplier*(base & 255))];
        }

        private function createPlate():Object {
            var holder:MovieClip = new MovieClip();
            var meter:MovieClip = new B21_StatusEnemy();
            var stars:MovieClip = new MovieClip();
            var details:B21_StatusReadout = new B21_StatusReadout();
            holder.addChild(meter);
            meter.addChild(stars);
            holder.addChild(details);
            addChild(holder);
            return {holder:holder,meter:meter,stars:stars,details:details,lastHealth:NaN,trailHealth:NaN,trailHold:0};
        }

        private function healthTrail(plate:Object, percent:Number):Number {
            var now:int = getTimer();
            if (!isFinite(plate.lastHealth) || percent > plate.lastHealth || percent >= plate.trailHealth) {
                plate.trailHealth = percent;
                plate.trailHold = now;
            } else if (percent < plate.lastHealth) plate.trailHold = now+300;
            plate.lastHealth = percent;
            var drained:Number = Math.max(0,Math.min(1,(now-plate.trailHold)/200));
            if (drained >= 1) plate.trailHealth = percent;
            return plate.trailHealth-(plate.trailHealth-percent)*drained;
        }

        private function lighter(color:uint):uint {
            var mixed:uint = 0;
            for (var shift:int = 0; shift <= 16; shift += 8) {
                var channel:uint = color >> shift & 0xFF;
                mixed |= uint(channel+(0xFF-channel)*0.45) << shift;
            }
            return mixed;
        }

        private function layoutBounds(plate:Object):Rectangle {
            var meter:MovieClip = plate.meter;
            var bounds:Rectangle = meter.HealthBarFrame_mc.getBounds(plate.holder);
            for each (var node:DisplayObject in [meter.DisplayText_mc,meter.LevelText_mc,plate.stars,
                meter.EncounterHolder_mc,plate.details])
                if (node.visible) bounds = bounds.union(node.getBounds(plate.holder));
            return bounds;
        }

        private function paint(plate:Object, row:Object, color:uint):void {
            var meter:MovieClip = plate.meter;
            var hostile:Boolean = int(row.standing) == 0;
            var allied:Boolean = int(row.standing) == 1;
            var standing:uint = hostile ? RED : allied && row.color != null ? uint(row.color) : allied ? color : 0xF7CC5D;
            meter.gotoAndStop(hostile ? "Hostile" : allied ? "Friendly" : "Nonhostile");
            freeze(meter);
            for each (var bar:String in ["MeterBar_mc","MeterBarEnemy_mc","MeterBarFriendly_mc"]) {
                fill(meter[bar],Number(row.health));
                tint(meter[bar],standing,bar == "MeterBarEnemy_mc" ? RED : CREAM);
            }
            var percent:Number = Number(row.health);
            var trail:Number = healthTrail(plate,percent);
            meter.Optional_mc.visible = trail > percent+0.002;
            if (meter.Optional_mc.visible) {
                fill(meter.Optional_mc,trail);
                var pulse:MovieClip = meter.Optional_mc.MeterBarInternal_mc;
                pulse.gotoAndStop(1+int(getTimer()*30/1000)%pulse.totalFrames);
                tint(meter.Optional_mc,lighter(standing));
            }
            label(meter.DisplayText_mc,String(row.name));
            // The Hostile frame already colors the cream name through its source filter.
            tint(meter.DisplayText_mc,hostile ? CREAM : standing);
            meter.LevelText_mc.gotoAndStop(Boolean(row.boss) ? "star" : "square");
            label(meter.LevelText_mc,String(row.level));
            tint(meter.LevelText_mc,standing);
            var nameField:TextField = field(meter.DisplayText_mc);
            var nameBox:Rectangle = nameField.getBounds(meter);
            var right:Number = nameBox.x+nameBox.width/2+nameField.textWidth/2*nameBox.width/nameField.width;
            var stars:MovieClip = plate.stars;
            var rank:int = Math.max(0,Math.min(5,int(row.rank)));
            while (stars.numChildren > rank) stars.removeChildAt(stars.numChildren-1);
            while (stars.numChildren < rank) stars.addChild(new B21_StatusStar());
            for (var i:int = 0; i < rank; ++i) {
                var star:MovieClip = stars.getChildAt(i) as MovieClip;
                star.gotoAndStop("full");
                star.x = i*22;
            }
            stars.x = right+12;
            stars.y = -40;
            tint(stars,standing);
            var boss:MovieClip = meter.EncounterHolder_mc;
            boss.visible = Boolean(row.boss);
            if (boss.visible) {
                boss.gotoAndStop("Skull");
                boss.Encounter_mc.gotoAndStop("Difficult");
                boss.Encounter_mc.BossIcon_mc.visible = true;
                boss.x = boss.y = 0;
                var bounds:Rectangle = boss.getBounds(meter);
                boss.x = (rank > 0 ? stars.getBounds(meter).right : right)+8-bounds.x;
                boss.y = -40-bounds.y-bounds.height/2;
                tint(boss,standing,RED);
            }
            var healthBounds:Rectangle = meter.HealthBarFrame_mc.getBounds(plate.holder);
            plate.details.x = healthBounds.x+healthBounds.width/2;
            plate.details.y = healthBounds.bottom+8;
            plate.details.update(row,standing);
        }

        public function update(data:Object, viewport:Object, scale:Number, color:uint):Boolean {
            var enabled:Boolean = data != null && Boolean(data.enabled);
            var rows:Array = enabled ? data.rows as Array : null;
            var kept:Object = {};
            var next:Object = {};
            var left:Number = viewport == null ? 0 : Number(viewport.x);
            var top:Number = viewport == null ? 0 : Number(viewport.y);
            var width:Number = viewport == null ? stage.stageWidth : Number(viewport.width);
            var height:Number = viewport == null ? stage.stageHeight : Number(viewport.height);
            var corner:Point = globalToLocal(new Point(left,top));
            var opposite:Point = globalToLocal(new Point(left+width,top+height));
            var area:Rectangle = new Rectangle(corner.x,corner.y,opposite.x-corner.x,opposite.y-corner.y);
            var margin:Number = 6*scale;
            if (rows != null) for (var i:int = 0; i < rows.length && i < 32; ++i) {
                var row:Object = rows[i];
                var key:String = "actor"+String(row.id);
                var plate:Object = plates[key];
                if (plate == null) { plate = createPlate(); plates[key] = plate; }
                kept[key] = true;
                next[key] = plate;
                paint(plate,row,color);
                var holder:MovieClip = plate.holder;
                var bounds:Rectangle = layoutBounds(plate);
                holder.scaleX = holder.scaleY = Math.min(scale*Number(row.scale),
                    (area.width-2*margin)/bounds.width,(area.height-2*margin)/bounds.height);
                var point:Point = globalToLocal(new Point(left+Number(row.x)*width,top+Number(row.y)*height));
                var bar:Rectangle = plate.meter.HealthBarFrame_mc.getBounds(holder);
                holder.x = point.x-(bar.x+bar.width/2)*holder.scaleX;
                var clearance:Number = Math.max(0,bounds.bottom-bar.bottom)*scale;
                holder.y = point.y-bar.bottom*holder.scaleY-clearance-12*scale;
                holder.x = Math.max(area.left+margin-bounds.left*holder.scaleX,
                    Math.min(area.right-margin-bounds.right*holder.scaleX,holder.x));
                holder.y = Math.max(area.top+margin-bounds.top*holder.scaleY,
                    Math.min(area.bottom-margin-bounds.bottom*holder.scaleY,holder.y));
                plate.anchorX = left+Number(row.x)*width;
                plate.anchorY = top+Number(row.y)*height;
            }
            for (var name:String in plates) if (kept[name] != true) {
                removeChild(plates[name].holder);
            }
            plates = next;
            visible = enabled;
            return enabled;
        }

        public function diagnostic():Array {
            var result:Array = [];
            for (var name:String in plates) {
                var plate:Object = plates[name];
                var at:Point = localToGlobal(new Point(plate.holder.x,plate.holder.y));
                var barBounds:Rectangle = plate.meter.HealthBarFrame_mc.getBounds(stage);
                var detailsBounds:Rectangle = plate.details.getBounds(stage);
                var bounds:Rectangle = layoutBounds(plate);
                var first:Point = plate.holder.localToGlobal(new Point(bounds.left,bounds.top));
                var last:Point = plate.holder.localToGlobal(new Point(bounds.right,bounds.bottom));
                result.push({id:name,name:field(plate.meter.DisplayText_mc).text,x:at.x,y:at.y,
                    anchorX:plate.anchorX,anchorY:plate.anchorY,
                    plateLeft:first.x,plateTop:first.y,plateRight:last.x,plateBottom:last.y,
                    plateScale:plate.holder.scaleY,barWidth:barBounds.width,
                    barColor:shownColor(plate.meter.MeterBar_mc),nameColor:shownColor(plate.meter.DisplayText_mc),
                    levelColor:shownColor(plate.meter.LevelText_mc),starColor:shownColor(plate.stars),
                    bossColor:shownColor(plate.meter.EncounterHolder_mc,RED),
                    detailsColor:field(plate.details) == null ? -1 : field(plate.details).textColor,
                    stars:plate.stars.numChildren,boss:plate.meter.EncounterHolder_mc.visible,
                    details:plate.details.text,icons:plate.details.icons,
                    detailsHeight:plate.details.height,
                    barCenter:barBounds.x+barBounds.width/2,detailsCenter:detailsBounds.x+detailsBounds.width/2,
                    barBottom:barBounds.bottom,detailsTop:detailsBounds.top,
                    nameVisible:plate.meter.DisplayText_mc.visible,levelVisible:plate.meter.LevelText_mc.visible,
                    trailVisible:plate.meter.Optional_mc.visible,
                    trailFrame:plate.meter.Optional_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,
                    frame:plate.meter.MeterBar_mc.MeterBarInternal_mc.Contents.Fill.currentFrame});
            }
            return result;
        }
    }
}
