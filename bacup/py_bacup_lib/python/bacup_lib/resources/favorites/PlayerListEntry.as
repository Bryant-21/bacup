package
{
    import flash.display.MovieClip;
    import flash.geom.ColorTransform;
    import flash.text.TextField;

    // FO76's quick-inventory row art, driven by the Tales wheel instead of BSScrollingList.
    public dynamic class PlayerListEntry extends MovieClip
    {
        public var border:MovieClip;
        public var FavoriteIcon_mc:MovieClip;
        public var textField:TextField;
        public var LeftIcon_mc:MovieClip;
        public var RowIndex:int = -1;
        private var textColor:uint = 0xFFFFCB;

        public function PlayerListEntry()
        {
            super();
            mouseChildren = false;
            if (textField != null)
            {
                textColor = textField.textColor;
                textField.mouseEnabled = false;
            }
            if (LeftIcon_mc != null)
            {
                if (LeftIcon_mc.BarterIcon_mc != null) LeftIcon_mc.BarterIcon_mc.visible = false;
                if (LeftIcon_mc.BestIcon_mc != null) LeftIcon_mc.BestIcon_mc.visible = false;
            }
        }

        public function RowHeight():Number
        {
            return border != null ? border.height : height;
        }

        // FO76 BSScrollingListEntry.SetEntryText: a selected row shows its border with black text.
        public function SetRow(data:Object, selected:Boolean):void
        {
            visible = true;
            var count:uint = uint(data.count);
            textField.text = String(data.text) + (count > 1 ? " (" + count + ")" : "");
            textField.textColor = selected ? 0 : textColor;
            if (border != null) border.alpha = selected ? 1 : 0;
            var tint:ColorTransform = selected ? new ColorTransform(0, 0, 0, 1, 0, 0, 0, 0) : new ColorTransform();
            if (LeftIcon_mc != null)
            {
                LeftIcon_mc.transform.colorTransform = tint;
                if (LeftIcon_mc.EquipIcon_mc != null) LeftIcon_mc.EquipIcon_mc.visible = data.equipped == true;
            }
            if (FavoriteIcon_mc != null)
            {
                FavoriteIcon_mc.visible = int(data.slot) >= 0;
                FavoriteIcon_mc.transform.colorTransform = tint;
                FavoriteIcon_mc.x = textField.x + Math.min(textField.width, textField.textWidth + 12);
            }
        }

        public function SetMessage(text:String):void
        {
            visible = true;
            textField.text = text;
            textField.textColor = textColor;
            if (border != null) border.alpha = 0;
            if (LeftIcon_mc != null && LeftIcon_mc.EquipIcon_mc != null) LeftIcon_mc.EquipIcon_mc.visible = false;
            if (FavoriteIcon_mc != null) FavoriteIcon_mc.visible = false;
        }
    }
}
