package
{
    import flash.display.MovieClip;
    import flash.filters.ColorMatrixFilter;

    public class ItemCard_ItemHealthEntry extends ItemCard_Entry
    {
        public var ConditionMeter_mc:MovieClip;

        public function ItemCard_ItemHealthEntry()
        {
            super();
        }

        override public function PopulateEntry(entry:Object):*
        {
            var maximum:Number = Number(entry.maximumHealth);
            ConditionMeter_mc.visible = maximum > 0;
            if (maximum <= 0) return;
            var durability:Number = Math.max(0, Math.min(100, Number(entry.durability)));
            // gotoAndStop treats an integral Number as a frame, and some players treat a fraction as a label.
            ConditionMeter_mc.gotoAndStop(int(ConditionMeter_mc.totalFrames - (ConditionMeter_mc.totalFrames - 1) * durability / 100));
            var meter:MovieClip = ConditionMeter_mc.MeterClip_mc;
            // Gold and pale over-repair fills have nearly identical red channels under HUD tinting.
            var contrast:Number = 255 / 203;
            meter.filters = [new ColorMatrixFilter([
                0, 0, contrast, 0, 0,
                0, 0, contrast, 0, 0,
                0, 0, contrast, 0, 0,
                0, 0, 0, 1, 0])];
            var health:Number = Math.max(0, Math.min(2, Number(entry.currentHealth) / maximum));
            meter.gotoAndStop(health > 0 ? int(meter.totalFrames - (meter.totalFrames - 2) * health / 2) : 1);
        }

        public static function IsEntryValid(entry:Object):Boolean
        {
            return entry.currentHealth != 255;
        }
    }
}
