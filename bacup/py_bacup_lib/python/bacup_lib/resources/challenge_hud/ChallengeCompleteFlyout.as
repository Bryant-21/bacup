package {
    import flash.display.DisplayObject;
    import flash.display.DisplayObjectContainer;
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.geom.ColorTransform;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.utils.getDefinitionByName;

    // FO76's challenge-complete banner, Regular variant only: Tales has no S.C.O.R.E., rank-ups or atoms.
    // The source timeline keeps its frame scripts; this class starts it and reports when it rolled off.
    public dynamic class ChallengeCompleteFlyout extends MovieClip {
        private var regular:MovieClip;
        private var current:uint = 0;
        private var done:uint = 0;
        private var icon:DisplayObject;

        public function ChallengeCompleteFlyout() {
            super();
            stop();
            mouseEnabled = false;
            mouseChildren = false;
            regular = MovieClip(getChildByName("RegularChallengeCompleteFlyout_mc"));
            for each (var name:String in ["ScoreChallengeCompleteFlyout_mc", "RankCompleteFlyout_mc"]) {
                var other:MovieClip = getChildByName(name) as MovieClip;
                if (other != null) {
                    other.gotoAndStop("off");
                    other.visible = false;
                }
            }
            var score:DisplayObject = getChildByName("ScoreWidgetManager_mc");
            if (score != null) score.visible = false;
            regular.gotoAndStop("off");
            addEventListener("AnimComplete", onAnimComplete);
            addEventListener("rollOffComplete", onRollOffComplete);
            visible = false;
        }

        // Returns the id of the last notice that finished rolling off.
        public function B21SetCompletion(data:Object):uint {
            visible = Boolean(data.visible);
            if (!visible) return done;
            // FO76 docks the flyouts to the left edge of the screen at (-1,-6).
            x = Number(data.left) - 1;
            y = -6;
            if (uint(data.id) != current) begin(data);
            retitle(String(data.title));
            tint(uint(data.color));
            return done;
        }

        // The timeline recreates FO76's static title (a $ key) on every roll-on.
        private function retitle(title:String):void {
            for (var i:int = 0; i < regular.numChildren; ++i) {
                var part:DisplayObjectContainer = regular.getChildAt(i) as DisplayObjectContainer;
                var text:TextField = part == null ? null : field(part);
                if (text != null && text.text.charAt(0) == "$") text.text = title;
            }
        }

        private function begin(data:Object):void {
            current = uint(data.id);
            var count:uint = uint(data.count);
            setText(field(child(regular, "Description_mc")), String(data.name) + " (" + count + "/" + count + ")");
            var packs:uint = uint(data.perkCardPacks);
            var caps:uint = uint(data.caps);
            var xp:uint = uint(data.xp);
            var holder:DisplayObjectContainer = child(regular, "RewardIcon_mc");
            if (icon != null && icon.parent != null) icon.parent.removeChild(icon);
            icon = null;
            var art:String = packs > 0 ? "" : caps > 0 ? "B21_ChallengeIconCaps" : xp > 0 ? "B21_ChallengeIconExperience" : "";
            if (art.length > 0) {
                var type:Class = getDefinitionByName(art) as Class;
                icon = DisplayObject(new type());
                // FO76's icon loader sizes the 128px library icons to this box.
                icon.width = icon.height = 82.94;
                holder.addChild(icon);
            }
            holder.visible = icon != null;
            child(regular, "PerkPackIcon_mc").visible = packs > 0;
            child(regular, "AtomIcon_mc").visible = false;
            // Tales' caps or XP take the place of FO76's atom count.
            var rewardCount:DisplayObjectContainer = child(regular, "RewardsCount_mc");
            rewardCount.visible = icon != null;
            if (icon != null) setText(field(rewardCount), "+" + (caps > 0 ? caps : xp));
            // FO76 shows the player's atom balance here; Tales has no atoms.
            var total:DisplayObjectContainer = child(regular, "TotalRewards_mc");
            var totalIcon:DisplayObject = total.getChildByName("TotalRewardsIcon_mc");
            if (totalIcon != null) totalIcon.visible = false;
            var totalText:TextField = field(total);
            if (totalText != null) totalText.text = "";
            regular.gotoAndPlay("rollOn");
        }

        private function onAnimComplete(event:Event):void {
            regular.gotoAndPlay("rollOff");
        }

        private function onRollOffComplete(event:Event):void {
            done = current;
            regular.gotoAndStop("off");
        }

        private function child(parent:DisplayObjectContainer, name:String):MovieClip {
            var value:MovieClip = parent.getChildByName(name) as MovieClip;
            if (value == null) throw new Error("Missing challenge flyout part " + name);
            return value;
        }

        private function field(container:DisplayObjectContainer):TextField {
            for (var i:int = 0; i < container.numChildren; ++i) {
                var text:TextField = container.getChildAt(i) as TextField;
                if (text != null) return text;
            }
            return null;
        }

        private function setText(target:TextField, value:String):void {
            if (target == null) throw new Error("Missing challenge flyout text");
            target.text = value;
            if (!target.multiline && target.textWidth > target.width - 4) {
                var format:TextFormat = target.getTextFormat();
                format.size = Math.max(1, Math.floor(Number(format.size) * (target.width - 4) / target.textWidth));
                target.setTextFormat(format);
            }
        }

        // FO4 HUD Color, as the status HUD applies it to FO76's cream art.
        private function tint(color:uint):void {
            var base:uint = 0xFFFFCB;
            var tintColor:ColorTransform = new ColorTransform(
                Math.max(2, (color >> 16) & 255) / Math.max(2, (base >> 16) & 255),
                Math.max(2, (color >> 8) & 255) / Math.max(2, (base >> 8) & 255),
                Math.max(2, color & 255) / Math.max(2, base & 255));
            transform.colorTransform = tintColor;
            var pack:MovieClip = child(regular, "PerkPackIcon_mc");
            var artColor:ColorTransform = pack.transform.colorTransform;
            artColor.redMultiplier = 1 / tintColor.redMultiplier;
            artColor.greenMultiplier = 1 / tintColor.greenMultiplier;
            artColor.blueMultiplier = 1 / tintColor.blueMultiplier;
            pack.transform.colorTransform = artColor;
        }
    }
}
