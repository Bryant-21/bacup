package {
    import flash.display.MovieClip;
    import flash.text.TextField;
    import flash.text.TextFormat;

    // FO76's NewQuestTracker (interface/newquesttracker.swf) on rows carrying its QuestTrackerProvider fields.
    // Quests and objectives are matched by ID: new ones fade in, missing ones play their fast fade-out and are
    // removed half a second later, then the rest slide into place. Finished rows hold their completed frame.
    public class B21_StatusQuestTracker extends MovieClip {
        private static const QUEST_SPACING:Number = 16;
        private static const REMOVE_SECONDS:Number = .5;
        private static const REARRANGE_SECONDS:Number = .35;
        private static const COMPLETE:uint = 2;
        private static const FAILED:uint = 3;
        private static const MAIN:uint = 1;
        private static const EVENT:uint = 4;
        public var divider:MovieClip = new B21_StatusQuestDivider();
        private var shown:Object = {};
        private var rows:Array = [];
        private var structure:String = "";
        private var leaving:Array = [];
        private var leavingObjectives:Array = [];
        private var removeIn:Number = -1;
        private var dividerShown:Boolean = false;
        private var tweens:Array = [];
        private var completedLabel:String = null;
        private var failedLabel:String = null;
        private var orPrefix:String = null;

        public function B21_StatusQuestTracker() {
            addChild(divider);
        }

        public function update(data:Array, delta:Number):void {
            advance(delta);
            var ordered:Array = [];
            for each (var event:Object in data) if (uint(event.displayType) == EVENT) ordered.push(event);
            for each (var other:Object in data) if (uint(other.displayType) != EVENT) ordered.push(other);
            var key:String = structureOf(ordered);
            // As FO76, nothing changes shape while removed rows are still fading.
            if (key != structure && removeIn < 0) apply(ordered, key);
            for each (var row:Object in ordered) {
                var entry:MovieClip = shown[String(row.questId)] as MovieClip;
                if (entry == null) continue;
                questTimer(entry, Number(row.timer));
                for each (var item:Object in row.objectives as Array) {
                    var objective:MovieClip = find(entry, uint(item.objectiveId));
                    if (objective != null && objective.B21Timer != Number(item.timer)) {
                        objective.B21Timer = Number(item.timer);
                        objectiveTitle(objective);
                    }
                }
            }
        }

        public function entries():Array {
            var result:Array = leaving.concat();
            for each (var row:Object in rows) if (shown[String(row.questId)] != null) result.push(shown[String(row.questId)]);
            return result;
        }

        public function get dividerVisible():Boolean { return dividerShown; }

        public function get busy():Boolean { return removeIn >= 0 || tweens.length > 0; }

        // Timers are left out: FO76 ticks them without rebuilding the tracker.
        private function structureOf(data:Array):String {
            var key:String = "";
            for each (var row:Object in data) {
                key += row.questId + ":" + row.title + ":" + row.displayType + ":" + row.state + "[";
                for each (var item:Object in row.objectives as Array)
                    key += item.objectiveId + ":" + item.title + ":" + item.state + ":" + item.isOrObjective + ":" + item.progress + ";";
                key += "]";
            }
            return key;
        }

        private function apply(data:Array, key:String):void {
            var next:Object = {};
            for each (var row:Object in data) {
                var id:String = String(row.questId);
                var entry:MovieClip = shown[id] as MovieClip;
                if (entry == null) entry = addQuest(row);
                else {
                    setQuest(entry, row);
                    entryState(entry);
                    var seen:Object = {};
                    var objectives:Array = row.objectives as Array;
                    for (var i:int = 0; i < objectives.length; ++i) {
                        var objective:MovieClip = find(entry, uint(objectives[i].objectiveId));
                        if (objective == null) {
                            objective = addObjective(entry, objectives[i]);
                            // FO76 starts each objective's fade 0-5 frames in.
                            objective.gotoAndPlay(2 + Math.floor(Math.random() * 6));
                        } else setObjective(objective, objectives[i]);
                        objective.B21Index = i;
                        seen[String(objectives[i].objectiveId)] = true;
                    }
                    for each (var old:MovieClip in entry.B21Objectives) if (!seen[String(old.B21Id)]) {
                        fadeOutObjective(old);
                        leavingObjectives.push([old, entry]);
                    }
                    entry.B21Objectives.sortOn("B21Index", Array.NUMERIC);
                    arrangeObjectives(entry, false);
                }
                next[id] = entry;
            }
            for (var gone:String in shown) if (next[gone] == null) {
                var quest:MovieClip = shown[gone];
                quest.gotoAndPlay("FadeOutFast");
                for each (var part:MovieClip in quest.B21Objectives) fadeOutObjective(part);
                leaving.push(quest);
            }
            shown = next;
            rows = data;
            structure = key;
            arrangeQuests(true);
            if (leaving.length > 0 || leavingObjectives.length > 0) removeIn = REMOVE_SECONDS;
        }

        private function advance(delta:Number):void {
            for (var i:int = tweens.length - 1; i >= 0; --i) {
                var tween:Object = tweens[i];
                tween.time = Math.min(REARRANGE_SECONDS, tween.time + delta);
                tween.clip.y = ease(tween.time, tween.from, tween.to - tween.from, REARRANGE_SECONDS);
                if (tween.time >= REARRANGE_SECONDS) tweens.splice(i, 1);
            }
            if (removeIn < 0) return;
            removeIn -= delta;
            if (removeIn > 0) return;
            removeIn = -1;
            while (leavingObjectives.length > 0) {
                var pair:Array = leavingObjectives.pop();
                var owner:MovieClip = pair[1];
                owner.B21Objectives.splice(owner.B21Objectives.indexOf(pair[0]), 1);
                stopTween(pair[0]);
                owner.removeChild(pair[0]);
                arrangeObjectives(owner, true);
                for (var index:int = 0; index < owner.B21Objectives.length; ++index) owner.B21Objectives[index].B21Index = index;
            }
            while (leaving.length > 0) {
                var quest:MovieClip = leaving.pop();
                stopTween(quest);
                for each (var part:MovieClip in quest.B21Objectives) stopTween(part);
                removeChild(quest);
            }
            arrangeQuests(true);
        }

        // fl.transitions.easing.Regular.easeInOut, which FO76's rearrange tweens use.
        private static function ease(time:Number, from:Number, change:Number, duration:Number):Number {
            time /= duration / 2;
            if (time < 1) return change / 2 * time * time + from;
            --time;
            return -change / 2 * (time * (time - 2) - 1) + from;
        }

        private function stopTween(clip:MovieClip):void {
            for (var i:int = tweens.length - 1; i >= 0; --i) if (tweens[i].clip == clip) tweens.splice(i, 1);
        }

        private function moveTo(clip:MovieClip, y:Number, animate:Boolean):void {
            stopTween(clip);
            if (animate && clip.y != y) tweens.push({clip:clip, from:clip.y, to:y, time:0});
            else clip.y = y;
        }

        private function addQuest(row:Object):MovieClip {
            var entry:MovieClip = new B21_StatusQuestEntry();
            entry.B21Objectives = [];
            entry.B21Timer = -1;
            setQuest(entry, row);
            var objectives:Array = row.objectives as Array;
            for (var i:int = 0; i < objectives.length; ++i) addObjective(entry, objectives[i]).B21Index = i;
            arrangeObjectives(entry, false);
            entry.B21New = true;
            entryState(entry);
            addChild(entry);
            entry.gotoAndPlay("FadeIn");
            return entry;
        }

        private function setQuest(entry:MovieClip, row:Object):void {
            entry.B21Id = uint(row.questId);
            entry.B21State = uint(row.state);
            entry.B21Event = uint(row.displayType) == EVENT;
            entry.B21Title = String(row.title);
            entry.Icon_mc.gotoAndStop(uint(row.displayType) == MAIN ? "MainQuestTracker" : "questTracker");
            questTimer(entry, Number(row.timer));
            questTitle(entry);
        }

        private function entryState(entry:MovieClip):void {
            entry.gotoAndPlay(entry.B21State == COMPLETE || entry.B21State == FAILED ? "CompleteIdle" : "Idle");
        }

        // FO76 also fills a "TIME REMAINING" line, but its Timer_mc is placed hidden and NewQuestTracker never shows
        // it, so the clock appears only in the title.
        private function questTimer(entry:MovieClip, timer:Number):void {
            if (!(timer >= 0)) timer = -1;
            if (entry.B21Timer == timer) return;
            entry.B21Timer = timer;
            questTitle(entry);
        }

        private function questTitle(entry:MovieClip):void {
            var title:String = String(entry.B21Title).toUpperCase();
            if (entry.B21Timer >= 0) title += " [" + formatTime(entry.B21Timer) + "]";
            fit(entry.Title_mc.textField, title);
        }

        private function addObjective(entry:MovieClip, data:Object):MovieClip {
            var objective:MovieClip = new B21_StatusQuestObjective();
            objective.B21Id = uint(data.objectiveId);
            objective.B21Timer = -1;
            objective.Meter_mc.visible = false;
            objective.MergedLeaderIcon_mc.Internal_mc.visible = false;
            objective.VertiibirdIcon_mc.Internal_mc.visible = false;
            entry.addChild(objective);
            entry.B21Objectives.push(objective);
            setObjective(objective, data);
            return objective;
        }

        private function setObjective(objective:MovieClip, data:Object):void {
            var title:String = String(data.title);
            if (Boolean(data.isOrObjective)) {
                if (orPrefix == null) orPrefix = translate("$QUEST_TRACKER_OBJECTIVE_OR_PREFIX");
                title = orPrefix + " " + title;
            }
            objective.B21State = uint(data.state);
            objective.B21Title = title;
            objective.B21Timer = Number(data.timer);
            var progress:Number = Number(data.progress);
            objective.Meter_mc.visible = progress >= 0;
            if (objective.Meter_mc.visible) {
                var meter:MovieClip = objective.Meter_mc.Internal_mc;
                meter.gotoAndStop(objective.B21State >= COMPLETE ? meter.totalFrames : Math.max(1, Math.floor(progress * meter.totalFrames)));
            }
            objectiveTitle(objective);
            objective.gotoAndPlay(objective.B21State == COMPLETE || objective.B21State == FAILED ? "CompleteIdle" : "Idle");
        }

        private function fadeOutObjective(objective:MovieClip):void {
            objective.gotoAndPlay(objective.B21State >= COMPLETE ? "FadeOutFast" : "FadeOutIncompleteFast");
        }

        // FO76 puts an objective's clock before its text; the completed copy also leads with its state.
        private function objectiveTitle(objective:MovieClip):void {
            var prefix:String = objective.B21Timer > 0 ? formatTime(objective.B21Timer) + " " : "";
            fit(objective.Title_mc.TitleText_tf, prefix + objective.B21Title);
            if (completedLabel == null) {
                completedLabel = translate("$QuestTrackerState_Completed");
                failedLabel = translate("$QuestTrackerState_Failed");
            }
            var state:String = objective.B21State == FAILED ? failedLabel : completedLabel;
            fit(objective.TitleCompleted_mc.TitleText_tf, state + " " + prefix + objective.B21Title);
        }

        private function find(entry:MovieClip, id:uint):MovieClip {
            for each (var objective:MovieClip in entry.B21Objectives) if (objective.B21Id == id) return objective;
            return null;
        }

        // FO76 stacks objectives below the header.
        private function arrangeObjectives(entry:MovieClip, animate:Boolean):void {
            var y:Number = entry.Sizer_mc.height;
            for each (var objective:MovieClip in entry.B21Objectives) {
                moveTo(objective, y, animate && objective.visible);
                y += objectiveHeight(objective);
            }
        }

        private function objectiveHeight(objective:MovieClip):Number {
            return objective.Sizer_mc.height + (objective.Meter_mc.visible ? objective.Meter_mc.Internal_mc.Sizer_mc.height : 0);
        }

        private function arrangeQuests(animate:Boolean):void {
            var y:Number = 0;
            var afterEvent:Boolean = false;
            var dividerUsed:Boolean = false;
            for each (var row:Object in rows) {
                var entry:MovieClip = shown[String(row.questId)] as MovieClip;
                if (entry == null) continue;
                if (entry.B21Event) afterEvent = true;
                else if (afterEvent) {
                    divider.y = y + QUEST_SPACING;
                    y += divider.Sizer_mc.height + QUEST_SPACING;
                    dividerUsed = true;
                    afterEvent = false;
                }
                moveTo(entry, y, animate && !entry.B21New);
                entry.B21New = false;
                y += entry.Sizer_mc.height;
                for each (var objective:MovieClip in entry.B21Objectives) y += objectiveHeight(objective);
                y += QUEST_SPACING;
            }
            if (dividerUsed != dividerShown) divider.gotoAndPlay(dividerUsed ? "rollOn" : "rollOff");
            dividerShown = dividerUsed;
        }

        // GlobalFunc.FormatTimeString: zero-padded days:hours:minutes:seconds without empty leading parts.
        public static function formatTime(seconds:Number):String {
            var days:Number = Math.floor(seconds / 86400);
            var hours:Number = Math.floor(seconds % 86400 / 3600);
            var minutes:Number = Math.floor(seconds % 3600 / 60);
            var rest:Number = Math.floor(seconds % 60);
            var parts:Array = [];
            if (days > 0) parts.push(pad(days));
            if (days > 0 || hours > 0) parts.push(pad(hours));
            if (days > 0 || hours > 0 || minutes > 0) parts.push(pad(minutes));
            if (days > 0 || hours > 0 || minutes > 0 || rest > 0) return parts.length > 0 ? parts.join(":") + ":" + pad(rest) : "0:" + pad(rest);
            return parts.join(":");
        }

        private static function pad(value:Number):String {
            return value < 10 ? "0" + value : String(value);
        }

        // GFx translates a $ key when it is assigned; reading it back gives the text.
        private static function translate(key:String):String {
            var scratch:TextField = new TextField();
            scratch.defaultTextFormat = new TextFormat("$MAIN_Font", 18, 0xFFFFFF);
            scratch.text = key;
            return scratch.text;
        }

        // FO76 sets these fields to shrink their text to fit.
        private static function fit(field:TextField, value:String):void {
            if (field == null || field.text == value) return;
            var format:TextFormat = field.defaultTextFormat;
            field.text = value;
            field.setTextFormat(format);
            if (field.textWidth > field.width - 4) {
                format.size = Math.max(1, Math.floor(Number(format.size) * (field.width - 4) / field.textWidth));
                field.setTextFormat(format);
            }
        }
    }
}
