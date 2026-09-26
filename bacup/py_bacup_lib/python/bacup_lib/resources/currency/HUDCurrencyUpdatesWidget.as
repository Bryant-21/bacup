package {
    import Shared.AS3.SWFLoaderClip;
    import flash.display.MovieClip;
    import flash.geom.Rectangle;
    import flash.text.TextFieldAutoSize;
    import flash.text.TextField;
    import flash.text.TextFormat;

    public class HUDCurrencyUpdatesWidget extends MovieClip {
        public var CurrencyIcon_mc:SWFLoaderClip;
        public var CurrencyChange_mc:MovieClip;
        public var CurrencyBase_mc:MovieClip;
        public var Breakdown_mc:MovieClip;
        private var icon:MovieClip;
        private var sequence:uint = 0;
        private var awardLabel:TextField;

        public function HUDCurrencyUpdatesWidget() {
            super();
            stop();
            Breakdown_mc.visible = false;
            awardLabel = new TextField();
            awardLabel.defaultTextFormat = new TextFormat("$MAIN_Font", 24, 0xFFFFFF);
            awardLabel.autoSize = TextFieldAutoSize.LEFT;
            awardLabel.selectable = false;
            awardLabel.mouseEnabled = false;
            addChild(awardLabel);
        }

        public function B21Update(data:Object):void {
            var frame:int = Math.min(totalFrames, 2 + Math.floor(Number(data.elapsed) * 30));
            gotoAndStop(frame);
            Breakdown_mc.visible = false;
            Breakdown_mc.stop();
            if (sequence != data.sequence || icon == null || icon.parent != CurrencyIcon_mc) {
                sequence = data.sequence;
                if (icon != null && icon.parent != null) icon.parent.removeChild(icon);
                // The source's transparent sizing box must not affect visible bounds.
                while (CurrencyIcon_mc.numChildren > 0) CurrencyIcon_mc.removeChildAt(0);
                CurrencyIcon_mc.clipWidth = 60;
                CurrencyIcon_mc.clipHeight = 60;
                if (String(data.icon) == "PerkCardPack") {
                    icon = CurrencyIcon_mc.setContainerIconClip("IconCR_PerkPack");
                } else if (String(data.icon).length > 0) {
                    icon = CurrencyIcon_mc.setContainerIconClip(String(data.icon));
                } else {
                    icon = new MovieClip();
                    icon.graphics.lineStyle(3, 0xFFFFFF);
                    icon.graphics.drawRect(7, 4, 44, 51);
                    icon.graphics.moveTo(16, 18);
                    icon.graphics.lineTo(42, 18);
                    icon.graphics.moveTo(16, 28);
                    icon.graphics.lineTo(42, 28);
                    icon.graphics.moveTo(16, 38);
                    icon.graphics.lineTo(35, 38);
                    CurrencyIcon_mc.addChild(icon);
                }
            }
            var progress:Number = Math.max(0, Math.min(1, (frame - 51) / 8));
            CurrencyBase_mc.CurrencyBase_tf.autoSize = TextFieldAutoSize.LEFT;
            CurrencyChange_mc.CurrencyChange_tf.autoSize = TextFieldAutoSize.LEFT;
            CurrencyChange_mc.Sign_tf.autoSize = TextFieldAutoSize.LEFT;
            CurrencyBase_mc.CurrencyBase_tf.text = String(Math.floor(Number(data.before) +
                (Number(data.after) - Number(data.before)) * progress));
            CurrencyChange_mc.CurrencyChange_tf.text = String(Number(data.after) - Number(data.before));
            CurrencyChange_mc.Sign_tf.text = "+";
            var balanceBounds:Rectangle = CurrencyBase_mc.getBounds(this);
            var iconBounds:Rectangle = CurrencyIcon_mc.getBounds(this);
            // Moving the timeline container can freeze Scaleform's initial zero alpha.
            if (icon != null) icon.y += (balanceBounds.y + balanceBounds.height / 2
                - iconBounds.y - iconBounds.height / 2) / CurrencyIcon_mc.scaleY;
            var balanceInChange:Rectangle = CurrencyBase_mc.getBounds(CurrencyChange_mc);
            CurrencyChange_mc.Sign_tf.x = balanceInChange.right + 12;
            CurrencyChange_mc.Sign_tf.y = balanceInChange.y + balanceInChange.height / 2
                - CurrencyChange_mc.Sign_tf.height / 2;
            CurrencyChange_mc.CurrencyChange_tf.x = CurrencyChange_mc.Sign_tf.x
                + CurrencyChange_mc.Sign_tf.width;
            CurrencyChange_mc.CurrencyChange_tf.y = balanceInChange.y + balanceInChange.height / 2
                - CurrencyChange_mc.CurrencyChange_tf.height / 2;
            awardLabel.text = data.label == null ? "" : String(data.label);
            awardLabel.x = balanceBounds.x + balanceBounds.width / 2 - awardLabel.width / 2;
            awardLabel.y = balanceBounds.y - awardLabel.height - 8;
            awardLabel.alpha = CurrencyBase_mc.alpha;
        }
    }
}
