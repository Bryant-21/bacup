package {
    import flash.display.MovieClip;
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.filters.ColorMatrixFilter;
    import flash.filters.DropShadowFilter;
    import flash.geom.ColorTransform;
    import flash.geom.Point;
    import flash.geom.Matrix;
    import flash.geom.Rectangle;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.utils.getDefinitionByName;
    import flash.utils.getTimer;
    import flash.utils.getQualifiedClassName;

    public class B21_StatusHUD extends MovieClip {
        public var B21Version:uint = 9;
        public var B21NameplatesVersion:uint = 2;
        public var B21NameplatesActive:uint = 0;
        public var nameplates:B21_StatusNameplates = new B21_StatusNameplates();
        // Two instances run: one inside HUDMenu (interaction, rewards) that captures the stock HUD
        // into B21Snapshot, and one in Tales' own HUD-depth menu that draws everything passive from
        // that snapshot. HUDMenu's glass renderer made translucent FO76 art opaque with black shadows.
        public var B21Snapshot:Object = null;
        public var B21HealthActive:uint = 0;
        public var B21RewardsVersion:uint = 1;
        public var B21RewardNotice:uint = 0;
        public var B21RewardQuestArt:uint = 0;
        public var B21RewardDone:uint = 0;
        public var B21RewardActive:uint = 0;
        public var B21RewardModel:uint = 0;
        public var B21RewardQuestLease:uint = 0;
        public var B21RewardModelX:Number = 0.5;
        public var B21RewardModelY:Number = 0.4;
        private var rewards:B21_StatusRewards = new B21_StatusRewards();
        public var B21HoldActive:uint = 0;
        // Diagnostic (2026-10-05, the Daily Ops hold prompt has never drawn in game): the step that stopped it.
        public var B21HoldIssue:String = "";
        public var B21QuickLootActive:uint = 0;
        public var B21QuickLootIssue:String = "";
        // Diagnostic (2026-10-05, stacked compass icons in game): set while a location clone's icons or frame
        // differ from FO4's marker, or visible locations share a bearing.
        public var B21CompassIssue:String = "";
        public var B21CrosshairActive:uint = 0;
        public var B21HitActive:uint = 0;
        // 1 while the snapshot carries FO4's tagged in-world markers, so Tales hides them for FO76's.
        public var B21FloatingActive:uint = 0;
        private var quickLoot:B21_StatusLoot = new B21_StatusLoot();
        private var reticle:B21_StatusReticle = new B21_StatusReticle();
        private var reticleHostile:Boolean = false;
        private var hit:MovieClip = new B21_StatusHit();
        private var hitFrame:int = 0;
        private var hitCount:int = 0;
        private var hitState:Object = null;
        private var hitSeen:int = -1;
        // FO76's game code keeps Optional_mc at the enemy's earlier health while hits land and then drains it;
        // FO4 never feeds that bar, so the movie keeps the trail itself.
        private static const TRAIL_HOLD_MS:int = 300;
        private static const TRAIL_DRAIN_MS:int = 200;
        private var trailTarget:Number = -1;
        private var trailPercent:Number = 0;
        private var trailLast:Number = 0;
        private var trailHold:int = 0;
        private var crossState:Object = null;
        private var lootState:Object = null;
        public var interaction:MovieClip = new B21_StatusInteraction();
        private var holdButton:MovieClip = new B21_StatusHoldButton();
        private var holdCompleteTime:Number = -1;
        private var holdHint:Object = null;
        public var healthGroup:MovieClip = new B21_StatusLeft();
        public var rightGroup:MovieClip = new B21_StatusRight();
        public var compassGroup:MovieClip = new B21_StatusCompass();
        public var questGroup:B21_StatusQuestTracker = new B21_StatusQuestTracker();
        public var conditionGroup:MovieClip = new MovieClip();
        public var experienceGroup:MovieClip = new MovieClip();
        public var enemy:MovieClip = new B21_StatusEnemy();
        public var stealth:MovieClip = new B21_StatusStealth();
        public var critical:MovieClip = new B21_StatusCritical();
        public var encounters:MovieClip = new B21_StatusEncounter();
        public var core:MovieClip = new B21_StatusCore();
        public var explosives:MovieClip = new MovieClip();
        public var floatingGroup:MovieClip = new MovieClip();
        public var damageGroup:MovieClip = new MovieClip();
        public var companions:B21_StatusCompanions = new B21_StatusCompanions();
        private var enemyStars:MovieClip = new MovieClip();
        private var targetReadout:B21_StatusReadout = new B21_StatusReadout();
        public var experienceDone:uint = 0;
        private var hp:MovieClip;
        private var ap:MovieClip;
        private var cnd:MovieClip = new B21_StatusEquippedCondition();
        private var conditionBox:MovieClip = new MovieClip();
        private var conditionFrame:MovieClip;
        private var weaponIcon:MovieClip;
        private var weaponName:String = "";
        private var weaponRuns:Array = null;
        private var keywordIcons:Object = {/* BACUP_ICON_KEYWORDS */};
        private var icons:MovieClip = new MovieClip();
        // Effect tiles live outside rightGroup: its group tint would also recolour red negative tiles.
        private var effectLayer:MovieClip = new MovieClip();
        private var enemyNameColor:uint;
        private static const CREAM:uint = 0xFFFFCB;
        private static const GOLD:uint = 0xF5CB5B;
        private static const HOSTILE:uint = 0xF67259;
        private static const NEUTRAL:uint = 0xF7CC5D;
        private static const ALLY_LAYER:int = 1;
        private static const NEUTRAL_LAYER:int = 2;
        private static const HOSTILE_LAYER:int = 3;
        // FO76 CrosshairBase's _hostileColorMatrixFilter: AdjustColor(brightness -90, contrast 15,
        // saturation 25, hue -50), which takes the cream ticks to (255,161,121).
        private static const HOSTILE_RETICLE:Array = [1.5483,1.1191,-1.4874,0,-117.63, -0.3422,1.094,0.4282,0,-117.63,
            1.1289,-1.1429,1.194,0,-117.63, 0,0,0,1,0];
        private var xp:MovieClip = new B21_StatusXP();
        private var levelUp:MovieClip = new B21_StatusLevelUp();
        private var xpSequence:uint = 0;
        private var xpElapsed:Number = 0;
        private var xpLevel:Boolean = false;
        private var xpFrom:Number = 0;
        private var xpTo:Number = 0;
        private var markerCount:int = 0;
        private var compassIssues:Array = [];
        private var compassLocations:Array = [];
        private var compassBearings:Array = [];
        private var needStates:Object = {};
        private var feralShown:Boolean = false;
        private var glowShown:Boolean = false;
        private var glowFrom:Number = 0;
        private var glowTo:Number = 0;
        private var glowTime:Number = 0;
        private var insideArea:Boolean = false;
        private var areaArmor:Boolean = false;
        private var iconNames:Object = {49:"AdrenalineIcon",50:"DiseaseIcon",51:"SleepIcon",
            52:"HungerIcon",53:"ThristIcon",54:"ChemsIcon",55:"StimpakIcon",56:"FoodIcon",57:"AlcoholIcon",58:"StealthBoyIcon"};
        private var threatIcons:Array = ["GrenadeIcon_mc","MineIcon_mc","CarIcon_mc","Arrow_mc"];
        // Parts hidden while a scope shows only the damage numbers; not every part is reset each update.
        private var scopeHidden:Array = null;

        public function B21_StatusHUD() {
            stop();
            mouseEnabled = false;
            mouseChildren = false;
            // In-world markers sit under every other part, as FO4's do.
            addChild(floatingGroup);
            floatingGroup.addChild(new MovieClip());
            floatingGroup.addChild(new MovieClip());
            addChild(healthGroup);
            addChild(rightGroup);
            addChild(effectLayer);
            addChild(compassGroup);
            addChild(questGroup);
            addChild(conditionGroup);
            addChild(experienceGroup);
            addChild(enemy);
            addChild(stealth);
            addChild(critical);
            addChild(encounters);
            addChild(core);
            addChild(explosives);
            addChild(damageGroup);
            addChild(nameplates);
            addChild(companions);
            addChild(rewards);
            addChild(interaction);
            for (var i:int = 0; i < numChildren; ++i) {
                var existing:MovieClip = getChildAt(i) as MovieClip;
                if (existing != null) existing.mouseChildren = existing.mouseEnabled = false;
            }
            mouseChildren = true;
            addChild(quickLoot);
            addChild(reticle);
            reticle.mouseChildren = reticle.mouseEnabled = false;
            addChild(hit);
            hit.mouseChildren = hit.mouseEnabled = false;
            interaction.addChild(holdButton);
            interaction.gotoAndStop(6);
            enemy.addChild(enemyStars);
            targetReadout.y = 14;
            targetReadout.visible = false;
            enemy.addChild(targetReadout);
            conditionGroup.addChild(conditionBox);
            conditionFrame = new MovieClip();
            // Filled edges retain AP's one-pixel black outline without GFx scaling a source stroke.
            conditionFrame.graphics.beginFill(0x000000);
            conditionFrame.graphics.drawRect(0,0,140,1);
            conditionFrame.graphics.drawRect(0,11,140,1);
            conditionFrame.graphics.drawRect(0,1,1,10);
            conditionFrame.graphics.drawRect(139,1,1,10);
            conditionFrame.graphics.endFill();
            conditionFrame.graphics.beginFill(0x191D1D,114/255);
            conditionFrame.graphics.drawRect(1,1,138,10);
            conditionFrame.graphics.endFill();
            conditionBox.addChild(conditionFrame);
            cnd.removeChildAt(0);
            cnd.MeterClip_mc.gotoAndStop(2);
            var fillBounds:Rectangle = cnd.getBounds(cnd);
            cnd.scaleX = 136/fillBounds.width;
            cnd.scaleY = 8/fillBounds.height;
            cnd.x = 2-fillBounds.x*cnd.scaleX;
            cnd.y = 2-fillBounds.y*cnd.scaleY;
            conditionBox.addChild(cnd);
            experienceGroup.addChild(xp);
            experienceGroup.addChild(levelUp);
            freeze(this);
            compassGroup.CompassBar_mc.visible = false;
            hp = healthGroup.HPMeter_mc;
            ap = rightGroup.ActionPointMeter_mc;
            hp.Optional_mc.visible = false;
            hp.GlowMeter_mc.visible = false;
            ap.Optional_mc.visible = false;
            hp.PercentText_mc.visible = true;
            hp.Segments_mc.visible = true;
            ap.ActionPointSegments_mc.visible = false;
            rightGroup.OverheatMeter_mc.Optional_mc.visible = false;
            rightGroup.OverheatMeter_mc.ActionPointSegments_mc.visible = false;
            rightGroup.HUDActiveEffectsWidget_mc.ActiveEffectStatusLabel_mc.visible = false;
            effectLayer.addChild(icons);
            // Each marker holder gets a HUD-tinted layer, ally and neutral layers, and on top one for FO76's
            // baked-red enemy markers, all under the holder's compass mask.
            for each (var holder:MovieClip in [compassGroup.OtherMarkerHolder_mc,compassGroup.QuestMarkerHolder_mc]) {
                while (holder.numChildren > 0) holder.removeChildAt(0);
                for (var layer:int = 0; layer <= HOSTILE_LAYER; ++layer) holder.addChild(new MovieClip());
            }
            // In this menu a clip-depth mask only clips under a filtered ancestor; otherwise GFx draws
            // its red 20% shapes (the compass boxes, a square behind skulls). GFx skips an identity
            // matrix, so this one darkens by 1/255 (invisible). It never changes, so each group renders
            // through one long-lived texture.
            for each (var masked:DisplayObject in [healthGroup,effectLayer,compassGroup,experienceGroup,enemy,encounters,conditionGroup,quickLoot])
                masked.filters = [new ColorMatrixFilter([254/255,0,0,0,0,0,254/255,0,0,0,0,0,254/255,0,0,0,0,0,1,0])];
            enemyNameColor = uint(field(enemy.DisplayText_mc).defaultTextFormat.color);
            experienceGroup.visible = false;
            visible = false;
        }
        private function updateInteraction(data:Object, host:Object, scale:Number):void {
            B21QuickLootActive = B21CrosshairActive = B21HitActive = 0;
            quickLoot.visible = reticle.visible = hit.visible = false;
            var center:Object = host.CenterGroup_mc;
            hitState = crossState = lootState = null;
            if (center == null) { reticle.reset(); hitFrame = 0; return; }
            try {
                var stock:Object = center.QuickContainerWidget_mc;
                if (Boolean(data.quickLoot) && stock != null) {
                    // The menu draws FO76's widget: in HUDMenu the glass renderer blackened its panel and text.
                    var lootShown:Object = chain(stock,host);
                    if (lootShown.v && lootShown.a > 0) {
                        lootState = quickLoot.capture(stock,data);
                        B21QuickLootIssue = quickLoot.issue;
                        if (lootState != null) {
                            lootState.at = pointData(center);
                            lootState.shown = lootShown;
                            B21QuickLootActive = 1;
                        }
                    }
                }
            } catch (error:Error) { B21QuickLootIssue = "error: "+error.message; lootState = null; }
            try {
                stock = center.HUDCrosshair_mc;
                if (Boolean(data.crosshair) && stock != null && stock.hasOwnProperty("B21RequestedState") && stock.hasOwnProperty("B21RequestedRadius")) {
                    var origin:Point = stock.localToGlobal(new Point());
                    var unit:Point = stock.localToGlobal(new Point(1,0));
                    var spread:Number = Number(stock.B21RequestedRadius)*Point.distance(origin,unit)/scale;
                    var requested:String = String(stock.B21RequestedState);
                    // The menu draws FO76's reticle: in HUDMenu the glass renderer thickened its shadow.
                    if (["None","Dot","Standard","Activate","Command"].indexOf(requested) >= 0 && isFinite(spread) && spread >= 0) {
                        crossState = {state:requested,spread:spread,at:pointData(center),shown:chain(stock,host)};
                        B21CrosshairActive = 1;
                    }
                }
            } catch (error:Error) { crossState = null; }
            try {
                stock = center.HitIndicator_mc;
                if (Boolean(data.crosshair) && stock != null) {
                    // FO4's engine replays its indicator from "Start" (frame 4) on every hit. The menu
                    // plays FO76's red one for each restart counted here: drawn in HUDMenu, the glass
                    // renderer blackened its glow.
                    var frame:int = int(stock.currentFrame);
                    if (frame >= 4 && (hitFrame < 4 || frame < hitFrame)) ++hitCount;
                    hitFrame = frame;
                    hitState = {count:hitCount,at:pointData(center),shown:chain(stock,host)};
                    B21HitActive = 1;
                } else hitFrame = 0;
            } catch (error:Error) { hitFrame = 0; hitState = null; }
        }
        private function holdFailed(reason:String):Boolean {
            B21HoldIssue = reason;
            return false;
        }
        private function updateHold(data:Object, host:Object, scale:Number):Boolean {
            B21HoldActive = 0;
            interaction.visible = false;
            var progress:Number = Number(data.holdProgress);
            if (!isFinite(progress) || progress < 0) { holdCompleteTime = -1; holdHint = null; B21HoldIssue = ""; return true; }
            if (!Boolean(data.health)) { B21HoldIssue = "health HUD off"; return true; }
            var stock:Object = host.CenterGroup_mc.RolloverWidget_mc;
            if (stock == null || stock.RolloverName_tf == null) return holdFailed("no stock rollover");
            var bar:DisplayObjectContainer = stock.ButtonHintBar_mc as DisplayObjectContainer;
            if (bar == null) return holdFailed("rollover button bar is "+getQualifiedClassName(stock.ButtonHintBar_mc));
            // The bar's ButtonPoolV and ButtonHintBarInternal_mc read as missing in game (quick loot found
            // the same), so read the hints from the display list as quick loot does.
            for each (var shown:DisplayObjectContainer in B21_StatusLoot.stockHints(bar)) {
                var sourceGlyph:TextField = B21_StatusLoot.hintGlyph(shown);
                if (!shown.visible || sourceGlyph == null) continue;
                holdHint = {glyph:sourceGlyph.text,font:sourceGlyph.getTextFormat().font,pcKey:Boolean(Object(shown).UsePCKey),
                    label:TextField(shown.getChildByName("textField_tf")).text.toUpperCase()};
                break;
            }
            // FO4 removes the displayed hints while its rollover bar redraws; the hold keeps the last one.
            if (holdHint == null) return holdFailed("no visible rollover hint ("+B21_StatusLoot.stockHints(bar).length+" in bar)");
            var glyph:TextField = holdButton.IconHolderInstance.IconAnimInstance.Icon_tf;
            anchor(interaction,[1,0,0,1,0,0],globalPoint(host.CenterGroup_mc),960,540,scale);
            inheritVisibility(stock,interaction,host);
            fittedText(interaction.Internal_mc.Header_mc.Header_tf,stock.RolloverName_tf.text.toUpperCase());
            interaction.Internal_mc.Header_mc.TaggedForSearch_mc.visible = false;
            glyph.text = String(holdHint.glyph);
            var format:TextFormat = glyph.defaultTextFormat;
            format.font = String(holdHint.font);
            glyph.setTextFormat(format);
            glyph.textColor = 0xffffff;
            glyph.autoSize = "left";
            glyph.y = holdHint.pcKey ? 1.25 : 2.25;
            holdButton.textField_tf.text = String(holdHint.label);
            holdButton.textField_tf.textColor = 0xffffff;
            holdButton.textField_tf.autoSize = "left";
            holdButton.textField_tf.x = holdButton.IconHolderInstance.width+10;
            holdButton.SecondaryIconHolderInstance.visible = false;
            holdButton.Highlight_mc.visible = holdButton.Sizer_mc.visible = false;
            var ring:MovieClip = holdButton.HoldMeter_mc;
            var start:int = 0;
            var complete:int = 0;
            for each (var label:Object in ring.currentLabels) {
                if (label.name == "buttonHold") start = label.frame;
                if (label.name == "buttonHoldComplete") complete = label.frame;
            }
            if (start == 0 || complete <= start) return holdFailed("hold ring labels missing");
            if (progress >= 1) {
                holdCompleteTime = holdCompleteTime < 0 ? 0 : holdCompleteTime+Math.max(0,Number(data.delta));
                ring.gotoAndStop(Math.min(ring.totalFrames,complete+Math.floor(holdCompleteTime*B21_StatusSource.layout.holdFPS)));
            } else {
                holdCompleteTime = -1;
                ring.gotoAndStop(progress > 0 ? start+Math.floor((complete-start-1)*progress) : "idle");
            }
            ring.x = holdButton.IconHolderInstance.x+holdButton.IconHolderInstance.width/2;
            ring.scaleX = ring.scaleY = holdHint.pcKey ? 1.25 : 1;
            holdButton.x = B21_StatusSource.layout.holdButton[4] - (holdButton.textField_tf.x+holdButton.textField_tf.width)/2;
            holdButton.y = B21_StatusSource.layout.holdButton[5];
            tintLayer(interaction,uint(data.color));
            B21HoldActive = interaction.visible ? 1 : 0;
            var box:Rectangle = interaction.getBounds(stage);
            B21HoldIssue = interaction.visible && interaction.alpha > 0
                ? "drawn "+holdHint.glyph+" "+holdHint.label+" at "+Math.round(box.x)+","+Math.round(box.y)+" "+Math.round(box.width)+"x"+Math.round(box.height)
                : "rollover hidden (visible "+interaction.visible+", alpha "+interaction.alpha.toFixed(2)+")";
            return true;
        }
        private function freeze(node:DisplayObject):void {
            if (node is MovieClip) MovieClip(node).stop();
            var container:DisplayObjectContainer = node as DisplayObjectContainer;
            if (container != null) for (var i:int = 0; i < container.numChildren; ++i) freeze(container.getChildAt(i));
        }
        // Each hit restarts the hold, so the trail only drains once the damage stops.
        private function enemyTrail(percent:Number, target:Number):Number {
            var now:int = getTimer();
            if (target != trailTarget || percent >= trailPercent) {
                trailTarget = target;
                trailPercent = percent;
            } else if (percent < trailLast) trailHold = now+TRAIL_HOLD_MS;
            trailLast = percent;
            // A fixed drain time, so any chunk is gone about half a second after the last hit.
            var drained:Number = (now-trailHold)/TRAIL_DRAIN_MS;
            if (drained >= 1) trailPercent = percent;
            return drained <= 0 ? trailPercent : trailPercent-(trailPercent-percent)*drained;
        }
        private function lighter(color:uint):uint {
            var mixed:uint = 0;
            for (var shift:int = 0; shift <= 16; shift += 8) {
                var channel:uint = color >> shift & 0xFF;
                mixed |= uint(channel+(0xFF-channel)*0.45) << shift;
            }
            return mixed;
        }
        // FO76 art is cream #FFFFCB and gold #F5CB5B with semantic colours baked in, and FO76's default HUD
        // colour draws it as authored. Scaling each channel so cream lands on the HUD colour keeps gold
        // headers darker. Semantic parts (hostile red, danger, crit gold...) must never take this.
        // A colour transform, not a filter, as FO4 colours its own HUD: short-lived filtered clips churn
        // render textures (a render-thread crash while shooting). Channels floor at 2 so an inverse fits
        // Flash's 8.8 fixed-point multipliers (at most ~128).
        private function tint(clip:DisplayObject, color:uint, base:uint = 0xFFFFCB):void {
            var cxform:ColorTransform = clip.transform.colorTransform;
            cxform.redMultiplier = Math.max(2,(color >> 16) & 255)/Math.max(2,(base >> 16) & 255);
            cxform.greenMultiplier = Math.max(2,(color >> 8) & 255)/Math.max(2,(base >> 8) & 255);
            cxform.blueMultiplier = Math.max(2,color & 255)/Math.max(2,base & 255);
            clip.transform.colorTransform = cxform;
        }
        private function lighter(color:uint):uint {
            var red:uint = (color >> 16) & 255, green:uint = (color >> 8) & 255, blue:uint = color & 255;
            return ((red + (255-red)/2) << 16) | ((green + (255-green)/2) << 8) | (blue + (255-blue)/2);
        }
        private function hudColor(source:uint, color:uint):uint {
            var red:uint = Math.min(255,Math.round(((source >> 16) & 255)*((color >> 16) & 255)/255));
            var green:uint = Math.min(255,Math.round(((source >> 8) & 255)*((color >> 8) & 255)/255));
            var blue:uint = Math.min(255,Math.round((source & 255)*(color & 255)/203));
            return (red << 16) | (green << 8) | blue;
        }
        // The HUDMenu prompts (quick loot, crosshair, hold, rewards) keep a group filter, as before this
        // change: it fades each group as one layer at reduced HUD opacity.
        private function tintLayer(clip:DisplayObject, color:uint):void {
            clip.filters = [new ColorMatrixFilter([Math.max(2,(color >> 16) & 255)/255,0,0,0,0,
                0,Math.max(2,(color >> 8) & 255)/255,0,0,0,0,0,Math.max(2,color & 255)/203,0,0,0,0,0,1,0])];
        }
        private function shownColor(clip:DisplayObject, base:uint):Array {
            var cxform:ColorTransform = clip.transform.colorTransform;
            return [Math.round(cxform.redMultiplier*((base >> 16) & 255)),Math.round(cxform.greenMultiplier*((base >> 8) & 255)),
                Math.round(cxform.blueMultiplier*(base & 255))];
        }
        private function colorOf(clip:DisplayObject, base:uint = 0xFFFFCB):Array {
            var cxform:ColorTransform = clip.transform.colorTransform;
            return cxform.redMultiplier == 1 && cxform.greenMultiplier == 1 && cxform.blueMultiplier == 1 ? null : shownColor(clip,base);
        }
        private function follow(layer:DisplayObject, source:DisplayObject):void {
            var origin:Point = globalToLocal(source.localToGlobal(new Point(0,0)));
            var unitX:Point = globalToLocal(source.localToGlobal(new Point(1,0)));
            var unitY:Point = globalToLocal(source.localToGlobal(new Point(0,1)));
            layer.transform.matrix = new Matrix(unitX.x-origin.x,unitX.y-origin.y,unitY.x-origin.x,unitY.y-origin.y,origin.x,origin.y);
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
        private function text(node:DisplayObject, value:String):void {
            var target:TextField = field(node);
            if (target != null) target.text = value;
        }
        private function fittedText(node:DisplayObject, value:String):void {
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
        private function fill(meter:Object, percent:Number):Boolean {
            if (meter == null || meter.MeterBarInternal_mc == null || meter.MeterBarInternal_mc.Contents == null) return false;
            var bar:MovieClip = meter.MeterBarInternal_mc.Contents.Fill as MovieClip;
            if (bar == null) return false;
            // Idle FO4 meters (no enemy target, empty crit bar) report NaN.
            if (!isFinite(percent)) percent = 0;
            bar.gotoAndStop(1 + Math.floor(300 * Math.max(0, Math.min(1, percent))));
            if (bar.healthBarLeader != null) bar.healthBarLeader.visible = false;
            return true;
        }
        private function globalPoint(clip:Object):Point { return DisplayObject(clip).localToGlobal(new Point()); }
        private function pointData(clip:Object):Object { var at:Point = globalPoint(clip); return {x:at.x,y:at.y}; }
        // Maps a HUDMenu stage point into this movie's stage; Tales derives data.map from both viewports.
        private function place(at:Object, map:Object):Point {
            return new Point(Number(map.ax)*Number(at.x)+Number(map.bx),Number(map.ay)*Number(at.y)+Number(map.by));
        }
        private function anchor(clip:MovieClip, source:Array, at:Point, baseX:Number, baseY:Number, scale:Number):void {
            var point:Point = globalToLocal(at);
            clip.transform.matrix = new Matrix(source[0]*scale,source[1]*scale,source[2]*scale,source[3]*scale,
                point.x + (source[4]-baseX)*scale, point.y + (source[5]-baseY)*scale);
        }
        private function updateCondition(data:Object):Boolean {
            conditionGroup.visible = Boolean(data.condition) && Number(data.durability) >= 0;
            var value:Number = Math.max(0, Math.min(2, Number(data.durability)));
            if (cnd.MeterClip_mc == null) return false;
            if (conditionGroup.visible) updateWeaponIcon(data);
            cnd.MeterClip_mc.visible = value > 0;
            cnd.MeterClip_mc.gotoAndStop(value == 0 ? cnd.MeterClip_mc.totalFrames : Math.round(cnd.MeterClip_mc.totalFrames - (cnd.MeterClip_mc.totalFrames-2)*value/2));
            tint(cnd.MeterClip_mc,uint(data.color),GOLD);
            // FO76's pale inset marks the extra 100%. A darker HUD-coloured inset remains distinct
            // even when the user's saturated HUD colour would make both source fills identical.
            if (value > 1 && cnd.MeterClip_mc.numChildren > 1)
                cnd.MeterClip_mc.getChildAt(1).transform.colorTransform = new ColorTransform(
                    .5*245/255,.5*203/255,.5*91/203,1);
            if (weaponIcon != null) tint(weaponIcon,uint(data.color));
            return true;
        }
        /* BACUP_ICON_RESOLVER */
        private function updateWeaponIcon(data:Object):void {
            // Tales sends the equipped model's rendered silhouette when Dynamic Weapon Icons is on.
            var runs:Array = data.iconRuns as Array;
            if (runs != null && runs.length == 0) runs = null;
            var name:String = runs != null ? "DynamicIcon" : ResolveIcon(data);
            if (runs == null && B21_StatusSource.weapons[name] == null) name = "UnknownIcon";
            if (name == weaponName && runs == weaponRuns) return;
            if (weaponIcon != null) conditionGroup.removeChild(weaponIcon);
            if (runs != null) {
                // Cream, like the source glyphs, so the shared HUD tint lands it on HUD Color.
                weaponIcon = new MovieClip();
                B21InventoryIcon.draw(weaponIcon,{B21IconRuns:runs},0,0,B21InventoryIcon.GRID,B21InventoryIcon.GRID,CREAM);
            } else {
                var type:Class = getDefinitionByName(B21_StatusSource.weapons[name]) as Class;
                weaponIcon = new type() as MovieClip;
                freeze(weaponIcon);
            }
            weaponRuns = runs;
            var bounds:Rectangle = weaponIcon.getBounds(weaponIcon);
            weaponIcon.scaleX = weaponIcon.scaleY = 28 / Math.max(1,bounds.width,bounds.height);
            weaponIcon.x = -36 - bounds.x * weaponIcon.scaleX;
            weaponIcon.y = (12 - bounds.height * weaponIcon.scaleY)/2 - bounds.y * weaponIcon.scaleY;
            weaponIcon.filters = [new DropShadowFilter(1,45,0,0.8,2,2,1,1,false,false,false)];
            conditionGroup.addChild(weaponIcon);
            weaponName = name;
        }
        private function effectCard(index:int, name:String, stacks:int, negative:Boolean, remaining:Number, color:uint):void {
            var card:MovieClip;
            if (index < icons.numChildren) card = icons.getChildAt(index) as MovieClip;
            else { card = new B21_StatusEffect(); icons.addChild(card); }
            card.gotoAndStop(negative ? "negative" : "positive");
            freeze(card);
            card.Icon_mc.gotoAndStop(name);
            card.Stack_mc.visible = stacks > 0;
            if (stacks > 0) {
                text(card.Stack_mc.StackAmount_tf,String(stacks));
                card.Stack_mc.BG_mc.width = 19 + 12*(String(stacks).length-1);
                card.Stack_mc.StackAmount_tf.width = card.Stack_mc.BG_mc.width-2;
                card.Stack_mc.StackAmount_tf.x = card.Stack_mc.BG_mc.x+1;
            }
            // FO76 drains the tile's fill from the top over the remaining time; untimed effects park it empty.
            var fill:MovieClip = card.FillInternal_mc == null ? null : card.FillInternal_mc.Fill_mc as MovieClip;
            if (fill != null) fill.y = remaining >= 0 ? 14.5 - 29*Math.min(1,remaining) : 14.5;
            card.x = -35 - index * 39;
            card.y = 0;
            card.visible = true;
            tint(card,negative ? CREAM : color);
        }
        private function updateEffects(frames:Array, inArmor:Boolean, data:Object):void {
            var widget:DisplayObject = rightGroup.HUDActiveEffectsWidget_mc;
            follow(effectLayer,widget);
            effectLayer.visible = rightGroup.visible && widget.visible;
            var color:uint = uint(data.color);
            var count:int = 0;
            if (Number(data.bulletStorm) > 0) { effectCard(count,"OnslaughtIcon",int(data.bulletStorm),false,-1,color); ++count; }
            if (Number(data.onslaught) > 0) { effectCard(count,"OnslaughtIcon",int(data.onslaught),false,-1,color); ++count; }
            // Tales supplies the FO76 keyword icons (mutations, diseases...); FO4's widget still adds its
            // own consumable categories that no keyword covers.
            var shown:Object = {};
            for each (var effect:Object in data.effects as Array) {
                var icon:String = String(effect.icon);
                if (count >= 8 || shown[icon]) continue;
                shown[icon] = true;
                effectCard(count,icon,0,Boolean(effect.negative),Number(effect.remaining),color);
                ++count;
            }
            for each (var replaced:String in data.replacedEffectCategories as Array) shown[replaced] = true;
            for (var i:int = 0; i < frames.length && count < 8; ++i) {
                var name:String = iconNames[int(frames[i])];
                if (name == null || shown[name]) continue;
                shown[name] = true;
                effectCard(count,name,0,false,-1,color);
                ++count;
            }
            while (icons.numChildren > count) icons.removeChildAt(icons.numChildren-1);
            icons.x = inArmor ? 50 : 0;
            icons.y = inArmor ? 15 : 0;
        }
        private function hasLabel(clip:MovieClip, label:String):Boolean {
            for each (var frame:Object in clip.currentLabels) if (frame.name == label) return true;
            return false;
        }
        private function markerSymbol(marker:DisplayObject):String {
            var name:String = getQualifiedClassName(marker).split("::").pop();
            if (marker.hasOwnProperty("B21SourceSymbol")) name = Object(marker).B21SourceSymbol;
            var mapped:String = B21_StatusSource.markers[name];
            return mapped == null ? B21_StatusSource.markers["LandmarkMarker"] : mapped;
        }
        private function state(node:DisplayObject):Object {
            if (node is TextField) return {n:node.name,t:TextField(node).text};
            var clip:MovieClip = node as MovieClip;
            if (clip == null) return null;
            var children:Array = [];
            for (var i:int = 0; i < clip.numChildren; ++i) {
                var part:DisplayObject = clip.getChildAt(i);
                var child:Object = state(part);
                // Location frames are blank; the engine attaches the icon at runtime.
                if (child != null && clip.currentLabel == "Location" && part is MovieClip) {
                    child.s = markerSymbol(part);
                    child.x = part.x; child.y = part.y;
                    child.sx = part.scaleX; child.sy = part.scaleY; child.r = part.rotation;
                }
                if (child != null) children.push(child);
            }
            return {n:clip.name,l:clip.currentLabel,f:clip.currentFrame,v:clip.visible,a:clip.alpha,c:children};
        }
        private function applyState(source:Object, target:DisplayObject):void {
            if (source.t != null) {
                if (target is TextField) TextField(target).text = String(source.t);
                return;
            }
            var clip:MovieClip = target as MovieClip;
            if (clip == null) return;
            // FO4 attaches a location icon with an empty instance name. GFx keeps it empty (Ruffle invents
            // instanceN), so getChildByName never finds the clone's copy and one was added every frame.
            // A Location clone pairs its icons with the source's by index instead; FO76's Location frame
            // places nothing, so these icons are its only children.
            var icons:Array = [];
            if (source.l == "Location") for each (var entry:Object in source.c) if (entry.s != null) icons.push(entry);
            if (clip.currentLabel == "Location")
                while (clip.numChildren > icons.length) clip.removeChildAt(clip.numChildren-1);
            var found:Boolean = false;
            if (source.l != null) for each (var frame:Object in clip.currentLabels) if (frame.name == source.l) found = true;
            clip.gotoAndStop(found ? String(source.l) : Math.min(int(source.f),clip.totalFrames));
            clip.visible = Boolean(source.v);
            clip.alpha = Number(source.a);
            for (var i:int = 0; i < icons.length; ++i) {
                var wanted:Object = icons[i];
                var icon:MovieClip = i < clip.numChildren ? clip.getChildAt(i) as MovieClip : null;
                if (icon == null || getQualifiedClassName(icon) != wanted.s) {
                    if (icon != null) clip.removeChildAt(i);
                    var type:Class = getDefinitionByName(String(wanted.s)) as Class;
                    icon = new type();
                    clip.addChildAt(icon,i);
                    freeze(icon);
                }
                icon.x = Number(wanted.x); icon.y = Number(wanted.y);
                icon.scaleX = Number(wanted.sx); icon.scaleY = Number(wanted.sy); icon.rotation = Number(wanted.r);
                applyState(wanted,icon);
            }
            for each (var child:Object in source.c) {
                if (child.s == null) {
                    var other:DisplayObject = clip.getChildByName(String(child.n));
                    if (other != null) applyState(child,other);
                }
            }
        }
        private function captureMarkers(source:DisplayObjectContainer):Array {
            var rows:Array = [];
            for (var i:int = 0; i < source.numChildren; ++i) {
                var marker:DisplayObject = source.getChildAt(i);
                if (!(marker is MovieClip)) continue;
                // FO4 locations FO76 has no icon for (Nuka-World's, POIMarker) draw as FO76's landmark
                // instead of dropping the whole HUD back to vanilla.
                var mapped:String = markerSymbol(marker);
                if (mapped == null) {
                    if (marker.visible && marker.width > 0 && marker.height > 0) return null;
                    continue;
                }
                rows.push({symbol:mapped,x:marker.x,y:marker.y,state:state(marker)});
            }
            return rows;
        }
        private function updateMarkers(rows:Array, holder:MovieClip, color:uint):Boolean {
            if (rows == null) return false;
            var counts:Array = [0,0,0,0];
            for each (var row:Object in rows) {
                // FO76 bakes the Enemy/EnemyTargeted frames red; they skip the HUD colour. Tales rides allies
                // and neutrals on FO4's enemy markers, naming them; they draw FO76's plain Neutral dot, tinted.
                var layer:int = 0;
                if (row.state.l == "Enemy" && (row.state.n == "B21:Ally" || row.state.n == "B21:Neutral")) {
                    layer = row.state.n == "B21:Ally" ? ALLY_LAYER : NEUTRAL_LAYER;
                    row.state.l = "Neutral";
                } else if (row.state.l == "Enemy" || row.state.l == "EnemyTargeted") layer = HOSTILE_LAYER;
                var target:MovieClip = holder.getChildAt(layer) as MovieClip;
                var count:int = counts[layer];
                var clone:MovieClip = count < target.numChildren ? target.getChildAt(count) as MovieClip : null;
                if (clone == null || getQualifiedClassName(clone) != row.symbol) {
                    if (clone != null) target.removeChildAt(count);
                    var type:Class = getDefinitionByName(String(row.symbol)) as Class;
                    clone = new type();
                    target.addChildAt(clone,count);
                    freeze(clone);
                }
                applyState(row.state,clone);
                if (row.state.l == "Location" || clone.currentLabel == "Location") noteCompass(row,clone);
                // FO4's holders share one scale; FO76 scales the quest holder 1.66 and the other 1.33, so
                // markers at one bearing line up only on the quest holder's scale (its ±252 matches the rule).
                clone.x = Number(row.x)*compassGroup.QuestMarkerHolder_mc.scaleX/holder.scaleX;
                clone.y = Number(row.y);
                ++markerCount;
                ++counts[layer];
            }
            for (layer = 0; layer <= HOSTILE_LAYER; ++layer) {
                var clips:MovieClip = holder.getChildAt(layer) as MovieClip;
                while (clips.numChildren > counts[layer]) clips.removeChildAt(clips.numChildren-1);
            }
            tint(holder.getChildAt(0),color);
            tint(holder.getChildAt(ALLY_LAYER),lighter(color));
            tint(holder.getChildAt(NEUTRAL_LAYER),NEUTRAL);
            return true;
        }
        private function noteCompass(row:Object, clone:MovieClip):void {
            var listed:Array = [];
            for each (var entry:Object in row.state.c) listed.push(String(entry.n) + "=" + String(entry.s) + "/" + String(entry.l));
            var held:Array = [];
            for (var i:int = 0; i < clone.numChildren; ++i) {
                var part:DisplayObject = clone.getChildAt(i);
                var clip:MovieClip = part as MovieClip;
                held.push(part.name + "=" + getQualifiedClassName(part) + "/" + (clip == null ? "" : String(clip.currentLabel)) +
                          (part.visible ? "" : "(hidden)") + "@" + Math.round(part.alpha*100));
            }
            if (held.length != listed.length || clone.currentLabel != row.state.l)
                compassIssues.push(String(row.state.l) + "#" + String(row.state.f) + "->" + String(clone.currentLabel) + "#" +
                                   clone.currentFrame + " src[" + listed.join(",") + "] clone[" + held.join(",") + "]");
            if (Boolean(row.state.v)) {
                compassLocations.push(Math.round(Number(row.x)) + ":" + listed.join(","));
                compassBearings.push(Number(row.x));
            }
        }
        private function compassIssue():String {
            var pile:Boolean = false;
            for (var a:int = 0; a < compassBearings.length; ++a)
                for (var b:int = 0; b < compassBearings.length; ++b)
                    if (a != b && Math.abs(Number(compassBearings[a])-Number(compassBearings[b])) < 4) pile = true;
            if (compassIssues.length == 0 && !pile) return "";
            return (compassIssues.join(" | ") + " || visible " + compassLocations.join(" ")).substr(0,1500);
        }
        private function updateArea(within:Boolean, inArmor:Boolean):void {
            var normal:MovieClip = compassGroup.AreaQuest_WithinClip_mc;
            var armor:MovieClip = compassGroup.AreaQuest_WithinClipPA_mc;
            normal.visible = !inArmor;
            armor.visible = inArmor;
            if (within != insideArea || areaArmor != inArmor) {
                var clip:MovieClip = inArmor ? armor : normal;
                if (within) clip.gotoAndPlay("rollOn");
                else if (insideArea) clip.gotoAndPlay("rollOut");
                else clip.gotoAndStop(1);
            }
            insideArea = within;
            areaArmor = inArmor;
        }
        private function updateNeed(clip:MovieClip, amount:Number, icon:String, delta:Number, alwaysShow:Boolean):void {
            var state:Object = needStates[clip.name];
            if (state == null) needStates[clip.name] = state = {last:NaN,timer:0,alpha:0};
            if (!(isFinite(amount) && amount >= 0)) {
                state.last = NaN;
                state.timer = state.alpha = 0;
                clip.visible = false;
                return;
            }
            // FO76: always up below 20%; otherwise a 3% change or any rise shows it for 10 s, then it
            // fades out over its 0.2 s rollOff.
            if (alwaysShow || amount < .2) state.timer = -1;
            else if (isNaN(state.last) || Math.abs(amount-state.last) >= .03 || amount > state.last || state.timer < 0) state.timer = 10;
            else state.timer = Math.max(0,state.timer-delta);
            state.last = amount;
            var target:Number = state.timer != 0 ? 1 : 0;
            state.alpha = delta > 0 ? (target > state.alpha ? Math.min(target,state.alpha+delta/.2) : Math.max(target,state.alpha-delta/.2)) : target;
            clip.visible = state.alpha > 0;
            if (!clip.visible) return;
            clip.gotoAndStop(7);
            clip.GhostMeter_mc.visible = false;
            clip.survivalMeterIcon_mc.gotoAndStop(icon);
            clip.Meter_mc.gotoAndStop(Math.max(1,Math.ceil(Math.min(1,amount)*clip.Meter_mc.totalFrames)));
            // Fade the parts: scripting the meter's own alpha would stop the powerArmorHUD frame moving it.
            // FO76 keeps the Food/Thirst label at alpha 0 on every frame.
            for (var part:int = 0; part < clip.numChildren; ++part)
                clip.getChildAt(part).alpha = clip.getChildAt(part).name.indexOf("MeterLabel") >= 0 ? 0 : state.alpha;
        }
        // FO76 rolls the feral meter on once and drops it straight to "off"; empty pulses once, then rests.
        private function updateFeral(value:Number):void {
            var feral:MovieClip = rightGroup.FeralMeter_mc;
            var shown:Boolean = value >= 0;
            if (shown != feralShown) {
                feralShown = shown;
                if (shown) feral.gotoAndPlay("rollOn"); else feral.gotoAndStop("off");
            }
            feral.visible = shown;
            if (!shown) return;
            var inner:MovieClip = feral.FeralMeterInternal_mc;
            if (value > 0) inner.gotoAndStop(Math.max(1,int((1-value)*100)));
            else if (inner.currentLabel != "empty" && inner.currentLabel != "emptyAnim") inner.gotoAndPlay("emptyAnim");
        }
        // FO76 rolls the green outline on, slides the stripes to 312*p over 0.15 s on every change, and
        // after sliding back to 0 rolls the outline off before hiding.
        private function updateGlow(value:Number, delta:Number):void {
            var meter:MovieClip = hp.GlowMeter_mc;
            var stripes:MovieClip = meter.Meter_mc.Fill_mc;
            var outline:MovieClip = meter.Glow_mc;
            var target:Number = isFinite(value) && value > 0 ? 312*Math.min(1,value) : 0;
            if (target > 0 && (!glowShown || outline.currentFrame >= 50)) {
                if (!glowShown) stripes.x = 0;
                glowShown = true;
                outline.gotoAndPlay("rollOn");
            }
            meter.visible = glowShown;
            if (!glowShown) return;
            if (target != glowTo) { glowFrom = stripes.x; glowTo = target; glowTime = 0; }
            glowTime += delta > 0 ? delta : 0;
            stripes.x = glowFrom+(glowTo-glowFrom)*Math.min(1,glowTime/.15);
            if (target > 0 || glowTime < .15) return;
            if (outline.currentFrame < 50) outline.gotoAndPlay("rollOff");
            else if (outline.currentFrame >= outline.totalFrames) meter.visible = glowShown = false;
        }
        private function inheritVisibility(source:DisplayObject, target:DisplayObject, host:DisplayObject):void {
            show(target,chain(source,host));
        }
        private function chain(source:DisplayObject, host:Object):Object {
            var shown:Boolean = source.visible;
            var opacity:Number = 1;
            for (var node:DisplayObject = source; node != null && node != host; node = node.parent) {
                shown = shown && node.visible;
                opacity *= node.alpha;
            }
            return {v:shown,a:opacity};
        }
        private function show(target:DisplayObject, source:Object):void {
            target.visible = Boolean(source.v);
            target.alpha = Number(source.a);
        }
        private function clipState(clip:Object, host:Object):Object {
            var captured:Object = state(DisplayObject(clip));
            var shown:Object = chain(DisplayObject(clip),host);
            captured.shown = shown.v;
            captured.opacity = shown.a;
            return captured;
        }
        private function applyClip(source:Object, target:MovieClip):void {
            applyState(source,target);
            target.visible = Boolean(source.shown);
            target.alpha = Number(source.opacity);
        }
        private function stars(holder:MovieClip, count:int, filled:int, spacing:Number):void {
            count = Math.max(0,Math.min(5,count));
            while (holder.numChildren > count) holder.removeChildAt(holder.numChildren-1);
            while (holder.numChildren < count) holder.addChild(new B21_StatusStar());
            for (var i:int = 0; i < count; ++i) {
                var star:MovieClip = holder.getChildAt(i) as MovieClip;
                star.gotoAndStop(i < filled ? "full" : "empty");
                star.x = i*spacing;
            }
        }
        private function captureThreats(source:DisplayObjectContainer, host:Object):Object {
            var rows:Array = [];
            for (var i:int = 0; i < source.numChildren; ++i) {
                var warning:MovieClip = source.getChildAt(i) as MovieClip;
                if (warning == null || !warning.visible) continue;
                if (warning.GrenadeIcon_mc == null || warning.MineIcon_mc == null || warning.CarIcon_mc == null || warning.Arrow_mc == null) return null;
                var angles:Array = [];
                for each (var name:String in threatIcons) angles.push(warning[name].rotation);
                rows.push({state:state(warning),x:warning.x,y:warning.y,rotation:warning.rotation,
                           scaleX:warning.scaleX,scaleY:warning.scaleY,icons:angles});
            }
            var captured:Object = chain(source,host);
            captured.rows = rows;
            return captured;
        }
        private function updateExplosives(threats:Object, hudScale:Number):void {
            var count:int = 0;
            for each (var row:Object in threats.rows) {
                var clone:MovieClip;
                if (count < explosives.numChildren) clone = explosives.getChildAt(count) as MovieClip;
                else { clone = new B21_StatusExplosive(); explosives.addChild(clone); freeze(clone); }
                applyState(row.state,clone);
                clone.x = Number(row.x)/hudScale;
                clone.y = Number(row.y)/hudScale;
                clone.rotation = Number(row.rotation);
                clone.scaleX = Number(row.scaleX);
                clone.scaleY = Number(row.scaleY);
                for (var i:int = 0; i < threatIcons.length; ++i) clone[threatIcons[i]].rotation = Number(row.icons[i]);
                ++count;
            }
            while (explosives.numChildren > count) explosives.removeChildAt(explosives.numChildren-1);
            show(explosives,threats);
        }
        // The engine attaches FO4's in-world markers to FloatingQuestMarkerBase. Tales names those FO76 has
        // art for "B21:<FloatingTarget frame>[:<Internal_mc frame>]" (QuestAreaMarkerHooks); others stay FO4's.
        private function captureFloating(host:Object):Array {
            var base:DisplayObjectContainer = DisplayObjectContainer(host).getChildByName("FloatingQuestMarkerBase") as DisplayObjectContainer;
            if (base == null) return null;
            var rows:Array = [];
            for (var i:int = 0; i < base.numChildren; ++i) {
                var marker:DisplayObject = base.getChildAt(i);
                var look:Array = marker.name.split(":");
                if (look.length < 2 || look[0] != "B21") continue;
                var shown:Object = chain(marker,host);
                if (!shown.v || shown.a <= 0) continue;
                var at:Point = globalPoint(marker);
                rows.push({frame:look[1],type:look.length > 2 ? look[2] : null,x:at.x,y:at.y,a:shown.a});
            }
            return rows;
        }
        private function updateFloating(rows:Array, map:Object, scale:Number, color:uint):void {
            var tinted:MovieClip = floatingGroup.getChildAt(0) as MovieClip;
            var hostile:MovieClip = floatingGroup.getChildAt(1) as MovieClip;
            var tintedCount:int = 0;
            var hostileCount:int = 0;
            if (rows == null) rows = [];
            for each (var row:Object in rows) {
                // FO76 bakes EnemyTargeted red; like the compass's enemy markers it skips the HUD colour.
                var red:Boolean = row.frame == "EnemyTargeted";
                var layer:MovieClip = red ? hostile : tinted;
                var index:int = red ? hostileCount : tintedCount;
                if (red) ++hostileCount; else ++tintedCount;
                var clone:MovieClip = index < layer.numChildren ? layer.getChildAt(index) as MovieClip : null;
                if (clone == null) {
                    clone = new B21_StatusFloatingTarget();
                    layer.addChild(clone);
                    freeze(clone);
                }
                clone.gotoAndStop(hasLabel(clone,String(row.frame)) ? String(row.frame) : "ActiveQuest");
                var inner:MovieClip = clone.getChildByName("Internal_mc") as MovieClip;
                if (inner != null && row.type != null && hasLabel(inner,String(row.type))) inner.gotoAndStop(String(row.type));
                var at:Point = floatingGroup.globalToLocal(place(row,map));
                clone.x = at.x;
                clone.y = at.y;
                clone.scaleX = clone.scaleY = scale;
                clone.alpha = Number(row.a);
            }
            while (tinted.numChildren > tintedCount) tinted.removeChildAt(tinted.numChildren-1);
            while (hostile.numChildren > hostileCount) hostile.removeChildAt(hostile.numChildren-1);
            tint(tinted,color);
        }
        private function captureCombat(host:Object):Object {
            if (host.TopCenterGroup_mc == null || host.CenterGroup_mc == null) return null;
            var top:Object = host.TopCenterGroup_mc;
            var bottom:Object = host.BottomCenterGroup_mc;
            var right:Object = host.RightMeters_mc;
            var rads:Object = host.LeftMeters_mc.RadsMeter_mc;
            var ammo:Object = right.AmmoCount_mc;
            var target:Object = top.EnemyHealthMeter_mc;
            var sneak:Object = top.StealthMeter_mc;
            var crit:Object = bottom.CritMeter_mc;
            var threats:DisplayObjectContainer = host.CenterGroup_mc.ExplosiveIndicatorBase_mc as DisplayObjectContainer;
            if (rads == null || ammo == null || target == null || sneak == null || crit == null || threats == null ||
                rads.RadsNumber_tf == null || rads.RADS_tf == null || ammo.ClipCount_tf == null || ammo.ReserveCount_tf == null ||
                target.MeterBar_mc == null ||
                target.DisplayText_tf == null || target.SkullIcon_mc == null || target.LegendaryIcon_mc == null ||
                sneak.StealthTextInstance == null || crit.MeterBar_mc == null || crit.DisplayText_tf == null || crit.CritMeterStars_mc == null) return null;
            var combat:Object = {top:pointData(top),threatsAt:pointData(threats),
                rads:clipState(rads,host),ammo:clipState(ammo,host)};
            var enemyState:Object = chain(DisplayObject(target),host);
            enemyState.percent = Number(target.MeterBar_mc.Percent);
            enemyState.optional = target.Optional_mc != null && Boolean(target.Optional_mc.visible);
            if (enemyState.optional) enemyState.optionalPercent = Number(target.Optional_mc.Percent);
            enemyState.text = String(target.DisplayText_tf.text);
            enemyState.skull = Boolean(target.SkullIcon_mc.visible);
            enemyState.legendary = Boolean(target.LegendaryIcon_mc.visible);
            combat.enemy = enemyState;
            var stealthState:Object = chain(DisplayObject(sneak),host);
            stealthState.text = String(sneak.StealthTextInstance.text);
            stealthState.distance = Number(sneak.LastPercent);
            combat.stealth = stealthState;
            var critState:Object = chain(DisplayObject(crit),host);
            critState.frame = crit.currentFrame;
            critState.bar = crit.MeterBar_mc is MovieClip ? crit.MeterBar_mc.currentFrame : 0;
            critState.text = String(crit.DisplayText_tf.text);
            critState.percent = Number(crit.MeterBar_mc.Percent);
            critState.stars = [];
            for (var i:int = 0; i < crit.CritMeterStars_mc.numChildren; ++i)
                critState.stars.push(MovieClip(crit.CritMeterStars_mc.getChildAt(i)).currentFrame);
            combat.critical = critState;
            var heat:Object = right.OverheatMeter_mc;
            if (heat != null && heat.MeterBar_mc != null && Boolean(heat.visible)) {
                combat.heat = chain(DisplayObject(heat),host);
                combat.heat.percent = Number(heat.MeterBar_mc.Percent);
            }
            combat.threats = captureThreats(threats,host);
            return combat.threats == null ? null : combat;
        }
        private function updateCombat(data:Object, snap:Object, map:Object, inArmor:Boolean, scale:Number, hudScale:Number):Boolean {
            var combat:Object = snap.combat;
            applyClip(combat.rads,healthGroup.RadsMeter_mc);
            // FO4 fades its counter out a few seconds after the last shot but keeps its text current;
            // FO76 keeps it up while an ammo weapon is drawn.
            applyClip(combat.ammo,rightGroup.AmmoCount_mc);
            rightGroup.AmmoCount_mc.visible = Boolean(data.ammoDrawn) && !inArmor;
            rightGroup.AmmoCount_mc.alpha = 1;
            // FO4 shows its explosive counter only for 10 s after a change; Tales reads the equipped
            // grenade or mine, shown like the ammo counter while a weapon is drawn.
            var grenade:MovieClip = rightGroup.ExplosiveAmmoCount_mc;
            var thrown:int = isFinite(Number(data.explosiveCount)) ? int(data.explosiveCount) : -1;
            if (thrown >= 0) {
                grenade.TypeIcon_mc.gotoAndStop(data.explosiveMine ? 2 : 1);
                grenade.AvailableCount_tf.text = String(Math.min(99,thrown));
            }
            grenade.visible = thrown > 0;
            grenade.alpha = 1;
            anchor(enemy,B21_StatusSource.layout.enemy,place(combat.top,map),960,54,scale);
            anchor(stealth,B21_StatusSource.layout.stealth,place(combat.top,map),960,54,scale);
            anchor(critical,B21_StatusSource.layout.critical,place(snap.bottom,map),960,1026,scale);
            anchor(encounters,B21_StatusSource.layout.encounter,place(snap.bottom,map),960,1026,scale);
            anchor(core,B21_StatusSource.layout.core,place(snap.left,map),96,1026,scale);
            anchor(explosives,[1,0,0,1,960,540],place(combat.threatsAt,map),960,540,scale);
            var target:Object = combat.enemy;
            show(enemy,target);
            var hostile:Boolean = !data.enemyFriendly && data.enemyHostile != false;
            var targetDetails:Object = data.nameplates == null ? null : data.nameplates.target;
            if (targetDetails != null && Number(targetDetails.id) != Number(data.enemyId)) targetDetails = null;
            var teamColor:Boolean = !hostile && targetDetails != null && targetDetails.color != null;
            enemy.gotoAndStop(data.enemyFriendly ? "Friendly" : hostile ? "Hostile" : "Nonhostile");
            freeze(enemy);
            var percent:Number = Number(target.percent);
            if (!fill(enemy.MeterBar_mc,percent) || !fill(enemy.MeterBarEnemy_mc,percent) || !fill(enemy.MeterBarFriendly_mc,percent)) return false;
            // The three bars share one spot. Showing only FO76's red one left enemies with no bar in game,
            // so all stay up as before and each is coloured: whichever draws reads red for a hostile target
            // and HUD colour otherwise.
            var barColor:uint = teamColor ? uint(targetDetails.color) : hostile ? HOSTILE : uint(data.color);
            tint(enemy.MeterBar_mc,barColor);
            tint(enemy.MeterBarFriendly_mc,barColor);
            tint(enemy.MeterBarEnemy_mc,barColor,HOSTILE);
            var trail:Number = enemyTrail(isFinite(percent) ? percent : 0,Number(data.enemyId));
            enemy.Optional_mc.visible = Boolean(target.optional) || trail > percent+0.002;
            if (enemy.Optional_mc.visible) {
                if (!fill(enemy.Optional_mc,Boolean(target.optional) ? Number(target.optionalPercent) : trail)) return false;
                // FO76's pulse, at 30 fps; freeze stopped its timeline.
                var pulse:MovieClip = enemy.Optional_mc.MeterBarInternal_mc;
                pulse.gotoAndStop(1+int(getTimer()*30/1000)%pulse.totalFrames);
                tint(enemy.Optional_mc,lighter(barColor));
            }
            fittedText(enemy.DisplayText_mc,String(target.text));
            // The Hostile frame's own filter turns the cream name red; other standings take the HUD colour.
            field(enemy.DisplayText_mc).textColor = hudColor(enemyNameColor,hostile ? CREAM : barColor);
            // FO76's frame scripts colour the level box per standing; the timeline only carries hostile red.
            var standingColor:uint = teamColor ? barColor : data.enemyFriendly ? 0xF7CC5D : hostile ? 0xF5765E : uint(data.color);
            tint(enemy.LevelText_mc,standingColor);
            enemy.LevelText_mc.visible = Number(data.enemyLevel) >= 0;
            if (enemy.LevelText_mc.visible) {
                enemy.LevelText_mc.gotoAndStop(data.enemyBoss ? "star" : "square");
                fittedText(enemy.LevelText_mc,String(data.enemyLevel));
            }
            var rank:int = int(data.enemyRank);
            if (rank <= 0 && Boolean(target.legendary)) rank = 1;
            var nameField:TextField = field(enemy.DisplayText_mc);
            var nameBounds:Rectangle = nameField.getBounds(enemy);
            var nameEnd:Number = nameBounds.x+nameBounds.width/2+nameField.textWidth/2*nameBounds.width/nameField.width;
            stars(enemyStars,rank,rank,22);
            // Tales' legendary stars are cream crit-meter art; like the name they take the target's colour
            // (the old FO4-meter row drew them in FO4's warning colour).
            tint(enemyStars,standingColor);
            enemyStars.x = nameEnd+12;
            enemyStars.y = -40;
            // FO4's skull marks a target far above the player's level and follows the name and its
            // stars; FO76's plain skull stands in for it. Bosses keep FO76's star level box.
            var skull:MovieClip = enemy.EncounterHolder_mc;
            skull.visible = Boolean(target.skull);
            if (skull.visible) {
                skull.gotoAndStop("Skull");
                skull.Encounter_mc.gotoAndStop("Easy");
                tint(skull,teamColor ? barColor : CREAM);
                skull.x = skull.y = 0;
                var art:Rectangle = skull.getBounds(enemy);
                skull.x = (rank > 0 ? enemyStars.getBounds(enemy).right : nameEnd)+6-art.x;
                skull.y = enemyStars.y-(art.y+art.height/2);
            }
            var sneak:Object = combat.stealth;
            targetReadout.update(targetDetails,standingColor);
            var healthBounds:Rectangle = enemy.HealthBarFrame_mc.getBounds(enemy);
            targetReadout.x = healthBounds.x+healthBounds.width/2;
            show(stealth,sneak);
            stealth.gotoAndStop(5);
            // FO76 rests on cream brackets for hidden/detected and red ones (own instances) for caution/danger.
            var mode:int = stealthMode(String(sneak.text),data.stealthModes as Array);
            var inner:MovieClip = stealth.Internal_mc;
            inner.gotoAndStop(mode >= 2 ? 24 : 14);
            var label:MovieClip = inner.stealthTextStates;
            label.gotoAndStop(String(["hidden","detected","caution","danger"][mode]));
            freeze(label);
            text(label,String(sneak.text));
            var distance:Number = Number(sneak.distance);
            // FO4 leaves LastPercent undefined until the player first sneaks.
            if (!isFinite(distance)) distance = 0;
            var bracketLeft:DisplayObject = inner.getChildByName("BracketLeftInstance");
            bracketLeft.x = -75-distance-bracketLeft.width;
            inner.getChildByName("BracketRightInstance").x = 75+distance;
            tint(stealth,mode >= 2 ? CREAM : uint(data.color));
            targetReadout.y = Math.max(healthBounds.bottom+8,stealth.visible ? stealth.getBounds(enemy).bottom+14 : 0);
            var crit:Object = combat.critical;
            show(critical,crit);
            critical.gotoAndStop(Math.min(int(crit.frame),critical.totalFrames));
            if (int(crit.bar) > 0) critical.MeterBar_mc.gotoAndStop(Math.min(int(crit.bar),critical.MeterBar_mc.totalFrames));
            text(critical.DisplayText_mc,String(crit.text));
            if (!fill(critical.MeterBar_mc,Number(crit.percent))) return false;
            var banks:Array = crit.stars as Array;
            stars(critical.CritMeterStars_mc,banks.length,0,22);
            for (var i:int = 0; i < critical.CritMeterStars_mc.numChildren; ++i)
                MovieClip(critical.CritMeterStars_mc.getChildAt(i)).gotoAndStop(Math.min(2,int(banks[i])));
            var rows:Array = data.encounters as Array;
            encounters.visible = rows != null && rows.length > 0;
            for (i = 0; i < 3; ++i) {
                var encounter:MovieClip = encounters["EncounterHealthMeter"+(i+1)+"_mc"];
                encounter.visible = rows != null && i < rows.length;
                if (!encounter.visible) continue;
                encounter.gotoAndStop("Hostile");
                freeze(encounter);
                if (!fill(encounter.MeterBar_mc,Number(rows[i].health))) return false;
                fittedText(encounter.DisplayText_mc,String(rows[i].name).toUpperCase());
                encounter.EncounterHolder_mc.gotoAndStop("Skull");
                encounter.EncounterHolder_mc.Encounter_mc.gotoAndStop("Difficult");
                encounter.EncounterHolder_mc.Encounter_mc.BossIcon_mc.visible = true;
            }
            core.visible = Boolean(data.inPowerArmor) && !inArmor && Number(data.corePercent) >= 0;
            if (core.visible) {
                core.gotoAndStop(7);
                core.survivalMeterIcon_mc.gotoAndStop("corePositive");
                core.Meter_mc.gotoAndStop(Math.max(1,Math.ceil(Math.min(1,Number(data.corePercent))*500)));
                text(core.CoreCount_mc,String(data.coreCount));
            }
            var heat:Object = combat.heat;
            rightGroup.OverheatMeter_mc.visible = heat != null;
            if (heat != null) {
                show(rightGroup.OverheatMeter_mc,heat);
                if (!fill(rightGroup.OverheatMeter_mc.MeterBar_mc,Number(heat.percent))) return false;
            }
            updateExplosives(combat.threats,hudScale);
            // Encounter bars and danger arrows keep FO76's baked red and white.
            tint(critical,uint(data.color));
            tint(core,uint(data.color));
            return true;
        }
        // FO4's stealth meter only gets its text; Tales sends the sSneakHidden/Detected/Caution/Danger GMSTs.
        private function stealthMode(value:String, modes:Array):int {
            value = value.toUpperCase();
            for (var i:int = modes == null ? -1 : modes.length-1; i >= 0; --i) {
                var mode:String = String(modes[i]).toUpperCase();
                if (mode != "" && value.indexOf(mode) >= 0) return i;
            }
            return 0;
        }
        private function updateExperience(data:Object):void {
            if (data == null || !data.visible) { experienceGroup.visible = false; return; }
            if (xpSequence != uint(data.sequence)) {
                xpSequence = uint(data.sequence);
                xpElapsed = 0;
                xpLevel = Boolean(data.levelUp);
                xp.gotoAndStop(xpLevel ? "levelup" : "xp");
                freeze(xp);
                // FO76's meters carry no fill motion; its native side moves them. The ghost bar marks the
                // new total at once and the solid bar eases up to it (to full on a level-up).
                xpFrom = Number(data.percentStart);
                xpTo = xpLevel ? 1 : Number(data.percentEnd);
                fill(xp.Optional_mc, xpTo);
                xp.Optional_mc.alpha = 0.5;
                // The fills are gold too; moved onto cream they match the HP/AP bars.
                for each (var bar:Object in [xp.LevelUPBar, xp.Optional_mc]) tint(bar.MeterBarInternal_mc.Contents.Fill, CREAM, GOLD);
                var number:String = String(xpLevel ? data.endLevel : data.startLevel);
                while(number.length < 4) number = "0" + number;
                text(xp.CurrentLevelField, number);
                text(xp.NumberText, String(Math.round(Number(data.xpAdded))));
                xp.NumberText.visible = !xpLevel;
                text(xp.PlusSign, "+");
                text(xp.xptext, "XP");
                levelUp.gotoAndStop(xpLevel ? "On" : "Off");
                text(levelUp.LevelUpText, "$LEVEL UP");
            }
            xpElapsed += Number(data.delta);
            var eased:Number = Math.max(0, Math.min(1, (xpElapsed - 0.2) / 0.8));
            fill(xp.LevelUPBar, xpFrom + (xpTo - xpFrom) * (1 - (1 - eased) * (1 - eased)));
            // FO76 draws this meter's text in its gold header colour, which the HUD-colour tint turns a
            // shade darker than HP/AP; on cream it lands on the HUD colour like the other meters.
            creamText(experienceGroup);
            experienceGroup.visible = xpSequence != experienceDone;
            if (xpLevel) {
                levelUp.gotoAndStop(Math.min(levelUp.totalFrames,20+Math.floor(xpElapsed*30)));
                if (xpElapsed >= (levelUp.totalFrames-20)/30) experienceDone = xpSequence;
            } else if (xpElapsed >= 2.8) experienceDone = xpSequence;
            if (experienceDone == xpSequence) experienceGroup.visible = false;
        }
        private function creamText(node:DisplayObject):void {
            var label:TextField = node as TextField;
            if (label != null) { if (label.textColor != CREAM) label.textColor = CREAM; return; }
            var container:DisplayObjectContainer = node as DisplayObjectContainer;
            if (container != null) for (var i:int = 0; i < container.numChildren; ++i) creamText(container.getChildAt(i));
        }
        private function updateDamage(data:Object, scale:Number):void {
            var rows:Array = data.damageNumbers as Array;
            var retained:Object = {};
            var viewport:Object = data.damageViewport;
            var left:Number = viewport == null ? 0 : Number(viewport.x);
            var top:Number = viewport == null ? 0 : Number(viewport.y);
            var width:Number = viewport == null ? stage.stageWidth : Number(viewport.width);
            var height:Number = viewport == null ? stage.stageHeight : Number(viewport.height);
            if (rows != null && Boolean(data.visible)) for (var i:int = 0; i < rows.length && i < 64; ++i) {
                var row:Object = rows[i];
                var age:Number = Number(row.age);
                var key:String = Boolean(row.bonus) ? "Crit_mc" : "Base_mc";
                var frame:int = 2 + int(Math.floor(age * B21_StatusSource.damageFPS));
                if (!isFinite(age) || age < 0 || frame >= int(B21_StatusSource.damageFrames[key]) ||
                    !isFinite(Number(row.x)) || !isFinite(Number(row.y)) || Number(row.x) < 0 || Number(row.x) > 1 ||
                    Number(row.y) < 0 || Number(row.y) > 1 || Number(row.value) <= 0) continue;
                var name:String = "hit" + String(row.id);
                var clip:MovieClip = damageGroup.getChildByName(name) as MovieClip;
                if (clip == null) {
                    clip = new B21_StatusDamage();
                    clip.name = name;
                    damageGroup.addChild(clip);
                    freeze(clip);
                }
                retained[name] = true;
                clip.Base_mc.visible = !Boolean(row.bonus);
                clip.Crit_mc.visible = Boolean(row.bonus);
                var animation:MovieClip = clip[key] as MovieClip;
                animation.gotoAndStop(frame);
                fittedText(animation.Number_mc,String(int(row.value)));
                // Crit numbers keep FO76's gold.
                tint(clip,Boolean(row.bonus) ? CREAM : uint(data.color));
                clip.scaleX = clip.scaleY = scale;
                var point:Point = damageGroup.globalToLocal(new Point(left + Number(row.x)*width,top + Number(row.y)*height));
                clip.x = point.x + (int(uint(row.id)*17 % 41)-20)*scale;
                clip.y = point.y - age*45*scale;
            }
            for (var j:int = damageGroup.numChildren-1; j >= 0; --j) {
                var existing:DisplayObject = damageGroup.getChildAt(j);
                if (retained[existing.name] != true) damageGroup.removeChildAt(j);
            }
        }
        public function B21Update(data:Object, host:Object):Boolean {
            visible = false;
            return host == null ? updateMenu(data) : updateHUD(data,host);
        }
        private function updateHUD(data:Object, host:Object):Boolean {
            B21Snapshot = null;
            B21FloatingActive = 0;
            for each (var passive:DisplayObject in [healthGroup,rightGroup,effectLayer,compassGroup,questGroup,conditionGroup,experienceGroup,
                                                   enemy,stealth,critical,encounters,core,explosives,damageGroup,floatingGroup,nameplates]) passive.visible = false;
            var scale:Number = stage.stageHeight / 1080;
            if (host.RightMeters_mc == null || !isFinite(scale) || scale <= 0) return false;
            updateInteraction(data,host,scale);
            var holdReady:Boolean = false;
            try { holdReady = updateHold(data,host,scale); }
            catch (error:Error) { holdReady = false; B21HoldIssue = "error: "+error.message; }
            if (!holdReady) {
                B21HoldActive = 0;
                interaction.visible = false;
                holdCompleteTime = -1;
            }
            var snap:Object = capture(data,host);
            if (snap == null) return false;
            alpha = Number(data.opacity);
            // The reward banner draws in the HUD-depth menu: HUDMenu's glass renderer turned its
            // translucent bars and shadows solid black.
            rewards.visible = false;
            B21Snapshot = snap;
            visible = Boolean(data.visible);
            return true;
        }
        private function capture(data:Object, host:Object):Object {
            var effects:Object = host.RightMeters_mc.HUDActiveEffectsWidget_mc;
            var snap:Object = {stageHeight:stage.stageHeight,alpha:host.alpha,visible:host.visible,
                inArmor:effects != null && Boolean(effects.bInPowerArmorMode),right:pointData(host.RightMeters_mc),
                bottom:host.BottomCenterGroup_mc == null ? null : pointData(host.BottomCenterGroup_mc)};
            snap.stageWidth = stage.stageWidth;
            snap.left = host.LeftMeters_mc == null ? null : pointData(host.LeftMeters_mc);
            snap.location = snap.left == null || host.LeftMeters_mc.LocationText_mc == null ? null :
                pointData(host.LeftMeters_mc.LocationText_mc);
            snap.hit = hitState;
            snap.crosshair = crossState;
            snap.quickLoot = lootState;
            snap.topCenter = host.TopCenterGroup_mc == null ? null : pointData(host.TopCenterGroup_mc);
            var questUpdate:Object = host.HUDNotificationsGroup_mc == null ? null : host.HUDNotificationsGroup_mc.QuestUpdates_mc;
            snap.questUpdate = questUpdate == null || questUpdate.QuestName_tf == null ? null : {visible:questUpdate.visible,
                alpha:questUpdate.alpha,name:String(questUpdate.QuestName_tf.text),type:String(questUpdate.UpdateType_tf.text)};
            if (!Boolean(data.health)) return snap;
            if (host.LeftMeters_mc == null || host.TopRightGroup_mc == null || host.BottomCenterGroup_mc == null) return null;
            var stockHP:Object = host.LeftMeters_mc.HPMeter_mc;
            var stockAP:Object = host.RightMeters_mc.ActionPointMeter_mc;
            var stockCompass:Object = host.BottomCenterGroup_mc.CompassWidget_mc;
            if (stockHP == null || stockAP == null || effects == null || stockHP.RadsBar_mc == null ||
                effects.ClipHolderInternal == null || stockCompass == null || stockCompass.CompassBar_mc == null ||
                stockCompass.QuestMarkerHolder_mc == null || stockCompass.OtherMarkerHolder_mc == null) return null;
            var frames:Array = [];
            var holder:Object = effects.ClipHolderInternal;
            for (var i:int = 0; i < holder.numChildren; ++i) {
                var entry:Object = holder.getChildAt(i);
                if (entry != null && entry.hasOwnProperty("IconFrame") && entry.visible) frames.push(int(entry.IconFrame));
            }
            snap.effects = frames;
            snap.topRight = pointData(host.TopRightGroup_mc);
            snap.combat = captureCombat(host);
            snap.otherMarkers = captureMarkers(stockCompass.OtherMarkerHolder_mc);
            snap.questMarkers = captureMarkers(stockCompass.QuestMarkerHolder_mc);
            snap.floating = captureFloating(host);
            B21FloatingActive = snap.floating != null ? 1 : 0;
            return snap.combat == null || snap.otherMarkers == null || snap.questMarkers == null ? null : snap;
        }
        private function updateMenu(data:Object):Boolean {
            B21HealthActive = 0;
            quickLoot.visible = reticle.visible = hit.visible = interaction.visible = rewards.visible = false;
            var snap:Object = data.snapshot;
            var map:Object = data.map;
            if (snap == null || map == null) return false;
            var hudScale:Number = Number(snap.stageHeight) / 1080;
            var scale:Number = hudScale * Number(map.ay);
            if (!isFinite(scale) || scale <= 0) return false;
            if (Boolean(data.damageOnly)) {
                if (scopeHidden == null) {
                    scopeHidden = [];
                    for (var child:int = 0; child < numChildren; ++child) {
                        var scoped:DisplayObject = getChildAt(child);
                        if (scoped != damageGroup && scoped.visible) { scoped.visible = false; scopeHidden.push(scoped); }
                    }
                }
                alpha = Number(snap.alpha)*Number(data.opacity);
                updateDamage(data,scale);
                visible = Boolean(data.visible) && Boolean(snap.visible);
                return true;
            }
            if (scopeHidden != null) {
                for each (var hidden:DisplayObject in scopeHidden) hidden.visible = true;
                scopeHidden = null;
            }
            var inArmor:Boolean = Boolean(snap.inArmor);
            // The HUD instance leaves the health parts out of its first frames after a load.
            var health:Boolean = Boolean(data.health) && snap.combat != null;
            companions.update(Boolean(data.companionHUD) && snap.left != null ? data.companions as Array : null,uint(data.color),
                Number(data.companionPortraitScale),data.companionPortraitBorder == true,
                data.companionPortraitShadow == true,data.companionPortraitShape != null ? String(data.companionPortraitShape)
                    : data.companionPortraitCircle == true ? "circle" : "square",
                String(data.companionPortraitBackground),data.companionCompact == true);
            if (snap.left != null) {
                var companionAt:Point = globalToLocal(place(snap.left,map));
                companions.x = companionAt.x+(B21_StatusSource.layout.left[4]-96)*scale;
                // The location name sits between the HP bar and the rows (LeftMeters_mc.LocationText_mc; its text
                // starts just below the clip's origin). FO76 likewise keeps its team list above its location text.
                companions.y = snap.location != null ? globalToLocal(place(snap.location,map)).y : companionAt.y-28*scale;
                companions.scaleX = companions.scaleY = scale;
            }
            rightGroup.gotoAndStop(inArmor ? "powerArmorHUD" : "defaultHUD");
            anchor(rightGroup,B21_StatusSource.layout.right,place(snap.right,map),1824,1026,scale);
            if (inArmor) anchor(conditionGroup,[1,0,0,1,1695,632],place(snap.right,map),1824,1026,scale);
            else {
                var apBox:Rectangle = ap.APBarFrame_mc.getBounds(this);
                conditionGroup.transform.matrix = new Matrix(scale,0,0,scale,apBox.x,apBox.y-72*scale);
            }
            healthGroup.visible = health && !inArmor;
            rightGroup.visible = compassGroup.visible = questGroup.visible = effectLayer.visible = floatingGroup.visible = health;
            enemy.visible = stealth.visible = critical.visible = encounters.visible = core.visible = explosives.visible = false;
            if (!updateCondition(data)) return false;
            if (health) {
                anchor(healthGroup,B21_StatusSource.layout.left,place(snap.left,map),96,1026,scale);
                anchor(compassGroup,B21_StatusSource.layout.compass,place(snap.bottom,map),960,1026,scale);
                anchor(questGroup,B21_StatusSource.layout.quest,place(snap.topRight,map),1824,54,scale);
                if (!updateCombat(data,snap,map,inArmor,scale,hudScale)) return false;
                var hpPercent:Number = Number(data.hpPercent);
                if (!fill(hp.MeterBar_mc,hpPercent) || !fill(hp.RadsBar_mc,Number(data.radsPercent)) || !fill(ap.MeterBar_mc,Number(data.apPercent))) return false;
                text(hp.DisplayText_mc,String(data.hpLabel));
                text(ap.DisplayText_mc,String(data.apLabel));
                text(hp.PercentText_mc,String(isFinite(hpPercent) ? Math.round(100*Math.max(0,hpPercent)) : 0));
                updateGlow(Number(data.glow),Number(data.delta));
                updateFeral(Number(data.feral));
                updateEffects(snap.effects as Array,inArmor,data);
                var delta:Number = Number(data.delta);
                if (!(delta > 0)) delta = 0;
                updateNeed(rightGroup.HUDHungerMeter_mc,Number(data.hunger),"foodPositive",delta,Boolean(data.alwaysShowNeeds));
                updateNeed(rightGroup.HUDThirstMeter_mc,Number(data.thirst),"thirstPositive",delta,Boolean(data.alwaysShowNeeds));
                questGroup.update(data.quests == null ? [] : data.quests as Array, delta);
                tint(questGroup,uint(data.color));
                markerCount = 0;
                compassIssues = [];
                compassLocations = [];
                compassBearings = [];
                if (!updateMarkers(snap.otherMarkers as Array,compassGroup.OtherMarkerHolder_mc,uint(data.color)) ||
                    !updateMarkers(snap.questMarkers as Array,compassGroup.QuestMarkerHolder_mc,uint(data.color))) return false;
                B21CompassIssue = compassIssue();
                updateFloating(snap.floating as Array,map,scale,uint(data.color));
                updateArea(Boolean(data.insideArea),inArmor);
                // The Glow meter and the rads segment keep their green and red.
                for (var i:int = 0; i < hp.numChildren; ++i) {
                    var part:DisplayObject = hp.getChildAt(i);
                    if (part != hp.GlowMeter_mc && part != hp.RadsBar_mc) tint(part,uint(data.color));
                }
                tint(rightGroup,uint(data.color));
                // Part by part, not the whole group: compassGroup renders through its mask filter's
                // texture, where an inverse tint on the enemy layer saturated and FO76's red came out
                // HUD-coloured. The marker holders' tinted layers are coloured in updateMarkers.
                for (var piece:int = 0; piece < compassGroup.numChildren; ++piece) {
                    var compassPart:DisplayObject = compassGroup.getChildAt(piece);
                    if (compassPart != compassGroup.OtherMarkerHolder_mc && compassPart != compassGroup.QuestMarkerHolder_mc)
                        tint(compassPart,uint(data.color));
                }
                B21HealthActive = 1;
            }
            ap.visible = !inArmor;
            if (snap.bottom != null) {
                anchor(experienceGroup,[1,0,0,1,960,1026],place(snap.bottom,map),960,1026,scale);
                xp.x = B21_StatusSource.layout.xp[4]-960;
                xp.y = B21_StatusSource.layout.xp[5]-1026;
                levelUp.x = B21_StatusSource.layout.level[4]-960;
                levelUp.y = B21_StatusSource.layout.level[5]-1026;
                updateExperience(data.experience);
                tint(experienceGroup,uint(data.color));
            }
            var crossData:Object = snap.crosshair;
            if (crossData != null && crossData.at != null && crossData.shown != null &&
                reticle.update(String(crossData.state),Number(crossData.spread),Number(data.delta))) {
                anchor(reticle,B21_StatusSource.layout.crosshair,place(crossData.at,map),0,0,scale);
                show(reticle,crossData.shown);
                reticleHostile = Boolean(data.crosshairHostile);
                if (reticleHostile) reticle.filters = [new ColorMatrixFilter(HOSTILE_RETICLE)];
                else tintLayer(reticle,uint(data.color));
            } else { reticle.reset(); reticleHostile = false; }
            var hitData:Object = snap.hit;
            if (hitData != null && hitData.at != null && hitData.shown != null) {
                if (hitSeen >= 0 && int(hitData.count) != hitSeen) hit.gotoAndPlay("Start");
                hitSeen = int(hitData.count);
                anchor(hit,B21_StatusSource.layout.hit,place(hitData.at,map),0,0,scale);
                show(hit,hitData.shown);
            } else { hit.gotoAndStop(1); hitSeen = -1; }
            var lootData:Object = snap.quickLoot;
            if (lootData != null && lootData.at != null && lootData.shown != null && quickLoot.update(lootData,data)) {
                anchor(quickLoot,B21_StatusSource.layout.quickLoot,place(lootData.at,map),0,0,scale);
                show(quickLoot,lootData.shown);
                // Tinted like the meters, so the box matches them; its permanent filter keeps the masks.
                tint(quickLoot,uint(data.color));
            } else quickLoot.reset();
            if (snap.topCenter != null) {
                rewards.update(data.rewards,snap.questUpdate,place(snap.topCenter,map),
                    Math.min(hudScale,Number(snap.stageWidth)/1920)*Number(map.ay),data.damageViewport);
                tintLayer(rewards,uint(data.color));
            }
            B21RewardDone = rewards.done;
            B21RewardNotice = rewards.noticeSeen;
            B21RewardQuestArt = rewards.questArt && rewards.visible ? 1 : 0;
            B21RewardActive = rewards.activeID;
            B21RewardModel = rewards.model && rewards.visible ? rewards.activeID : 0;
            B21RewardQuestLease = rewards.questLease ? 1 : 0;
            B21RewardModelX = rewards.modelX;
            B21RewardModelY = rewards.modelY;
            alpha = Number(snap.alpha)*Number(data.opacity);
            updateDamage(data,scale);
            B21NameplatesActive = nameplates.update(data.nameplates,data.damageViewport,scale,uint(data.color)) ? 1 : 0;
            if (B21NameplatesActive == 1) enemy.visible = false;
            visible = Boolean(data.visible) && Boolean(snap.visible);
            return true;
        }
        private function filtered(clip:DisplayObject, index:int, list:Array):Boolean {
            return clip.filters.length == 1;
        }
        private function labelAlpha(meter:MovieClip):Number {
            for (var part:int = 0; part < meter.numChildren; ++part)
                if (meter.getChildAt(part).name.indexOf("MeterLabel") >= 0) return meter.getChildAt(part).alpha;
            return -1;
        }
        // Every multiplier from the enemy layer up: any tint there, even one undone further down, loses
        // FO76's red inside the compass's filtered texture.
        private function hostileMarkerColor():Array {
            var scales:Array = [];
            for each (var node:DisplayObject in [compassGroup,compassGroup.OtherMarkerHolder_mc,compassGroup.OtherMarkerHolder_mc.getChildAt(HOSTILE_LAYER)]) {
                var cxform:ColorTransform = node.transform.colorTransform;
                scales.push(cxform.redMultiplier,cxform.greenMultiplier,cxform.blueMultiplier);
            }
            return scales;
        }
        public function B21Diagnostics():Object {
            var targetBarBounds:Rectangle = enemy.HealthBarFrame_mc.getBounds(stage);
            var targetDetailsBounds:Rectangle = targetReadout.getBounds(stage);
            var damage:Array = [];
            for (var d:int = 0; d < damageGroup.numChildren; ++d) {
                var number:MovieClip = damageGroup.getChildAt(d) as MovieClip;
                var animation:MovieClip = number.Crit_mc.visible ? number.Crit_mc : number.Base_mc;
                var damageText:TextField = field(animation.Number_mc);
                var position:Point = damageGroup.localToGlobal(new Point(number.x,number.y));
                damage.push({id:number.name,x:position.x,y:position.y,frame:animation.currentFrame,
                    text:damageText.text,font:damageText.defaultTextFormat.font,size:damageText.defaultTextFormat.size,
                    embedded:damageText.embedFonts,bonus:number.Crit_mc.visible,alpha:animation.Number_mc.alpha,color:colorOf(number)});
            }
            var warnings:Array = [];
            for (var i:int = 0; i < explosives.numChildren; ++i) {
                var warning:MovieClip = explosives.getChildAt(i) as MovieClip;
                warnings.push({x:warning.x,y:warning.y,angle:warning.rotation,grenade:warning.GrenadeIcon_mc.visible,
                    mine:warning.MineIcon_mc.visible,car:warning.CarIcon_mc.visible,iconAngle:warning.GrenadeIcon_mc.rotation});
            }
            var banks:Array = [];
            for (i = 0; i < critical.CritMeterStars_mc.numChildren; ++i) banks.push(MovieClip(critical.CritMeterStars_mc.getChildAt(i)).currentFrame);
            var cards:Array = [];
            for (i = 0; i < icons.numChildren; ++i) {
                var card:MovieClip = icons.getChildAt(i) as MovieClip;
                var cardFill:Object = card.FillInternal_mc == null ? null : card.FillInternal_mc.Fill_mc;
                cards.push({icon:card.Icon_mc.currentLabel,tile:card.currentLabel,fill:cardFill == null ? null : cardFill.y,color:colorOf(card)});
            }
            var tracker:Array = [];
            for each (var quest:MovieClip in questGroup.entries()) {
                tracker.push({kind:"entry",y:quest.y,icon:quest.Icon_mc.currentLabel,title:quest.Title_mc.textField.text,
                    label:quest.currentLabel,frame:quest.currentFrame,color:colorOf(quest)});
                for each (var goal:MovieClip in quest.B21Objectives)
                    tracker.push({kind:"objective",y:quest.y+goal.y,title:goal.Title_mc.TitleText_tf.text,
                        completed:goal.TitleCompleted_mc.TitleText_tf.text,label:goal.currentLabel,frame:goal.currentFrame,meter:goal.Meter_mc.visible,
                        color:colorOf(goal)});
            }
            if (questGroup.dividerVisible) tracker.push({kind:"divider",y:questGroup.divider.y,icon:null,title:null,
                label:questGroup.divider.currentLabel,color:colorOf(questGroup.divider)});
            tracker.sortOn("y", Array.NUMERIC);
            var floating:Array = [];
            for (var layerIndex:int = 0; layerIndex < 2; ++layerIndex) {
                var floatingLayer:MovieClip = floatingGroup.getChildAt(layerIndex) as MovieClip;
                for (i = 0; i < floatingLayer.numChildren; ++i) {
                    var target:MovieClip = floatingLayer.getChildAt(i) as MovieClip;
                    var targetInner:MovieClip = target.getChildByName("Internal_mc") as MovieClip;
                    var targetAt:Point = floatingLayer.localToGlobal(new Point(target.x,target.y));
                    floating.push({frame:target.currentLabel,type:targetInner == null ? null : targetInner.currentLabel,
                        x:targetAt.x,y:targetAt.y,scale:target.scaleX,alpha:target.alpha,hostile:layerIndex == 1,
                        color:colorOf(floatingLayer,GOLD)});
                }
            }
            return {floating:floating,floatingActive:B21FloatingActive,floatingVisible:floatingGroup.visible,
                rewards:rewards.diagnostic(),quickLoot:quickLoot.diagnostics(),quickLootActive:B21QuickLootActive,quickLootVisible:quickLoot.visible,
                quickX:quickLoot.x,quickY:quickLoot.y,crossX:reticle.x,crossY:reticle.y,
                crosshair:reticle.diagnostics(),crosshairActive:B21CrosshairActive,crosshairVisible:reticle.visible,
                crosshairHostile:reticleHostile,hitActive:B21HitActive,hitFrame:hit.currentFrame,hitVisible:hit.visible,
                damage:damage,hpFrame:hp.MeterBar_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,
                radFrame:hp.RadsBar_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,
                apFrame:ap.MeterBar_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,
                conditionFrame:cnd.MeterClip_mc.currentFrame,conditionFrames:cnd.MeterClip_mc.totalFrames,conditionFill:cnd.MeterClip_mc.visible,
                conditionColor:colorOf(cnd.MeterClip_mc,GOLD),conditionBackColor:colorOf(conditionFrame),conditionFlat:conditionGroup.filters.length == 1,
                conditionOverrepair:cnd.MeterClip_mc.visible && cnd.MeterClip_mc.numChildren > 1,
                iconCount:icons.numChildren,effectOffsetX:icons.x,effectOffsetY:icons.y,effectCards:cards,tracker:tracker,
                trackerColor:colorOf(questGroup),hpColor:colorOf(hp.MeterBar_mc),radsBarColor:colorOf(hp.RadsBar_mc),
                radsColor:colorOf(healthGroup.RadsMeter_mc),hostileMarkers:MovieClip(compassGroup.OtherMarkerHolder_mc.getChildAt(HOSTILE_LAYER)).numChildren,
                hostileMarkerColor:hostileMarkerColor(),markerColor:shownColor(compassGroup.OtherMarkerHolder_mc.getChildAt(0),CREAM),
                compassColor:shownColor(compassGroup.getChildAt(0),CREAM),

                weaponIconVisible:conditionGroup.visible && weaponIcon != null,
                weaponIconName:weaponName,
                hungerVisible:rightGroup.HUDHungerMeter_mc.visible,thirstVisible:rightGroup.HUDThirstMeter_mc.visible,
                hungerFrame:rightGroup.HUDHungerMeter_mc.Meter_mc.currentFrame,
                thirstFrame:rightGroup.HUDThirstMeter_mc.Meter_mc.currentFrame,
                needLabelAlpha:[labelAlpha(rightGroup.HUDHungerMeter_mc),labelAlpha(rightGroup.HUDThirstMeter_mc)],
                feralVisible:rightGroup.FeralMeter_mc.visible,
                feralFrame:rightGroup.FeralMeter_mc.FeralMeterInternal_mc.currentFrame,
                feralRollFrame:rightGroup.FeralMeter_mc.currentFrame,glowFrame:hp.GlowMeter_mc.Glow_mc.currentFrame,
                glowVisible:hp.GlowMeter_mc.visible,glowX:hp.GlowMeter_mc.Meter_mc.Fill_mc.x,
                radsVisible:healthGroup.RadsMeter_mc.visible,radsText:healthGroup.RadsMeter_mc.RadsNumber_tf.text,
                ammoVisible:rightGroup.AmmoCount_mc.visible,clipText:rightGroup.AmmoCount_mc.ClipCount_tf.text,
                reserveText:rightGroup.AmmoCount_mc.ReserveCount_tf.text,
                grenadeVisible:rightGroup.ExplosiveAmmoCount_mc.visible,grenadeText:rightGroup.ExplosiveAmmoCount_mc.AvailableCount_tf.text,
                grenadeFrame:rightGroup.ExplosiveAmmoCount_mc.TypeIcon_mc.currentFrame,warnings:warnings,
                holdVisible:interaction.visible,holdFrame:holdButton.HoldMeter_mc.currentFrame,
                holdKey:holdButton.IconHolderInstance.IconAnimInstance.Icon_tf.text,holdRingScale:holdButton.HoldMeter_mc.scaleX,
                holdName:interaction.Internal_mc.Header_mc.Header_tf.text,holdLabel:holdButton.textField_tf.text,
                nameplates:nameplates.diagnostic(),nameplatesActive:B21NameplatesActive,
                targetReadout:targetReadout.text,targetIcons:targetReadout.icons,targetDetailsHeight:targetReadout.height,
                targetDetailsColor:field(targetReadout) == null ? -1 : field(targetReadout).textColor,
                targetDetailsTop:targetReadout.getBounds(stage).top,stealthBottom:stealth.getBounds(stage).bottom,
                targetBarCenter:targetBarBounds.x+targetBarBounds.width/2,
                targetBarBottom:targetBarBounds.bottom,
                targetDetailsCenter:targetDetailsBounds.x+targetDetailsBounds.width/2,
                enemyVisible:enemy.visible,enemyName:field(enemy.DisplayText_mc).text,enemyLabel:enemy.currentLabel,
                enemyTextFits:field(enemy.DisplayText_mc).textWidth <= field(enemy.DisplayText_mc).width-4,
                enemyHealthFrame:enemy.MeterBarEnemy_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,enemyStars:enemyStars.numChildren,
                enemySkull:enemy.EncounterHolder_mc.visible,enemySkullFrame:enemy.EncounterHolder_mc.Encounter_mc.currentLabel,
                enemySkullLeft:enemy.EncounterHolder_mc.getBounds(enemy).x,enemyStarsRight:enemyStars.getBounds(enemy).right,
                enemyNameRight:field(enemy.DisplayText_mc).getBounds(enemy).x+field(enemy.DisplayText_mc).getBounds(enemy).width/2+
                    field(enemy.DisplayText_mc).textWidth/2,
                maskHosts:[healthGroup,effectLayer,compassGroup,experienceGroup,enemy,encounters].filter(filtered).length,
                enemyBar:enemy.MeterBarEnemy_mc.visible ? "enemy" : enemy.MeterBarFriendly_mc.visible ? "friendly" : "neutral",
                enemyBarColors:[shownColor(enemy.MeterBar_mc,CREAM),shownColor(enemy.MeterBarEnemy_mc,HOSTILE),shownColor(enemy.MeterBarFriendly_mc,CREAM)],
                enemyNameColor:field(enemy.DisplayText_mc).textColor,enemyColor:colorOf(enemy),encounterColor:colorOf(encounters),
                enemyLevelColor:shownColor(enemy.LevelText_mc,CREAM),enemyStarColor:shownColor(enemyStars,CREAM),
                explosivesColor:colorOf(explosives),
                stealthVisible:stealth.visible,stealthText:field(stealth.Internal_mc.stealthTextStates).text,
                stealthFrame:stealth.Internal_mc.currentFrame,stealthState:stealth.Internal_mc.stealthTextStates.currentLabel,
                stealthColor:colorOf(stealth),
                stealthLeft:stealth.Internal_mc.getChildByName("BracketLeftInstance").x,stealthRight:stealth.Internal_mc.getChildByName("BracketRightInstance").x,
                criticalVisible:critical.visible,criticalFrame:critical.MeterBar_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,critStars:banks,
                encounterVisible:encounters.visible,coreVisible:core.visible,coreFrame:core.Meter_mc.currentFrame,
                coreText:field(core.CoreCount_mc).text,heatVisible:rightGroup.OverheatMeter_mc.visible,
                heatFrame:rightGroup.OverheatMeter_mc.MeterBar_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,
                questCount:questGroup.entries().length,questBusy:questGroup.busy,markerCount:markerCount,compassVisible:compassGroup.visible,
                xpVisible:experienceGroup.visible,experienceDone:experienceDone,levelTextAlpha:levelUp.LevelUpText.alpha,
                xpBarFrame:xp.LevelUPBar.MeterBarInternal_mc.Contents.Fill.currentFrame,
                xpGhostFrame:xp.Optional_mc.MeterBarInternal_mc.Contents.Fill.currentFrame,
                xpTextColor:field(xp.CurrentLevelField).textColor,
                xpFillColor:shownColor(xp.LevelUPBar.MeterBarInternal_mc.Contents.Fill,GOLD)};
        }
    }
}
