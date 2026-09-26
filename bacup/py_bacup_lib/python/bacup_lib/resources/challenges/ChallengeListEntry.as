package {
    import flash.display.MovieClip;
    import flash.geom.ColorTransform;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import Shared.AS3.SWFLoaderClip;
    import Shared.AS3.BSScrollingListEntry;
    import Shared.GlobalFunc;

    public class ChallengeListEntry extends BSScrollingListEntry {
        public var Internal_mc:MovieClip;
        private var RewardText_tf:TextField;
        private var CheckMark_mc:MovieClip;
        private var Tracked_mc:MovieClip;
        private var RewardIcon_mc:SWFLoaderClip;
        private var SpecialIcon_mc:MovieClip;
        private var bCompleted:Boolean;
        private var bIsTrackable:Boolean;
        private var bIsRerolled:Boolean;
        private var m_IconInstance:MovieClip;
        private var initialTextX:Number;
        private var initialTextWidth:Number;
        private var m_DefaultSize:int;
        private var lineCount:int;
        private static const blackColorTransform:ColorTransform = new ColorTransform(0, 0, 0, 1);
        private static const normalColorTransform:ColorTransform = new ColorTransform();

        public function ChallengeListEntry() {
            super();
            stop();
            RewardText_tf = Internal_mc.RewardText_tf;
            CheckMark_mc = Internal_mc.CheckMark_mc;
            Tracked_mc = Internal_mc.Tracked_mc;
            RewardIcon_mc = Internal_mc.RewardIcon_mc;
            SpecialIcon_mc = Internal_mc.SpecialIcon_mc;
            textField = Internal_mc.textField;
            initialTextX = textField.x;
            initialTextWidth = textField.width;
            m_DefaultSize = int(textField.getTextFormat().size);
            RewardIcon_mc.clipWidth = RewardIcon_mc.width / RewardIcon_mc.scaleX;
            RewardIcon_mc.clipHeight = RewardIcon_mc.height / RewardIcon_mc.scaleY;
        }

        override public function SetEntryText(entry:Object, option:String):* {
            bCompleted = entry.completed;
            bIsTrackable = !bCompleted;
            bIsRerolled = false;
            CheckMark_mc.visible = bCompleted;
            Tracked_mc.visible = entry.isTracked;
            SpecialIcon_mc.visible = false;
            textField.x = initialTextX + (bCompleted ? 38 : 0);
            textField.width = 650 - (bCompleted ? 38 : 0);
            textField.text = entry.text + " (" + entry.currentValue + "/" + entry.thresholdValue + ")";
            GlobalFunc.quickMultiLineShrinkToFit(textField, m_DefaultSize);
            RewardText_tf.text = entry.reward.caps + " CAPS  +  " + entry.reward.xp + " XP";
            if (entry.reward.perkCardPacks > 0) RewardText_tf.appendText("  +  " + entry.reward.perkCardPacks + " PACKS");
            RewardText_tf.x = initialTextX + 34;
            RewardText_tf.y = textField.y + Math.max(1, textField.numLines) * 26;
            RewardText_tf.width = 610;
            RewardText_tf.height = 28;
            var rewardFormat:TextFormat = new TextFormat("$MAIN_Font", 18);
            rewardFormat.align = "left";
            RewardText_tf.setTextFormat(rewardFormat);
            RewardIcon_mc.x = initialTextX;
            RewardIcon_mc.y = RewardText_tf.y;
            RewardIcon_mc.scaleX = 1;
            RewardIcon_mc.scaleY = 1;
            if (m_IconInstance == null) {
                m_IconInstance = new IconCR_Caps();
                RewardIcon_mc.addChild(m_IconInstance);
                m_IconInstance.width = 24;
                m_IconInstance.height = 24;
            }
            RewardIcon_mc.visible = true;
            textField.textColor = selected ? 0 : GlobalFunc.COLOR_TEXT_BODY;
            RewardText_tf.textColor = textField.textColor;
            var color:ColorTransform = selected ? blackColorTransform : normalColorTransform;
            CheckMark_mc.transform.colorTransform = color;
            Tracked_mc.transform.colorTransform = color;
            RewardIcon_mc.transform.colorTransform = color;
            textField.alpha = bCompleted ? 0.6 : 1;
            RewardText_tf.alpha = textField.alpha;
            lineCount = Math.max(2, Math.min(3, textField.numLines + 1));
            var background:MovieClip = getChildAt(1) as MovieClip;
            if (background != null) background.gotoAndStop("Line" + lineCount);
            border.gotoAndStop("Line" + lineCount);
            border.alpha = selected ? 1 : 0;
            Sizer_mc.gotoAndStop("Line" + lineCount);
            graphics.clear();
            graphics.beginFill(selected ? 0 : GlobalFunc.COLOR_TEXT_BODY, 0.7);
            graphics.drawRect(initialTextX, Sizer_mc.height - 3,
                initialTextWidth * Math.min(1, entry.currentValue / Math.max(1, entry.thresholdValue)), 2);
            graphics.endFill();
        }
    }
}
